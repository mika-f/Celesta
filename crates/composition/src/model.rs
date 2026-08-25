use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Animatable, Rational, TextStyle, Time, TimeRange};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scene {
    pub width: u32,
    pub height: u32,
    pub frame_rate: Rational,
    pub time: Time,
    /// Project-provided fonts available to text layout, in addition to system fonts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fonts: Vec<ResolvedAsset>,
    /// Bottom-to-top painter's order.
    pub layers: Vec<Layer>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layer {
    pub id: String,
    pub transform: EvaluatedTransform,
    pub opacity: f64,
    pub content: LayerContent,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LayerContent {
    Video {
        asset: ResolvedAsset,
        timing: MediaTiming,
    },
    Image {
        asset: ResolvedAsset,
    },
    Text {
        text: String,
        style: TextStyle,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_width: Option<f64>,
    },
    Group {
        layers: Vec<Layer>,
    },
    MissingComponent {
        component: String,
        props: BTreeMap<String, Value>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluatedTransform {
    pub position: Point,
    pub scale: Point,
    /// Clockwise rotation in degrees.
    pub rotation: f64,
    /// Normalized coordinates where `(0.5, 0.5)` is the center.
    pub anchor: Point,
}

impl Default for EvaluatedTransform {
    fn default() -> Self {
        Self {
            position: Point { x: 0.0, y: 0.0 },
            scale: Point { x: 1.0, y: 1.0 },
            rotation: 0.0,
            anchor: Point { x: 0.5, y: 0.5 },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedAsset {
    pub id: String,
    pub location: AssetLocation,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AssetLocation {
    File { path: String },
    Url { url: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaTiming {
    pub local_time: Time,
    pub source_start: Time,
    pub source_time_seconds: f64,
    pub playback_rate: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioGraph {
    pub sample_rate: u32,
    #[serde(default = "default_audio_master_volume")]
    pub master_volume: f64,
    pub clips: Vec<AudioClip>,
}

const fn default_audio_master_volume() -> f64 {
    1.0
}

impl Default for AudioGraph {
    fn default() -> Self {
        Self {
            sample_rate: 0,
            master_volume: 1.0,
            clips: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioClip {
    pub id: String,
    pub asset: ResolvedAsset,
    pub range: TimeRange,
    pub source_start: Time,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_duration: Option<Time>,
    pub playback_rate: Animatable<f64>,
    pub volume: Animatable<f64>,
    pub muted: bool,
}
