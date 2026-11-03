//! The interface Celesta's renderer asks for plugin content through.

use crate::mesh::{MeshFrame, MeshModel};
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
    ) -> Result<(ModelId, MeshModel), PluginError>;

    /// The model `time` seconds into its own clock, as `data` directs.
    fn frame(
        &mut self,
        model: ModelId,
        time: f64,
        data: &serde_json::Value,
    ) -> Result<MeshFrame, PluginError>;

    /// Forgets a model the renderer no longer draws.
    fn close(&mut self, _model: ModelId) {}
}
