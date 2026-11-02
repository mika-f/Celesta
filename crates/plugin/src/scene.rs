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
pub enum MaterialKind {
    Unlit(UnlitMaterial),
}

/// A color, multiplied by a texture when it has one, with no lighting.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlitMaterial {
    pub base_color: [f32; 4],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_color_texture: Option<TextureRef>,
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
    /// The first is the main light.
    pub directional: Vec<DirectionalLight>,
    /// Light from every direction as nine spherical harmonics coefficients
    /// per channel, in Unity's `ShadeSH9` order and scale.
    pub ambient: [[f32; 3]; 9],
}

/// Light from far away. `direction` is where it travels, in world space.
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
/// animation sets them. By default none do.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_color: Option<[f32; 4]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub texture_offset: Option<[f32; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub texture_scale: Option<[f32; 2]>,
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
}
