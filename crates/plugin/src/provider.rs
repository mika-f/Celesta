//! The interface Celesta's renderer asks for plugin content through.

use crate::mesh::{MeshFrame, MeshModel};
use crate::scene::{SceneFrame, SceneModel};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// A model a provider opened, valid until the provider drops it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModelId(pub u64);

#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("no plugin provides \"{0}\" content")]
    UnknownPlugin(String),
    /// The plugin refused the request; `message` is its own.
    #[error("plugin \"{plugin}\": {message}")]
    Failed { plugin: String, message: String },
    /// The plugin's process could not be started or stopped answering.
    #[error("plugin \"{plugin}\" is unavailable: {message}")]
    Unavailable { plugin: String, message: String },
}

/// A model's fixed part: 2D meshes on a canvas, or a 3D scene. Its frames
/// are of the same kind.
#[derive(Clone, Debug, PartialEq)]
pub enum ContentModel {
    Meshes(MeshModel),
    Scene(SceneModel),
}

impl From<MeshModel> for ContentModel {
    fn from(model: MeshModel) -> Self {
        Self::Meshes(model)
    }
}

impl From<SceneModel> for ContentModel {
    fn from(model: SceneModel) -> Self {
        Self::Scene(model)
    }
}

/// A model at one moment, of the kind it was opened as.
// A frame is made and drawn one at a time, so a scene's camera and lights
// inline cost less than boxing them would.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq)]
pub enum ContentFrame {
    Meshes(MeshFrame),
    Scene(SceneFrame),
}

impl From<MeshFrame> for ContentFrame {
    fn from(frame: MeshFrame) -> Self {
        Self::Meshes(frame)
    }
}

impl From<SceneFrame> for ContentFrame {
    fn from(frame: SceneFrame) -> Self {
        Self::Scene(frame)
    }
}

/// Something a plugin wants the user to know about a model it opened, such
/// as the model's terms of use. The editor lists it among the project's
/// problems, and the exporter prints it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub level: NoticeLevel,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NoticeLevel {
    Info,
    Warning,
}

/// A model a provider opened: its id, its fixed part, and what the user
/// should know about it.
#[derive(Clone, Debug, PartialEq)]
pub struct Opened {
    pub model: ModelId,
    pub content: ContentModel,
    pub notices: Vec<Notice>,
}

/// Answers for `LayerContent::Plugin` layers. A frame must depend only on
/// the model and the arguments: the renderer asks in any order and as often
/// as it likes.
pub trait ContentProvider: Send {
    /// Opens the model `plugin` makes of `assets` (absolute local paths, by
    /// the names the layer gave them).
    fn open(
        &mut self,
        plugin: &str,
        assets: &BTreeMap<String, PathBuf>,
    ) -> Result<Opened, PluginError>;

    /// The model `time` seconds into its own clock, as `data` directs, in
    /// the kind the model was opened as.
    fn frame(
        &mut self,
        model: ModelId,
        time: f64,
        data: &serde_json::Value,
    ) -> Result<ContentFrame, PluginError>;

    /// Forgets a model the renderer no longer draws.
    fn close(&mut self, _model: ModelId) {}
}
