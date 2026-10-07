use crate::error::GpuRenderError;
use crate::layer::PreparedLayer;
use crate::texture::{DecodedImage, LayerTexture};
use crate::transform::{Affine, LayerState};
use celesta_composition::{BlendMode, Point, TextStyle};
use celesta_renderer::TextRasterizer;

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
            .rasterize(&job.text, &job.style, job.max_width, scale)
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
    let mut image = DecodedImage::new(text.width(), text.height(), text.into_pixels())?;
    image.baseline_anchor = baseline;
    image.anchor_origin = origin;
    image.anchor_span = (end.0 - origin.0, end.1 - origin.1);
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
    const STEPS_PER_OCTAVE: f32 = 8.0;
    let (largest, _) = transform.stretch();
    let steps = largest.log2() * STEPS_PER_OCTAVE;
    let nearest = steps.round();
    // A scale that already is a step (2, 0.5) should not round up past it.
    let steps = if (steps - nearest).abs() < 1e-3 {
        nearest
    } else {
        steps.ceil()
    };
    // Keep absurd scales finite; the texture size is limited separately.
    (steps.clamp(-6.0 * STEPS_PER_OCTAVE, 6.0 * STEPS_PER_OCTAVE) / STEPS_PER_OCTAVE).exp2()
}
