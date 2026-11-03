//! What a content plugin draws in 3D: a scene of skinned, morphing
//! geometry with materials, seen through a camera the plugin places.
//!
//! A scene is right-handed, +Y up, in metres, as glTF is; the camera looks
//! down its −Z. Matrices are `f32` 4×4, column-major. Texture coordinates
//! start at the texture's top-left, as in glTF and unlike 2D meshes. Colors
//! are linear unless a field says otherwise.

use crate::mesh::TextureSource;
use serde::{Deserialize, Serialize};

/// A scene's fixed part, sent once when its assets are opened.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneModel {
    /// The layer's size in pixels; the camera's aspect is this canvas's.
    pub canvas: [f32; 2],
    pub textures: Vec<SceneTexture>,
    pub geometries: Vec<Geometry>,
    pub skins: Vec<Skin>,
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
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "alphaMode", rename_all = "camelCase")]
pub enum AlphaMode {
    Opaque,
    Mask {
        #[serde(rename = "alphaCutoff")]
        cutoff: f32,
    },
    Blend,
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
    /// A blended material also writes depth. Default `false`.
    pub transparent_with_z_write: bool,
    /// Moves the draw within its render queue. Default `0`.
    pub render_queue_offset_number: i32,
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
            transparent_with_z_write: false,
            render_queue_offset_number: 0,
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
    /// The first is the main light. Only the first 4 shade, Unity's default
    /// pixel light count.
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
    pub overrides: MaterialOverrides,
}

/// Values that replace the draw's material's for this frame, as material
/// animation sets them: VRM 1.0's `materialColorBind` and
/// `textureTransformBind` targets. By default none do. The MToon-only ones,
/// from `shade_color` on, are ignored on an unlit draw.
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
            alpha_mode: AlphaMode::Blend,
            double_sided: true,
            kind: MaterialKind::Mtoon(MtoonMaterial {
                transparent_with_z_write: true,
                render_queue_offset_number: -3,
                outline_width_mode: OutlineWidthMode::WorldCoordinates,
                ..MtoonMaterial::default()
            }),
        };
        let json = serde_json::to_string(&material).unwrap();
        for expected in [
            r#""kind":"mtoon""#,
            r#""transparentWithZWrite":true"#,
            r#""renderQueueOffsetNumber":-3"#,
            r#""outlineWidthMode":"worldCoordinates""#,
            r#""uvAnimationRotationSpeedFactor":0.0"#,
        ] {
            assert!(json.contains(expected), "{expected} in {json}");
        }
        assert_eq!(serde_json::from_str::<Material>(&json).unwrap(), material);
    }
}
