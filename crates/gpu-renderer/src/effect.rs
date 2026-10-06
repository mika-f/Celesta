use crate::bounds::{PixelBounds, blur_reach};
use crate::compositor::begin_pass;
use crate::error::GpuRenderError;
use crate::texture::{CanvasTexture, canvas_texture};
use celesta_composition::LayerEffects;
use celesta_renderer::Color as CpuColor;

pub(crate) struct EffectProcessor {
    pub(crate) params_layout: wgpu::BindGroupLayout,
    /// The bilinear sampler `effect.wgsl` pairs taps through, bound beside
    /// the parameters.
    pub(crate) sampler: wgpu::Sampler,
    /// The blur pass, then `effect.wgsl`'s `downsample` and `upsample`.
    pub(crate) pipelines: [wgpu::RenderPipeline; 3],
    /// Canvases groups draw onto and effects filter through, kept across
    /// effects and frames instead of allocating several per effect every
    /// frame, each with the frame it was last taken in. A canvas goes back
    /// to the pool once the commands that read it are encoded; the queue
    /// runs them in order, so reusing it later is safe.
    pub(crate) pool: Vec<(CanvasTexture, u64)>,
    /// Counts composited frames; see `end_frame`.
    pub(crate) frame: u64,
    /// Every filter pass's `Params` of the frame being encoded, one per
    /// `params_stride` bytes, written to `params` once the frame is encoded
    /// and read with a dynamic offset: a dense frame has hundreds of passes,
    /// and a buffer and a bind group each would dominate their cost.
    pub(crate) params: wgpu::Buffer,
    pub(crate) params_bind_group: wgpu::BindGroup,
    pub(crate) params_data: Vec<u8>,
    pub(crate) params_stride: u32,
}

/// Bytes of `Params` in `effect.wgsl`: three `vec4<f32>`s.
pub(crate) const EFFECT_PARAMS_SIZE: u64 = 3 * 4 * 4;

/// The most filter passes one blur, shadow, or glow takes: shrinking, the
/// two blur passes, and enlarging.
pub(crate) const PASSES_PER_FILTER: usize = 4;

/// The sigma, in texels of the reduced copy, a blur needs at least before it
/// runs at reduced resolution. The copy's box filter and the bilinear
/// enlargement then add little next to the Gaussian, so the result stays
/// within a few 8-bit steps of the exact blur; see `reduced_blur`.
const MIN_REDUCED_SIGMA: f32 = 4.0;

/// The most a blur's resolution is reduced by, per axis.
const MAX_REDUCTION: u32 = 8;

/// Runs a blur of `sigma` output pixels at `1 / factor` resolution, with
/// the sigma it takes there, when `factor` is above 1: the largest power of
/// two up to `MAX_REDUCTION` that leaves at least `MIN_REDUCED_SIGMA`.
///
/// Reading a `factor`-wide box, blurring, and enlarging bilinearly (a tent
/// `factor` pixels to each side) adds the box's variance `(factor² - 1) / 12`
/// and the tent's `factor² / 6` to the Gaussian's. The reduced sigma is
/// smaller by exactly that, so the overall spread matches `sigma`. Every
/// pass reads `factor²` fewer pixels, and the blur passes `factor` times
/// fewer taps each.
pub(crate) fn reduced_blur(sigma: f32) -> (u32, f32) {
    let mut factor = 1;
    while factor < MAX_REDUCTION && sigma / (factor * 2) as f32 >= MIN_REDUCED_SIGMA {
        factor *= 2;
    }
    if factor == 1 {
        return (1, sigma);
    }
    let factor_squared = (factor * factor) as f32;
    let variance = sigma * sigma - (factor_squared - 1.0) / 12.0 - factor_squared / 6.0;
    (factor, (variance / factor_squared).sqrt())
}

/// What one filter pass of `effect.wgsl` does.
#[derive(Clone, Copy)]
pub(crate) enum Filter {
    /// One direction of the Gaussian; with a color, shifted by the offset
    /// and tinted.
    Blur {
        direction: [f32; 2],
        radius: f32,
        offset: [f32; 2],
        color: Option<[f32; 4]>,
    },
    /// Shrinks by `factor`.
    Downsample { factor: u32 },
    /// Enlarges by `factor`; with a color, shifted by the offset and tinted.
    Upsample {
        factor: u32,
        offset: [f32; 2],
        color: Option<[f32; 4]>,
    },
}

impl EffectProcessor {
    /// Reads its source through `texture_layout` (`layer.wgsl`'s), so a
    /// canvas's own bind group serves both shaders.
    pub(crate) fn new(device: &wgpu::Device, texture_layout: &wgpu::BindGroupLayout) -> Self {
        let params_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Celesta effect parameters bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(EFFECT_PARAMS_SIZE),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Celesta effect sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Celesta effect pipeline layout"),
            bind_group_layouts: &[Some(texture_layout), Some(&params_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("effect.wgsl"));
        let pipelines = ["fragment", "downsample", "upsample"].map(|entry_point| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Celesta Gaussian effect pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vertex"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry_point),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        });
        let params_stride = device
            .limits()
            .min_uniform_buffer_offset_alignment
            .max(EFFECT_PARAMS_SIZE as u32);
        let (params, params_bind_group) = Self::params_buffer(
            device,
            &params_layout,
            &sampler,
            u64::from(params_stride) * 64,
        );
        Self {
            params_layout,
            sampler,
            pipelines,
            pool: Vec::new(),
            frame: 0,
            params,
            params_bind_group,
            params_data: Vec::new(),
            params_stride,
        }
    }

    pub(crate) fn params_buffer(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        size: u64,
    ) -> (wgpu::Buffer, wgpu::BindGroup) {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Celesta effect parameters"),
            size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Celesta effect parameters"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &buffer,
                        offset: 0,
                        size: wgpu::BufferSize::new(EFFECT_PARAMS_SIZE),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });
        (buffer, bind_group)
    }

    /// Starts a frame that runs at most `passes` filter passes.
    pub(crate) fn begin_frame(&mut self, device: &wgpu::Device, passes: usize) {
        self.frame += 1;
        self.params_data.clear();
        let size = u64::from(self.params_stride) * passes as u64;
        if size > self.params.size() {
            // In-flight frames keep the old buffer alive until they finish.
            (self.params, self.params_bind_group) = Self::params_buffer(
                device,
                &self.params_layout,
                &self.sampler,
                size.next_power_of_two(),
            );
        }
    }

    /// Uploads the frame's filter parameters, and drops the canvases neither
    /// this frame nor the one before took, so sizes a scene stopped using do
    /// not pile up. Queued writes land after previously submitted work, so
    /// frames still in flight keep reading their own parameters.
    pub(crate) fn end_frame(&mut self, queue: &wgpu::Queue) {
        if !self.params_data.is_empty() {
            queue.write_buffer(&self.params, 0, &self.params_data);
        }
        let frame = self.frame;
        self.pool.retain(|(_, used)| used + 1 >= frame);
    }

    /// A transparent-or-stale `width`x`height` canvas from the pool, or a new
    /// one. Every user clears it before drawing.
    pub(crate) fn take_canvas(
        &mut self,
        device: &wgpu::Device,
        layer_layout: &wgpu::BindGroupLayout,
        width: u32,
        height: u32,
    ) -> CanvasTexture {
        let fits = |(canvas, _): &(CanvasTexture, u64)| {
            let size = canvas.texture.size();
            size.width == width && size.height == height
        };
        match self.pool.iter().rposition(fits) {
            Some(index) => self.pool.swap_remove(index).0,
            None => canvas_texture(device, layer_layout, width, height),
        }
    }

    pub(crate) fn recycle(&mut self, canvas: CanvasTexture) {
        self.pool.push((canvas, self.frame));
    }

    /// Blurs (and, with `color`, shifts and tints) `source`, whose layers
    /// cover `content`. Each pass only shades the pixels its result can be
    /// non-transparent at, so a small glow costs a small area, not the canvas.
    /// A large blur runs at reduced resolution; see `reduced_blur`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        source: &CanvasTexture,
        layer_layout: &wgpu::BindGroupLayout,
        content: PixelBounds,
        radius: f32,
        offset: [f32; 2],
        color: Option<[f32; 4]>,
    ) -> CanvasTexture {
        let size = source.texture.size();
        let reach = blur_reach(radius);
        let (factor, reduced) = reduced_blur(radius.clamp(0.0, 64.0));
        if factor > 1 {
            let scale = 1.0 / factor as f32;
            let [left, top, right, bottom] = content.0;
            // Every reduced texel any of the content's pixels reaches.
            let small = PixelBounds([
                (left * scale).floor(),
                (top * scale).floor(),
                (right * scale).ceil(),
                (bottom * scale).ceil(),
            ]);
            let small_reach = blur_reach(reduced);
            let (width, height) = (size.width.div_ceil(factor), size.height.div_ceil(factor));
            let shrunk = self.take_canvas(device, layer_layout, width, height);
            let horizontal = self.take_canvas(device, layer_layout, width, height);
            let vertical = self.take_canvas(device, layer_layout, width, height);
            let result = self.take_canvas(device, layer_layout, size.width, size.height);
            self.pass(
                encoder,
                source,
                &shrunk,
                small,
                Filter::Downsample { factor },
            );
            self.pass(
                encoder,
                &shrunk,
                &horizontal,
                small.expand(small_reach, 0.0),
                Filter::Blur {
                    direction: [1.0, 0.0],
                    radius: reduced,
                    offset: [0.0, 0.0],
                    color: None,
                },
            );
            self.pass(
                encoder,
                &horizontal,
                &vertical,
                small.expand(small_reach, small_reach),
                Filter::Blur {
                    direction: [0.0, 1.0],
                    radius: reduced,
                    offset: [0.0, 0.0],
                    color: None,
                },
            );
            self.pass(
                encoder,
                &vertical,
                &result,
                content.expand(reach, reach).offset(offset),
                Filter::Upsample {
                    factor,
                    offset,
                    color,
                },
            );
            for canvas in [shrunk, horizontal, vertical] {
                self.recycle(canvas);
            }
            return result;
        }
        let horizontal =
            (radius > 0.0).then(|| self.take_canvas(device, layer_layout, size.width, size.height));
        let vertical = self.take_canvas(device, layer_layout, size.width, size.height);
        if let Some(horizontal) = &horizontal {
            self.pass(
                encoder,
                source,
                horizontal,
                content.expand(reach, 0.0),
                Filter::Blur {
                    direction: [1.0, 0.0],
                    radius,
                    offset: [0.0, 0.0],
                    color: None,
                },
            );
        }
        self.pass(
            encoder,
            horizontal.as_ref().unwrap_or(source),
            &vertical,
            content.expand(reach, reach).offset(offset),
            Filter::Blur {
                direction: [0.0, 1.0],
                radius,
                offset,
                color,
            },
        );
        if let Some(horizontal) = horizontal {
            self.recycle(horizontal);
        }
        vertical
    }

    pub(crate) fn pass(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        source: &CanvasTexture,
        target: &CanvasTexture,
        region: PixelBounds,
        filter: Filter,
    ) {
        let (pipeline, direction, radius, offset, factor, color) = match filter {
            Filter::Blur {
                direction,
                radius,
                offset,
                color,
            } => (0, direction, radius, offset, 1, color),
            Filter::Downsample { factor } => (1, [0.0; 2], 0.0, [0.0; 2], factor, None),
            Filter::Upsample {
                factor,
                offset,
                color,
            } => (
                2,
                // The canvas's size, which a shadow reads transparency past.
                [
                    target.texture.width() as f32,
                    target.texture.height() as f32,
                ],
                0.0,
                offset,
                factor,
                color,
            ),
        };
        let tint = color.unwrap_or([0.0; 4]);
        let params = [
            direction[0],
            direction[1],
            radius,
            f32::from(color.is_some()),
            offset[0],
            offset[1],
            factor as f32,
            0.0,
            tint[0],
            tint[1],
            tint[2],
            tint[3],
        ];
        let at = self.params_data.len();
        assert!(
            (at + self.params_stride as usize) as u64 <= self.params.size(),
            "begin_frame reserves every pass's parameters"
        );
        self.params_data
            .extend(params.into_iter().flat_map(f32::to_ne_bytes));
        self.params_data.resize(at + self.params_stride as usize, 0);
        let mut pass = begin_pass(
            encoder,
            &target.view,
            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
        );
        // Outside `region` the result is transparent, which the clear wrote.
        let Some([x, y, width, height]) = region.scissor(target.texture.size()) else {
            return;
        };
        pass.set_scissor_rect(x, y, width, height);
        pass.set_pipeline(&self.pipelines[pipeline]);
        pass.set_bind_group(0, &source.bind_group, &[]);
        pass.set_bind_group(1, &self.params_bind_group, &[at as u32]);
        pass.draw(0..3, 0..1);
    }
}

#[derive(Clone, Copy)]
pub(crate) struct EffectShadow {
    pub(crate) color: [f32; 4],
    pub(crate) blur: f32,
    pub(crate) offset: [f32; 2],
}

#[derive(Clone, Copy)]
pub(crate) struct EffectSpec {
    pub(crate) blur: f32,
    pub(crate) shadow: Option<EffectShadow>,
    pub(crate) glow: Option<EffectShadow>,
}

impl EffectSpec {
    /// The canvas pixels the filtered result of layers covering `content`
    /// can touch: the content itself, its blur, and each shifted shadow.
    pub(crate) fn output_bounds(&self, content: PixelBounds) -> PixelBounds {
        let mut bounds = content.expand(blur_reach(self.blur), blur_reach(self.blur));
        for shadow in [self.shadow, self.glow].into_iter().flatten() {
            let reach = blur_reach(shadow.blur);
            bounds = PixelBounds::union(
                Some(bounds),
                Some(content.expand(reach, reach).offset(shadow.offset)),
            )
            .expect("both are some");
        }
        bounds
    }

    pub(crate) fn parse(effects: &LayerEffects) -> Result<Self, GpuRenderError> {
        let parse_color = |color: &str| -> Result<[f32; 4], GpuRenderError> {
            let color = CpuColor::from_hex(color).map_err(GpuRenderError::Effects)?;
            Ok([color.red, color.green, color.blue, color.alpha]
                .map(|value| f32::from(value) / 255.0))
        };
        Ok(Self {
            blur: effects.blur.clamp(0.0, 64.0) as f32,
            shadow: effects
                .shadow
                .as_ref()
                .map(|shadow| -> Result<_, GpuRenderError> {
                    Ok(EffectShadow {
                        color: parse_color(&shadow.color)?,
                        blur: shadow.blur.clamp(0.0, 64.0) as f32,
                        offset: [shadow.offset_x as f32, shadow.offset_y as f32],
                    })
                })
                .transpose()?,
            glow: effects
                .glow
                .as_ref()
                .map(|glow| -> Result<_, GpuRenderError> {
                    Ok(EffectShadow {
                        color: parse_color(&glow.color)?,
                        blur: glow.blur.clamp(0.0, 64.0) as f32,
                        offset: [0.0, 0.0],
                    })
                })
                .transpose()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::reduced_blur;

    #[test]
    fn reduces_only_blurs_that_stay_wide_enough() {
        assert_eq!(reduced_blur(0.0), (1, 0.0));
        assert_eq!(reduced_blur(7.9), (1, 7.9));
        for (sigma, factor) in [(8.0, 2), (16.0, 4), (31.9, 4), (32.0, 8), (64.0, 8)] {
            let (reduced_factor, reduced) = reduced_blur(sigma);
            assert_eq!(reduced_factor, factor, "sigma {sigma}");
            // The box and the tent add `factor² / 4 - 1 / 12` of variance.
            let f = factor as f32;
            let total = reduced * reduced * f * f + f * f / 4.0 - 1.0 / 12.0;
            assert!((total - sigma * sigma).abs() < 1e-2, "sigma {sigma}");
            assert!(reduced >= 3.9, "sigma {sigma}");
        }
    }
}
