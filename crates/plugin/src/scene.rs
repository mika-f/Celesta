//! What a content plugin draws in 3D: a scene of skinned, morphing
//! geometry with materials, seen through a camera the plugin places.
//!
//! A scene is right-handed, +Y up, in metres, as glTF is; the camera looks
//! down its −Z. Matrices are `f32` 4×4, column-major. Texture coordinates
//! start at the texture's top-left, as in glTF and unlike 2D meshes. Colors
//! are linear unless a field says otherwise.

use crate::mesh::TextureSource;
use serde::{Deserialize, Serialize};

/// The most shaders one scene model has.
pub const MAX_SCENE_SHADERS: usize = 32;

/// The most bytes of WGSL one scene shader has.
pub const MAX_SHADER_BYTES: usize = 262_144;

/// The most texture slots one scene shader has: WebGPU's default 16
/// sampled textures and 16 samplers a stage, 4 kept for Celesta.
pub const MAX_MATERIAL_TEXTURES: usize = 12;

/// The most bytes a scene shader's `Params` takes.
pub const MAX_MATERIAL_PARAMS: usize = 512;

/// A scene's fixed part, sent once when its assets are opened.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneModel {
    /// The layer's size in pixels; the camera's aspect is this canvas's.
    pub canvas: [f32; 2],
    pub textures: Vec<SceneTexture>,
    pub geometries: Vec<Geometry>,
    pub skins: Vec<Skin>,
    /// The WGSL the model's custom materials are drawn with, each sent
    /// once however many materials use it. At most `MAX_SCENE_SHADERS`.
    pub shaders: Vec<SceneShader>,
    pub materials: Vec<Material>,
    /// What can be drawn, each a geometry with a material: the units frames
    /// place and Celesta sorts transparent draws by.
    pub draws: Vec<DrawInfo>,
}

/// A texture's pixels, straight alpha, and how materials sample it.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneTexture {
    pub source: TextureSource,
    pub color_space: ColorSpace,
    pub sampler: Sampler,
}

/// How a texture's values are encoded: sRGB colors, which Celesta decodes
/// to linear when sampling, or linear data such as normals.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ColorSpace {
    Srgb,
    Linear,
}

/// How a texture is sampled, as a glTF sampler says. By default it repeats,
/// filters linearly, and has mipmaps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sampler {
    pub wrap_s: Wrap,
    pub wrap_t: Wrap,
    pub mag_filter: Filter,
    pub min_filter: Filter,
    /// Celesta builds mipmaps and samples between them when minifying.
    pub mipmaps: bool,
}

impl Default for Sampler {
    fn default() -> Self {
        Self {
            wrap_s: Wrap::default(),
            wrap_t: Wrap::default(),
            mag_filter: Filter::default(),
            min_filter: Filter::default(),
            mipmaps: true,
        }
    }
}

/// What a texture coordinate outside 0 to 1 samples.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Wrap {
    #[default]
    Repeat,
    MirroredRepeat,
    ClampToEdge,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Filter {
    Nearest,
    #[default]
    Linear,
}

/// An indexed triangle list in its rest pose. Every vertex stream has one
/// entry per vertex.
#[derive(Clone, Debug, PartialEq)]
pub struct Geometry {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// Texture coordinates, `v` counting down from the top row.
    pub uvs: Vec<[f32; 2]>,
    /// Tangents with the bitangent's sign in `w`, where a material needs
    /// them.
    pub tangents: Option<Vec<[f32; 4]>>,
    /// The four joints of the draw's skin that move each vertex, and how
    /// much each does. A skinned geometry has both.
    pub joints: Option<Vec<[u16; 4]>>,
    pub weights: Option<Vec<[f32; 4]>>,
    /// Morph targets, which a frame's morph weights blend in.
    pub targets: Vec<MorphTarget>,
    /// Triangles, three vertex indices each, counter-clockwise when they
    /// face front.
    pub indices: Vec<u32>,
}

/// A morph target's offsets from the rest pose, one per vertex. `normals`
/// is empty when the target leaves the normals as they are.
#[derive(Clone, Debug, PartialEq)]
pub struct MorphTarget {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
}

/// A skin's joint count. The plugin keeps the inverse bind matrices and
/// sends final skinning matrices each frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skin {
    pub joints: u32,
}

/// How a draw's surface looks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Material {
    #[serde(flatten)]
    pub alpha_mode: AlphaMode,
    /// Back faces are drawn too.
    pub double_sided: bool,
    #[serde(flatten)]
    pub kind: MaterialKind,
}

/// How a material's alpha is used, as in glTF: ignored, cut off below
/// `cutoff`, or blended over what is behind.
///
/// A blended draw is in Unity's render queue 3000 moved by `queue_offset`
/// clamped to −9..0, after the opaque (2000) and masked (2450) ones, and
/// writes no depth. With `depth_write` it is in queue 2501 moved by
/// `queue_offset` clamped to 0..9, and writes depth. Blended draws in one
/// queue go back to front. A blended pixel with alpha below 2⁻¹⁰ is not
/// drawn. These are MToon's `transparentWithZWrite` and
/// `renderQueueOffsetNumber`, for every material kind.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "alphaMode", rename_all = "camelCase")]
pub enum AlphaMode {
    Opaque,
    Mask {
        #[serde(rename = "alphaCutoff")]
        cutoff: f32,
    },
    Blend {
        /// Default `false`.
        #[serde(
            default,
            rename = "depthWrite",
            skip_serializing_if = "std::ops::Not::not"
        )]
        depth_write: bool,
        /// Default `0`.
        #[serde(default, rename = "queueOffset", skip_serializing_if = "is_zero")]
        queue_offset: i32,
    },
}

impl AlphaMode {
    /// Blended, writing no depth, in the render queue's own place.
    pub const BLEND: Self = Self::Blend {
        depth_write: false,
        queue_offset: 0,
    };
}

fn is_zero(value: &i32) -> bool {
    *value == 0
}

/// Which shading a material uses, with that shading's parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
// A scene has a handful of materials, read in place: boxing MToon's
// parameters would only add an indirection.
#[allow(clippy::large_enum_variant)]
pub enum MaterialKind {
    Unlit(UnlitMaterial),
    Mtoon(MtoonMaterial),
    Custom(CustomMaterial),
}

/// A material's WGSL, written against Celesta's scene material interface
/// (`docs/handoff/2026-11-03-plugin-scene-materials.md`), which Celesta
/// checks and compiles once however many materials use it.
///
/// The WGSL declares `struct Params`, the layout of its materials'
/// parameters, at most `MAX_MATERIAL_PARAMS` bytes, and defines
/// `fn material_fragment(fragment: Fragment) -> vec4f`. It may define
/// `fn material_vertex(vertex: Vertex) -> VertexOffset`, and, for materials
/// with an `outline`, `fn material_outline_fragment(fragment: Fragment) ->
/// vec4f` and `fn material_outline_vertex(vertex: Vertex) -> VertexOffset`.
/// Slot `k` of `textures` is its `texture_k` and `sampler_k`. It reads
/// Celesta's `frame`, `draw`, and `params` and binds nothing of its own,
/// and names starting with `celesta` are Celesta's. At most
/// `MAX_SHADER_BYTES` of WGSL and `MAX_MATERIAL_TEXTURES` slots.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneShader {
    /// What Celesta's messages call the shader.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub wgsl: String,
    /// The shader's texture slots: slot `k` is `texture_k` and `sampler_k`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub textures: Vec<ShaderTexture>,
}

/// A shader's texture slot.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShaderTexture {
    /// What Celesta's messages call the slot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// What the slot samples when a material leaves it empty.
    #[serde(default)]
    pub empty: EmptyTexel,
}

/// The 1×1 texture an empty slot samples: opaque white `(1, 1, 1, 1)` or
/// opaque black `(0, 0, 0, 1)`, with Celesta's default sampler.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EmptyTexel {
    #[default]
    White,
    Black,
}

/// A material drawn with one of the model's shaders.
///
/// Celesta draws it with its alpha mode, render queue, and sidedness as
/// any material, and binds `params` and `textures` to the shader's
/// `params` and texture slots. Defaults leave out every optional field.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomMaterial {
    /// Which `SceneModel::shaders` entry draws it.
    pub shader: usize,
    /// The shader's `Params`: exactly as many bytes as its `struct Params`
    /// takes, in WGSL's uniform layout, little-endian. They travel in the
    /// message payload, never in the JSON; a frame's `DrawState::params`
    /// replaces them for one draw.
    #[serde(skip)]
    pub params: Vec<u8>,
    /// The `SceneModel::textures` entry in slot `k` of the shader. Slots
    /// past the end, or `None`, are empty and sample the slot's
    /// `ShaderTexture::empty`. No more than the shader has slots.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub textures: Vec<Option<usize>>,
    /// An outline pass drawn behind the surface with the shader's
    /// `material_outline_vertex` and `material_outline_fragment`, which the
    /// shader must then define.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline: Option<OutlinePass>,
    /// The shader reads `Vertex::tangent`: a draw of this material on a
    /// geometry without tangents is refused. Without it, a geometry
    /// without tangents gives the shader tangents of zero.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub requires_tangents: bool,
}

/// How an outline pass is rasterised: by default it culls front faces,
/// whatever the material's sidedness, and is pushed away from the camera
/// by Unity's `Offset 1, 1`, so the surface stays in front of it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlinePass {
    #[serde(default = "CullFace::front")]
    pub cull: CullFace,
    #[serde(default)]
    pub depth_bias: DepthBias,
}

impl Default for OutlinePass {
    fn default() -> Self {
        Self {
            cull: CullFace::Front,
            depth_bias: DepthBias::default(),
        }
    }
}

/// Which faces a pass leaves out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CullFace {
    None,
    Front,
    Back,
}

impl CullFace {
    fn front() -> Self {
        Self::Front
    }
}

/// How far a pass's depth is pushed away from the camera, as Unity's
/// `Offset slope_scale, constant` and WebGPU's depth bias: `constant`
/// units of depth resolution plus `slope_scale` times the depth's slope.
/// By default `Offset 1, 1`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DepthBias {
    pub constant: i32,
    pub slope_scale: f32,
}

impl Default for DepthBias {
    fn default() -> Self {
        Self {
            constant: 1,
            slope_scale: 1.0,
        }
    }
}

/// A color, multiplied by a texture when it has one, with no lighting.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlitMaterial {
    pub base_color: [f32; 4],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_color_texture: Option<TextureRef>,
}

/// VRM 1.0's toon shading, `VRMC_materials_mtoon`, with the parameters of
/// UniVRM's MToon10 shader. A field left out of the JSON takes UniVRM's
/// default, given on each field.
///
/// Every texture but the matcap and the outline width is sampled at the
/// base color texture's coordinates: `base_color_texture`'s `transform`,
/// and a frame's `texture_offset` and `texture_scale` overrides, move them
/// all, and then the UV animation does, as UniVRM's single `_MainTex_ST`
/// does. That is why the other slots are `TextureSlot`s without a
/// transform. The outline width texture is sampled at the raw texture
/// coordinates, and the matcap at its own, from the view-space normal.
///
/// `shading_shift_texture` is read from its red channel,
/// `outline_width_multiply_texture` from its green, and
/// `uv_animation_mask_texture` from its blue; the others' RGB is used. The
/// plugin sets each texture's `color_space`: sRGB for the base color, shade
/// multiply, emissive, matcap, and rim multiply textures; linear for the
/// normal, shading shift, outline width, and UV animation mask textures.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MtoonMaterial {
    /// Linear RGBA. Default `[1, 1, 1, 1]`.
    pub base_color: [f32; 4],
    /// Its `transform` moves every texture but the matcap and the outline
    /// width.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_color_texture: Option<TextureRef>,
    /// A tangent-space normal map; `scale` is glTF's `normalTexture.scale`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normal_texture: Option<ScaledTextureSlot>,
    /// Linear RGB, which may exceed 1: the file's
    /// `KHR_materials_emissive_strength` is multiplied in. Default
    /// `[0, 0, 0]`.
    pub emissive_factor: [f32; 3],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emissive_texture: Option<TextureSlot>,
    /// The color of the shaded side, linear RGB. Default `[1, 1, 1]`.
    pub shade_color_factor: [f32; 3],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shade_multiply_texture: Option<TextureSlot>,
    /// Shifts where light turns to shade. Default `-0.05`.
    pub shading_shift_factor: f32,
    /// Red channel, times `scale`, added to the shift.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shading_shift_texture: Option<ScaledTextureSlot>,
    /// How sharp the light-to-shade edge is, 0 smooth to 1 hard; above 1
    /// the ramp turns around, as in UniVRM. Default `0.95`.
    pub shading_toony_factor: f32,
    /// How far ambient light is flattened to its average. Default `0.9`.
    pub gi_equalization_factor: f32,
    /// Linear RGB. Default `[0, 0, 0]`.
    pub matcap_factor: [f32; 3],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matcap_texture: Option<TextureSlot>,
    /// Linear RGB. Default `[0, 0, 0]`.
    pub parametric_rim_color_factor: [f32; 3],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rim_multiply_texture: Option<TextureSlot>,
    /// How far the rim is lit by the scene's light. Default `1`.
    pub rim_lighting_mix_factor: f32,
    /// Default `5`.
    pub parametric_rim_fresnel_power_factor: f32,
    /// Default `0`.
    pub parametric_rim_lift_factor: f32,
    /// Default `OutlineWidthMode::None`.
    pub outline_width_mode: OutlineWidthMode,
    /// The outline's width, in `outline_width_mode`'s units (metres for
    /// world coordinates). Default `0`.
    pub outline_width_factor: f32,
    /// Green channel, at the raw texture coordinates.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outline_width_multiply_texture: Option<TextureSlot>,
    /// Linear RGB. Default `[0, 0, 0]`.
    pub outline_color_factor: [f32; 3],
    /// How far the outline is lit by the scene's light. Default `1`.
    pub outline_lighting_mix_factor: f32,
    /// Blue channel: how much the UV animation moves each texel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uv_animation_mask_texture: Option<TextureSlot>,
    /// Texture coordinate units a second. Default `0`.
    pub uv_animation_scroll_x_speed_factor: f32,
    /// Texture coordinate units a second. Default `0`.
    pub uv_animation_scroll_y_speed_factor: f32,
    /// Radians a second. Default `0`.
    pub uv_animation_rotation_speed_factor: f32,
}

impl Default for MtoonMaterial {
    fn default() -> Self {
        Self {
            base_color: [1.0; 4],
            base_color_texture: None,
            normal_texture: None,
            emissive_factor: [0.0; 3],
            emissive_texture: None,
            shade_color_factor: [1.0; 3],
            shade_multiply_texture: None,
            shading_shift_factor: -0.05,
            shading_shift_texture: None,
            shading_toony_factor: 0.95,
            gi_equalization_factor: 0.9,
            matcap_factor: [0.0; 3],
            matcap_texture: None,
            parametric_rim_color_factor: [0.0; 3],
            rim_multiply_texture: None,
            rim_lighting_mix_factor: 1.0,
            parametric_rim_fresnel_power_factor: 5.0,
            parametric_rim_lift_factor: 0.0,
            outline_width_mode: OutlineWidthMode::None,
            outline_width_factor: 0.0,
            outline_width_multiply_texture: None,
            outline_color_factor: [0.0; 3],
            outline_lighting_mix_factor: 1.0,
            uv_animation_mask_texture: None,
            uv_animation_scroll_x_speed_factor: 0.0,
            uv_animation_scroll_y_speed_factor: 0.0,
            uv_animation_rotation_speed_factor: 0.0,
        }
    }
}

/// What an MToon outline's width is measured in, if it has one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OutlineWidthMode {
    /// No outline is drawn.
    #[default]
    None,
    WorldCoordinates,
    ScreenCoordinates,
}

/// A `SceneModel::textures` entry an MToon material samples at its base
/// color texture's coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureSlot {
    pub texture: usize,
}

/// A `TextureSlot` whose values are multiplied by `scale`, 1 by default.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScaledTextureSlot {
    pub texture: usize,
    #[serde(default = "one")]
    pub scale: f32,
}

fn one() -> f32 {
    1.0
}

/// A `SceneModel::textures` entry, sampled at texture coordinates moved by
/// `transform` when there is one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureRef {
    pub texture: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<TextureTransform>,
}

/// Moves texture coordinates as `KHR_texture_transform` does: scaled, then
/// rotated by `rotation` radians, then offset.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureTransform {
    pub offset: [f32; 2],
    pub rotation: f32,
    pub scale: [f32; 2],
}

/// A geometry drawn with a material, and with a skin when the geometry is
/// skinned.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawInfo {
    pub geometry: usize,
    pub material: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skin: Option<usize>,
}

/// A scene at one moment.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneFrame {
    pub camera: Camera,
    pub lights: Lights,
    /// Seconds, for materials that animate their texture coordinates.
    pub time: f64,
    /// Every draw to place, in any order: Celesta draws opaque draws first
    /// and transparent ones back to front.
    pub draws: Vec<DrawState>,
    /// Each skin's skinning matrices, one per joint: the joint's world
    /// matrix times its inverse bind matrix.
    pub skins: Vec<Vec<[f32; 16]>>,
}

/// Where the scene is seen from. `view` takes world space to the camera's.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Camera {
    pub view: [f32; 16],
    pub projection: Projection,
}

/// The camera's lens; the aspect is the canvas's. `yfov` is in radians and
/// `height` in metres.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Projection {
    Perspective { yfov: f32, near: f32, far: f32 },
    Orthographic { height: f32, near: f32, far: f32 },
}

/// What lights the scene. By default nothing does.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lights {
    /// The first is the main light. At most 4, Unity's default pixel light
    /// count: a frame with more is refused.
    pub directional: Vec<DirectionalLight>,
    /// Light from every direction: `ambient[k]` is the RGB of coefficient
    /// `k` of
    /// `SH(n) = c0·x + c1·y + c2·z + c3 + c4·xy + c5·yz + c6·z² + c7·zx + c8·(x² − y²)`
    /// for a normal `n` in the scene's axes. That is Unity's `ShadeSH9`
    /// order and scale: `unity_SHAr`/`Ag`/`Ab` xyzw, then
    /// `unity_SHBr`/`Bg`/`Bb` xyzw, then `unity_SHC` rgb. A flat ambient
    /// color is `c3` alone.
    ///
    /// UniVRM mirrors X between glTF and Unity, so coefficients read from a
    /// Unity scene negate `c0`, `c4`, and `c7`, and its light directions
    /// negate x.
    pub ambient: [[f32; 3]; 9],
}

/// Light from far away. `direction` is where it travels, in world space;
/// shading uses `L = −normalize(direction)`, and `color × intensity` as the
/// light's color.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectionalLight {
    pub direction: [f32; 3],
    pub color: [f32; 3],
    pub intensity: f32,
}

/// A draw at one moment.
///
/// A vertex is morphed first, the weighted target offsets added to its
/// rest position. An unskinned draw then places it at `world × p`; a
/// skinned draw at `world × Σ wᵢ·skinᵢ·p`, over the vertex's joints, so a
/// plugin sends the identity as `world` for a glTF skinned mesh.
#[derive(Clone, Debug, PartialEq)]
pub struct DrawState {
    /// Which `SceneModel::draws` entry this is.
    pub index: usize,
    pub visible: bool,
    pub world: [f32; 16],
    /// One per morph target of the draw's geometry.
    pub morph_weights: Vec<f32>,
    /// The world-space point a transparent draw is sorted by. Without one,
    /// Celesta uses the centre of the geometry's rest bounds, moved by
    /// `world`.
    pub center: Option<[f32; 3]>,
    /// Only a built-in material's draw has them.
    pub overrides: MaterialOverrides,
    /// Replaces a custom material's `CustomMaterial::params` for this draw
    /// in this frame, as material animation does: as many bytes as the
    /// material's shader's `Params` takes. Only a custom material's draw
    /// has them; a later frame without them draws the material's own.
    pub params: Option<Vec<u8>>,
}

/// The unlit material's values for this frame, which replace the draw's
/// material's as material animation sets them: VRM 1.0's
/// `materialColorBind` and `textureTransformBind` targets. By default none
/// do. The MToon-only ones, from `shade_color` on, apply to an MToon draw
/// and are ignored on an unlit draw, but must still be finite there. A
/// custom material's draw has none: its `DrawState::params` replace its
/// parameters instead.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialOverrides {
    /// Replaces the base color.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_color: Option<[f32; 4]>,
    /// Replaces the base color texture's transform's offset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub texture_offset: Option<[f32; 2]>,
    /// Replaces the base color texture's transform's scale.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub texture_scale: Option<[f32; 2]>,
    /// Replaces `MtoonMaterial::shade_color_factor`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shade_color: Option<[f32; 3]>,
    /// Replaces `MtoonMaterial::emissive_factor`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emission_color: Option<[f32; 3]>,
    /// Replaces `MtoonMaterial::matcap_factor`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matcap_color: Option<[f32; 3]>,
    /// Replaces `MtoonMaterial::parametric_rim_color_factor`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rim_color: Option<[f32; 3]>,
    /// Replaces `MtoonMaterial::outline_color_factor`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline_color: Option<[f32; 3]>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_model_default_sampler() {
        assert_eq!(
            Sampler::default(),
            Sampler {
                wrap_s: Wrap::Repeat,
                wrap_t: Wrap::Repeat,
                mag_filter: Filter::Linear,
                min_filter: Filter::Linear,
                mipmaps: true,
            }
        );
    }

    #[test]
    fn alpha_mode_json() {
        let material = |alpha_mode| Material {
            alpha_mode,
            double_sided: false,
            kind: MaterialKind::Unlit(UnlitMaterial {
                base_color: [1.0, 0.5, 0.25, 1.0],
                base_color_texture: None,
            }),
        };
        let masked = material(AlphaMode::Mask { cutoff: 0.5 });
        let json = serde_json::to_string(&masked).unwrap();
        assert!(
            json.contains(r#""alphaMode":"mask","alphaCutoff":0.5"#),
            "{json}"
        );
        assert!(json.contains(r#""kind":"unlit""#), "{json}");
        assert_eq!(serde_json::from_str::<Material>(&json).unwrap(), masked);

        let opaque = material(AlphaMode::Opaque);
        let json = serde_json::to_string(&opaque).unwrap();
        assert!(json.contains(r#""alphaMode":"opaque""#), "{json}");
        assert!(!json.contains("alphaCutoff"), "{json}");
        assert_eq!(serde_json::from_str::<Material>(&json).unwrap(), opaque);
    }

    #[test]
    fn blend_json_carries_depth_write_and_queue_offset() {
        let material: Material = serde_json::from_str(
            r#"{"alphaMode":"blend","doubleSided":false,"kind":"unlit","baseColor":[1,1,1,1]}"#,
        )
        .unwrap();
        assert_eq!(material.alpha_mode, AlphaMode::BLEND);

        let blend = |alpha_mode| Material {
            alpha_mode,
            double_sided: false,
            kind: MaterialKind::Unlit(UnlitMaterial {
                base_color: [1.0; 4],
                base_color_texture: None,
            }),
        };
        let sorted = blend(AlphaMode::Blend {
            depth_write: true,
            queue_offset: -3,
        });
        let json = serde_json::to_string(&sorted).unwrap();
        assert!(
            json.contains(r#""alphaMode":"blend","depthWrite":true,"queueOffset":-3"#),
            "{json}"
        );
        assert_eq!(serde_json::from_str::<Material>(&json).unwrap(), sorted);

        let plain = blend(AlphaMode::BLEND);
        let json = serde_json::to_string(&plain).unwrap();
        assert!(json.contains(r#""alphaMode":"blend""#), "{json}");
        assert!(!json.contains("depthWrite"), "{json}");
        assert!(!json.contains("queueOffset"), "{json}");
        assert_eq!(serde_json::from_str::<Material>(&json).unwrap(), plain);
    }

    #[test]
    fn mtoon_material_defaults_follow_univrm() {
        let material: Material =
            serde_json::from_str(r#"{"alphaMode":"opaque","doubleSided":false,"kind":"mtoon"}"#)
                .unwrap();
        assert_eq!(material.kind, MaterialKind::Mtoon(MtoonMaterial::default()));
        let mtoon = MtoonMaterial::default();
        assert_eq!(mtoon.shading_shift_factor, -0.05);
        assert_eq!(mtoon.shading_toony_factor, 0.95);
        assert_eq!(mtoon.shade_color_factor, [1.0; 3]);
        assert_eq!(mtoon.matcap_factor, [0.0; 3]);
        assert_eq!(mtoon.gi_equalization_factor, 0.9);
        assert_eq!(mtoon.parametric_rim_fresnel_power_factor, 5.0);
        assert_eq!(mtoon.rim_lighting_mix_factor, 1.0);
        assert_eq!(mtoon.outline_lighting_mix_factor, 1.0);
        assert_eq!(mtoon.outline_width_mode, OutlineWidthMode::None);
        assert_eq!(mtoon.base_color, [1.0; 4]);
        assert_eq!(
            serde_json::from_str::<ScaledTextureSlot>(r#"{"texture":2}"#).unwrap(),
            ScaledTextureSlot {
                texture: 2,
                scale: 1.0
            }
        );
    }

    #[test]
    fn mtoon_material_json_names() {
        let material = Material {
            alpha_mode: AlphaMode::Blend {
                depth_write: true,
                queue_offset: -3,
            },
            double_sided: true,
            kind: MaterialKind::Mtoon(MtoonMaterial {
                outline_width_mode: OutlineWidthMode::WorldCoordinates,
                ..MtoonMaterial::default()
            }),
        };
        let json = serde_json::to_string(&material).unwrap();
        for expected in [
            r#""kind":"mtoon""#,
            r#""depthWrite":true"#,
            r#""queueOffset":-3"#,
            r#""outlineWidthMode":"worldCoordinates""#,
            r#""uvAnimationRotationSpeedFactor":0.0"#,
        ] {
            assert!(json.contains(expected), "{expected} in {json}");
        }
        assert_eq!(serde_json::from_str::<Material>(&json).unwrap(), material);
    }

    #[test]
    fn custom_material_json_names() {
        let custom = |params| Material {
            alpha_mode: AlphaMode::Opaque,
            double_sided: false,
            kind: MaterialKind::Custom(CustomMaterial {
                shader: 1,
                params,
                textures: vec![Some(0), None],
                outline: Some(OutlinePass::default()),
                requires_tangents: true,
            }),
        };
        let material = custom(vec![1, 2, 3, 4]);
        let json = serde_json::to_string(&material).unwrap();
        for expected in [
            r#""kind":"custom""#,
            r#""shader":1"#,
            r#""textures":[0,null]"#,
            r#""outline":{"cull":"front","depthBias":{"constant":1,"slopeScale":1.0}}"#,
            r#""requiresTangents":true"#,
        ] {
            assert!(json.contains(expected), "{expected} in {json}");
        }
        assert!(!json.contains("params"), "{json}");
        // The parameters travel in the payload, not in the JSON.
        assert_eq!(
            serde_json::from_str::<Material>(&json).unwrap(),
            custom(Vec::new())
        );

        let bare: Material = serde_json::from_str(
            r#"{"alphaMode":"opaque","doubleSided":false,"kind":"custom","shader":0}"#,
        )
        .unwrap();
        assert_eq!(
            bare.kind,
            MaterialKind::Custom(CustomMaterial {
                shader: 0,
                params: Vec::new(),
                textures: Vec::new(),
                outline: None,
                requires_tangents: false,
            })
        );

        assert_eq!(
            serde_json::from_str::<CustomMaterial>(r#"{"shader":0,"outline":{}}"#)
                .unwrap()
                .outline,
            Some(OutlinePass::default())
        );
        assert_eq!(
            OutlinePass::default(),
            OutlinePass {
                cull: CullFace::Front,
                depth_bias: DepthBias {
                    constant: 1,
                    slope_scale: 1.0,
                },
            }
        );
    }

    #[test]
    fn scene_shader_json_names() {
        let shader: SceneShader =
            serde_json::from_str(r#"{"wgsl":"x","textures":[{"empty":"black"},{}]}"#).unwrap();
        assert_eq!(
            shader,
            SceneShader {
                name: None,
                wgsl: "x".into(),
                textures: vec![
                    ShaderTexture {
                        name: None,
                        empty: EmptyTexel::Black,
                    },
                    ShaderTexture {
                        name: None,
                        empty: EmptyTexel::White,
                    },
                ],
            }
        );
        let named = SceneShader {
            name: Some("toon".into()),
            wgsl: "x".into(),
            textures: Vec::new(),
        };
        let json = serde_json::to_string(&named).unwrap();
        assert_eq!(json, r#"{"name":"toon","wgsl":"x"}"#);
        assert_eq!(serde_json::from_str::<SceneShader>(&json).unwrap(), named);
    }
}
