use crate::bounds::PixelBounds;
use crate::compositor::begin_pass;
use crate::error::GpuRenderError;
use crate::texture::CanvasTexture;
use celesta_composition::{
    LayerFilter, LayerShader, Scene, ShaderParam, ShaderParamType, ShaderSource,
};
use celesta_renderer::parse_color;
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::ops::Range;

/// The most parameters one custom shader declares.
pub(crate) const MAX_SHADER_PARAMS: usize = 16;

/// The most output pixels a shader writes beyond its content box.
const MAX_PADDING: f32 = 512.0;

/// Compiled shaders no frame has listed for this many frames are dropped,
/// so a shader edited over and over during a preview does not pile up
/// pipelines.
const UNUSED_FRAMES: u64 = 300;

/// Bytes of `Celesta` in `shader_prelude.wgsl`: two `vec2f`s and a `vec4f`.
const BUILTINS_SIZE: u64 = 8 * 4;

/// Bytes of the largest `Params`: every parameter takes one 16-byte slot.
const PARAMS_SIZE: u64 = MAX_SHADER_PARAMS as u64 * 16;

const PRELUDE: &str = include_str!("shader_prelude.wgsl");
const ENTRY: &str = include_str!("shader_entry.wgsl");
const COLOR_FILTER: &str = include_str!("color_filter.wgsl");
const BUILTIN_EFFECT: &str = include_str!("builtin_effect.wgsl");

/// The signature `shader_entry.wgsl` calls, named in errors that arise there.
const EFFECT_SIGNATURE: &str = "fn effect(input: EffectInput) -> vec4f";

/// One layer's filters in the frame being prepared, run in order.
#[derive(Clone, Copy)]
pub(crate) struct ShaderChain {
    /// The first of its uses in the frame's uses; the rest follow it.
    first: usize,
    /// How many filters it runs.
    count: usize,
    /// Output pixels the filters together may write beyond the content
    /// box: the sum of their paddings, since each can write that far past
    /// what the one before it wrote.
    pub(crate) padding: f32,
}

impl ShaderChain {
    /// Its uses' indices in the frame's uses, in the order they run.
    pub(crate) fn uses(self) -> Range<usize> {
        self.first..self.first + self.count
    }
}

struct CachedShader {
    source: ShaderSource,
    /// The compile error is kept too, so a broken shader is not compiled
    /// again every frame while its author fixes it.
    pipeline: Result<wgpu::RenderPipeline, String>,
    /// The last frame whose scene listed the shader.
    last_used: u64,
}

/// The pipeline a use runs.
enum UsePipeline {
    /// The custom shader of this id.
    Custom(String),
    /// `color_filter.wgsl`.
    ColorFilter,
    /// `builtin_effect.wgsl`.
    BuiltinEffect,
}

struct ShaderUse {
    pipeline: UsePipeline,
    /// The values, one parameter per slot as `Params` lays them out.
    params: [[f32; 4]; MAX_SHADER_PARAMS],
    /// Output pixels it may write beyond what it reads.
    padding: f32,
}

/// Compiles the scene's custom shaders and runs the layers' filters: those
/// shaders and the built-in filters.
pub(crate) struct ShaderProcessor {
    layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    shaders: HashMap<String, CachedShader>,
    /// `color_filter.wgsl`, compiled when the first layer uses it.
    color_filter: Option<wgpu::RenderPipeline>,
    /// `builtin_effect.wgsl`, compiled when the first layer uses it.
    builtin_effect: Option<wgpu::RenderPipeline>,
    /// Counts prepared frames; see `UNUSED_FRAMES`.
    frame: u64,
    scene_size: [f32; 2],
    uses: Vec<ShaderUse>,
    /// Every pass's `Celesta` and `Params` of the frame being encoded, at
    /// `builtins_stride` and `params_stride` bytes each, read with dynamic
    /// offsets as `EffectProcessor` reads its passes' parameters.
    uniforms: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    data: Vec<u8>,
    builtins_stride: u32,
    params_stride: u32,
    /// The most uses one frame's uniforms buffer can hold.
    pub(crate) max_uses: usize,
    /// The format of the canvases shaders write, `canvas_format`'s.
    format: wgpu::TextureFormat,
    /// How many times a shader was compiled, for tests of the cache.
    #[cfg(test)]
    pub(crate) compiles: usize,
}

impl ShaderProcessor {
    /// Reads its source through `texture_layout` (`layer.wgsl`'s), so a
    /// canvas's own bind group serves custom shaders too. Writes canvases of
    /// `format`.
    pub(crate) fn new(
        device: &wgpu::Device,
        texture_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
    ) -> Self {
        let uniform = |binding, size| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: wgpu::BufferSize::new(size),
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Celesta custom shader uniforms bind group layout"),
            entries: &[uniform(0, BUILTINS_SIZE), uniform(1, PARAMS_SIZE)],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Celesta custom shader pipeline layout"),
            bind_group_layouts: &[Some(texture_layout), Some(&layout)],
            immediate_size: 0,
        });
        let alignment = device.limits().min_uniform_buffer_offset_alignment;
        let builtins_stride = (BUILTINS_SIZE as u32).next_multiple_of(alignment);
        let params_stride = (PARAMS_SIZE as u32).next_multiple_of(alignment);
        let max_uses =
            (device.limits().max_buffer_size / u64::from(builtins_stride + params_stride)) as usize;
        let (uniforms, bind_group) = Self::uniform_buffer(
            device,
            &layout,
            u64::from(builtins_stride + params_stride) * 4,
        );
        Self {
            layout,
            pipeline_layout,
            shaders: HashMap::new(),
            color_filter: None,
            builtin_effect: None,
            frame: 0,
            scene_size: [0.0; 2],
            uses: Vec::new(),
            uniforms,
            bind_group,
            data: Vec::new(),
            builtins_stride,
            params_stride,
            max_uses,
            format,
            #[cfg(test)]
            compiles: 0,
        }
    }

    fn uniform_buffer(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        size: u64,
    ) -> (wgpu::Buffer, wgpu::BindGroup) {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Celesta custom shader uniforms"),
            size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let binding = |binding, size| wgpu::BindGroupEntry {
            binding,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &buffer,
                offset: 0,
                size: wgpu::BufferSize::new(size),
            }),
        };
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Celesta custom shader uniforms"),
            layout,
            entries: &[binding(0, BUILTINS_SIZE), binding(1, PARAMS_SIZE)],
        });
        (buffer, bind_group)
    }

    /// Starts preparing `scene`: compiles the shaders it lists that are not
    /// compiled yet, and fails when one of them does not compile.
    pub(crate) fn begin_scene(
        &mut self,
        device: &wgpu::Device,
        scene: &Scene,
    ) -> Result<(), GpuRenderError> {
        self.frame += 1;
        self.uses.clear();
        self.scene_size = [scene.width as f32, scene.height as f32];
        let frame = self.frame;
        self.shaders
            .retain(|_, cached| cached.last_used + UNUSED_FRAMES >= frame);
        for (index, source) in scene.shaders.iter().enumerate() {
            if scene.shaders[..index]
                .iter()
                .any(|other| other.id == source.id)
            {
                return Err(shader_error(
                    source,
                    "the scene lists two shaders with this id".to_owned(),
                ));
            }
            let compiled = self
                .shaders
                .get(&source.id)
                .is_some_and(|cached| cached.source == *source);
            if !compiled {
                #[cfg(test)]
                {
                    self.compiles += 1;
                }
                let pipeline = self.compile(device, source);
                self.shaders.insert(
                    source.id.clone(),
                    CachedShader {
                        source: source.clone(),
                        pipeline,
                        last_used: frame,
                    },
                );
            }
            let cached = self.shaders.get_mut(&source.id).expect("inserted above");
            cached.last_used = frame;
            if let Err(message) = &cached.pipeline {
                return Err(shader_error(source, message.clone()));
            }
        }
        Ok(())
    }

    /// Records `layer`'s `filters`, in order, in the frame being prepared;
    /// `None` when it has none.
    pub(crate) fn use_filters(
        &mut self,
        device: &wgpu::Device,
        layer: &str,
        filters: &[LayerFilter],
    ) -> Result<Option<ShaderChain>, GpuRenderError> {
        let first = self.uses.len();
        let mut padding = 0.0;
        for filter in filters {
            padding += match filter {
                LayerFilter::Shader(shader) => self.use_shader(layer, shader)?,
                LayerFilter::Brightness { .. }
                | LayerFilter::Contrast { .. }
                | LayerFilter::Saturation { .. }
                | LayerFilter::Hue { .. }
                | LayerFilter::Grayscale { .. }
                | LayerFilter::Invert { .. }
                | LayerFilter::Tint { .. }
                | LayerFilter::Vignette { .. } => self.use_color_filter(device, layer, filter)?,
                _ => self.use_builtin_effect(device, layer, filter)?,
            };
        }
        Ok((!filters.is_empty()).then_some(ShaderChain {
            first,
            count: filters.len(),
            padding,
        }))
    }

    /// Records `layer`'s use of the built-in color `filter`, which writes
    /// only the pixels it reads.
    fn use_color_filter(
        &mut self,
        device: &wgpu::Device,
        layer: &str,
        filter: &LayerFilter,
    ) -> Result<f32, GpuRenderError> {
        let (kind, name, amounts, color) = match filter {
            LayerFilter::Shader(_)
            | LayerFilter::Pixelate { .. }
            | LayerFilter::Halftone { .. }
            | LayerFilter::ChromaticAberration { .. }
            | LayerFilter::Wave { .. }
            | LayerFilter::Noise { .. }
            | LayerFilter::Scanlines { .. }
            | LayerFilter::ZoomBlur { .. }
            | LayerFilter::ChromaKey { .. } => {
                unreachable!("use_filters runs the other filters itself")
            }
            LayerFilter::Brightness { amount } => (0, "brightness", [*amount, 0.0, 0.0], None),
            LayerFilter::Contrast { amount } => (1, "contrast", [*amount, 0.0, 0.0], None),
            LayerFilter::Saturation { amount } => (2, "saturation", [*amount, 0.0, 0.0], None),
            // In one turn, so a large angle keeps its precision as `f32`.
            LayerFilter::Hue { degrees } => (3, "hue", [degrees.rem_euclid(360.0), 0.0, 0.0], None),
            LayerFilter::Grayscale { amount } => (4, "grayscale", [*amount, 0.0, 0.0], None),
            LayerFilter::Invert { amount } => (5, "invert", [*amount, 0.0, 0.0], None),
            LayerFilter::Tint { color, amount } => (6, "tint", [*amount, 0.0, 0.0], Some(color)),
            LayerFilter::Vignette {
                amount,
                radius,
                softness,
            } => (7, "vignette", [*amount, *radius, *softness], None),
        };
        let error = |message: String| GpuRenderError::BuiltinFilter {
            filter: name,
            message,
        };
        // The values are uploaded as `f32`, which a finite `f64` can overflow.
        if !amounts.iter().all(|value| (*value as f32).is_finite()) {
            return Err(error(format!(
                "layer `{layer}` gives a value that is not finite"
            )));
        }
        let color = match color {
            Some(color) => {
                let color = parse_color(color)
                    .map_err(|message| error(format!("layer `{layer}`: {message}")))?;
                [color.red, color.green, color.blue, color.alpha]
                    .map(|value| f32::from(value) / 255.0)
            }
            None => [0.0; 4],
        };
        if self.uses.len() >= self.max_uses {
            return Err(GpuRenderError::TooManyLayers(self.uses.len() + 1));
        }
        if self.color_filter.is_none() {
            let pipeline = self
                .compile(
                    device,
                    &builtin_source("color filter", COLOR_FILTER, "color"),
                )
                .unwrap_or_else(|message| panic!("color_filter.wgsl does not compile: {message}"));
            self.color_filter = Some(pipeline);
        }
        let mut params = [[0.0; 4]; MAX_SHADER_PARAMS];
        params[0] = [
            kind as f32,
            amounts[0] as f32,
            amounts[1] as f32,
            amounts[2] as f32,
        ];
        params[1] = color;
        self.uses.push(ShaderUse {
            pipeline: UsePipeline::ColorFilter,
            params,
            padding: 0.0,
        });
        Ok(0.0)
    }

    /// Records `layer`'s use of the built-in stylize effect or chroma key
    /// `filter`, returning how far it may write beyond what it reads.
    fn use_builtin_effect(
        &mut self,
        device: &wgpu::Device,
        layer: &str,
        filter: &LayerFilter,
    ) -> Result<f32, GpuRenderError> {
        let diagonal = f64::from(self.scene_size[0].hypot(self.scene_size[1]));
        // `[kind, values…]` as `op` and `more` lay them out, the reach
        // beyond the pixels it reads, and the chroma key's color.
        let (name, values, padding, color): (_, [f64; 7], f64, Option<&String>) = match filter {
            LayerFilter::Pixelate { size } => (
                "pixelate",
                [0.0, *size, 0.0, 0.0, 0.0, 0.0, 0.0],
                *size,
                None,
            ),
            LayerFilter::Halftone { size, angle } => (
                "halftone",
                [1.0, *size, angle.rem_euclid(360.0), 0.0, 0.0, 0.0, 0.0],
                size * std::f64::consts::SQRT_2,
                None,
            ),
            LayerFilter::ChromaticAberration { offset, angle } => (
                "chromaticAberration",
                [2.0, *offset, angle.rem_euclid(360.0), 0.0, 0.0, 0.0, 0.0],
                offset.abs(),
                None,
            ),
            LayerFilter::Wave {
                amplitude,
                wavelength,
                phase,
                angle,
            } => (
                "wave",
                [
                    3.0,
                    *amplitude,
                    *wavelength,
                    phase.rem_euclid(360.0),
                    angle.rem_euclid(360.0),
                    0.0,
                    0.0,
                ],
                amplitude.abs(),
                None,
            ),
            LayerFilter::Noise {
                amount,
                seed,
                monochrome,
            } => (
                "noise",
                [
                    4.0,
                    *amount,
                    // In halves, which `f32` holds exactly.
                    f64::from(seed & 0xffff),
                    f64::from(seed >> 16),
                    if *monochrome { 1.0 } else { 0.0 },
                    0.0,
                    0.0,
                ],
                0.0,
                None,
            ),
            LayerFilter::Scanlines {
                amount,
                spacing,
                thickness,
                offset,
            } => (
                "scanlines",
                [5.0, *amount, *spacing, *thickness, *offset, 0.0, 0.0],
                0.0,
                None,
            ),
            LayerFilter::ZoomBlur {
                amount,
                center_x,
                center_y,
            } => {
                // A pixel `d` from the centre shows what lies `1 - amount`
                // as far, so streaks reach past the content by up to
                // `amount / (1 - amount)` of its distance from the centre,
                // which the scene's diagonal bounds.
                let amount_clamped = amount.clamp(0.0, 1.0);
                let reach = if amount_clamped < 1.0 {
                    amount_clamped / (1.0 - amount_clamped) * diagonal
                } else {
                    f64::from(MAX_PADDING)
                };
                (
                    "zoomBlur",
                    [6.0, *amount, *center_x, *center_y, 0.0, 0.0, 0.0],
                    reach,
                    None,
                )
            }
            LayerFilter::ChromaKey {
                color,
                tolerance,
                softness,
                spill,
            } => (
                "chromaKey",
                [7.0, *tolerance, *softness, *spill, 0.0, 0.0, 0.0],
                0.0,
                Some(color),
            ),
            _ => unreachable!("use_filters runs the other filters itself"),
        };
        let error = |message: String| GpuRenderError::BuiltinFilter {
            filter: name,
            message,
        };
        // The values are uploaded as `f32`, which a finite `f64` can overflow.
        if !values.iter().all(|value| (*value as f32).is_finite()) {
            return Err(error(format!(
                "layer `{layer}` gives a value that is not finite"
            )));
        }
        let mut values = values.map(|value| value as f32);
        if let Some(color) = color {
            let color = parse_color(color)
                .map_err(|message| error(format!("layer `{layer}`: {message}")))?;
            values[4..7].copy_from_slice(
                &[color.red, color.green, color.blue].map(|value| f32::from(value) / 255.0),
            );
        }
        if self.uses.len() >= self.max_uses {
            return Err(GpuRenderError::TooManyLayers(self.uses.len() + 1));
        }
        if self.builtin_effect.is_none() {
            let pipeline = self
                .compile(
                    device,
                    &builtin_source("built-in effect", BUILTIN_EFFECT, "more"),
                )
                .unwrap_or_else(|message| {
                    panic!("builtin_effect.wgsl does not compile: {message}")
                });
            self.builtin_effect = Some(pipeline);
        }
        let mut params = [[0.0; 4]; MAX_SHADER_PARAMS];
        params[0] = [values[0], values[1], values[2], values[3]];
        params[1] = [values[4], values[5], values[6], 0.0];
        let padding = (padding as f32).clamp(0.0, MAX_PADDING);
        self.uses.push(ShaderUse {
            pipeline: UsePipeline::BuiltinEffect,
            params,
            padding,
        });
        Ok(padding)
    }

    /// How many uses the frame has recorded so far.
    pub(crate) fn use_count(&self) -> usize {
        self.uses.len()
    }

    /// Whether a use after the first `first` works in its canvas's pixel
    /// grid: anything but a color filter, which maps each pixel alone.
    pub(crate) fn uses_pixel_grid_since(&self, first: usize) -> bool {
        self.uses[first..]
            .iter()
            .any(|shader_use| !matches!(shader_use.pipeline, UsePipeline::ColorFilter))
    }

    /// Forgets the frame's uses after the first `count`, recorded by layers
    /// that were rolled back.
    pub(crate) fn truncate_uses(&mut self, count: usize) {
        self.uses.truncate(count);
    }

    /// Records `layer`'s use of `shader`, returning its padding.
    fn use_shader(&mut self, layer: &str, shader: &LayerShader) -> Result<f32, GpuRenderError> {
        let error = |message: String| GpuRenderError::Shader {
            shader: shader.id.clone(),
            message,
        };
        let Some(cached) = self
            .shaders
            .get(&shader.id)
            .filter(|cached| cached.last_used == self.frame)
        else {
            return Err(error(format!(
                "layer `{layer}` uses a shader the scene does not list"
            )));
        };
        let source = &cached.source;
        let error = |message: String| shader_error(source, message);
        let expected: usize = source
            .params
            .iter()
            .map(|param| param.ty.components())
            .sum();
        if shader.params.len() != expected {
            return Err(error(format!(
                "layer `{layer}` gives {} parameter values, but the shader's parameters take {expected}",
                shader.params.len()
            )));
        }
        // The values are uploaded as `f32`, which a finite `f64` can overflow.
        if !shader
            .params
            .iter()
            .all(|value| (*value as f32).is_finite())
        {
            return Err(error(format!(
                "layer `{layer}` gives a parameter value that is not finite"
            )));
        }
        if !shader.padding.is_finite() {
            return Err(error(format!(
                "layer `{layer}` gives a padding that is not finite"
            )));
        }
        if self.uses.len() >= self.max_uses {
            return Err(GpuRenderError::TooManyLayers(self.uses.len() + 1));
        }
        let mut params = [[0.0; 4]; MAX_SHADER_PARAMS];
        let mut values = shader.params.iter();
        for (slot, param) in params.iter_mut().zip(&source.params) {
            for component in &mut slot[..param.ty.components()] {
                *component = *values.next().expect("the count was checked") as f32;
            }
        }
        let padding = (shader.padding as f32).clamp(0.0, MAX_PADDING);
        self.uses.push(ShaderUse {
            pipeline: UsePipeline::Custom(shader.id.clone()),
            params,
            padding,
        });
        Ok(padding)
    }

    /// Output pixels the use at `index` may write beyond what it reads.
    pub(crate) fn padding(&self, index: usize) -> f32 {
        self.uses[index].padding
    }

    /// How many compiled shaders (or their errors) are kept.
    #[cfg(test)]
    pub(crate) fn cached(&self) -> usize {
        self.shaders.len()
    }

    /// Starts encoding the prepared frame's shader passes. `use_shader`
    /// keeps their uniforms within the device's largest buffer.
    pub(crate) fn begin_frame(&mut self, device: &wgpu::Device) {
        self.data.clear();
        let size = u64::from(self.builtins_stride + self.params_stride) * self.uses.len() as u64;
        if size > self.uniforms.size() {
            let size = size
                .next_power_of_two()
                .min(device.limits().max_buffer_size);
            // In-flight frames keep the old buffer alive until they finish.
            (self.uniforms, self.bind_group) = Self::uniform_buffer(device, &self.layout, size);
        }
    }

    /// Uploads the frame's shader uniforms; see `EffectProcessor::end_frame`.
    pub(crate) fn end_frame(&mut self, queue: &wgpu::Queue) {
        if !self.data.is_empty() {
            queue.write_buffer(&self.uniforms, 0, &self.data);
        }
    }

    /// Runs the use at `index` on `source`, a canvas whose top-left texel
    /// covers scene pixel `origin`, into `target`, writing only `region` (in
    /// canvas pixels). `content` is the content box, in scene pixels.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn pass(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        source: &CanvasTexture,
        target: &CanvasTexture,
        origin: [u32; 2],
        content: PixelBounds,
        region: PixelBounds,
        index: usize,
    ) {
        let shader = &self.uses[index];
        let pipeline = match &shader.pipeline {
            UsePipeline::Custom(id) => self.shaders[id]
                .pipeline
                .as_ref()
                .expect("begin_scene rejects scenes whose shaders do not compile"),
            UsePipeline::ColorFilter => self
                .color_filter
                .as_ref()
                .expect("use_color_filter compiles it"),
            UsePipeline::BuiltinEffect => self
                .builtin_effect
                .as_ref()
                .expect("use_builtin_effect compiles it"),
        };
        let [left, top, right, bottom] = content.0;
        let builtins = [
            self.scene_size[0],
            self.scene_size[1],
            origin[0] as f32,
            origin[1] as f32,
            left,
            top,
            right,
            bottom,
        ];
        let at = self.data.len();
        let params_at = at + self.builtins_stride as usize;
        assert!(
            (params_at + self.params_stride as usize) as u64 <= self.uniforms.size(),
            "begin_frame reserves every pass's uniforms"
        );
        self.data
            .extend(builtins.into_iter().flat_map(f32::to_ne_bytes));
        self.data.resize(params_at, 0);
        self.data.extend(
            shader
                .params
                .iter()
                .flatten()
                .copied()
                .flat_map(f32::to_ne_bytes),
        );
        self.data.resize(params_at + self.params_stride as usize, 0);
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
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &source.bind_group, &[]);
        pass.set_bind_group(1, &self.bind_group, &[at as u32, params_at as u32]);
        pass.draw(0..3, 0..1);
    }

    /// Assembles, validates, and compiles `source`, or says why it cannot.
    fn compile(
        &self,
        device: &wgpu::Device,
        source: &ShaderSource,
    ) -> Result<wgpu::RenderPipeline, String> {
        let (code, author) = assemble(source)?;
        let module = naga::front::wgsl::parse_str(&code).map_err(|error| {
            let at = error
                .labels()
                .next()
                .and_then(|(span, _)| span.to_range())
                .map(|range| range.start);
            locate(&code, &author, at, error.message().to_owned())
        })?;
        check_declarations(&module, &code, &author)?;
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::default(),
        )
        .validate(&module)
        .map_err(|error| {
            let at = error
                .spans()
                .next()
                .and_then(|(span, _)| span.to_range())
                .map(|range| range.start);
            locate(&code, &author, at, error_chain(error.as_inner()))
        })?;
        let label = source.name.as_deref().unwrap_or(&source.id);
        // naga accepted the module, so wgpu should too; the scope keeps a
        // failure it still reports from reaching the device's error handler.
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(label),
            source: wgpu::ShaderSource::Naga(Cow::Owned(module)),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&self.pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("celesta_vertex"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("celesta_fragment"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: self.format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        match pollster::block_on(scope.pop()) {
            Some(error) => Err(error.to_string()),
            None => Ok(pipeline),
        }
    }
}

/// A built-in filter's `wgsl` as a shader source with two `vec4f`
/// parameters: `op` and `second`.
fn builtin_source(name: &str, wgsl: &str, second: &str) -> ShaderSource {
    let param = |name: &str| ShaderParam {
        name: name.to_owned(),
        ty: ShaderParamType::Vec4,
    };
    ShaderSource {
        id: name.to_owned(),
        name: Some(name.to_owned()),
        wgsl: wgsl.to_owned(),
        params: vec![param("op"), param(second)],
    }
}

fn shader_error(source: &ShaderSource, message: String) -> GpuRenderError {
    GpuRenderError::Shader {
        shader: source.name.clone().unwrap_or_else(|| source.id.clone()),
        message,
    }
}

const fn wgsl_type(ty: ShaderParamType) -> &'static str {
    match ty {
        ShaderParamType::F32 => "f32",
        ShaderParamType::Vec2 => "vec2f",
        ShaderParamType::Vec3 => "vec3f",
        ShaderParamType::Vec4 => "vec4f",
    }
}

/// The prelude, the parameters, the author's code, and the entry points as
/// one module, and where the author's code lies in it.
fn assemble(source: &ShaderSource) -> Result<(String, Range<usize>), String> {
    if source.params.len() > MAX_SHADER_PARAMS {
        return Err(format!(
            "declares {} parameters; the most is {MAX_SHADER_PARAMS}",
            source.params.len()
        ));
    }
    for (index, param) in source.params.iter().enumerate() {
        let name = &param.name;
        let mut chars = name.chars();
        let identifier = chars
            .next()
            .is_some_and(|first| first.is_ascii_alphabetic())
            && chars.all(|char| char.is_ascii_alphanumeric() || char == '_');
        if !identifier || name.starts_with("celesta") {
            return Err(format!(
                "parameter name `{name}` must be letters, digits, and underscores, start with a letter, and not start with `celesta`"
            ));
        }
        if source.params[..index]
            .iter()
            .any(|other| other.name == *name)
        {
            return Err(format!("declares parameter `{name}` twice"));
        }
    }
    let mut code = String::from(PRELUDE);
    if !source.params.is_empty() {
        code.push_str("\nstruct Params {\n");
        for param in &source.params {
            writeln!(
                code,
                "    @align(16) {}: {},",
                param.name,
                wgsl_type(param.ty)
            )
            .expect("writing to a String cannot fail");
        }
        code.push_str("};\n\n@group(1) @binding(1)\nvar<uniform> params: Params;\n");
    }
    code.push('\n');
    let start = code.len();
    code.push_str(&source.wgsl);
    let author = start..code.len();
    code.push('\n');
    code.push_str(ENTRY);
    Ok((code, author))
}

/// Rejects entry points and resources the author declared: the prelude
/// binds everything a custom shader gets.
fn check_declarations(
    module: &naga::Module,
    code: &str,
    author: &Range<usize>,
) -> Result<(), String> {
    let start = |span: naga::Span| span.to_range().map(|range| range.start);
    let fail = |at: Option<usize>, message: String| Err(locate(code, author, at, message));
    for entry in &module.entry_points {
        if !matches!(entry.name.as_str(), "celesta_vertex" | "celesta_fragment") {
            // naga keeps no span for an entry point; its `fn` is in the
            // author's code.
            let at = code[author.clone()]
                .find(&format!("fn {}", entry.name))
                .map(|offset| author.start + offset);
            return fail(
                at,
                format!(
                    "declares the entry point `{}`; a custom shader defines `effect` instead",
                    entry.name
                ),
            );
        }
    }
    for (handle, global) in module.global_variables.iter() {
        let name = global.name.as_deref().unwrap_or("");
        let at = start(module.global_variables.get_span(handle));
        // Told apart by where they are declared, not by name: a shader
        // without parameters could otherwise bind its own `params`.
        let authored = at.is_none_or(|at| author.contains(&at));
        let prelude = matches!(name, "celesta_source" | "celesta" | "params");
        if global.binding.is_some() && (authored || !prelude) {
            return fail(
                at,
                format!(
                    "declares the resource `{name}`; a custom shader cannot bind resources of its own"
                ),
            );
        }
    }
    // Names starting with `celesta` belong to the prelude, whose own
    // declarations lie outside the author's code.
    let declared = module
        .functions
        .iter()
        .map(|(handle, function)| (function.name.as_deref(), module.functions.get_span(handle)))
        .chain(module.global_variables.iter().map(|(handle, global)| {
            (
                global.name.as_deref(),
                module.global_variables.get_span(handle),
            )
        }))
        .chain(module.constants.iter().map(|(handle, constant)| {
            (constant.name.as_deref(), module.constants.get_span(handle))
        }))
        .chain(
            module
                .types
                .iter()
                .map(|(handle, ty)| (ty.name.as_deref(), module.types.get_span(handle))),
        );
    for (name, span) in declared {
        let at = start(span);
        if let Some(name) = name
            && name.starts_with("celesta")
            && at.is_some_and(|at| author.contains(&at))
        {
            return fail(
                at,
                format!("declares `{name}`; names starting with `celesta` belong to Celesta"),
            );
        }
    }
    Ok(())
}

/// `message`, located at byte `at` of `code`: as a line and column of the
/// author's code when it lies there, or with the signature the entry points
/// expect when it lies in them.
fn locate(code: &str, author: &Range<usize>, at: Option<usize>, message: String) -> String {
    match at {
        Some(at) if author.contains(&at) || (at == author.end && !author.is_empty()) => {
            let before = &code[author.start..at];
            let line = before.matches('\n').count() + 1;
            let column = before.len() - before.rfind('\n').map_or(0, |newline| newline + 1) + 1;
            format!("{line}:{column}: {message}")
        }
        Some(at) if at > author.end => {
            format!("{message} (a custom shader defines `{EFFECT_SIGNATURE}`)")
        }
        _ => message,
    }
}

/// `error` and the errors that caused it, outermost first.
fn error_chain(error: &dyn std::error::Error) -> String {
    let mut message = error.to_string();
    let mut cause = error.source();
    while let Some(error) = cause {
        write!(message, ": {error}").expect("writing to a String cannot fail");
        cause = error.source();
    }
    message
}
