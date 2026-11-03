//! `LayerContent::Plugin` scene content: a provider's 3D scene, uploaded
//! once when it is opened and checked against each of its frames. A scene
//! draws onto the same kind of canvas as 2D meshes (see `mesh.rs`).
//!
//! Each frame, a compute pass morphs and skins every visible draw's
//! vertices (`scene_skin.wgsl`). A render pass then draws them, in the
//! order of Unity's render queues (`draw_queue`) and by their material's
//! shader and alpha mode, through the camera onto a canvas of linear,
//! premultiplied color with `Depth32Float` depth, both cleared (to
//! transparent black and to 1): 4× multisampled `Rgba16Float` where the GPU
//! can draw it (`scene_target` says what else). Every material's shader
//! is written against Celesta's material interface and assembled with its
//! entry points by `scene_material.rs`: unlit (`scene_unlit.wgsl`), or one
//! of the shaders the model lists, compiled once whichever models list it.
//! A draw binds the frame's uniform as group 0; its own uniform and its
//! material's parameters as group 1, each at its offset in the draw's
//! slot; and its material's textures, bound when the model loaded, as
//! group 2. The resolved canvas is converted to the layer's encoding,
//! premultiplied sRGB-encoded `Rgba8Unorm` (`scene.wgsl`).
//!
//! Besides the model's buffers and textures, `SceneFrameResources` keeps
//! the canvases of each size the last frame drew, within
//! `SCENE_TARGET_TEXELS`, and the buffers a frame's skinning and draws read,
//! so that a frame allocates only what it outgrows; a frame without a scene
//! lets them go. Nothing a frame draws depends on an earlier one: the canvases are
//! cleared, and every part of a buffer the frame reads is written by it.

use crate::error::GpuRenderError;
use crate::mesh::{OUTPUT_FORMAT, texture_image};
use crate::renderer::GpuRenderer;
use crate::scene_material::{
    MAX_MATERIAL_PARAMS, MAX_MATERIAL_TEXTURES, MAX_SHADER_BYTES, check_material,
};
use crate::texture::LayerTexture;
use celesta_plugin::scene::{MAX_SCENE_CANVAS_SIDE, MAX_SCENE_SHADERS};
use celesta_plugin::{
    AlphaMode, Camera, ColorSpace, CullFace, DrawState, EmptyTexel, Filter, Material, MaterialKind,
    MaterialOverrides, ModelId, Projection, SceneFrame, SceneModel, SceneShader, TextureRef,
    UnlitMaterial, Wrap,
};
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use wgpu::util::DeviceExt;

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Vertices per compute workgroup, as `scene_skin.wgsl` declares.
const WORKGROUP: u32 = 64;
/// The bytes of one skinned vertex: `scene_skin.wgsl`'s `Skinned`.
const SKINNED_STRIDE: u64 = 48;
/// The bytes of one `SkinVertex`, the largest element a stream holds.
const SKIN_VERTEX_STRIDE: u64 = 32;
/// The most texels a scene's canvas has. A texel of it costs at most about
/// 60 bytes (four samples of color and of depth, the resolved color, the
/// layer's), so this is about 500 MB; a 4K frame (3840 × 2160) still fits
/// at 1×, and a larger canvas is drawn at a lower density.
pub(crate) const SCENE_CANVAS_TEXELS: u64 = 8_388_608;

/// The color format and sample count a scene is drawn with, before it is
/// converted to the layer's encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SceneTarget {
    pub(crate) format: wgpu::TextureFormat,
    /// Samples per pixel of the color and depth canvases: 4, or 1.
    pub(crate) samples: u32,
}

/// What a GPU whose formats have `features` draws scenes on: the first of
/// `Rgba16Float` and `Rgba8Unorm` it can render to, blend, and sample, 4×
/// multisampled when it can multisample and resolve that format and
/// multisample `Depth32Float` 4×, else at 1 sample. 4 is the only count
/// above 1 that every format WebGPU lets be multisampled supports. `None`
/// when neither format can be drawn on.
pub(crate) fn scene_target(
    features: impl Fn(wgpu::TextureFormat) -> wgpu::TextureFormatFeatures,
) -> Option<SceneTarget> {
    use wgpu::{TextureFormatFeatureFlags as Flags, TextureUsages as Usages};
    let format = [
        wgpu::TextureFormat::Rgba16Float,
        wgpu::TextureFormat::Rgba8Unorm,
    ]
    .into_iter()
    .find(|format| {
        let features = features(*format);
        features
            .allowed_usages
            .contains(Usages::RENDER_ATTACHMENT | Usages::TEXTURE_BINDING)
            && features.flags.contains(Flags::BLENDABLE)
    })?;
    let multisampled = features(format)
        .flags
        .contains(Flags::MULTISAMPLE_X4 | Flags::MULTISAMPLE_RESOLVE)
        && features(DEPTH_FORMAT).flags.contains(Flags::MULTISAMPLE_X4);
    Some(SceneTarget {
        format,
        samples: if multisampled { 4 } else { 1 },
    })
}
/// The most texels of scene canvases kept from frame to frame: two capped
/// canvases, about 1 GB. A frame that draws more still gets every canvas it
/// draws, but the canvases it does not draw are dropped first.
pub(crate) const SCENE_TARGET_TEXELS: u64 = 2 * SCENE_CANVAS_TEXELS;
/// The most bytes of buffers a scene model has the GPU allocate when it is
/// opened: its geometries' streams, morph targets, and triangles, and each
/// draw's skinned vertices.
pub(crate) const SCENE_BUFFER_BUDGET: u64 = 1 << 30;

/// The size in texels of a scene canvas `canvas` layer units large drawn
/// at `density` texels a unit: lowered, the aspect kept, until no side is
/// longer than `max_dimension` and there are no more than
/// `SCENE_CANVAS_TEXELS` in all.
pub(crate) fn scene_canvas_size(canvas: [f32; 2], density: f64, max_dimension: u32) -> (u32, u32) {
    let (width, height) = (f64::from(canvas[0]), f64::from(canvas[1]));
    let largest = width.max(height).max(1.0);
    let scale = density.min(f64::from(max_dimension) / largest);
    let size = |scale: f64, round: fn(f64) -> f64| {
        (
            (round(width * scale) as u32).max(1),
            (round(height * scale) as u32).max(1),
        )
    };
    let (columns, rows) = size(scale, f64::ceil);
    if u64::from(columns) * u64::from(rows) <= SCENE_CANVAS_TEXELS {
        return (columns, rows);
    }
    // Rounded down, the product of the sides stays within the cap.
    let fits = (SCENE_CANVAS_TEXELS as f64 / (width * height)).sqrt();
    size(scale.min(fits), f64::floor)
}

/// The canvas `frame` of `model` is drawn on: its own, or else the
/// model's.
pub(crate) fn frame_canvas(model: &SceneModel, frame: &SceneFrame) -> [f32; 2] {
    frame.canvas.unwrap_or(model.canvas)
}

/// Why a scene cannot be drawn on `canvas`, the model's or a frame's own
/// (`whose`): a side that is not finite, shorter than 1, or longer than
/// `MAX_SCENE_CANVAS_SIDE`. At least 1 rather than positive: a subnormal
/// side makes the aspect (`width / height`) 0 or infinite, which the
/// projection divides by, so the shader would get infinities and NaNs.
fn invalid_canvas(whose: &str, canvas: [f32; 2]) -> Option<String> {
    if canvas
        .iter()
        .all(|side| (1.0..=MAX_SCENE_CANVAS_SIDE).contains(side))
    {
        return None;
    }
    Some(format!(
        "{whose} canvas is {canvas:?}: each side must be finite, at least 1, and at most {MAX_SCENE_CANVAS_SIDE}"
    ))
}

/// Why `frame`'s own canvas cannot be drawn on, by `invalid_canvas`'s
/// rule. Checked before the canvas is sized, so that it is refused rather
/// than clamped.
pub(crate) fn invalid_frame_canvas(frame: &SceneFrame) -> Option<String> {
    invalid_canvas("the frame's", frame.canvas?)
}

/// A scene model and its copy on the GPU.
pub(crate) struct LoadedScene {
    pub(crate) id: ModelId,
    pub(crate) model: SceneModel,
    /// The canvas its last frame was drawn on, which a layer marks used
    /// before the plugin is asked for the next frame.
    pub(crate) last_canvas: [f32; 2],
    /// One per `model.geometries` entry.
    pub(crate) geometries: Vec<GeometryBuffers>,
    /// One per `model.materials` entry: its textures and samplers, bound as
    /// group 2 of its draws.
    pub(crate) materials: Vec<wgpu::BindGroup>,
    /// One per `model.materials` entry: the shader it is drawn with.
    pub(crate) shaders: Vec<Arc<CompiledShader>>,
    /// The pipelines its materials draw with, held so that
    /// `ScenePipelines` keeps them while the model is loaded; a draw finds
    /// its pipeline by key.
    _pipelines: Vec<Arc<wgpu::RenderPipeline>>,
    /// One per `model.draws` entry.
    pub(crate) draws: Vec<DrawBuffers>,
}

/// A geometry's vertex streams and triangles, laid out to be read as
/// storage buffers (each is also usable as a vertex buffer). Every stream
/// has one element per vertex, in vertex order; an empty one still holds a
/// single zeroed 32-byte element, so that it can be bound.
pub(crate) struct GeometryBuffers {
    pub(crate) vertices: u32,
    /// The centre of the rest positions' bounds, which a blended draw
    /// without its own `center` is sorted by.
    pub(crate) center: [f32; 3],
    /// `vec4<f32>`: the position, `w` 1.
    pub(crate) positions: wgpu::Buffer,
    /// `vec4<f32>`: the normal, `w` 0.
    pub(crate) normals: wgpu::Buffer,
    /// `vec2<f32>`, `v` counting down from the texture's top row.
    pub(crate) uvs: wgpu::Buffer,
    /// `vec4<f32>`, the bitangent's sign in `w`.
    pub(crate) tangents: Option<wgpu::Buffer>,
    /// `scene_skin.wgsl`'s `SkinVertex`, when the geometry is skinned: the
    /// joints as `vec4<u32>`, widened from the model's `u16`s, then their
    /// weights as `vec4<f32>`.
    pub(crate) skin: Option<wgpu::Buffer>,
    /// `vec4<f32>` position offsets, `w` 0, target by target: target `t`'s
    /// offset for vertex `v` is element `t × vertices + v`.
    pub(crate) morph_positions: Option<wgpu::Buffer>,
    /// Normal offsets laid out as `morph_positions`, zero for a target that
    /// leaves the normals as they are; absent when every target does.
    pub(crate) morph_normals: Option<wgpu::Buffer>,
    /// `u32`, three per triangle.
    pub(crate) indices: wgpu::Buffer,
    pub(crate) index_count: u32,
}

/// What one draw's vertices are skinned into, and the bind group that
/// skins them.
pub(crate) struct DrawBuffers {
    /// `scene_skin.wgsl`'s `Skinned`, one per vertex of the draw's
    /// geometry, rewritten each frame the draw is visible; read as the
    /// draw's vertex buffer.
    skinned: wgpu::Buffer,
    /// The geometry's streams and `skinned`, as `scene_skin.wgsl`'s group 0.
    skin_group: wgpu::BindGroup,
}

/// What scene frames draw with, kept from frame to frame: the canvases of
/// each size a scene layer has had since the last frame began, within
/// `target_texels`, and the buffers of a frame's skinning and draws, which
/// grow to the largest frame's and are rewritten by every frame. A frame
/// without a scene layer lets all of it go; one whose scene layer fails or
/// draws nothing keeps it.
pub(crate) struct SceneFrameResources {
    /// By canvas size in texels.
    targets: HashMap<(u32, u32), SceneTargets>,
    /// The most texels `targets` keeps before it drops the sizes this frame
    /// has not drawn: `SCENE_TARGET_TEXELS`, lowered by tests.
    pub(crate) target_texels: u64,
    /// The sizes `mark_used` has had since the last `begin_frame`, drawn or
    /// not.
    used: HashSet<(u32, u32)>,
    /// `STORAGE`: every skin's matrices, then each queued draw's morph
    /// pairs.
    frame_data: GrowingBuffer,
    /// `UNIFORM`: one `SkinParams` per queued draw.
    skin_params: GrowingBuffer,
    /// `UNIFORM`: one slot per queued draw (`draw_slot`): its `DrawUniform`,
    /// then its material's parameters.
    draws: GrowingBuffer,
    /// `UNIFORM`: the `FrameUniform`: the camera, lights, and time.
    view: GrowingBuffer,
    /// What binds the buffers above, made again when one of them is.
    groups: Option<FrameGroups>,
    /// The textures and buffers created, ever.
    pub(crate) allocations: u64,
}

impl Default for SceneFrameResources {
    fn default() -> Self {
        Self {
            targets: HashMap::new(),
            target_texels: SCENE_TARGET_TEXELS,
            used: HashSet::new(),
            frame_data: GrowingBuffer::default(),
            skin_params: GrowingBuffer::default(),
            draws: GrowingBuffer::default(),
            view: GrowingBuffer::default(),
            groups: None,
            allocations: 0,
        }
    }
}

impl SceneFrameResources {
    /// Starts a frame: the canvases of sizes the last frame did not draw
    /// are dropped, and everything else too when it drew no scene.
    pub(crate) fn begin_frame(&mut self) {
        if self.used.is_empty() {
            *self = Self {
                target_texels: self.target_texels,
                allocations: self.allocations,
                ..Self::default()
            };
            return;
        }
        self.targets.retain(|size, _| self.used.contains(size));
        self.used.clear();
    }

    /// Keeps the canvas of `size`, and the buffers, past the next
    /// `begin_frame`. A scene layer marks the size its model's last frame
    /// had before anything can fail, so that a frame that errors or has
    /// nothing to draw does not let go of what the next frame draws with,
    /// and the size of its frame's canvas once the frame says it.
    pub(crate) fn mark_used(&mut self, size: (u32, u32)) {
        self.used.insert(size);
    }

    /// The canvas sizes kept, smallest first.
    #[cfg(test)]
    pub(crate) fn cached_sizes(&self) -> Vec<(u32, u32)> {
        let mut sizes: Vec<_> = self.targets.keys().copied().collect();
        sizes.sort_by_key(|&size| texels(size));
        sizes
    }

    /// Whether any frame buffer, or what binds them, is kept.
    #[cfg(test)]
    pub(crate) fn holds_buffers(&self) -> bool {
        let buffers = [&self.frame_data, &self.skin_params, &self.draws, &self.view];
        self.groups.is_some() || buffers.iter().any(|buffer| buffer.buffer.is_some())
    }
}

fn texels((width, height): (u32, u32)) -> u64 {
    u64::from(width) * u64::from(height)
}

/// Makes room in `targets` for a canvas of `size`, which it lacks: when
/// the canvases kept and it would be more than `budget` texels, the sizes
/// not in `used` are dropped. What `used` holds stays, even over `budget`.
fn make_room<T>(
    targets: &mut HashMap<(u32, u32), T>,
    used: &HashSet<(u32, u32)>,
    size: (u32, u32),
    budget: u64,
) {
    let kept: u64 = targets.keys().map(|&size| texels(size)).sum();
    if kept + texels(size) > budget {
        targets.retain(|size, _| used.contains(size));
    }
}

/// A scene canvas of one size: its multisampled color and depth, the color
/// they resolve to, and the bind group that converts it. At 1 sample there
/// is no multisampled color: the draws go straight to `resolved`.
struct SceneTargets {
    multisampled: Option<wgpu::TextureView>,
    depth: wgpu::TextureView,
    resolved: wgpu::TextureView,
    convert_group: wgpu::BindGroup,
}

/// The bind groups of a frame's buffers.
struct FrameGroups {
    /// `frame_data` and `skin_params`, as `scene_skin.wgsl`'s group 1.
    frame: wgpu::BindGroup,
    /// `view`, as the material interface's group 0.
    view: wgpu::BindGroup,
    /// `draws`, as the material interface's group 1.
    draws: wgpu::BindGroup,
}

/// A buffer reused from frame to frame, made larger when what is written
/// does not fit.
#[derive(Default)]
struct GrowingBuffer {
    buffer: Option<wgpu::Buffer>,
    capacity: u64,
}

impl GrowingBuffer {
    /// Writes `bytes` at the start of the buffer, first making a buffer of
    /// `usage` large enough for them when this one is not: their size's
    /// next power of two, at least 256 bytes and at most `largest`, which
    /// they fit. Returns whether it made a new buffer.
    fn write(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        usage: wgpu::BufferUsages,
        bytes: &[u8],
        largest: u64,
        allocations: &mut u64,
    ) -> bool {
        let size = bytes.len() as u64;
        let reallocated = self.buffer.is_none() || size > self.capacity;
        if reallocated {
            self.capacity = size.next_power_of_two().max(256).min(largest);
            self.buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Celesta scene frame buffer"),
                size: self.capacity,
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            *allocations += 1;
        }
        queue.write_buffer(self.buffer(), 0, bytes);
        reallocated
    }

    fn buffer(&self) -> &wgpu::Buffer {
        self.buffer.as_ref().expect("written before it is bound")
    }
}

/// A compiled material shader, by the order it was compiled in: 0 the
/// built-in unlit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ShaderId(u32);

/// A material shader assembled with Celesta's entry points, checked, and
/// compiled (`scene_material.rs`).
pub(crate) struct CompiledShader {
    id: ShaderId,
    module: wgpu::ShaderModule,
    /// The bytes of its `Params`.
    params_size: u32,
    /// Its texture slots.
    textures: usize,
    /// Whether it has the outline pass's entry points.
    outline: bool,
}

/// A plugin's material shader as the shader cache knows it: its WGSL's
/// SHA-256 digest and length, which stand for the source without keeping
/// it, and its texture slots.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ShaderKey {
    digest: [u8; 32],
    len: usize,
    textures: usize,
}

impl ShaderKey {
    fn of(shader: &SceneShader) -> Self {
        Self {
            digest: Sha256::digest(shader.wgsl.as_bytes()).into(),
            len: shader.wgsl.len(),
            textures: shader.textures.len(),
        }
    }
}

/// Why a material shader was not compiled.
#[derive(Debug)]
enum ShaderFailure {
    /// It breaks the interface, or the device refuses its module: it would
    /// again.
    Refused(String),
    /// The device ran out of memory, which it may not the next time.
    OutOfMemory(String),
}

impl CompiledShader {
    /// Checks the material shader `wgsl` of `textures` slots and compiles
    /// it, or says why it breaks the interface.
    fn compile(
        device: &wgpu::Device,
        id: ShaderId,
        wgsl: &str,
        textures: usize,
        label: &str,
    ) -> Result<Self, ShaderFailure> {
        let checked = check_material(wgsl, textures).map_err(ShaderFailure::Refused)?;
        let module = scoped(device, || {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Naga(Cow::Owned(checked.module)),
            })
        })
        .map_err(|error| match error {
            wgpu::Error::OutOfMemory { .. } => ShaderFailure::OutOfMemory(error.to_string()),
            _ => ShaderFailure::Refused(error.to_string()),
        })?;
        Ok(Self {
            id,
            module,
            params_size: checked.params_size,
            textures,
            outline: checked.outline,
        })
    }
}

/// What `make` returns, or the first error the device reports while it
/// runs: a validation error, an internal one (a backend's shader compiler
/// refusing what naga accepted, such as a fragment function past Metal's
/// stack), or running out of memory. What a material's WGSL decides runs in
/// here, so that none of it reaches the device's uncaptured-error handler,
/// which panics.
fn scoped<T>(device: &wgpu::Device, make: impl FnOnce() -> T) -> Result<T, wgpu::Error> {
    let memory = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let internal = device.push_error_scope(wgpu::ErrorFilter::Internal);
    let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let made = make();
    // Popped innermost first.
    let errors = [validation, internal, memory].map(|scope| pollster::block_on(scope.pop()));
    match errors.into_iter().flatten().next() {
        Some(error) => Err(error),
        None => Ok(made),
    }
}

/// Which of a draw's passes a pipeline draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Pass {
    /// The surface.
    Surface,
    /// The outline, drawn after the surface, pushed away from the camera
    /// by its depth bias: `bias_slope` is an `f32`'s bits.
    Outline { bias_constant: i32, bias_slope: u32 },
}

/// Which faces a pipeline culls.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Cull {
    /// A double-sided material keeps both, as does an outline asking to.
    None,
    /// A single-sided material shows its front faces.
    Back,
    /// An outline, by default, shows the back faces it pushes out.
    Front,
}

/// Which faces a draw keeps, and which way its front faces turn on screen:
/// clockwise when its world matrix or the camera's view mirrors (its upper
/// 3×3's determinant is negative), but not both.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Facing {
    cull: Cull,
    clockwise: bool,
}

/// What a scene draw pipeline is made for. `ScenePipelines::prepare`
/// makes the ones a model's materials use when it loads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PipelineKey {
    shader: ShaderId,
    pass: Pass,
    facing: Facing,
    alpha: Alpha,
}

impl PipelineKey {
    /// The surface pass of `material`, drawn with `shader`, its front faces
    /// clockwise or not.
    fn surface(material: &Material, shader: ShaderId, clockwise: bool) -> Self {
        Self {
            shader,
            pass: Pass::Surface,
            facing: Facing {
                cull: if material.double_sided {
                    Cull::None
                } else {
                    Cull::Back
                },
                clockwise,
            },
            alpha: Alpha::of(&material.alpha_mode),
        }
    }

    /// The outline pass of `material`, drawn with `shader`, its front faces
    /// clockwise or not, when `shader` has the outline's entry points: a
    /// custom material's with an `outline`, culled and pushed away from
    /// the camera as it asks, whether or not the material is double-sided.
    /// It draws the material's alpha.
    fn outline(material: &Material, shader: &CompiledShader, clockwise: bool) -> Option<Self> {
        let pass = match &material.kind {
            MaterialKind::Custom(custom) => custom.outline?,
            MaterialKind::Unlit(_) => return None,
        };
        // `prepare` refuses a custom material asking for an outline its
        // shader does not define.
        shader.outline.then_some(Self {
            shader: shader.id,
            pass: Pass::Outline {
                bias_constant: pass.depth_bias.constant,
                bias_slope: pass.depth_bias.slope_scale.to_bits(),
            },
            facing: Facing {
                cull: match pass.cull {
                    CullFace::None => Cull::None,
                    CullFace::Front => Cull::Front,
                    CullFace::Back => Cull::Back,
                },
                clockwise,
            },
            alpha: Alpha::of(&material.alpha_mode),
        })
    }

    /// Its vertex and fragment entry points: `scene_material_entry.wgsl`'s
    /// or `scene_material_outline_entry.wgsl`'s.
    fn entry_points(self) -> (&'static str, &'static str) {
        match (self.pass, self.alpha) {
            (Pass::Surface, Alpha::Opaque) => ("celesta_surface_vertex", "celesta_surface_opaque"),
            (Pass::Surface, Alpha::Mask) => ("celesta_surface_vertex", "celesta_surface_mask"),
            (Pass::Surface, Alpha::BlendDepthWrite | Alpha::Blend) => {
                ("celesta_surface_vertex", "celesta_surface_blend")
            }
            (Pass::Outline { .. }, Alpha::Opaque) => {
                ("celesta_outline_vertex", "celesta_outline_opaque")
            }
            (Pass::Outline { .. }, Alpha::Mask) => {
                ("celesta_outline_vertex", "celesta_outline_mask")
            }
            (Pass::Outline { .. }, Alpha::BlendDepthWrite | Alpha::Blend) => {
                ("celesta_outline_vertex", "celesta_outline_blend")
            }
        }
    }

    /// What the backend's shader compiler is given for it.
    fn stages(self) -> Stages {
        let (vertex, fragment) = self.entry_points();
        Stages {
            shader: self.shader,
            vertex,
            fragment,
        }
    }

    /// Its depth bias.
    fn depth_bias(self) -> wgpu::DepthBiasState {
        match self.pass {
            Pass::Surface => Default::default(),
            Pass::Outline {
                bias_constant,
                bias_slope,
            } => wgpu::DepthBiasState {
                constant: bias_constant,
                slope_scale: f32::from_bits(bias_slope),
                clamp: 0.0,
            },
        }
    }
}

/// What a backend compiles for a pipeline: a shader's module and two of
/// its entry points. Culling, winding, depth bias, depth writes, and
/// blending are the pipeline's state around them, so pipelines that differ
/// only in those compile the same.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Stages {
    shader: ShaderId,
    vertex: &'static str,
    fragment: &'static str,
}

/// How a draw's alpha is drawn, as its material's alpha mode says.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Alpha {
    /// Alpha ignored, depth written.
    Opaque,
    /// Coverage from the alpha against the cutoff, depth written.
    Mask,
    /// Blended by alpha over what is drawn, depth written: a blend with
    /// `depth_write`.
    BlendDepthWrite,
    /// Blended by alpha over what is drawn, depth tested but not written.
    Blend,
}

impl Alpha {
    /// How `alpha_mode` is drawn.
    fn of(alpha_mode: &AlphaMode) -> Self {
        match *alpha_mode {
            AlphaMode::Opaque => Self::Opaque,
            AlphaMode::Mask { .. } => Self::Mask,
            AlphaMode::Blend {
                depth_write: true, ..
            } => Self::BlendDepthWrite,
            AlphaMode::Blend {
                depth_write: false, ..
            } => Self::Blend,
        }
    }

    /// Unity's render queue before a material's offset: lower queues draw
    /// first.
    fn queue(self) -> u32 {
        match self {
            Self::Opaque => 2000,
            Self::Mask => 2450,
            Self::BlendDepthWrite => 2501,
            Self::Blend => 3000,
        }
    }

    fn writes_depth(self) -> bool {
        self != Self::Blend
    }

    /// Straight color over the canvas's premultiplied color, as Unity
    /// blends it; alpha accumulates as coverage.
    fn blend(self) -> Option<wgpu::BlendState> {
        matches!(self, Self::Blend | Self::BlendDepthWrite).then_some(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        })
    }
}

/// `alpha_mode`'s place in Unity's render queues: 2000 opaque, 2450 mask,
/// 3000 blend. A blend moves by its `queue_offset`, to
/// 2501 to 2510 with `depth_write` and 2991 to 3000 without.
fn render_queue(alpha_mode: &AlphaMode) -> u32 {
    let alpha = Alpha::of(alpha_mode);
    let offset = match *alpha_mode {
        AlphaMode::Blend {
            depth_write: true,
            queue_offset,
        } => queue_offset.clamp(0, 9),
        AlphaMode::Blend {
            depth_write: false,
            queue_offset,
        } => queue_offset.clamp(-9, 0),
        AlphaMode::Opaque | AlphaMode::Mask { .. } => 0,
    };
    alpha.queue().saturating_add_signed(offset)
}

/// What `scene_skin.wgsl` reads as `Params`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SkinParams {
    counts: [u32; 4],
    flags: [u32; 4],
}

/// The most directional lights a frame has: Unity's default pixel light
/// count, which `Frame`'s `lights` holds.
const MAX_DIRECTIONAL_LIGHTS: usize = 4;

/// One of `FrameUniform::lights`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct LightUniform {
    /// `L`: towards the light, length 1, `w` 0.
    to_light: [f32; 4],
    /// The light's color times its intensity, `w` 0.
    color: [f32; 4],
}

/// What the material interface reads as `Frame`, group 0: the camera, the
/// lights, and the time.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct FrameUniform {
    view_projection: [f32; 16],
    view: [f32; 16],
    projection: [f32; 16],
    /// The camera's position in world space; `w` 1 for a perspective
    /// camera, 0 for an orthographic one.
    eye: [f32; 4],
    /// The camera's +Z, towards the viewer, in world space.
    forward: [f32; 4],
    /// The frame's time in seconds.
    time: f32,
    /// The canvas's height over its width.
    height_over_width: f32,
    /// How many of `lights` there are.
    light_count: u32,
    reserved: u32,
    /// The main light first; black when the frame has none.
    lights: [LightUniform; MAX_DIRECTIONAL_LIGHTS],
    /// `Lights::ambient`, `w` 0.
    ambient: [[f32; 4]; 9],
}

/// What the material interface reads as `Draw`, group 1 binding 0 at the
/// start of each draw's slot.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct DrawUniform {
    world: [f32; 16],
    /// `normal_matrix(world)`'s columns, `w` 0.
    normal_matrix: [[f32; 4]; 3],
    /// `handedness(world)`.
    handedness: f32,
    /// A mask material's cutoff, else 0.
    alpha_cutoff: f32,
    reserved: [f32; 2],
}

// The sizes of the material interface's `Frame` and `Draw`.
const _: () = assert!(std::mem::size_of::<FrameUniform>() == 512);
const _: () = assert!(std::mem::size_of::<DrawUniform>() == 128);

/// The built-in unlit material's shader, of one slot: its base color
/// texture.
const UNLIT_SHADER: &str = include_str!("scene_unlit.wgsl");

/// The bytes of `scene_unlit.wgsl`'s `Params`: `unlit_params`.
const UNLIT_PARAMS: usize = 48;

/// What `scene_unlit.wgsl` reads as `Params`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct UnlitParams {
    base_color: [f32; 4],
    /// The texture transform's rows, `w` 0.
    uv_u: [f32; 4],
    uv_v: [f32; 4],
}

const _: () = assert!(std::mem::size_of::<UnlitParams>() == UNLIT_PARAMS);

/// How many plugin shaders the shader cache holds before it drops those
/// no loaded model uses: eight models listing the most shaders a model
/// may.
pub(crate) const MAX_CACHED_SHADERS: usize = 8 * MAX_SCENE_SHADERS;

/// How many draw pipelines the pipeline cache holds before it drops those
/// no loaded model holds. Loaded models keep theirs however many they
/// are: the bound is on what the cache keeps for models that are gone.
pub(crate) const MAX_CACHED_PIPELINES: usize = 256;

/// The most draw pipelines one model's materials may need. A shader has at
/// most 16 surface pipelines (4 alpha modes, 2 cullings, and 2 windings),
/// and at most 24 outline pipelines for each depth bias its materials'
/// outlines use (4 alpha modes, 3 cullings, and 2 windings). The cap is
/// not every combination: the surface pipelines of the built-in unlit
/// shader and `MAX_SCENE_SHADERS` plugin shaders, with the outlines of
/// one bias for each plugin shader, are more.
pub(crate) const MAX_MODEL_PIPELINES: usize = 1024;

/// What a model's materials draw with, which `ScenePipelines::prepare`
/// gives the loaded model to hold.
pub(crate) struct Prepared {
    /// One per material: the shader it is drawn with.
    pub(crate) shaders: Vec<Arc<CompiledShader>>,
    /// Every pipeline the materials draw with.
    pub(crate) pipelines: Vec<Arc<wgpu::RenderPipeline>>,
}

/// The pipelines that draw scenes and what they bind, shared by every
/// scene.
pub(crate) struct ScenePipelines {
    /// What every draw pipeline, and every canvas `SceneFrameResources`
    /// keeps, is made for: the renderer's `scene_target`, chosen once when
    /// it is made, so no cache here or there holds anything made for
    /// another, and none is keyed by it.
    target: SceneTarget,
    geometry_layout: wgpu::BindGroupLayout,
    frame_layout: wgpu::BindGroupLayout,
    skin: wgpu::ComputePipeline,
    /// The built-in unlit material's shader.
    unlit: Arc<CompiledShader>,
    /// Plugins' material shaders by their WGSL and texture slots (their
    /// names and empty texels do not change the module), compiled or why
    /// they cannot be: each is checked once, however many models list it,
    /// unless the device ran out of memory compiling it. Past
    /// `MAX_CACHED_SHADERS`, those no loaded model holds are dropped, with
    /// their pipelines.
    shaders: HashMap<ShaderKey, Result<Arc<CompiledShader>, String>>,
    /// The `ShaderId` the next plugin shader that compiles gets.
    next_shader: u32,
    /// How many plugin shaders were checked: the cache's misses.
    #[cfg(test)]
    pub(crate) compiles: usize,
    /// The next plugin shader checked runs out of memory instead.
    #[cfg(test)]
    pub(crate) out_of_memory: bool,
    /// How many draw pipelines were made or tried.
    #[cfg(test)]
    pub(crate) pipelines_made: usize,
    /// The next draw pipeline made fails with this error instead.
    #[cfg(test)]
    pub(crate) refuse_pipeline: Option<wgpu::Error>,
    /// Group 0: the frame uniform.
    view_layout: wgpu::BindGroupLayout,
    /// Group 1: the draw uniform and the material's parameters.
    draw_layout: wgpu::BindGroupLayout,
    /// By texture slots: group 2 of a material of that many (slot `k`'s
    /// texture at binding `2k`, its sampler at `2k + 1`), and the pipeline
    /// layout of groups 0 to 2.
    texture_layouts: HashMap<usize, (wgpu::BindGroupLayout, wgpu::PipelineLayout)>,
    /// The pipelines the loaded models' materials use, made as each loads.
    /// Past `MAX_CACHED_PIPELINES`, those no loaded model holds are
    /// dropped.
    draw: HashMap<PipelineKey, Arc<wgpu::RenderPipeline>>,
    /// Why the device refused the pipelines it refused (a backend's
    /// compiler refusing what naga accepted), by what it compiled, so that
    /// a model sent again, or with another culling or depth bias, is
    /// refused without making them again. At most one for each pair of
    /// entry points of a cached shader; dropped with the shader. Running
    /// out of memory is not kept: the device may not the next time.
    refused: HashMap<Stages, String>,
    convert_layout: wgpu::BindGroupLayout,
    convert: wgpu::RenderPipeline,
    /// Zeros, bound in place of a stream a geometry lacks.
    empty: wgpu::Buffer,
    /// 1x1 `Rgba8Unorm` texels, bound in place of a texture a material
    /// lacks.
    white: wgpu::TextureView,
    black: wgpu::TextureView,
    /// What samples `white` and `black`.
    default_sampler: wgpu::Sampler,
}

fn storage_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

/// A uniform of `size` bytes bound at a different offset for each draw.
fn dynamic_uniform_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
    size: u64,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: true,
            min_binding_size: wgpu::BufferSize::new(size),
        },
        count: None,
    }
}

impl ScenePipelines {
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue, target: SceneTarget) -> Self {
        let skin_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Celesta scene skinning shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("scene_skin.wgsl").into()),
        });
        let convert_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Celesta scene convert shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("scene.wgsl").into()),
        });
        let unlit = Arc::new(
            CompiledShader::compile(
                device,
                ShaderId(0),
                UNLIT_SHADER,
                1,
                "Celesta scene unlit material",
            )
            .expect("the built-in material passes the material checks"),
        );

        let geometry_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Celesta scene geometry layout"),
            entries: &[
                storage_entry(0, true),
                storage_entry(1, true),
                storage_entry(2, true),
                storage_entry(3, true),
                storage_entry(4, true),
                storage_entry(5, true),
                storage_entry(6, false),
            ],
        });
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Celesta scene skinning frame layout"),
            entries: &[
                storage_entry(0, true),
                dynamic_uniform_entry(
                    1,
                    wgpu::ShaderStages::COMPUTE,
                    std::mem::size_of::<SkinParams>() as u64,
                ),
            ],
        });
        let skin = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Celesta scene skinning"),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("Celesta scene skinning layout"),
                    bind_group_layouts: &[Some(&geometry_layout), Some(&frame_layout)],
                    immediate_size: 0,
                }),
            ),
            module: &skin_shader,
            entry_point: Some("cs_main"),
            compilation_options: Default::default(),
            cache: None,
        });

        let view_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Celesta scene frame layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(
                        std::mem::size_of::<FrameUniform>() as u64
                    ),
                },
                count: None,
            }],
        });
        let both = wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT;
        let draw_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Celesta scene draw layout"),
            entries: &[
                dynamic_uniform_entry(0, both, std::mem::size_of::<DrawUniform>() as u64),
                // Bound whole, whatever the size of the material's `Params`.
                dynamic_uniform_entry(1, both, MAX_MATERIAL_PARAMS as u64),
            ],
        });

        let convert_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Celesta scene convert layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let convert = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Celesta scene convert"),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("Celesta scene convert layout"),
                    bind_group_layouts: &[Some(&convert_layout)],
                    immediate_size: 0,
                }),
            ),
            vertex: wgpu::VertexState {
                module: &convert_shader,
                entry_point: Some("vs_full"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &convert_shader,
                entry_point: Some("fs_convert"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: OUTPUT_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });

        let empty = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Celesta scene empty stream"),
            contents: &[0; SKIN_VERTEX_STRIDE as usize],
            usage: wgpu::BufferUsages::STORAGE,
        });
        let texel = |label, value: [u8; 4]| {
            device
                .create_texture_with_data(
                    queue,
                    &wgpu::TextureDescriptor {
                        label: Some(label),
                        size: wgpu::Extent3d {
                            width: 1,
                            height: 1,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    },
                    wgpu::util::TextureDataOrder::LayerMajor,
                    &value,
                )
                .create_view(&Default::default())
        };
        let mut pipelines = Self {
            target,
            geometry_layout,
            frame_layout,
            skin,
            unlit,
            shaders: HashMap::new(),
            next_shader: 1,
            #[cfg(test)]
            compiles: 0,
            #[cfg(test)]
            out_of_memory: false,
            #[cfg(test)]
            pipelines_made: 0,
            #[cfg(test)]
            refuse_pipeline: None,
            view_layout,
            draw_layout,
            texture_layouts: HashMap::new(),
            draw: HashMap::new(),
            refused: HashMap::new(),
            convert_layout,
            convert,
            empty,
            white: texel("Celesta scene white texel", [255; 4]),
            black: texel("Celesta scene black texel", [0, 0, 0, 255]),
            default_sampler: device.create_sampler(&Default::default()),
        };
        pipelines.texture_layout(device, pipelines.unlit.textures);
        pipelines
    }

    /// The group 2 layout of a material of `textures` slots and its
    /// pipeline layout, made the first time a material has that many.
    fn texture_layout(
        &mut self,
        device: &wgpu::Device,
        textures: usize,
    ) -> &(wgpu::BindGroupLayout, wgpu::PipelineLayout) {
        let Self {
            view_layout,
            draw_layout,
            texture_layouts,
            ..
        } = self;
        texture_layouts.entry(textures).or_insert_with(|| {
            let entries: Vec<wgpu::BindGroupLayoutEntry> = (0..textures as u32)
                .flat_map(|slot| texture_entries(2 * slot))
                .collect();
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Celesta scene material layout"),
                entries: &entries,
            });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Celesta scene draw layout"),
                bind_group_layouts: &[Some(view_layout), Some(draw_layout), Some(&layout)],
                immediate_size: 0,
            });
            (layout, pipeline_layout)
        })
    }

    /// The compiled module of `shader`, from the cache or checked and
    /// compiled now, or why it cannot be compiled. A shader that fails
    /// takes no `ShaderId`; one the device ran out of memory compiling is
    /// not kept, and is tried again the next time a model lists it.
    fn shader(
        &mut self,
        device: &wgpu::Device,
        shader: &SceneShader,
    ) -> Result<Arc<CompiledShader>, String> {
        let key = ShaderKey::of(shader);
        if let Some(compiled) = self.shaders.get(&key) {
            return compiled.clone();
        }
        let compiled = match self.compile(device, shader) {
            Ok(compiled) => {
                self.next_shader += 1;
                Ok(Arc::new(compiled))
            }
            Err(ShaderFailure::Refused(message)) => Err(message),
            Err(ShaderFailure::OutOfMemory(message)) => return Err(message),
        };
        if self.shaders.len() >= MAX_CACHED_SHADERS {
            self.drop_unused_shaders();
        }
        self.shaders.insert(key, compiled.clone());
        compiled
    }

    /// Drops the cached shaders no loaded model holds (every refusal, and
    /// every compiled shader the cache alone holds) and the pipelines made
    /// or refused with them. `ShaderId`s are not given out again, so no
    /// pipeline key of a dropped shader stands for another.
    fn drop_unused_shaders(&mut self) {
        let mut dropped = HashSet::new();
        self.shaders.retain(|_, compiled| match compiled {
            Ok(shader) if Arc::strong_count(shader) > 1 => true,
            Ok(shader) => {
                dropped.insert(shader.id);
                false
            }
            Err(_) => false,
        });
        self.draw.retain(|key, _| !dropped.contains(&key.shader));
        self.refused.retain(|key, _| !dropped.contains(&key.shader));
    }

    /// Drops the draw pipelines no loaded model holds. Refusals stay while
    /// their shader is cached: a refused model is never loaded to hold
    /// them.
    fn drop_unused_pipelines(&mut self) {
        self.draw
            .retain(|_, pipeline| Arc::strong_count(pipeline) > 1);
    }

    /// How many shaders, draw pipelines, and refused pipelines are kept.
    #[cfg(test)]
    pub(crate) fn kept(&self) -> (usize, usize, usize) {
        (self.shaders.len(), self.draw.len(), self.refused.len())
    }

    /// `shader` checked and compiled as the next `ShaderId`.
    fn compile(
        &mut self,
        device: &wgpu::Device,
        shader: &SceneShader,
    ) -> Result<CompiledShader, ShaderFailure> {
        #[cfg(test)]
        {
            self.compiles += 1;
            if std::mem::take(&mut self.out_of_memory) {
                let error = wgpu::Error::OutOfMemory {
                    source: "the test's device".into(),
                };
                return Err(ShaderFailure::OutOfMemory(error.to_string()));
            }
        }
        CompiledShader::compile(
            device,
            ShaderId(self.next_shader),
            &shader.wgsl,
            shader.textures.len(),
            shader
                .name
                .as_deref()
                .unwrap_or("Celesta scene plugin material"),
        )
    }

    /// Compiles every shader `model` lists, used or not; makes the
    /// pipelines its materials draw with, surface passes and outlines, for
    /// either winding, unless they are made already; and gives the shader
    /// each material is drawn with and the pipelines they draw with. A
    /// custom material's parameters must be as long as its shader's
    /// `Params`, and its shader must define the outline it asks for. A
    /// model whose materials need more than `MAX_MODEL_PIPELINES` is
    /// refused before any is made. Each pipeline is made in its own error
    /// scope: when one fails, the message names the material and none of
    /// the material's pipelines is kept, and one the device refused is
    /// refused without being made again the next time a model asks for it
    /// or for another of the same `Stages`, before any of that model's
    /// pipelines is made.
    pub(crate) fn prepare(
        &mut self,
        device: &wgpu::Device,
        model: &SceneModel,
    ) -> Result<Prepared, String> {
        let mut compiled = Vec::with_capacity(model.shaders.len());
        for (index, shader) in model.shaders.iter().enumerate() {
            match self.shader(device, shader) {
                Ok(shader) => compiled.push(shader),
                Err(error) => {
                    let user = model.materials.iter().position(|material| {
                        matches!(&material.kind, MaterialKind::Custom(custom) if custom.shader == index)
                    });
                    let named = shader_named(index, shader);
                    return Err(match user {
                        Some(material) => format!("material {material}'s {named}: {error}"),
                        None => format!("{named}: {error}"),
                    });
                }
            }
        }
        // Each material's shader and the pipelines it draws with, checked
        // before any is made.
        let mut drawn = Vec::with_capacity(model.materials.len());
        let mut needed = HashSet::new();
        for (index, material) in model.materials.iter().enumerate() {
            let shader = match &material.kind {
                MaterialKind::Unlit(_) => Arc::clone(&self.unlit),
                MaterialKind::Custom(custom) => {
                    let shader = Arc::clone(&compiled[custom.shader]);
                    if custom.params.len() != shader.params_size as usize {
                        return Err(format!(
                            "material {index}'s parameters are {} bytes, but shader {}'s Params is {}",
                            custom.params.len(),
                            custom.shader,
                            shader.params_size
                        ));
                    }
                    if custom.outline.is_some() && !shader.outline {
                        return Err(format!(
                            "material {index} asks for an outline, but shader {} defines no `material_outline_fragment`",
                            custom.shader
                        ));
                    }
                    shader
                }
            };
            let keys: Vec<PipelineKey> = [false, true]
                .into_iter()
                .flat_map(|clockwise| {
                    std::iter::once(PipelineKey::surface(material, shader.id, clockwise))
                        .chain(PipelineKey::outline(material, &shader, clockwise))
                })
                .collect();
            needed.extend(keys.iter().copied());
            drawn.push((shader, keys));
        }
        // A model cannot make pipelines without end through distinct depth
        // biases, say: each is a backend compile, and the loaded model
        // holds them all.
        if needed.len() > MAX_MODEL_PIPELINES {
            return Err(format!(
                "the model's materials need {} pipelines, more than {MAX_MODEL_PIPELINES}",
                needed.len()
            ));
        }
        // Before this model makes any: the pipelines of its earlier
        // materials are not held until it is loaded.
        if self.draw.len() >= MAX_CACHED_PIPELINES {
            self.drop_unused_pipelines();
        }
        // A pipeline it lacks that the device refused refuses the model
        // before any is made, so that one sent again makes none.
        for (index, (_, keys)) in drawn.iter().enumerate() {
            if let Some(message) = keys
                .iter()
                .filter(|key| !self.draw.contains_key(key))
                .find_map(|key| self.refused.get(&key.stages()))
            {
                return Err(format!("material {index}'s pipelines: {message}"));
            }
        }
        let mut shaders = Vec::with_capacity(drawn.len());
        let mut pipelines = Vec::new();
        for (index, (shader, keys)) in drawn.into_iter().enumerate() {
            // `material_group` binds through it, made pipelines or not.
            let layout = self.texture_layout(device, shader.textures).1.clone();
            let missing: Vec<PipelineKey> = keys
                .iter()
                .copied()
                .filter(|key| !self.draw.contains_key(key))
                .collect();
            let mut made = Vec::with_capacity(missing.len());
            for key in missing {
                match self.make_pipeline(device, &shader, &layout, key) {
                    Ok(pipeline) => made.push((key, Arc::new(pipeline))),
                    Err(error) => {
                        let message = error.to_string();
                        if !matches!(error, wgpu::Error::OutOfMemory { .. }) {
                            self.refused.insert(key.stages(), message.clone());
                        }
                        return Err(format!("material {index}'s pipelines: {message}"));
                    }
                }
            }
            self.draw.extend(made);
            pipelines.extend(keys.iter().map(|key| Arc::clone(&self.draw[key])));
            shaders.push(shader);
        }
        Ok(Prepared { shaders, pipelines })
    }

    /// The pipeline of `key`, drawn with `shader` through `layout`, or the
    /// error the device reports making it.
    fn make_pipeline(
        &mut self,
        device: &wgpu::Device,
        shader: &CompiledShader,
        layout: &wgpu::PipelineLayout,
        key: PipelineKey,
    ) -> Result<wgpu::RenderPipeline, wgpu::Error> {
        #[cfg(test)]
        {
            self.pipelines_made += 1;
            if let Some(error) = self.refuse_pipeline.take() {
                return Err(error);
            }
        }
        let target = self.target;
        scoped(device, || {
            Self::draw_pipeline(device, shader, layout, key, target)
        })
    }

    /// The pipeline of `key`, drawn with `shader` through `layout` onto
    /// `target`.
    fn draw_pipeline(
        device: &wgpu::Device,
        shader: &CompiledShader,
        layout: &wgpu::PipelineLayout,
        key: PipelineKey,
        target: SceneTarget,
    ) -> wgpu::RenderPipeline {
        debug_assert_eq!(key.shader, shader.id);
        // The skinned vertices, then the geometry's texture coordinates.
        let skinned_attributes =
            wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4];
        let uv_attributes = wgpu::vertex_attr_array![3 => Float32x2];
        let vertex_buffers = [
            Some(wgpu::VertexBufferLayout {
                array_stride: SKINNED_STRIDE,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &skinned_attributes,
            }),
            Some(wgpu::VertexBufferLayout {
                array_stride: 8,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &uv_attributes,
            }),
        ];
        let (vertex_entry, fragment_entry) = key.entry_points();
        // A mask covers as many samples as there are
        // (`scene_material_entry.wgsl`).
        let samples = [("CELESTA_SAMPLES", f64::from(target.samples))];
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Celesta scene draw"),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: &shader.module,
                entry_point: Some(vertex_entry),
                compilation_options: Default::default(),
                buffers: &vertex_buffers,
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader.module,
                entry_point: Some(fragment_entry),
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants: &samples,
                    ..Default::default()
                },
                targets: &[Some(wgpu::ColorTargetState {
                    format: target.format,
                    blend: key.alpha.blend(),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: if key.facing.clockwise {
                    wgpu::FrontFace::Cw
                } else {
                    wgpu::FrontFace::Ccw
                },
                cull_mode: match key.facing.cull {
                    Cull::None => None,
                    Cull::Back => Some(wgpu::Face::Back),
                    Cull::Front => Some(wgpu::Face::Front),
                },
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(key.alpha.writes_depth()),
                // Unity's `ZTest LEqual`: of two coplanar draws, the later
                // in the queue shows.
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: key.depth_bias(),
            }),
            multisample: wgpu::MultisampleState {
                count: target.samples,
                ..Default::default()
            },
            multiview_mask: None,
            cache: None,
        })
    }

    /// `material`'s group 2 for `shader`, which it is drawn with: its
    /// textures, of `textures`, each with its own sampler, and an empty
    /// texel with the default sampler for each slot it leaves empty. A
    /// custom material's empty slots sample what `shaders`, the model's,
    /// say.
    fn material_group(
        &self,
        device: &wgpu::Device,
        textures: &[(wgpu::TextureView, wgpu::Sampler)],
        material: &Material,
        shader: &CompiledShader,
        shaders: &[SceneShader],
    ) -> wgpu::BindGroup {
        let bound = |texture: Option<usize>, empty: EmptyTexel| match texture {
            Some(texture) => (&textures[texture].0, &textures[texture].1),
            None => (
                match empty {
                    EmptyTexel::White => &self.white,
                    EmptyTexel::Black => &self.black,
                },
                &self.default_sampler,
            ),
        };
        let slots: Vec<_> = match &material.kind {
            MaterialKind::Unlit(unlit) => vec![bound(
                unlit
                    .base_color_texture
                    .as_ref()
                    .map(|texture| texture.texture),
                EmptyTexel::White,
            )],
            MaterialKind::Custom(custom) => shaders[custom.shader]
                .textures
                .iter()
                .enumerate()
                .map(|(slot, texture)| {
                    bound(custom.textures.get(slot).copied().flatten(), texture.empty)
                })
                .collect(),
        };
        debug_assert_eq!(slots.len(), shader.textures);
        let (layout, _) = &self.texture_layouts[&shader.textures];
        let entries: Vec<wgpu::BindGroupEntry> = slots
            .into_iter()
            .enumerate()
            .flat_map(|(slot, (view, sampler))| {
                [
                    wgpu::BindGroupEntry {
                        binding: 2 * slot as u32,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2 * slot as u32 + 1,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                ]
            })
            .collect();
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Celesta scene material"),
            layout,
            entries: &entries,
        })
    }
}

/// A filterable 2D texture at `binding` and its sampler at `binding + 1`,
/// read by both stages.
fn texture_entries(binding: u32) -> [wgpu::BindGroupLayoutEntry; 2] {
    let visibility = wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT;
    [
        wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: binding + 1,
            visibility,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        },
    ]
}

/// The model's shader `index` in a message: its name follows when it has
/// one.
fn shader_named(index: usize, shader: &SceneShader) -> String {
    match &shader.name {
        Some(name) => format!("shader {index} ({name})"),
        None => format!("shader {index}"),
    }
}

fn finite(values: &[f32]) -> bool {
    values.iter().all(|value| value.is_finite())
}

/// Why `model` cannot be drawn: a canvas `invalid_canvas` refuses, vertex
/// streams of different lengths, triangles outside their vertices, or a
/// reference to something it lacks, buffers past `SCENE_BUFFER_BUDGET`, or
/// shaders past the limits, more texture slots filled than a material's
/// shader has, an unlit material's base color or texture transform that is
/// not finite, or an outline depth bias that is not finite. A draw's skin
/// must have every joint its geometry's vertices name, and a draw whose
/// material requires tangents a geometry with tangents; all of these are
/// fixed, so they are checked here, once, rather than in every frame, and
/// before naga or the GPU reads any shader.
pub(crate) fn invalid_scene(model: &SceneModel) -> Option<String> {
    if let Some(message) = invalid_canvas("the model's", model.canvas) {
        return Some(message);
    }
    for (index, geometry) in model.geometries.iter().enumerate() {
        let vertices = geometry.positions.len();
        let streams = [
            ("normals", Some(geometry.normals.len())),
            ("uvs", Some(geometry.uvs.len())),
            ("tangents", geometry.tangents.as_ref().map(Vec::len)),
            ("joints", geometry.joints.as_ref().map(Vec::len)),
            ("weights", geometry.weights.as_ref().map(Vec::len)),
        ];
        for (name, length) in streams {
            if let Some(length) = length
                && length != vertices
            {
                return Some(format!(
                    "geometry {index} has {length} {name} for {vertices} vertices"
                ));
            }
        }
        if geometry.joints.is_some() != geometry.weights.is_some() {
            return Some(format!(
                "geometry {index} needs both joints and weights, or neither"
            ));
        }
        if geometry.indices.len() % 3 != 0
            || geometry
                .indices
                .iter()
                .any(|vertex| *vertex as usize >= vertices)
        {
            return Some(format!(
                "geometry {index} has triangles outside its {vertices} vertices"
            ));
        }
        for (target, offsets) in geometry.targets.iter().enumerate() {
            if offsets.positions.len() != vertices
                || !(offsets.normals.is_empty() || offsets.normals.len() == vertices)
            {
                return Some(format!(
                    "geometry {index}'s target {target} has {} positions and {} normals for {vertices} vertices",
                    offsets.positions.len(),
                    offsets.normals.len()
                ));
            }
        }
    }
    // Within the limits before naga reads any of them, used or not.
    let shaders = model.shaders.len();
    if shaders > MAX_SCENE_SHADERS {
        return Some(format!(
            "the scene has {shaders} shaders, more than {MAX_SCENE_SHADERS}"
        ));
    }
    for (index, shader) in model.shaders.iter().enumerate() {
        let named = shader_named(index, shader);
        if shader.wgsl.len() > MAX_SHADER_BYTES {
            return Some(format!(
                "{named} is {} bytes of WGSL, more than {MAX_SHADER_BYTES}",
                shader.wgsl.len()
            ));
        }
        if shader.textures.len() > MAX_MATERIAL_TEXTURES {
            return Some(format!(
                "{named} has {} texture slots, more than {MAX_MATERIAL_TEXTURES}",
                shader.textures.len()
            ));
        }
    }
    for (index, material) in model.materials.iter().enumerate() {
        if let AlphaMode::Mask { cutoff } = material.alpha_mode
            && !cutoff.is_finite()
        {
            return Some(format!("material {index}'s alphaCutoff is {cutoff}"));
        }
        let textures = model.textures.len();
        match &material.kind {
            MaterialKind::Unlit(unlit) => {
                if !finite(&unlit.base_color) {
                    return Some(format!("material {index}'s base color is not finite"));
                }
                if let Some(texture) = &unlit.base_color_texture {
                    if texture.texture >= textures {
                        return Some(format!(
                            "material {index} uses texture {}, of {textures}",
                            texture.texture
                        ));
                    }
                    if let Some(transform) = &texture.transform
                        && !finite(&[
                            transform.offset[0],
                            transform.offset[1],
                            transform.rotation,
                            transform.scale[0],
                            transform.scale[1],
                        ])
                    {
                        return Some(format!(
                            "material {index}'s texture transform is not finite"
                        ));
                    }
                }
            }
            MaterialKind::Custom(custom) => {
                let shaders = model.shaders.len();
                let Some(shader) = model.shaders.get(custom.shader) else {
                    return Some(format!(
                        "material {index} uses shader {}, of {shaders}",
                        custom.shader
                    ));
                };
                if custom.textures.len() > shader.textures.len() {
                    return Some(format!(
                        "material {index} fills {} texture slots, but shader {} has {}",
                        custom.textures.len(),
                        custom.shader,
                        shader.textures.len()
                    ));
                }
                for (slot, (texture, declared)) in
                    custom.textures.iter().zip(&shader.textures).enumerate()
                {
                    if let Some(texture) = *texture
                        && texture >= textures
                    {
                        let named = match &declared.name {
                            Some(name) => format!("texture slot {slot} ({name})"),
                            None => format!("texture slot {slot}"),
                        };
                        return Some(format!(
                            "material {index}'s {named} uses texture {texture}, of {textures}"
                        ));
                    }
                }
                if let Some(outline) = &custom.outline
                    && !outline.depth_bias.slope_scale.is_finite()
                {
                    return Some(format!(
                        "material {index}'s outline depth bias is not finite"
                    ));
                }
            }
        }
    }
    // The highest joint each geometry's vertices name.
    let highest_joints: Vec<Option<u16>> = model
        .geometries
        .iter()
        .map(|geometry| {
            geometry
                .joints
                .as_ref()
                .and_then(|joints| joints.as_flattened().iter().max().copied())
        })
        .collect();
    for (index, draw) in model.draws.iter().enumerate() {
        let Some(geometry) = model.geometries.get(draw.geometry) else {
            return Some(format!(
                "draw {index} uses geometry {}, of {}",
                draw.geometry,
                model.geometries.len()
            ));
        };
        if draw.material >= model.materials.len() {
            return Some(format!(
                "draw {index} uses material {}, of {}",
                draw.material,
                model.materials.len()
            ));
        }
        // A custom material that requires them needs the geometry's
        // tangents: none are made up.
        if let MaterialKind::Custom(custom) = &model.materials[draw.material].kind
            && custom.requires_tangents
            && geometry.tangents.is_none()
        {
            return Some(format!(
                "draw {index}'s material {} requires tangents, but geometry {} has none",
                draw.material, draw.geometry
            ));
        }
        match (draw.skin, geometry.joints.is_some()) {
            (None, false) => {}
            (None, true) => {
                return Some(format!(
                    "draw {index} has no skin for skinned geometry {}",
                    draw.geometry
                ));
            }
            (Some(skin), false) => {
                return Some(format!(
                    "draw {index} has skin {skin}, but geometry {} is not skinned",
                    draw.geometry
                ));
            }
            (Some(skin), true) => {
                let Some(joints) = model.skins.get(skin).map(|skin| skin.joints) else {
                    return Some(format!(
                        "draw {index} uses skin {skin}, of {}",
                        model.skins.len()
                    ));
                };
                if let Some(highest) = highest_joints[draw.geometry]
                    && u32::from(highest) >= joints
                {
                    return Some(format!(
                        "draw {index}'s geometry {} uses joint {highest}, of skin {skin}'s {joints}",
                        draw.geometry
                    ));
                }
            }
        }
    }
    let bytes = scene_buffer_bytes(model);
    if bytes > SCENE_BUFFER_BUDGET {
        return Some(format!(
            "the scene's geometries and draws take {bytes} bytes of GPU buffers, more than the {SCENE_BUFFER_BUDGET} allowed"
        ));
    }
    None
}

/// The bytes of the buffers `LoadedScene::load` makes for `model`, whose
/// draws name geometries it has: each geometry's streams, morph targets,
/// and triangles, and each draw's skinned vertices. An empty stream still
/// takes a 32-byte element. Saturates rather than overflowing.
fn scene_buffer_bytes(model: &SceneModel) -> u64 {
    let stream = |elements: usize, size: u64| {
        (elements as u64)
            .saturating_mul(size)
            .max(SKIN_VERTEX_STRIDE)
    };
    let mut total = 0_u64;
    for geometry in &model.geometries {
        let vertices = geometry.positions.len();
        let morphs = vertices.saturating_mul(geometry.targets.len());
        let mut streams = vec![
            stream(vertices, 16),
            stream(vertices, 16),
            stream(vertices, 8),
            stream(geometry.indices.len(), 4),
        ];
        if geometry.tangents.is_some() {
            streams.push(stream(vertices, 16));
        }
        if geometry.joints.is_some() {
            streams.push(stream(vertices, SKIN_VERTEX_STRIDE));
        }
        if !geometry.targets.is_empty() {
            streams.push(stream(morphs, 16));
        }
        if geometry
            .targets
            .iter()
            .any(|target| !target.normals.is_empty())
        {
            streams.push(stream(morphs, 16));
        }
        for bytes in streams {
            total = total.saturating_add(bytes);
        }
    }
    for draw in &model.draws {
        let vertices = model.geometries[draw.geometry].positions.len().max(1);
        total = total.saturating_add((vertices as u64).saturating_mul(SKINNED_STRIDE));
    }
    total
}

/// Why the uniforms of a frame of `draws` queued draws cannot be written:
/// one `alignment`-aligned skinning parameter block and one draw slot
/// (`draw_slot`) per draw, in buffers no larger than `largest` bytes.
fn oversized_uniforms(draws: usize, alignment: u32, largest: u64) -> Option<String> {
    let buffers = [
        (
            "skinning parameters",
            aligned(std::mem::size_of::<SkinParams>(), alignment),
        ),
        ("draw uniforms", draw_slot(alignment).1),
    ];
    for (name, size) in buffers {
        let bytes = (draws as u64).saturating_mul(size as u64);
        if bytes > largest {
            return Some(format!(
                "the frame's {draws} draws' {name} take {bytes} bytes, more than this GPU's {largest}"
            ));
        }
    }
    None
}

/// Why `camera` cannot see anything: a lens without depth or angle, or a
/// view that is projective or cannot be undone.
fn invalid_camera(camera: &Camera) -> Option<String> {
    if !finite(&camera.view) {
        return Some(format!("the camera's view {:?} is not finite", camera.view));
    }
    let (near, far) = match camera.projection {
        Projection::Perspective { yfov, near, far } => {
            if !(yfov > 0.0 && yfov < std::f32::consts::PI) {
                return Some(format!("the camera's yfov is {yfov}, not between 0 and π"));
            }
            (near, far)
        }
        Projection::Orthographic { height, near, far } => {
            if !(height.is_finite() && height > 0.0) {
                return Some(format!("the camera's height is {height}"));
            }
            (near, far)
        }
    };
    if !(near > 0.0 && near < far && far.is_finite()) {
        return Some(format!(
            "the camera's near is {near} and far {far}, not 0 < near < far"
        ));
    }
    // A view is affine: a projective one has no single eye, and
    // `camera_position` would put it in the wrong place.
    let view = &camera.view;
    if view[3] != 0.0 || view[7] != 0.0 || view[11] != 0.0 || view[15] != 1.0 {
        return Some(format!(
            "the camera's view has bottom row {:?}, not (0, 0, 0, 1)",
            [view[3], view[7], view[11], view[15]]
        ));
    }
    // A view is normally rigid, its determinant ±1; one of about zero has
    // no inverse worth the name.
    if determinant(view).abs() <= f64::EPSILON {
        return Some("the camera's view is not invertible".to_owned());
    }
    // The shader reads the eye as `f32`: a view that shrinks the world and
    // moves it far away can put it past `f32`'s range.
    let eye = camera_position(&camera.view);
    if !eye.iter().all(|&value| (value as f32).is_finite()) {
        return Some(format!(
            "the camera's view puts the eye at {eye:?}, beyond f32's range"
        ));
    }
    None
}

/// The determinant of a 4×4 matrix, by the 2×2 minors of its first two
/// rows and its last two.
fn determinant(m: &[f32; 16]) -> f64 {
    // Column-major: row `r`, column `c` is `m[c * 4 + r]`.
    let a = |r: usize, c: usize| f64::from(m[c * 4 + r]);
    let low = |r: usize, j: usize, k: usize| a(r, j) * a(r + 1, k) - a(r + 1, j) * a(r, k);
    low(0, 0, 1) * low(2, 2, 3) - low(0, 0, 2) * low(2, 1, 3)
        + low(0, 0, 3) * low(2, 1, 2)
        + low(0, 1, 2) * low(2, 0, 3)
        - low(0, 1, 3) * low(2, 0, 2)
        + low(0, 2, 3) * low(2, 0, 1)
}

/// Why `frame` does not fit `model` (which `invalid_scene` passed), whose
/// materials are drawn with `shaders`, one each: a canvas of its own that
/// `invalid_frame_canvas` refuses, a draw it lacks or lists twice, morph
/// weights or skinning matrices that do not match, parameters for a
/// built-in material or of a length its shader's `Params` is not,
/// overrides for a custom material, a number that is not finite, or a
/// camera that sees nothing.
pub(crate) fn invalid_scene_frame(
    model: &SceneModel,
    shaders: &[Arc<CompiledShader>],
    frame: &SceneFrame,
) -> Option<String> {
    if let Some(message) = invalid_frame_canvas(frame) {
        return Some(message);
    }
    if let Some(message) = invalid_camera(&frame.camera) {
        return Some(message);
    }
    if !frame.time.is_finite() {
        return Some(format!("the frame's time is {}", frame.time));
    }
    if !finite(frame.lights.ambient.as_flattened()) {
        return Some("the ambient light is not finite".to_owned());
    }
    let lights = frame.lights.directional.len();
    if lights > MAX_DIRECTIONAL_LIGHTS {
        return Some(format!(
            "the frame has {lights} directional lights, more than {MAX_DIRECTIONAL_LIGHTS}"
        ));
    }
    for (index, light) in frame.lights.directional.iter().enumerate() {
        // The color times the intensity too, which the shader reads.
        let color = light.color.map(|value| value * light.intensity);
        if !(finite(&light.direction) && finite(&color) && light.intensity.is_finite()) {
            return Some(format!("directional light {index} is not finite"));
        }
        if to_light(light.direction).is_none() {
            return Some(format!("directional light {index} has no direction"));
        }
    }
    if frame.skins.len() != model.skins.len() {
        return Some(format!(
            "the frame has {} skins for the model's {}",
            frame.skins.len(),
            model.skins.len()
        ));
    }
    for (index, (matrices, skin)) in frame.skins.iter().zip(&model.skins).enumerate() {
        if matrices.len() != skin.joints as usize {
            return Some(format!(
                "skin {index} has {} matrices for {} joints",
                matrices.len(),
                skin.joints
            ));
        }
        if !matrices.iter().all(|matrix| finite(matrix)) {
            return Some(format!("skin {index} has a matrix that is not finite"));
        }
    }
    let mut listed = vec![false; model.draws.len()];
    for state in &frame.draws {
        let index = state.index;
        let Some(info) = model.draws.get(index) else {
            return Some(format!(
                "the frame lists draw {index}, of {}",
                model.draws.len()
            ));
        };
        if std::mem::replace(&mut listed[index], true) {
            return Some(format!("the frame lists draw {index} twice"));
        }
        let material = info.material;
        match &model.materials[material].kind {
            MaterialKind::Custom(custom) => {
                if state.overrides != MaterialOverrides::default() {
                    return Some(format!(
                        "draw {index}'s material {material} is custom: overrides are for unlit materials"
                    ));
                }
                let size = shaders[material].params_size;
                if let Some(params) = &state.params
                    && params.len() != size as usize
                {
                    return Some(format!(
                        "draw {index}'s parameters are {} bytes, but its material {material}'s shader {} takes {size}",
                        params.len(),
                        custom.shader
                    ));
                }
            }
            MaterialKind::Unlit(_) if state.params.is_some() => {
                return Some(format!(
                    "draw {index}'s material {material} is unlit, which takes no parameters"
                ));
            }
            MaterialKind::Unlit(_) => {}
        }
        let targets = model.geometries[info.geometry].targets.len();
        if state.morph_weights.len() != targets {
            return Some(format!(
                "draw {index} has {} morph weights for {targets} targets",
                state.morph_weights.len()
            ));
        }
        let overrides = &state.overrides;
        let numbers = [
            ("world", Some(&state.world[..])),
            ("morph weights", Some(&state.morph_weights[..])),
            ("center", state.center.as_ref().map(|center| &center[..])),
            (
                "base color override",
                overrides.base_color.as_ref().map(|color| &color[..]),
            ),
            (
                "texture offset override",
                overrides.texture_offset.as_ref().map(|offset| &offset[..]),
            ),
            (
                "texture scale override",
                overrides.texture_scale.as_ref().map(|scale| &scale[..]),
            ),
        ];
        for (name, values) in numbers {
            if let Some(values) = values
                && !finite(values)
            {
                return Some(format!("draw {index}'s {name} is not finite"));
            }
        }
    }
    None
}

/// Straight-alpha RGBA mip levels from the image down to 1x1, each texel
/// the box average of the 2x2 above it. sRGB colors average in linear light;
/// alpha, and linear data, as they are.
fn straight_mips(
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    color_space: ColorSpace,
) -> Vec<(u32, u32, Vec<u8>)> {
    let decode: [f32; 256] = std::array::from_fn(|value| {
        let value = value as f32 / 255.0;
        match color_space {
            ColorSpace::Srgb if value <= 0.04045 => value / 12.92,
            ColorSpace::Srgb => ((value + 0.055) / 1.055).powf(2.4),
            ColorSpace::Linear => value,
        }
    });
    let encode = |value: f32| {
        let value = match color_space {
            ColorSpace::Srgb if value <= 0.003_130_8 => value * 12.92,
            ColorSpace::Srgb => 1.055 * value.powf(1.0 / 2.4) - 0.055,
            ColorSpace::Linear => value,
        };
        (value * 255.0).round().clamp(0.0, 255.0) as u8
    };
    let mut levels = vec![(width, height, pixels)];
    loop {
        let (width, height, level) = levels.last().expect("starts with the image");
        let (width, height) = (*width, *height);
        if width == 1 && height == 1 {
            return levels;
        }
        let next_width = (width / 2).max(1);
        let next_height = (height / 2).max(1);
        let mut next = Vec::with_capacity((next_width * next_height * 4) as usize);
        for y in 0..next_height {
            for x in 0..next_width {
                let mut sum = [0.0_f32; 4];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let sx = (x * 2 + dx).min(width - 1);
                    let sy = (y * 2 + dy).min(height - 1);
                    let texel = &level[((sy * width + sx) * 4) as usize..][..4];
                    for channel in 0..3 {
                        sum[channel] += decode[usize::from(texel[channel])];
                    }
                    sum[3] += f32::from(texel[3]);
                }
                next.extend([
                    encode(sum[0] / 4.0),
                    encode(sum[1] / 4.0),
                    encode(sum[2] / 4.0),
                    (sum[3] / 4.0).round() as u8,
                ]);
            }
        }
        levels.push((next_width, next_height, next));
    }
}

/// The midpoint of `positions`' bounding box; the origin when there are
/// none.
fn bounds_center(positions: &[[f32; 3]]) -> [f32; 3] {
    let Some(first) = positions.first() else {
        return [0.0; 3];
    };
    let (low, high) = positions
        .iter()
        .fold((*first, *first), |(low, high), position| {
            (
                std::array::from_fn(|axis| low[axis].min(position[axis])),
                std::array::from_fn(|axis| high[axis].max(position[axis])),
            )
        });
    std::array::from_fn(|axis| (low[axis] + high[axis]) * 0.5)
}

fn address_mode(wrap: Wrap) -> wgpu::AddressMode {
    match wrap {
        Wrap::Repeat => wgpu::AddressMode::Repeat,
        Wrap::MirroredRepeat => wgpu::AddressMode::MirrorRepeat,
        Wrap::ClampToEdge => wgpu::AddressMode::ClampToEdge,
    }
}

fn filter_mode(filter: Filter) -> wgpu::FilterMode {
    match filter {
        Filter::Nearest => wgpu::FilterMode::Nearest,
        Filter::Linear => wgpu::FilterMode::Linear,
    }
}

impl LoadedScene {
    /// Checks `model`, makes the pipelines its materials draw with, uploads
    /// its geometries and textures, binds each material's textures, and
    /// makes what each draw is skinned into.
    pub(crate) fn load(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pipelines: &mut ScenePipelines,
        max_texture_dimension: u32,
        id: ModelId,
        model: SceneModel,
        layer: &str,
    ) -> Result<Self, GpuRenderError> {
        let invalid = |message| GpuRenderError::InvalidPluginContent {
            layer: layer.to_owned(),
            message,
        };
        if let Some(message) = invalid_scene(&model) {
            return Err(invalid(message));
        }
        let prepared = pipelines.prepare(device, &model).map_err(invalid)?;
        let pipelines = &*pipelines;
        let limits = device.limits();
        let largest_buffer = limits
            .max_buffer_size
            .min(limits.max_storage_buffer_binding_size);
        let mut geometries = Vec::new();
        for (index, geometry) in model.geometries.iter().enumerate() {
            let buffer = |name: &str, contents: &[u8], usage| {
                if contents.len() as u64 > largest_buffer {
                    return Err(invalid(format!(
                        "geometry {index}'s {name} take {} bytes, more than this GPU's {largest_buffer}",
                        contents.len()
                    )));
                }
                Ok(
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Celesta scene geometry"),
                        contents: if contents.is_empty() {
                            &[0; SKIN_VERTEX_STRIDE as usize]
                        } else {
                            contents
                        },
                        usage,
                    }),
                )
            };
            let stream = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::VERTEX;
            let vec4 = |values: &[[f32; 3]], w: f32| -> Vec<f32> {
                values
                    .iter()
                    .flat_map(|value| [value[0], value[1], value[2], w])
                    .collect()
            };
            let vertices = geometry.positions.len();
            let morph_positions: Vec<f32> = geometry
                .targets
                .iter()
                .flat_map(|target| vec4(&target.positions, 0.0))
                .collect();
            let morph_normals: Option<Vec<f32>> = geometry
                .targets
                .iter()
                .any(|target| !target.normals.is_empty())
                .then(|| {
                    geometry
                        .targets
                        .iter()
                        .flat_map(|target| {
                            if target.normals.is_empty() {
                                vec![0.0; vertices * 4]
                            } else {
                                vec4(&target.normals, 0.0)
                            }
                        })
                        .collect()
                });
            // Each vertex's four joints, then the bits of their four weights.
            let skin: Option<Vec<u32>> = geometry
                .joints
                .as_ref()
                .zip(geometry.weights.as_ref())
                .map(|(joints, weights)| {
                    joints
                        .iter()
                        .zip(weights)
                        .flat_map(|(joints, weights)| {
                            let joints = joints.map(u32::from);
                            let weights = weights.map(f32::to_bits);
                            joints.into_iter().chain(weights)
                        })
                        .collect()
                });
            if vertices as u64 * SKINNED_STRIDE > largest_buffer {
                return Err(invalid(format!(
                    "geometry {index}'s {vertices} vertices take {} bytes skinned, more than this GPU's {largest_buffer}",
                    vertices as u64 * SKINNED_STRIDE
                )));
            }
            geometries.push(GeometryBuffers {
                vertices: vertices as u32,
                center: bounds_center(&geometry.positions),
                positions: buffer(
                    "positions",
                    bytemuck::cast_slice(&vec4(&geometry.positions, 1.0)),
                    stream,
                )?,
                normals: buffer(
                    "normals",
                    bytemuck::cast_slice(&vec4(&geometry.normals, 0.0)),
                    stream,
                )?,
                uvs: buffer(
                    "uvs",
                    bytemuck::cast_slice(geometry.uvs.as_flattened()),
                    stream,
                )?,
                tangents: geometry
                    .tangents
                    .as_ref()
                    .map(|tangents| {
                        buffer(
                            "tangents",
                            bytemuck::cast_slice(tangents.as_flattened()),
                            stream,
                        )
                    })
                    .transpose()?,
                skin: skin
                    .map(|skin| buffer("joints and weights", bytemuck::cast_slice(&skin), stream))
                    .transpose()?,
                morph_positions: (!geometry.targets.is_empty())
                    .then(|| {
                        buffer(
                            "morph targets",
                            bytemuck::cast_slice(&morph_positions),
                            stream,
                        )
                    })
                    .transpose()?,
                morph_normals: morph_normals
                    .map(|normals| {
                        buffer(
                            "morph target normals",
                            bytemuck::cast_slice(&normals),
                            stream,
                        )
                    })
                    .transpose()?,
                indices: buffer(
                    "indices",
                    bytemuck::cast_slice(&geometry.indices),
                    wgpu::BufferUsages::INDEX | wgpu::BufferUsages::STORAGE,
                )?,
                index_count: geometry.indices.len() as u32,
            });
        }

        let mut textures = Vec::new();
        for (index, texture) in model.textures.iter().enumerate() {
            let image = texture_image(&texture.source, layer)?;
            let (width, height) = image.dimensions();
            if width == 0 || height == 0 || width.max(height) > max_texture_dimension {
                return Err(invalid(format!(
                    "texture {index} is {width}x{height}, outside 1 to {max_texture_dimension} texels a side"
                )));
            }
            let sampler = &texture.sampler;
            let levels = if sampler.mipmaps {
                straight_mips(width, height, image.into_raw(), texture.color_space)
            } else {
                vec![(width, height, image.into_raw())]
            };
            let data: Vec<u8> = levels
                .iter()
                .flat_map(|(_, _, pixels)| pixels.iter().copied())
                .collect();
            let uploaded = device.create_texture_with_data(
                queue,
                &wgpu::TextureDescriptor {
                    label: Some("Celesta scene texture"),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: levels.len() as u32,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: match texture.color_space {
                        ColorSpace::Srgb => wgpu::TextureFormat::Rgba8UnormSrgb,
                        ColorSpace::Linear => wgpu::TextureFormat::Rgba8Unorm,
                    },
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                },
                wgpu::util::TextureDataOrder::LayerMajor,
                &data,
            );
            textures.push((
                uploaded.create_view(&Default::default()),
                device.create_sampler(&wgpu::SamplerDescriptor {
                    label: Some("Celesta scene sampler"),
                    address_mode_u: address_mode(sampler.wrap_s),
                    address_mode_v: address_mode(sampler.wrap_t),
                    mag_filter: filter_mode(sampler.mag_filter),
                    min_filter: filter_mode(sampler.min_filter),
                    mipmap_filter: if sampler.mipmaps {
                        wgpu::MipmapFilterMode::Linear
                    } else {
                        wgpu::MipmapFilterMode::Nearest
                    },
                    ..Default::default()
                }),
            ));
        }
        let materials = model
            .materials
            .iter()
            .zip(&prepared.shaders)
            .map(|(material, shader)| {
                pipelines.material_group(device, &textures, material, shader, &model.shaders)
            })
            .collect();

        let draws = model
            .draws
            .iter()
            .map(|draw| {
                let geometry = &geometries[draw.geometry];
                let skinned = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Celesta scene skinned vertices"),
                    size: u64::from(geometry.vertices.max(1)) * SKINNED_STRIDE,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::VERTEX,
                    mapped_at_creation: false,
                });
                let or_empty = |stream: &Option<wgpu::Buffer>| {
                    stream.as_ref().unwrap_or(&pipelines.empty).clone()
                };
                let (tangents, skin, morph_positions, morph_normals) = (
                    or_empty(&geometry.tangents),
                    or_empty(&geometry.skin),
                    or_empty(&geometry.morph_positions),
                    or_empty(&geometry.morph_normals),
                );
                let streams = [
                    geometry.positions.as_entire_binding(),
                    geometry.normals.as_entire_binding(),
                    tangents.as_entire_binding(),
                    skin.as_entire_binding(),
                    morph_positions.as_entire_binding(),
                    morph_normals.as_entire_binding(),
                    skinned.as_entire_binding(),
                ];
                let entries: Vec<wgpu::BindGroupEntry> = streams
                    .into_iter()
                    .enumerate()
                    .map(|(binding, resource)| wgpu::BindGroupEntry {
                        binding: binding as u32,
                        resource,
                    })
                    .collect();
                let skin_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Celesta scene geometry"),
                    layout: &pipelines.geometry_layout,
                    entries: &entries,
                });
                DrawBuffers {
                    skinned,
                    skin_group,
                }
            })
            .collect();
        Ok(Self {
            id,
            last_canvas: model.canvas,
            model,
            geometries,
            materials,
            shaders: prepared.shaders,
            _pipelines: prepared.pipelines,
            draws,
        })
    }
}

/// `a × b`, both column-major.
fn multiply(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    std::array::from_fn(|index| {
        let (column, row) = (index / 4, index % 4);
        (0..4).map(|k| a[k * 4 + row] * b[column * 4 + k]).sum()
    })
}

/// The projection of `projection` onto a canvas `aspect` times as wide as
/// it is high: right-handed, the camera looking down −Z, with depth 0 at
/// `near` and 1 at `far`. An orthographic camera sees `height` metres.
fn projection_matrix(projection: &Projection, aspect: f32) -> [f32; 16] {
    match *projection {
        Projection::Perspective { yfov, near, far } => {
            let y = 1.0 / (yfov * 0.5).tan();
            let x = y / aspect;
            let z = far / (near - far);
            [
                x,
                0.0,
                0.0,
                0.0, //
                0.0,
                y,
                0.0,
                0.0, //
                0.0,
                0.0,
                z,
                -1.0, //
                0.0,
                0.0,
                z * near,
                0.0,
            ]
        }
        Projection::Orthographic { height, near, far } => {
            let y = 2.0 / height;
            let x = y / aspect;
            let z = 1.0 / (near - far);
            [
                x,
                0.0,
                0.0,
                0.0, //
                0.0,
                y,
                0.0,
                0.0, //
                0.0,
                0.0,
                z,
                0.0, //
                0.0,
                0.0,
                z * near,
                1.0,
            ]
        }
    }
}

/// `L` for a light travelling along `direction`: `−normalize(direction)`,
/// worked out in `f64`; `None` when `direction` has no length.
fn to_light(direction: [f32; 3]) -> Option<[f32; 3]> {
    let direction = direction.map(f64::from);
    let length = direction
        .iter()
        .map(|value| value * value)
        .sum::<f64>()
        .sqrt();
    (length > 0.0 && length.is_finite()).then(|| direction.map(|value| (-value / length) as f32))
}

/// The cofactor matrix of `matrix`'s upper 3×3 as columns, and its
/// determinant, in `f64`. The inverse is the cofactor's transpose over the
/// determinant.
fn cofactor(matrix: &[f32; 16]) -> ([[f64; 3]; 3], f64) {
    let column = |c: usize| [0, 1, 2].map(|r| f64::from(matrix[c * 4 + r]));
    let cross = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let (a, b, c) = (column(0), column(1), column(2));
    // The cofactor matrix's columns are the cross products of the others.
    let cofactor = [cross(b, c), cross(c, a), cross(a, b)];
    let determinant = (0..3).map(|row| a[row] * cofactor[0][row]).sum();
    (cofactor, determinant)
}

/// −1 when `matrix` mirrors (its upper 3×3's determinant is negative), else
/// 1, a determinant of 0 included. Of a world matrix, the sign
/// `normal_matrix` takes, and the draw's handedness, which a material's
/// bitangents take (Unity's `unity_WorldTransformParams.w`); of a world
/// matrix and the view, which way a draw's front faces turn on screen.
fn handedness(matrix: &[f32; 16]) -> f64 {
    if cofactor(matrix).1 < 0.0 { -1.0 } else { 1.0 }
}

/// What takes `world`'s normals to world space: the cofactor matrix of its
/// upper 3×3, times the sign of its determinant (+1 for a determinant of
/// 0), as columns. That is the inverse transpose times the determinant's
/// magnitude: the same directions, which the shader normalises, and finite
/// even when `world` scales to nothing. Worked out in `f64` and divided by
/// its largest entry's magnitude, so that it stays finite as `f32` however
/// large `world` is.
fn normal_matrix(world: &[f32; 16]) -> [[f32; 4]; 3] {
    let (cofactor, _) = cofactor(world);
    let sign = handedness(world);
    let largest = cofactor
        .as_flattened()
        .iter()
        .fold(0.0, |largest: f64, value| largest.max(value.abs()));
    let scale = if largest > 0.0 { sign / largest } else { sign };
    cofactor.map(|column| {
        let [x, y, z] = column.map(|value| (value * scale) as f32);
        [x, y, z, 0.0]
    })
}

/// Where the camera of `view` sits in world space. With the view's upper
/// 3×3 `M` and translation `t`, that is `−M⁻¹t`, which the view takes to
/// the origin, whatever scale the view has. `view` is affine and
/// invertible, as `invalid_camera` checks, so `M` has an inverse.
fn camera_position(view: &[f32; 16]) -> [f64; 3] {
    let (cofactor, determinant) = cofactor(view);
    let t = [view[12], view[13], view[14]].map(f64::from);
    // `M⁻¹t`'s entry `k` is cofactor column `k` dotted with `t`.
    cofactor.map(|column| -(0..3).map(|r| column[r] * t[r]).sum::<f64>() / determinant)
}

/// What `frame` reads as `Frame` on a canvas `aspect` times as wide as it
/// is high, its lights checked by `invalid_scene_frame`.
fn frame_uniform(frame: &SceneFrame, aspect: f32) -> FrameUniform {
    let camera = &frame.camera;
    let projection = projection_matrix(&camera.projection, aspect);
    // The camera's +Z is the view's third row, in direction (which the
    // shader normalises) for a view of uniform scale.
    let eye = camera_position(&camera.view);
    let perspective = matches!(camera.projection, Projection::Perspective { .. });
    let mut lights = [bytemuck::Zeroable::zeroed(); MAX_DIRECTIONAL_LIGHTS];
    for (uniform, light) in lights.iter_mut().zip(&frame.lights.directional) {
        let [x, y, z] = to_light(light.direction).expect("checked by invalid_scene_frame");
        let [r, g, b] = light.color.map(|value| value * light.intensity);
        *uniform = LightUniform {
            to_light: [x, y, z, 0.0],
            color: [r, g, b, 0.0],
        };
    }
    FrameUniform {
        view_projection: multiply(&projection, &camera.view),
        view: camera.view,
        projection,
        eye: [
            eye[0] as f32,
            eye[1] as f32,
            eye[2] as f32,
            if perspective { 1.0 } else { 0.0 },
        ],
        forward: [camera.view[2], camera.view[6], camera.view[10], 0.0],
        time: frame.time as f32,
        height_over_width: 1.0 / aspect,
        light_count: frame.lights.directional.len() as u32,
        reserved: 0,
        lights,
        ambient: frame.lights.ambient.map(|[r, g, b]| [r, g, b, 0.0]),
    }
}

/// `matrix × (point, 1)`, `matrix` column-major.
fn transform_point(matrix: &[f32; 16], point: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|row| {
        (0..3)
            .map(|column| matrix[column * 4 + row] * point[column])
            .sum::<f32>()
            + matrix[12 + row]
    })
}

/// The first queue Unity sorts back to front: both kinds of blend.
const TRANSPARENT_QUEUES: u32 = 2501;

/// The draws of `frame` to draw, in Unity's order: by their material's
/// `render_queue`. Draws in the transparent queues go back to front, as
/// Unity sorts them, by their `center`, or else their geometry's rest
/// bounds centre moved by `world`: farthest from the camera first through
/// a perspective camera, lowest view-space z first through an orthographic
/// one. Ties go by draw index. Hidden draws, and draws without triangles,
/// are left out.
fn draw_queue<'a>(scene: &LoadedScene, frame: &'a SceneFrame) -> Vec<&'a DrawState> {
    let perspective = matches!(frame.camera.projection, Projection::Perspective { .. });
    let eye = camera_position(&frame.camera.view);
    let mut queue: Vec<(u32, f64, &DrawState)> = frame
        .draws
        .iter()
        .filter_map(|state| {
            let info = &scene.model.draws[state.index];
            let geometry = &scene.geometries[info.geometry];
            if !state.visible || geometry.index_count == 0 {
                return None;
            }
            let queue = render_queue(&scene.model.materials[info.material].alpha_mode);
            // Lower draws first.
            let order = if queue >= TRANSPARENT_QUEUES {
                let center = state
                    .center
                    .unwrap_or_else(|| transform_point(&state.world, geometry.center));
                if perspective {
                    -(0..3)
                        .map(|axis| (f64::from(center[axis]) - eye[axis]).powi(2))
                        .sum::<f64>()
                } else {
                    // The camera looks down −Z: the farther, the lower its z.
                    f64::from(transform_point(&frame.camera.view, center)[2])
                }
            } else {
                0.0
            };
            Some((queue, order, state))
        })
        .collect();
    queue.sort_by(|(a_queue, a_order, a), (b_queue, b_order, b)| {
        a_queue
            .cmp(b_queue)
            .then(a_order.total_cmp(b_order))
            .then(a.index.cmp(&b.index))
    });
    queue.into_iter().map(|(_, _, state)| state).collect()
}

/// One `size`-byte element of `buffer`, bound at a dynamic offset.
fn uniform_binding(buffer: &wgpu::Buffer, size: usize) -> wgpu::BindingResource<'_> {
    wgpu::BindingResource::Buffer(wgpu::BufferBinding {
        buffer,
        offset: 0,
        size: wgpu::BufferSize::new(size as u64),
    })
}

/// `size` rounded up to a multiple of `alignment`.
fn aligned(size: usize, alignment: u32) -> usize {
    size.next_multiple_of(alignment as usize)
}

/// Where a draw's parameters start in its slot of the draw buffer, and the
/// slot's bytes: its `DrawUniform` at the start, then `MAX_MATERIAL_PARAMS`
/// bytes of parameters, each at an `alignment`-aligned offset.
fn draw_slot(alignment: u32) -> (usize, usize) {
    let params = aligned(std::mem::size_of::<DrawUniform>(), alignment);
    (params, params + aligned(MAX_MATERIAL_PARAMS, alignment))
}

/// `values` laid out `stride` bytes apart.
fn strided<T: bytemuck::Pod>(values: &[T], stride: usize) -> Vec<u8> {
    let mut bytes = vec![0; values.len() * stride];
    for (index, value) in values.iter().enumerate() {
        let value = bytemuck::bytes_of(value);
        bytes[index * stride..][..value.len()].copy_from_slice(value);
    }
    bytes
}

/// What `state` draws its `material` with, besides the material's
/// parameters: its world matrix, normal matrix, and handedness, and a mask
/// material's cutoff.
fn draw_uniform(material: &Material, state: &DrawState) -> DrawUniform {
    DrawUniform {
        world: state.world,
        normal_matrix: normal_matrix(&state.world),
        handedness: handedness(&state.world) as f32,
        alpha_cutoff: match material.alpha_mode {
            AlphaMode::Mask { cutoff } => cutoff,
            AlphaMode::Opaque | AlphaMode::Blend { .. } => 0.0,
        },
        reserved: [0.0; 2],
    }
}

/// The parameters `state` draws its `material` with, padded with zeros to
/// `MAX_MATERIAL_PARAMS`: an unlit material's with `state`'s overrides, a
/// custom one's own or, for this frame only, `state`'s.
fn material_params(material: &Material, state: &DrawState) -> [u8; MAX_MATERIAL_PARAMS] {
    let mut slot = [0; MAX_MATERIAL_PARAMS];
    match &material.kind {
        MaterialKind::Unlit(unlit) => {
            slot[..UNLIT_PARAMS].copy_from_slice(&unlit_params(unlit, &state.overrides));
        }
        // As long as its shader's `Params`, which `prepare` checked for the
        // material's and `invalid_scene_frame` for the draw's.
        MaterialKind::Custom(custom) => {
            let params = state.params.as_deref().unwrap_or(&custom.params);
            slot[..params.len()].copy_from_slice(params);
        }
    }
    slot
}

/// The rows of `texture`'s transform, which scales, then rotates, then
/// offsets coordinates as `KHR_texture_transform`'s sample code does, with
/// the offset and scale `overrides` names in place of the texture's. A
/// rotation turns `(1, 0)` toward `(0, 1)`.
fn texture_rows(
    texture: Option<&TextureRef>,
    overrides: &MaterialOverrides,
) -> ([f32; 4], [f32; 4]) {
    let transform = texture.and_then(|texture| texture.transform);
    let (offset, rotation, scale) = transform.map_or(([0.0; 2], 0.0, [1.0; 2]), |transform| {
        (transform.offset, transform.rotation, transform.scale)
    });
    let offset = overrides.texture_offset.unwrap_or(offset);
    let scale = overrides.texture_scale.unwrap_or(scale);
    let (sin, cos) = rotation.sin_cos();
    (
        [cos * scale[0], -sin * scale[1], offset[0], 0.0],
        [sin * scale[0], cos * scale[1], offset[1], 0.0],
    )
}

/// `scene_unlit.wgsl`'s `Params` for `unlit`: its base color and its
/// texture's transform, with the values `overrides` names in place of the
/// material's.
fn unlit_params(unlit: &UnlitMaterial, overrides: &MaterialOverrides) -> [u8; UNLIT_PARAMS] {
    let (uv_u, uv_v) = texture_rows(unlit.base_color_texture.as_ref(), overrides);
    bytemuck::cast(UnlitParams {
        base_color: overrides.base_color.unwrap_or(unlit.base_color),
        uv_u,
        uv_v,
    })
}

impl GpuRenderer {
    /// `frame` of `scene` on a `width` by `height` canvas, once the frame is
    /// checked against the model, drawn with and into `resources`, where
    /// the caller has marked that size used.
    pub(crate) fn scene_texture(
        &self,
        layer: &str,
        scene: &LoadedScene,
        frame: &SceneFrame,
        width: u32,
        height: u32,
        resources: &mut SceneFrameResources,
    ) -> Result<LayerTexture, GpuRenderError> {
        let invalid = |message| GpuRenderError::InvalidPluginContent {
            layer: layer.to_owned(),
            message,
        };
        if let Some(message) = invalid_scene_frame(&scene.model, &scene.shaders, frame) {
            return Err(invalid(message));
        }
        let output = self.plugin_canvas(width, height);
        let queue = draw_queue(scene, frame);
        if queue.is_empty() {
            return Ok(self.plugin_layer_texture(output, width, height));
        }
        let pipelines = self
            .meshes
            .scene_pipelines
            .as_ref()
            .expect("created when the scene was opened");
        let model = &scene.model;
        let limits = self.device.limits();
        let largest_buffer = limits
            .max_buffer_size
            .min(limits.max_storage_buffer_binding_size);
        let alignment = limits.min_uniform_buffer_offset_alignment;
        if let Some(message) = oversized_uniforms(queue.len(), alignment, largest_buffer) {
            return Err(invalid(message));
        }

        // Every skin's matrices, then each queued draw's morph pairs.
        let mut frame_data: Vec<[u32; 4]> = Vec::new();
        let mut first_matrix = Vec::with_capacity(frame.skins.len());
        for matrices in &frame.skins {
            first_matrix.push(frame_data.len() as u32);
            for matrix in matrices {
                for column in matrix.as_chunks::<4>().0 {
                    frame_data.push(column.map(f32::to_bits));
                }
            }
        }
        let params_stride = aligned(std::mem::size_of::<SkinParams>(), alignment);
        let (params_at, draw_stride) = draw_slot(alignment);
        let mut skin_params = Vec::with_capacity(queue.len());
        // Every byte of every slot is written: a slot shows nothing of an
        // earlier frame or another draw.
        let mut draw_slots = vec![0; queue.len() * draw_stride];
        for (state, slot) in queue.iter().zip(draw_slots.chunks_exact_mut(draw_stride)) {
            let info = &model.draws[state.index];
            let geometry = &scene.geometries[info.geometry];
            let first_morph = frame_data.len() as u32;
            for (morph, weight) in state.morph_weights.iter().enumerate() {
                if *weight != 0.0 {
                    frame_data.push([morph as u32, weight.to_bits(), 0, 0]);
                }
            }
            skin_params.push(SkinParams {
                counts: [
                    geometry.vertices,
                    info.skin.map_or(0, |skin| first_matrix[skin]),
                    first_morph,
                    frame_data.len() as u32 - first_morph,
                ],
                flags: [
                    u32::from(info.skin.is_some()),
                    u32::from(geometry.tangents.is_some()),
                    u32::from(geometry.morph_normals.is_some()),
                    0,
                ],
            });
            let material = &model.materials[info.material];
            let uniform = draw_uniform(material, state);
            let params = material_params(material, state);
            debug_assert!(
                params[scene.shaders[info.material].params_size as usize..]
                    .iter()
                    .all(|&byte| byte == 0),
                "material {}'s parameters outgrow its shader's",
                info.material
            );
            slot[..std::mem::size_of::<DrawUniform>()]
                .copy_from_slice(bytemuck::bytes_of(&uniform));
            slot[params_at..][..params.len()].copy_from_slice(&params);
        }
        if frame_data.is_empty() {
            frame_data.push([0; 4]);
        }
        let frame_bytes = std::mem::size_of_val(frame_data.as_slice()) as u64;
        if frame_bytes > largest_buffer {
            return Err(invalid(format!(
                "the frame's skinning matrices and morph weights take {frame_bytes} bytes, more than this GPU's {largest_buffer}"
            )));
        }

        let canvas = frame_canvas(model, frame);
        let frame_uniform = frame_uniform(frame, canvas[0] / canvas[1]);
        // Each queue write lands before this layer's commands, which are
        // submitted below, and after any earlier layer's.
        let SceneFrameResources {
            targets,
            target_texels,
            used,
            frame_data: frame_buffer,
            skin_params: params_buffer,
            draws: draw_buffer,
            view: view_buffer,
            groups,
            allocations,
        } = resources;
        let (device, gpu_queue) = (&self.device, &self.queue);
        let uniform = wgpu::BufferUsages::UNIFORM;
        let wrote = [
            frame_buffer.write(
                device,
                gpu_queue,
                wgpu::BufferUsages::STORAGE,
                bytemuck::cast_slice(&frame_data),
                largest_buffer,
                allocations,
            ),
            params_buffer.write(
                device,
                gpu_queue,
                uniform,
                &strided(&skin_params, params_stride),
                largest_buffer,
                allocations,
            ),
            draw_buffer.write(
                device,
                gpu_queue,
                uniform,
                &draw_slots,
                largest_buffer,
                allocations,
            ),
            view_buffer.write(
                device,
                gpu_queue,
                uniform,
                bytemuck::bytes_of(&frame_uniform),
                largest_buffer,
                allocations,
            ),
        ];
        if groups.is_none() || wrote.contains(&true) {
            *groups = Some(FrameGroups {
                frame: device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Celesta scene skinning frame"),
                    layout: &pipelines.frame_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: frame_buffer.buffer().as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: uniform_binding(
                                params_buffer.buffer(),
                                std::mem::size_of::<SkinParams>(),
                            ),
                        },
                    ],
                }),
                view: device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Celesta scene frame uniform"),
                    layout: &pipelines.view_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: view_buffer.buffer().as_entire_binding(),
                    }],
                }),
                draws: device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Celesta scene draws"),
                    layout: &pipelines.draw_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: uniform_binding(
                                draw_buffer.buffer(),
                                std::mem::size_of::<DrawUniform>(),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: uniform_binding(draw_buffer.buffer(), MAX_MATERIAL_PARAMS),
                        },
                    ],
                }),
            });
        }
        let FrameGroups {
            frame: frame_group,
            view: view_group,
            draws: draw_group,
        } = groups.as_ref().expect("made above");

        // The caller marked `size` used, so `make_room` keeps it.
        let size = (width, height);
        if !targets.contains_key(&size) {
            make_room(targets, used, size, *target_texels);
        }
        let SceneTargets {
            multisampled,
            depth,
            resolved,
            convert_group,
        } = &*targets.entry(size).or_insert_with(|| {
            let mut target = |label, format, sample_count, usage| {
                *allocations += 1;
                device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some(label),
                        size: wgpu::Extent3d {
                            width,
                            height,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count,
                        dimension: wgpu::TextureDimension::D2,
                        format,
                        usage,
                        view_formats: &[],
                    })
                    .create_view(&Default::default())
            };
            let SceneTarget { format, samples } = pipelines.target;
            let multisampled = (samples > 1).then(|| {
                target(
                    "Celesta scene color",
                    format,
                    samples,
                    wgpu::TextureUsages::RENDER_ATTACHMENT,
                )
            });
            // Drawn onto directly at 1 sample, else resolved into.
            let resolved = target(
                "Celesta scene resolved color",
                format,
                1,
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            );
            let depth = target(
                "Celesta scene depth",
                DEPTH_FORMAT,
                samples,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
            );
            let convert_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Celesta scene resolved color"),
                layout: &pipelines.convert_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&resolved),
                }],
            });
            SceneTargets {
                multisampled,
                depth,
                resolved,
                convert_group,
            }
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Celesta scene"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Celesta scene skinning"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipelines.skin);
            let most = limits.max_compute_workgroups_per_dimension;
            for (slot, state) in queue.iter().enumerate() {
                let vertices = skin_params[slot].counts[0];
                pass.set_bind_group(0, &scene.draws[state.index].skin_group, &[]);
                pass.set_bind_group(1, frame_group, &[(slot * params_stride) as u32]);
                // Rows of workgroups when one row cannot hold them.
                let groups = vertices.div_ceil(WORKGROUP);
                let columns = groups.min(most);
                pass.dispatch_workgroups(columns, groups.div_ceil(columns), 1);
            }
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Celesta scene draw"),
                color_attachments: &[Some(match multisampled {
                    Some(multisampled) => wgpu::RenderPassColorAttachment {
                        view: multisampled,
                        depth_slice: None,
                        resolve_target: Some(resolved),
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Discard,
                        },
                    },
                    None => wgpu::RenderPassColorAttachment {
                        view: resolved,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, view_group, &[]);
            // A mirroring view turns every front face clockwise on screen,
            // as a mirroring world turns its draw's; both together turn
            // them back.
            let view_mirrors = handedness(&frame.camera.view) < 0.0;
            for (slot, state) in queue.iter().enumerate() {
                let info = &model.draws[state.index];
                let geometry = &scene.geometries[info.geometry];
                let material = &model.materials[info.material];
                let clockwise = (handedness(&state.world) < 0.0) != view_mirrors;
                let pipeline = |key| {
                    pipelines
                        .draw
                        .get(&key)
                        .expect("prepared when the model loaded")
                };
                let shader = &scene.shaders[info.material];
                pass.set_pipeline(pipeline(PipelineKey::surface(
                    material, shader.id, clockwise,
                )));
                let at = slot * draw_stride;
                pass.set_bind_group(1, draw_group, &[at as u32, (at + params_at) as u32]);
                pass.set_bind_group(2, &scene.materials[info.material], &[]);
                pass.set_vertex_buffer(0, scene.draws[state.index].skinned.slice(..));
                pass.set_vertex_buffer(1, geometry.uvs.slice(..));
                pass.set_index_buffer(geometry.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..geometry.index_count, 0, 0..1);
                // The outline follows in the same queue slot, from the same
                // skinned vertices, uniforms, parameters, and textures.
                if let Some(key) = PipelineKey::outline(material, shader, clockwise) {
                    pass.set_pipeline(pipeline(key));
                    pass.draw_indexed(0..geometry.index_count, 0, 0..1);
                }
            }
        }
        {
            let output_view = output.create_view(&Default::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Celesta scene convert"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &output_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&pipelines.convert);
            pass.set_bind_group(0, convert_group, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit([encoder.finish()]);
        Ok(self.plugin_layer_texture(output, width, height))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_queue_clamps_blend_offsets() {
        let blend = |depth_write, queue_offset| AlphaMode::Blend {
            depth_write,
            queue_offset,
        };
        let cases = [
            (blend(false, 5), Alpha::Blend, 3000),
            (blend(false, -3), Alpha::Blend, 2997),
            (blend(false, -20), Alpha::Blend, 2991),
            (blend(false, i32::MIN), Alpha::Blend, 2991),
            (blend(true, -3), Alpha::BlendDepthWrite, 2501),
            (blend(true, 4), Alpha::BlendDepthWrite, 2505),
            (blend(true, 600), Alpha::BlendDepthWrite, 2510),
            (blend(true, i32::MAX), Alpha::BlendDepthWrite, 2510),
            (AlphaMode::BLEND, Alpha::Blend, 3000),
            (AlphaMode::Mask { cutoff: 0.5 }, Alpha::Mask, 2450),
            (AlphaMode::Opaque, Alpha::Opaque, 2000),
        ];
        for (case, (alpha_mode, alpha, queue)) in cases.into_iter().enumerate() {
            assert_eq!(
                (Alpha::of(&alpha_mode), render_queue(&alpha_mode)),
                (alpha, queue),
                "case {case}"
            );
        }
    }

    #[test]
    fn determinant_of_scaled_and_singular_matrices() {
        let mut m = [0.0_f32; 16];
        for (i, scale) in [2.0, 3.0, 4.0, 1.0].into_iter().enumerate() {
            m[i * 4 + i] = scale;
        }
        m[12] = 5.0;
        assert_eq!(determinant(&m), 24.0);
        // Swapping two rows negates it.
        let mut swapped = m;
        for c in 0..4 {
            swapped.swap(c * 4, c * 4 + 1);
        }
        assert_eq!(determinant(&swapped), -24.0);
        m[0] = 0.0;
        assert_eq!(determinant(&m), 0.0);
    }

    #[test]
    fn projections_put_near_at_depth_0_and_far_at_1() {
        let projections = [
            Projection::Perspective {
                yfov: 1.0,
                near: 0.5,
                far: 20.0,
            },
            Projection::Orthographic {
                height: 2.0,
                near: 0.5,
                far: 20.0,
            },
        ];
        for projection in projections {
            let matrix = projection_matrix(&projection, 2.0);
            let depth = |z: f32| {
                let clip: Vec<f32> = (0..4)
                    .map(|row| matrix[8 + row] * z + matrix[12 + row])
                    .collect();
                clip[2] / clip[3]
            };
            assert!(depth(-0.5).abs() < 1e-6, "{projection:?}");
            assert!((depth(-20.0) - 1.0).abs() < 1e-6, "{projection:?}");
        }
        // 2 m high on a canvas twice as wide as high: 1 m reaches the top
        // edge and 2 m the side.
        let matrix = projection_matrix(
            &Projection::Orthographic {
                height: 2.0,
                near: 0.5,
                far: 20.0,
            },
            2.0,
        );
        assert_eq!((matrix[0] * 2.0, matrix[5]), (1.0, 1.0));
    }

    #[test]
    fn scene_canvas_size_caps_texels_and_keeps_the_aspect() {
        // A 4K frame fits at 1×, and smaller ones follow the density.
        assert_eq!(scene_canvas_size([3840.0, 2160.0], 1.0, 8192), (3840, 2160));
        assert_eq!(scene_canvas_size([64.0, 64.0], 2.0, 8192), (128, 128));
        // ×64, a 64-unit square would be 4096² texels; capped, it keeps
        // its aspect and stays within the cap.
        let (width, height) = scene_canvas_size([64.0, 64.0], 64.0, 8192);
        assert_eq!(width, height);
        assert!(u64::from(width) * u64::from(height) <= SCENE_CANVAS_TEXELS);
        assert!(width >= 2895, "{width}");
        // A 4K frame at 2× is drawn at about 1.01×.
        let (width, height) = scene_canvas_size([3840.0, 2160.0], 2.0, 8192);
        assert!(u64::from(width) * u64::from(height) <= SCENE_CANVAS_TEXELS);
        assert!((f64::from(width) / f64::from(height) - 16.0 / 9.0).abs() < 1e-3);
        assert!(width >= 3860, "{width}");
        // The longest side's limit still holds first.
        assert_eq!(scene_canvas_size([100_000.0, 1.0], 1.0, 8192), (8192, 1));
    }

    /// Format features where `Rgba16Float` is missing `half_float`'s
    /// usages and flags, and every other format has everything.
    fn features_without(
        half_float: wgpu::TextureFormatFeatures,
    ) -> impl Fn(wgpu::TextureFormat) -> wgpu::TextureFormatFeatures {
        move |format| {
            let all = wgpu::TextureFormatFeatures {
                allowed_usages: wgpu::TextureUsages::all(),
                flags: wgpu::TextureFormatFeatureFlags::all(),
            };
            if format == wgpu::TextureFormat::Rgba16Float {
                wgpu::TextureFormatFeatures {
                    allowed_usages: all.allowed_usages - half_float.allowed_usages,
                    flags: all.flags - half_float.flags,
                }
            } else {
                all
            }
        }
    }

    #[test]
    fn scene_target_falls_back_from_what_the_gpu_lacks() {
        use wgpu::{TextureFormat, TextureFormatFeatureFlags as Flags, TextureUsages as Usages};
        let target = |format, samples| Some(SceneTarget { format, samples });
        let lacking = |allowed_usages, flags| {
            scene_target(features_without(wgpu::TextureFormatFeatures {
                allowed_usages,
                flags,
            }))
        };
        assert_eq!(
            lacking(Usages::empty(), Flags::empty()),
            target(TextureFormat::Rgba16Float, 4)
        );
        // Without 4× multisampling or resolving, it is drawn at 1 sample.
        for flags in [Flags::MULTISAMPLE_X4, Flags::MULTISAMPLE_RESOLVE] {
            assert_eq!(
                lacking(Usages::empty(), flags),
                target(TextureFormat::Rgba16Float, 1)
            );
        }
        // Not renderable, blendable, or sampleable, it gives way to
        // `Rgba8Unorm`, still multisampled.
        assert_eq!(
            lacking(Usages::RENDER_ATTACHMENT, Flags::empty()),
            target(TextureFormat::Rgba8Unorm, 4)
        );
        assert_eq!(
            lacking(Usages::TEXTURE_BINDING, Flags::empty()),
            target(TextureFormat::Rgba8Unorm, 4)
        );
        assert_eq!(
            lacking(Usages::empty(), Flags::BLENDABLE),
            target(TextureFormat::Rgba8Unorm, 4)
        );
        // Depth that cannot be multisampled leaves color at 1 sample too.
        let no_depth_msaa = |format| {
            let mut features = features_without(wgpu::TextureFormatFeatures {
                allowed_usages: Usages::empty(),
                flags: Flags::empty(),
            })(format);
            if format == DEPTH_FORMAT {
                features.flags -= Flags::MULTISAMPLE_X4;
            }
            features
        };
        assert_eq!(
            scene_target(no_depth_msaa),
            target(TextureFormat::Rgba16Float, 1)
        );
        // Neither format renderable: nothing to draw on.
        let nothing = |format| {
            let mut features = features_without(wgpu::TextureFormatFeatures {
                allowed_usages: Usages::empty(),
                flags: Flags::empty(),
            })(format);
            if format != DEPTH_FORMAT {
                features.allowed_usages -= Usages::RENDER_ATTACHMENT;
            }
            features
        };
        assert_eq!(scene_target(nothing), None);
    }

    /// A model of one unskinned geometry of `vertices` vertices without
    /// triangles, drawn `draws` times.
    fn bare_model(vertices: usize, draws: usize) -> SceneModel {
        SceneModel {
            canvas: [1.0, 1.0],
            textures: vec![],
            geometries: vec![celesta_plugin::Geometry {
                positions: vec![[0.0; 3]; vertices],
                normals: vec![[0.0; 3]; vertices],
                uvs: vec![[0.0; 2]; vertices],
                tangents: None,
                joints: None,
                weights: None,
                targets: vec![],
                indices: vec![],
            }],
            skins: vec![],
            shaders: vec![],
            materials: vec![celesta_plugin::Material {
                alpha_mode: AlphaMode::Opaque,
                double_sided: false,
                kind: MaterialKind::Unlit(celesta_plugin::UnlitMaterial {
                    base_color: [1.0; 4],
                    base_color_texture: None,
                }),
            }],
            draws: vec![
                celesta_plugin::DrawInfo {
                    geometry: 0,
                    material: 0,
                    skin: None,
                };
                draws
            ],
        }
    }

    #[test]
    fn canvas_sides_are_at_least_1_and_at_most_the_longest() {
        // The model's canvas and a frame's own keep one rule.
        let mut model = bare_model(4, 1);
        for side in [1.0, 640.5, MAX_SCENE_CANVAS_SIDE] {
            assert_eq!(invalid_canvas("the frame's", [side, 64.0]), None, "{side}");
            model.canvas = [64.0, side];
            assert_eq!(invalid_scene(&model), None, "{side}");
        }
        // A subnormal side, which would make the aspect 0 or infinite, as
        // well as one too long to draw.
        let refused = [
            1e-45,
            0.5,
            0.0,
            -1.0,
            16_384.5,
            1e30,
            f32::INFINITY,
            f32::NAN,
        ];
        for side in refused {
            let message = invalid_canvas("the frame's", [side, 64.0]);
            assert!(
                message.as_deref().is_some_and(|m| m.contains("canvas")),
                "{side}: {message:?}"
            );
            model.canvas = [64.0, side];
            let message = invalid_scene(&model);
            assert!(
                message.as_deref().is_some_and(|m| m.contains("canvas")),
                "{side}: {message:?}"
            );
        }
    }

    #[test]
    fn scene_buffer_bytes_counts_streams_and_skinned_draws() {
        // Positions and normals 16 bytes a vertex, UVs 8, no triangles (one
        // 32-byte element), and 48 bytes a vertex for each of 3 draws.
        let mut model = bare_model(100, 3);
        assert_eq!(
            scene_buffer_bytes(&model),
            1600 + 1600 + 800 + 32 + 3 * 4800
        );
        // Morph targets with normals add both offset streams.
        model.geometries[0].targets = vec![
            celesta_plugin::MorphTarget {
                positions: vec![[0.0; 3]; 100],
                normals: vec![[0.0; 3]; 100],
            };
            2
        ];
        assert_eq!(
            scene_buffer_bytes(&model),
            1600 + 1600 + 800 + 32 + 3 * 4800 + 2 * 3200
        );
        assert!(invalid_scene(&model).is_none());
        // 100,000 vertices, 48 bytes each skinned, drawn 230 times: over
        // 1 GiB, refused before anything is allocated.
        let model = bare_model(100_000, 230);
        let message = invalid_scene(&model).expect("over the budget");
        assert!(message.contains("bytes"), "{message}");
    }

    #[test]
    fn lit_normal_matrix_is_the_signed_cofactor() {
        let scaling = |x: f32, y: f32, z: f32| {
            let mut m = [0.0_f32; 16];
            for (i, scale) in [x, y, z, 1.0].into_iter().enumerate() {
                m[i * 4 + i] = scale;
            }
            m
        };
        // The cofactor diag(1, 2, 2), over its largest entry: the inverse
        // transpose diag(0.5, 1, 1).
        assert_eq!(
            normal_matrix(&scaling(2.0, 1.0, 1.0)),
            [
                [0.5, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0]
            ]
        );
        // However large `world` is, it stays finite.
        let huge = normal_matrix(&scaling(1e30, 1e30, 1e30));
        assert_eq!(huge[0], [1.0, 0.0, 0.0, 0.0]);
        // Mirrored: the inverse transpose itself, not its negation.
        assert_eq!(
            normal_matrix(&scaling(-1.0, 1.0, 1.0)),
            [
                [-1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0]
            ]
        );
        let collapsed = normal_matrix(&scaling(0.0, 0.0, 0.0));
        assert!(
            collapsed
                .as_flattened()
                .iter()
                .all(|value| value.is_finite())
        );
        // A rotation is its own inverse transpose, its largest entry 1.
        let (sin, cos) = 0.5_f32.sin_cos();
        let mut turned = scaling(1.0, 1.0, 1.0);
        turned[0] = cos;
        turned[1] = sin;
        turned[4] = -sin;
        turned[5] = cos;
        let matrix = normal_matrix(&turned);
        for (column, expected) in [[cos, sin, 0.0], [-sin, cos, 0.0], [0.0, 0.0, 1.0]]
            .iter()
            .enumerate()
        {
            for row in 0..3 {
                assert!((matrix[column][row] - expected[row]).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn shading_eye_is_where_the_view_takes_the_origin_from() {
        let frame = |view: [f32; 16]| SceneFrame {
            canvas: None,
            camera: Camera {
                view,
                projection: Projection::Perspective {
                    yfov: 1.0,
                    near: 0.1,
                    far: 100.0,
                },
            },
            lights: Default::default(),
            time: 0.0,
            draws: vec![],
            skins: vec![],
        };
        // Scaled ×2 and moved 4 back: the camera is 2 m from the origin
        // (`−Rᵀt` would put it at 8).
        let scaled = [
            2.0, 0.0, 0.0, 0.0, //
            0.0, 2.0, 0.0, 0.0, //
            0.0, 0.0, 2.0, 0.0, //
            0.0, 0.0, -4.0, 1.0,
        ];
        let uniform = frame_uniform(&frame(scaled), 1.0);
        assert_eq!(uniform.eye, [0.0, 0.0, 2.0, 1.0]);
        // Looking down −Y from (0, 2, 0), its +Z world +Y.
        let down = [
            1.0, 0.0, 0.0, 0.0, //
            0.0, 0.0, 1.0, 0.0, //
            0.0, -1.0, 0.0, 0.0, //
            0.0, 0.0, -2.0, 1.0,
        ];
        let uniform = frame_uniform(&frame(down), 1.0);
        assert_eq!(uniform.eye, [0.0, 2.0, 0.0, 1.0]);
        assert_eq!(uniform.forward, [0.0, 1.0, 0.0, 0.0]);
        // Turned about Y and scaled ×3, a view puts the camera where the
        // same view divided by 3 (rigid) does.
        let (sin, cos) = 0.7_f32.sin_cos();
        let (s, c) = (3.0 * sin, 3.0 * cos);
        let turned = [
            c, 0.0, s, 0.0, //
            0.0, 3.0, 0.0, 0.0, //
            -s, 0.0, c, 0.0, //
            1.0, -2.0, -5.0, 1.0,
        ];
        let mut rigid = turned.map(|value| value / 3.0);
        rigid[15] = 1.0;
        let scaled_eye = frame_uniform(&frame(turned), 1.0).eye;
        let rigid_eye = frame_uniform(&frame(rigid), 1.0).eye;
        for axis in 0..3 {
            assert!(
                (scaled_eye[axis] - rigid_eye[axis]).abs() < 1e-5,
                "{scaled_eye:?} vs {rigid_eye:?}"
            );
        }
    }

    #[test]
    fn frame_and_draw_uniforms_match_the_prelude() {
        let checked = check_material(UNLIT_SHADER, 1).unwrap();
        let module = &checked.module;
        let mut layouter = naga::proc::Layouter::default();
        layouter.update(module.to_ctx()).unwrap();
        let named = |name: &str| {
            module
                .types
                .iter()
                .find(|(_, ty)| ty.name.as_deref() == Some(name))
                .unwrap_or_else(|| panic!("the prelude declares {name}"))
        };
        let offset = |ty: &naga::Type, member: &str| match &ty.inner {
            naga::TypeInner::Struct { members, .. } => {
                members
                    .iter()
                    .find(|field| field.name.as_deref() == Some(member))
                    .unwrap_or_else(|| panic!("{member} is a member"))
                    .offset
            }
            _ => panic!("{:?} is not a struct", ty.name),
        };
        let (frame, frame_type) = named("Frame");
        assert_eq!(layouter[frame].size, 512);
        assert_eq!(offset(frame_type, "light_count"), 232);
        assert_eq!(offset(frame_type, "ambient"), 368);
        let (draw, draw_type) = named("Draw");
        assert_eq!(layouter[draw].size, 128);
        assert_eq!(offset(draw_type, "alpha_cutoff"), 116);

        assert_eq!(std::mem::size_of::<FrameUniform>(), 512);
        assert_eq!(std::mem::offset_of!(FrameUniform, light_count), 232);
        assert_eq!(std::mem::offset_of!(FrameUniform, ambient), 368);
        assert_eq!(std::mem::size_of::<DrawUniform>(), 128);
        assert_eq!(std::mem::offset_of!(DrawUniform, handedness), 112);
        assert_eq!(std::mem::offset_of!(DrawUniform, alpha_cutoff), 116);
    }

    #[test]
    fn built_in_materials_pass_the_material_checks() {
        let unlit = check_material(UNLIT_SHADER, 1).unwrap();
        assert_eq!((unlit.params_size, unlit.outline), (48, false));
        assert_eq!(unlit.params_size as usize, UNLIT_PARAMS);
    }

    #[test]
    fn unlit_numbers_must_be_finite() {
        let mut model = bare_model(3, 1);
        model.textures.push(celesta_plugin::SceneTexture {
            source: celesta_plugin::TextureSource::Rgba {
                width: 1,
                height: 1,
                pixels: std::sync::Arc::new(vec![255; 4]),
            },
            color_space: ColorSpace::Linear,
            sampler: Default::default(),
        });
        fn unlit(model: &mut SceneModel) -> &mut celesta_plugin::UnlitMaterial {
            match &mut model.materials[0].kind {
                MaterialKind::Unlit(unlit) => unlit,
                MaterialKind::Custom(_) => unreachable!("bare_model's material is unlit"),
            }
        }
        let transformed = |offset: [f32; 2], rotation: f32, scale: [f32; 2]| {
            Some(celesta_plugin::TextureRef {
                texture: 0,
                transform: Some(celesta_plugin::TextureTransform {
                    offset,
                    rotation,
                    scale,
                }),
            })
        };
        unlit(&mut model).base_color_texture = transformed([0.5, 0.0], 0.25, [2.0, 2.0]);
        assert_eq!(invalid_scene(&model), None);

        type Set = Box<dyn Fn(&mut celesta_plugin::UnlitMaterial)>;
        let cases: [(&str, Set); 4] = [
            ("base color", Box::new(|m| m.base_color[3] = f32::NAN)),
            (
                "texture transform",
                Box::new(move |m| {
                    m.base_color_texture = transformed([f32::INFINITY, 0.0], 0.0, [1.0, 1.0])
                }),
            ),
            (
                "texture transform",
                Box::new(move |m| {
                    m.base_color_texture = transformed([0.0, 0.0], f32::NAN, [1.0, 1.0])
                }),
            ),
            (
                "texture transform",
                Box::new(move |m| {
                    m.base_color_texture = transformed([0.0, 0.0], 0.0, [1.0, f32::NEG_INFINITY])
                }),
            ),
        ];
        for (name, set) in cases {
            let mut model = model.clone();
            set(unlit(&mut model));
            assert_eq!(
                invalid_scene(&model),
                Some(format!("material 0's {name} is not finite"))
            );
        }
    }

    #[test]
    fn unlit_params_pack_colour_and_transform_rows() {
        let floats = |bytes: &[u8]| -> Vec<f32> {
            bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|&chunk| f32::from_le_bytes(chunk))
                .collect()
        };
        let unlit = celesta_plugin::UnlitMaterial {
            base_color: [1.0, 0.5, 0.25, 1.0],
            base_color_texture: Some(celesta_plugin::TextureRef {
                texture: 0,
                transform: Some(celesta_plugin::TextureTransform {
                    offset: [0.5, 0.0],
                    rotation: 0.0,
                    scale: [1.0, 1.0],
                }),
            }),
        };
        let overrides = celesta_plugin::MaterialOverrides::default();
        let bytes = unlit_params(&unlit, &overrides);
        assert_eq!(floats(&bytes[0..16]), [1.0, 0.5, 0.25, 1.0]);
        assert_eq!(floats(&bytes[16..32]), [1.0, 0.0, 0.5, 0.0]);
        assert_eq!(floats(&bytes[32..48]), [0.0, 1.0, 0.0, 0.0]);

        let overrides = celesta_plugin::MaterialOverrides {
            texture_offset: Some([0.25, 0.0]),
            ..Default::default()
        };
        let bytes = unlit_params(&unlit, &overrides);
        assert_eq!(floats(&bytes[16..32])[2], 0.25);
    }

    #[test]
    fn make_room_drops_sizes_not_drawn_only_past_the_budget() {
        let sizes = |targets: &HashMap<(u32, u32), ()>| {
            let mut sizes: Vec<_> = targets.keys().copied().collect();
            sizes.sort();
            sizes
        };
        let mut targets = HashMap::from([((10, 10), ()), ((20, 10), ())]);
        let used = HashSet::from([(20, 10)]);
        // 300 + 100 texels fit 400: nothing goes.
        make_room(&mut targets, &used, (10, 10), 400);
        assert_eq!(sizes(&targets), [(10, 10), (20, 10)]);
        // 300 + 200 do not: the size not drawn goes, the drawn one stays.
        make_room(&mut targets, &used, (20, 10), 400);
        assert_eq!(sizes(&targets), [(20, 10)]);
        // Over the budget by the drawn sizes alone, they still stay.
        make_room(&mut targets, &used, (30, 30), 400);
        assert_eq!(sizes(&targets), [(20, 10)]);
        assert_eq!(
            SceneFrameResources::default().target_texels,
            2 * SCENE_CANVAS_TEXELS
        );
    }

    #[test]
    fn uniforms_past_the_largest_buffer_are_refused() {
        // 256-byte aligned, 1,000 draws' skinning parameters take 256,000
        // bytes, and their draw slots (a 128-byte draw uniform and 512
        // bytes of parameters, each aligned) three times that.
        assert_eq!(draw_slot(256), (256, 768));
        assert_eq!(draw_slot(64), (128, 640));
        assert_eq!(oversized_uniforms(1000, 256, 768_000), None);
        let message = oversized_uniforms(1000, 256, 767_999).expect("too large");
        assert!(
            message.contains("draw uniforms take 768000 bytes"),
            "{message}"
        );
        let message = oversized_uniforms(1001, 256, 768_000).expect("too large");
        assert!(message.contains("768768 bytes"), "{message}");
        assert!(oversized_uniforms(usize::MAX, 256, u64::MAX - 1).is_some());
    }

    #[test]
    fn srgb_mips_average_in_linear_light() {
        // Black and white average to linear 0.5, which sRGB encodes as 188.
        let pixels = [0, 0, 0, 255, 255, 255, 255, 255].to_vec();
        let levels = straight_mips(2, 1, pixels.clone(), ColorSpace::Srgb);
        assert_eq!(levels.len(), 2);
        assert_eq!(levels[1], (1, 1, vec![188, 188, 188, 255]));
        let levels = straight_mips(2, 1, pixels, ColorSpace::Linear);
        assert_eq!(levels[1], (1, 1, vec![128, 128, 128, 255]));
        // A uniform texel stays as it is.
        let levels = straight_mips(2, 2, [128, 64, 200, 77].repeat(4), ColorSpace::Srgb);
        assert_eq!(levels[1].2, vec![128, 64, 200, 77]);
    }
}
