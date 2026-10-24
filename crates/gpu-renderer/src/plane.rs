//! Projective layers: a layer with a perspective and a 3D rotation draws its
//! content onto a canvas in its own plane (a plane), which then draws onto
//! its parent through the layer's homography.

use crate::bounds::{CanvasRegion, PixelBounds};
use crate::error::GpuRenderError;
use crate::hit::HitClip;
use crate::layer::{PreparedItem, PreparedLayer, WHOLE_BAND};
use crate::plan::content_bounds;
use crate::renderer::GpuRenderer;
use crate::text::round_up_to_eighth_octave;
use crate::transform::{Affine, ClipEntry, LayerState};
use celesta_composition::{
    BlendMode, IDENTITY3, Layer, Matrix3, Point, invert3, multiply3, project, stretch_at,
};
use std::collections::HashSet;

/// The nearest a point may come to the viewer and still be drawn: points
/// with `w` below it are behind the viewer, or so close they would fill the
/// screen.
pub(crate) const NEAR_W: f64 = 1.0 / 16.0;

/// The convex polygon `corners` clipped to the part `m` maps to
/// `w ≥ NEAR_W`.
pub(crate) fn clip_in_front(m: &Matrix3, corners: &[[f64; 2]]) -> Vec<[f64; 2]> {
    clip_polygon(corners, |[x, y]| {
        m[2][0] * x + m[2][1] * y + m[2][2] - NEAR_W
    })
}

/// Sutherland–Hodgman: the convex polygon `corners` clipped to where the
/// affine function `inside` is at least 0.
fn clip_polygon(corners: &[[f64; 2]], inside: impl Fn([f64; 2]) -> f64) -> Vec<[f64; 2]> {
    let mut clipped = Vec::with_capacity(corners.len() + 1);
    for (index, &current) in corners.iter().enumerate() {
        let next = corners[(index + 1) % corners.len()];
        let (here, there) = (inside(current), inside(next));
        if here >= 0.0 {
            clipped.push(current);
        }
        if (here >= 0.0) != (there >= 0.0) {
            let t = here / (here - there);
            clipped.push([
                current[0] + t * (next[0] - current[0]),
                current[1] + t * (next[1] - current[1]),
            ]);
        }
    }
    clipped
}

/// The corners of `bounds`, in order around it.
fn corners(PixelBounds(bounds): PixelBounds) -> [[f64; 2]; 4] {
    let [left, top, right, bottom] = bounds.map(f64::from);
    [[left, top], [right, top], [right, bottom], [left, bottom]]
}

/// The box of `points`; `None` without any.
fn box_of(points: impl IntoIterator<Item = [f64; 2]>) -> Option<[f64; 4]> {
    points.into_iter().fold(None, |bounds, [x, y]| {
        Some(
            bounds.map_or([x, y, x, y], |[left, top, right, bottom]: [f64; 4]| {
                [left.min(x), top.min(y), right.max(x), bottom.max(y)]
            }),
        )
    })
}

/// The box of where `m` maps the part of the canvas `(0, 0, width, height)`
/// in front of the viewer, grown by 2 pixels; `None` when none of it is.
pub(crate) fn projected_bounds(m: &Matrix3, width: u32, height: u32) -> Option<PixelBounds> {
    let canvas = PixelBounds([0.0, 0.0, width as f32, height as f32]);
    let polygon = clip_in_front(m, &corners(canvas));
    let [left, top, right, bottom] = box_of(polygon.into_iter().map(|[x, y]| {
        let [x, y, _] = project(m, x, y);
        [x, y]
    }))?;
    Some(PixelBounds([left, top, right, bottom].map(|value| value as f32)).expand(2.0, 2.0))
}

/// How a plane's content is rasterized: `scale` plane pixels per local
/// unit, the local box from `left`, `top` drawn onto a `width`x`height`
/// canvas with a 1-pixel transparent margin round it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PlaneRaster {
    pub(crate) scale: f64,
    pub(crate) left: f64,
    pub(crate) top: f64,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

/// The convex polygon `polygon`, all of it in front of the viewer (see
/// `clip_in_front`), clipped to the part `m` maps into `target`. Each side
/// of `target` is a half-plane in the plane too: `X/W ≥ left` is
/// `X − left·W ≥ 0` where `W > 0`, which is linear in the point, so the clip
/// is exact whether or not the plane's horizon shows.
fn clip_to_shown(m: &Matrix3, polygon: &[[f64; 2]], target: PixelBounds) -> Vec<[f64; 2]> {
    let [left, top, right, bottom] = target.0.map(f64::from);
    let row = |r: usize, [x, y]: [f64; 2]| m[r][0] * x + m[r][1] * y + m[r][2];
    let polygon = clip_polygon(polygon, |p| row(0, p) - left * row(2, p));
    let polygon = clip_polygon(&polygon, |p| row(1, p) - top * row(2, p));
    let polygon = clip_polygon(&polygon, |p| right * row(2, p) - row(0, p));
    clip_polygon(&polygon, |p| bottom * row(2, p) - row(1, p))
}

/// The part of the box `local` that `forward` maps in front of the viewer
/// and into `target`: a convex polygon, empty when none of it shows.
fn polygon_shown(forward: &Matrix3, local: PixelBounds, target: PixelBounds) -> Vec<[f64; 2]> {
    let polygon = clip_in_front(forward, &corners(local));
    clip_to_shown(forward, &polygon, target)
}

/// The scale the most magnified corner of `polygon` needs, rounded up as
/// text raster scales are and clamped to `[MIN_SCALE, MAX_SCALE]`; `None`
/// without a corner in front of the viewer.
fn needed_scale(forward: &Matrix3, polygon: &[[f64; 2]]) -> Option<f64> {
    let stretch = polygon
        .iter()
        .filter_map(|&[x, y]| stretch_at(forward, x, y))
        .reduce(f64::max)?;
    Some(round_up_to_eighth_octave(stretch).clamp(MIN_SCALE, MAX_SCALE))
}

/// `w` where `m` maps the point: its distance factor, 1 at the layer's
/// origin and smaller nearer the viewer.
fn w_at(m: &Matrix3, [x, y]: [f64; 2]) -> f64 {
    m[2][0] * x + m[2][1] * y + m[2][2]
}

/// The raster of a plane whose content covers `local` (layer-local units),
/// drawn through `forward` (layer-local units to `target`'s space) onto a
/// space `target` covers; `None` when nothing of it can show. Only the part
/// in front of the viewer that lands in `target` is rasterized, at the scale
/// its most magnified corner needs, rounded up as text raster scales are,
/// and lowered to keep the canvas within `max_texture` on either side and
/// `max_area` pixels in all.
#[cfg(test)]
pub(crate) fn plane_raster(
    forward: &Matrix3,
    local: PixelBounds,
    target: PixelBounds,
    max_texture: u32,
    max_area: u64,
) -> Option<PlaneRaster> {
    let polygon = polygon_shown(forward, local, target);
    let scale = needed_scale(forward, &polygon)?;
    raster_of(&polygon, scale, max_texture, max_area)
}

/// The raster of the box of `polygon` at `scale`, lowered to keep the canvas
/// within `max_texture` on either side and `max_area` pixels in all.
fn raster_of(
    polygon: &[[f64; 2]],
    scale: f64,
    max_texture: u32,
    max_area: u64,
) -> Option<PlaneRaster> {
    let [left, top, right, bottom] = box_of(polygon.iter().copied())?;
    let (width, height) = (right - left, bottom - top);
    if !(width.is_finite() && height.is_finite()) {
        return None;
    }
    let mut scale = scale;
    let side = |extent: f64, scale: f64| (extent * scale).ceil() + 2.0;
    let limit = f64::from(max_texture);
    if side(width, scale) > limit || side(height, scale) > limit {
        scale = (limit - 2.0) / width.max(height);
    }
    // A plane reaching to the horizon would otherwise fill the largest
    // texture: the largest scale whose canvas, with its margin and the
    // rounding up, fits `max_area`, a root of
    // `(width·s + 3)(height·s + 3) = max_area`.
    let max_area = max_area as f64;
    if side(width, scale) * side(height, scale) > max_area {
        let (a, b, c) = (width * height, 3.0 * (width + height), 9.0 - max_area);
        let lowered = if a > 0.0 {
            (-b + (b * b - 4.0 * a * c).sqrt()) / (2.0 * a)
        } else {
            -c / b
        };
        if lowered.is_finite() && lowered > 0.0 {
            scale = scale.min(lowered);
        }
    }
    // Rounding can leave the lowered scale a hair over the limit.
    let side = |extent: f64| side(extent, scale).min(limit) as u32;
    Some(PlaneRaster {
        scale,
        left,
        top,
        width: side(width),
        height: side(height),
    })
}

/// The part of `polygon` whose `w` through `forward` is from `near` less
/// `grow_near` to `far` plus `grow_far`, unbounded on a side that is `None`.
fn part_grown(
    polygon: &[[f64; 2]],
    forward: &Matrix3,
    near: Option<f64>,
    far: Option<f64>,
    grow_near: f64,
    grow_far: f64,
) -> Vec<[f64; 2]> {
    let mut part = polygon.to_vec();
    if let Some(near) = near {
        part = clip_polygon(&part, |point| w_at(forward, point) - (near - grow_near));
    }
    if let Some(far) = far {
        part = clip_polygon(&part, |point| far + grow_far - w_at(forward, point));
    }
    part
}

/// How far half of the output pixel at local `(x, y)` reaches in local
/// units along the direction `w` grows in: half the extent of its
/// footprint, the pixel square mapped back through the inverse of the
/// projection's Jacobian there. `None` behind the viewer.
fn half_footprint(forward: &Matrix3, x: f64, y: f64) -> Option<f64> {
    let m = forward;
    let [px, py, w] = project(m, x, y);
    if w.is_nan() || w <= 0.0 {
        return None;
    }
    let jacobian = |row: usize, col: usize, p: f64| (m[row][col] - p * m[2][col]) / w;
    let (a, b) = (jacobian(0, 0, px), jacobian(0, 1, px));
    let (c, d) = (jacobian(1, 0, py), jacobian(1, 1, py));
    let det = a * d - b * c;
    let length = m[2][0].hypot(m[2][1]);
    let (nx, ny) = (m[2][0] / length, m[2][1] / length);
    // The inverse's columns: a step of one output pixel across, then down.
    let reach = 0.5 * ((nx * d - ny * c).abs() + (-nx * b + ny * a).abs()) / det.abs();
    reach.is_finite().then_some(reach)
}

/// The most bands a plane is split into.
pub(crate) const MAX_BANDS: usize = 8;

/// The texels a band's canvas reaches past its band on a side it shares
/// with another: the footprint filter's taps at a seam, and their bilinear
/// neighbours.
const BAND_MARGIN: f64 = 4.0;

/// Stands for an unbounded side of a band in the values a band's canvas is
/// drawn with: finite, as shaders may assume no infinities.
pub(crate) const BAND_OPEN: f64 = 1e30;

/// One band of a plane: its raster, and the `w` it holds, from `near` up to
/// but not including `far`, `None` where unbounded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PlaneBand {
    pub(crate) raster: PlaneRaster,
    pub(crate) near: Option<f64>,
    pub(crate) far: Option<f64>,
}

/// The bands a plane is drawn in (see `plane_raster` for the arguments);
/// empty when nothing of it can show. A plane whose one canvas would be
/// lowered below the scale its most magnified corner needs is split, when
/// `bandable`, along lines of equal `w` at steps of √2 in `w`, at most
/// `MAX_BANDS`, each band rasterized at the scale its own most magnified
/// corner needs. Each band's canvas reaches `BAND_MARGIN` texels and
/// `reach` local units (how far effects inside spread) past the sides it
/// shares, and together they stay within `max_area` pixels, all lowered
/// alike when they would not. Any other plane is one unbounded band, its
/// raster `plane_raster`'s.
pub(crate) fn plane_bands(
    forward: &Matrix3,
    local: PixelBounds,
    target: PixelBounds,
    max_texture: u32,
    max_area: u64,
    reach: f64,
    bandable: bool,
) -> Vec<PlaneBand> {
    let polygon = polygon_shown(forward, local, target);
    let Some(needed) = needed_scale(forward, &polygon) else {
        return Vec::new();
    };
    let Some(single) = raster_of(&polygon, needed, max_texture, max_area) else {
        return Vec::new();
    };
    let whole = vec![PlaneBand {
        raster: single,
        near: None,
        far: None,
    }];
    if !bandable || single.scale >= needed {
        return whole;
    }
    let (nearest, farthest) = polygon
        .iter()
        .map(|&point| w_at(forward, point))
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(near, far), w| {
            (near.min(w), far.max(w))
        });
    let ratio = farthest / nearest;
    if !(ratio.is_finite() && ratio > 1.0) {
        return whole;
    }
    let count = (ratio.ln() / std::f64::consts::SQRT_2.ln())
        .ceil()
        .clamp(1.0, MAX_BANDS as f64) as usize;
    if count == 1 {
        return whole;
    }
    // Each boundary once, so the bands meeting there share it exactly.
    let boundaries = (1..count)
        .map(|j| nearest * ratio.powf(j as f64 / count as f64))
        .collect::<Vec<_>>();
    let ranges = (0..count)
        .map(|k| {
            (
                k.checked_sub(1).map(|j| boundaries[j]),
                boundaries.get(k).copied(),
            )
        })
        .collect::<Vec<_>>();
    let Some(needed) = ranges
        .iter()
        .map(|&(near, far)| {
            needed_scale(forward, &part_grown(&polygon, forward, near, far, 0.0, 0.0))
        })
        .collect::<Option<Vec<_>>>()
    else {
        return whole;
    };
    // How much `w` changes per local unit, across the bands.
    let gradient = forward[2][0].hypot(forward[2][1]);
    // How far, in local units, half of a pixel on the boundary `b` reaches
    // across it: the footprint the shader filters can span many texels
    // where the plane recedes steeply, and the band on either side must
    // hold what it reads.
    let footprint = |b: f64| {
        clip_polygon(&polygon, |point| w_at(forward, point) - b)
            .into_iter()
            .filter(|&point| (w_at(forward, point) - b).abs() <= 1e-9 * b.abs().max(1.0))
            .filter_map(|[x, y]| half_footprint(forward, x, y))
            .fold(0.0, f64::max)
    };
    let reaches = boundaries.iter().map(|&b| footprint(b)).collect::<Vec<_>>();
    let rasters = |factor: f64| {
        ranges
            .iter()
            .enumerate()
            .zip(&needed)
            .map(|((k, &(near, far)), &scale)| {
                let scale = scale * factor;
                let margin = BAND_MARGIN / scale + reach;
                // The near side's boundary is `boundaries[k - 1]`, the far
                // side's `boundaries[k]`.
                let grow =
                    |side: Option<usize>| side.map_or(0.0, |j| gradient * (margin + reaches[j]));
                let part = part_grown(
                    &polygon,
                    forward,
                    near,
                    far,
                    grow(k.checked_sub(1)),
                    grow(far.map(|_| k)),
                );
                raster_of(&part, scale, max_texture, u64::MAX)
            })
            .collect::<Option<Vec<_>>>()
    };
    let area = |rasters: &[PlaneRaster]| {
        rasters
            .iter()
            .map(|raster| u64::from(raster.width) * u64::from(raster.height))
            .sum::<u64>()
    };
    let Some(mut chosen) = rasters(1.0) else {
        return whole;
    };
    if area(&chosen) > max_area {
        // Lowered alike, from the factor the areas alone suggest down in
        // small steps, as the margins do not shrink with the scale.
        let mut factor = (max_area as f64 / area(&chosen) as f64).sqrt();
        for _ in 0..64 {
            let Some(lowered) = rasters(factor) else {
                return whole;
            };
            chosen = lowered;
            if area(&chosen) <= max_area {
                break;
            }
            factor *= 0.99;
        }
        if area(&chosen) > max_area {
            return whole;
        }
    }
    chosen
        .into_iter()
        .zip(ranges)
        .map(|(raster, (near, far))| PlaneBand { raster, near, far })
        .collect()
}

/// The inverse of the stretch of `forward` at the layer's origin (output
/// pixels per local unit). `forward` maps the origin to `w = 1`, in front of
/// the viewer, so the fallback of 1 only covers a stretch that is not
/// finite.
fn stretch_ratio(forward: &Matrix3) -> f64 {
    stretch_at(forward, 0.0, 0.0).map_or(1.0, f64::recip)
}

/// How much larger than its output size an effect's pixels are drawn on the
/// plane `raster` rasterizes, measured at the layer's origin: `raster.scale`
/// plane pixels per local unit over the output pixels per local unit there,
/// so a blur of `r · effect_scale` plane pixels spreads `r` output pixels
/// and effects scaled by it come out the size they would without the plane.
pub(crate) fn effect_scale(forward: &Matrix3, raster: &PlaneRaster) -> f32 {
    (raster.scale * stretch_ratio(forward)) as f32
}

/// The range of plane raster scales, as for text: `2^-6` to `2^6`.
const MIN_SCALE: f64 = 1.0 / 64.0;
const MAX_SCALE: f64 = 64.0;

/// A plane of the frame being prepared, for hit testing through it.
pub(crate) struct PlaneEntry {
    /// The map from the plane's canvas pixels to its parent's space.
    pub(crate) to_parent: Matrix3,
    /// Index into `GpuRenderer::plane_entries` of the plane it is drawn
    /// onto, if any.
    pub(crate) parent: Option<u32>,
    /// The innermost clip its canvas is drawn through.
    pub(crate) clip: Option<u32>,
}

/// The planes from `plane` outwards: it, its parent, and so on.
fn plane_chain(planes: &[PlaneEntry], plane: Option<u32>) -> impl Iterator<Item = u32> + '_ {
    std::iter::successors(plane, |&index| planes[index as usize].parent)
}

/// `corners`, in the canvas pixels of `plane`, mapped up through each
/// plane's `to_parent` to scene pixels; `None` when any is behind the
/// viewer at any level.
pub(crate) fn corners_to_scene(
    planes: &[PlaneEntry],
    plane: Option<u32>,
    corners: [Point; 4],
) -> Option<[Point; 4]> {
    let mut scene = corners;
    for point in &mut scene {
        for index in plane_chain(planes, plane) {
            let [x, y, w] = project(&planes[index as usize].to_parent, point.x, point.y);
            if w.is_nan() || w <= 0.0 {
                return None;
            }
            *point = Point { x, y };
        }
    }
    Some(scene)
}

/// Every clip a leaf in `plane` is drawn through, innermost first: the
/// clips of `clip` and its parents in the plane's canvas, then those the
/// plane's canvas is drawn through in its parent's space, and so on out to
/// the scene. A plane that cannot be inverted (`None`) cannot be hit.
pub(crate) fn hit_clips(
    clips: &[ClipEntry],
    planes: &[PlaneEntry],
    plane: Option<u32>,
    clip: Option<u32>,
) -> Option<Vec<HitClip>> {
    let chain = plane_chain(planes, plane).collect::<Vec<_>>();
    // The map from each plane's canvas pixels to the scene's, built from
    // the outermost plane in.
    let mut to_scene = Vec::with_capacity(chain.len());
    let mut composed = IDENTITY3;
    for &index in chain.iter().rev() {
        composed = multiply3(&composed, &planes[index as usize].to_parent);
        to_scene.push(composed);
    }
    to_scene.reverse();
    let mut hit_clips = Vec::new();
    let mut clip = clip;
    // The leaf's own space, then each plane's parent's, down to the scene.
    for level in 0..=chain.len() {
        let to_local = match to_scene.get(level) {
            Some(to_scene) => {
                let [[a, b, c], [d, e, f], [g, h, i]] = invert3(to_scene)?;
                Some([a, b, c, d, e, f, g, h, i])
            }
            None => None,
        };
        hit_clips.extend(
            std::iter::successors(clip, |&index| clips[index as usize].parent).map(|index| {
                HitClip {
                    to_local,
                    ..clips[index as usize].hit_clip()
                }
            }),
        );
        if let Some(&index) = chain.get(level) {
            clip = planes[index as usize].clip;
        }
    }
    Some(hit_clips)
}

/// How far the space a plane can show reaches while its parent's content is
/// only measured: unbounded, short of overflowing an f32.
const UNBOUNDED: f32 = 1e9;

impl GpuRenderer {
    /// Prepares the projective `layer`, drawn onto `parent`'s space at
    /// `opacity` with `blend_mode`, as canvases in its own plane: its
    /// content is prepared once in its local units to measure it (and
    /// rolled back), then for real onto each band's canvas at the scale
    /// `plane_bands` picks, which an `EndPlane` then draws through the
    /// layer's homography.
    pub(crate) fn prepare_plane(
        &mut self,
        layer: &Layer,
        parent: LayerState,
        (opacity, blend_mode): (f64, BlendMode),
        output: &mut Vec<PreparedItem>,
    ) -> Result<(), GpuRenderError> {
        let forward = multiply3(&matrix(parent.transform), &layer.transform.homography());
        // Effects are measured in the plane's pixels, which are local units
        // while measuring and `raster.scale` per unit once rasterized.
        let parent_scale = self.walk.effect_scale;
        self.walk.effect_scale = parent_scale * stretch_ratio(&forward) as f32;
        let measured = self.measure_plane(layer, output);
        self.walk.effect_scale = parent_scale;
        let Some(measured) = measured? else {
            return Ok(());
        };
        let target = if self.measuring_plane {
            PixelBounds([-UNBOUNDED, -UNBOUNDED, UNBOUNDED, UNBOUNDED])
        } else {
            let (width, height) = self.scene_size;
            PixelBounds([0.0, 0.0, width as f32, height as f32])
        };
        let bands = plane_bands(
            &forward,
            measured.bounds,
            target,
            self.max_texture_dimension,
            self.plane_area_limit.saturating_mul(self.scene_area),
            measured.reach,
            measured.bandable && !self.measuring_plane,
        );
        // `1/W` is `u·x + v·y + t` at a point `(x, y)` of the parent's
        // space; a band holds `w` from `near` to `far`, so it draws where
        // `1/far < 1/W ≤ 1/near`. Each boundary is turned into `t − 1/b`
        // once, by the same expression for both bands that meet there.
        let Some([u, v, t]) = invert3(&forward).map(|inverse| inverse[2]) else {
            return Ok(());
        };
        let edge = |boundary: Option<f64>, open: f64| boundary.map_or(open, |b| t - b.recip());
        let plane = Plane {
            layer,
            parent,
            opacity,
            blend_mode,
            forward,
            parent_scale,
        };
        // A layer is one hit region however many bands draw it: the ones
        // an earlier band recorded are left out of the later ones.
        let first_leaf = self.hit_leaves.len();
        for band in &bands {
            let before = self.hit_leaves.len();
            let values = if bands.len() == 1 {
                WHOLE_BAND
            } else {
                [u, v, edge(band.far, BAND_OPEN), edge(band.near, -BAND_OPEN)]
                    .map(|value| value as f32)
            };
            self.prepare_band(&plane, band.raster, values, output)?;
            if before > first_leaf {
                let recorded = self.hit_leaves[first_leaf..before]
                    .iter()
                    .map(|leaf| leaf.layer_id.clone())
                    .collect::<HashSet<_>>();
                let mut position = 0;
                self.hit_leaves.retain(|leaf| {
                    let keep = position < before || !recorded.contains(&leaf.layer_id);
                    position += 1;
                    keep
                });
            }
        }
        Ok(())
    }

    /// Prepares one band of `plane` onto a canvas of its own, which its
    /// `EndPlane` draws onto the parent.
    fn prepare_band(
        &mut self,
        plane: &Plane,
        raster: PlaneRaster,
        band: [f32; 4],
        output: &mut Vec<PreparedItem>,
    ) -> Result<(), GpuRenderError> {
        // In steps, so a plane that changes size a little from frame to
        // frame reuses its canvas; the rest is transparent margin.
        let step = |extent: u32| {
            (extent.div_ceil(CanvasRegion::STEP) * CanvasRegion::STEP)
                .min(self.max_texture_dimension)
                .max(extent)
        };
        let (width, height) = (step(raster.width), step(raster.height));
        let to_plane = raster.to_plane();
        let effect_scale = plane.parent_scale * effect_scale(&plane.forward, &raster);
        let to_parent = multiply3(
            &plane.forward,
            &invert3(&matrix(to_plane)).expect("a plane's raster scale is positive"),
        );
        let index = u32::try_from(self.plane_entries.len())
            .map_err(|_| GpuRenderError::TooManyLayers(self.plane_entries.len()))?;
        self.plane_entries.push(PlaneEntry {
            to_parent,
            parent: self.walk.plane,
            clip: plane.parent.clip,
        });
        output.push(PreparedItem::BeginPlane { width, height });
        let (walk, space) = (self.walk, self.scene_size);
        self.walk.plane = Some(index);
        self.walk.effect_scale = effect_scale;
        self.scene_size = (width, height);
        let prepared = self.prepare_plane_content(
            plane.layer,
            LayerState {
                transform: to_plane,
                opacity: 1.0,
                clip: None,
            },
            output,
        );
        (self.walk, self.scene_size) = (walk, space);
        prepared?;
        let [top, middle, bottom] = to_parent.map(|row| row.map(|value| value as f32));
        output.push(PreparedItem::EndPlane(PreparedLayer::plane(
            LayerState {
                transform: Affine {
                    a: top[0],
                    b: middle[0],
                    c: top[1],
                    d: middle[1],
                    tx: top[2],
                    ty: middle[2],
                },
                opacity: (plane.parent.opacity * plane.opacity.clamp(0.0, 1.0) as f32)
                    .clamp(0.0, 1.0),
                clip: plane.parent.clip,
            },
            plane.blend_mode,
            bottom,
            band,
        )));
        Ok(())
    }

    /// The content of the projective `layer` in its own plane, placed by
    /// `state`: what `prepare_layer` prepares of the layer again with only
    /// its anchor, at full opacity, with normal blending, and without
    /// effects, without copying the layer. Such a layer is flat and faces
    /// the viewer, so `self.walk`, which already holds the layer's
    /// orientation, is left as it is, and `state` places it unchanged.
    fn prepare_plane_content(
        &mut self,
        layer: &Layer,
        state: LayerState,
        output: &mut Vec<PreparedItem>,
    ) -> Result<(), GpuRenderError> {
        if state.opacity == 0.0 || state.transform.is_degenerate() {
            return Ok(());
        }
        // At full opacity and normal blending, only a mask isolates a group,
        // and `prepare_content` gives one a canvas before asking.
        self.prepare_content(layer, state, BlendMode::Normal, false, output)
    }

    /// What the projective `layer`'s content covers in its local units,
    /// prepared onto the end of `output` and rolled back; `None` when it
    /// draws nothing.
    fn measure_plane(
        &mut self,
        layer: &Layer,
        output: &mut Vec<PreparedItem>,
    ) -> Result<Option<PlaneMeasure>, GpuRenderError> {
        let start = output.len();
        let texts = self.pending_texts.len();
        let paths = self.pending_paths.len();
        let clips = self.clip_entries.len();
        let paints = self.paint_entries.len();
        let leaves = self.hit_leaves.len();
        let planes = self.plane_entries.len();
        let uses = self.shaders.use_count();
        let measuring = std::mem::replace(&mut self.measuring_plane, true);
        let prepared = self.prepare_plane_content(layer, LayerState::default(), output);
        self.measuring_plane = measuring;
        let measured = prepared.map(|()| {
            // What the text and paths still pending cover, by item.
            let mut pending: Vec<(usize, Option<PixelBounds>)> = Vec::new();
            for text in &self.pending_texts[texts..] {
                pending.push((text.item, Some(text.measured_bounds())));
            }
            for path in &self.pending_paths[paths..] {
                pending.push((path.item, path.shape_box()));
            }
            pending.sort_unstable_by_key(|(item, _)| *item);
            let bounds = content_bounds(&output[start..], |index| {
                let item = start + index;
                pending
                    .binary_search_by_key(&item, |(item, _)| *item)
                    .map_or(None, |found| pending[found].1)
            })?;
            // How far past what it filters an effect inside spreads: the
            // box an empty one's result can touch.
            let reach = output[start..]
                .iter()
                .filter_map(|item| match item {
                    PreparedItem::EndEffect(_, spec) => Some(
                        spec.output_bounds(PixelBounds([0.0; 4]))
                            .0
                            .into_iter()
                            .map(f32::abs)
                            .fold(0.0, f32::max),
                    ),
                    _ => None,
                })
                .fold(0.0, f32::max);
            Some(PlaneMeasure {
                bounds,
                reach: f64::from(reach),
                bandable: !self.shaders.uses_pixel_grid_since(uses),
            })
        });
        output.truncate(start);
        self.pending_texts.truncate(texts);
        self.pending_paths.truncate(paths);
        self.clip_entries.truncate(clips);
        self.paint_entries.truncate(paints);
        self.hit_leaves.truncate(leaves);
        self.plane_entries.truncate(planes);
        self.shaders.truncate_uses(uses);
        measured
    }
}

/// What a plane's content covers, measured in its local units.
struct PlaneMeasure {
    bounds: PixelBounds,
    /// How far an effect inside spreads past what it filters, in local
    /// units.
    reach: f64,
    /// Whether its content may be drawn in bands: none of it runs a shader
    /// that works in the plane's pixel grid.
    bandable: bool,
}

/// A projective layer whose bands are being prepared.
struct Plane<'a> {
    layer: &'a Layer,
    parent: LayerState,
    /// The opacity and blend mode it is drawn onto its parent with.
    opacity: f64,
    blend_mode: BlendMode,
    /// The map from its local units to its parent's space.
    forward: Matrix3,
    /// The parent's `Walk::effect_scale`.
    parent_scale: f32,
}

impl PlaneRaster {
    /// The map from the plane's local units to its canvas pixels:
    /// `Translate(1 − left·scale, 1 − top·scale) · Scale(scale)`.
    fn to_plane(self) -> Affine {
        let scale = self.scale as f32;
        Affine {
            a: scale,
            d: scale,
            tx: (1.0 - self.left * self.scale) as f32,
            ty: (1.0 - self.top * self.scale) as f32,
            ..Affine::IDENTITY
        }
    }
}

/// `transform` as a 3x3 matrix.
fn matrix(transform: Affine) -> Matrix3 {
    [
        [transform.a, transform.c, transform.tx],
        [transform.b, transform.d, transform.ty],
        [0.0, 0.0, 1.0],
    ]
    .map(|row| row.map(f64::from))
}

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_composition::{EvaluatedTransform, IDENTITY3, Point, stretch_at};

    const SCREEN: PixelBounds = PixelBounds([0.0, 0.0, 1920.0, 1080.0]);

    fn entry(to_parent: Matrix3, parent: Option<u32>) -> PlaneEntry {
        PlaneEntry {
            to_parent,
            parent,
            clip: None,
        }
    }

    #[test]
    fn corners_behind_the_viewer_at_any_level_are_dropped() {
        // Each of these maps every point behind the viewer (w = -1); the
        // two together map it back in front (w = 1).
        let behind = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]];
        let front = IDENTITY3;
        let corners = [Point { x: 1.0, y: 2.0 }; 4];
        let planes = [
            entry(behind, None),
            entry(behind, Some(0)),
            entry(front, None),
            entry(front, Some(2)),
        ];
        assert!(corners_to_scene(&planes, Some(1), corners).is_none());
        assert!(corners_to_scene(&planes, Some(0), corners).is_none());
        assert!(corners_to_scene(&planes, Some(2), corners).is_some());
        assert!(corners_to_scene(&planes, Some(3), corners).is_some());
        assert!(corners_to_scene(&planes, None, corners).is_some());
    }

    #[test]
    fn projected_bounds_of_an_affine_canvas_is_its_box() {
        assert_eq!(
            projected_bounds(&IDENTITY3, 10, 20),
            Some(PixelBounds([-2.0, -2.0, 12.0, 22.0]))
        );
    }

    #[test]
    fn projected_bounds_drop_what_is_behind_the_viewer() {
        let m = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [-0.01, 0.0, 1.0]];
        let bounds = projected_bounds(&m, 200, 10).expect("the left part is in front");
        assert!(bounds.0.iter().all(|value| value.is_finite()), "{bounds:?}");
        assert_eq!(bounds.0[0], -2.0);
    }

    #[test]
    fn plane_raster_of_an_unscaled_layer() {
        let raster = plane_raster(
            &IDENTITY3,
            PixelBounds([0.0, 0.0, 100.0, 50.0]),
            SCREEN,
            8192,
            u64::MAX,
        )
        .unwrap();
        assert_eq!(
            raster,
            PlaneRaster {
                scale: 1.0,
                left: 0.0,
                top: 0.0,
                width: 102,
                height: 52,
            }
        );
    }

    #[test]
    fn plane_raster_follows_a_scale() {
        let scale = [[2.0, 0.0, 0.0], [0.0, 2.0, 0.0], [0.0, 0.0, 1.0]];
        let raster = plane_raster(
            &scale,
            PixelBounds([0.0, 0.0, 100.0, 50.0]),
            SCREEN,
            8192,
            u64::MAX,
        )
        .unwrap();
        assert_eq!(raster.scale, 2.0);
        assert_eq!(raster.width, 202);
    }

    #[test]
    fn plane_raster_covers_the_nearest_corner() {
        let forward = EvaluatedTransform {
            position: Point { x: 500.0, y: 500.0 },
            rotation_y: 60.0,
            perspective: Some(1000.0),
            ..EvaluatedTransform::default()
        }
        .homography();
        let raster = plane_raster(
            &forward,
            PixelBounds([-100.0, -100.0, 100.0, 100.0]),
            SCREEN,
            8192,
            u64::MAX,
        )
        .unwrap();
        for (x, y) in [
            (-100.0, -100.0),
            (100.0, -100.0),
            (100.0, 100.0),
            (-100.0, 100.0),
        ] {
            let stretch = stretch_at(&forward, x, y).unwrap();
            assert!(raster.scale >= stretch, "{} < {stretch}", raster.scale);
        }
        let steps = raster.scale.log2() * 8.0;
        assert!((steps - steps.round()).abs() < 1e-9, "{steps}");
    }

    #[test]
    fn plane_raster_is_limited_to_the_visible_part() {
        let raster = plane_raster(
            &IDENTITY3,
            PixelBounds([-1e6, -1e6, 1e6, 1e6]),
            SCREEN,
            8192,
            u64::MAX,
        )
        .unwrap();
        assert_eq!((raster.width, raster.height), (1922, 1082));
    }

    /// The example's floor, whose horizon shows: the screen's top corners
    /// map behind it.
    fn floor() -> Matrix3 {
        EvaluatedTransform {
            position: Point { x: 960.0, y: 800.0 },
            rotation_x: 60.0,
            perspective: Some(900.0),
            ..EvaluatedTransform::default()
        }
        .homography()
    }

    #[test]
    fn plane_raster_of_a_floor_is_scaled_by_what_shows_of_its_near_edge() {
        // The near corners are far off either side of the screen.
        let forward = floor();
        let raster = plane_raster(
            &forward,
            PixelBounds([-900.0, -750.0, 900.0, 294.0]),
            SCREEN,
            8192,
            u64::MAX,
        )
        .unwrap();
        let shown_near_edge = (-64..=64)
            .map(|i| f64::from(i) * 900.0 / 64.0)
            .filter(|&x| (0.0..=1920.0).contains(&project(&forward, x, 294.0)[0]));
        for x in shown_near_edge {
            let stretch = stretch_at(&forward, x, 294.0).unwrap();
            assert!(raster.scale >= stretch, "{} < {stretch}", raster.scale);
        }
        let off_screen = stretch_at(&forward, 900.0, 294.0).unwrap();
        assert!(
            raster.scale < off_screen,
            "{} >= {off_screen}",
            raster.scale
        );
    }

    #[test]
    fn plane_raster_of_a_floor_past_the_screen_bottom_is_scaled_by_the_bottom() {
        // The near part runs off the bottom of the screen and then behind
        // the viewer; what shows ends at the screen's bottom edge.
        let forward = floor();
        let raster = plane_raster(
            &forward,
            PixelBounds([-900.0, -750.0, 900.0, 1500.0]),
            SCREEN,
            8192,
            u64::MAX,
        )
        .unwrap();
        let inverse = invert3(&forward).unwrap();
        let bottom = [0.0, 1920.0]
            .map(|x| {
                let [x, y, _] = project(&inverse, x, 1080.0);
                stretch_at(&forward, x, y).unwrap()
            })
            .into_iter()
            .reduce(f64::max)
            .unwrap();
        assert_eq!(raster.scale, round_up_to_eighth_octave(bottom));
        assert!(raster.width < 8192 && raster.height < 8192, "{raster:?}");
    }

    #[test]
    fn plane_raster_stays_within_the_texture_limit() {
        let raster = plane_raster(
            &IDENTITY3,
            PixelBounds([0.0, 0.0, 10000.0, 10.0]),
            PixelBounds([0.0, 0.0, 20000.0, 20000.0]),
            8192,
            u64::MAX,
        )
        .unwrap();
        assert!(raster.width <= 8192, "{}", raster.width);
        assert!(raster.scale < 1.0, "{}", raster.scale);
    }

    #[test]
    fn plane_raster_stays_within_the_area_limit() {
        let raster = plane_raster(
            &IDENTITY3,
            PixelBounds([0.0, 0.0, 1000.0, 1000.0]),
            PixelBounds([0.0, 0.0, 2000.0, 2000.0]),
            8192,
            4 * 100 * 100,
        )
        .unwrap();
        let area = u64::from(raster.width) * u64::from(raster.height);
        assert!(area <= 4 * 100 * 100, "{area}");
        assert!(raster.scale < 1.0, "{}", raster.scale);
        assert!(
            area > 3 * 100 * 100,
            "the scale is lowered no more than needed: {area}"
        );
    }

    /// A text crawl 20000 tall receding from near the screen's bottom.
    fn crawl() -> Matrix3 {
        EvaluatedTransform {
            position: Point { x: 960.0, y: 900.0 },
            rotation_x: 70.0,
            perspective: Some(800.0),
            ..EvaluatedTransform::default()
        }
        .homography()
    }

    const CRAWL: PixelBounds = PixelBounds([-600.0, -19724.0, 600.0, 276.0]);
    const SCENE_AREA: u64 = 1920 * 1080;

    fn crawl_bands(max_area: u64) -> Vec<PlaneBand> {
        plane_bands(&crawl(), CRAWL, SCREEN, 16384, max_area, 0.0, true)
    }

    #[test]
    fn an_uncapped_plane_is_one_band_equal_to_plane_raster() {
        let local = PixelBounds([-900.0, -750.0, 900.0, 294.0]);
        let bands = plane_bands(&floor(), local, SCREEN, 8192, u64::MAX, 0.0, true);
        let raster = plane_raster(&floor(), local, SCREEN, 8192, u64::MAX).unwrap();
        assert_eq!(
            bands,
            vec![PlaneBand {
                raster,
                near: None,
                far: None
            }]
        );
    }

    #[test]
    fn a_capped_crawl_is_split_into_eight_bands() {
        let bands = crawl_bands(4 * SCENE_AREA);
        assert_eq!(bands.len(), 8, "{bands:?}");
        // The near corners need 2^(5/4), more than the 1.48 the near
        // edge's middle does.
        assert!((bands[0].raster.scale - 2.378).abs() < 1e-3, "{bands:?}");
        assert!((bands[7].raster.scale - 0.068).abs() < 1e-3, "{bands:?}");
        assert_eq!((bands[0].near, bands[7].far), (None, None));
        let single = plane_raster(&crawl(), CRAWL, SCREEN, 16384, 4 * SCENE_AREA).unwrap();
        assert!(
            single.scale < 0.6,
            "the single canvas is capped: {single:?}"
        );
    }

    #[test]
    fn bands_share_their_boundaries_geometrically() {
        let bands = crawl_bands(4 * SCENE_AREA);
        for pair in bands.windows(2) {
            assert_eq!(pair[0].far, pair[1].near);
            assert!(pair[0].far.is_some());
        }
        let ratios = bands[1..7]
            .iter()
            .map(|band| band.far.unwrap() / band.near.unwrap())
            .collect::<Vec<_>>();
        for ratio in &ratios {
            assert!((ratio - ratios[0]).abs() < 1e-9, "{ratios:?}");
        }
        let first = bands[1].near.unwrap() / ratios[0];
        assert!((first - 0.6758).abs() < 1e-3, "{first}");
    }

    #[test]
    fn band_scales_cover_their_corners() {
        let forward = crawl();
        let polygon = polygon_shown(&forward, CRAWL, SCREEN);
        for band in crawl_bands(4 * SCENE_AREA) {
            let near = band.near.unwrap_or(f64::NEG_INFINITY);
            let far = band.far.unwrap_or(f64::INFINITY);
            let part = clip_polygon(&polygon, |p| w_at(&forward, p) - near);
            let part = clip_polygon(&part, |p| far - w_at(&forward, p));
            for [x, y] in part {
                let stretch = stretch_at(&forward, x, y).unwrap();
                assert!(band.raster.scale >= stretch - 1e-9, "{band:?} < {stretch}");
            }
        }
    }

    #[test]
    fn bands_stay_within_the_budget() {
        let bands = crawl_bands(1_000_000);
        let area = bands
            .iter()
            .map(|band| u64::from(band.raster.width) * u64::from(band.raster.height))
            .sum::<u64>();
        assert!(area <= 1_000_000, "{area}");
        assert!(
            bands
                .iter()
                .all(|band| band.raster.width <= 16384 && band.raster.height <= 16384)
        );
        let single = plane_raster(&crawl(), CRAWL, SCREEN, 16384, 1_000_000).unwrap();
        assert!(
            bands[0].raster.scale > single.scale,
            "{bands:?} vs {single:?}"
        );
    }

    #[test]
    fn bands_never_reach_past_the_near_clip() {
        // Nothing below the screen's bottom cuts the plane, so the near
        // clip does.
        let target = PixelBounds([0.0, 0.0, 1920.0, UNBOUNDED]);
        let local = PixelBounds([-600.0, -19724.0, 600.0, 2000.0]);
        let bands = plane_bands(&crawl(), local, target, 16384, 4 * SCENE_AREA, 0.0, true);
        assert!(bands.len() > 1, "{bands:?}");
        let near_y = (1.0 - NEAR_W) * 800.0 / 70f64.to_radians().sin();
        let raster = bands[0].raster;
        let bottom = raster.top + (f64::from(raster.height) - 2.0) / raster.scale;
        assert!(
            bottom <= near_y + 1.0 / raster.scale + 1e-6,
            "{bottom} > {near_y}: {raster:?}"
        );
    }

    #[test]
    fn unbandable_planes_stay_single() {
        let bands = plane_bands(&crawl(), CRAWL, SCREEN, 16384, 4 * SCENE_AREA, 0.0, false);
        let raster = plane_raster(&crawl(), CRAWL, SCREEN, 16384, 4 * SCENE_AREA).unwrap();
        assert_eq!(
            bands,
            vec![PlaneBand {
                raster,
                near: None,
                far: None
            }]
        );
    }

    #[test]
    fn crossing_the_cap_keeps_the_near_scale() {
        // A texture limit that does not bind, so only the area does.
        let bands = |max_area| plane_bands(&crawl(), CRAWL, SCREEN, 65536, max_area, 0.0, true);
        let single = plane_raster(&crawl(), CRAWL, SCREEN, 65536, u64::MAX).unwrap();
        let need = u64::from(single.width) * u64::from(single.height);
        let above = bands(need);
        let below = bands(need - 1);
        assert_eq!(above.len(), 1);
        assert!(below.len() > 1);
        assert_eq!(above[0].raster.scale, below[0].raster.scale);
    }

    #[test]
    fn effects_reach_widens_the_overlap() {
        let tight = crawl_bands(4 * SCENE_AREA);
        let reaching = plane_bands(&crawl(), CRAWL, SCREEN, 16384, 4 * SCENE_AREA, 50.0, true);
        assert!(reaching[1].raster.height > tight[1].raster.height);
        assert_eq!(reaching[1].near, tight[1].near);
    }

    #[test]
    fn effect_scale_is_one_for_a_flat_plane() {
        let raster = PlaneRaster {
            scale: 1.0,
            left: 0.0,
            top: 0.0,
            width: 10,
            height: 10,
        };
        assert_eq!(effect_scale(&IDENTITY3, &raster), 1.0);
    }

    #[test]
    fn effect_scale_follows_the_raster_against_the_projection() {
        // Rasterized 25% finer than the layer is drawn, so a plane pixel
        // is 80% of an output pixel.
        let forward = [[2.0, 0.0, 0.0], [0.0, 2.0, 0.0], [0.0, 0.0, 1.0]];
        let raster = PlaneRaster {
            scale: 2.5,
            left: 0.0,
            top: 0.0,
            width: 10,
            height: 10,
        };
        assert_eq!(effect_scale(&forward, &raster), 1.25);
    }
}
