use crate::bounds::PixelBounds;
use crate::compositor::begin_pass;
use crate::error::GpuRenderError;
use crate::texture::CanvasTexture;
use celesta_composition::{LayerShader, Scene, ShaderParamType, ShaderSource};
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

/// The signature `shader_entry.wgsl` calls, named in errors that arise there.
const EFFECT_SIGNATURE: &str = "fn effect(input: EffectInput) -> vec4f";

/// One layer's use of a custom shader in the frame being prepared.
#[derive(Clone, Copy)]
pub(crate) struct ShaderSpec {
    /// Its index in the frame's uses.
    pub(crate) index: usize,
    /// Output pixels it may write beyond the content box.
    pub(crate) padding: f32,
}

struct CachedShader {
    source: ShaderSource,
    /// The compile error is kept too, so a broken shader is not compiled
    /// again every frame while its author fixes it.
    pipeline: Result<wgpu::RenderPipeline, String>,
    /// The last frame whose scene listed the shader.
    last_used: u64,
}

struct ShaderUse {
    id: String,
    /// The values, one parameter per slot as `Params` lays them out.
    params: [[f32; 4]; MAX_SHADER_PARAMS],
}

/// Compiles the scene's custom shaders and runs the layers' uses of them.
pub(crate) struct ShaderProcessor {
    layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    shaders: HashMap<String, CachedShader>,
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
    /// How many times a shader was compiled, for tests of the cache.
    #[cfg(test)]
    pub(crate) compiles: usize,
}

impl ShaderProcessor {
    /// Reads its source through `texture_layout` (`layer.wgsl`'s), so a
    /// canvas's own bind group serves custom shaders too.
    pub(crate) fn new(device: &wgpu::Device, texture_layout: &wgpu::BindGroupLayout) -> Self {
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
            frame: 0,
            scene_size: [0.0; 2],
            uses: Vec::new(),
            uniforms,
            bind_group,
            data: Vec::new(),
            builtins_stride,
            params_stride,
            max_uses,
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

    /// Records `layer`'s use of `shader` in the frame being prepared.
    pub(crate) fn use_shader(
        &mut self,
        layer: &str,
        shader: &LayerShader,
    ) -> Result<ShaderSpec, GpuRenderError> {
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
        self.uses.push(ShaderUse {
            id: shader.id.clone(),
            params,
        });
        Ok(ShaderSpec {
            index: self.uses.len() - 1,
            padding: (shader.padding as f32).clamp(0.0, MAX_PADDING),
        })
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

    /// Runs `spec` on `source`, a canvas whose top-left texel covers scene
    /// pixel `origin`, into `target`, writing only `region` (in canvas
    /// pixels). `content` is the content box, in scene pixels.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn pass(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        source: &CanvasTexture,
        target: &CanvasTexture,
        origin: [u32; 2],
        content: PixelBounds,
        region: PixelBounds,
        spec: ShaderSpec,
    ) {
        let shader = &self.uses[spec.index];
        let pipeline = self.shaders[&shader.id]
            .pipeline
            .as_ref()
            .expect("begin_scene rejects scenes whose shaders do not compile");
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
                    format: wgpu::TextureFormat::Rgba8Unorm,
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
        if global.binding.is_some() && !matches!(name, "celesta_source" | "celesta" | "params") {
            return fail(
                start(module.global_variables.get_span(handle)),
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
