//! Quality and safety checks an agent that cannot see the preview reads as
//! numbers: flashing (WCAG 2.3.1's general and red flash thresholds), text
//! contrast (WCAG 1.4.3's 4.5:1, 3:1 for large text), scenes and shots
//! shorter than a second, and a blank first frame.
//!
//! Two kinds of check share the codes in [`QualityCode`]:
//!
//! - from layers ([`SceneLint`], [`short_chapters`], [`short_items`]): what
//!   the evaluated scenes say, cheap enough for every export and
//!   inspection;
//! - from pixels ([`PixelLint`], [`TextContrast`]): what the rendered frames
//!   show, which needs every frame rendered, so only `Celesta-export
//!   --check` runs them.

mod contrast;
mod luminance;
mod pixels;
mod scene;

use std::fmt;

use celesta_composition::Rational;
use serde::Serialize;

pub use contrast::{PROBE_FILLS, TextContrast, TextSample, with_probe_fill, without_layer};
pub use luminance::{contrast_ratio, relative_luminance};
pub use pixels::PixelLint;
pub use scene::{SceneLint, short_chapters, short_items};

/// Most flashes (pairs of opposing luminance changes) any one second may
/// hold, from WCAG 2.3.1.
pub const MAX_FLASHES_PER_SECOND: u32 = 3;
/// The contrast text needs against what is behind it (WCAG 1.4.3).
pub const TEXT_CONTRAST: f64 = 4.5;
/// The contrast large text needs.
pub const LARGE_TEXT_CONTRAST: f64 = 3.0;
/// The shortest a scene or shot may last, in seconds.
pub const MIN_SCENE_SECONDS: f64 = 1.0;

/// What a quality issue is about; its text (`flashing`, …) is stable for
/// scripts to branch on.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, strum::IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum QualityCode {
    /// More than three general flashes in a second, over enough of the
    /// picture.
    Flashing,
    /// More than three flashes of saturated red in a second.
    RedFlashing,
    /// Text below 4.5:1 (3:1 for large text) against what is behind it.
    LowContrastText,
    /// A chapter (React) or a video track's item (JSON project) shorter
    /// than a second.
    ShortScene,
    /// The picture between two hard cuts lasts less than a second.
    ShortShot,
    /// Frame 0, the thumbnail and the video's first frame, shows nothing or
    /// one flat color.
    BlankFirstFrame,
}

impl QualityCode {
    pub fn as_str(self) -> &'static str {
        self.into()
    }
}

impl fmt::Display for QualityCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// How an issue was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Measured {
    /// From the evaluated layers (or the timeline), without rendering.
    Layers,
    /// From rendered pixels.
    Pixels,
}

/// Composition frames `[from, to)`, with their times in seconds, rounded
/// to milliseconds.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameSpan {
    pub from: u64,
    /// The frame after the last.
    pub to: u64,
    pub start: f64,
    pub end: f64,
}

impl FrameSpan {
    pub fn new(from: u64, to: u64, frame_rate: Rational) -> Self {
        let rounded = |frame| (seconds(frame, frame_rate) * 1000.0).round() / 1000.0;
        Self {
            from,
            to,
            start: rounded(from),
            end: rounded(to),
        }
    }
}

/// A rectangle of the canvas, in canvas pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl fmt::Display for Region {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}×{} at ({}, {})",
            self.width, self.height, self.x, self.y
        )
    }
}

/// One quality or safety problem: a warning, never a failure.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityIssue {
    pub code: QualityCode,
    /// What is wrong and where, for reading; [`fmt::Display`] puts the code
    /// in front of it.
    pub message: String,
    pub measured: Measured,
    /// The frames it holds on, in order; empty for a whole-timeline issue.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub frames: Vec<FrameSpan>,
    /// The layer it is about (a text layer's id).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
    /// Where on the canvas.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<Region>,
    /// The measured number: flashes in the worst second, a contrast ratio,
    /// or seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    /// What `value` should reach (contrast) or stay within (flashes,
    /// seconds are at least it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<f64>,
}

impl QualityIssue {
    fn new(code: QualityCode, measured: Measured, message: String) -> Self {
        Self {
            code,
            message,
            measured,
            frames: Vec::new(),
            layer: None,
            region: None,
            value: None,
            limit: None,
        }
    }
}

/// `code: message`, as warnings print it.
impl fmt::Display for QualityIssue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

/// The seconds `frame` starts at.
fn seconds(frame: u64, frame_rate: Rational) -> f64 {
    if frame_rate.numerator == 0 {
        return 0.0;
    }
    frame as f64 * f64::from(frame_rate.denominator) / f64::from(frame_rate.numerator)
}

/// Frames in one second, at least one: 30 at 29.97 fps.
fn frames_per_second(frame_rate: Rational) -> u64 {
    if frame_rate.denominator == 0 {
        return 1;
    }
    (f64::from(frame_rate.numerator) / f64::from(frame_rate.denominator))
        .round()
        .max(1.0) as u64
}

/// `frames 30–59 (1.00–2.00 s)` for the spans, at most a few listed.
fn describe_spans(spans: &[FrameSpan]) -> String {
    const LISTED: usize = 4;
    let mut parts: Vec<String> = spans
        .iter()
        .take(LISTED)
        .map(|span| {
            if span.to == span.from + 1 {
                format!("frame {} ({:.2} s)", span.from, span.start)
            } else {
                format!(
                    "frames {}–{} ({:.2}–{:.2} s)",
                    span.from,
                    span.to - 1,
                    span.start,
                    span.end
                )
            }
        })
        .collect();
    if spans.len() > LISTED {
        parts.push(format!("and {} more", spans.len() - LISTED));
    }
    parts.join(", ")
}

/// Adds frame `frame` to `spans`, which stay sorted and joined wherever
/// they touch, whatever order the frames come in.
fn push_frame(spans: &mut Vec<(u64, u64)>, frame: u64) {
    // The first span that ends at or after the frame.
    let index = spans.partition_point(|&(_, to)| to < frame);
    match spans.get_mut(index) {
        Some(span) if span.0 <= frame => span.1 = span.1.max(frame + 1),
        Some(span) if span.0 == frame + 1 => span.0 = frame,
        _ => spans.insert(index, (frame, frame + 1)),
    }
    // A span that now reaches the next joins it.
    if let Some(&(_, to)) = spans.get(index)
        && spans.get(index + 1).is_some_and(|&(from, _)| from <= to)
    {
        let (_, next) = spans.remove(index + 1);
        spans[index].1 = to.max(next);
    }
}

/// The issues that touch composition frames `[from, to)`, and those about
/// no frames: what an export or check of that span is about.
pub(crate) fn within(issues: Vec<QualityIssue>, (from, to): (u64, u64)) -> Vec<QualityIssue> {
    issues
        .into_iter()
        .filter(|issue| {
            issue.frames.is_empty()
                || issue
                    .frames
                    .iter()
                    .any(|span| span.from < to && from < span.to)
        })
        .collect()
}

/// Reports each of `issues` as an [`ExportProgress::Quality`], in a stable
/// order.
pub(crate) fn report(
    mut issues: Vec<QualityIssue>,
    progress: &mut impl FnMut(crate::ExportProgress),
) {
    sort_issues(&mut issues);
    for issue in issues {
        progress(crate::ExportProgress::Quality(issue));
    }
}

/// Sorts `issues` by code, then first frame, then layer, so the same video
/// reports them in the same order.
pub fn sort_issues(issues: &mut [QualityIssue]) {
    issues.sort_by(|a, b| {
        let first = |issue: &QualityIssue| issue.frames.first().map_or(0, |span| span.from);
        (a.code, first(a), &a.layer).cmp(&(b.code, first(b), &b.layer))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_snake_case_and_issues_print_them_first() {
        assert_eq!(QualityCode::LowContrastText.as_str(), "low_contrast_text");
        assert_eq!(
            serde_json::to_value(QualityCode::RedFlashing).unwrap(),
            "red_flashing"
        );
        let issue = QualityIssue::new(
            QualityCode::ShortScene,
            Measured::Layers,
            "chapter \"intro\" lasts 0.50 s".into(),
        );
        assert_eq!(
            issue.to_string(),
            "short_scene: chapter \"intro\" lasts 0.50 s"
        );
        let value = serde_json::to_value(&issue).unwrap();
        assert_eq!(value["code"], "short_scene");
        assert_eq!(value["measured"], "layers");
        assert!(value.get("frames").is_none() && value.get("layer").is_none());
    }

    #[test]
    fn spans_join_adjacent_frames_and_read_in_seconds() {
        let mut spans = Vec::new();
        for frame in [3, 4, 5, 9, 9, 10] {
            push_frame(&mut spans, frame);
        }
        assert_eq!(spans, [(3, 6), (9, 11)]);
        // Frames in any order make the same spans.
        let mut reversed = Vec::new();
        for frame in [10, 9, 9, 5, 3, 4, 7, 6] {
            push_frame(&mut reversed, frame);
        }
        assert_eq!(reversed, [(3, 8), (9, 11)]);
        let rate = Rational::new(30, 1);
        let spans: Vec<FrameSpan> = spans
            .into_iter()
            .map(|(from, to)| FrameSpan::new(from, to, rate))
            .collect();
        assert_eq!(
            describe_spans(&spans),
            "frames 3–5 (0.10–0.20 s), frames 9–10 (0.30–0.37 s)"
        );
        assert_eq!(frames_per_second(Rational::new(30_000, 1_001)), 30);
    }
}
