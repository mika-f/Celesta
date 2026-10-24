//! A layer's geometry for the canvas tools: the map from its own pixels to
//! its parent's space, built with the same functions the renderer places it
//! with, the box it covers in its own pixels, and the position that keeps a
//! chosen point still through a change of its other transform props.
//!
//! A layer's own pixels are relative to its anchor point, as
//! `EvaluatedTransform::homography` lays the layer plane out.

use crate::element_edit::received_number;
use celesta_composition::{EvaluatedTransform, Point};
use celesta_gpu_renderer::{LayerFrame, invert_affine};
use celesta_react_bridge::ReceivedProps;

/// The layer's box in its own pixels, relative to its anchor point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LocalBox {
    pub(crate) min: Point,
    pub(crate) max: Point,
}

impl LocalBox {
    pub(crate) fn size(&self) -> Point {
        Point {
            x: self.max.x - self.min.x,
            y: self.max.y - self.min.y,
        }
    }

    pub(crate) fn center(&self) -> Point {
        self.at(0.5, 0.5)
    }

    /// The point at fractions (`fx`, `fy`) of the box (0,0 top-left, 1,1 bottom-right).
    pub(crate) fn at(&self, fx: f64, fy: f64) -> Point {
        let size = self.size();
        Point {
            x: self.min.x + fx * size.x,
            y: self.min.y + fy * size.y,
        }
    }
}

/// The layer's own-pixel → parent map for `transform`: `[a, b, c, d, tx, ty]`
/// built from `EvaluatedTransform::linear` and `translation` (2D part).
pub(crate) fn layer_map(transform: &EvaluatedTransform) -> [f64; 6] {
    let linear = transform.linear();
    let translation = transform.translation(&linear);
    [
        linear[0][0],
        linear[1][0],
        linear[0][1],
        linear[1][1],
        translation.x,
        translation.y,
    ]
}

/// The pivot in parent space: where `translation` puts the turning point.
pub(crate) fn pivot(transform: &EvaluatedTransform) -> Point {
    apply(&layer_map(transform), transform.origin)
}

/// The box `frame.outline` encloses, in the layer's own pixels; `None` when a
/// map collapses, or when the renderer draws the layer through its
/// homography (a perspective that tilts it), which `layer_map` does not
/// describe.
pub(crate) fn local_box(frame: &LayerFrame) -> Option<LocalBox> {
    own_box(frame, frame.outline)
}

/// The box the layer's anchor is a fraction of, in its own pixels (see
/// [`with_anchor_box`]); `None` when its anchor has no effect or the box is
/// unknown, and as for [`local_box`].
pub(crate) fn anchor_box(frame: &LayerFrame) -> Option<LocalBox> {
    own_box(frame, frame.anchor_box?)
}

/// The props that put a layer's anchor point at fractions of its box.
pub(crate) const ANCHOR_PROPS: [&str; 2] = ["anchorX", "anchorY"];

/// `@celesta/shapes` primitives sized by `radius`: their box is `2 * radius`
/// square.
const RADIUS_SHAPES: [&str; 4] = ["Circle", "Polygon", "Star", "Pie"];
/// Primitives that bake their anchor into what they draw against `width` x
/// `height`: the shapes drawn as a path (whose own origin is the anchor
/// point), and a dashed `<Rect>`, drawn as a group of a rect and a path.
const SIZED_SHAPES: [&str; 4] = ["Rect", "Ellipse", "Triangle", "Heart"];

/// `frame` with `anchor_box` set to the box the source element's anchor is a
/// fraction of, for the element `primitive` names (`report.chain`'s first)
/// with the props the runtime passed it (`received`):
/// - a `<Group>`'s anchor has no effect, as the renderer places its children
///   by its position alone: none;
/// - a shape (and a `<Rect>`, which can be drawn as a group) puts its anchor
///   at `anchorX`, `anchorY` (0 when not passed, as every one defaults) of its
///   `width` x `height` (or `2 * radius`) box; `transform.anchor` becomes
///   that anchor too, as the layer drawing a shape's path has an anchor of
///   its own that places nothing (`layer_map` never reads it);
/// - anything else keeps the renderer's: a text's layout box, a rect's,
///   image's or video's quad, none for a path.
pub(crate) fn with_anchor_box(
    mut frame: LayerFrame,
    primitive: &str,
    received: &ReceivedProps,
) -> LayerFrame {
    if primitive == "Group" {
        frame.anchor_box = None;
        return frame;
    }
    let number = |name: &str| received_number(received, name);
    let size = if RADIUS_SHAPES.contains(&primitive) {
        number("radius").map(|radius| (2.0 * radius, 2.0 * radius))
    } else if SIZED_SHAPES.contains(&primitive) {
        number("width").zip(number("height"))
    } else {
        None
    };
    if let Some((width, height)) = size {
        let [ax, ay] = ANCHOR_PROPS.map(|name| number(name).unwrap_or(0.0));
        let map = layer_map(&frame.transform);
        frame.anchor_box = Some(
            [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)].map(|(u, v)| {
                let local = Point {
                    x: (u - ax) * width,
                    y: (v - ay) * height,
                };
                apply(&frame.parent, apply(&map, local))
            }),
        );
        frame.transform.anchor = Point { x: ax, y: ay };
    }
    frame
}

/// The box `quad` (scene pixels, a parallelogram the layer's map makes of a
/// box) encloses in the layer's own pixels; `None` as for [`local_box`].
fn own_box(frame: &LayerFrame, quad: [Point; 4]) -> Option<LocalBox> {
    let transform = &frame.transform;
    if transform.perspective_distance().is_some() && transform.is_projective() {
        return None;
    }
    let parent = invert_affine(frame.parent)?;
    let layer = invert_affine(layer_map(&frame.transform))?;
    let corners = quad.map(|corner| apply(&layer, apply(&parent, corner)));
    let fold = |pick: fn(f64, f64) -> f64| Point {
        x: corners.iter().map(|p| p.x).reduce(pick).unwrap_or_default(),
        y: corners.iter().map(|p| p.y).reduce(pick).unwrap_or_default(),
    };
    Some(LocalBox {
        min: fold(f64::min),
        max: fold(f64::max),
    })
}

/// `position` that keeps `fixed_parent` where it is once the layer's props
/// become `after` (with `after.position` still the old position) and the fixed
/// point sits at `fixed_local` in the new own pixels.
pub(crate) fn keep_fixed(
    after: &EvaluatedTransform,
    fixed_local: Point,
    fixed_parent: Point,
) -> Point {
    // The map moves by exactly what `position` moves by, so the miss is the
    // change.
    let missed = apply(&layer_map(after), fixed_local);
    Point {
        x: after.position.x + fixed_parent.x - missed.x,
        y: after.position.y + fixed_parent.y - missed.y,
    }
}

/// `m` applied to `p` (`x' = a x + c y + tx`).
pub(crate) fn apply(m: &[f64; 6], p: Point) -> Point {
    Point {
        x: m[0] * p.x + m[2] * p.y + m[4],
        y: m[1] * p.x + m[3] * p.y + m[5],
    }
}

/// The frame of a `width` × `height` box placed by `transform` inside
/// `parent`, its outline mapped with `layer_map` as the renderer lays a quad
/// out: the box's corners relative to the anchor point.
#[cfg(test)]
pub(crate) fn box_frame(
    transform: EvaluatedTransform,
    parent: [f64; 6],
    width: f64,
    height: f64,
) -> LayerFrame {
    let own = layer_map(&transform);
    let anchor = transform.anchor;
    let outline = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)].map(|(u, v)| {
        let local = Point {
            x: (u - anchor.x) * width,
            y: (v - anchor.y) * height,
        };
        apply(&parent, apply(&own, local))
    });
    LayerFrame {
        transform,
        parent,
        outline,
        // A box primitive's quad is the box its anchor is a fraction of.
        anchor_box: Some(outline),
        leaves: vec!["a".to_owned()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_react_bridge::ReceivedValue;

    fn at(x: f64, y: f64) -> Point {
        Point { x, y }
    }

    fn assert_near(actual: Point, expected: Point) {
        assert!(
            (actual.x - expected.x).abs() < 1e-9 && (actual.y - expected.y).abs() < 1e-9,
            "{actual:?} != {expected:?}"
        );
    }

    /// `m` applied to `p`, written out here so the tests do not lean on the
    /// module's own helper.
    fn map(m: [f64; 6], p: Point) -> Point {
        at(
            m[0] * p.x + m[2] * p.y + m[4],
            m[1] * p.x + m[3] * p.y + m[5],
        )
    }

    /// The frame of a `width` × `height` box placed by `transform` inside
    /// `parent`: its outline is the box's corners, relative to the anchor
    /// point as the renderer lays a quad out, mapped to the scene.
    fn frame(
        transform: EvaluatedTransform,
        parent: [f64; 6],
        width: f64,
        height: f64,
    ) -> LayerFrame {
        let anchor = transform.anchor;
        let (s, c) = transform.rotation.to_radians().sin_cos();
        let Point { x: sx, y: sy } = transform.scale;
        // Only rotation, scale and position, with no origin: `layer_map`'s
        // layout written out by hand.
        let own = [
            c * sx,
            s * sx,
            -s * sy,
            c * sy,
            transform.position.x,
            transform.position.y,
        ];
        let outline = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)].map(|(u, v)| {
            let local = at((u - anchor.x) * width, (v - anchor.y) * height);
            map(parent, map(own, local))
        });
        LayerFrame {
            transform,
            parent,
            outline,
            anchor_box: Some(outline),
            leaves: vec!["a".to_owned()],
        }
    }

    #[test]
    fn layer_map_matches_the_renderer_layout() {
        let transform = EvaluatedTransform {
            position: at(100.0, 50.0),
            rotation: 90.0,
            scale: at(2.0, 1.0),
            ..EvaluatedTransform::default()
        };
        assert_near(map(layer_map(&transform), at(10.0, 0.0)), at(100.0, 70.0));
        assert_near(pivot(&transform), at(100.0, 50.0));

        let with_origin = EvaluatedTransform {
            origin: at(5.0, 0.0),
            ..transform
        };
        assert_near(pivot(&with_origin), at(105.0, 50.0));
        assert_near(map(layer_map(&with_origin), at(5.0, 0.0)), at(105.0, 50.0));
    }

    #[test]
    fn local_box_reads_the_outline() {
        let parent = [2.0, 0.0, 0.0, 2.0, 7.0, -3.0];
        let centered = EvaluatedTransform {
            position: at(40.0, 30.0),
            rotation: 30.0,
            ..EvaluatedTransform::default()
        };
        let local = local_box(&frame(centered, parent, 200.0, 100.0)).expect("a box");
        assert_near(local.size(), at(200.0, 100.0));
        assert_near(local.center(), at(0.0, 0.0));
        assert_near(local.min, at(-100.0, -50.0));
        assert_near(local.at(1.0, 0.5), at(100.0, 0.0));

        let top_left = EvaluatedTransform {
            anchor: at(0.0, 0.0),
            ..centered
        };
        let local = local_box(&frame(top_left, parent, 200.0, 100.0)).expect("a box");
        assert_near(local.min, at(0.0, 0.0));
        assert_near(local.max, at(200.0, 100.0));

        // An origin moves the turning point, not the box: it stays relative
        // to the anchor point.
        let with_origin = EvaluatedTransform {
            origin: at(40.0, 10.0),
            scale: at(1.5, 0.5),
            ..centered
        };
        let local = local_box(&box_frame(with_origin, parent, 200.0, 100.0)).expect("a box");
        assert_near(local.min, at(-100.0, -50.0));
        assert_near(local.max, at(100.0, 50.0));
    }

    fn received(props: &[(&str, f64)]) -> ReceivedProps {
        ReceivedProps {
            others: props
                .iter()
                .map(|(name, value)| ((*name).to_owned(), ReceivedValue::Number(*value)))
                .collect(),
            ..ReceivedProps::default()
        }
    }

    #[test]
    fn the_anchor_box_is_the_one_the_source_element_anchors_in() {
        let parent = [2.0, 0.0, 0.0, 2.0, 7.0, -3.0];
        let turned = EvaluatedTransform {
            position: at(40.0, 30.0),
            rotation: 30.0,
            anchor: at(0.0, 0.0),
            ..EvaluatedTransform::default()
        };
        // A path drawn by a shape: the renderer knows no anchor box, and its
        // outline reaches past the shape's box by half the stroke.
        let mut path = box_frame(turned, parent, 120.0, 70.0);
        path.anchor_box = None;
        let anchored = |primitive, props: &[(&str, f64)]| {
            anchor_box(&with_anchor_box(path.clone(), primitive, &received(props)))
        };
        // `<Ellipse>` bakes its anchor into its path against `width` x `height`.
        let ellipse = anchored(
            "Ellipse",
            &[("width", 100.0), ("height", 50.0), ("anchorX", 0.5)],
        )
        .expect("a box");
        assert_near(ellipse.min, at(-50.0, 0.0));
        assert_near(ellipse.max, at(50.0, 50.0));
        // Its anchor is the props', not the one of the layer drawing its path.
        let mut centered = path.clone();
        centered.transform.anchor = at(0.5, 0.5);
        let shape = received(&[("width", 100.0), ("height", 50.0), ("anchorX", 0.25)]);
        assert_eq!(
            with_anchor_box(centered, "Ellipse", &shape)
                .transform
                .anchor,
            at(0.25, 0.0)
        );
        // `<Circle>` and the radius shapes against `2 * radius`.
        let circle = anchored("Circle", &[("radius", 30.0), ("anchorY", 1.0)]).expect("a box");
        assert_near(circle.min, at(0.0, -60.0));
        assert_near(circle.max, at(60.0, 0.0));
        assert!(anchored("Star", &[("radius", 30.0)]).is_some());
        // A dashed `<Rect>` is drawn as a group but anchors its box.
        let rect = anchored("Rect", &[("width", 80.0), ("height", 20.0)]).expect("a box");
        assert_near(rect.max, at(80.0, 20.0));
        // A shape whose size the runtime did not report keeps what the
        // renderer knows, here nothing.
        assert_eq!(anchored("Ellipse", &[]), None);
        // A plain path has no anchor.
        assert_eq!(anchored("Path", &[]), None);

        // A text's layout box comes from the renderer, smaller than its quad.
        let mut text = box_frame(turned, parent, 120.0, 70.0);
        let layout = box_frame(turned, parent, 100.0, 50.0).outline;
        text.anchor_box = Some(layout);
        let text = with_anchor_box(text, "Text", &received(&[]));
        assert_near(anchor_box(&text).expect("a box").max, at(100.0, 50.0));
        // A `<Group>` places its children by its position: its anchor has no
        // effect, whatever box the renderer reports.
        let group = with_anchor_box(
            box_frame(turned, parent, 120.0, 70.0),
            "Group",
            &received(&[]),
        );
        assert_eq!(anchor_box(&group), None);
    }

    #[test]
    fn a_projective_layer_has_no_local_box() {
        // The renderer draws it through its homography, which `layer_map`
        // does not describe.
        let tilted = EvaluatedTransform {
            rotation_y: 30.0,
            perspective: Some(800.0),
            ..EvaluatedTransform::default()
        };
        assert!(tilted.is_projective());
        let parent = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
        assert_eq!(local_box(&box_frame(tilted, parent, 100.0, 50.0)), None);
        // Without a perspective it is drawn flat, by its affine part.
        let flat = EvaluatedTransform {
            perspective: None,
            ..tilted
        };
        assert!(local_box(&box_frame(flat, parent, 100.0, 50.0)).is_some());
    }

    #[test]
    fn a_collapsed_map_has_no_local_box() {
        let flat = EvaluatedTransform {
            scale: at(0.0, 1.0),
            ..EvaluatedTransform::default()
        };
        let mut collapsed = frame(flat, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], 10.0, 10.0);
        assert_eq!(local_box(&collapsed), None);
        collapsed.transform = EvaluatedTransform::default();
        collapsed.parent = [1.0, 2.0, 2.0, 4.0, 0.0, 0.0];
        assert_eq!(local_box(&collapsed), None);
    }

    #[test]
    fn keeping_the_center_through_a_rotation() {
        let before = EvaluatedTransform {
            position: at(10.0, 10.0),
            anchor: at(0.0, 0.0),
            ..EvaluatedTransform::default()
        };
        let after = EvaluatedTransform {
            rotation: 90.0,
            ..before
        };
        // Under no parent and under one rotated 45° and scaled 2: the fixed
        // point is in parent space, so the parent cancels.
        let (sin, cos) = 45.0_f64.to_radians().sin_cos();
        for parent in [
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            [2.0 * cos, 2.0 * sin, -2.0 * sin, 2.0 * cos, 30.0, -20.0],
        ] {
            let local = local_box(&frame(before, parent, 100.0, 50.0)).expect("a box");
            let center = local.center();
            assert_near(center, at(50.0, 25.0));
            let fixed_parent = map(layer_map(&before), center);
            assert_near(fixed_parent, at(60.0, 35.0));

            let position = keep_fixed(&after, center, fixed_parent);
            // M'(c) = (10 − 25, 10 + 50) = (−15, 60), so the layer moves by
            // (60 − (−15), 35 − 60).
            assert_near(position, at(85.0, -15.0));
            let moved = EvaluatedTransform { position, ..after };
            assert_near(
                map(parent, map(layer_map(&moved), center)),
                map(parent, map(layer_map(&before), center)),
            );
        }
    }

    #[test]
    fn keeping_a_point_with_skew_and_origin() {
        let before = EvaluatedTransform {
            position: at(30.0, 20.0),
            skew: at(20.0, 0.0),
            origin: at(40.0, 0.0),
            ..EvaluatedTransform::default()
        };
        let after = EvaluatedTransform {
            scale: at(1.5, 0.8),
            ..before
        };
        let fixed_local = at(-50.0, 25.0);
        let fixed_parent = map(layer_map(&before), fixed_local);

        let position = keep_fixed(&after, fixed_local, fixed_parent);
        assert!(
            (position.x - before.position.x).abs() > 1.0,
            "the scale moves the point, so the position compensates: {position:?}"
        );
        let moved = EvaluatedTransform { position, ..after };
        assert_near(map(layer_map(&moved), fixed_local), fixed_parent);
    }
}
