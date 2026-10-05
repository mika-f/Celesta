use crate::error::RenderError;
use crate::paint::{ResolvedPaint, resolve_paint};
use crate::path::{PathTransform, RasterizedPath, apply, invert};
use crate::text::RasterizedText;
use crate::types::Color;
use celesta_composition::{Paint, Stroke};

/// Rasterizes a flat-shaded, optionally rounded and stroked rectangle into an
/// RGBA buffer, anti-aliased by signed distance: the rect at its own size,
/// one texel per unit, rounded up to whole texels. Takes the raw
/// `Paint`/`Stroke` composition types (like `TextRasterizer::rasterize`
/// takes `&TextStyle`) so callers never need their own color parsing. To
/// draw a rect where a layer puts it, use [`rasterize_rect_transformed`].
pub fn rasterize_rect(
    width: f64,
    height: f64,
    corner_radius: f64,
    fill: Option<&Paint>,
    stroke: Option<&Stroke>,
) -> Result<RasterizedText, RenderError> {
    let paint = resolve_rect_paint(fill, stroke)?;
    let pixel_width = width.max(0.0).ceil().max(1.0) as u32;
    let pixel_height = height.max(0.0).ceil().max(1.0) as u32;
    let pixels = shade_rect(
        &RectBox::new(width, height, corner_radius),
        &paint,
        PathTransform::scale_translate(1.0, 1.0, 0.0, 0.0),
        (0, 0, pixel_width, pixel_height),
    );
    Ok(RasterizedText::whole(
        pixel_width,
        pixel_height,
        0.0,
        pixels,
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

/// Paints the rect `[0, width] x [0, height]` of a layer's own coordinates
/// through `transform`, anti-aliased, into the part of a
/// `canvas_width`x`canvas_height` output it covers, like `rasterize_path`
/// does a path. Every output pixel is shaded at its centre, so fractional
/// positions, sizes, anchors and scales keep their edges where they fall,
/// which a texture of whole texels placed on a whole pixel would not. `None`
/// when no output pixel is covered.
///
/// Paint coordinates (gradient points) are in the layer's own coordinates,
/// from the rect's top-left corner.
pub fn rasterize_rect_transformed(
    width: f64,
    height: f64,
    corner_radius: f64,
    paint: &RectPaint,
    transform: PathTransform,
    canvas_width: u32,
    canvas_height: u32,
) -> Option<RasterizedPath> {
    let inverse = invert(transform)?;
    let corners = [(0.0, 0.0), (width, 0.0), (0.0, height), (width, height)]
        .map(|(x, y)| apply(transform, x, y));
    // Anti-aliasing reaches half a pixel past the edge.
    let span = |values: [f64; 4], limit: u32| {
        let low = values.into_iter().fold(f64::INFINITY, f64::min);
        let high = values.into_iter().fold(f64::NEG_INFINITY, f64::max);
        let low = (low - 1.0).floor().max(0.0);
        let high = (high + 1.0).ceil().min(f64::from(limit));
        (high > low).then_some((low as u32, (high - low) as u32))
    };
    let (left, pixel_width) = span(corners.map(|(x, _)| x), canvas_width)?;
    let (top, pixel_height) = span(corners.map(|(_, y)| y), canvas_height)?;
    let pixels = shade_rect(
        &RectBox::new(width, height, corner_radius),
        paint,
        inverse,
        (left, top, pixel_width, pixel_height),
    );
    Some(RasterizedPath {
        left: left as i32,
        top: top as i32,
        image: RasterizedText::whole(pixel_width, pixel_height, 0.0, pixels),
    })
}

/// A rect's box in its own units, centred on the origin.
struct RectBox {
    half_width: f64,
    half_height: f64,
    radius: f64,
}

impl RectBox {
    fn new(width: f64, height: f64, corner_radius: f64) -> Self {
        let half_width = width / 2.0;
        let half_height = height / 2.0;
        Self {
            half_width,
            half_height,
            radius: corner_radius.max(0.0).min(half_width.min(half_height)),
        }
    }
}

/// Shades the output pixels `(left, top, width, height)` of a rect drawn
/// through the transform whose inverse is `inverse`.
fn shade_rect(
    rect: &RectBox,
    paint: &RectPaint,
    inverse: PathTransform,
    (left, top, pixel_width, pixel_height): (u32, u32, u32, u32),
) -> Vec<u8> {
    let mut pixels = vec![0_u8; pixel_width as usize * pixel_height as usize * 4];
    let RectBox {
        half_width,
        half_height,
        radius,
    } = *rect;
    let distance = RectDistance::new(inverse);
    let outer_cap = distance.coverage_cap(half_width, half_height);
    // The stroke's paint, and the box inside it that the fill shows through.
    let stroke = paint
        .stroke
        .as_ref()
        .filter(|(_, stroke_width)| *stroke_width > 0.0)
        .map(|(stroke_paint, stroke_width)| {
            let inner = RectBox {
                half_width: (half_width - stroke_width).max(0.0),
                half_height: (half_height - stroke_width).max(0.0),
                radius: (radius - stroke_width).max(0.0),
            };
            let cap = distance.coverage_cap(inner.half_width, inner.half_height);
            (stroke_paint, inner, cap)
        });

    for y in 0..pixel_height {
        for x in 0..pixel_width {
            // The output pixel's centre in the rect's own units, from its
            // top-left corner (where gradients are defined) and its centre.
            let (canvas_x, canvas_y) = (f64::from(left + x) + 0.5, f64::from(top + y) + 0.5);
            let (sample_x, sample_y) = apply(inverse, canvas_x, canvas_y);
            let px = sample_x - half_width;
            let py = sample_y - half_height;
            let outer_distance = distance.of(px, py, half_width, half_height, radius);
            let outer_alpha = (0.5 - outer_distance).clamp(0.0, outer_cap);
            if outer_alpha <= 0.0 {
                continue;
            }

            let mut color = paint
                .fill
                .as_ref()
                .map_or(Color::TRANSPARENT, |fill| fill.color_at(sample_x, sample_y));
            if let Some((stroke_paint, inner, inner_cap)) = &stroke {
                let stroke_color = stroke_paint.color_at(sample_x, sample_y);
                let inner_distance =
                    distance.of(px, py, inner.half_width, inner.half_height, inner.radius);
                let inner_alpha = (0.5 - inner_distance).clamp(0.0, *inner_cap);
                // Of the part of the pixel the rect covers, the fill's share;
                // the rest is stroke.
                let fill_share = (inner_alpha / outer_alpha).min(1.0);
                color = Color::rgba(
                    lerp(stroke_color.red, color.red, fill_share),
                    lerp(stroke_color.green, color.green, fill_share),
                    lerp(stroke_color.blue, color.blue, fill_share),
                    lerp(stroke_color.alpha, color.alpha, fill_share),
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
    pixels
}

/// [`signed_distance_rounded_box`] for a box drawn through an affine
/// transform, given that transform's `inverse`, measured for anti-aliasing:
/// in units of a pixel's width across the edge, `|n.x| + |n.y|` for the
/// edge's normal `n` in output pixels (1 for an edge along a row or column,
/// up to √2 at 45 degrees). `0.5 - distance` then ramps across the edge as
/// a pixel-sized box filter does, so a thin strip at any angle draws the
/// same however its edges fall between pixel centres. That is the local
/// distance over the L1 length of its gradient in output pixels: exact
/// along straight edges, and a first-order approximation round a corner
/// (an ellipse, under a non-uniform scale). `layer.wgsl` in
/// `celesta-gpu-renderer` mirrors it.
struct RectDistance {
    /// The inverse's rows: how the local x and y change per output pixel.
    gradient_x: (f64, f64),
    gradient_y: (f64, f64),
    /// The L1 length of each, the local units across a pixel's width
    /// across a vertical edge and a horizontal one.
    footprint_x: f64,
    footprint_y: f64,
}

impl RectDistance {
    fn new(inverse: PathTransform) -> Self {
        let gradient_x = (inverse.a, inverse.c);
        let gradient_y = (inverse.b, inverse.d);
        let l1 = |(x, y): (f64, f64)| x.abs() + y.abs();
        Self {
            gradient_x,
            gradient_y,
            footprint_x: l1(gradient_x),
            footprint_y: l1(gradient_y),
        }
    }

    /// The most of a pixel a box of these half extents can cover. One
    /// sample's `0.5 - distance` alone reaches 0.5 at the centre of a box
    /// thinner than a pixel, or even empty. A box filter gives a pixel well
    /// inside the ramps of a pair of parallel edges their distance apart, in
    /// the units [`Self::of`] measures in; inside both pairs' ramps, the
    /// product of the two. Each is at most 1.
    fn coverage_cap(&self, half_width: f64, half_height: f64) -> f64 {
        let across = |size: f64, footprint: f64| {
            let fraction = size / footprint;
            // Not `clamp`, which keeps a NaN (a NaN size) that the caller's
            // `clamp(0.0, cap)` panics on.
            if fraction >= 0.0 {
                fraction.min(1.0)
            } else {
                0.0
            }
        };
        across(2.0 * half_width, self.footprint_x) * across(2.0 * half_height, self.footprint_y)
    }

    fn of(&self, px: f64, py: f64, half_width: f64, half_height: f64, radius: f64) -> f64 {
        let qx = px.abs() - half_width + radius;
        let qy = py.abs() - half_height + radius;
        if qx > 0.0 && qy > 0.0 {
            // The local distance grows along `(qx, qy) / length`, signed by
            // the quadrant: a shear stretches opposite corners apart.
            let length = qx.hypot(qy);
            let signed = |q: f64, p: f64| if p < 0.0 { -q } else { q };
            let (gx, gy) = (signed(qx, px) / length, signed(qy, py) / length);
            let along_x = gx * self.gradient_x.0 + gy * self.gradient_y.0;
            let along_y = gx * self.gradient_x.1 + gy * self.gradient_y.1;
            (length - radius) / (along_x.abs() + along_y.abs())
        } else {
            ((qx - radius) / self.footprint_x).max((qy - radius) / self.footprint_y)
        }
    }
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
