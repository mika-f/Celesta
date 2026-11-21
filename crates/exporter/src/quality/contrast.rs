//! Text contrast measured on rendered pixels: each text layer is rendered
//! alone (for where its glyphs are) and the frame without it (for what is
//! behind them), and the frame's own pixels over the glyphs are compared
//! with that backdrop pixel by pixel.

use std::collections::BTreeMap;

use celesta_composition::{Layer, LayerContent, Rational, Scene, TextStyle};

use super::luminance::{contrast_ratio, relative_luminance};
use super::scene::{Affine, halo_colors};
use super::{
    FrameSpan, LARGE_TEXT_CONTRAST, Measured, QualityCode, QualityIssue, TEXT_CONTRAST,
    describe_spans, push_frame,
};

/// A glyph pixel is one the text alone covers at least this much (of 255).
const CORE_ALPHA: u8 = 230;
/// With fewer core pixels than this (thin or small text), half-covered
/// ones count too.
const MIN_CORE_PIXELS: usize = 16;
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
                if shown.trim().is_empty() || layer.id.is_empty() {
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

/// `scene` with only layer `id` and the groups it is in, each keeping its
/// transform, opacity, clip, mask, and effects; `None` without such a layer.
pub fn only_layer(scene: &Scene, id: &str) -> Option<Scene> {
    fn keep(layers: &[Layer], id: &str) -> Option<Layer> {
        layers.iter().find_map(|layer| {
            if layer.id == id {
                return Some(layer.clone());
            }
            let LayerContent::Group {
                layers,
                clip,
                mask,
                motion_blur,
            } = &layer.content
            else {
                return None;
            };
            let child = keep(layers, id)?;
            Some(Layer {
                content: LayerContent::Group {
                    layers: vec![child],
                    clip: *clip,
                    mask: mask.clone(),
                    motion_blur: *motion_blur,
                },
                ..layer.clone()
            })
        })
    }
    Some(Scene {
        layers: vec![keep(&scene.layers, id)?],
        ..scene.clone()
    })
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

    /// Measures `sample` at composition frame `frame` from three renders of
    /// the same size, as RGBA8: `full`, the frame; `hidden`, the frame
    /// without the text; `alone`, the text by itself on a transparent
    /// background. Returns the measured ratio, `None` when the text drew
    /// nothing.
    pub fn measure(
        &mut self,
        frame: u64,
        sample: &TextSample,
        full: &[u8],
        hidden: &[u8],
        alone: &[u8],
    ) -> Option<f64> {
        let ratio = glyph_contrast(full, hidden, alone, &sample.halos)?;
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
/// pixels `alone` covers, each pixel also credited with its contrast
/// against the text's `halos`.
fn glyph_contrast(full: &[u8], hidden: &[u8], alone: &[u8], halos: &[[u8; 3]]) -> Option<f64> {
    let halos: Vec<f64> = halos.iter().map(|&halo| relative_luminance(halo)).collect();
    let ratios_at = |threshold: u8| -> Vec<f64> {
        pixels(alone)
            .zip(pixels(full).zip(pixels(hidden)))
            .filter(|(alone, _)| alone[3] >= threshold)
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
    let mut ratios = ratios_at(CORE_ALPHA);
    if ratios.len() < MIN_CORE_PIXELS {
        ratios = ratios_at(128);
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
    fn only_and_without_keep_the_path_to_the_layer() {
        let scene = scene();
        let alone = only_layer(&scene, "title").unwrap();
        assert_eq!(alone.layers.len(), 1);
        assert_eq!(alone.layers[0].id, "card");
        assert_eq!(alone.layers[0].opacity, 0.5);
        let LayerContent::Group { layers, .. } = &alone.layers[0].content else {
            panic!("a group");
        };
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].id, "title");
        assert!(only_layer(&scene, "missing").is_none());

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

    #[test]
    fn glyph_pixels_are_compared_with_what_is_behind_them() {
        let n = 100;
        let alone = pixels(n, [255, 255, 255, 255]);
        // Grey text on a slightly lighter grey.
        let full = pixels(n, [110, 110, 110, 255]);
        let hidden = pixels(n, [140, 140, 140, 255]);
        let ratio = glyph_contrast(&full, &hidden, &alone, &[]).unwrap();
        assert!(ratio < 1.6, "{ratio}");
        // A black outline makes white-on-white text read.
        let white = pixels(n, [255, 255, 255, 255]);
        assert!(glyph_contrast(&white, &white, &alone, &[[0, 0, 0]]).unwrap() > 20.0);
        // Nothing drawn: nothing to measure.
        assert!(glyph_contrast(&full, &hidden, &pixels(n, [0; 4]), &[]).is_none());

        let mut contrast = TextContrast::new(Rational::new(30, 1));
        let sample = TextSample {
            id: "t".into(),
            text: "Hi".into(),
            limit: TEXT_CONTRAST,
            opacity: 1.0,
            scale: 1.0,
            halos: Vec::new(),
        };
        for frame in [15, 30] {
            contrast.measure(frame, &sample, &full, &hidden, &alone);
        }
        let issues = contrast.finish();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, QualityCode::LowContrastText);
        assert_eq!(issues[0].measured, Measured::Pixels);
        assert_eq!(issues[0].frames.len(), 2);
    }

    #[test]
    fn the_worst_tenth_of_the_glyphs_decides() {
        // 80 pixels over black, 20 over white: white text.
        let alone = pixels(100, [255, 255, 255, 255]);
        let full = pixels(100, [255, 255, 255, 255]);
        let mut hidden = pixels(80, [0, 0, 0, 255]);
        hidden.extend(pixels(20, [255, 255, 255, 255]));
        assert!(glyph_contrast(&full, &hidden, &alone, &[]).unwrap() < 1.01);
        let mut hidden = pixels(95, [0, 0, 0, 255]);
        hidden.extend(pixels(5, [255, 255, 255, 255]));
        assert!(glyph_contrast(&full, &hidden, &alone, &[]).unwrap() > 20.0);
    }
}
