use crate::error::RenderError;
use crate::paint::{ResolvedPaint, resolve_paint};
use crate::text::RasterizedText;
use crate::types::Color;
use celesta_composition::{Paint, Stroke};

/// Rasterizes a flat-shaded, optionally rounded and stroked rectangle into an
/// RGBA buffer, anti-aliased by signed distance. Shares `RasterizedText`'s
/// shape (width/height/pixels) so it composites through the exact same
/// `render_image` path text does. Takes the raw `Paint`/`Stroke` composition
/// types (like `TextRasterizer::rasterize` takes `&TextStyle`) so callers,
/// including `celesta-gpu-renderer`, never need their own color parsing.
pub fn rasterize_rect(
    width: f64,
    height: f64,
    corner_radius: f64,
    fill: Option<&Paint>,
    stroke: Option<&Stroke>,
) -> Result<RasterizedText, RenderError> {
    let RectPaint { fill, stroke } = resolve_rect_paint(fill, stroke)?;
    Ok(rasterize_rect_pixels(
        width,
        height,
        corner_radius,
        fill,
        stroke,
    ))
}

/// The colors [`rasterize_rect`] paints a rect with.
#[derive(Clone, Debug, PartialEq)]
pub struct RectPaint {
    pub fill: Option<ResolvedPaint>,
    /// The stroke paint and width.
    pub stroke: Option<(ResolvedPaint, f64)>,
}

/// Parses a rect's fill and stroke into the colors [`rasterize_rect`] paints
/// with. A renderer that draws rects itself (`celesta-gpu-renderer` shades
/// them on the GPU) uses this so it resolves paint exactly like the
/// rasterizer.
pub fn resolve_rect_paint(
    fill: Option<&Paint>,
    stroke: Option<&Stroke>,
) -> Result<RectPaint, RenderError> {
    let fill = resolve_paint(fill)?;
    let stroke = stroke
        .map(|stroke| {
            Ok::<_, RenderError>((ResolvedPaint::from_paint(&stroke.paint)?, stroke.width))
        })
        .transpose()?;
    Ok(RectPaint { fill, stroke })
}

pub(crate) fn rasterize_rect_pixels(
    width: f64,
    height: f64,
    corner_radius: f64,
    fill: Option<ResolvedPaint>,
    stroke: Option<(ResolvedPaint, f64)>,
) -> RasterizedText {
    let pixel_width = width.max(0.0).ceil().max(1.0) as u32;
    let pixel_height = height.max(0.0).ceil().max(1.0) as u32;
    let mut pixels = vec![0_u8; pixel_width as usize * pixel_height as usize * 4];

    let half_width = width / 2.0;
    let half_height = height / 2.0;
    let radius = corner_radius.max(0.0).min(half_width.min(half_height));
    let stroke = stroke.filter(|(_, stroke_width)| *stroke_width > 0.0);

    for y in 0..pixel_height {
        for x in 0..pixel_width {
            let px = x as f64 + 0.5 - half_width;
            let py = y as f64 + 0.5 - half_height;
            let outer_distance =
                signed_distance_rounded_box(px, py, half_width, half_height, radius);
            let outer_alpha = (0.5 - outer_distance).clamp(0.0, 1.0);
            if outer_alpha <= 0.0 {
                continue;
            }

            let (sample_x, sample_y) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
            let mut color = fill
                .as_ref()
                .map_or(Color::TRANSPARENT, |fill| fill.color_at(sample_x, sample_y));
            if let Some((stroke_paint, stroke_width)) = &stroke {
                let (stroke_color, stroke_width) =
                    (stroke_paint.color_at(sample_x, sample_y), *stroke_width);
                let inner_half_width = (half_width - stroke_width).max(0.0);
                let inner_half_height = (half_height - stroke_width).max(0.0);
                let inner_radius = (radius - stroke_width).max(0.0);
                let inner_distance = signed_distance_rounded_box(
                    px,
                    py,
                    inner_half_width,
                    inner_half_height,
                    inner_radius,
                );
                let inner_alpha = (0.5 - inner_distance).clamp(0.0, 1.0);
                color = Color::rgba(
                    lerp(stroke_color.red, color.red, inner_alpha),
                    lerp(stroke_color.green, color.green, inner_alpha),
                    lerp(stroke_color.blue, color.blue, inner_alpha),
                    lerp(stroke_color.alpha, color.alpha, inner_alpha),
                );
            }

            let offset = (y * pixel_width + x) as usize * 4;
            pixels[offset] = color.red;
            pixels[offset + 1] = color.green;
            pixels[offset + 2] = color.blue;
            pixels[offset + 3] = (f64::from(color.alpha) * outer_alpha)
                .round()
                .clamp(0.0, 255.0) as u8;
        }
    }

    RasterizedText::whole(pixel_width, pixel_height, 0.0, pixels)
}

/// Inigo Quilez's rounded-box signed distance function: negative inside the
/// shape, zero at the edge, positive outside, in the same pixel units as
/// `half_width`/`half_height`/`radius`.
pub(crate) fn signed_distance_rounded_box(
    px: f64,
    py: f64,
    half_width: f64,
    half_height: f64,
    radius: f64,
) -> f64 {
    let qx = px.abs() - half_width + radius;
    let qy = py.abs() - half_height + radius;
    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius
}

pub(crate) fn lerp(a: u8, b: u8, t: f64) -> u8 {
    (f64::from(a) + (f64::from(b) - f64::from(a)) * t)
        .round()
        .clamp(0.0, 255.0) as u8
}

pub(crate) fn dilate_mask(mask: &[u8], width: u32, height: u32, radius: u32) -> Vec<u8> {
    let mut output = vec![0; mask.len()];
    let radius = radius as i32;
    for y in 0..height as i32 {
        for x in 0..width as i32 {
            let alpha = mask[y as usize * width as usize + x as usize];
            if alpha == 0 {
                continue;
            }
            for offset_y in -radius..=radius {
                for offset_x in -radius..=radius {
                    let target_x = x + offset_x;
                    let target_y = y + offset_y;
                    if target_x < 0
                        || target_y < 0
                        || target_x >= width as i32
                        || target_y >= height as i32
                    {
                        continue;
                    }
                    let offset = target_y as usize * width as usize + target_x as usize;
                    output[offset] = output[offset].max(alpha);
                }
            }
        }
    }
    output
}
