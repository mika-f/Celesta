//! `LayerContent::Path`, rasterized at output resolution.
//!
//! A path is drawn straight into output pixels, with the layer's whole
//! transform applied to its geometry rather than to a finished texture, so a
//! scaled path stays sharp and a thin diagonal stroke is anti-aliased once.
//! Both renderers composite the result unscaled, which keeps their pixels
//! identical.

use celesta_composition::{LineCap, LineJoin, Paint, PathCommand, Stroke};
use resvg::tiny_skia;

use crate::{Color, RasterizedText, RenderError, ResolvedPaint};

/// What [`rasterize_path`] draws: the fields of `LayerContent::Path`.
#[derive(Clone, Copy, Debug)]
pub struct PathShape<'a> {
    pub commands: &'a [PathCommand],
    pub fill: Option<&'a Paint>,
    pub stroke: Option<&'a Stroke>,
    pub line_cap: LineCap,
    pub line_join: LineJoin,
    pub miter_limit: f64,
}

/// An affine map from a layer's own coordinates to output pixels:
/// `x' = a·x + c·y + tx`, `y' = b·x + d·y + ty`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathTransform {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub tx: f64,
    pub ty: f64,
}

impl PathTransform {
    /// Scales by `scale`, then moves the origin to `position`.
    pub const fn scale_translate(scale_x: f64, scale_y: f64, tx: f64, ty: f64) -> Self {
        Self {
            a: scale_x,
            b: 0.0,
            c: 0.0,
            d: scale_y,
            tx,
            ty,
        }
    }
}

/// A path's pixels in output space, straight alpha, with their top-left
/// corner at output pixel (`left`, `top`).
#[derive(Clone, Debug, PartialEq)]
pub struct RasterizedPath {
    pub left: i32,
    pub top: i32,
    pub image: RasterizedText,
}

/// One path to draw with [`rasterize_paths`].
#[derive(Clone, Copy, Debug)]
pub struct PathDraw<'a> {
    pub shape: PathShape<'a>,
    pub transform: PathTransform,
    /// Applied to the path as a whole: where its stroke covers its fill, the
    /// fill does not show through.
    pub opacity: f64,
}

/// Fills and then strokes `shape` through `transform`, anti-aliased, into
/// the part of a `width`x`height` output it covers. `None` when nothing
/// would be visible there: no paint, no area, or entirely outside.
///
/// Paint coordinates (gradient points) are in the layer's own coordinates,
/// like the path's.
pub fn rasterize_path(
    shape: &PathShape<'_>,
    transform: PathTransform,
    width: u32,
    height: u32,
) -> Result<Option<RasterizedPath>, RenderError> {
    rasterize_paths(
        &[PathDraw {
            shape: *shape,
            transform,
            opacity: 1.0,
        }],
        width,
        height,
    )
}

/// Draws `draws` in order, source-over, into one image covering all of them
/// (within a `width`x`height` output), like [`rasterize_path`] for each. A
/// renderer draws consecutive paths this way to make one texture of them.
///
/// Each path's coverage is rasterized over its own bounds, exactly as
/// [`rasterize_path`] would, so a path's pixels do not depend on what it is
/// batched with. Paths are rasterized in parallel, and composited in
/// parallel by bands of rows.
pub fn rasterize_paths(
    draws: &[PathDraw<'_>],
    width: u32,
    height: u32,
) -> Result<Option<RasterizedPath>, RenderError> {
    // Outlines and coverage: the expensive part, independent per path.
    let threads = std::thread::available_parallelism().map_or(1, |count| count.get());
    let prepare = |draws: &[PathDraw<'_>]| {
        let mut prepared = Vec::with_capacity(draws.len());
        for draw in draws {
            if let Some(outline) = Outline::new(draw, width, height)? {
                let coverage = outline.coverage();
                prepared.push((outline, coverage));
            }
        }
        Ok::<_, RenderError>(prepared)
    };
    let prepared = if draws.len() == 1 || threads == 1 {
        prepare(draws)?
    } else {
        let chunk = draws.len().div_ceil(threads);
        std::thread::scope(|scope| {
            let workers: Vec<_> = draws
                .chunks(chunk)
                .map(|chunk| scope.spawn(move || prepare(chunk)))
                .collect();
            let mut prepared = Vec::with_capacity(draws.len());
            for worker in workers {
                prepared.extend(worker.join().expect("path thread panicked")?);
            }
            Ok::<_, RenderError>(prepared)
        })?
    };
    let Some(region) = prepared
        .iter()
        .map(|(outline, _)| outline.region)
        .reduce(PixelRegion::union)
    else {
        return Ok(None);
    };

    // Compositing, in painter's order within each band of rows.
    let row = region.width as usize * 4;
    let mut accumulated = vec![0.0_f32; row * region.height as usize];
    let band_rows = if prepared.len() == 1 {
        region.height
    } else {
        region.height.div_ceil(threads as u32).max(MIN_BAND_ROWS)
    } as usize;
    let composite = |top: usize, band: &mut [f32]| {
        for (outline, coverage) in &prepared {
            outline.composite(coverage, region, top, band);
        }
    };
    if band_rows >= region.height as usize {
        composite(0, &mut accumulated);
    } else {
        std::thread::scope(|scope| {
            for (index, band) in accumulated.chunks_mut(row * band_rows).enumerate() {
                let composite = &composite;
                scope.spawn(move || composite(index * band_rows, band));
            }
        });
    }

    let pixels = accumulated
        .chunks_exact(4)
        .flat_map(|pixel| {
            let alpha = pixel[3];
            if alpha <= 0.0 {
                return [0; 4];
            }
            let channel = |value: f32| (value * 255.0).round().clamp(0.0, 255.0) as u8;
            [
                channel(pixel[0] / alpha),
                channel(pixel[1] / alpha),
                channel(pixel[2] / alpha),
                channel(alpha),
            ]
        })
        .collect();
    Ok(Some(RasterizedPath {
        left: region.left,
        top: region.top,
        image: RasterizedText::whole(region.width, region.height, 0.0, pixels),
    }))
}

/// Pixels of coverage checked together when skipping uncovered ones.
const SKIP_RUN: usize = 16;

/// Rows below which compositing a band on its own thread costs more than
/// it saves.
const MIN_BAND_ROWS: u32 = 32;

/// A path's fill and stroke as outlines in output pixels, ready to paint.
struct Outline {
    fill: Option<(ResolvedPaint, tiny_skia::Path)>,
    stroke: Option<(ResolvedPaint, tiny_skia::Path)>,
    /// Output pixels back to the layer's coordinates, for gradients.
    inverse: PathTransform,
    opacity: f32,
    /// The output pixels the outlines touch.
    region: PixelRegion,
}

/// How much of each pixel of an outline's region the fill and the stroke
/// cover, 0 to 255.
struct Coverage {
    fill: Option<tiny_skia::Mask>,
    stroke: Option<tiny_skia::Mask>,
}

impl Outline {
    fn new(draw: &PathDraw<'_>, width: u32, height: u32) -> Result<Option<Self>, RenderError> {
        let shape = &draw.shape;
        let fill = shape.fill.map(ResolvedPaint::from_paint).transpose()?;
        let stroke = shape
            .stroke
            .filter(|stroke| stroke.width.is_finite() && stroke.width > 0.0)
            .map(|stroke| Ok::<_, RenderError>((ResolvedPaint::from_paint(&stroke.paint)?, stroke)))
            .transpose()?;
        let opacity = draw.opacity.clamp(0.0, 1.0) as f32;
        if (fill.is_none() && stroke.is_none()) || opacity <= 0.0 {
            return Ok(None);
        }
        let Some(inverse) = invert(draw.transform) else {
            return Ok(None);
        };
        let Some(path) = build_path(shape.commands) else {
            return Ok(None);
        };
        let transform = draw.transform;
        let ts = tiny_skia::Transform::from_row(
            transform.a as f32,
            transform.b as f32,
            transform.c as f32,
            transform.d as f32,
            transform.tx as f32,
            transform.ty as f32,
        );

        let fill = fill.and_then(|paint| Some((paint, path.clone().transform(ts)?)));
        let stroke = stroke.and_then(|(paint, stroke)| {
            // Stroked in the layer's own coordinates, then transformed, so a
            // non-uniform scale stretches the stroke like the rest of the
            // layer.
            let style = tiny_skia::Stroke {
                width: stroke.width as f32,
                miter_limit: shape.miter_limit as f32,
                line_cap: match shape.line_cap {
                    LineCap::Butt => tiny_skia::LineCap::Butt,
                    LineCap::Round => tiny_skia::LineCap::Round,
                    LineCap::Square => tiny_skia::LineCap::Square,
                },
                line_join: match shape.line_join {
                    LineJoin::Miter => tiny_skia::LineJoin::Miter,
                    LineJoin::Round => tiny_skia::LineJoin::Round,
                    LineJoin::Bevel => tiny_skia::LineJoin::Bevel,
                },
                dash: None,
            };
            let resolution = tiny_skia::PathStroker::compute_resolution_scale(&ts);
            Some((paint, path.stroke(&style, resolution)?.transform(ts)?))
        });
        let Some(bounds) = [&fill, &stroke]
            .into_iter()
            .flatten()
            .map(|(_, outline)| outline.bounds())
            .reduce(union)
        else {
            return Ok(None);
        };
        let Some(region) = PixelRegion::new(bounds, width, height) else {
            return Ok(None);
        };
        Ok(Some(Self {
            fill,
            stroke,
            inverse,
            opacity,
            region,
        }))
    }

    fn coverage(&self) -> Coverage {
        let region = self.region;
        let offset =
            tiny_skia::Transform::from_translate(-(region.left as f32), -(region.top as f32));
        let mask = |outline: &Option<(ResolvedPaint, tiny_skia::Path)>| {
            let (_, outline) = outline.as_ref()?;
            let mut mask = tiny_skia::Mask::new(region.width, region.height)?;
            mask.fill_path(outline, tiny_skia::FillRule::Winding, true, offset);
            Some(mask)
        };
        Coverage {
            fill: mask(&self.fill),
            stroke: mask(&self.stroke),
        }
    }

    /// Composites the stroke over the fill, at `opacity`, onto `band`:
    /// premultiplied rows of `region` starting at row `top`.
    fn composite(&self, coverage: &Coverage, region: PixelRegion, top: usize, band: &mut [f32]) {
        let row = region.width as usize * 4;
        let band_rows = band.len() / row;
        let own = self.region;
        let first = (own.top - region.top) as usize;
        let rows = first.max(top)..(first + own.height as usize).min(top + band_rows);
        let left = (own.left - region.left) as usize;
        let fill = self.fill.as_ref().map(|(paint, _)| paint);
        let stroke = self.stroke.as_ref().map(|(paint, _)| paint);
        let solid = |paint: Option<&ResolvedPaint>| paint.and_then(ResolvedPaint::solid);
        let (solid_fill, solid_stroke) = (solid(fill), solid(stroke));
        let masks = (
            coverage.fill.as_ref().map(tiny_skia::Mask::data),
            coverage.stroke.as_ref().map(tiny_skia::Mask::data),
        );
        let own_width = own.width as usize;
        for y in rows {
            let own_row = (y - first) * own_width;
            let fill_row = masks.0.map(|mask| &mask[own_row..own_row + own_width]);
            let stroke_row = masks.1.map(|mask| &mask[own_row..own_row + own_width]);
            // Most of a thin stroke's region is uncovered: skip it a run at
            // a time.
            let empty = |row: Option<&[u8]>, range: std::ops::Range<usize>| {
                row.is_none_or(|row| row[range].iter().all(|&amount| amount == 0))
            };
            let runs = (0..own_width).step_by(SKIP_RUN).filter(|&start| {
                let run = start..(start + SKIP_RUN).min(own_width);
                !(empty(fill_row, run.clone()) && empty(stroke_row, run))
            });
            for x in runs.flat_map(|start| start..(start + SKIP_RUN).min(own_width)) {
                let covered = |row: Option<&[u8]>| row.map_or(0, |row| row[x]);
                let (fill_amount, stroke_amount) = (covered(fill_row), covered(stroke_row));
                if fill_amount == 0 && stroke_amount == 0 {
                    continue;
                }
                let local = std::cell::OnceCell::new();
                let color = |paint: Option<&ResolvedPaint>, solid: Option<Color>, amount: u8| {
                    if amount == 0 {
                        return [0.0; 4];
                    }
                    let Some(paint) = paint else {
                        return [0.0; 4];
                    };
                    let color = solid.unwrap_or_else(|| {
                        let &(local_x, local_y) = local.get_or_init(|| {
                            let output_x = f64::from(own.left) + x as f64 + 0.5;
                            let output_y = f64::from(region.top) + y as f64 + 0.5;
                            apply(self.inverse, output_x, output_y)
                        });
                        paint.color_at(local_x, local_y)
                    });
                    let alpha = f32::from(color.alpha) / 255.0 * f32::from(amount) / 255.0;
                    [
                        f32::from(color.red) / 255.0 * alpha,
                        f32::from(color.green) / 255.0 * alpha,
                        f32::from(color.blue) / 255.0 * alpha,
                        alpha,
                    ]
                };
                let top_color = color(stroke, solid_stroke, stroke_amount);
                let bottom = color(fill, solid_fill, fill_amount);
                let destination = &mut band[(y - top) * row + (left + x) * 4..][..4];
                let keep_bottom = 1.0 - top_color[3];
                let source: [f32; 4] = std::array::from_fn(|i| {
                    (top_color[i] + bottom[i] * keep_bottom) * self.opacity
                });
                let keep = 1.0 - source[3];
                for (destination, source) in destination.iter_mut().zip(source) {
                    *destination = source + *destination * keep;
                }
            }
        }
    }
}

/// Whole output pixels covering some bounds, cut to the output.
#[derive(Clone, Copy, Debug)]
struct PixelRegion {
    left: i32,
    top: i32,
    width: u32,
    height: u32,
}

impl PixelRegion {
    fn new(bounds: tiny_skia::Rect, width: u32, height: u32) -> Option<Self> {
        let left = f64::from(bounds.left().floor()).max(0.0);
        let top = f64::from(bounds.top().floor()).max(0.0);
        let right = f64::from(bounds.right().ceil()).min(f64::from(width));
        let bottom = f64::from(bounds.bottom().ceil()).min(f64::from(height));
        (right > left && bottom > top).then_some(Self {
            left: left as i32,
            top: top as i32,
            width: (right - left) as u32,
            height: (bottom - top) as u32,
        })
    }
}

impl PixelRegion {
    fn union(self, other: Self) -> Self {
        let left = self.left.min(other.left);
        let top = self.top.min(other.top);
        let right = (self.left + self.width as i32).max(other.left + other.width as i32);
        let bottom = (self.top + self.height as i32).max(other.top + other.height as i32);
        Self {
            left,
            top,
            width: (right - left) as u32,
            height: (bottom - top) as u32,
        }
    }
}

fn union(a: tiny_skia::Rect, b: tiny_skia::Rect) -> tiny_skia::Rect {
    tiny_skia::Rect::from_ltrb(
        a.left().min(b.left()),
        a.top().min(b.top()),
        a.right().max(b.right()),
        a.bottom().max(b.bottom()),
    )
    .unwrap_or(a)
}

fn build_path(commands: &[PathCommand]) -> Option<tiny_skia::Path> {
    let mut builder = tiny_skia::PathBuilder::with_capacity(commands.len(), commands.len());
    for command in commands {
        match *command {
            PathCommand::MoveTo { x, y } => builder.move_to(x as f32, y as f32),
            PathCommand::LineTo { x, y } => builder.line_to(x as f32, y as f32),
            PathCommand::QuadTo { x1, y1, x, y } => {
                builder.quad_to(x1 as f32, y1 as f32, x as f32, y as f32);
            }
            PathCommand::CubicTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => builder.cubic_to(
                x1 as f32, y1 as f32, x2 as f32, y2 as f32, x as f32, y as f32,
            ),
            PathCommand::Close => builder.close(),
        }
    }
    builder.finish()
}

fn invert(t: PathTransform) -> Option<PathTransform> {
    let determinant = t.a * t.d - t.b * t.c;
    if !determinant.is_normal() {
        return None;
    }
    let (a, b, c, d) = (
        t.d / determinant,
        -t.b / determinant,
        -t.c / determinant,
        t.a / determinant,
    );
    Some(PathTransform {
        a,
        b,
        c,
        d,
        tx: -(a * t.tx + c * t.ty),
        ty: -(b * t.tx + d * t.ty),
    })
}

fn apply(t: PathTransform, x: f64, y: f64) -> (f64, f64) {
    (t.a * x + t.c * y + t.tx, t.b * x + t.d * y + t.ty)
}

#[cfg(test)]
mod tests {
    use celesta_composition::{
        DEFAULT_MITER_LIMIT, EvaluatedTransform, Layer, LayerContent, Point, Rational, Scene, Time,
    };

    use super::*;
    use crate::{CpuRenderer, RenderOptions, RgbaFrame};

    fn polyline(points: &[(f64, f64)], closed: bool) -> Vec<PathCommand> {
        let mut commands: Vec<_> = points
            .iter()
            .enumerate()
            .map(|(index, &(x, y))| {
                if index == 0 {
                    PathCommand::MoveTo { x, y }
                } else {
                    PathCommand::LineTo { x, y }
                }
            })
            .collect();
        if closed {
            commands.push(PathCommand::Close);
        }
        commands
    }

    fn solid(color: &str) -> Paint {
        Paint::Solid {
            color: color.to_owned(),
        }
    }

    fn stroked(commands: Vec<PathCommand>, color: &str, width: f64) -> LayerContent {
        LayerContent::Path {
            commands,
            fill: None,
            stroke: Some(Stroke {
                paint: solid(color),
                width,
            }),
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
            miter_limit: DEFAULT_MITER_LIMIT,
        }
    }

    fn layer(content: LayerContent, transform: EvaluatedTransform) -> Layer {
        Layer {
            id: "path".to_owned(),
            transform,
            opacity: 1.0,
            blend_mode: Default::default(),
            effects: Default::default(),
            content,
        }
    }

    fn render(width: u32, height: u32, layers: Vec<Layer>) -> RgbaFrame {
        CpuRenderer::new(RenderOptions {
            background: Color::TRANSPARENT,
        })
        .render(&Scene {
            width,
            height,
            frame_rate: Rational::new(30, 1),
            time: Time::ZERO,
            fonts: Vec::new(),
            layers,
        })
        .unwrap()
    }

    fn alpha(frame: &RgbaFrame, x: u32, y: u32) -> u8 {
        frame.pixels()[((y * frame.width() + x) * 4 + 3) as usize]
    }

    #[test]
    fn paints_overlapping_parts_of_a_translucent_stroke_once() {
        // An X drawn as one path crosses itself at (10, 10).
        let content = stroked(
            polyline(&[(2.0, 2.0), (18.0, 18.0), (18.0, 2.0), (2.0, 18.0)], false),
            "#FF000080",
            3.0,
        );
        let frame = render(20, 20, vec![layer(content, EvaluatedTransform::default())]);
        assert_eq!(alpha(&frame, 10, 10), 0x80);
        assert_eq!(alpha(&frame, 6, 6), 0x80);
    }

    #[test]
    fn closes_a_subpath_without_a_seam() {
        let square = [(4.0, 4.0), (16.0, 4.0), (16.0, 16.0), (4.0, 16.0)];
        let closed = render(
            20,
            20,
            vec![layer(
                stroked(polyline(&square, true), "#FFFFFF", 4.0),
                EvaluatedTransform::default(),
            )],
        );
        // The mitered corner where the subpath starts and ends is as full as
        // any other corner.
        assert_eq!(alpha(&closed, 2, 2), 255);
        assert_eq!(alpha(&closed, 17, 17), 255);
        // Left open (the last side drawn as a line back to the start), the
        // start has butt ends and no corner.
        let mut open = polyline(&square, false);
        open.push(PathCommand::LineTo { x: 4.0, y: 4.0 });
        let open = render(
            20,
            20,
            vec![layer(
                stroked(open, "#FFFFFF", 4.0),
                EvaluatedTransform::default(),
            )],
        );
        assert_eq!(alpha(&open, 2, 2), 0);
    }

    #[test]
    fn bevels_acute_joins_past_the_miter_limit() {
        // A 20° spike pointing right at (30, 10).
        let spike = polyline(&[(2.0, 6.0), (30.0, 10.0), (2.0, 14.0)], false);
        let render_with = |miter_limit: f64| {
            let LayerContent::Path {
                commands,
                fill,
                stroke,
                line_cap,
                line_join,
                ..
            } = stroked(spike.clone(), "#FFFFFF", 4.0)
            else {
                unreachable!()
            };
            render(
                48,
                20,
                vec![layer(
                    LayerContent::Path {
                        commands,
                        fill,
                        stroke,
                        line_cap,
                        line_join,
                        miter_limit,
                    },
                    EvaluatedTransform::default(),
                )],
            )
        };
        // The miter of a ~16° corner is ~7 stroke widths long.
        assert_eq!(alpha(&render_with(DEFAULT_MITER_LIMIT), 36, 10), 0);
        assert!(alpha(&render_with(20.0), 36, 10) > 200);
    }

    #[test]
    fn scales_the_geometry_instead_of_the_pixels() {
        let content = stroked(polyline(&[(1.0, 5.0), (9.0, 5.0)], false), "#FFFFFF", 1.0);
        let frame = render(
            40,
            40,
            vec![layer(
                content,
                EvaluatedTransform {
                    scale: Point { x: 4.0, y: 4.0 },
                    anchor: Point { x: 0.5, y: 0.5 },
                    ..EvaluatedTransform::default()
                },
            )],
        );
        // A 1px stroke on y = 5, four times larger: rows 18..22 exactly,
        // with hard edges rather than a magnified anti-aliased pixel. The
        // anchor is ignored.
        for y in 17..23 {
            let expected = if (18..22).contains(&y) { 255 } else { 0 };
            assert_eq!(alpha(&frame, 20, y), expected, "row {y}");
        }
    }

    #[test]
    fn anti_aliases_thin_diagonal_strokes() {
        let content = stroked(polyline(&[(0.0, 0.0), (40.0, 30.0)], false), "#FFFFFF", 0.5);
        let frame = render(40, 30, vec![layer(content, EvaluatedTransform::default())]);
        let total: f64 = frame
            .pixels()
            .chunks_exact(4)
            .map(|pixel| f64::from(pixel[3]) / 255.0)
            .sum();
        // Ink equals the stroke's area (50 x 0.5) to within a few percent,
        // spread over partially covered pixels.
        assert!((total - 25.0).abs() < 1.5, "total coverage {total}");
        assert!(frame.pixels().chunks_exact(4).all(|pixel| pixel[3] < 200));
    }

    #[test]
    fn fills_and_strokes_with_gradients_in_local_coordinates() {
        let content = LayerContent::Path {
            commands: polyline(&[(0.0, 0.0), (20.0, 0.0), (20.0, 10.0), (0.0, 10.0)], true),
            fill: Some(Paint::Linear {
                start: Point { x: 0.0, y: 0.0 },
                end: Point { x: 20.0, y: 0.0 },
                stops: vec![
                    celesta_composition::GradientStop {
                        offset: 0.0,
                        color: "#000000".to_owned(),
                    },
                    celesta_composition::GradientStop {
                        offset: 1.0,
                        color: "#FFFFFF".to_owned(),
                    },
                ],
            }),
            stroke: None,
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
            miter_limit: DEFAULT_MITER_LIMIT,
        };
        let frame = render(
            30,
            20,
            vec![layer(
                content,
                EvaluatedTransform {
                    position: Point { x: 5.0, y: 5.0 },
                    ..EvaluatedTransform::default()
                },
            )],
        );
        let red = |x: u32| frame.pixels()[((10 * 30 + x) * 4) as usize];
        assert!(red(5) < 10, "{}", red(5));
        assert!((120..136).contains(&red(15)), "{}", red(15));
        assert!(red(24) > 245, "{}", red(24));
        assert_eq!(alpha(&frame, 4, 10), 0);
    }

    #[test]
    fn caps_a_zero_length_segment_into_a_dot() {
        let dot = |line_cap| {
            let LayerContent::Path {
                commands,
                fill,
                stroke,
                line_join,
                miter_limit,
                ..
            } = stroked(
                polyline(&[(10.0, 10.0), (10.0, 10.0)], false),
                "#FFFFFF",
                6.0,
            )
            else {
                unreachable!()
            };
            let frame = render(
                20,
                20,
                vec![layer(
                    LayerContent::Path {
                        commands,
                        fill,
                        stroke,
                        line_cap,
                        line_join,
                        miter_limit,
                    },
                    EvaluatedTransform::default(),
                )],
            );
            (alpha(&frame, 10, 10), alpha(&frame, 12, 7))
        };
        assert_eq!(dot(LineCap::Butt), (0, 0));
        assert_eq!(dot(LineCap::Round).0, 255);
        // Square caps make a 6x6 square, which reaches a corner a disc misses.
        assert_eq!(dot(LineCap::Square), (255, 255));
        assert!(dot(LineCap::Round).1 < 128);
    }

    #[test]
    fn a_batch_matches_drawing_its_paths_one_by_one() {
        let stroke = Stroke {
            paint: solid("#EF402B"),
            width: 3.0,
        };
        let fill = solid("#2050FF");
        let commands = [
            polyline(&[(2.0, 2.0), (40.0, 30.0), (60.0, 4.0)], false),
            polyline(&[(4.0, 28.0), (30.0, 2.0), (58.0, 28.0)], true),
        ];
        let shapes = [
            PathShape {
                commands: &commands[0],
                fill: None,
                stroke: Some(&stroke),
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
                miter_limit: DEFAULT_MITER_LIMIT,
            },
            PathShape {
                commands: &commands[1],
                fill: Some(&fill),
                stroke: Some(&stroke),
                line_cap: LineCap::Butt,
                line_join: LineJoin::Miter,
                miter_limit: DEFAULT_MITER_LIMIT,
            },
        ];
        let identity = PathTransform::scale_translate(1.0, 1.0, 0.0, 0.0);
        let draws: Vec<_> = shapes
            .iter()
            .zip([0.6, 0.5])
            .map(|(shape, opacity)| PathDraw {
                shape: *shape,
                transform: identity,
                opacity,
            })
            .collect();
        let batch = rasterize_paths(&draws, 64, 32).unwrap().unwrap();

        // Each path on its own, composited with its opacity like a layer.
        let mut expected = RgbaFrame {
            width: 64,
            height: 32,
            pixels: vec![0; 64 * 32 * 4],
        };
        for draw in &draws {
            let path = rasterize_path(&draw.shape, draw.transform, 64, 32)
                .unwrap()
                .unwrap();
            crate::render_image_pixels(
                &mut expected,
                path.image.width(),
                path.image.height(),
                path.image.pixels(),
                Point { x: 0.0, y: 0.0 },
                &crate::ParentState {
                    position: Point {
                        x: f64::from(path.left),
                        y: f64::from(path.top),
                    },
                    opacity: draw.opacity,
                    ..Default::default()
                },
            );
        }
        let width = batch.image.width() as usize;
        for (index, pixel) in batch.image.pixels().chunks_exact(4).enumerate() {
            let x = batch.left as usize + index % width;
            let y = batch.top as usize + index / width;
            let expected = &expected.pixels()[(y * 64 + x) * 4..][..4];
            // Colors only matter where there is enough alpha to see them.
            let tolerance = if pixel[3] < 16 { 255 } else { 2 };
            assert!(
                pixel[3].abs_diff(expected[3]) <= 1
                    && pixel
                        .iter()
                        .zip(expected)
                        .take(3)
                        .all(|(a, b)| a.abs_diff(*b) <= tolerance),
                "({x}, {y}): batch {pixel:?}, one by one {expected:?}"
            );
        }
        // Under the second path's stroke, its fill does not show through.
        let fill_under_stroke = &batch.image.pixels()
            [((2 - batch.top as usize) * width + (30 - batch.left as usize)) * 4..][..4];
        assert!(fill_under_stroke[2] < 0x60, "{fill_under_stroke:?}");
    }

    #[test]
    fn draws_nothing_without_paint_or_outside_the_output() {
        let shape = PathShape {
            commands: &polyline(&[(0.0, 0.0), (10.0, 10.0)], false),
            fill: None,
            stroke: None,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            miter_limit: DEFAULT_MITER_LIMIT,
        };
        let identity = PathTransform::scale_translate(1.0, 1.0, 0.0, 0.0);
        assert_eq!(rasterize_path(&shape, identity, 20, 20).unwrap(), None);
        let stroke = Stroke {
            paint: solid("#FFFFFF"),
            width: 2.0,
        };
        let shape = PathShape {
            stroke: Some(&stroke),
            ..shape
        };
        let outside = PathTransform::scale_translate(1.0, 1.0, 100.0, 0.0);
        assert_eq!(rasterize_path(&shape, outside, 20, 20).unwrap(), None);
        // Clipped to the output: a path hanging off the left edge starts at 0.
        let left = PathTransform::scale_translate(1.0, 1.0, -5.0, 0.0);
        let path = rasterize_path(&shape, left, 20, 20).unwrap().unwrap();
        assert_eq!((path.left, path.top), (0, 0));
        assert!(path.image.width() <= 7);
    }
}
