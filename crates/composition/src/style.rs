use crate::Point;
use crate::tagged::TypeTagged;
use serde::{Deserialize, Deserializer, Serialize};

// Deserialized through `PaintDef`, which keeps the same serde field attributes.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Paint {
    Solid {
        color: String,
    },
    /// Colors blend along the line from `start` to `end`, in the painted
    /// layer's local pixels (a rect's top-left, or a text layer's layout box
    /// top-left). Beyond either end the nearest stop's color continues.
    Linear {
        start: Point,
        end: Point,
        stops: Vec<GradientStop>,
    },
    /// Colors blend outward from `center` to `radius`, in the same space as
    /// `Linear`.
    Radial {
        center: Point,
        radius: f64,
        stops: Vec<GradientStop>,
    },
}

impl<'de> Deserialize<'de> for Paint {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        PaintDef::deserialize(TypeTagged(deserializer))
    }
}

/// `Paint` without the `type` tag, which [`TypeTagged`] reads instead.
#[derive(Deserialize)]
#[serde(
    remote = "Paint",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum PaintDef {
    Solid {
        color: String,
    },
    Linear {
        start: Point,
        end: Point,
        stops: Vec<GradientStop>,
    },
    Radial {
        center: Point,
        radius: f64,
        stops: Vec<GradientStop>,
    },
}

/// A gradient color at `offset` (0 at the start/center, 1 at the end/radius).
/// The color may carry alpha (`#RRGGBBAA`), so a gradient can fade out.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct GradientStop {
    pub offset: f64,
    pub color: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct TextStyle {
    /// Language tag for font fallback, such as `ja`, `ja-JP`, or `zh-Hant`.
    /// When omitted, uses the system locale. Does not select a BudouX model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_weight: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Paint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<Stroke>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<TextAlign>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_height: Option<f64>,
    /// Extra space after each glyph, in px. Negative values tighten.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub letter_spacing: Option<f64>,
    /// Solid color overrides in Unicode code-point ranges of the complete text.
    /// Ranges must be ordered and non-overlapping. A shaped cluster uses the
    /// color at its first code point, so color changes never split shaping.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub color_runs: Vec<TextColorRun>,
    /// Reveal clusters beginning before this code-point offset, retaining the
    /// complete text's layout. Omitted means all text is visible.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible_characters: Option<usize>,
    /// Where a line may wrap when the text has a `maxWidth`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_break: Option<LineBreak>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Stroke {
    pub paint: Paint,
    pub width: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub enum LineBreak {
    /// Between any two characters a line may break between (UAX #14), such
    /// as any two Japanese characters.
    #[default]
    Normal,
    /// Only between phrases ([BudouX](https://github.com/google/budoux)'s
    /// Japanese model) and at spaces, so a Japanese word or a particle is
    /// not split across lines. A phrase wider than `maxWidth` wraps inside
    /// itself as `Normal` text does.
    Phrase,
}

/// A solid color over a half-open Unicode code-point range in the full text.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct TextColorRun {
    pub start: usize,
    pub end: usize,
    pub color: String,
}
