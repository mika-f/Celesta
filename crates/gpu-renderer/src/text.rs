use crate::bounds::PixelBounds;
use crate::error::GpuRenderError;
use crate::layer::PreparedLayer;
use crate::texture::{DecodedImage, LayerTexture};
use crate::transform::{Affine, LayerState};
use celesta_composition::{
    BlendMode, Layer, LayerContent, Point, TextAlign, TextStroke, TextStyle,
};
use celesta_renderer::{TextMetrics, TextRasterizer, TextWindow};

/// A text layer whose texture was not cached when `prepare_layer` reached
/// it, holding what is needed to rasterize it and to replace `item`.
pub(crate) struct PendingText {
    /// Index of its `PreparedItem::PendingText` in the frame's items.
    pub(crate) item: usize,
    pub(crate) key: String,
    pub(crate) text: String,
    pub(crate) style: TextStyle,
    pub(crate) max_width: Option<f64>,
    pub(crate) raster_scale: f32,
    pub(crate) anchor: Point,
    pub(crate) baseline_anchor: bool,
    pub(crate) state: LayerState,
    pub(crate) blend_mode: BlendMode,
    /// The part of it to rasterize, when not all of it fits a texture.
    pub(crate) window: Option<TextWindow>,
    /// The whole text's box in its target, for a windowed text, whose quad
    /// is only the window's.
    pub(crate) shape: Option<PixelBounds>,
    /// Its layout, measured without drawing it.
    pub(crate) size: TextSize,
}

impl PendingText {
    /// The box the text covers once rasterized, placed by its state, from
    /// its measured layout (see `TextPlacement`).
    pub(crate) fn measured_bounds(&self) -> PixelBounds {
        TextPlacement::new(
            &self.size,
            &self.style,
            self.max_width,
            self.anchor,
            self.baseline_anchor,
        )
        .bounds(self.state.transform)
    }
}

/// A text layer's layout, measured once and kept while frames use it
/// (`GpuRenderer::text_sizes`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct TextSize {
    pub(crate) width: f64,
    pub(crate) height: f64,
    /// The first line's top edge to its baseline.
    pub(crate) ascent: f64,
    pub(crate) line_height: f64,
    /// How many lines it is; a raster window needs more than one.
    pub(crate) lines: usize,
    /// `GpuRenderer::texture_generation` when a frame last used it.
    pub(crate) last_used: u64,
}

impl TextSize {
    pub(crate) fn of(metrics: &TextMetrics, generation: u64) -> Self {
        Self {
            width: metrics.width,
            height: metrics.height,
            ascent: metrics.ascent,
            line_height: metrics.line_height,
            lines: metrics.lines,
            last_used: generation,
        }
    }
}

/// How a text layer is laid out (`GpuRenderer::text_layout`): what an
/// editor needs to place a caret in it, in layout units from its first
/// line's top-left.
#[derive(Clone, Debug, PartialEq)]
pub struct TextLayout {
    /// The text it was laid out for.
    pub text: String,
    pub metrics: TextMetrics,
    pub max_width: Option<f64>,
    /// The style's `align`, `Left` when unset.
    pub align: TextAlign,
}

/// Layer `id` among `layers`, their groups' children and their masks'
/// layers.
pub(crate) fn find_layer<'a>(layers: &'a [Layer], id: &str) -> Option<&'a Layer> {
    layers.iter().find_map(|layer| {
        if layer.id == id {
            return Some(layer);
        }
        let LayerContent::Group { layers, mask, .. } = &layer.content else {
            return None;
        };
        find_layer(layers, id).or_else(|| find_layer(&mask.as_ref()?.layers, id))
    })
}

/// How much of a text layer to rasterize.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Windowed {
    /// All of it: its raster fits a texture.
    Whole,
    /// Only this window of it, which its target shows.
    Window(TextWindow),
    /// None: its raster would not fit, and its target shows none of it.
    Hidden,
}

/// The raster pixels a text window's sides are snapped out to, so a text
/// that moves a little from frame to frame keeps its window.
pub(crate) const TEXT_WINDOW_STEP: f64 = 512.0;

/// A text layer laid out in its layer's units: the layout box (as the
/// texture's anchor box) from `(-anchor_x, -anchor_y)`, and how far its
/// glyphs can reach past it: the stroke's width and half the font size,
/// room for glyphs that reach past their line box and for the raster's
/// rounding.
pub(crate) struct TextPlacement {
    width: f64,
    height: f64,
    anchor_x: f64,
    anchor_y: f64,
    reach: f64,
}

impl TextPlacement {
    pub(crate) fn new(
        size: &TextSize,
        style: &TextStyle,
        max_width: Option<f64>,
        anchor: Point,
        baseline_anchor: bool,
    ) -> Self {
        let width = max_width.unwrap_or(size.width);
        let stroke = style.stroke.as_ref().map_or(0.0, TextStroke::width);
        let font_size = style.font_size.unwrap_or(32.0).abs();
        Self {
            width,
            height: size.height,
            anchor_x: anchor.x * width,
            anchor_y: if baseline_anchor {
                size.ascent
            } else {
                anchor.y * size.height
            },
            reach: stroke + font_size / 2.0,
        }
    }

    /// The layout box grown by `reach`, in layout units from its top-left.
    fn reached(&self) -> [f64; 4] {
        [
            -self.reach,
            -self.reach,
            self.width + self.reach,
            self.height + self.reach,
        ]
    }

    /// The box the text covers once rasterized, placed by `transform`.
    pub(crate) fn bounds(&self, transform: Affine) -> PixelBounds {
        let [left, top, right, bottom] = self.reached();
        let t = transform;
        let corners = [(left, top), (right, top), (left, bottom), (right, bottom)].map(|(x, y)| {
            let (x, y) = ((x - self.anchor_x) as f32, (y - self.anchor_y) as f32);
            (t.a * x + t.c * y + t.tx, t.b * x + t.d * y + t.ty)
        });
        let min = |values: [f32; 4]| values.into_iter().fold(f32::INFINITY, f32::min);
        let max = |values: [f32; 4]| values.into_iter().fold(f32::NEG_INFINITY, f32::max);
        PixelBounds([
            min(corners.map(|(x, _)| x)),
            min(corners.map(|(_, y)| y)),
            max(corners.map(|(x, _)| x)),
            max(corners.map(|(_, y)| y)),
        ])
    }

    /// How much of the text to rasterize at `raster_scale` when placed by
    /// `transform` onto a `target`-sized space: all of it while its raster
    /// fits `limit` on either side (and always a single line, whose rows
    /// the rasterizer trims to its ink); otherwise the part `target` shows,
    /// snapped out to `TEXT_WINDOW_STEP` raster pixels.
    pub(crate) fn windowed(
        &self,
        multiline: bool,
        transform: Affine,
        raster_scale: f32,
        target: (u32, u32),
        limit: u32,
    ) -> Windowed {
        let scale = f64::from(raster_scale);
        let [left, top, right, bottom] = self.reached();
        let limit = f64::from(limit);
        if !multiline || ((right - left) * scale <= limit && (bottom - top) * scale <= limit) {
            return Windowed::Whole;
        }
        let [a, b, c, d, tx, ty] = [
            transform.a,
            transform.b,
            transform.c,
            transform.d,
            transform.tx,
            transform.ty,
        ]
        .map(f64::from);
        let det = a * d - b * c;
        if det == 0.0 || !det.is_finite() {
            return Windowed::Whole;
        }
        let (width, height) = (f64::from(target.0), f64::from(target.1));
        let corners = [(0.0, 0.0), (width, 0.0), (0.0, height), (width, height)].map(|(x, y)| {
            let (x, y) = (x - tx, y - ty);
            (
                (d * x - c * y) / det + self.anchor_x,
                (a * y - b * x) / det + self.anchor_y,
            )
        });
        let min = |values: [f64; 4]| values.into_iter().fold(f64::INFINITY, f64::min);
        let max = |values: [f64; 4]| values.into_iter().fold(f64::NEG_INFINITY, f64::max);
        let step = TEXT_WINDOW_STEP / scale;
        let window = [
            (min(corners.map(|(x, _)| x)) / step).floor() * step,
            (min(corners.map(|(_, y)| y)) / step).floor() * step,
            (max(corners.map(|(x, _)| x)) / step).ceil() * step,
            (max(corners.map(|(_, y)| y)) / step).ceil() * step,
        ];
        let window = [
            window[0].max(left),
            window[1].max(top),
            window[2].min(right),
            window[3].min(bottom),
        ];
        if window[0] < window[2] && window[1] < window[3] {
            Windowed::Window(window)
        } else {
            Windowed::Hidden
        }
    }
}

/// Rasterizes `job` at its scale, or smaller when that would exceed `limit`
/// on either side (the draw's filtering then enlarges it).
pub(crate) fn rasterize_text(
    rasterizer: &mut TextRasterizer,
    job: &PendingText,
    limit: u32,
) -> Result<DecodedImage, GpuRenderError> {
    let mut scale = job.raster_scale;
    let text = loop {
        let text = rasterizer
            .rasterize_window(&job.text, &job.style, job.max_width, scale, job.window)
            .map_err(GpuRenderError::Text)?;
        let largest = text.width().max(text.height());
        if largest <= limit {
            break text;
        }
        // An unwrapped line can exceed the texture limit even at 1x.
        scale *= limit as f32 / largest as f32 * 0.99;
    };
    let baseline = text.baseline_anchor();
    let origin = text.anchor_in_image(0.0, 0.0);
    let end = text.anchor_in_image(1.0, 1.0);
    // The layout box starts at the anchor box's left edge (where the lines
    // start) and an ascent above the first baseline, which a single line's
    // anchor box, trimmed to its ink, does not reach.
    let units = f64::from(scale);
    let left = origin.0 * f64::from(text.width()) / units;
    let top = f64::from(text.baseline()) / units - job.size.ascent;
    let text_box = [
        left,
        top,
        left + job.max_width.unwrap_or(job.size.width),
        top + job.size.lines as f64 * job.size.line_height,
    ];
    let mut image = DecodedImage::new(text.width(), text.height(), text.into_pixels())?;
    image.baseline_anchor = baseline;
    image.anchor_origin = origin;
    image.anchor_span = (end.0 - origin.0, end.1 - origin.1);
    image.text_box = Some(text_box);
    image.raster_scale = scale;
    Ok(image)
}

/// The layer drawing a text `texture`, its normalized `anchor` mapped onto
/// the texture's text box (or its first baseline).
pub(crate) fn text_layer(
    texture: LayerTexture,
    anchor: Point,
    baseline_anchor: bool,
    state: LayerState,
    blend_mode: BlendMode,
) -> PreparedLayer {
    let anchor = Point {
        x: texture.anchor_origin.0 + anchor.x * texture.anchor_span.0,
        y: if baseline_anchor {
            texture.baseline_anchor
        } else {
            texture.anchor_origin.1 + anchor.y * texture.anchor_span.1
        },
    };
    PreparedLayer::new(texture, anchor, state, blend_mode)
}

/// The scale to rasterize text at so it is drawn at most slightly shrunk:
/// the largest factor `transform` stretches it by, rounded up to the next
/// eighth of an octave. An animated scale then reuses a few textures (about
/// eight per doubling) instead of rasterizing every frame, and the texture
/// is at most 9% larger than drawn. Exactly 1 for unscaled text.
pub(crate) fn text_raster_scale(transform: Affine) -> f32 {
    let (largest, _) = transform.stretch();
    // Keep absurd scales finite; the texture size is limited separately.
    round_up_to_eighth_octave(f64::from(largest)).clamp(1.0 / 64.0, 64.0) as f32
}

/// `scale` rounded up to the next eighth of an octave (a power of `2^(1/8)`);
/// a scale that already is one (2, 0.5) stays.
pub(crate) fn round_up_to_eighth_octave(scale: f64) -> f64 {
    const STEPS_PER_OCTAVE: f64 = 8.0;
    let steps = scale.log2() * STEPS_PER_OCTAVE;
    let nearest = steps.round();
    let steps = if (steps - nearest).abs() < 1e-3 {
        nearest
    } else {
        steps.ceil()
    };
    (steps / STEPS_PER_OCTAVE).exp2()
}
