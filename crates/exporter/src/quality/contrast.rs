//! Text contrast measured on rendered pixels. Each text layer's frame is
//! rendered three more times: with the text's fill black and then white,
//! fully opaque (the pixels that differ are where its glyphs show, past
//! whatever covers them), and without the text (what is behind them). The
//! frame's own pixels there are compared with that backdrop pixel by pixel.

use std::collections::BTreeMap;

use celesta_composition::{
    Color, Layer, LayerContent, LayerFilter, Paint, Rational, Scene, TextStroke, TextStyle,
};

use super::luminance::{contrast_ratio, relative_luminance};
use super::scene::{Affine, halo_colors};
use super::{
    FrameSpan, LARGE_TEXT_CONTRAST, Measured, QualityCode, QualityIssue, TEXT_CONTRAST,
    describe_spans, push_frame,
};

/// A glyph pixel is one whose black and white probes differ by at least
/// this share of the most any pixel does: where the glyph shows whole.
const CORE_SHARE: u32 = 90;
/// With fewer core pixels than this (thin or small text), pixels at half
/// of it count too.
const MIN_CORE_PIXELS: usize = 16;
/// Probes differing by less than this anywhere show no glyph: the text is
/// covered or off the canvas.
const MIN_PROBE_DIFFERENCE: u8 = 16;
/// The fills the probes paint the text with.
pub const PROBE_FILLS: [&str; 2] = ["#000000", "#FFFFFF"];
/// The share of glyph pixels allowed below the limit: the measure is the
/// 10th percentile of their ratios.
const PERCENTILE: f64 = 0.1;

const LARGE_TEXT_PX: f64 = 24.0;
const LARGE_BOLD_TEXT_PX: f64 = 18.66;
const DEFAULT_FONT_SIZE: f64 = 32.0;

/// A text layer of one frame to measure.
#[derive(Clone, Debug, PartialEq)]
pub struct TextSample {
    pub id: String,
    /// The start of its text, for messages.
    pub text: String,
    /// The contrast it needs: 3 for large text, else 4.5.
    pub limit: f64,
    /// Its opacity through its groups, and how much they scale it; a text
    /// whose either differs from the frame before is still moving in.
    pub opacity: f64,
    pub scale: f64,
    halos: Vec<[u8; 3]>,
}

impl TextSample {
    /// The visible text layers of `scene`, in drawing order.
    pub fn of(scene: &Scene) -> Vec<Self> {
        let short_side = f64::from(scene.width.min(scene.height).max(1));
        let mut samples = Vec::new();
        collect(
            &scene.layers,
            Affine::IDENTITY,
            1.0,
            short_side,
            &mut samples,
        );
        samples
    }

    /// Whether it holds still against `before`, the same layer's sample
    /// in the frame before (`None` when it was not there).
    pub fn steady(&self, before: Option<&Self>) -> bool {
        before.is_none_or(|before| {
            (before.opacity - self.opacity).abs() <= 1e-3
                && (before.scale - self.scale).abs() <= 1e-3
        })
    }
}

fn collect(
    layers: &[Layer],
    parent: Affine,
    opacity: f64,
    short_side: f64,
    out: &mut Vec<TextSample>,
) {
    for layer in layers {
        let opacity = opacity * layer.opacity.clamp(0.0, 1.0);
        let world = parent.then(Affine::of(&layer.transform));
        if opacity < 0.01 || world.scale() < 1e-6 {
            continue;
        }
        match &layer.content {
            LayerContent::Group { layers, .. } => collect(layers, world, opacity, short_side, out),
            LayerContent::Text { text, style, .. } => {
                let shown: String = match style.visible_characters {
                    Some(visible) => text.chars().take(visible).collect(),
                    None => text.clone(),
                };
                if shown.trim().is_empty() || layer.id.is_empty() || !draws(style) {
                    continue;
                }
                out.push(TextSample {
                    id: layer.id.clone(),
                    text: snippet(&shown),
                    limit: limit(style, world.scale(), short_side),
                    opacity,
                    scale: world.scale(),
                    halos: halo_colors(layer, style),
                });
            }
            _ => {}
        }
    }
}

/// Whether a text draws anything: its fill or color runs, or else its
/// strokes. A text with neither draws nothing for the probes to find,
/// though they paint it.
fn draws(style: &TextStyle) -> bool {
    fill_shows(style) || stroke_shows(style)
}

/// Whether a color, as written, is drawn: not fully transparent.
fn opaque_at_all(color: &str) -> bool {
    Color::from_hex(color).is_none_or(|color| color.alpha > 0)
}

/// Whether a paint draws anything.
fn paint_shows(paint: &Paint) -> bool {
    match paint {
        Paint::Solid { color } => opaque_at_all(color),
        Paint::Linear { stops, .. } | Paint::Radial { stops, .. } => {
            stops.is_empty() || stops.iter().any(|stop| opaque_at_all(&stop.color))
        }
    }
}

/// Whether a text's strokes draw anything.
fn stroke_shows(style: &TextStyle) -> bool {
    style.stroke.as_ref().is_some_and(|stroke| {
        stroke
            .strokes()
            .iter()
            .any(|stroke| stroke.width > 0.0 && paint_shows(&stroke.paint))
    })
}

/// Whether any of a text's fill is drawn: a fully transparent one (and no
/// color run) is not.
fn fill_shows(style: &TextStyle) -> bool {
    style.fill.as_ref().is_none_or(paint_shows)
        || style.color_runs.iter().any(|run| opaque_at_all(&run.color))
}

fn snippet(text: &str) -> String {
    let mut sample: String = text.chars().take(24).collect();
    if text.chars().count() > 24 {
        sample.push('…');
    }
    sample
}

/// 3:1 for text at least 24 px (18.66 px bold) on a 1080-pixel short side,
/// else 4.5:1.
fn limit(style: &TextStyle, scale: f64, short_side: f64) -> f64 {
    let size = style.font_size.unwrap_or(DEFAULT_FONT_SIZE).abs() * scale * 1080.0 / short_side;
    let bold = style.font_weight.unwrap_or(400) >= 700;
    if size >= LARGE_TEXT_PX || (bold && size >= LARGE_BOLD_TEXT_PX) {
        LARGE_TEXT_CONTRAST
    } else {
        TEXT_CONTRAST
    }
}

/// `scene` without layer `id`, wherever it is.
pub fn without_layer(scene: &Scene, id: &str) -> Scene {
    fn strip(layers: &[Layer], id: &str) -> Vec<Layer> {
        layers
            .iter()
            .filter(|layer| layer.id != id)
            .map(|layer| match &layer.content {
                LayerContent::Group {
                    layers,
                    clip,
                    mask,
                    motion_blur,
                } => Layer {
                    content: LayerContent::Group {
                        layers: strip(layers, id),
                        clip: *clip,
                        mask: mask.clone(),
                        motion_blur: *motion_blur,
                    },
                    ..layer.clone()
                },
                _ => layer.clone(),
            })
            .collect()
    }
    Scene {
        layers: strip(&scene.layers, id),
        ..scene.clone()
    }
}

/// `scene` with text layer `id` painted solid `color` where it draws: its
/// fill, without color runs, or for a text with a transparent fill and a
/// stroke (an outline), its strokes. Its own color filters are dropped
/// (filters that move pixels stay, as they move the glyphs) and its own
/// opacity is 1 (its groups keep theirs): a probe for where it shows.
/// `None` without such a text layer.
pub fn with_probe_fill(scene: &Scene, id: &str, color: &str) -> Option<Scene> {
    fn paint(layers: &mut [Layer], id: &str, color: &str) -> bool {
        layers.iter_mut().any(|layer| match &mut layer.content {
            LayerContent::Text { style, .. } if layer.id == id => {
                let solid = Paint::Solid {
                    color: color.to_owned(),
                };
                if fill_shows(style) {
                    style.fill = Some(solid);
                    style.color_runs.clear();
                } else if let Some(stroke) = &mut style.stroke {
                    let strokes = match stroke {
                        TextStroke::One(stroke) => std::slice::from_mut(stroke),
                        TextStroke::Many(strokes) => strokes.as_mut_slice(),
                    };
                    for stroke in strokes {
                        stroke.paint = solid.clone();
                    }
                }
                layer.opacity = 1.0;
                // A tint or another color filter could paint both probes
                // the same; where the glyphs are does not depend on it.
                layer.effects.filters.retain(moves_pixels);
                true
            }
            LayerContent::Group { layers, .. } => paint(layers, id, color),
            _ => false,
        })
    }
    let mut probe = scene.clone();
    paint(&mut probe.layers, id, color).then_some(probe)
}

/// Whether `filter` moves pixels rather than only changing their colors, so
/// a probe keeps it to find the glyphs where they show. A custom shader may
/// do either, so it stays.
fn moves_pixels(filter: &LayerFilter) -> bool {
    match filter {
        LayerFilter::Shader(_)
        | LayerFilter::Pixelate { .. }
        | LayerFilter::Halftone { .. }
        | LayerFilter::ChromaticAberration { .. }
        | LayerFilter::Wave { .. }
        | LayerFilter::ZoomBlur { .. }
        | LayerFilter::ChromaKey { .. } => true,
        LayerFilter::Brightness { .. }
        | LayerFilter::Contrast { .. }
        | LayerFilter::Saturation { .. }
        | LayerFilter::Hue { .. }
        | LayerFilter::Grayscale { .. }
        | LayerFilter::Invert { .. }
        | LayerFilter::Tint { .. }
        | LayerFilter::Vignette { .. }
        | LayerFilter::Noise { .. }
        | LayerFilter::Scanlines { .. } => false,
    }
}

/// Text contrast over the sampled frames of a check.
pub struct TextContrast {
    frame_rate: Rational,
    records: BTreeMap<String, Record>,
}

struct Record {
    text: String,
    worst: f64,
    limit: f64,
    failing: Vec<(u64, u64)>,
}

impl TextContrast {
    pub fn new(frame_rate: Rational) -> Self {
        Self {
            frame_rate,
            records: BTreeMap::new(),
        }
    }

    /// Measures `sample` at composition frame `frame` from renders of the
    /// same size, as RGBA8: `full`, the frame; `hidden`, the frame without
    /// the text; `probes`, the frame with the text's fill as
    /// [`PROBE_FILLS`] ([`with_probe_fill`]). Returns the measured ratio,
    /// `None` when none of the text shows.
    pub fn measure(
        &mut self,
        frame: u64,
        sample: &TextSample,
        full: &[u8],
        hidden: &[u8],
        probes: [&[u8]; 2],
    ) -> Option<f64> {
        let ratio = glyph_contrast(full, hidden, probes, &sample.halos)?;
        let record = self
            .records
            .entry(sample.id.clone())
            .or_insert_with(|| Record {
                text: sample.text.clone(),
                worst: f64::INFINITY,
                limit: sample.limit,
                failing: Vec::new(),
            });
        if ratio < sample.limit {
            push_frame(&mut record.failing, frame);
            if ratio < record.worst {
                record.worst = ratio;
                record.limit = sample.limit;
            }
        }
        Some(ratio)
    }

    pub fn finish(self) -> Vec<QualityIssue> {
        self.records
            .into_iter()
            .filter(|(_, record)| !record.failing.is_empty())
            .map(|(id, record)| {
                let spans: Vec<FrameSpan> = record
                    .failing
                    .iter()
                    .map(|&(from, to)| FrameSpan::new(from, to, self.frame_rate))
                    .collect();
                let mut issue = QualityIssue::new(
                    QualityCode::LowContrastText,
                    Measured::Pixels,
                    format!(
                        "text layer {id:?} ({:?}) has {:.2}:1 contrast against what is behind it (90 % of its glyph pixels reach it), below {}:1, at sampled {}; darken or lighten the text or what is behind it, or give it an outline or a backing panel",
                        record.text,
                        record.worst,
                        record.limit,
                        describe_spans(&spans)
                    ),
                );
                issue.frames = spans;
                issue.layer = Some(id);
                issue.value = Some((record.worst * 100.0).round() / 100.0);
                issue.limit = Some(record.limit);
                issue
            })
            .collect()
    }
}

/// The RGBA8 pixels of `rgba`.
fn pixels(rgba: &[u8]) -> std::slice::Iter<'_, [u8; 4]> {
    rgba.as_chunks::<4>().0.iter()
}

/// The 10th percentile of the contrast between `full` and `hidden` over the
/// pixels where the probes differ (the glyphs that show), each pixel also
/// credited with its contrast against the text's `halos`.
fn glyph_contrast(
    full: &[u8],
    hidden: &[u8],
    [dark, light]: [&[u8]; 2],
    halos: &[[u8; 3]],
) -> Option<f64> {
    let halos: Vec<f64> = halos.iter().map(|&halo| relative_luminance(halo)).collect();
    let shown: Vec<u8> = pixels(dark)
        .zip(pixels(light))
        .map(|(dark, light)| {
            (0..3)
                .map(|channel| dark[channel].abs_diff(light[channel]))
                .max()
                .unwrap_or(0)
        })
        .collect();
    let most = shown.iter().copied().max().unwrap_or(0);
    if most < MIN_PROBE_DIFFERENCE {
        return None;
    }
    let ratios_at = |share: u32| -> Vec<f64> {
        let threshold = u32::from(most) * share / 100;
        shown
            .iter()
            .zip(pixels(full).zip(pixels(hidden)))
            .filter(|(shown, _)| u32::from(**shown) >= threshold)
            .map(|(_, (full, hidden))| {
                let glyph = relative_luminance([full[0], full[1], full[2]]);
                let behind = relative_luminance([hidden[0], hidden[1], hidden[2]]);
                halos
                    .iter()
                    .fold(contrast_ratio(glyph, behind), |ratio, &halo| {
                        ratio.max(contrast_ratio(glyph, halo))
                    })
            })
            .collect()
    };
    let mut ratios = ratios_at(CORE_SHARE);
    if ratios.len() < MIN_CORE_PIXELS {
        ratios = ratios_at(CORE_SHARE / 2);
    }
    if ratios.len() < 4 {
        return None;
    }
    ratios.sort_by(f64::total_cmp);
    Some(ratios[((ratios.len() as f64 * PERCENTILE) as usize).min(ratios.len() - 1)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn scene() -> Scene {
        serde_json::from_value(json!({
            "width": 64, "height": 36,
            "frameRate": { "numerator": 30, "denominator": 1 },
            "time": { "value": 0, "timescale": 30 },
            "layers": [
                { "id": "bg", "transform": {}, "content": { "type": "rect", "width": 64, "height": 36 } },
                { "id": "card", "opacity": 0.5, "transform": { "scale": { "x": 2, "y": 2 } }, "content": { "type": "group", "layers": [
                    { "id": "title", "transform": {}, "content": { "type": "text", "text": "Hi", "style": { "fontSize": 12, "fill": { "type": "linear", "start": { "x": 0, "y": 0 }, "end": { "x": 1, "y": 0 }, "stops": [] } } } },
                    { "id": "badge", "transform": {}, "content": { "type": "rect", "width": 4, "height": 4 } }
                ] } },
                { "id": "blank", "transform": {}, "content": { "type": "text", "text": "  ", "style": {} } }
            ],
        }))
        .unwrap()
    }

    #[test]
    fn a_fully_transparent_fill_is_not_sampled() {
        let mut scene = scene();
        scene.layers[2] = serde_json::from_value(json!({
            "id": "clear", "transform": {},
            "content": { "type": "text", "text": "Hidden", "style": { "fill": { "type": "solid", "color": "#00000000" } } },
        }))
        .unwrap();
        let ids: Vec<String> = TextSample::of(&scene)
            .into_iter()
            .map(|sample| sample.id)
            .collect();
        assert_eq!(ids, ["title"]);
        // An outline of a transparent fill draws, and the probes paint it.
        scene.layers[2] = serde_json::from_value(json!({
            "id": "outline", "transform": {},
            "content": { "type": "text", "text": "Outline", "style": {
                "fill": { "type": "solid", "color": "#00000000" },
                "stroke": { "paint": { "type": "solid", "color": "#DDDDDD" }, "width": 3 },
            } },
        }))
        .unwrap();
        assert_eq!(TextSample::of(&scene).len(), 2);
        let probe = with_probe_fill(&scene, "outline", "#000000").unwrap();
        let LayerContent::Text { style, .. } = &probe.layers[2].content else {
            panic!("a text");
        };
        assert_eq!(
            style.fill,
            Some(Paint::Solid {
                color: "#00000000".into()
            })
        );
        assert_eq!(
            style.stroke.as_ref().unwrap().strokes()[0].paint,
            Paint::Solid {
                color: "#000000".into()
            }
        );
        // Half transparent, it draws and is measured.
        scene.layers[2] = serde_json::from_value(json!({
            "id": "faint", "transform": {},
            "content": { "type": "text", "text": "Faint", "style": { "fill": { "type": "solid", "color": "#FFFFFF40" } } },
        }))
        .unwrap();
        assert_eq!(TextSample::of(&scene).len(), 2);
    }

    #[test]
    fn samples_carry_their_groups_opacity_and_scale() {
        let samples = TextSample::of(&scene());
        assert_eq!(samples.len(), 1, "{samples:?}");
        let sample = &samples[0];
        assert_eq!(sample.id, "title");
        assert_eq!((sample.opacity, sample.scale), (0.5, 2.0));
        // 12 px scaled twice on a 36-pixel short side is large.
        assert_eq!(sample.limit, LARGE_TEXT_CONTRAST);
        let mut moving = sample.clone();
        moving.opacity = 0.4;
        assert!(sample.steady(Some(sample)) && sample.steady(None));
        assert!(!sample.steady(Some(&moving)));
    }

    #[test]
    fn probes_paint_the_text_and_without_removes_it() {
        let scene = scene();
        let probe = with_probe_fill(&scene, "title", "#000000").unwrap();
        let LayerContent::Group { layers, .. } = &probe.layers[1].content else {
            panic!("a group");
        };
        // The group keeps its opacity; the text is opaque and solid.
        assert_eq!(probe.layers[1].opacity, 0.5);
        assert_eq!(layers[0].opacity, 1.0);
        let LayerContent::Text { style, .. } = &layers[0].content else {
            panic!("a text");
        };
        assert_eq!(
            style.fill,
            Some(Paint::Solid {
                color: "#000000".into()
            })
        );
        assert!(with_probe_fill(&scene, "missing", "#000000").is_none());
        // A tint on the text would paint both probes alike: it is dropped.
        let mut tinted = scene.clone();
        let LayerContent::Group { layers, .. } = &mut tinted.layers[1].content else {
            panic!("a group");
        };
        // A wave moves the glyphs, so it stays.
        layers[0].effects.filters = vec![
            serde_json::from_value(json!({ "type": "tint", "color": "#FF0000", "amount": 1 }))
                .unwrap(),
            serde_json::from_value(json!({
                "type": "wave", "amplitude": 20, "wavelength": 400, "phase": 90, "angle": 0,
            }))
            .unwrap(),
        ];
        let probe = with_probe_fill(&tinted, "title", "#FFFFFF").unwrap();
        let LayerContent::Group { layers, .. } = &probe.layers[1].content else {
            panic!("a group");
        };
        assert_eq!(layers[0].effects.filters.len(), 1);
        assert!(matches!(
            layers[0].effects.filters[0],
            LayerFilter::Wave { .. }
        ));
        assert!(with_probe_fill(&scene, "badge", "#000000").is_none());

        let hidden = without_layer(&scene, "title");
        let LayerContent::Group { layers, .. } = &hidden.layers[1].content else {
            panic!("a group");
        };
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].id, "badge");
        assert_eq!(hidden.layers.len(), 3);
    }

    /// RGBA8 of `n` pixels of `color`.
    fn pixels(n: usize, color: [u8; 4]) -> Vec<u8> {
        color.repeat(n)
    }

    /// Black and white probes of `n` pixels, the text showing on the first
    /// `shown` of them.
    fn probes(n: usize, shown: usize) -> [Vec<u8>; 2] {
        let probe = |color: u8| {
            let mut pixels = pixels(shown, [color, color, color, 255]);
            pixels.extend(pixels_of(n - shown, [90, 90, 90, 255]));
            pixels
        };
        [probe(0), probe(255)]
    }

    fn pixels_of(n: usize, color: [u8; 4]) -> Vec<u8> {
        color.repeat(n)
    }

    fn contrast(
        full: &[u8],
        hidden: &[u8],
        probes: &[Vec<u8>; 2],
        halos: &[[u8; 3]],
    ) -> Option<f64> {
        glyph_contrast(full, hidden, [&probes[0], &probes[1]], halos)
    }

    #[test]
    fn glyph_pixels_are_compared_with_what_is_behind_them() {
        let n = 100;
        let probes = probes(n, n);
        // Grey text on a slightly lighter grey.
        let full = pixels(n, [110, 110, 110, 255]);
        let hidden = pixels(n, [140, 140, 140, 255]);
        let ratio = contrast(&full, &hidden, &probes, &[]).unwrap();
        assert!(ratio < 1.6, "{ratio}");
        // A black outline makes white-on-white text read.
        let white = pixels(n, [255, 255, 255, 255]);
        assert!(contrast(&white, &white, &probes, &[[0, 0, 0]]).unwrap() > 20.0);
        // Covered by a card, the probes match: nothing shows to measure.
        assert!(contrast(&full, &hidden, &self::probes(n, 0), &[]).is_none());

        let mut text = TextContrast::new(Rational::new(30, 1));
        let sample = TextSample {
            id: "t".into(),
            text: "Hi".into(),
            limit: TEXT_CONTRAST,
            opacity: 1.0,
            scale: 1.0,
            halos: Vec::new(),
        };
        for frame in [30, 15] {
            text.measure(frame, &sample, &full, &hidden, [&probes[0], &probes[1]]);
        }
        let issues = text.finish();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, QualityCode::LowContrastText);
        assert_eq!(issues[0].measured, Measured::Pixels);
        assert_eq!(issues[0].frames.len(), 2);
        assert_eq!(issues[0].frames[0].from, 15);
    }

    #[test]
    fn faded_text_is_measured_where_it_shows() {
        // White text held at 20 % on black: its pixels are a dark grey,
        // though the probes, drawn opaque, find all of them.
        let n = 100;
        let full = pixels(n, [51, 51, 51, 255]);
        let hidden = pixels(n, [0, 0, 0, 255]);
        let ratio = contrast(&full, &hidden, &probes(n, n), &[]).unwrap();
        assert!(ratio < 2.5, "{ratio}");
        // Only the part that shows counts: 30 pixels peek out from under a
        // card and are white on black.
        let mut full = pixels(30, [255, 255, 255, 255]);
        full.extend(pixels(70, [200, 30, 30, 255]));
        let mut hidden = pixels(30, [0, 0, 0, 255]);
        hidden.extend(pixels(70, [200, 30, 30, 255]));
        assert!(contrast(&full, &hidden, &probes(n, 30), &[]).unwrap() > 20.0);
    }

    #[test]
    fn the_worst_tenth_of_the_glyphs_decides() {
        // 80 pixels over black, 20 over white: white text.
        let probes = probes(100, 100);
        let full = pixels(100, [255, 255, 255, 255]);
        let mut hidden = pixels(80, [0, 0, 0, 255]);
        hidden.extend(pixels(20, [255, 255, 255, 255]));
        assert!(contrast(&full, &hidden, &probes, &[]).unwrap() < 1.01);
        let mut hidden = pixels(95, [0, 0, 0, 255]);
        hidden.extend(pixels(5, [255, 255, 255, 255]));
        assert!(contrast(&full, &hidden, &probes, &[]).unwrap() > 20.0);
    }
}
