//! The plugin's `ContentProvider`. It opens nothing yet: the model it will
//! answer with is built up first.

use crate::PLUGIN;
use celesta_plugin::{ContentFrame, ContentProvider, ModelId, Opened, PluginError};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// What a frame's `data` asks of the avatar. The provider (Task 8) adds
/// the camera and lights, and the motion plan its own fields.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PerformanceData {
    /// Expression weights by VRM name (`happy`, `blink`, a custom name).
    #[serde(default)]
    pub expressions: BTreeMap<String, f32>,
    /// Where the eyes look. Absent, they look ahead (yaw 0, pitch 0).
    #[serde(default)]
    pub look_at: Option<LookAtData>,
}

/// Where the eyes look: `"camera"`, a point `[x, y, z]` in model space, or
/// `{ yaw, pitch }` in degrees from the head's forward (positive yaw to the
/// avatar's right, positive pitch up).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum LookAtData {
    Keyword(LookAtKeyword),
    Point([f32; 3]),
    Angles { yaw: f32, pitch: f32 },
}

/// A target named by a word.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum LookAtKeyword {
    /// The frame camera's eye.
    #[serde(rename = "camera")]
    Camera,
}

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
