//! A Celesta content plugin that draws VRM 1.0 avatars as 3D scenes.
//!
//! It runs as a process of its own (see `src/main.rs`): Celesta asks it to
//! open a `.vrm` file and then for frames, and it answers with a `scene`
//! model and frames over `celesta_plugin`'s protocol.
//!
//! The scene follows glTF's conventions: right-handed, +Y up, in metres,
//! the avatar facing +Z with its left at +X, matrices column-major, and
//! texture coordinates from the top-left.

mod document;
mod geometry;
mod humanoid;
mod nodes;
mod provider;
#[cfg(any(test, feature = "synthetic"))]
pub mod synthetic;

pub use document::LoadError;
pub use humanoid::{HumanBone, Humanoid};
pub use nodes::{NodeTree, Pose, Trs};
pub use provider::VrmProvider;

/// The plugin's id, as its manifest and its layers name it.
pub const PLUGIN: &str = "vrm";
