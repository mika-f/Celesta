use serde::{Deserialize, Serialize};
use std::fmt;

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
