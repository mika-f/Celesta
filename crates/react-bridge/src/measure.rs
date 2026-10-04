use celesta_composition::{ResolvedAsset, TextStyle};
use celesta_renderer::{TextMetrics, TextRasterizer};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MeasureTextRequest {
    pub(crate) text: String,
    #[serde(default)]
    pub(crate) style: TextStyle,
    #[serde(default)]
    pub(crate) max_width: Option<f64>,
    /// Fonts declared by the composition or explicitly requested in `prepare()`.
    #[serde(default)]
    pub(crate) fonts: Vec<ResolvedAsset>,
}

#[derive(Serialize)]
pub(crate) struct MeasureTextResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) metrics: Option<TextMetricsPayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TextMetricsPayload {
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) ascent: f64,
    pub(crate) descent: f64,
    pub(crate) line_height: f64,
    pub(crate) lines: usize,
    pub(crate) glyphs: Vec<GlyphPayload>,
}

#[derive(Serialize)]
pub(crate) struct GlyphPayload {
    pub(crate) text: String,
    pub(crate) x: f64,
    pub(crate) width: f64,
    pub(crate) line: usize,
}

pub(crate) fn measure_text_response(
    measurer: &mut Option<TextRasterizer>,
    request: &MeasureTextRequest,
) -> MeasureTextResponse {
    let measurer = measurer.get_or_insert_with(TextRasterizer::new);
    match measurer.load_fonts(&request.fonts, Path::new(".")) {
        Ok(()) => MeasureTextResponse {
            metrics: Some(text_metrics_payload(&measurer.measure(
                &request.text,
                &request.style,
                request.max_width,
            ))),
            error: None,
        },
        Err(error) => MeasureTextResponse {
            metrics: None,
            error: Some(format!("could not measure text: {error}")),
        },
    }
}

pub(crate) fn text_metrics_payload(metrics: &TextMetrics) -> TextMetricsPayload {
    TextMetricsPayload {
        width: metrics.width,
        height: metrics.height,
        ascent: metrics.ascent,
        descent: metrics.descent,
        line_height: metrics.line_height,
        lines: metrics.lines,
        glyphs: metrics
            .glyphs
            .iter()
            .map(|glyph| GlyphPayload {
                text: glyph.text.clone(),
                x: glyph.x,
                width: glyph.width,
                line: glyph.line,
            })
            .collect(),
    }
}
