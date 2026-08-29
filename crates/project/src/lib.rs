//! Mikan project format v0.

mod validation;

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::Path;

pub use mikan_composition::{
    Animatable, AnimatablePoint, Easing, Keyframe, KeyframeAnimation, Paint, Rational, Stroke,
    TextAlign, TextStyle, Time, TimeError, TimeRange, Transform,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use serde_json::Value;

pub type AssetId = String;
pub type CharacterId = String;
pub type TrackId = String;
pub type ItemId = String;
pub type PropertyValue = Value;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub version: ProjectVersion,
    pub settings: ProjectSettings,
    pub assets: BTreeMap<AssetId, Asset>,
    pub characters: BTreeMap<CharacterId, Character>,
    pub tracks: Vec<Track>,
    pub properties: BTreeMap<String, PropertyValue>,
}

impl Project {
    pub fn from_json(input: &str) -> Result<Self, LoadError> {
        let project: Self = serde_json::from_str(input).map_err(LoadError::Json)?;
        project.validate().map_err(LoadError::Validation)?;
        Ok(project)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, LoadError> {
        let input = fs::read_to_string(path).map_err(LoadError::Io)?;
        Self::from_json(&input)
    }

    /// Serializes the project using the stable, human-readable v0 JSON layout.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        let mut output = serde_json::to_string_pretty(self)?;
        output.push('\n');
        Ok(output)
    }

    pub fn effective_duration(&self) -> Result<Time, TimeError> {
        if let Some(duration) = self.settings.duration {
            return Ok(duration);
        }

        let mut duration = Time::ZERO;
        for item in self.tracks.iter().flat_map(|track| &track.items) {
            let end = item.range.end()?;
            if end.cmp_exact(duration)?.is_gt() {
                duration = end;
            }
        }
        Ok(duration)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export, type = "0"))]
pub struct ProjectVersion(u8);

impl ProjectVersion {
    pub const V0: Self = Self(0);
}

impl Serialize for ProjectVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u8(self.0)
    }
}

impl<'de> Deserialize<'de> for ProjectVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match u8::deserialize(deserializer)? {
            0 => Ok(Self::V0),
            version => Err(de::Error::custom(format_args!(
                "unsupported project version {version}"
            ))),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ProjectSettings {
    pub width: u32,
    pub height: u32,
    pub frame_rate: Rational,
    pub sample_rate: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub master_volume: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<Time>,
    /// Path (relative to the project file, like an asset's) to the `.tsx`
    /// entry a `TimelineContent::Component` item's `component` name
    /// resolves against. Nothing in `mikan-project` or `mikan-evaluator`
    /// reads this — it exists so a GUI editor knows which Node process to
    /// query for a registered component's property schema (see
    /// `@mikan/react`'s `registerComponent`/`ComponentPropertySchema`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub react_entry: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Asset {
    Video {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        source: AssetSource,
    },
    Audio {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        source: AssetSource,
    },
    Image {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        source: AssetSource,
    },
    Font {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        source: AssetSource,
    },
}

impl Asset {
    pub const fn kind(&self) -> AssetKind {
        match self {
            Self::Video { .. } => AssetKind::Video,
            Self::Audio { .. } => AssetKind::Audio,
            Self::Image { .. } => AssetKind::Image,
            Self::Font { .. } => AssetKind::Font,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetKind {
    Video,
    Audio,
    Image,
    Font,
}

impl fmt::Display for AssetKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Video => "video",
            Self::Audio => "audio",
            Self::Image => "image",
            Self::Font => "font",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AssetSource {
    File { path: String },
    Url { url: String },
}

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

#[derive(Debug)]
pub enum LoadError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Validation(ValidationErrors),
}

impl fmt::Display for LoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "could not read project: {error}"),
            Self::Json(error) => write!(formatter, "invalid project JSON: {error}"),
            Self::Validation(errors) => write!(formatter, "invalid project: {errors}"),
        }
    }
}

impl Error for LoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Validation(errors) => Some(errors),
        }
    }
}

pub use validation::{ValidationError, ValidationErrors};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_the_minimal_project() {
        let project =
            Project::from_json(include_str!("../../../examples/minimal.mikan.json")).unwrap();
        assert_eq!(project.version, ProjectVersion::V0);
        assert_eq!(project.effective_duration().unwrap(), Time::ZERO);
    }

    #[test]
    fn calculates_duration_from_the_latest_item_end() {
        let project =
            Project::from_json(include_str!("../../../examples/voiceroid.mikan.json")).unwrap();
        assert_eq!(project.effective_duration().unwrap(), Time::new(8, 1));
    }

    #[test]
    fn pretty_serialization_round_trips() {
        let project =
            Project::from_json(include_str!("../../../examples/voiceroid.mikan.json")).unwrap();

        let json = project.to_json().unwrap();

        assert!(json.ends_with('\n'));
        assert!(json.contains("\n  \"settings\":"));
        assert_eq!(Project::from_json(&json).unwrap(), project);
    }

    #[test]
    fn validates_project_master_volume() {
        let mut project =
            Project::from_json(include_str!("../../../examples/voiceroid.mikan.json")).unwrap();
        project.settings.master_volume = Some(-0.1);

        let errors = project.validate().unwrap_err();

        assert_eq!(errors.as_slice()[0].path, "settings.masterVolume");
    }

    #[test]
    fn lip_sync_configuration_and_cues_round_trip() {
        let mut project =
            Project::from_json(include_str!("../../../examples/voiceroid.mikan.json")).unwrap();
        project
            .characters
            .get_mut("akane")
            .unwrap()
            .portrait
            .as_mut()
            .unwrap()
            .lip_sync = Some(LipSyncDefinition {
            a: "akane-default".to_owned(),
            i: "akane-default".to_owned(),
            u: "akane-default".to_owned(),
            e: "akane-default".to_owned(),
            o: "akane-default".to_owned(),
            closed: Some("akane-default".to_owned()),
            transform: None,
        });
        let TimelineContent::Dialogue { lip_sync, .. } = &mut project.tracks[0].items[0].content
        else {
            panic!("example must contain dialogue")
        };
        *lip_sync = vec![
            LipSyncCue {
                time: Time::ZERO,
                shape: MouthShape::Closed,
            },
            LipSyncCue {
                time: Time::new(1, 2),
                shape: MouthShape::A,
            },
        ];

        let json = project.to_json().unwrap();
        let loaded = Project::from_json(&json).unwrap();

        assert_eq!(loaded, project);
        assert!(json.contains("\"lipSync\""));
        assert!(json.contains("\"shape\": \"a\""));
    }

    #[test]
    fn lip_sync_cues_require_voice_and_strict_time_order() {
        let mut project =
            Project::from_json(include_str!("../../../examples/voiceroid.mikan.json")).unwrap();
        let TimelineContent::Dialogue {
            audio, lip_sync, ..
        } = &mut project.tracks[0].items[0].content
        else {
            panic!("example must contain dialogue")
        };
        *audio = None;
        *lip_sync = vec![
            LipSyncCue {
                time: Time::new(1, 1),
                shape: MouthShape::I,
            },
            LipSyncCue {
                time: Time::new(1, 1),
                shape: MouthShape::Closed,
            },
        ];

        let errors = project.validate().unwrap_err();

        assert!(
            errors
                .as_slice()
                .iter()
                .any(|error| error.path.ends_with("content.lipSync"))
        );
        assert!(errors.as_slice().iter().any(|error| {
            error.path.ends_with("content.lipSync[1].time")
                && error.message.contains("strictly ascending")
        }));
    }
}
