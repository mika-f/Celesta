//! `LayerContent::Plugin` scene content: a provider's 3D scene, uploaded
//! once when it is opened and checked against each of its frames. A scene
//! draws onto the same kind of canvas as 2D meshes (see `mesh.rs`).
//!
//! Each frame, a compute pass morphs and skins every visible draw's
//! vertices (`scene_skin.wgsl`). A render pass then draws them, in UniVRM's
//! queue order (`draw_queue`) and by their material's shading and alpha
//! mode, through the camera onto a 4× multisampled canvas of linear,
//! premultiplied `Rgba16Float` color with `Depth32Float` depth, both
//! cleared (to transparent black and to 1): unlit (`scene.wgsl`) or MToon
//! under the frame's lights (`scene_mtoon.wgsl`, the same shader module).
//! A draw binds the frame's uniform as group 0, its own as group 1 at its
//! offset, and its material's textures, bound when the model loaded, as
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
use crate::texture::LayerTexture;
use celesta_plugin::{
    AlphaMode, Camera, ColorSpace, DrawState, Filter, Material, MaterialKind, ModelId,
    MtoonMaterial, OutlineWidthMode, Projection, SceneFrame, SceneModel, Wrap,
};
use std::collections::{HashMap, HashSet};
use wgpu::util::DeviceExt;

/// The canvas a scene is drawn on before it is converted to the layer's.
const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Samples per pixel of the color and depth canvases.
const SAMPLES: u32 = 4;
/// Vertices per compute workgroup, as `scene_skin.wgsl` declares.
const WORKGROUP: u32 = 64;
/// The bytes of one skinned vertex: `scene_skin.wgsl`'s `Skinned`.
const SKINNED_STRIDE: u64 = 48;
/// The bytes of one `SkinVertex`, the largest element a stream holds.
const SKIN_VERTEX_STRIDE: u64 = 32;
/// The most texels a scene's canvas has. A texel of it costs about 60 bytes
/// (four samples of color and of depth, the resolved color, the layer's), so
/// this is about 500 MB; a 4K frame (3840 × 2160) still fits at 1×, and a
/// larger canvas is drawn at a lower density.
pub(crate) const SCENE_CANVAS_TEXELS: u64 = 8_388_608;
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

/// A scene model and its copy on the GPU.
pub(crate) struct LoadedScene {
    pub(crate) id: ModelId,
    pub(crate) model: SceneModel,
    /// One per `model.geometries` entry.
    pub(crate) geometries: Vec<GeometryBuffers>,
    /// One per `model.materials` entry: its textures and samplers, bound as
    /// group 2 of its draws.
    pub(crate) materials: Vec<wgpu::BindGroup>,
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
/// each size drawn since the last frame began, within `target_texels`, and
/// the buffers of a frame's skinning and draws, which grow to the largest
/// frame's and are rewritten by every frame. A frame that draws no scene
/// lets all of it go.
pub(crate) struct SceneFrameResources {
    /// By canvas size in texels.
    targets: HashMap<(u32, u32), SceneTargets>,
    /// The most texels `targets` keeps before it drops the sizes this frame
    /// has not drawn: `SCENE_TARGET_TEXELS`, lowered by tests.
    pub(crate) target_texels: u64,
    /// The sizes drawn since the last `begin_frame`.
    used: HashSet<(u32, u32)>,
    /// `STORAGE`: every skin's matrices, then each queued draw's morph
    /// pairs.
    frame_data: GrowingBuffer,
    /// `UNIFORM`: one `SkinParams` per queued draw.
    skin_params: GrowingBuffer,
    /// `UNIFORM`: one `DrawUniform` per queued draw.
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
/// they resolve to, and the bind group that converts it.
struct SceneTargets {
    multisampled: wgpu::TextureView,
    depth: wgpu::TextureView,
    resolved: wgpu::TextureView,
    convert_group: wgpu::BindGroup,
}

/// The bind groups of a frame's buffers.
struct FrameGroups {
    /// `frame_data` and `skin_params`, as `scene_skin.wgsl`'s group 1.
    frame: wgpu::BindGroup,
    /// `view`, as `scene.wgsl`'s group 0.
    view: wgpu::BindGroup,
    /// `draws`, as `scene.wgsl`'s group 1.
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

/// How a material shades its surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Shading {
    /// Its base color, unlit (`scene.wgsl`).
    Unlit,
    /// UniVRM's MToon10 (`scene_mtoon.wgsl`).
    Mtoon,
}

impl Shading {
    fn of(kind: &MaterialKind) -> Self {
        match kind {
            MaterialKind::Unlit(_) => Self::Unlit,
            MaterialKind::Mtoon(_) => Self::Mtoon,
        }
    }
}

/// Which of a draw's passes a pipeline draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Pass {
    /// The surface.
    Main,
    /// An MToon material's outline, drawn after its main pass.
    #[expect(dead_code, reason = "outlines are not drawn yet")]
    Outline,
}

/// Which faces a pipeline culls.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Cull {
    /// A double-sided material keeps both.
    None,
    /// A single-sided material shows its front faces.
    Back,
    /// An outline shows the back faces it pushes out.
    #[expect(dead_code, reason = "outlines are not drawn yet")]
    Front,
}

/// Which faces a draw keeps, and which way its front faces turn on screen:
/// clockwise when its world matrix mirrors it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Facing {
    cull: Cull,
    clockwise: bool,
}

/// What a scene draw pipeline is made for. `ScenePipelines::prepare`
/// makes the ones a model's materials use when it loads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PipelineKey {
    shading: Shading,
    pass: Pass,
    facing: Facing,
    alpha: Alpha,
}

impl PipelineKey {
    /// The main pass of `material`, its front faces clockwise or not.
    fn main(material: &Material, clockwise: bool) -> Self {
        Self {
            shading: Shading::of(&material.kind),
            pass: Pass::Main,
            facing: Facing {
                cull: if material.double_sided {
                    Cull::None
                } else {
                    Cull::Back
                },
                clockwise,
            },
            alpha: Alpha::of(material.alpha_mode),
        }
    }
}

/// How a draw's alpha is drawn, as UniVRM draws its material's alpha mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Alpha {
    /// Alpha ignored, depth written.
    Opaque,
    /// Coverage from the alpha against the cutoff, depth written.
    Mask,
    /// Blended by alpha over what is drawn, depth written: an MToon
    /// material's `transparentWithZWrite`.
    #[expect(dead_code, reason = "render queues do not place it yet")]
    BlendZWrite,
    /// Blended by alpha over what is drawn, depth tested but not written.
    Blend,
}

impl Alpha {
    fn of(mode: AlphaMode) -> Self {
        match mode {
            AlphaMode::Opaque => Self::Opaque,
            AlphaMode::Mask { .. } => Self::Mask,
            AlphaMode::Blend => Self::Blend,
        }
    }

    /// Unity's render queue: lower queues draw first.
    fn queue(self) -> u32 {
        match self {
            Self::Opaque => 2000,
            Self::Mask => 2450,
            Self::BlendZWrite => 2501,
            Self::Blend => 3000,
        }
    }

    /// The fragment entry point of `shading`'s main pass.
    fn entry_point(self, shading: Shading) -> &'static str {
        match (shading, self) {
            (Shading::Unlit, Self::Opaque) => "fs_opaque",
            (Shading::Unlit, Self::Mask) => "fs_mask",
            (Shading::Unlit, Self::BlendZWrite | Self::Blend) => "fs_blend",
            (Shading::Mtoon, Self::Opaque) => "fs_mtoon_opaque",
            (Shading::Mtoon, Self::Mask) => "fs_mtoon_mask",
            (Shading::Mtoon, Self::BlendZWrite | Self::Blend) => "fs_mtoon_blend",
        }
    }

    fn writes_depth(self) -> bool {
        self != Self::Blend
    }

    /// Straight color over the canvas's premultiplied color, as Unity
    /// blends it; alpha accumulates as coverage.
    fn blend(self) -> Option<wgpu::BlendState> {
        matches!(self, Self::Blend | Self::BlendZWrite).then_some(wgpu::BlendState {
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

/// What `scene.wgsl` reads as `Frame`, group 0: the camera, the lights, and
/// the time.
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
    /// The frame's time in seconds, the canvas's height over its width, and
    /// how many of `lights` there are.
    params: [f32; 4],
    /// The main light first; black when the frame has none.
    lights: [LightUniform; MAX_DIRECTIONAL_LIGHTS],
    /// `Lights::ambient`, `w` 0.
    ambient: [[f32; 4]; 9],
}

/// What `scene.wgsl` reads as `Draw`, group 1 at each draw's offset. An
/// unlit draw fills `world` to `uv_v`; the rest is MToon's.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct DrawUniform {
    world: [f32; 16],
    /// `normal_matrix(world)`'s columns, `w` 0.
    normal_matrix: [[f32; 4]; 3],
    base_color: [f32; 4],
    /// The texture transform's first row, then a mask material's cutoff.
    uv_u: [f32; 4],
    uv_v: [f32; 4],
    /// The shade color, then the shading shift factor.
    shade: [f32; 4],
    /// The emissive factor, then the shading shift texture's scale.
    emissive: [f32; 4],
    /// The matcap factor, then the shading toony factor.
    matcap: [f32; 4],
    /// The parametric rim color, then the rim lighting mix.
    rim: [f32; 4],
    /// The rim's fresnel power and lift, the GI equalization factor, and
    /// the normal texture's scale.
    rim_shape: [f32; 4],
    /// The outline color, then the outline lighting mix.
    outline: [f32; 4],
    /// The outline width factor, the outline width mode (0 none, 1 world
    /// coordinates, 2 screen coordinates), 0, 0.
    outline_width: [f32; 4],
    /// The UV animation's scroll in x and y, its rotation in turns a second,
    /// and 1 when the material has a normal texture, else 0.
    uv_animation: [f32; 4],
}

// The sizes of `scene.wgsl`'s `Frame` and `Draw`.
const _: () = assert!(std::mem::size_of::<FrameUniform>() == 512);
const _: () = assert!(std::mem::size_of::<DrawUniform>() == 288);

/// The pipelines that draw scenes and what they bind, shared by every
/// scene.
pub(crate) struct ScenePipelines {
    geometry_layout: wgpu::BindGroupLayout,
    frame_layout: wgpu::BindGroupLayout,
    skin: wgpu::ComputePipeline,
    /// `scene.wgsl` with `scene_mtoon.wgsl`.
    shader: wgpu::ShaderModule,
    /// Group 0: the frame uniform.
    view_layout: wgpu::BindGroupLayout,
    /// Group 1: the draw uniform.
    draw_layout: wgpu::BindGroupLayout,
    /// Group 2 of an unlit material: its base color texture and sampler.
    unlit_layout: wgpu::BindGroupLayout,
    /// Group 2 of an MToon material: each of `MTOON_SLOTS`' texture and
    /// sampler.
    mtoon_layout: wgpu::BindGroupLayout,
    unlit_pipeline_layout: wgpu::PipelineLayout,
    mtoon_pipeline_layout: wgpu::PipelineLayout,
    /// The pipelines the loaded models' materials use, made as each loads.
    draw: HashMap<PipelineKey, wgpu::RenderPipeline>,
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

/// A texel a material samples in place of a texture it lacks: Unity's
/// default for the property.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DefaultTexel {
    White,
    Black,
}

/// An MToon material's texture slots, in binding order: slot `k` binds its
/// texture at `2k` and its sampler at `2k + 1` of group 2. Each has its
/// JSON name and the texel bound when it is empty, which leaves its term
/// as UniVRM's keyword gates do without the texture: no shift but the
/// factor's, no matcap, the whole of the UV animation, the width factor
/// alone. An empty normal slot is not read.
const MTOON_SLOTS: [(&str, DefaultTexel); 9] = [
    ("baseColorTexture", DefaultTexel::White),
    ("shadeMultiplyTexture", DefaultTexel::White),
    ("shadingShiftTexture", DefaultTexel::Black),
    ("normalTexture", DefaultTexel::White),
    ("emissiveTexture", DefaultTexel::White),
    ("matcapTexture", DefaultTexel::Black),
    ("rimMultiplyTexture", DefaultTexel::White),
    ("outlineWidthMultiplyTexture", DefaultTexel::White),
    ("uvAnimationMaskTexture", DefaultTexel::White),
];

/// The slot the vertex stage samples (the outline's width); the fragment
/// stage samples the others.
const MTOON_VERTEX_SLOT: usize = 7;

/// The textures `mtoon`'s slots name, in `MTOON_SLOTS`' order.
fn mtoon_textures(mtoon: &MtoonMaterial) -> [Option<usize>; 9] {
    [
        mtoon.base_color_texture.as_ref().map(|slot| slot.texture),
        mtoon.shade_multiply_texture.map(|slot| slot.texture),
        mtoon.shading_shift_texture.map(|slot| slot.texture),
        mtoon.normal_texture.map(|slot| slot.texture),
        mtoon.emissive_texture.map(|slot| slot.texture),
        mtoon.matcap_texture.map(|slot| slot.texture),
        mtoon.rim_multiply_texture.map(|slot| slot.texture),
        mtoon
            .outline_width_multiply_texture
            .map(|slot| slot.texture),
        mtoon.uv_animation_mask_texture.map(|slot| slot.texture),
    ]
}

/// Every number of `mtoon`, its texture slots' scales and base color
/// texture transform included.
fn mtoon_numbers(mtoon: &MtoonMaterial) -> Vec<f32> {
    let mut numbers = mtoon.base_color.to_vec();
    numbers.extend(mtoon.emissive_factor);
    numbers.extend(mtoon.shade_color_factor);
    numbers.extend(mtoon.matcap_factor);
    numbers.extend(mtoon.parametric_rim_color_factor);
    numbers.extend(mtoon.outline_color_factor);
    numbers.extend([
        mtoon.shading_shift_factor,
        mtoon.shading_toony_factor,
        mtoon.gi_equalization_factor,
        mtoon.rim_lighting_mix_factor,
        mtoon.parametric_rim_fresnel_power_factor,
        mtoon.parametric_rim_lift_factor,
        mtoon.outline_width_factor,
        mtoon.outline_lighting_mix_factor,
        mtoon.uv_animation_scroll_x_speed_factor,
        mtoon.uv_animation_scroll_y_speed_factor,
        mtoon.uv_animation_rotation_speed_factor,
    ]);
    numbers.extend(mtoon.normal_texture.map(|slot| slot.scale));
    numbers.extend(mtoon.shading_shift_texture.map(|slot| slot.scale));
    if let Some(transform) = mtoon
        .base_color_texture
        .as_ref()
        .and_then(|texture| texture.transform)
    {
        numbers.extend(transform.offset);
        numbers.extend(transform.scale);
        numbers.push(transform.rotation);
    }
    numbers
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

/// A uniform bound at a different offset for each draw.
fn dynamic_uniform_entry<T>(
    binding: u32,
    visibility: wgpu::ShaderStages,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: true,
            min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<T>() as u64),
        },
        count: None,
    }
}

impl ScenePipelines {
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let skin_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Celesta scene skinning shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("scene_skin.wgsl").into()),
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Celesta scene shader"),
            source: wgpu::ShaderSource::Wgsl(
                concat!(include_str!("scene.wgsl"), include_str!("scene_mtoon.wgsl")).into(),
            ),
        });

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
                dynamic_uniform_entry::<SkinParams>(1, wgpu::ShaderStages::COMPUTE),
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
        let draw_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Celesta scene draw layout"),
            entries: &[dynamic_uniform_entry::<DrawUniform>(
                0,
                wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            )],
        });
        let unlit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Celesta scene unlit material layout"),
            entries: &texture_entries(0, wgpu::ShaderStages::FRAGMENT),
        });
        let mtoon_entries: Vec<wgpu::BindGroupLayoutEntry> = (0..MTOON_SLOTS.len())
            .flat_map(|slot| {
                let visibility = if slot == MTOON_VERTEX_SLOT {
                    wgpu::ShaderStages::VERTEX
                } else {
                    wgpu::ShaderStages::FRAGMENT
                };
                texture_entries(2 * slot as u32, visibility)
            })
            .collect();
        let mtoon_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Celesta scene MToon material layout"),
            entries: &mtoon_entries,
        });
        let pipeline_layout = |label, material_layout| {
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &[
                    Some(&view_layout),
                    Some(&draw_layout),
                    Some(material_layout),
                ],
                immediate_size: 0,
            })
        };
        let unlit_pipeline_layout = pipeline_layout("Celesta scene unlit layout", &unlit_layout);
        let mtoon_pipeline_layout = pipeline_layout("Celesta scene MToon layout", &mtoon_layout);

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
                module: &shader,
                entry_point: Some("vs_full"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
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
        Self {
            geometry_layout,
            frame_layout,
            skin,
            shader,
            view_layout,
            draw_layout,
            unlit_layout,
            mtoon_layout,
            unlit_pipeline_layout,
            mtoon_pipeline_layout,
            draw: HashMap::new(),
            convert_layout,
            convert,
            empty,
            white: texel("Celesta scene white texel", [255; 4]),
            black: texel("Celesta scene black texel", [0, 0, 0, 255]),
            default_sampler: device.create_sampler(&Default::default()),
        }
    }

    /// Makes the main-pass pipelines `model`'s materials draw with, for
    /// either winding, unless they are made already.
    pub(crate) fn prepare(&mut self, device: &wgpu::Device, model: &SceneModel) {
        for material in &model.materials {
            for clockwise in [false, true] {
                let key = PipelineKey::main(material, clockwise);
                if !self.draw.contains_key(&key) {
                    let pipeline = self.draw_pipeline(device, key);
                    self.draw.insert(key, pipeline);
                }
            }
        }
    }

    fn draw_pipeline(&self, device: &wgpu::Device, key: PipelineKey) -> wgpu::RenderPipeline {
        // The skinned vertices, then the geometry's texture coordinates.
        let (layout, vertex_entry, skinned_attributes, uv_location) = match key.shading {
            Shading::Unlit => (
                &self.unlit_pipeline_layout,
                "vs_main",
                &wgpu::vertex_attr_array![0 => Float32x4][..],
                1,
            ),
            Shading::Mtoon => (
                &self.mtoon_pipeline_layout,
                "vs_mtoon",
                &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4][..],
                3,
            ),
        };
        let fragment_entry = match key.pass {
            Pass::Main => key.alpha.entry_point(key.shading),
            Pass::Outline => unreachable!("no key draws an outline yet"),
        };
        let uv_attributes = [wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x2,
            offset: 0,
            shader_location: uv_location,
        }];
        let vertex_buffers = [
            Some(wgpu::VertexBufferLayout {
                array_stride: SKINNED_STRIDE,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: skinned_attributes,
            }),
            Some(wgpu::VertexBufferLayout {
                array_stride: 8,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &uv_attributes,
            }),
        ];
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Celesta scene draw"),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: &self.shader,
                entry_point: Some(vertex_entry),
                compilation_options: Default::default(),
                buffers: &vertex_buffers,
            },
            fragment: Some(wgpu::FragmentState {
                module: &self.shader,
                entry_point: Some(fragment_entry),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: COLOR_FORMAT,
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
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: SAMPLES,
                ..Default::default()
            },
            multiview_mask: None,
            cache: None,
        })
    }

    /// `material`'s group 2: its textures, of `textures`, with their
    /// samplers, and a default texel for each it lacks.
    fn material_group(
        &self,
        device: &wgpu::Device,
        textures: &[(wgpu::TextureView, wgpu::Sampler)],
        material: &Material,
    ) -> wgpu::BindGroup {
        let bound = |texture: Option<usize>, default: DefaultTexel| match texture {
            Some(texture) => (&textures[texture].0, &textures[texture].1),
            None => (
                match default {
                    DefaultTexel::White => &self.white,
                    DefaultTexel::Black => &self.black,
                },
                &self.default_sampler,
            ),
        };
        let (layout, slots) = match &material.kind {
            MaterialKind::Unlit(unlit) => (
                &self.unlit_layout,
                vec![bound(
                    unlit
                        .base_color_texture
                        .as_ref()
                        .map(|texture| texture.texture),
                    DefaultTexel::White,
                )],
            ),
            MaterialKind::Mtoon(mtoon) => (
                &self.mtoon_layout,
                mtoon_textures(mtoon)
                    .into_iter()
                    .zip(MTOON_SLOTS)
                    .map(|(texture, (_, default))| bound(texture, default))
                    .collect(),
            ),
        };
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

/// A filterable 2D texture at `binding` and its sampler at `binding + 1`.
fn texture_entries(
    binding: u32,
    visibility: wgpu::ShaderStages,
) -> [wgpu::BindGroupLayoutEntry; 2] {
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

fn finite(values: &[f32]) -> bool {
    values.iter().all(|value| value.is_finite())
}

/// Why `model` cannot be drawn: a canvas without area, vertex streams of
/// different lengths, triangles outside their vertices, or a reference to
/// something it lacks, or buffers past `SCENE_BUFFER_BUDGET`. A draw's skin
/// must have every joint its geometry's vertices name; both are fixed, so
/// that is checked here, once, rather than in every frame.
pub(crate) fn invalid_scene(model: &SceneModel) -> Option<String> {
    if !model
        .canvas
        .iter()
        .all(|size| size.is_finite() && *size > 0.0)
    {
        return Some(format!("the canvas is {:?}", model.canvas));
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
    for (index, material) in model.materials.iter().enumerate() {
        if let AlphaMode::Mask { cutoff } = material.alpha_mode
            && !cutoff.is_finite()
        {
            return Some(format!("material {index}'s alphaCutoff is {cutoff}"));
        }
        let textures = model.textures.len();
        match &material.kind {
            MaterialKind::Unlit(unlit) => {
                if let Some(texture) = &unlit.base_color_texture
                    && texture.texture >= textures
                {
                    return Some(format!(
                        "material {index} uses texture {}, of {textures}",
                        texture.texture
                    ));
                }
            }
            MaterialKind::Mtoon(mtoon) => {
                for (texture, (name, _)) in mtoon_textures(mtoon).into_iter().zip(MTOON_SLOTS) {
                    if let Some(texture) = texture
                        && texture >= textures
                    {
                        return Some(format!(
                            "material {index}'s {name} uses texture {texture}, of {textures}"
                        ));
                    }
                }
                if !finite(&mtoon_numbers(mtoon)) {
                    return Some(format!(
                        "material {index}'s mtoon parameters are not finite"
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
/// one skinning parameter block and one draw uniform per draw, each
/// `alignment`-aligned, in buffers no larger than `largest` bytes.
fn oversized_uniforms(draws: usize, alignment: u32, largest: u64) -> Option<String> {
    let buffers = [
        ("skinning parameters", std::mem::size_of::<SkinParams>()),
        ("draw uniforms", std::mem::size_of::<DrawUniform>()),
    ];
    for (name, size) in buffers {
        let bytes = (draws as u64).saturating_mul(aligned(size, alignment) as u64);
        if bytes > largest {
            return Some(format!(
                "the frame's {draws} draws' {name} take {bytes} bytes, more than this GPU's {largest}"
            ));
        }
    }
    None
}

/// Why `camera` cannot see anything: a lens without depth or angle, or a
/// view that cannot be undone.
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
    // A view is normally rigid, its determinant ±1; one of about zero has
    // no inverse worth the name.
    if determinant(&camera.view).abs() <= f64::EPSILON {
        return Some("the camera's view is not invertible".to_owned());
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

/// Why `frame` does not fit `model` (which `invalid_scene` passed): a draw
/// it lacks or lists twice, morph weights or skinning matrices that do not
/// match, a number that is not finite, or a camera that sees nothing.
pub(crate) fn invalid_scene_frame(model: &SceneModel, frame: &SceneFrame) -> Option<String> {
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
        pipelines.prepare(device, &model);
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
            .map(|material| pipelines.material_group(device, &textures, material))
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
            model,
            geometries,
            materials,
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

/// What takes `world`'s normals to world space: the cofactor matrix of its
/// upper 3×3, times the sign of its determinant (+1 for a determinant of
/// 0), as columns. That is the inverse transpose times the determinant's
/// magnitude: the same directions, which the shader normalises, and finite
/// even when `world` scales to nothing. Worked out in `f64` and divided by
/// its largest entry's magnitude, so that it stays finite as `f32` however
/// large `world` is.
fn normal_matrix(world: &[f32; 16]) -> [[f32; 4]; 3] {
    let column = |c: usize| [0, 1, 2].map(|r| f64::from(world[c * 4 + r]));
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
    let determinant: f64 = (0..3).map(|row| a[row] * cofactor[0][row]).sum();
    let sign = if determinant < 0.0 { -1.0 } else { 1.0 };
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

/// What `frame` reads as `Frame` on a canvas `aspect` times as wide as it
/// is high, its lights checked by `invalid_scene_frame`.
fn frame_uniform(frame: &SceneFrame, aspect: f32) -> FrameUniform {
    let camera = &frame.camera;
    let projection = projection_matrix(&camera.projection, aspect);
    // The view's rotation `R` and translation `t`: the camera sits at
    // `−Rᵀt`, and its +Z is `R`'s third row.
    let view = camera.view.map(f64::from);
    let eye: [f64; 3] =
        std::array::from_fn(|c| -(0..3).map(|r| view[c * 4 + r] * view[12 + r]).sum::<f64>());
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
        params: [
            frame.time as f32,
            1.0 / aspect,
            frame.lights.directional.len() as f32,
            0.0,
        ],
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

/// The draws of `frame` to draw, in UniVRM's order: by their material's
/// queue, opaque before mask before blend. Blended draws go farthest first
/// by the view-space depth of their `center`, or else of their geometry's
/// rest bounds centre moved by `world`. Ties go by draw index. Hidden
/// draws, and draws without triangles, are left out.
fn draw_queue<'a>(scene: &LoadedScene, frame: &'a SceneFrame) -> Vec<&'a DrawState> {
    let mut queue: Vec<(u32, f32, &DrawState)> = frame
        .draws
        .iter()
        .filter_map(|state| {
            let info = &scene.model.draws[state.index];
            let geometry = &scene.geometries[info.geometry];
            if !state.visible || geometry.index_count == 0 {
                return None;
            }
            let alpha = Alpha::of(scene.model.materials[info.material].alpha_mode);
            // The camera looks down −Z: the farther, the lower its z.
            let depth = if alpha == Alpha::Blend {
                let center = state
                    .center
                    .unwrap_or_else(|| transform_point(&state.world, geometry.center));
                transform_point(&frame.camera.view, center)[2]
            } else {
                0.0
            };
            Some((alpha.queue(), depth, state))
        })
        .collect();
    queue.sort_by(|(a_queue, a_depth, a), (b_queue, b_depth, b)| {
        a_queue
            .cmp(b_queue)
            .then(a_depth.total_cmp(b_depth))
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

/// `values` laid out `stride` bytes apart.
fn strided<T: bytemuck::Pod>(values: &[T], stride: usize) -> Vec<u8> {
    let mut bytes = vec![0; values.len() * stride];
    for (index, value) in values.iter().enumerate() {
        let value = bytemuck::bytes_of(value);
        bytes[index * stride..][..value.len()].copy_from_slice(value);
    }
    bytes
}

/// A material's base color and texture: the whole of what an unlit material
/// draws, and MToon's lit color. Every MToon texture but the matcap and the
/// outline width is sampled through this texture's transform.
fn base_layer(kind: &MaterialKind) -> ([f32; 4], Option<&celesta_plugin::TextureRef>) {
    match kind {
        MaterialKind::Unlit(unlit) => (unlit.base_color, unlit.base_color_texture.as_ref()),
        MaterialKind::Mtoon(mtoon) => (mtoon.base_color, mtoon.base_color_texture.as_ref()),
    }
}

/// What `state` draws its material with: its world matrix and normal
/// matrix, the base color, and the rows of its texture transform, which
/// scales, then rotates, then offsets coordinates as
/// `KHR_texture_transform`'s sample code does. A rotation turns `(1, 0)`
/// toward `(0, 1)`. The frame's overrides replace the material's values.
/// An MToon material adds its factors.
fn draw_uniform(material: &Material, state: &DrawState) -> DrawUniform {
    let (base_color, texture) = base_layer(&material.kind);
    let transform = texture.and_then(|texture| texture.transform);
    let (offset, rotation, scale) = transform.map_or(([0.0; 2], 0.0, [1.0; 2]), |transform| {
        (transform.offset, transform.rotation, transform.scale)
    });
    let offset = state.overrides.texture_offset.unwrap_or(offset);
    let scale = state.overrides.texture_scale.unwrap_or(scale);
    let (sin, cos) = rotation.sin_cos();
    let cutoff = match material.alpha_mode {
        AlphaMode::Mask { cutoff } => cutoff,
        AlphaMode::Opaque | AlphaMode::Blend => 0.0,
    };
    let uniform = DrawUniform {
        world: state.world,
        normal_matrix: normal_matrix(&state.world),
        base_color: state.overrides.base_color.unwrap_or(base_color),
        uv_u: [cos * scale[0], -sin * scale[1], offset[0], cutoff],
        uv_v: [sin * scale[0], cos * scale[1], offset[1], 0.0],
        ..bytemuck::Zeroable::zeroed()
    };
    match &material.kind {
        MaterialKind::Unlit(_) => uniform,
        MaterialKind::Mtoon(mtoon) => mtoon_factors(mtoon, uniform),
    }
}

/// `uniform` with `mtoon`'s factors.
fn mtoon_factors(mtoon: &MtoonMaterial, uniform: DrawUniform) -> DrawUniform {
    let with = |[r, g, b]: [f32; 3], w: f32| [r, g, b, w];
    DrawUniform {
        shade: with(mtoon.shade_color_factor, mtoon.shading_shift_factor),
        emissive: with(
            mtoon.emissive_factor,
            mtoon.shading_shift_texture.map_or(0.0, |slot| slot.scale),
        ),
        matcap: with(mtoon.matcap_factor, mtoon.shading_toony_factor),
        rim: with(
            mtoon.parametric_rim_color_factor,
            mtoon.rim_lighting_mix_factor,
        ),
        rim_shape: [
            mtoon.parametric_rim_fresnel_power_factor,
            mtoon.parametric_rim_lift_factor,
            mtoon.gi_equalization_factor,
            mtoon.normal_texture.map_or(0.0, |slot| slot.scale),
        ],
        outline: with(
            mtoon.outline_color_factor,
            mtoon.outline_lighting_mix_factor,
        ),
        outline_width: [
            mtoon.outline_width_factor,
            match mtoon.outline_width_mode {
                OutlineWidthMode::None => 0.0,
                OutlineWidthMode::WorldCoordinates => 1.0,
                OutlineWidthMode::ScreenCoordinates => 2.0,
            },
            0.0,
            0.0,
        ],
        uv_animation: [
            mtoon.uv_animation_scroll_x_speed_factor,
            mtoon.uv_animation_scroll_y_speed_factor,
            mtoon.uv_animation_rotation_speed_factor / std::f32::consts::TAU,
            if mtoon.normal_texture.is_some() {
                1.0
            } else {
                0.0
            },
        ],
        ..uniform
    }
}

impl GpuRenderer {
    /// `frame` of `scene` on a `width` by `height` canvas, once the frame is
    /// checked against the model, drawn with and into `resources`.
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
        if let Some(message) = invalid_scene_frame(&scene.model, frame) {
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
        let mut skin_params = Vec::with_capacity(queue.len());
        let mut draw_uniforms = Vec::with_capacity(queue.len());
        for state in &queue {
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
            draw_uniforms.push(draw_uniform(&model.materials[info.material], state));
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

        let params_stride = aligned(std::mem::size_of::<SkinParams>(), alignment);
        let draw_stride = aligned(std::mem::size_of::<DrawUniform>(), alignment);
        let frame_uniform = frame_uniform(frame, model.canvas[0] / model.canvas[1]);
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
                &strided(&draw_uniforms, draw_stride),
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
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform_binding(
                            draw_buffer.buffer(),
                            std::mem::size_of::<DrawUniform>(),
                        ),
                    }],
                }),
            });
        }
        let FrameGroups {
            frame: frame_group,
            view: view_group,
            draws: draw_group,
        } = groups.as_ref().expect("made above");

        let size = (width, height);
        if !targets.contains_key(&size) {
            make_room(targets, used, size, *target_texels);
        }
        used.insert(size);
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
            let multisampled = target(
                "Celesta scene color",
                COLOR_FORMAT,
                SAMPLES,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
            );
            let resolved = target(
                "Celesta scene resolved color",
                COLOR_FORMAT,
                1,
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            );
            let depth = target(
                "Celesta scene depth",
                DEPTH_FORMAT,
                SAMPLES,
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
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: multisampled,
                    depth_slice: None,
                    resolve_target: Some(resolved),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Discard,
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
            for (slot, state) in queue.iter().enumerate() {
                let info = &model.draws[state.index];
                let geometry = &scene.geometries[info.geometry];
                let material = &model.materials[info.material];
                let key = PipelineKey::main(material, determinant(&state.world) < 0.0);
                let pipeline = pipelines
                    .draw
                    .get(&key)
                    .expect("prepared when the model loaded");
                pass.set_pipeline(pipeline);
                pass.set_bind_group(1, draw_group, &[(slot * draw_stride) as u32]);
                pass.set_bind_group(2, &scene.materials[info.material], &[]);
                pass.set_vertex_buffer(0, scene.draws[state.index].skinned.slice(..));
                pass.set_vertex_buffer(1, geometry.uvs.slice(..));
                pass.set_index_buffer(geometry.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..geometry.index_count, 0, 0..1);
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
    fn mtoon_slots_and_numbers_are_checked() {
        let mut model = bare_model(3, 1);
        fn mtoon(model: &mut SceneModel) -> &mut celesta_plugin::MtoonMaterial {
            match &mut model.materials[0].kind {
                MaterialKind::Mtoon(mtoon) => mtoon,
                MaterialKind::Unlit(_) => unreachable!("made MToon below"),
            }
        }
        model.materials[0].kind = MaterialKind::Mtoon(celesta_plugin::MtoonMaterial::default());
        assert_eq!(invalid_scene(&model), None);

        let slot = Some(celesta_plugin::TextureSlot { texture: 1 });
        let scaled = Some(celesta_plugin::ScaledTextureSlot {
            texture: 1,
            scale: 1.0,
        });
        type Set = fn(
            &mut celesta_plugin::MtoonMaterial,
            Option<celesta_plugin::TextureSlot>,
            Option<celesta_plugin::ScaledTextureSlot>,
        );
        let cases: [(&str, Set); 9] = [
            ("baseColorTexture", |m, _, _| {
                m.base_color_texture = Some(celesta_plugin::TextureRef {
                    texture: 1,
                    transform: None,
                })
            }),
            ("shadeMultiplyTexture", |m, slot, _| {
                m.shade_multiply_texture = slot
            }),
            ("shadingShiftTexture", |m, _, scaled| {
                m.shading_shift_texture = scaled
            }),
            ("normalTexture", |m, _, scaled| m.normal_texture = scaled),
            ("emissiveTexture", |m, slot, _| m.emissive_texture = slot),
            ("matcapTexture", |m, slot, _| m.matcap_texture = slot),
            ("rimMultiplyTexture", |m, slot, _| {
                m.rim_multiply_texture = slot
            }),
            ("outlineWidthMultiplyTexture", |m, slot, _| {
                m.outline_width_multiply_texture = slot
            }),
            ("uvAnimationMaskTexture", |m, slot, _| {
                m.uv_animation_mask_texture = slot
            }),
        ];
        for (name, set) in cases {
            let mut model = model.clone();
            set(mtoon(&mut model), slot, scaled);
            assert_eq!(
                invalid_scene(&model),
                Some(format!("material 0's {name} uses texture 1, of 0"))
            );
        }

        let not_finite: [fn(&mut celesta_plugin::MtoonMaterial); 4] = [
            |m| m.shading_toony_factor = f32::NAN,
            |m| m.base_color[3] = f32::INFINITY,
            |m| m.uv_animation_rotation_speed_factor = f32::NEG_INFINITY,
            |m| {
                m.normal_texture = Some(celesta_plugin::ScaledTextureSlot {
                    texture: 0,
                    scale: f32::NAN,
                })
            },
        ];
        model.textures.push(celesta_plugin::SceneTexture {
            source: celesta_plugin::TextureSource::Rgba {
                width: 1,
                height: 1,
                pixels: std::sync::Arc::new(vec![255; 4]),
            },
            color_space: ColorSpace::Linear,
            sampler: Default::default(),
        });
        for set in not_finite {
            let mut model = model.clone();
            set(mtoon(&mut model));
            assert_eq!(
                invalid_scene(&model).as_deref(),
                Some("material 0's mtoon parameters are not finite")
            );
        }
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
        // bytes, and their 288-byte draw uniforms twice that.
        assert_eq!(oversized_uniforms(1000, 256, 512_000), None);
        let message = oversized_uniforms(1000, 256, 511_999).expect("too large");
        assert!(
            message.contains("draw uniforms take 512000 bytes"),
            "{message}"
        );
        let message = oversized_uniforms(1001, 256, 512_000).expect("too large");
        assert!(message.contains("512512 bytes"), "{message}");
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
