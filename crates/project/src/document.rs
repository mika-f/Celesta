use crate::asset::Asset;
use crate::character::Character;
use crate::error::LoadError;
use crate::timeline::Track;
use crate::{AssetId, CharacterId, PropertyValue, Rational, Time, TimeError};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

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
pub struct ProjectVersion(pub(crate) u8);

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
    /// resolves against. Nothing in `celesta-project` or `celesta-evaluator`
    /// reads this — it exists so a GUI editor knows which Node process to
    /// query for a registered component's property schema (see
    /// `@celesta/react`'s `registerComponent`/`ComponentPropertySchema`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub react_entry: Option<String>,
}
