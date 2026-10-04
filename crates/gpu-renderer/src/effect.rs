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
    pub(crate) pipeline: wgpu::RenderPipeline,
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
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
                entry_point: Some("fragment"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
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
            pipeline,
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
        let horizontal =
            (radius > 0.0).then(|| self.take_canvas(device, layer_layout, size.width, size.height));
        let vertical = self.take_canvas(device, layer_layout, size.width, size.height);
        if let Some(horizontal) = &horizontal {
            self.pass(
                encoder,
                source,
                horizontal,
                content.expand(reach, 0.0),
                [1.0, 0.0],
                radius,
                [0.0, 0.0],
                None,
            );
        }
        self.pass(
            encoder,
            horizontal.as_ref().unwrap_or(source),
            &vertical,
            content.expand(reach, reach).offset(offset),
            [0.0, 1.0],
            radius,
            offset,
            color,
        );
        if let Some(horizontal) = horizontal {
            self.recycle(horizontal);
        }
        vertical
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn pass(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        source: &CanvasTexture,
        target: &CanvasTexture,
        region: PixelBounds,
        direction: [f32; 2],
        radius: f32,
        offset: [f32; 2],
        color: Option<[f32; 4]>,
    ) {
        let tint = color.unwrap_or([0.0; 4]);
        let params = [
            direction[0],
            direction[1],
            radius,
            f32::from(color.is_some()),
            offset[0],
            offset[1],
            0.0,
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
        pass.set_pipeline(&self.pipeline);
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
