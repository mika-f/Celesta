//! Where a selected layer sits in a frame: its own transform, the map from
//! its parent's space to scene pixels, and an outline to draw around it, so a
//! pointer drag in the scene can be turned into a change of the transform.

use crate::hit::HitRegion;
use crate::transform::Affine;
use celesta_composition::{EvaluatedTransform, Layer, LayerContent, Point, Scene};

/// One layer's frame, as `layer_frame` measures it.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerFrame {
    /// The layer's own transform.
    pub transform: EvaluatedTransform,
    /// Maps the layer's parent space to scene pixels, as `[a, b, c, d, tx, ty]`
    /// (`x' = a x + c y + tx`, `y' = b x + d y + ty`), composed with the same
    /// `Affine` the renderer draws with.
    pub parent: [f64; 6],
    /// A leaf's quad in scene pixels, in the order of its `HitRegion`; for a
    /// group, the axis-aligned box around its descendants' regions.
    pub outline: [Point; 4],
    /// The box the renderer anchors the layer in, in scene pixels in the
    /// order of `outline`: what `anchor` is a fraction of. A text's layout
    /// box (see `HitRegion::anchor_box`), a rect's, image's or video's quad.
    /// `None` where the anchor has no effect: a group, a path, instances.
    pub anchor_box: Option<[Point; 4]>,
    /// The leaf layers a hit test can name for this layer: the layer itself,
    /// or a group's descendant leaves.
    pub leaves: Vec<String>,
}

/// The frame of layer `id` in `scene`, drawn as `regions` records. `None` when
/// the scene has no such layer, an ancestor turns in 3D or has a perspective
/// (its map is not an affine one), the parents' map collapses the plane, or
/// nothing of the layer was drawn.
pub fn layer_frame(scene: &Scene, regions: &[HitRegion], id: &str) -> Option<LayerFrame> {
    let (layer, parent) = find(&scene.layers, Affine::IDENTITY, id)??;
    if parent.is_degenerate() {
        return None;
    }
    let mut ids = Vec::new();
    leaf_ids(layer, &mut ids);
    let (outline, anchor_box) = match &layer.content {
        LayerContent::Group { .. } => (
            bounding_box(
                regions
                    .iter()
                    .filter(|region| ids.contains(&region.layer_id.as_str())),
            )?,
            None,
        ),
        content => {
            let region = regions.iter().find(|region| region.layer_id == id)?;
            // A path and instances are placed by their own origin.
            let anchored = !matches!(
                content,
                LayerContent::Path { .. } | LayerContent::Instances { .. }
            );
            (
                region.corners,
                anchored.then(|| region.anchor_box.unwrap_or(region.corners)),
            )
        }
    };
    Some(LayerFrame {
        transform: layer.transform,
        parent: [parent.a, parent.b, parent.c, parent.d, parent.tx, parent.ty].map(f64::from),
        outline,
        anchor_box,
        leaves: ids.into_iter().map(str::to_owned).collect(),
    })
}

/// The layer `id` and the map from its parent's space to the scene. The outer
/// `None` is "not in these layers"; the inner one, "found below a layer whose
/// map is not affine". A group's own anchor plays no part, as in the renderer,
/// which places a group's children by its position, scale, rotation and skew.
fn find<'a>(layers: &'a [Layer], parent: Affine, id: &str) -> Option<Option<(&'a Layer, Affine)>> {
    for layer in layers {
        if layer.id == id {
            return Some(Some((layer, parent)));
        }
        let LayerContent::Group { layers, .. } = &layer.content else {
            continue;
        };
        let transform = &layer.transform;
        let found = find(
            layers,
            parent.multiply(Affine::from_transform(transform)),
            id,
        );
        match found {
            None => {}
            Some(_)
                if transform.rotation_x != 0.0
                    || transform.rotation_y != 0.0
                    || transform.perspective.is_some() =>
            {
                return Some(None);
            }
            Some(found) => return Some(found),
        }
    }
    None
}

/// The ids of the leaf layers under `layer` (itself, for a leaf), the ones
/// `HitRegion`s are recorded for.
fn leaf_ids<'a>(layer: &'a Layer, ids: &mut Vec<&'a str>) {
    match &layer.content {
        LayerContent::Group { layers, .. } => {
            for child in layers {
                leaf_ids(child, ids);
            }
        }
        _ => ids.push(&layer.id),
    }
}

/// The corners of the box around every corner of `regions`, clockwise from the
/// top left; `None` for no region.
fn bounding_box<'a>(regions: impl Iterator<Item = &'a HitRegion>) -> Option<[Point; 4]> {
    let mut bounds: Option<(Point, Point)> = None;
    for corner in regions.flat_map(|region| region.corners) {
        bounds = Some(match bounds {
            None => (corner, corner),
            Some((min, max)) => (
                Point {
                    x: min.x.min(corner.x),
                    y: min.y.min(corner.y),
                },
                Point {
                    x: max.x.max(corner.x),
                    y: max.y.max(corner.y),
                },
            ),
        });
    }
    let (min, max) = bounds?;
    Some([
        min,
        Point { x: max.x, y: min.y },
        max,
        Point { x: min.x, y: max.y },
    ])
}

/// The inverse of `m`, `None` when it collapses the plane (or is not finite).
pub fn invert_affine(m: [f64; 6]) -> Option<[f64; 6]> {
    let [a, b, c, d, tx, ty] = m;
    let determinant = a * d - b * c;
    if !determinant.is_finite() || determinant.abs() <= f64::EPSILON {
        return None;
    }
    let inverse = [
        d / determinant,
        -b / determinant,
        -c / determinant,
        a / determinant,
        (c * ty - d * tx) / determinant,
        (b * tx - a * ty) / determinant,
    ];
    inverse.iter().all(|v| v.is_finite()).then_some(inverse)
}

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_composition::{LayerEffects, Rational, Time};

    fn layer(id: &str, transform: EvaluatedTransform, content: LayerContent) -> Layer {
        Layer {
            id: id.to_owned(),
            transform,
            opacity: 1.0,
            blend_mode: Default::default(),
            effects: LayerEffects::default(),
            content,
        }
    }

    fn group(id: &str, transform: EvaluatedTransform, layers: Vec<Layer>) -> Layer {
        layer(
            id,
            transform,
            LayerContent::Group {
                layers,
                clip: None,
                mask: None,
                motion_blur: false,
            },
        )
    }

    fn scene(layers: Vec<Layer>) -> Scene {
        Scene {
            width: 100,
            height: 100,
            frame_rate: Rational::new(30, 1),
            time: Time::default(),
            fonts: Vec::new(),
            shaders: Vec::new(),
            layers,
        }
    }

    fn at(x: f64, y: f64) -> Point {
        Point { x, y }
    }

    fn region(id: &str, min: Point, max: Point) -> HitRegion {
        HitRegion {
            layer_id: id.to_owned(),
            corners: [min, at(max.x, min.y), max, at(min.x, max.y)],
            hole: None,
            anchor_box: None,
            text_box: None,
            clips: Vec::new(),
        }
    }

    #[test]
    fn a_frame_carries_the_box_its_anchor_refers_to() {
        let text = layer(
            "t",
            EvaluatedTransform::default(),
            LayerContent::Text {
                text: "A".to_owned(),
                style: Box::default(),
                max_width: None,
                baseline_anchor: false,
            },
        );
        let path = layer(
            "p",
            EvaluatedTransform::default(),
            LayerContent::Path {
                commands: Vec::new(),
                fill: None,
                stroke: None,
                line_cap: Default::default(),
                line_join: Default::default(),
                miter_limit: 4.0,
                line_dash: None,
            },
        );
        let scene = scene(vec![
            leaf("a"),
            text,
            path,
            group("g", EvaluatedTransform::default(), vec![leaf("b")]),
        ]);
        let layout = [at(2.0, 2.0), at(8.0, 2.0), at(8.0, 6.0), at(2.0, 6.0)];
        let regions = [
            region("a", at(0.0, 0.0), at(5.0, 5.0)),
            HitRegion {
                anchor_box: Some(layout),
                ..region("t", at(0.0, 0.0), at(10.0, 8.0))
            },
            region("p", at(0.0, 0.0), at(5.0, 5.0)),
            region("b", at(0.0, 0.0), at(5.0, 5.0)),
        ];
        let frame = |id| layer_frame(&scene, &regions, id).expect("a frame");
        // A rect's quad is the box; a text's is padded past its layout box.
        assert_eq!(frame("a").anchor_box, Some(regions[0].corners));
        assert_eq!(frame("t").anchor_box, Some(layout));
        // A path and a group have no box to anchor in.
        assert_eq!(frame("p").anchor_box, None);
        assert_eq!(frame("g").anchor_box, None);
    }

    /// A leaf the frame is asked about: only its id and transform matter.
    fn leaf(id: &str) -> Layer {
        layer(
            id,
            EvaluatedTransform {
                position: at(3.0, 4.0),
                ..EvaluatedTransform::default()
            },
            LayerContent::Rect {
                width: 10.0,
                height: 10.0,
                fill: None,
                stroke: None,
                corner_radius: 0.0,
            },
        )
    }

    fn assert_close(actual: [f64; 6], expected: [f64; 6]) {
        for (actual, expected) in actual.iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-9, "{actual:?} {expected:?}");
        }
    }

    #[test]
    fn parent_of_a_layer_in_a_translated_group() {
        let transform = EvaluatedTransform {
            position: at(10.0, 20.0),
            ..EvaluatedTransform::default()
        };
        let scene = scene(vec![group("g", transform, vec![leaf("a")])]);
        let regions = [region("a", at(0.0, 0.0), at(5.0, 5.0))];
        let frame = layer_frame(&scene, &regions, "a").expect("a frame");
        assert_close(frame.parent, [1.0, 0.0, 0.0, 1.0, 10.0, 20.0]);
        assert_eq!(frame.transform.position, at(3.0, 4.0));
        assert_eq!(frame.outline, regions[0].corners);
    }

    #[test]
    fn parent_of_a_layer_in_a_rotated_group() {
        let transform = EvaluatedTransform {
            position: at(10.0, 0.0),
            rotation: 90.0,
            ..EvaluatedTransform::default()
        };
        let scene = scene(vec![group("g", transform, vec![leaf("a")])]);
        let regions = [region("a", at(0.0, 0.0), at(5.0, 5.0))];
        let frame = layer_frame(&scene, &regions, "a").expect("a frame");
        // The renderer's sin/cos of 90 degrees, rounded to f32.
        let (sin, cos) = (1.0, f64::from(90.0_f64.to_radians().cos() as f32));
        assert_close(frame.parent, [cos, sin, -sin, cos, 10.0, 0.0]);
    }

    #[test]
    fn parent_of_a_layer_in_a_scaled_group_inside_another() {
        let outer = EvaluatedTransform {
            position: at(5.0, 5.0),
            scale: at(2.0, 3.0),
            ..EvaluatedTransform::default()
        };
        let inner = EvaluatedTransform {
            position: at(1.0, 1.0),
            scale: at(0.5, 0.5),
            ..EvaluatedTransform::default()
        };
        let scene = scene(vec![group(
            "outer",
            outer,
            vec![group("inner", inner, vec![leaf("a")])],
        )]);
        let regions = [region("a", at(0.0, 0.0), at(5.0, 5.0))];
        let frame = layer_frame(&scene, &regions, "a").expect("a frame");
        assert_close(frame.parent, [1.0, 0.0, 0.0, 1.5, 7.0, 8.0]);
    }

    #[test]
    fn a_parent_that_turns_in_3d_has_no_frame() {
        for transform in [
            EvaluatedTransform {
                rotation_x: 10.0,
                ..EvaluatedTransform::default()
            },
            EvaluatedTransform {
                rotation_y: 10.0,
                ..EvaluatedTransform::default()
            },
            EvaluatedTransform {
                perspective: Some(500.0),
                ..EvaluatedTransform::default()
            },
        ] {
            let scene = scene(vec![group("g", transform, vec![leaf("a")])]);
            let regions = [region("a", at(0.0, 0.0), at(5.0, 5.0))];
            assert_eq!(layer_frame(&scene, &regions, "a"), None);
        }
    }

    #[test]
    fn a_parent_scaled_to_nothing_has_no_frame() {
        let transform = EvaluatedTransform {
            scale: at(0.0, 1.0),
            ..EvaluatedTransform::default()
        };
        let scene = scene(vec![group("g", transform, vec![leaf("a")])]);
        let regions = [region("a", at(0.0, 0.0), at(5.0, 5.0))];
        assert_eq!(layer_frame(&scene, &regions, "a"), None);
    }

    #[test]
    fn a_missing_id_has_no_frame() {
        let scene = scene(vec![leaf("a")]);
        assert_eq!(layer_frame(&scene, &[], "b"), None);
    }

    #[test]
    fn a_group_is_outlined_by_the_box_around_its_children() {
        let scene = scene(vec![
            group(
                "g",
                EvaluatedTransform::default(),
                vec![leaf("a"), leaf("b")],
            ),
            leaf("c"),
        ]);
        let regions = [
            region("a", at(10.0, 20.0), at(30.0, 40.0)),
            region("b", at(25.0, 5.0), at(60.0, 35.0)),
            region("c", at(0.0, 0.0), at(100.0, 100.0)),
        ];
        let frame = layer_frame(&scene, &regions, "g").expect("a frame");
        assert_eq!(
            frame.outline,
            [at(10.0, 5.0), at(60.0, 5.0), at(60.0, 40.0), at(10.0, 40.0)]
        );
        // A press on a child hits the child's id; the group owns both.
        assert_eq!(frame.leaves, ["a", "b"]);
        let leaf = layer_frame(&scene, &regions, "c").expect("a frame");
        assert_eq!(leaf.leaves, ["c"]);
    }

    #[test]
    fn inverting_an_affine_map() {
        let m = [2.0, 1.0, -1.0, 3.0, 5.0, -7.0];
        let inverse = invert_affine(m).expect("invertible");
        let [a, b, c, d, tx, ty] = m;
        let [ia, ib, ic, id, itx, ity] = inverse;
        // The product is the identity.
        assert_close(
            [
                ia * a + ic * b,
                ib * a + id * b,
                ia * c + ic * d,
                ib * c + id * d,
                ia * tx + ic * ty + itx,
                ib * tx + id * ty + ity,
            ],
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        );
        assert_eq!(invert_affine([1.0, 2.0, 2.0, 4.0, 0.0, 0.0]), None);
    }
}
