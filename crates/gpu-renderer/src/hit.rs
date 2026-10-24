//! Where each leaf layer of a frame was drawn, so a point in the scene can
//! be mapped back to the topmost layer under it.

use celesta_composition::Point;

/// A leaf layer's quad in scene pixels and the clips it was drawn through.
#[derive(Clone, Debug, PartialEq)]
pub struct HitRegion {
    pub layer_id: String,
    /// The quad's corners in order around it: the ones that were its
    /// top-left, top-right, bottom-right and bottom-left before the layer's
    /// transform. A path's and an `Instances` layer's are those of the box
    /// around their shapes.
    pub corners: [Point; 4],
    /// The quad inside a stroked rect with no fill (or a fully transparent
    /// one), in the same order as `corners`: only the stroke band around it
    /// is hit, so an outline does not cover the layers under it. `None` for
    /// every other layer and for a stroke that covers the whole rect. The
    /// inner corners' rounding is left out, so a click there misses.
    pub hole: Option<[Point; 4]>,
    /// The box the layer's anchor is a fraction of, in the same order as
    /// `corners`, when it is not the quad: a text's layout box (`maxWidth`
    /// or its measured width), which its texture pads for strokes and
    /// glyphs that reach past it. `None` for every other layer.
    pub anchor_box: Option<[Point; 4]>,
    /// For a drawn text, its layout box — the layout width (`maxWidth` or
    /// its measured width) by `lines · line_height`, from the first line's
    /// top — in scene pixels, in the order of `corners`, projected like
    /// them. Unlike `anchor_box`, a single line's is its line box, not the
    /// rows of its ink. `None` for every other layer, and when a corner is
    /// behind the viewer.
    pub text_box: Option<[Point; 4]>,
    /// Every clip the layer is drawn through, innermost first.
    pub clips: Vec<HitClip>,
}

/// One clipped group, as `ClipEntry` describes it to the fragment shader.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HitClip {
    /// Maps a scene position into the group's frame, relative to the clip
    /// rectangle's centre.
    pub(crate) inverse: [f32; 6],
    pub(crate) half: [f32; 2],
    pub(crate) radius: f32,
    /// Maps a scene position into the space the clip is in, row-major, when
    /// that is a projective plane's canvas; `None` for the scene itself.
    pub(crate) to_local: Option<[f64; 9]>,
}

impl HitClip {
    /// Whether `point`, in scene pixels, is inside the clip: the rounded box
    /// `clip_coverage` in `layer.wgsl` measures, its edge counted inside.
    pub fn contains(&self, point: Point) -> bool {
        let point = match self.to_local {
            None => point,
            Some([a, b, c, d, e, f, g, h, i]) => {
                let w = g * point.x + h * point.y + i;
                // Behind the viewer, or not a number.
                if w.is_nan() || w <= 0.0 {
                    return false;
                }
                Point {
                    x: (a * point.x + b * point.y + c) / w,
                    y: (d * point.x + e * point.y + f) / w,
                }
            }
        };
        let [a, b, c, d, tx, ty] = self.inverse.map(f64::from);
        let local = [
            a * point.x + c * point.y + tx,
            b * point.x + d * point.y + ty,
        ];
        let radius = f64::from(self.radius);
        let [qx, qy] = [0, 1].map(|axis| local[axis].abs() - f64::from(self.half[axis]) + radius);
        let distance = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius;
        distance <= 0.0
    }
}

impl HitRegion {
    /// The quad's area in square scene pixels, 0 for one collapsed to a
    /// line or a point (and for one with a corner that is not finite).
    pub(crate) fn area(&self) -> f64 {
        let twice = (0..4)
            .map(|i| {
                let (from, to) = (self.corners[i], self.corners[(i + 1) % 4]);
                from.x * to.y - to.x * from.y
            })
            .sum::<f64>();
        if twice.is_finite() {
            twice.abs() / 2.0
        } else {
            0.0
        }
    }

    /// Whether `point`, in scene pixels, is inside the quad and every clip.
    /// A quad without area contains nothing: every point is on the line it
    /// collapsed to as far as its edges tell.
    fn contains(&self, point: Point) -> bool {
        if self.area() == 0.0 {
            return false;
        }
        let outer = sides(&self.corners, point);
        (outer.iter().all(|side| *side >= 0.0) || outer.iter().all(|side| *side <= 0.0))
            && !self.hole.is_some_and(|hole| {
                // Strictly inside: the hole's edge is the stroke's.
                let inner = sides(&hole, point);
                inner.iter().all(|side| *side > 0.0) || inner.iter().all(|side| *side < 0.0)
            })
            && self.clips.iter().all(|clip| clip.contains(point))
    }
}

/// Which side of each of the quad's edges `point` is on. Inside a convex
/// quad, it is the same side of every edge, whichever way round a mirroring
/// transform turned the corners.
fn sides(corners: &[Point; 4], point: Point) -> [f64; 4] {
    [0, 1, 2, 3].map(|i| {
        let (from, to) = (corners[i], corners[(i + 1) % 4]);
        (to.x - from.x) * (point.y - from.y) - (to.y - from.y) * (point.x - from.x)
    })
}

/// A leaf layer `prepare_layer` met while recording, whose corners are
/// known once its item is resolved.
pub(crate) struct HitLeaf {
    /// Index of its item (an `Instances` layer's particles) in the frame's
    /// items, before `prepare_draws` drops the paths that cover no pixel.
    pub(crate) item: usize,
    pub(crate) layer_id: String,
    /// The innermost clip the layer is drawn through.
    pub(crate) clip: Option<u32>,
    /// Index into `GpuRenderer::plane_entries` of the projective plane the
    /// layer is drawn onto, if any: its corners and clips are in that
    /// plane's canvas pixels.
    pub(crate) plane: Option<u32>,
}

/// The id of the topmost region containing `point`: regions are in
/// painter's order, so the last one.
pub fn hit_test(regions: &[HitRegion], point: Point) -> Option<&str> {
    regions
        .iter()
        .rev()
        .find(|region| region.contains(point))
        .map(|region| region.layer_id.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region(id: &str, corners: [(f64, f64); 4]) -> HitRegion {
        HitRegion {
            layer_id: id.to_owned(),
            corners: corners.map(|(x, y)| Point { x, y }),
            hole: None,
            anchor_box: None,
            text_box: None,
            clips: Vec::new(),
        }
    }

    fn square(id: &str) -> HitRegion {
        region(id, [(10.0, 10.0), (20.0, 10.0), (20.0, 20.0), (10.0, 20.0)])
    }

    #[test]
    fn hits_a_square_only_inside_it() {
        let regions = [square("square")];
        assert_eq!(
            hit_test(&regions, Point { x: 15.0, y: 15.0 }),
            Some("square")
        );
        assert_eq!(hit_test(&regions, Point { x: 25.0, y: 15.0 }), None);
    }

    #[test]
    fn hits_a_rotated_square_by_its_own_edges() {
        // The square above, turned 45° about its centre: a diamond.
        let (centre, reach) = (15.0, 5.0 * std::f64::consts::SQRT_2);
        let regions = [region(
            "diamond",
            [
                (centre, centre - reach),
                (centre + reach, centre),
                (centre, centre + reach),
                (centre - reach, centre),
            ],
        )];
        assert_eq!(
            hit_test(&regions, Point { x: 15.0, y: 9.0 }),
            Some("diamond")
        );
        // Inside the square's box, outside the diamond.
        assert_eq!(hit_test(&regions, Point { x: 11.0, y: 11.0 }), None);
    }

    #[test]
    fn hits_the_later_of_two_overlapping_regions() {
        let regions = [square("below"), square("above")];
        assert_eq!(
            hit_test(&regions, Point { x: 15.0, y: 15.0 }),
            Some("above")
        );
    }

    #[test]
    fn misses_a_region_outside_its_clip() {
        let mut clipped = square("clipped");
        // The left half of the square: (10,10)-(15,20).
        clipped.clips.push(HitClip {
            inverse: [1.0, 0.0, 0.0, 1.0, -12.5, -15.0],
            half: [2.5, 5.0],
            radius: 0.0,
            to_local: None,
        });
        let regions = [clipped];
        assert_eq!(
            hit_test(&regions, Point { x: 12.0, y: 15.0 }),
            Some("clipped")
        );
        assert_eq!(hit_test(&regions, Point { x: 17.0, y: 15.0 }), None);
    }

    #[test]
    fn clips_through_a_projective_map_only_in_front_of_the_viewer() {
        // The square's clip, reached through w = 1 - x/20: a point at
        // x >= 20 is behind the viewer and in no clip.
        let mut clipped = square("clipped");
        clipped.clips.push(HitClip {
            inverse: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            half: [100.0, 100.0],
            radius: 0.0,
            to_local: Some([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, -0.05, 0.0, 1.0]),
        });
        let regions = [clipped];
        assert_eq!(
            hit_test(&regions, Point { x: 15.0, y: 15.0 }),
            Some("clipped")
        );
        let mut behind = regions[0].clone();
        behind.corners =
            [(10.0, 10.0), (30.0, 10.0), (30.0, 20.0), (10.0, 20.0)].map(|(x, y)| Point { x, y });
        assert_eq!(hit_test(&[behind], Point { x: 25.0, y: 15.0 }), None);
    }

    #[test]
    fn misses_a_region_its_outer_clip_excludes() {
        let mut clipped = square("clipped");
        // Innermost first: the left half of the square, (10,10)-(15,20),
        // inside a clip of its right half, (15,10)-(20,20).
        clipped.clips.push(HitClip {
            inverse: [1.0, 0.0, 0.0, 1.0, -12.5, -15.0],
            half: [2.5, 5.0],
            radius: 0.0,
            to_local: None,
        });
        clipped.clips.push(HitClip {
            inverse: [1.0, 0.0, 0.0, 1.0, -17.5, -15.0],
            half: [2.5, 5.0],
            radius: 0.0,
            to_local: None,
        });
        let regions = [clipped];
        assert_eq!(hit_test(&regions, Point { x: 12.0, y: 15.0 }), None);
        assert_eq!(hit_test(&regions, Point { x: 17.0, y: 15.0 }), None);
    }

    #[test]
    fn misses_a_quad_without_area() {
        // A rect 0 wide and 0 high, then one only 0 high.
        let point = region("point", [(15.0, 15.0); 4]);
        let line = region(
            "line",
            [(10.0, 15.0), (20.0, 15.0), (20.0, 15.0), (10.0, 15.0)],
        );
        for at in [(15.0, 15.0), (0.0, 0.0), (12.0, 15.0), (40.0, 15.0)] {
            let at = Point { x: at.0, y: at.1 };
            assert_eq!(hit_test(&[point.clone(), line.clone()], at), None);
        }
    }

    /// The square above as an outline 2 wide: its hole is (12,12)-(18,18).
    fn outline(id: &str) -> HitRegion {
        HitRegion {
            hole: Some(
                [(12.0, 12.0), (18.0, 12.0), (18.0, 18.0), (12.0, 18.0)]
                    .map(|(x, y)| Point { x, y }),
            ),
            ..square(id)
        }
    }

    #[test]
    fn hits_an_outline_only_on_its_band() {
        let regions = [outline("outline")];
        for at in [(11.0, 15.0), (15.0, 19.0), (10.0, 10.0), (12.0, 15.0)] {
            assert_eq!(
                hit_test(&regions, Point { x: at.0, y: at.1 }),
                Some("outline"),
                "{at:?}"
            );
        }
        assert_eq!(hit_test(&regions, Point { x: 15.0, y: 15.0 }), None);
    }

    #[test]
    fn hits_the_layer_below_an_outlines_hole() {
        let regions = [square("below"), outline("outline")];
        assert_eq!(
            hit_test(&regions, Point { x: 15.0, y: 15.0 }),
            Some("below")
        );
        assert_eq!(
            hit_test(&regions, Point { x: 11.0, y: 15.0 }),
            Some("outline")
        );
    }

    #[test]
    fn misses_without_regions() {
        assert_eq!(hit_test(&[], Point { x: 15.0, y: 15.0 }), None);
    }
}
