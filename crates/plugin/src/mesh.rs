//! What a content plugin draws: textured triangle meshes in canvas pixels.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

/// A model's fixed part, sent once when its assets are opened.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshModel {
    /// The layer's size in pixels; mesh positions are in this space.
    pub canvas: [f32; 2],
    pub textures: Vec<TextureSource>,
    pub meshes: Vec<MeshInfo>,
}

/// A texture's pixels, straight alpha. Celesta premultiplies them and
/// builds mipmaps.
#[derive(Clone, Debug, PartialEq)]
pub enum TextureSource {
    /// An image file Celesta decodes.
    Path(PathBuf),
    Rgba {
        width: u32,
        height: u32,
        pixels: Arc<Vec<u8>>,
    },
}

/// How a mesh composites onto the meshes drawn before it, on premultiplied
/// color: source over, added to them, or multiplying them. Additive and
/// multiplicative meshes leave the canvas's alpha as it is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MeshBlend {
    #[default]
    Normal,
    Additive,
    Multiplicative,
}

/// Clips a mesh to where other meshes draw: the union of their textures'
/// alpha, or everywhere else when `inverted`. The masking meshes' own
/// visibility does not matter.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshMask {
    pub meshes: Vec<usize>,
    #[serde(default)]
    pub inverted: bool,
}

/// What does not change for a mesh while the model animates.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshInfo {
    pub texture: usize,
    pub blend: MeshBlend,
    /// Back faces are drawn too. A triangle faces front when its vertices
    /// turn counter-clockwise as seen on the canvas: top-left, bottom-left,
    /// bottom-right.
    pub double_sided: bool,
    pub mask: Option<MeshMask>,
    /// Texture coordinates, `v` counting up from the bottom row, each
    /// finite.
    pub uvs: Vec<[f32; 2]>,
    /// Triangles, three vertex indices each.
    pub indices: Vec<u32>,
}

/// A model at one moment: every mesh, in drawing order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshFrame {
    pub meshes: Vec<MeshState>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MeshState {
    /// Which `MeshModel::meshes` entry this is.
    pub index: usize,
    /// A hidden mesh is not drawn but still masks others.
    pub visible: bool,
    pub opacity: f32,
    /// Multiplies the texture's color.
    pub multiply: [f32; 3],
    /// Screens over it, after `multiply`.
    pub screen: [f32; 3],
    /// One per vertex, in canvas pixels, y down.
    pub positions: Vec<[f32; 2]>,
}
