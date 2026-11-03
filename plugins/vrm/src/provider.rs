//! The plugin's `ContentProvider`, and the `data` a frame reads.
//!
//! `data` is not trusted: the layer's JavaScript checks its numbers, but
//! the plugin checks them again, naming the key of any that is not finite
//! or out of its range.

use crate::PLUGIN;
use crate::camera::{CameraData, LightsData};
use crate::model::VrmModel;
use celesta_plugin::{ContentFrame, ContentProvider, ModelId, Opened, PluginError};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

/// What a frame's `data` asks of the avatar; the motion plan adds its own
/// fields.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PerformanceData {
    /// The layer's size in pixels, `[width, height]`, which the frame
    /// answers as its canvas and the camera presets frame. Absent, the
    /// model's (`CANVAS`).
    #[serde(default)]
    pub canvas: Option<[f32; 2]>,
    /// Expression weights by VRM name (`happy`, `blink`, a custom name).
    #[serde(default)]
    pub expressions: BTreeMap<String, f32>,
    /// Where the eyes look. Absent, they look ahead (yaw 0, pitch 0).
    #[serde(default)]
    pub look_at: Option<LookAtData>,
    /// A framing of the humanoid, or a camera of its own. Absent, `full`.
    #[serde(default)]
    pub camera: Option<CameraData>,
    /// Lights in place of the reference light.
    #[serde(default)]
    pub lights: Option<LightsData>,
}

/// Where the eyes look: `"camera"`, a point `[x, y, z]` in model space, or
/// `{ yaw, pitch }` in degrees from the head's forward (positive yaw to the
/// avatar's right, positive pitch up).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(
    untagged,
    deny_unknown_fields,
    expecting = "lookAt is \"camera\", [x, y, z], or { yaw, pitch } with finite numbers"
)]
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

/// The plugin's [`ContentProvider`]: what its process serves, and what
/// tests call in-process.
///
/// - `open` takes one asset, `model`, the `.vrm`, and answers its scene
///   and its notices (the usage terms first).
/// - `frame` reads `data` as [`PerformanceData`] (`null` is the default)
///   and answers the scene at `time`.
/// - `close` drops the model.
#[derive(Default)]
pub struct VrmProvider {
    models: HashMap<ModelId, VrmModel>,
    next: u64,
}

/// The asset the `.vrm` is.
const MODEL: &str = "model";

impl ContentProvider for VrmProvider {
    fn open(
        &mut self,
        plugin: &str,
        assets: &BTreeMap<String, PathBuf>,
    ) -> Result<Opened, PluginError> {
        if plugin != PLUGIN {
            return Err(PluginError::UnknownPlugin(plugin.to_owned()));
        }
        if let Some(name) = assets.keys().find(|name| *name != MODEL) {
            return Err(failed(format!("unknown asset {name:?}")));
        }
        let path = assets
            .get(MODEL)
            .ok_or_else(|| failed(format!("asset {MODEL:?} (the .vrm) is missing")))?;
        let mut model =
            VrmModel::load(path).map_err(|error| failed(format!("{}: {error}", path.display())))?;
        self.next += 1;
        let id = ModelId(self.next);
        let opened = Opened {
            model: id,
            content: model.take_scene_model().into(),
            notices: model.notices().to_vec(),
        };
        self.models.insert(id, model);
        Ok(opened)
    }

    fn frame(
        &mut self,
        model: ModelId,
        time: f64,
        data: &Value,
    ) -> Result<ContentFrame, PluginError> {
        let vrm = self
            .models
            .get_mut(&model)
            .ok_or_else(|| failed(format!("no model {}", model.0)))?;
        let data = performance_data(data)?;
        vrm.frame(time, &data)
            .map(ContentFrame::from)
            .map_err(|error| failed(error.to_string()))
    }

    fn close(&mut self, model: ModelId) {
        self.models.remove(&model);
    }
}

/// `data` as [`PerformanceData`]: `null` is the default. Nothing inside it
/// is `null`, which is how JSON writes a number that is not finite.
fn performance_data(data: &Value) -> Result<PerformanceData, PluginError> {
    if data.is_null() {
        return Ok(PerformanceData::default());
    }
    if let Some(key) = null_inside(data, String::new()) {
        return Err(failed(format!(
            "invalid data: {key} is null, not a finite number (JSON writes NaN and infinities as null)"
        )));
    }
    PerformanceData::deserialize(data).map_err(|error| failed(format!("invalid data: {error}")))
}

/// The key of the first `null` inside `value`, which is at `key`: `a.b`
/// for an object's field, `a[1]` for an array's item.
fn null_inside(value: &Value, key: String) -> Option<String> {
    match value {
        Value::Null => Some(key),
        Value::Object(fields) => fields.iter().find_map(|(name, value)| {
            let key = if key.is_empty() {
                name.clone()
            } else {
                format!("{key}.{name}")
            };
            null_inside(value, key)
        }),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .find_map(|(index, value)| null_inside(value, format!("{key}[{index}]"))),
        _ => None,
    }
}

fn failed(message: impl Into<String>) -> PluginError {
    PluginError::Failed {
        plugin: PLUGIN.into(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::SyntheticVrm;
    use celesta_plugin::ContentModel;
    use serde_json::json;
    use tempfile::TempDir;

    /// The synthetic avatar written to a file of a fresh directory.
    fn avatar_file() -> (TempDir, PathBuf) {
        let directory = TempDir::new().unwrap();
        let path = directory.path().join("avatar.vrm");
        std::fs::write(&path, SyntheticVrm::avatar().glb()).unwrap();
        (directory, path)
    }

    fn assets(entries: &[(&str, &PathBuf)]) -> BTreeMap<String, PathBuf> {
        entries
            .iter()
            .map(|(name, path)| (name.to_string(), path.to_path_buf()))
            .collect()
    }

    fn message(error: PluginError) -> String {
        match error {
            PluginError::Failed { plugin, message } => {
                assert_eq!(plugin, PLUGIN);
                message
            }
            other => panic!("not a failure of the plugin: {other}"),
        }
    }

    #[test]
    fn opens_the_model_and_answers_its_frames() {
        let (_directory, path) = avatar_file();
        let mut provider = VrmProvider::default();
        let opened = provider.open(PLUGIN, &assets(&[("model", &path)])).unwrap();
        let mut model = VrmModel::load(&path).unwrap();
        let ContentModel::Scene(scene) = &opened.content else {
            panic!("a VRM avatar is a scene: {:?}", opened.content);
        };
        assert_eq!(scene, model.scene_model());
        assert_eq!(opened.notices, model.notices());
        let data = json!({ "expressions": { "happy": 0.5 }, "camera": "face" });
        let frame = provider.frame(opened.model, 1.0, &data).unwrap();
        let expected = model
            .frame(1.0, &serde_json::from_value(data.clone()).unwrap())
            .unwrap();
        assert_eq!(frame, ContentFrame::Scene(expected));
        // Two models of one file are two models.
        let second = provider.open(PLUGIN, &assets(&[("model", &path)])).unwrap();
        assert_ne!(second.model, opened.model);
        provider.close(opened.model);
        assert!(message(provider.frame(opened.model, 0.0, &data).unwrap_err()).contains("model"));
        provider.frame(second.model, 0.0, &data).unwrap();
    }

    #[test]
    fn data_is_checked() {
        let (_directory, path) = avatar_file();
        let mut provider = VrmProvider::default();
        let id = provider
            .open(PLUGIN, &assets(&[("model", &path)]))
            .unwrap()
            .model;
        let refusal = |provider: &mut VrmProvider, data: Value| {
            message(provider.frame(id, 0.0, &data).unwrap_err())
        };
        let message_of = refusal(&mut provider, json!({ "expresions": {} }));
        assert!(message_of.contains("invalid data"), "{message_of}");
        // `null` is the default frame.
        let default = provider.frame(id, 0.0, &Value::Null).unwrap();
        assert_eq!(default, provider.frame(id, 0.0, &json!({})).unwrap());
        let mut model = VrmModel::load(&path).unwrap();
        let expected = model.frame(0.0, &PerformanceData::default()).unwrap();
        assert_eq!(default, ContentFrame::Scene(expected));
        // A number JSON could not write (NaN, ∞) arrives as `null`, named.
        for (data, key) in [
            (
                json!({ "expressions": { "happy": null } }),
                "expressions.happy",
            ),
            (
                json!({ "camera": { "position": [0, null, 2], "target": [0, 0, 0] } }),
                "camera.position[1]",
            ),
            (
                json!({ "lookAt": { "yaw": null, "pitch": 0 } }),
                "lookAt.yaw",
            ),
            (json!({ "canvas": [null, 1080] }), "canvas[0]"),
        ] {
            let found = refusal(&mut provider, data.clone());
            assert!(found.contains("invalid data"), "{data}: {found}");
            assert!(found.contains(key), "{data}: {found}");
        }
        // A frame's own checks, through the provider.
        let found = refusal(&mut provider, json!({ "expressions": { "happy": 1e39 } }));
        assert!(found.contains("expression happy is not finite"), "{found}");
        let found = refusal(&mut provider, json!({ "camera": "side" }));
        assert!(found.contains("camera is \"full\""), "{found}");
    }

    #[test]
    fn assets_are_checked() {
        let (_directory, path) = avatar_file();
        let mut provider = VrmProvider::default();
        let found = message(
            provider
                .open(PLUGIN, &assets(&[("model", &path), ("motion", &path)]))
                .unwrap_err(),
        );
        assert!(found.contains("unknown asset \"motion\""), "{found}");
        let found = message(provider.open(PLUGIN, &BTreeMap::new()).unwrap_err());
        assert!(
            found.contains("asset \"model\" (the .vrm) is missing"),
            "{found}"
        );
        let missing = path.with_file_name("missing.vrm");
        let found = message(
            provider
                .open(PLUGIN, &assets(&[("model", &missing)]))
                .unwrap_err(),
        );
        assert!(found.contains("missing.vrm"), "{found}");
        let error = provider
            .open("turntable", &assets(&[("model", &path)]))
            .unwrap_err();
        assert!(matches!(error, PluginError::UnknownPlugin(_)), "{error}");
    }
}
