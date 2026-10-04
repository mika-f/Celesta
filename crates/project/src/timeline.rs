use crate::character::LipSyncCue;
use crate::{
    Animatable, AssetId, BlendMode, CharacterId, ItemId, PropertyValue, TextStyle, Time, TimeRange,
    TrackId, Transform,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: TrackId,
    pub name: String,
    pub kind: TrackKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub muted: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solo: Option<bool>,
    pub items: Vec<TimelineItem>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub enum TrackKind {
    Video,
    Audio,
    Overlay,
    Dialogue,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct TimelineItem {
    pub id: ItemId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub range: TimeRange,
    pub content: TimelineContent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<Transform>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<Animatable<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blend_mode: Option<BlendMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effects: Option<TimelineEffects>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct TimelineEffects {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blur: Option<Animatable<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow: Option<TimelineShadow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glow: Option<TimelineGlow>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct TimelineShadow {
    pub color: Animatable<String>,
    pub blur: Animatable<f64>,
    pub offset_x: Animatable<f64>,
    pub offset_y: Animatable<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct TimelineGlow {
    pub color: Animatable<String>,
    pub blur: Animatable<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TimelineContent {
    Video {
        asset: AssetId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_range: Option<SourceRange>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        playback_rate: Option<Animatable<f64>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        volume: Option<Animatable<f64>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        muted: Option<bool>,
    },
    Audio {
        asset: AssetId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_range: Option<SourceRange>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        playback_rate: Option<Animatable<f64>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        volume: Option<Animatable<f64>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        muted: Option<bool>,
    },
    Image {
        asset: AssetId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        width: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        height: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fit: Option<celesta_composition::ImageFit>,
    },
    Text {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        style: Option<TextStyle>,
    },
    Dialogue {
        character: CharacterId,
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        audio: Option<AssetId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        volume: Option<Animatable<f64>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expression: Option<String>,
        /// Frame-aligned mouth shapes generated from this clip's voice waveform.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        lip_sync: Vec<LipSyncCue>,
    },
    Component {
        component: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        props: Option<BTreeMap<String, PropertyValue>>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SourceRange {
    pub start: Time,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<Time>,
}
