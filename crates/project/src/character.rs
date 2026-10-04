use crate::{AssetId, TextStyle, Time, Transform};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Character {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub portrait: Option<PortraitDefinition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<SubtitleDefinition>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct PortraitDefinition {
    pub default_expression: String,
    pub expressions: BTreeMap<String, AssetId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<Transform>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lip_sync: Option<LipSyncDefinition>,
}

/// Transparent mouth overlays used by audio-backed dialogue clips.
///
/// Keeping the mouth separate from `expressions` lets lip sync preserve the
/// currently selected facial expression. `transform` is relative to the
/// dialogue item; when omitted the portrait transform is reused so full-size
/// overlays align with the portrait without extra setup.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct LipSyncDefinition {
    pub a: AssetId,
    pub i: AssetId,
    pub u: AssetId,
    pub e: AssetId,
    pub o: AssetId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed: Option<AssetId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<Transform>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub enum MouthShape {
    Closed,
    A,
    I,
    U,
    E,
    O,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct LipSyncCue {
    pub time: Time,
    pub shape: MouthShape,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SubtitleDefinition {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<TextStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<Transform>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_width: Option<f64>,
}
