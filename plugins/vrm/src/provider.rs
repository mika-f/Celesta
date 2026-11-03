//! The plugin's `ContentProvider`. It opens nothing yet: the model it will
//! answer with is built up first.

use crate::PLUGIN;
use celesta_plugin::{ContentFrame, ContentProvider, ModelId, Opened, PluginError};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Default)]
pub struct VrmProvider;

impl ContentProvider for VrmProvider {
    fn open(
        &mut self,
        _plugin: &str,
        _assets: &BTreeMap<String, PathBuf>,
    ) -> Result<Opened, PluginError> {
        Err(not_ready())
    }

    fn frame(
        &mut self,
        _model: ModelId,
        _time: f64,
        _data: &serde_json::Value,
    ) -> Result<ContentFrame, PluginError> {
        Err(not_ready())
    }
}

fn not_ready() -> PluginError {
    PluginError::Failed {
        plugin: PLUGIN.into(),
        message: "not ready".into(),
    }
}
