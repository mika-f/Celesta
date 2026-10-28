use serde::{Deserialize, Serialize};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::Display)]
#[strum(serialize_all = "lowercase")]
pub enum AssetKind {
    Video,
    Audio,
    Image,
    Font,
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
