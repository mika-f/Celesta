//! A Celesta content plugin that draws VRM 1.0 avatars as 3D scenes.
//!
//! It runs as a process of its own (see `src/main.rs`): Celesta asks it to
//! open a `.vrm` file and then for frames, and it answers with a `scene`
//! model and frames over `celesta_plugin`'s protocol.
//!
//! The scene follows glTF's conventions: right-handed, +Y up, in metres,
//! the avatar facing +Z with its left at +X, matrices column-major, and
//! texture coordinates from the top-left.

mod camera;
mod document;
mod expressions;
mod first_person;
mod geometry;
mod humanoid;
mod look_at;
mod materials;
mod meta;
mod model;
pub mod mtoon;
mod nodes;
mod provider;
#[cfg(any(test, feature = "synthetic"))]
pub mod synthetic;

pub use camera::{
    CameraData, CameraPreset, ExplicitCamera, LightsData, REFERENCE_AMBIENT, REFERENCE_LIGHT,
    reference_lights,
};
pub use document::LoadError;
pub use expressions::{ExpressionKey, Preset as ExpressionPreset};
pub use humanoid::{HumanBone, Humanoid};
pub use model::{CANVAS, FrameError, VrmModel};
pub use nodes::{NodeTree, Pose, Trs};
pub use provider::{LookAtData, LookAtKeyword, PerformanceData, VrmProvider};

/// The plugin's id, as its manifest and its layers name it.
pub const PLUGIN: &str = "vrm";
