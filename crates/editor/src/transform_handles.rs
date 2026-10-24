//! The geometry of dragging a selected layer on the canvas: where its handles
//! sit, which handle a point is on, and what a drag from one point to another
//! makes of the layer's transform props. Scene pixels throughout.
//!
//! The values are the layer's `EvaluatedTransform` fields after the drag. For
//! a React primitive those are its props as written: `buildLayer`'s
//! `extractTransform` copies `x`/`y` to `position`, `rotation`, and
//! `scaleX ?? scale`/`scaleY ?? scale` to `scale`, with no offset (a text's
//! baseline anchor changes where it draws, not its position). `prop_values`
//! still carries an offset or a ratio over from the props the runtime saw,
//! for the layers that deviate (an `<Instances>`, whose x, y and rotation are
//! its particles').

use crate::canvas_geometry::{
    ANCHOR_PROPS, anchor_box, apply, keep_fixed, layer_map, local_box, pivot,
};
use crate::element_edit::{PropKey, TransformProp, received_number};
use celesta_composition::{EvaluatedTransform, Point};
use celesta_gpu_renderer::{LayerFrame, invert_affine};
use celesta_react_bridge::ReceivedProps;
use celesta_source_edit::format_number;

/// What the corner and edge handles do, as the viewer's "Handles" toggle
/// chose it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum ResizeTool {
    /// Change the props that hold the box (`width`, `height`, `radius`, a
    /// text's `maxWidth`), keeping the opposite edge where it is.
    #[default]
    Size,
    /// Scale the layer (`scale`, or `scaleX`/`scaleY`).
    Scale,
}

/// A size prop and the bounds its table field declares.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SizeField {
    pub(crate) name: String,
    pub(crate) min: Option<f64>,
    pub(crate) max: Option<f64>,
    /// Only whole numbers.
    pub(crate) integer: bool,
    /// The element doesn't write it (`Missing`): the runtime derives it (an
    /// `<Image>`'s other side from the source's aspect), so a drag of the
    /// other side writes it as it is (see `Resize::pinned`).
    pub(crate) absent: bool,
    /// A keyframe: written at that keyframe only, so no handle that moves
    /// `x`/`y` along its axis is offered (see `resizes`).
    pub(crate) keyframed: bool,
}

impl SizeField {
    pub(crate) fn key(&self) -> PropKey {
        PropKey::Prop(self.name.clone())
    }

    /// The smallest value a drag gives: the declared minimum, and never
    /// below `smallest` (the value of a box one own pixel across), so no
    /// drag collapses or flips the box.
    fn low(&self, smallest: f64) -> f64 {
        self.min.map_or(smallest, |min| min.max(smallest))
    }

    fn high(&self) -> f64 {
        self.max.unwrap_or(f64::INFINITY)
    }

    /// `value` inside [`low`](Self::low) and the maximum, whole with
    /// `integer`.
    fn clamp(&self, value: f64, smallest: f64) -> f64 {
        let (low, high) = (self.low(smallest), self.high());
        if self.integer {
            value.round().max(low.ceil()).min(high.floor())
        } else {
            value.max(low).min(high)
        }
    }
}

/// The props that hold an element's box (its table's `size`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SizeProps {
    /// `width` x `height`: the corners change both, the edges one.
    Box { width: SizeField, height: SizeField },
    /// A `2 * radius` square: the corners only, uniformly.
    Radius(SizeField),
    /// A width alone (a text's `maxWidth`): the left and right edges.
    Width(SizeField),
}

/// The Size tool on the selected layer: the props that hold its box,
/// whether `x`/`y` can move it so the opposite edge stays (see
/// `compensable`), and whether something that compensation turns, scales,
/// or measures from changes between keyframes (`rotation`, a scale, an
/// anchor), which leaves only the handles that keep the anchor point.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Resize {
    pub(crate) props: SizeProps,
    pub(crate) compensable: bool,
    pub(crate) animated: bool,
}

impl Resize {
    fn fields(&self) -> Vec<&SizeField> {
        match &self.props {
            SizeProps::Box { width, height } => vec![width, height],
            SizeProps::Radius(field) | SizeProps::Width(field) => vec![field],
        }
    }

    /// The fields that set the box's extent along x and along y.
    fn axis_fields(&self) -> (&SizeField, &SizeField) {
        match &self.props {
            SizeProps::Box { width, height } => (width, height),
            SizeProps::Radius(field) | SizeProps::Width(field) => (field, field),
        }
    }

    /// The size props, which a drag of this layer may change.
    pub(crate) fn keys(&self) -> Vec<PropKey> {
        self.fields().into_iter().map(SizeField::key).collect()
    }

    /// The box's axes (x, y) a drag of `handle` resizes; `None` for a handle
    /// the box has none of (a radius's edges, a width's corners).
    fn axes(&self, handle: Handle) -> Option<(bool, bool)> {
        match (&self.props, handle) {
            (SizeProps::Box { .. } | SizeProps::Radius(_), Handle::Corner(_)) => Some((true, true)),
            (SizeProps::Box { .. }, Handle::Edge(i)) => Some((i % 2 == 1, i % 2 == 0)),
            (SizeProps::Width(_), Handle::Edge(1 | 3)) => Some((true, false)),
            _ => None,
        }
    }

    /// The size props a drag of `handle` writes: those it changes, and a
    /// box's other side when the element doesn't write it (`pinned`).
    fn handle_keys(&self, handle: Handle) -> Vec<PropKey> {
        let Some((x, y)) = self.axes(handle) else {
            return Vec::new();
        };
        match &self.props {
            SizeProps::Box { width, height } => [
                (x || width.absent).then(|| width.key()),
                (y || height.absent).then(|| height.key()),
            ]
            .into_iter()
            .flatten()
            .collect(),
            SizeProps::Radius(field) | SizeProps::Width(field) => vec![field.key()],
        }
    }

    /// Whether `key` is a box side the element doesn't write: a drag of the
    /// other side writes it as it is, so the box keeps it whatever the
    /// runtime would derive from the new one (an `<Image>`'s aspect).
    fn pinned(&self, key: &PropKey) -> bool {
        match &self.props {
            SizeProps::Box { width, height } => [width, height]
                .iter()
                .any(|field| field.absent && field.key() == *key),
            SizeProps::Radius(_) | SizeProps::Width(_) => false,
        }
    }

    /// The value `key` had when the drag started: what the box the size
    /// props describe (`anchor_box`) measures. `None` for any other prop.
    fn start(&self, frame: &LayerFrame, key: &PropKey) -> Option<f64> {
        let size = anchor_box(frame)?.size();
        match &self.props {
            SizeProps::Box { width, .. } if width.key() == *key => Some(size.x),
            SizeProps::Box { height, .. } if height.key() == *key => Some(size.y),
            SizeProps::Radius(field) if field.key() == *key => Some(size.x / 2.0),
            SizeProps::Width(field) if field.key() == *key => Some(size.x),
            _ => None,
        }
    }

    /// Whether `key` is written as the new box's width itself, whatever the
    /// runtime saw: a text's `maxWidth`, which its box is (or, unset, its
    /// measured width).
    fn absolute(&self, key: &PropKey) -> bool {
        matches!(&self.props, SizeProps::Width(field) if field.key() == *key)
    }
}

/// `value` as the source will hold it (`format_number`'s rounding): what
/// a compensation is computed from, so the reloaded layer lands where the
/// preview drew it.
fn as_written(value: f64) -> f64 {
    format_number(value).parse().unwrap_or(value)
}

/// What a rotation or scale drag turns about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum PivotMode {
    /// The center of the layer's box, which stays put: `x`/`y` compensate.
    /// Opt-in: in real scenes `x`/`y` are often constants or computed,
    /// which can't compensate.
    Center,
    /// The layer's pivot, its anchor point plus `origin`, where the renderer
    /// turns and scales it.
    #[default]
    Pivot,
}

/// The box center a drag of `frame` keeps still under `pivot`, in the
/// layer's own pixels; `None` in Pivot mode, or when the box is unknown (the
/// drag then turns about the pivot).
fn kept_center(frame: &LayerFrame, pivot: PivotMode) -> Option<Point> {
    match pivot {
        PivotMode::Center => local_box(frame).map(|local| local.center()),
        PivotMode::Pivot => None,
    }
}

/// The point a rotation or scale of `frame` turns about under `mode`, in its
/// parent's space: the box center in Center mode, else the pivot.
fn turning_point(frame: &LayerFrame, mode: PivotMode) -> Point {
    let transform = &frame.transform;
    kept_center(frame, mode).map_or_else(
        || pivot(transform),
        |center| apply(&layer_map(transform), center),
    )
}

/// A part of the selection that a drag starts from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Handle {
    /// The layer's body: moves it.
    Move,
    /// The outline's corner `i`, in the outline's order: scales both axes.
    Corner(u8),
    /// The middle of the outline's edge from corner `i` to corner `i + 1`
    /// (top, right, bottom, left): scales one axis.
    Edge(u8),
    /// The square on a stem above the top edge: rotates.
    Rotate,
    /// The cross on the pivot, pressed with Alt: moves the anchor, and
    /// `x`/`y` with it so the layer stays where it is.
    Anchor,
}

/// The anchor props an anchor drag writes.
pub(crate) fn anchor_keys() -> [PropKey; 2] {
    ANCHOR_PROPS.map(|name| PropKey::Prop(name.to_owned()))
}

/// Where the pivot cross of `frame` sits, in scene pixels.
pub(crate) fn pivot_cross(frame: &LayerFrame) -> Point {
    apply(&frame.parent, pivot(&frame.transform))
}

/// What the cross on the canvas marks, in scene pixels: the point a
/// rotation or scale of the layer turns about.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Marker {
    /// The box's middle, with Center in effect: a marker only.
    Center(Point),
    /// The pivot (anchor + origin), which Alt-drags the anchor.
    Pivot(Point),
}

impl Marker {
    pub(crate) fn at(self) -> Point {
        match self {
            Self::Center(at) | Self::Pivot(at) => at,
        }
    }
}

/// The cross of `frame` for the mode a drag uses (`pivot`, see
/// `pivot_used`): the box's middle in Center mode, else the pivot.
pub(crate) fn marker(frame: &LayerFrame, pivot: PivotMode) -> Marker {
    match kept_center(frame, pivot) {
        Some(center) => Marker::Center(apply(
            &frame.parent,
            apply(&layer_map(&frame.transform), center),
        )),
        None => Marker::Pivot(pivot_cross(frame)),
    }
}

/// Where the cross an Alt-drag moves the anchor from sits (scene pixels),
/// when `editable` lets it: both anchors, `x` and `y` (which the caller puts
/// in only when they can compensate), and a box the anchor refers to (see
/// `with_anchor_box`).
pub(crate) fn anchor_handle(frame: &LayerFrame, editable: &[PropKey]) -> Option<Point> {
    let keys = [TransformProp::X.key(), TransformProp::Y.key()];
    let draggable = keys
        .iter()
        .chain(&anchor_keys())
        .all(|key| editable.contains(key))
        && anchor_box(frame).is_some();
    draggable.then(|| pivot_cross(frame))
}

/// The handle a press at `point` (scene pixels) takes: with `alt`, the
/// pivot cross when it is within `radius` and an anchor drag can start
/// there (even over another handle); otherwise the one [`handle_at`] finds.
pub(crate) fn pressed_handle(
    frame: &LayerFrame,
    shown: &[(Handle, Point)],
    editable: &[PropKey],
    point: Point,
    radius: f64,
    alt: bool,
) -> Option<Handle> {
    let on_cross = anchor_handle(frame, editable)
        .is_some_and(|at| (at.x - point.x).hypot(at.y - point.y) <= radius);
    if alt && on_cross {
        return Some(Handle::Anchor);
    }
    handle_at(frame, shown, point, radius)
}

/// Rotation steps with Shift held, in degrees.
const SNAP_DEGREES: f64 = 15.0;
/// The smallest scale a drag makes; a drag through the anchor does not flip
/// or collapse the layer.
const MIN_SCALE_RATIO: f64 = 0.01;

/// The handles of `frame` and where they sit: the corners, the edges' middles
/// (none for a `uniform` scale), and the rotation handle `stem` scene pixels
/// out from the top edge's middle.
pub(crate) fn handles(frame: &LayerFrame, uniform: bool, stem: f64) -> Vec<(Handle, Point)> {
    handles_on(&frame.outline, uniform, stem)
}

/// [`handles`] on the corners of `quad` (scene pixels, in the outline's
/// order).
fn handles_on(quad: &[Point; 4], uniform: bool, stem: f64) -> Vec<(Handle, Point)> {
    let mut handles: Vec<(Handle, Point)> = (0..4u8)
        .map(|i| (Handle::Corner(i), quad[usize::from(i)]))
        .collect();
    if !uniform {
        handles.extend((0..4u8).map(|i| (Handle::Edge(i), edge_middle(quad, i))));
    }
    handles.push((Handle::Rotate, rotate_handle(quad, stem)));
    handles
}

/// The quad the handles of `frame` sit on: with `resize` (the Size tool),
/// the box the size props describe (`anchor_box`, the own top-left,
/// top-right, bottom-right, bottom-left), which a drag measures; else, and
/// when that box is unknown, the outline. A group's outline is its
/// children's scene-axis box and a text's is padded, so a corner of it is
/// not the box's.
pub(crate) fn handle_quad(frame: &LayerFrame, resize: Option<&Resize>) -> [Point; 4] {
    resize.and(frame.anchor_box).unwrap_or(frame.outline)
}

/// The middle of the top edge and the rotation handle at the stem's end,
/// on [`handle_quad`].
pub(crate) fn rotate_stem(
    frame: &LayerFrame,
    stem: f64,
    resize: Option<&Resize>,
) -> (Point, Point) {
    let quad = handle_quad(frame, resize);
    (edge_middle(&quad, 0), rotate_handle(&quad, stem))
}

fn edge_middle(outline: &[Point; 4], i: u8) -> Point {
    let (a, b) = (
        outline[usize::from(i) % 4],
        outline[(usize::from(i) + 1) % 4],
    );
    Point {
        x: (a.x + b.x) / 2.0,
        y: (a.y + b.y) / 2.0,
    }
}

/// `stem` out from the top edge's middle, away from the outline's center.
fn rotate_handle(outline: &[Point; 4], stem: f64) -> Point {
    let top = edge_middle(outline, 0);
    let center = Point {
        x: outline.iter().map(|p| p.x).sum::<f64>() / 4.0,
        y: outline.iter().map(|p| p.y).sum::<f64>() / 4.0,
    };
    let (dx, dy) = (top.x - center.x, top.y - center.y);
    let length = dx.hypot(dy);
    let (ux, uy) = if length > f64::EPSILON {
        (dx / length, dy / length)
    } else {
        (0.0, -1.0)
    };
    Point {
        x: top.x + ux * stem,
        y: top.y + uy * stem,
    }
}

/// The handle within `radius` of `point` (the nearest), else `Move` when
/// `point` is inside the outline, else `None`.
pub(crate) fn handle_at(
    frame: &LayerFrame,
    handles: &[(Handle, Point)],
    point: Point,
    radius: f64,
) -> Option<Handle> {
    handles
        .iter()
        .map(|(handle, at)| (*handle, (at.x - point.x).hypot(at.y - point.y)))
        .filter(|(_, distance)| *distance <= radius)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(handle, _)| handle)
        .or_else(|| inside(&frame.outline, point).then_some(Handle::Move))
}

/// Whether `point` is inside the convex quad `outline`, either winding.
fn inside(outline: &[Point; 4], point: Point) -> bool {
    let sides = (0..4).map(|i| {
        let (a, b) = (outline[i], outline[(i + 1) % 4]);
        (b.x - a.x) * (point.y - a.y) - (b.y - a.y) * (point.x - a.x)
    });
    let (mut positive, mut negative) = (false, false);
    for side in sides {
        positive |= side > 0.0;
        negative |= side < 0.0;
    }
    !(positive && negative)
}

/// How far a rotation drag from `start` to `now` (scene pixels) has turned
/// about the point `pivot` turns about (see [`PivotMode`]), in degrees: the
/// angle between them, plus the whole turns that keep it nearest `previous`
/// (the turn at the previous move), so a drag that goes on past a half turn
/// keeps counting instead of wrapping.
pub(crate) fn turn(
    frame: &LayerFrame,
    pivot: PivotMode,
    start: Point,
    now: Point,
    previous: f64,
) -> Option<f64> {
    let inverse = invert_affine(frame.parent)?;
    let (start, now) = (apply(&inverse, start), apply(&inverse, now));
    let about = turning_point(frame, pivot);
    let angle = |p: Point| (p.y - about.y).atan2(p.x - about.x).to_degrees();
    let raw = angle(now) - angle(start);
    Some(raw + ((previous - raw) / 360.0).round() * 360.0)
}

/// The transform values a drag of `handle` from `start` to `now` (scene
/// pixels) makes, for the props in `editable` only. The pointer moves are
/// taken into the layer's parent space; rotation turns by [`turn`] from
/// `previous_turn` (the result snapped to 15° with `shift`), and scaling
/// keeps the turning point in place, in one ratio for a uniform `Scale`.
/// Both turn about the point `pivot` names (the mode the drag uses, Center
/// only when `x`/`y` can compensate); in Center mode `x`/`y` follow so the
/// box's center stays where it was. With `resize` (the Size tool) the
/// corners and edges change the size props instead (see `resize_values`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn drag_values(
    frame: &LayerFrame,
    handle: Handle,
    (start, now): (Point, Point),
    shift: bool,
    previous_turn: f64,
    editable: &[PropKey],
    pivot: PivotMode,
    resize: Option<&Resize>,
) -> Vec<(PropKey, f64)> {
    let pointer = (start, now);
    let Some(inverse) = invert_affine(frame.parent) else {
        return Vec::new();
    };
    let (start, now) = (apply(&inverse, start), apply(&inverse, now));
    let transform = &frame.transform;
    let can = |prop: TransformProp| editable.contains(&prop.key());
    if let (Some(resize), Handle::Corner(_) | Handle::Edge(_)) = (resize, handle) {
        return resize_values(frame, handle, (start, now), shift, editable, resize);
    }
    // Center mode: the box center stays, so `x`/`y` are computed from the
    // values as written.
    let centered =
        can(TransformProp::X) && can(TransformProp::Y) && kept_center(frame, pivot).is_some();
    let written = |value: f64| if centered { as_written(value) } else { value };
    let mut values = Vec::new();
    let mut push = |prop: TransformProp, value: f64| values.push((prop.key(), value));
    // The props after the drag, with the old position.
    let mut after = *transform;
    match handle {
        Handle::Move => {
            let position = transform.position;
            if can(TransformProp::X) {
                push(TransformProp::X, position.x + now.x - start.x);
            }
            if can(TransformProp::Y) {
                push(TransformProp::Y, position.y + now.y - start.y);
            }
            return values;
        }
        Handle::Anchor => return anchor_values(frame, (start, now), editable),
        Handle::Rotate => {
            if can(TransformProp::Rotation)
                && let Some(delta) = turn(frame, pivot, pointer.0, pointer.1, previous_turn)
            {
                let mut rotation = transform.rotation + delta;
                if shift {
                    rotation = (rotation / SNAP_DEGREES).round() * SNAP_DEGREES;
                }
                let rotation = written(rotation);
                push(TransformProp::Rotation, rotation);
                after.rotation = rotation;
            }
        }
        Handle::Corner(_) | Handle::Edge(_) => {
            let Some(unturn) = unturn(transform) else {
                return values;
            };
            let about = turning_point(frame, pivot);
            let local = |p: Point| {
                let v = (p.x - about.x, p.y - about.y);
                Point {
                    x: unturn[0] * v.0 + unturn[2] * v.1,
                    y: unturn[1] * v.0 + unturn[3] * v.1,
                }
            };
            let (from, to) = (local(start), local(now));
            if can(TransformProp::Scale) {
                let length = from.x * from.x + from.y * from.y;
                if length > 1e-9 {
                    let ratio = ((to.x * from.x + to.y * from.y) / length).max(MIN_SCALE_RATIO);
                    let scale = written(transform.scale.x * ratio);
                    push(TransformProp::Scale, scale);
                    after.scale = Point { x: scale, y: scale };
                }
            } else {
                let ratio = |from: f64, to: f64| {
                    (from.abs() > 1e-9).then(|| (to / from).max(MIN_SCALE_RATIO))
                };
                let (x_axis, y_axis) = match handle {
                    Handle::Edge(i) => (i % 2 == 1, i % 2 == 0),
                    _ => (true, true),
                };
                if x_axis
                    && can(TransformProp::ScaleX)
                    && let Some(ratio) = ratio(from.x, to.x)
                {
                    after.scale.x = written(transform.scale.x * ratio);
                    push(TransformProp::ScaleX, after.scale.x);
                }
                if y_axis
                    && can(TransformProp::ScaleY)
                    && let Some(ratio) = ratio(from.y, to.y)
                {
                    after.scale.y = written(transform.scale.y * ratio);
                    push(TransformProp::ScaleY, after.scale.y);
                }
            }
        }
    }
    // Center mode: move the layer so the box center stays where it was.
    if !values.is_empty()
        && can(TransformProp::X)
        && can(TransformProp::Y)
        && let Some(center) = kept_center(frame, pivot)
    {
        let kept = apply(&layer_map(transform), center);
        let position = keep_fixed(&after, center, kept);
        values.push((TransformProp::X.key(), position.x));
        values.push((TransformProp::Y.key(), position.y));
    }
    values
}

/// The values an anchor drag from `start` to `now` (parent space) makes:
/// the anchor moves by the pointer's move in fractions of the box it refers
/// to (not clamped: an anchor outside the box is valid), so the cross
/// follows the pointer, and `x`/`y` move so the box stays where it was.
/// Empty unless [`anchor_handle`] allows the drag.
fn anchor_values(
    frame: &LayerFrame,
    (start, now): (Point, Point),
    editable: &[PropKey],
) -> Vec<(PropKey, f64)> {
    let transform = &frame.transform;
    let map = layer_map(transform);
    let (Some(_), Some(local), Some(inverse)) = (
        anchor_handle(frame, editable),
        anchor_box(frame),
        invert_affine(map),
    ) else {
        return Vec::new();
    };
    let size = local.size();
    if size.x.abs() < 1e-9 || size.y.abs() < 1e-9 {
        return Vec::new();
    }
    let (from, to) = (apply(&inverse, start), apply(&inverse, now));
    // The anchor as written: the props' (see `with_anchor_box`).
    let anchor = Point {
        x: as_written(transform.anchor.x + (to.x - from.x) / size.x),
        y: as_written(transform.anchor.y + (to.y - from.y) / size.y),
    };
    // Own pixels are relative to the anchor point, so the box's top-left
    // corner moves to `-anchor` of it; keep that corner where it is. The
    // anchor is not part of the map, so the props after are the same.
    let corner_after = Point {
        x: -anchor.x * size.x,
        y: -anchor.y * size.y,
    };
    let position = keep_fixed(transform, corner_after, apply(&map, local.min));
    let [anchor_x, anchor_y] = anchor_keys();
    vec![
        (anchor_x, anchor.x),
        (anchor_y, anchor.y),
        (TransformProp::X.key(), position.x),
        (TransformProp::Y.key(), position.y),
    ]
}

/// Where `handle` sits on the box, in fractions of it (see
/// `LocalBox::at`): 0 its left or top, 1 its right or bottom, `None` along
/// an edge (its middle).
fn handle_fractions(handle: Handle) -> (Option<f64>, Option<f64>) {
    match handle {
        // The outline's order: top-left, top-right, bottom-right,
        // bottom-left.
        Handle::Corner(i) => (
            Some(if matches!(i, 1 | 2) { 1.0 } else { 0.0 }),
            Some(if i >= 2 { 1.0 } else { 0.0 }),
        ),
        Handle::Edge(0) => (None, Some(0.0)),
        Handle::Edge(1) => (Some(1.0), None),
        Handle::Edge(2) => (None, Some(1.0)),
        Handle::Edge(3) => (Some(0.0), None),
        _ => (None, None),
    }
}

/// The fractions of the box (x, y) a resize by `handle` keeps where they
/// are: the opposite edge on each axis it changes, `None` on one it doesn't.
fn kept_fractions(handle: Handle, resize: &Resize) -> (Option<f64>, Option<f64>) {
    let (x, y) = resize.axes(handle).unwrap_or_default();
    let (hx, hy) = handle_fractions(handle);
    (
        hx.filter(|_| x).map(|h| 1.0 - h),
        hy.filter(|_| y).map(|h| 1.0 - h),
    )
}

/// Whether a resize by `handle` moves the layer's anchor point: the edge it
/// keeps is not where the anchor is, so `x`/`y` must move the layer for that
/// edge to stay (left and top edges with the default top-left anchor).
fn resize_moves(frame: &LayerFrame, handle: Handle, resize: &Resize) -> bool {
    let (x, y) = moved_axes(frame, handle, resize);
    x || y
}

/// The axes (x, y) along which a resize by `handle` moves the layer's
/// anchor point (see `resize_moves`).
fn moved_axes(frame: &LayerFrame, handle: Handle, resize: &Resize) -> (bool, bool) {
    let anchor = frame.transform.anchor;
    let (kx, ky) = kept_fractions(handle, resize);
    (
        kx.is_some_and(|k| (k - anchor.x).abs() > 1e-9),
        ky.is_some_and(|k| (k - anchor.y).abs() > 1e-9),
    )
}

/// Whether the Size tool's `handle` can resize the layer: the box has it,
/// the size props are `editable`, and when it moves the anchor point (see
/// `resize_moves`), `x`/`y` can compensate, nothing the compensation uses
/// is a keyframe (`Resize::animated`), and neither is the size along each
/// axis it moves along (literal `x`/`y` would keep the edge at that
/// keyframe only).
fn resizes(frame: &LayerFrame, handle: Handle, editable: &[PropKey], resize: &Resize) -> bool {
    let position = [TransformProp::X.key(), TransformProp::Y.key()];
    let (moves_x, moves_y) = moved_axes(frame, handle, resize);
    let (along_x, along_y) = resize.axis_fields();
    let animated =
        resize.animated || (moves_x && along_x.keyframed) || (moves_y && along_y.keyframed);
    resize.axes(handle).is_some()
        && anchor_box(frame).is_some()
        && resize.keys().iter().all(|key| editable.contains(key))
        && (!(moves_x || moves_y)
            || (resize.compensable
                && !animated
                && position.iter().all(|key| editable.contains(key))))
}

/// Whether any corner or edge of the box can resize the layer with the Size
/// tool (see `resizes`), with every size prop editable and `x`/`y` when they
/// can compensate.
pub(crate) fn resizes_any(frame: &LayerFrame, resize: &Resize) -> bool {
    let mut editable = resize.keys();
    if resize.compensable {
        editable.extend([TransformProp::X.key(), TransformProp::Y.key()]);
    }
    (0..4u8)
        .flat_map(|i| [Handle::Corner(i), Handle::Edge(i)])
        .any(|handle| resizes(frame, handle, &editable, resize))
}

/// Whether a size prop of `resize` is a keyframe (which hides the handles
/// that move the anchor point along its axis; see `resizes`).
pub(crate) fn size_animated(resize: &Resize) -> bool {
    let (along_x, along_y) = resize.axis_fields();
    along_x.keyframed || along_y.keyframed
}

/// The values a Size-tool drag of `handle` from `start` to `now` (parent
/// space) makes: the dragged edges follow the pointer in the layer's own
/// pixels, measured on the box the size props describe (`anchor_box`), and
/// the opposite ones stay, `x`/`y` moving the layer when the anchor point
/// is not on them. A radius, and a box's corner with `shift`, keep the
/// aspect (the pointer projected onto the box's diagonal). Sizes keep to
/// their fields' bounds and to one own pixel; empty unless [`resizes`].
fn resize_values(
    frame: &LayerFrame,
    handle: Handle,
    (start, now): (Point, Point),
    shift: bool,
    editable: &[PropKey],
    resize: &Resize,
) -> Vec<(PropKey, f64)> {
    let transform = &frame.transform;
    let map = layer_map(transform);
    let (true, Some((x_axis, y_axis)), Some(local), Some(inverse)) = (
        resizes(frame, handle, editable, resize),
        resize.axes(handle),
        anchor_box(frame),
        invert_affine(map),
    ) else {
        return Vec::new();
    };
    let size = local.size();
    if size.x.abs() < 1e-9 || size.y.abs() < 1e-9 {
        return Vec::new();
    }
    let (from, to) = (apply(&inverse, start), apply(&inverse, now));
    let (hx, hy) = handle_fractions(handle);
    // The dragged edge moves with the pointer: the right or bottom one away
    // from the box's start, the left or top one toward it.
    let grown = |on: bool, at: Option<f64>, moved: f64, length: f64| match at {
        Some(at) if on => length + if at > 0.5 { moved } else { -moved },
        _ => length,
    };
    let pulled = Point {
        x: grown(x_axis, hx, to.x - from.x, size.x),
        y: grown(y_axis, hy, to.y - from.y, size.y),
    };
    // The pointer projected onto the box's diagonal, as a ratio of the box.
    let ratio = (pulled.x * size.x + pulled.y * size.y) / (size.x * size.x + size.y * size.y);
    let mut values = Vec::new();
    let resized = match &resize.props {
        SizeProps::Box { width, height } => {
            let (w, h) = if shift && matches!(handle, Handle::Corner(_)) {
                let low = (width.low(1.0) / size.x).max(height.low(1.0) / size.y);
                let high = (width.high() / size.x).min(height.high() / size.y);
                let ratio = ratio.max(low).min(high);
                (
                    as_written(width.clamp(ratio * size.x, 1.0)),
                    as_written(height.clamp(ratio * size.y, 1.0)),
                )
            } else {
                (
                    as_written(width.clamp(pulled.x, 1.0)),
                    as_written(height.clamp(pulled.y, 1.0)),
                )
            };
            // A side the drag doesn't change stays as it is, written when the
            // element doesn't write it (see `Resize::pinned`).
            let (w, h) = (
                if x_axis { w } else { as_written(size.x) },
                if y_axis { h } else { as_written(size.y) },
            );
            if x_axis || width.absent {
                values.push((width.key(), w));
            }
            if y_axis || height.absent {
                values.push((height.key(), h));
            }
            Point {
                x: if x_axis { w } else { size.x },
                y: if y_axis { h } else { size.y },
            }
        }
        SizeProps::Radius(field) => {
            let radius = as_written(field.clamp(ratio * size.x / 2.0, 0.5));
            values.push((field.key(), radius));
            Point {
                x: 2.0 * radius,
                y: 2.0 * radius,
            }
        }
        SizeProps::Width(field) => {
            // Whole own pixels: a text is laid out `ceil(maxWidth)` wide, and
            // the compensation is computed from the box it will have.
            let whole = SizeField {
                integer: true,
                ..field.clone()
            };
            let width = as_written(whole.clamp(pulled.x, 1.0));
            values.push((field.key(), width));
            Point {
                x: width,
                y: size.y,
            }
        }
    };
    if resize_moves(frame, handle, resize) {
        // Own pixels are relative to the anchor point, which stays at the
        // anchor's fractions of the new box: keep the opposite edge's point
        // (the pivot's coordinate along an axis that does not change) where
        // it is.
        let (kx, ky) = kept_fractions(handle, resize);
        let (anchor, origin) = (transform.anchor, transform.origin);
        let kept = |k: Option<f64>, min: f64, length: f64, pivot: f64| {
            k.map_or(pivot, |k| min + k * length)
        };
        let before = Point {
            x: kept(kx, local.min.x, size.x, origin.x),
            y: kept(ky, local.min.y, size.y, origin.y),
        };
        let after = Point {
            x: kept(kx, -anchor.x * resized.x, resized.x, origin.x),
            y: kept(ky, -anchor.y * resized.y, resized.y, origin.y),
        };
        let position = keep_fixed(transform, after, apply(&map, before));
        values.push((TransformProp::X.key(), position.x));
        values.push((TransformProp::Y.key(), position.y));
    }
    values
}

/// The inverse of the layer's linear part without its scale (its rotation
/// and skew), as `[a, b, c, d]`: takes a parent-space offset from the anchor
/// into the layer's axes, still scaled.
fn unturn(transform: &EvaluatedTransform) -> Option<[f64; 4]> {
    let unscaled = EvaluatedTransform {
        scale: Point { x: 1.0, y: 1.0 },
        ..*transform
    };
    let l = unscaled.linear();
    invert_affine([l[0][0], l[1][0], l[0][1], l[1][1], 0.0, 0.0]).map(|m| [m[0], m[1], m[2], m[3]])
}

fn ratio(was: f64, now: f64) -> f64 {
    if was.abs() > 1e-12 { now / was } else { 1.0 }
}

/// The prop name a drag key writes (`x`, `anchorX`); `None` for an input or
/// a component prop, which no drag changes.
fn prop_name(key: &PropKey) -> Option<&str> {
    match key {
        PropKey::Prop(name) => Some(name),
        PropKey::Input { .. } | PropKey::CallSite(_) => None,
    }
}

/// The value the drag started from for `key`: the evaluated transform's
/// field for a transform prop and an anchor (a shape's, from its props: see
/// `with_anchor_box`); for a size prop of `resize`, what the box it
/// describes measures (`Resize::start`). `None` for any other prop.
fn start_value(frame: &LayerFrame, key: &PropKey, resize: Option<&Resize>) -> Option<f64> {
    let transform = &frame.transform;
    if let Some(prop) = TransformProp::from_key(key) {
        return Some(match prop {
            TransformProp::X => transform.position.x,
            TransformProp::Y => transform.position.y,
            TransformProp::Rotation => transform.rotation,
            TransformProp::Scale | TransformProp::ScaleX => transform.scale.x,
            TransformProp::ScaleY => transform.scale.y,
        });
    }
    if let Some(start) = resize.and_then(|resize| resize.start(frame, key)) {
        return Some(start);
    }
    let [anchor_x, anchor_y] = ANCHOR_PROPS;
    match prop_name(key)? {
        name if name == anchor_x => Some(transform.anchor.x),
        name if name == anchor_y => Some(transform.anchor.y),
        _ => None,
    }
}

/// Whether `key` is a scale, which a drag changes by a ratio.
fn is_scale(key: &PropKey) -> bool {
    matches!(
        TransformProp::from_key(key),
        Some(TransformProp::Scale | TransformProp::ScaleX | TransformProp::ScaleY)
    )
}

/// Each prop of the drag's `values` as the source has it (`was`) and as the
/// drag writes it (`now`): the prop the runtime saw (`received`) moved by the
/// same amount (position, anchors, rotation, sizes) or ratio (scales) as the
/// evaluated value; the evaluated value when it saw none. A text's
/// `maxWidth` is the new box width itself (`Resize::absolute`). A prop
/// without a start value (see `start_value`) is left out.
fn prop_changes(
    frame: &LayerFrame,
    values: &[(PropKey, f64)],
    received: &ReceivedProps,
    resize: Option<&Resize>,
) -> Vec<(PropKey, f64, f64)> {
    values
        .iter()
        .filter_map(|(key, value)| {
            let start = start_value(frame, key, resize)?;
            let was = received_number(received, prop_name(key)?).unwrap_or(start);
            let now = if resize.is_some_and(|resize| resize.absolute(key)) {
                *value
            } else if is_scale(key) {
                was * ratio(start, *value)
            } else {
                was + value - start
            };
            Some((key.clone(), was, now))
        })
        .collect()
}

/// The changes of `prop_changes` the drag writes: those the source would
/// not keep as written (a different formatted number), and, once anything
/// changes, a box side the element doesn't write (`Resize::pinned`), as it
/// is. Empty when the drag changed nothing.
fn written_changes(
    frame: &LayerFrame,
    values: &[(PropKey, f64)],
    received: &ReceivedProps,
    resize: Option<&Resize>,
) -> Vec<(PropKey, f64, f64)> {
    let pinned = |key: &PropKey| resize.is_some_and(|resize| resize.pinned(key));
    let changes = prop_changes(frame, values, received, resize);
    let differs = |(_, was, now): &(PropKey, f64, f64)| format_number(*was) != format_number(*now);
    let changed = changes
        .iter()
        .any(|change| differs(change) && !pinned(&change.0));
    changes
        .into_iter()
        .filter(|change| differs(change) || (changed && pinned(&change.0)))
        .collect()
}

/// The props to write for the drag's `values` (see `written_changes`).
pub(crate) fn prop_values(
    frame: &LayerFrame,
    values: &[(PropKey, f64)],
    received: &ReceivedProps,
    resize: Option<&Resize>,
) -> Vec<(PropKey, f64)> {
    written_changes(frame, values, received, resize)
        .into_iter()
        .map(|(key, _, now)| (key, now))
        .collect()
}

/// The value of every prop `handle` changes in a drag (of those in
/// `editable`, turning about what `pivot` names, resizing with `resize`),
/// for the preview: the drag's for the props it changed (see
/// `prop_values`), and the one the runtime saw when the drag started (else
/// the start value) for the others. The props are the same on every frame
/// of the drag, so the preview's overrides keep their positions and the
/// runtime never remounts for a new one.
pub(crate) fn preview_values(
    frame: &LayerFrame,
    handle: Handle,
    editable: &[PropKey],
    values: &[(PropKey, f64)],
    received: &ReceivedProps,
    pivot: PivotMode,
    resize: Option<&Resize>,
) -> Vec<(PropKey, f64)> {
    let changed = prop_values(frame, values, received, resize);
    handle_props(frame, handle, editable, pivot, resize)
        .into_iter()
        .filter_map(|key| {
            let value = match changed.iter().find(|(candidate, _)| *candidate == key) {
                Some(&(_, value)) => value,
                None => prop_name(&key)
                    .and_then(|name| received_number(received, name))
                    .or_else(|| start_value(frame, &key, resize))?,
            };
            Some((key, value))
        })
        .collect()
}

/// The props of `editable` a drag of `handle` may change (see `drag_values`):
/// in Center mode a rotation or scale also moves `x`/`y`; with `resize`, a
/// corner or edge changes the size props it resizes, and `x`/`y` when it
/// moves the anchor point.
fn handle_props(
    frame: &LayerFrame,
    handle: Handle,
    editable: &[PropKey],
    pivot: PivotMode,
    resize: Option<&Resize>,
) -> Vec<PropKey> {
    let position = [TransformProp::X.key(), TransformProp::Y.key()];
    if let (Some(resize), Handle::Corner(_) | Handle::Edge(_)) = (resize, handle) {
        let mut props = resize.handle_keys(handle);
        if resize_moves(frame, handle, resize) {
            props.extend(position);
        }
        props.retain(|key| editable.contains(key));
        return props;
    }
    let uniform = editable.contains(&TransformProp::Scale.key());
    let mut props: Vec<PropKey> = match handle {
        Handle::Move => position.to_vec(),
        // The anchor drag always keeps the box where it is.
        Handle::Anchor => anchor_keys().into_iter().chain(position.clone()).collect(),
        Handle::Rotate => vec![TransformProp::Rotation.key()],
        _ if uniform => vec![TransformProp::Scale.key()],
        Handle::Edge(i) if i % 2 == 1 => vec![TransformProp::ScaleX.key()],
        Handle::Edge(_) => vec![TransformProp::ScaleY.key()],
        Handle::Corner(_) => vec![TransformProp::ScaleX.key(), TransformProp::ScaleY.key()],
    };
    // In Center mode a rotation or scale moves the layer too.
    if pivot == PivotMode::Center
        && matches!(handle, Handle::Rotate | Handle::Corner(_) | Handle::Edge(_))
    {
        props.extend(position);
    }
    props.retain(|key| editable.contains(key));
    props
}

/// `x 120 → 184` for each prop the drag writes, with the values as written
/// to the source.
pub(crate) fn drag_label(
    frame: &LayerFrame,
    values: &[(PropKey, f64)],
    received: &ReceivedProps,
    resize: Option<&Resize>,
) -> String {
    written_changes(frame, values, received, resize)
        .into_iter()
        .map(|(key, was, now)| {
            format!(
                "{} {} → {}",
                prop_name(&key).unwrap_or_default(),
                format_number(was),
                format_number(now)
            )
        })
        .collect::<Vec<_>>()
        .join("  ")
}

/// The offset of `point` (scene pixels) from `about` (parent space) in the
/// layer's own axes, still scaled; `None` when the parent or the layer
/// collapses.
fn local_offset(frame: &LayerFrame, point: Point, about: Point) -> Option<Point> {
    let inverse = invert_affine(frame.parent)?;
    let unturn = unturn(&frame.transform)?;
    let p = apply(&inverse, point);
    let v = (p.x - about.x, p.y - about.y);
    Some(Point {
        x: unturn[0] * v.0 + unturn[2] * v.1,
        y: unturn[1] * v.0 + unturn[3] * v.1,
    })
}

/// Below this many scene pixels from the turning point's lines, a scale
/// handle has nothing to scale.
const ON_ANCHOR: f64 = 1e-6;

/// The handles that can change something for `editable`: corners and edges
/// whose drag scales an editable axis (not on the point `pivot` turns about,
/// or on its line for that axis), edges only for a per-axis scale, and the
/// rotation handle with `Rotation`. With `resize` (the Size tool), the
/// corners and edges the box has that can resize it (see `resizes`).
pub(crate) fn shown_handles(
    frame: &LayerFrame,
    editable: &[PropKey],
    stem: f64,
    pivot: PivotMode,
    resize: Option<&Resize>,
) -> Vec<(Handle, Point)> {
    let can = |prop: TransformProp| editable.contains(&prop.key());
    let uniform = can(TransformProp::Scale);
    let scales_x = uniform || can(TransformProp::ScaleX);
    let scales_y = uniform || can(TransformProp::ScaleY);
    let rotates = can(TransformProp::Rotation);
    if let Some(resize) = resize {
        return handles_on(&handle_quad(frame, Some(resize)), false, stem)
            .into_iter()
            .filter(|&(handle, _)| match handle {
                Handle::Corner(_) | Handle::Edge(_) => resizes(frame, handle, editable, resize),
                Handle::Rotate => rotates,
                Handle::Move | Handle::Anchor => true,
            })
            .collect();
    }
    let about = turning_point(frame, pivot);
    handles(frame, uniform, stem)
        .into_iter()
        .filter(|&(handle, at)| {
            let reaches = |x_axis: bool, y_axis: bool| {
                local_offset(frame, at, about).is_some_and(|offset| {
                    (x_axis && scales_x && offset.x.abs() > ON_ANCHOR)
                        || (y_axis && scales_y && offset.y.abs() > ON_ANCHOR)
                })
            };
            match handle {
                Handle::Corner(_) => reaches(true, true),
                Handle::Edge(i) => reaches(i % 2 == 1, i % 2 == 0),
                Handle::Rotate => rotates,
                Handle::Move | Handle::Anchor => true,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas_geometry::{box_frame, with_anchor_box};
    use celesta_react_bridge::ReceivedValue;

    fn anchor_key(axis: &str) -> PropKey {
        PropKey::Prop(format!("anchor{axis}"))
    }

    #[test]
    fn received_offset_applies_to_anchors_and_sizes() {
        // Evaluated anchor (0.5, 0.5); the runtime saw `anchorX` 0.25 and no
        // `anchorY`.
        let frame = LayerFrame {
            transform: EvaluatedTransform {
                anchor: Point { x: 0.5, y: 0.5 },
                ..EvaluatedTransform::default()
            },
            parent: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            outline: [Point { x: 0.0, y: 0.0 }; 4],
            anchor_box: None,
            leaves: Vec::new(),
        };
        let received = ReceivedProps {
            others: [("anchorX".to_owned(), ReceivedValue::Number(0.25))].into(),
            ..ReceivedProps::default()
        };
        // The drag moves each anchor by +0.1.
        let values = [(anchor_key("X"), 0.6), (anchor_key("Y"), 0.6)];
        let written = prop_values(&frame, &values, &received, None);
        assert_eq!(
            written
                .iter()
                .map(|(key, _)| key.clone())
                .collect::<Vec<_>>(),
            vec![anchor_key("X"), anchor_key("Y")]
        );
        assert!((written[0].1 - 0.35).abs() < 1e-9, "{written:?}");
        assert!((written[1].1 - 0.6).abs() < 1e-9, "{written:?}");
        assert_eq!(
            drag_label(&frame, &values, &received, None),
            "anchorX 0.25 → 0.35  anchorY 0.5 → 0.6"
        );

        // A size: the box is 100 wide while the runtime saw `width` 80, and
        // the drag makes it 130.
        let frame = box_frame(top_left_rect(), IDENTITY, 100.0, 50.0);
        let received = ReceivedProps {
            others: [("width".to_owned(), ReceivedValue::Number(80.0))].into(),
            ..ReceivedProps::default()
        };
        let resize = box_resize(true);
        let values = [(size_key("width"), 130.0)];
        assert_eq!(
            prop_values(&frame, &values, &received, Some(&resize)),
            [(size_key("width"), 110.0)]
        );
        assert_eq!(
            drag_label(&frame, &values, &received, Some(&resize)),
            "width 80 → 110"
        );
    }

    fn size_key(name: &str) -> PropKey {
        PropKey::Prop(name.to_owned())
    }

    fn field(name: &str) -> SizeField {
        SizeField {
            name: name.to_owned(),
            min: None,
            max: None,
            integer: false,
            absent: false,
            keyframed: false,
        }
    }

    /// The Size tool on a `width` x `height` box.
    fn box_resize(compensable: bool) -> Resize {
        Resize {
            props: SizeProps::Box {
                width: field("width"),
                height: field("height"),
            },
            compensable,
            animated: false,
        }
    }

    /// `uniform()` and the size props `resize` holds.
    fn with_sizes(resize: &Resize) -> Vec<PropKey> {
        let mut editable = uniform();
        editable.extend(resize.keys());
        editable
    }

    fn size(values: &[(PropKey, f64)], name: &str) -> f64 {
        values
            .iter()
            .find(|(key, _)| *key == size_key(name))
            .map(|(_, value)| *value)
            .unwrap_or_else(|| panic!("no {name}: {values:?}"))
    }

    /// A Size-tool drag of `handle` from own pixels `from` to `to` of
    /// `before`'s frame.
    fn resize_drag(
        frame: &LayerFrame,
        handle: Handle,
        (from, to): (Point, Point),
        shift: bool,
        resize: &Resize,
    ) -> Vec<(PropKey, f64)> {
        let transform = &frame.transform;
        drag_values(
            frame,
            handle,
            (scene(frame, transform, from), scene(frame, transform, to)),
            shift,
            0.0,
            &with_sizes(resize),
            // The opposite edge stays whatever the pivot mode.
            PivotMode::Center,
            Some(resize),
        )
    }

    fn assert_corners_kept(after: &LayerFrame, before: &LayerFrame, corners: &[usize]) {
        for &i in corners {
            assert_close(after.outline[i], before.outline[i]);
        }
    }

    #[test]
    fn right_edge_resize_at_anchor_0_rotated() {
        let before = EvaluatedTransform {
            rotation: 30.0,
            ..top_left_rect()
        };
        let frame = box_frame(before, turned_parent(), 100.0, 50.0);
        let values = resize_drag(
            &frame,
            Handle::Edge(1),
            (at(100.0, 25.0), at(150.0, 40.0)),
            false,
            &box_resize(true),
        );
        // Along the layer's own x axis only; the anchor is on the left edge,
        // which stays, so x and y do too.
        assert_eq!(names(&values), ["width"]);
        assert!((size(&values, "width") - 150.0).abs() < 1e-9, "{values:?}");
        let after = box_frame(moved(&before, &values), turned_parent(), 150.0, 50.0);
        assert_corners_kept(&after, &frame, &[0, 3]);
    }

    #[test]
    fn left_edge_resize_at_anchor_0_5_rotated() {
        let before = EvaluatedTransform {
            position: at(60.0, 40.0),
            rotation: 30.0,
            anchor: at(0.5, 0.5),
            ..EvaluatedTransform::default()
        };
        let frame = box_frame(before, turned_parent(), 100.0, 50.0);
        let values = resize_drag(
            &frame,
            Handle::Edge(3),
            (at(-50.0, 0.0), at(-80.0, 5.0)),
            false,
            &box_resize(true),
        );
        assert_eq!(names(&values), ["width", "x", "y"]);
        assert!((size(&values, "width") - 130.0).abs() < 1e-9, "{values:?}");
        // The right edge stays where it was.
        let after = box_frame(moved(&before, &values), turned_parent(), 130.0, 50.0);
        assert_corners_kept(&after, &frame, &[1, 2]);
    }

    #[test]
    fn a_skewed_layer_left_edge() {
        let before = EvaluatedTransform {
            position: at(60.0, 40.0),
            rotation: 15.0,
            skew: at(20.0, 0.0),
            origin: at(40.0, 0.0),
            anchor: at(0.5, 0.5),
            ..EvaluatedTransform::default()
        };
        let frame = box_frame(before, turned_parent(), 100.0, 50.0);
        let values = resize_drag(
            &frame,
            Handle::Edge(3),
            (at(-50.0, 0.0), at(-90.0, 0.0)),
            false,
            &box_resize(true),
        );
        assert!((size(&values, "width") - 140.0).abs() < 1e-9, "{values:?}");
        let after = box_frame(moved(&before, &values), turned_parent(), 140.0, 50.0);
        assert_corners_kept(&after, &frame, &[1, 2]);
    }

    #[test]
    fn shift_corner_keeps_the_aspect() {
        let before = top_left_rect();
        let frame = box_frame(before, turned_parent(), 100.0, 50.0);
        let drag = |shift| {
            resize_drag(
                &frame,
                Handle::Corner(2),
                (at(100.0, 50.0), at(180.0, 60.0)),
                shift,
                &box_resize(true),
            )
        };
        let free = drag(false);
        assert_eq!(names(&free), ["width", "height"]);
        assert!((size(&free, "width") - 180.0).abs() < 1e-9, "{free:?}");
        assert!((size(&free, "height") - 60.0).abs() < 1e-9, "{free:?}");
        // The pointer projected onto the box's diagonal: 1.68 times as big.
        let kept = drag(true);
        assert!((size(&kept, "width") - 168.0).abs() < 1e-9, "{kept:?}");
        assert!((size(&kept, "height") - 84.0).abs() < 1e-9, "{kept:?}");
    }

    #[test]
    fn radius_corners_resize_uniformly() {
        // A circle of radius 30 anchored at its center.
        let before = EvaluatedTransform {
            position: at(60.0, 40.0),
            rotation: 30.0,
            anchor: at(0.5, 0.5),
            ..EvaluatedTransform::default()
        };
        let frame = box_frame(before, turned_parent(), 60.0, 60.0);
        let resize = Resize {
            props: SizeProps::Radius(field("radius")),
            compensable: true,
            animated: false,
        };
        let values = resize_drag(
            &frame,
            Handle::Corner(2),
            (at(30.0, 30.0), at(45.0, 39.0)),
            false,
            &resize,
        );
        // (75 · 60 + 69 · 60) / (60² + 60²) = 1.2 times the box.
        assert_eq!(names(&values), ["radius", "x", "y"]);
        assert!((size(&values, "radius") - 36.0).abs() < 1e-9, "{values:?}");
        let after = box_frame(moved(&before, &values), turned_parent(), 72.0, 72.0);
        assert_corners_kept(&after, &frame, &[0]);
        // Corners only.
        let shown: Vec<Handle> = shown_handles(
            &frame,
            &with_sizes(&resize),
            20.0,
            PivotMode::Center,
            Some(&resize),
        )
        .into_iter()
        .map(|(handle, _)| handle)
        .collect();
        assert_eq!(
            shown,
            [
                Handle::Corner(0),
                Handle::Corner(1),
                Handle::Corner(2),
                Handle::Corner(3),
                Handle::Rotate
            ]
        );
    }

    #[test]
    fn text_max_width_starts_from_the_laid_out_box() {
        // A text whose 120 x 40 layout box is padded 12 px on the left and
        // right by its texture; no `maxWidth` written.
        let before = EvaluatedTransform {
            position: at(20.0, 30.0),
            rotation: 30.0,
            anchor: at(0.0, 0.0),
            ..EvaluatedTransform::default()
        };
        let layout = box_frame(before, turned_parent(), 120.0, 40.0).outline;
        let quad = [
            at(-12.0, 0.0),
            at(132.0, 0.0),
            at(132.0, 40.0),
            at(-12.0, 40.0),
        ]
        .map(|local| {
            scene(
                &box_frame(before, turned_parent(), 1.0, 1.0),
                &before,
                local,
            )
        });
        let frame = LayerFrame {
            outline: quad,
            anchor_box: Some(layout),
            ..box_frame(before, turned_parent(), 120.0, 40.0)
        };
        let resize = Resize {
            props: SizeProps::Width(field("maxWidth")),
            compensable: true,
            animated: false,
        };
        // From the right handle, on the padded quad, 50 px out.
        let values = resize_drag(
            &frame,
            Handle::Edge(1),
            (at(132.0, 20.0), at(182.0, 20.0)),
            false,
            &resize,
        );
        assert_eq!(names(&values), ["maxWidth"]);
        let none = ReceivedProps::default();
        assert_eq!(
            prop_values(&frame, &values, &none, Some(&resize)),
            [(size_key("maxWidth"), 170.0)]
        );
        // Written as the new box width, whatever the runtime saw.
        let wide = ReceivedProps {
            others: [("maxWidth".to_owned(), ReceivedValue::Number(300.4))].into(),
            ..ReceivedProps::default()
        };
        assert_eq!(
            drag_label(&frame, &values, &wide, Some(&resize)),
            "maxWidth 300.4 → 170"
        );
        // The left and right edges only; the left one keeps the right edge,
        // so it moves x and y.
        let shown: Vec<Handle> = shown_handles(
            &frame,
            &with_sizes(&resize),
            20.0,
            PivotMode::Center,
            Some(&resize),
        )
        .into_iter()
        .map(|(handle, _)| handle)
        .collect();
        assert_eq!(shown, [Handle::Edge(1), Handle::Edge(3), Handle::Rotate]);
        let values = resize_drag(
            &frame,
            Handle::Edge(3),
            (at(-12.0, 20.0), at(-42.0, 20.0)),
            false,
            &resize,
        );
        assert_eq!(names(&values), ["maxWidth", "x", "y"]);
        assert!(
            (size(&values, "maxWidth") - 150.0).abs() < 1e-9,
            "{values:?}"
        );
        let after = moved(&before, &values);
        assert_close(
            scene(&frame, &after, at(150.0, 0.0)),
            scene(&frame, &before, at(120.0, 0.0)),
        );
    }

    #[test]
    fn resize_through_the_opposite_edge_clamps() {
        let before = top_left_rect();
        let frame = box_frame(before, turned_parent(), 100.0, 50.0);
        let through = (at(100.0, 25.0), at(-300.0, 25.0));
        // No minimum declared: one own pixel.
        let values = resize_drag(&frame, Handle::Edge(1), through, false, &box_resize(true));
        assert!((size(&values, "width") - 1.0).abs() < 1e-9, "{values:?}");
        // A declared minimum of 0 still keeps one pixel; a larger one holds.
        for (min, expected) in [(0.0, 1.0), (10.0, 10.0)] {
            let resize = Resize {
                props: SizeProps::Box {
                    width: SizeField {
                        min: Some(min),
                        ..field("width")
                    },
                    height: field("height"),
                },
                compensable: true,
                animated: false,
            };
            let values = resize_drag(&frame, Handle::Edge(1), through, false, &resize);
            assert!(
                (size(&values, "width") - expected).abs() < 1e-9,
                "{values:?}"
            );
        }
        // From the left: the right edge stays, the box never flips.
        let values = resize_drag(
            &frame,
            Handle::Edge(3),
            (at(0.0, 25.0), at(400.0, 25.0)),
            false,
            &box_resize(true),
        );
        assert!((size(&values, "width") - 1.0).abs() < 1e-9, "{values:?}");
        let after = box_frame(moved(&before, &values), turned_parent(), 1.0, 50.0);
        assert_corners_kept(&after, &frame, &[1, 2]);
        // A maximum and whole numbers.
        let resize = Resize {
            props: SizeProps::Box {
                width: SizeField {
                    max: Some(120.0),
                    integer: true,
                    ..field("width")
                },
                height: field("height"),
            },
            compensable: true,
            animated: false,
        };
        let values = resize_drag(
            &frame,
            Handle::Edge(1),
            (at(100.0, 25.0), at(110.4, 25.0)),
            false,
            &resize,
        );
        assert!((size(&values, "width") - 110.0).abs() < 1e-9, "{values:?}");
        let values = resize_drag(
            &frame,
            Handle::Edge(1),
            (at(100.0, 25.0), at(200.0, 25.0)),
            false,
            &resize,
        );
        assert!((size(&values, "width") - 120.0).abs() < 1e-9, "{values:?}");
    }

    #[test]
    fn left_edge_hidden_without_compensable_x() {
        let frame = box_frame(top_left_rect(), turned_parent(), 100.0, 50.0);
        let shown = |resize: &Resize, frame: &LayerFrame| -> Vec<Handle> {
            shown_handles(
                frame,
                &with_sizes(resize),
                20.0,
                PivotMode::Center,
                Some(resize),
            )
            .into_iter()
            .map(|(handle, _)| handle)
            .collect()
        };
        // Every corner and edge (edges even with a uniform scale).
        assert_eq!(shown(&box_resize(true), &frame).len(), 9);
        // Anchored at the top-left: only what leaves the top and left edges
        // where they are.
        assert_eq!(
            shown(&box_resize(false), &frame),
            [
                Handle::Corner(2),
                Handle::Edge(1),
                Handle::Edge(2),
                Handle::Rotate
            ]
        );
        // Anchored at the center, every handle moves the anchor point.
        let centered = box_frame(
            EvaluatedTransform {
                anchor: at(0.5, 0.5),
                ..top_left_rect()
            },
            turned_parent(),
            100.0,
            50.0,
        );
        assert_eq!(shown(&box_resize(false), &centered), [Handle::Rotate]);
        // The values agree: nothing without x and y.
        let values = drag_values(
            &frame,
            Handle::Edge(3),
            (
                scene(&frame, &frame.transform, at(0.0, 25.0)),
                scene(&frame, &frame.transform, at(-20.0, 25.0)),
            ),
            false,
            0.0,
            &with_sizes(&box_resize(false)),
            PivotMode::Pivot,
            Some(&box_resize(false)),
        );
        assert!(values.is_empty(), "{values:?}");
    }

    #[test]
    fn preview_overrides_size_and_x_y_from_the_first_move() {
        let frame = box_frame(top_left_rect(), turned_parent(), 100.0, 50.0);
        let resize = box_resize(true);
        let received = ReceivedProps::default();
        let first_move = |handle, from: Point| {
            let values = resize_drag(&frame, handle, (from, from), false, &resize);
            assert!(
                prop_values(&frame, &values, &received, Some(&resize)).is_empty(),
                "{values:?}"
            );
            preview_values(
                &frame,
                handle,
                &with_sizes(&resize),
                &values,
                &received,
                PivotMode::Center,
                Some(&resize),
            )
        };
        // Every size the handle changes, at the box's own size, and x/y when
        // it moves the anchor point.
        let preview = first_move(Handle::Edge(3), at(0.0, 25.0));
        assert_eq!(names(&preview), ["width", "x", "y"]);
        assert!(
            (size(&preview, "width") - 100.0).abs() < 1e-6,
            "{preview:?}"
        );
        assert_eq!(
            names(&first_move(Handle::Edge(1), at(100.0, 25.0))),
            ["width"]
        );
        assert_eq!(
            names(&first_move(Handle::Corner(0), at(0.0, 0.0))),
            ["width", "height", "x", "y"]
        );
        // The rotation stem stays as it was.
        assert_eq!(
            names(&preview_values(
                &frame,
                Handle::Rotate,
                &with_sizes(&resize),
                &[],
                &received,
                PivotMode::Center,
                Some(&resize),
            )),
            ["rotation", "x", "y"]
        );
    }

    /// The scene-axis box around `quad`, in the outline's order, as a
    /// group's outline is.
    fn bounding(quad: [Point; 4]) -> [Point; 4] {
        let xs = quad.map(|p| p.x);
        let ys = quad.map(|p| p.y);
        let (left, right) = (
            xs.iter().copied().fold(f64::INFINITY, f64::min),
            xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        );
        let (top, bottom) = (
            ys.iter().copied().fold(f64::INFINITY, f64::min),
            ys.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        );
        [
            at(left, top),
            at(right, top),
            at(right, bottom),
            at(left, bottom),
        ]
    }

    fn shown_at(frame: &LayerFrame, resize: &Resize, handle: Handle) -> Point {
        shown_handles(
            frame,
            &with_sizes(resize),
            20.0,
            PivotMode::Center,
            Some(resize),
        )
        .into_iter()
        .find(|(candidate, _)| *candidate == handle)
        .map(|(_, point)| point)
        .unwrap_or_else(|| panic!("no {handle:?}"))
    }

    #[test]
    fn rect_tool_handles_sit_on_the_box_they_resize() {
        // A dashed `<Rect>` drawn as a group, turned a quarter: its outline
        // is its children's scene-axis box, padded by the stroke.
        let before = EvaluatedTransform {
            rotation: 90.0,
            ..top_left_rect()
        };
        let boxed = box_frame(before, IDENTITY, 100.0, 50.0);
        let mut padded = bounding(boxed.outline);
        padded[0].x -= 4.0;
        let frame = LayerFrame {
            outline: padded,
            ..boxed.clone()
        };
        let resize = box_resize(true);
        // Own corner 2 (bottom-right), not the outline's.
        let corner = shown_at(&frame, &resize, Handle::Corner(2));
        assert_close(corner, scene(&frame, &before, at(100.0, 50.0)));
        assert_close(
            shown_at(&frame, &resize, Handle::Edge(1)),
            scene(&frame, &before, at(100.0, 25.0)),
        );
        // Dragged outward along the box, it grows the box.
        let values = drag_values(
            &frame,
            Handle::Corner(2),
            (corner, scene(&frame, &before, at(110.0, 60.0))),
            false,
            0.0,
            &with_sizes(&resize),
            PivotMode::Center,
            Some(&resize),
        );
        assert!((size(&values, "width") - 110.0).abs() < 1e-9, "{values:?}");
        assert!((size(&values, "height") - 60.0).abs() < 1e-9, "{values:?}");
        // The rotation stem starts from the box's top edge too.
        let (base, _) = rotate_stem(&frame, 20.0, Some(&resize));
        assert_close(base, scene(&frame, &before, at(50.0, 0.0)));
        // The Scale tool keeps the outline.
        assert_eq!(
            rotate_stem(&frame, 20.0, None).0,
            edge_middle(&frame.outline, 0)
        );
    }

    #[test]
    fn an_absent_size_is_pinned_by_an_edge_drag() {
        // An `<Image>` written with a width only: its height comes from the
        // source's aspect, so a width drag writes the height it has.
        let frame = box_frame(top_left_rect(), turned_parent(), 100.0, 75.0);
        let resize = Resize {
            props: SizeProps::Box {
                width: field("width"),
                height: SizeField {
                    absent: true,
                    ..field("height")
                },
            },
            compensable: true,
            animated: false,
        };
        let values = resize_drag(
            &frame,
            Handle::Edge(1),
            (at(100.0, 30.0), at(140.0, 30.0)),
            false,
            &resize,
        );
        assert_eq!(names(&values), ["width", "height"]);
        let none = ReceivedProps::default();
        assert_eq!(
            prop_values(&frame, &values, &none, Some(&resize)),
            [(size_key("width"), 140.0), (size_key("height"), 75.0)]
        );
        assert_eq!(
            drag_label(&frame, &values, &none, Some(&resize)),
            "width 100 → 140  height 75 → 75"
        );
        // From the first move, and nothing written while nothing changes.
        let still = resize_drag(
            &frame,
            Handle::Edge(1),
            (at(100.0, 30.0), at(100.0, 30.0)),
            false,
            &resize,
        );
        assert!(prop_values(&frame, &still, &none, Some(&resize)).is_empty());
        assert_eq!(
            names(&preview_values(
                &frame,
                Handle::Edge(1),
                &with_sizes(&resize),
                &still,
                &none,
                PivotMode::Center,
                Some(&resize),
            )),
            ["width", "height"]
        );
        // The height's own edge writes it alone.
        let values = resize_drag(
            &frame,
            Handle::Edge(2),
            (at(50.0, 75.0), at(50.0, 95.0)),
            false,
            &resize,
        );
        assert_eq!(names(&values), ["height"]);
    }

    #[test]
    fn max_width_resize_keeps_the_far_edge_at_any_anchor() {
        let resize = Resize {
            props: SizeProps::Width(field("maxWidth")),
            compensable: true,
            animated: false,
        };
        for anchor_x in [0.5, 1.0] {
            let before = EvaluatedTransform {
                position: at(80.0, 30.0),
                rotation: 30.0,
                anchor: at(anchor_x, 0.0),
                ..EvaluatedTransform::default()
            };
            let frame = box_frame(before, turned_parent(), 120.0, 40.0);
            let left = -anchor_x * 120.0;
            for (handle, from, to, kept) in [
                (Handle::Edge(1), left + 120.0, left + 170.3, 0.0),
                (Handle::Edge(3), left, left - 30.4, 1.0),
            ] {
                let values = resize_drag(
                    &frame,
                    handle,
                    (at(from, 20.0), at(to, 20.0)),
                    false,
                    &resize,
                );
                // Whole own pixels, as the text is laid out.
                let width = size(&values, "maxWidth");
                assert_eq!(width, if kept == 0.0 { 170.0 } else { 150.0 }, "{values:?}");
                // The edge it keeps stays, at the pivot's height.
                let moves = (kept - anchor_x).abs() > 1e-9;
                assert_eq!(
                    names(&values).len(),
                    if moves { 3 } else { 1 },
                    "{values:?}"
                );
                let after = moved(&before, &values);
                assert_close(
                    scene(&frame, &after, at((kept - anchor_x) * width, 0.0)),
                    scene(&frame, &before, at((kept - anchor_x) * 120.0, 0.0)),
                );
            }
        }
    }

    #[test]
    fn the_cross_marks_what_the_drag_turns_about() {
        let before = EvaluatedTransform {
            rotation: 20.0,
            origin: at(10.0, 5.0),
            ..top_left_rect()
        };
        let frame = box_frame(before, turned_parent(), 100.0, 50.0);
        // Center: the box's middle, which Center keeps still.
        let Marker::Center(center) = marker(&frame, PivotMode::Center) else {
            panic!("the center is marked");
        };
        assert_close(center, scene(&frame, &before, at(50.0, 25.0)));
        // Pivot: anchor + origin, where the Alt drag starts.
        assert_eq!(
            marker(&frame, PivotMode::Pivot),
            Marker::Pivot(pivot_cross(&frame))
        );
        // Without a flat box Center can't apply: the pivot.
        let mut tilted = frame.clone();
        tilted.transform.rotation_y = 30.0;
        tilted.transform.perspective = Some(800.0);
        assert_eq!(
            marker(&tilted, PivotMode::Center),
            Marker::Pivot(pivot_cross(&tilted))
        );
    }

    #[test]
    fn the_pivot_is_the_default() {
        // `x`/`y` are often constants or computed, so Center rarely applies:
        // it is opt-in.
        assert_eq!(PivotMode::default(), PivotMode::Pivot);
        assert_eq!(ResizeTool::default(), ResizeTool::Size);
    }

    #[test]
    fn written_values_are_what_compensates() {
        // An anchor drag to a third of the box writes 0.33: x and y keep
        // the box for 0.33, not for 1/3.
        let before = EvaluatedTransform {
            anchor: at(0.0, 0.0),
            ..EvaluatedTransform::default()
        };
        let frame = box_frame(before, IDENTITY, 300.0, 90.0);
        let values = drag_values(
            &frame,
            Handle::Anchor,
            (at(0.0, 0.0), at(100.0, 30.0)),
            false,
            0.0,
            &anchor_props(),
            PivotMode::Pivot,
            None,
        );
        let after = moved(&before, &values);
        assert!((after.anchor.x - 0.33).abs() < 1e-12, "{values:?}");
        assert_same_outline(&box_frame(after, IDENTITY, 300.0, 90.0), &frame);

        // A size the same: 100.004 is written 100.
        let centered = EvaluatedTransform {
            anchor: at(0.5, 0.5),
            ..before
        };
        let frame = box_frame(centered, IDENTITY, 100.0, 50.0);
        let values = resize_drag(
            &frame,
            Handle::Edge(3),
            (at(-50.0, 0.0), at(-50.004, 0.0)),
            false,
            &box_resize(true),
        );
        assert_eq!(size(&values, "width"), 100.0);
        let x = value(&values, TransformProp::X);
        assert!(x.abs() < 1e-12, "{values:?}");
    }

    fn at(x: f64, y: f64) -> Point {
        Point { x, y }
    }

    const IDENTITY: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

    fn assert_close(actual: Point, expected: Point) {
        assert!(
            (actual.x - expected.x).abs() < 1e-6 && (actual.y - expected.y).abs() < 1e-6,
            "{actual:?} != {expected:?}"
        );
    }

    fn keys(props: &[TransformProp]) -> Vec<PropKey> {
        props.iter().map(|prop| prop.key()).collect()
    }

    fn uniform() -> Vec<PropKey> {
        keys(&[
            TransformProp::X,
            TransformProp::Y,
            TransformProp::Rotation,
            TransformProp::Scale,
        ])
    }

    fn per_axis() -> Vec<PropKey> {
        keys(&[
            TransformProp::X,
            TransformProp::Y,
            TransformProp::Rotation,
            TransformProp::ScaleX,
            TransformProp::ScaleY,
        ])
    }

    /// A parent turned 45° and scaled 2, moved off the origin.
    fn turned_parent() -> [f64; 6] {
        let (sin, cos) = 45.0_f64.to_radians().sin_cos();
        [2.0 * cos, 2.0 * sin, -2.0 * sin, 2.0 * cos, 30.0, -20.0]
    }

    /// `transform` with the drag's `values` put in.
    fn moved(transform: &EvaluatedTransform, values: &[(PropKey, f64)]) -> EvaluatedTransform {
        let mut after = *transform;
        for (key, value) in values {
            match TransformProp::from_key(key) {
                Some(TransformProp::X) => after.position.x = *value,
                Some(TransformProp::Y) => after.position.y = *value,
                Some(TransformProp::Rotation) => after.rotation = *value,
                Some(TransformProp::Scale) => after.scale = at(*value, *value),
                Some(TransformProp::ScaleX) => after.scale.x = *value,
                Some(TransformProp::ScaleY) => after.scale.y = *value,
                None => match prop_name(key) {
                    Some("anchorX") => after.anchor.x = *value,
                    Some("anchorY") => after.anchor.y = *value,
                    // A size changes the box, not the transform.
                    Some("width" | "height" | "radius" | "maxWidth") => {}
                    _ => panic!("not a drag prop: {key:?}"),
                },
            }
        }
        after
    }

    /// Where `local` (own pixels of `transform`) lands in the scene.
    fn scene(frame: &LayerFrame, transform: &EvaluatedTransform, local: Point) -> Point {
        apply(&frame.parent, apply(&layer_map(transform), local))
    }

    /// `point` turned by `degrees` (clockwise on screen) about `about`.
    fn turned(about: Point, point: Point, degrees: f64) -> Point {
        let (sin, cos) = degrees.to_radians().sin_cos();
        let (dx, dy) = (point.x - about.x, point.y - about.y);
        at(about.x + dx * cos - dy * sin, about.y + dx * sin + dy * cos)
    }

    fn names(values: &[(PropKey, f64)]) -> Vec<&str> {
        values
            .iter()
            .filter_map(|(key, _)| prop_name(key))
            .collect()
    }

    fn value(values: &[(PropKey, f64)], prop: TransformProp) -> f64 {
        values
            .iter()
            .find(|(key, _)| *key == prop.key())
            .map(|(_, value)| *value)
            .unwrap_or_else(|| panic!("no {}: {values:?}", prop.name()))
    }

    /// A 100 × 50 `Rect` anchored at its top-left corner at (10, 10).
    fn top_left_rect() -> EvaluatedTransform {
        EvaluatedTransform {
            position: at(10.0, 10.0),
            anchor: at(0.0, 0.0),
            ..EvaluatedTransform::default()
        }
    }

    #[test]
    fn center_rotation_keeps_the_box_center() {
        let before = top_left_rect();
        let frame = box_frame(before, turned_parent(), 100.0, 50.0);
        let center = at(50.0, 25.0);
        let middle = scene(&frame, &before, center);
        let start = at(middle.x + 100.0, middle.y);
        let values = drag_values(
            &frame,
            Handle::Rotate,
            (start, turned(middle, start, 30.0)),
            false,
            0.0,
            &uniform(),
            PivotMode::Center,
            None,
        );
        assert_eq!(names(&values), ["rotation", "x", "y"]);
        assert!((value(&values, TransformProp::Rotation) - 30.0).abs() < 1e-9);
        let after = moved(&before, &values);
        assert_close(scene(&frame, &after, center), middle);
    }

    #[test]
    fn center_scale_keeps_the_box_center() {
        let before = EvaluatedTransform {
            rotation: 20.0,
            ..top_left_rect()
        };
        let frame = box_frame(before, turned_parent(), 100.0, 50.0);
        let (center, corner) = (at(50.0, 25.0), at(100.0, 50.0));
        let middle = scene(&frame, &before, center);
        let start = scene(&frame, &before, corner);

        // Uniform: the corner dragged half again as far from the center.
        let now = at(
            middle.x + 1.5 * (start.x - middle.x),
            middle.y + 1.5 * (start.y - middle.y),
        );
        let values = drag_values(
            &frame,
            Handle::Corner(2),
            (start, now),
            false,
            0.0,
            &uniform(),
            PivotMode::Center,
            None,
        );
        assert_eq!(names(&values), ["scale", "x", "y"]);
        assert!((value(&values, TransformProp::Scale) - 1.5).abs() < 1e-9);
        let after = moved(&before, &values);
        assert_close(scene(&frame, &after, center), middle);
        assert_close(scene(&frame, &after, corner), now);

        // Per axis: twice as wide and 1.2 times as tall about the center.
        let now = scene(&frame, &before, at(150.0, 55.0));
        let values = drag_values(
            &frame,
            Handle::Corner(2),
            (start, now),
            false,
            0.0,
            &per_axis(),
            PivotMode::Center,
            None,
        );
        assert_eq!(names(&values), ["scaleX", "scaleY", "x", "y"]);
        assert!((value(&values, TransformProp::ScaleX) - 2.0).abs() < 1e-9);
        assert!((value(&values, TransformProp::ScaleY) - 1.2).abs() < 1e-9);
        let after = moved(&before, &values);
        assert_close(scene(&frame, &after, center), middle);
        assert_close(scene(&frame, &after, corner), now);
    }

    #[test]
    fn pivot_mode_turns_about_anchor_and_origin() {
        let before = EvaluatedTransform {
            origin: at(40.0, 0.0),
            ..top_left_rect()
        };
        let frame = box_frame(before, turned_parent(), 100.0, 50.0);
        let turning = apply(&frame.parent, pivot(&before));
        let start = at(turning.x + 100.0, turning.y);
        let values = drag_values(
            &frame,
            Handle::Rotate,
            (start, turned(turning, start, 30.0)),
            false,
            0.0,
            &uniform(),
            PivotMode::Pivot,
            None,
        );
        // Measured about anchor + origin, and nothing compensates.
        assert_eq!(names(&values), ["rotation"]);
        assert!((value(&values, TransformProp::Rotation) - 30.0).abs() < 1e-9);
        let after = moved(&before, &values);
        assert_close(scene(&frame, &after, before.origin), turning);

        // A scale keeps it too, and measures from it.
        let start = scene(&frame, &before, at(100.0, 50.0));
        let now = at(
            turning.x + 2.0 * (start.x - turning.x),
            turning.y + 2.0 * (start.y - turning.y),
        );
        let values = drag_values(
            &frame,
            Handle::Corner(2),
            (start, now),
            false,
            0.0,
            &uniform(),
            PivotMode::Pivot,
            None,
        );
        assert_eq!(names(&values), ["scale"]);
        assert!((value(&values, TransformProp::Scale) - 2.0).abs() < 1e-9);
        let after = moved(&before, &values);
        assert_close(scene(&frame, &after, before.origin), turning);
    }

    #[test]
    fn center_mode_preview_overrides_x_and_y_from_the_first_move() {
        let before = top_left_rect();
        let frame = box_frame(before, turned_parent(), 100.0, 50.0);
        let middle = scene(&frame, &before, at(50.0, 25.0));
        let start = at(middle.x + 100.0, middle.y);
        // The first move: a hair, which writes nothing yet.
        let values = drag_values(
            &frame,
            Handle::Rotate,
            (start, turned(middle, start, 1e-9)),
            false,
            0.0,
            &uniform(),
            PivotMode::Center,
            None,
        );
        let received = ReceivedProps::default();
        assert!(prop_values(&frame, &values, &received, None).is_empty());
        let preview = |handle, editable: &[PropKey], pivot| {
            preview_values(&frame, handle, editable, &values, &received, pivot, None)
        };
        assert_eq!(
            names(&preview(Handle::Rotate, &uniform(), PivotMode::Center)),
            ["rotation", "x", "y"]
        );
        assert_eq!(
            names(&preview(Handle::Corner(1), &per_axis(), PivotMode::Center)),
            ["scaleX", "scaleY", "x", "y"]
        );
        assert_eq!(
            names(&preview(Handle::Edge(0), &uniform(), PivotMode::Center)),
            ["scale", "x", "y"]
        );
        // About the pivot nothing compensates.
        assert_eq!(
            names(&preview(Handle::Rotate, &uniform(), PivotMode::Pivot)),
            ["rotation"]
        );
    }

    /// `x`, `y` and both anchors.
    fn anchor_props() -> Vec<PropKey> {
        let mut props = keys(&[TransformProp::X, TransformProp::Y]);
        props.extend([anchor_key("X"), anchor_key("Y")]);
        props
    }

    fn assert_same_outline(after: &LayerFrame, before: &LayerFrame) {
        for (now, was) in after.outline.iter().zip(&before.outline) {
            assert_close(*now, *was);
        }
    }

    #[test]
    fn an_anchor_drag_keeps_the_layer_on_screen() {
        let parent = [2.0, 0.0, 0.0, 2.0, 5.0, -5.0];
        for origin in [at(0.0, 0.0), at(10.0, -5.0)] {
            let before = EvaluatedTransform {
                position: at(30.0, 40.0),
                rotation: 30.0,
                scale: at(1.5, 1.5),
                anchor: at(0.0, 0.0),
                origin,
                ..EvaluatedTransform::default()
            };
            let frame = box_frame(before, parent, 100.0, 50.0);
            // From the cross to the box's middle.
            let cross = apply(&parent, pivot(&before));
            // The cross is at the origin in own pixels: move it by half the
            // box.
            let middle = scene(&frame, &before, at(50.0 + origin.x, 25.0 + origin.y));
            let values = drag_values(
                &frame,
                Handle::Anchor,
                (cross, middle),
                false,
                0.0,
                &anchor_props(),
                PivotMode::Center,
                None,
            );
            assert_eq!(names(&values), ["anchorX", "anchorY", "x", "y"]);
            let after = moved(&before, &values);
            assert!((after.anchor.x - 0.5).abs() < 1e-9, "{values:?}");
            assert!((after.anchor.y - 0.5).abs() < 1e-9, "{values:?}");
            // Every corner stays where it was; the cross is now where the
            // pointer let go.
            let moved_frame = box_frame(after, parent, 100.0, 50.0);
            assert_same_outline(&moved_frame, &frame);
            assert_close(apply(&parent, pivot(&after)), middle);
        }
    }

    #[test]
    fn an_anchor_drag_measures_the_box_the_anchor_refers_to() {
        // A text whose quad is padded 12 px past its 100 x 50 layout box on
        // every side, turned and scaled under a scaled parent.
        let parent = [2.0, 0.0, 0.0, 2.0, 5.0, -5.0];
        let before = EvaluatedTransform {
            position: at(30.0, 40.0),
            rotation: 30.0,
            scale: at(1.5, 1.5),
            anchor: at(0.0, 0.0),
            ..EvaluatedTransform::default()
        };
        let layout = box_frame(before, parent, 100.0, 50.0).outline;
        let pad = 12.0;
        let quad = [
            at(-pad, -pad),
            at(100.0 + pad, -pad),
            at(100.0 + pad, 50.0 + pad),
            at(-pad, 50.0 + pad),
        ]
        .map(|local| apply(&parent, apply(&layer_map(&before), local)));
        let frame = LayerFrame {
            outline: quad,
            anchor_box: Some(layout),
            ..box_frame(before, parent, 100.0, 50.0)
        };
        let cross = apply(&parent, pivot(&before));
        let middle = scene(&frame, &before, at(50.0, 25.0));
        let values = drag_values(
            &frame,
            Handle::Anchor,
            (cross, middle),
            false,
            0.0,
            &anchor_props(),
            PivotMode::Center,
            None,
        );
        // Half of the layout box, not of the padded quad.
        let after = moved(&before, &values);
        assert!((after.anchor.x - 0.5).abs() < 1e-9, "{values:?}");
        assert!((after.anchor.y - 0.5).abs() < 1e-9, "{values:?}");
        // Every corner of the layout box stays where it was.
        let moved_layout = box_frame(after, parent, 100.0, 50.0).outline;
        for (now, was) in moved_layout.iter().zip(&layout) {
            assert_close(*now, *was);
        }
    }

    #[test]
    fn an_unwritten_anchor_starts_from_the_box() {
        // A shape's path: its layer has whatever anchor the runtime gives a
        // path, while the shape's props put its anchor at its top-left.
        let layer = EvaluatedTransform {
            anchor: at(0.5, 0.5),
            ..top_left_rect()
        };
        let mut path = box_frame(layer, IDENTITY, 100.0, 50.0);
        path.anchor_box = None;
        let shape = ReceivedProps {
            others: [
                ("width".to_owned(), ReceivedValue::Number(100.0)),
                ("height".to_owned(), ReceivedValue::Number(50.0)),
            ]
            .into(),
            ..ReceivedProps::default()
        };
        let frame = with_anchor_box(path, "Ellipse", &shape);
        let values = drag_values(
            &frame,
            Handle::Anchor,
            (at(10.0, 10.0), at(35.0, 10.0)),
            false,
            0.0,
            &anchor_props(),
            PivotMode::Pivot,
            None,
        );
        let written = prop_values(&frame, &values, &ReceivedProps::default(), None);
        assert_eq!(written[0].0, anchor_key("X"));
        assert!((written[0].1 - 0.25).abs() < 1e-9, "{written:?}");
        assert_eq!(
            drag_label(&frame, &values, &ReceivedProps::default(), None),
            "anchorX 0 → 0.25  x 10 → 35"
        );
    }

    #[test]
    fn anchor_values_are_fractions_of_the_box() {
        let before = EvaluatedTransform {
            anchor: at(0.0, 0.0),
            ..EvaluatedTransform::default()
        };
        let frame = box_frame(before, IDENTITY, 200.0, 100.0);
        let values = drag_values(
            &frame,
            Handle::Anchor,
            (at(0.0, 0.0), at(50.0, 25.0)),
            false,
            0.0,
            &anchor_props(),
            PivotMode::Pivot,
            None,
        );
        let expected = [0.25, 0.25, 50.0, 25.0];
        assert_eq!(names(&values), ["anchorX", "anchorY", "x", "y"]);
        for ((_, value), expected) in values.iter().zip(expected) {
            assert!((value - expected).abs() < 1e-9, "{values:?}");
        }
        // Past the box: an anchor outside it is kept as it is.
        let values = drag_values(
            &frame,
            Handle::Anchor,
            (at(0.0, 0.0), at(-100.0, 150.0)),
            false,
            0.0,
            &anchor_props(),
            PivotMode::Pivot,
            None,
        );
        assert!((values[0].1 + 0.5).abs() < 1e-9, "{values:?}");
        assert!((values[1].1 - 1.5).abs() < 1e-9, "{values:?}");
        // The preview overrides all four from the first move.
        assert_eq!(
            names(&preview_values(
                &frame,
                Handle::Anchor,
                &anchor_props(),
                &[],
                &ReceivedProps::default(),
                PivotMode::Pivot,
                None,
            )),
            ["anchorX", "anchorY", "x", "y"]
        );
    }

    #[test]
    fn alt_on_the_cross_takes_the_anchor() {
        let before = EvaluatedTransform {
            position: at(50.0, 30.0),
            anchor: at(0.5, 0.5),
            ..EvaluatedTransform::default()
        };
        let frame = box_frame(before, IDENTITY, 100.0, 60.0);
        let mut editable = uniform();
        editable.extend([anchor_key("X"), anchor_key("Y")]);
        let shown = shown_handles(&frame, &editable, 20.0, PivotMode::Center, None);
        let press = |point, alt| pressed_handle(&frame, &shown, &editable, point, 4.0, alt);
        assert_eq!(anchor_handle(&frame, &editable), Some(at(50.0, 30.0)));
        // Without Alt the cross is the body.
        assert_eq!(press(at(51.0, 31.0), false), Some(Handle::Move));
        assert_eq!(press(at(51.0, 31.0), true), Some(Handle::Anchor));
        // Off the cross Alt changes nothing.
        assert_eq!(press(at(0.0, 0.0), true), Some(Handle::Corner(0)));
        // Without the anchors there is no cross to drag.
        let shown = shown_handles(&frame, &uniform(), 20.0, PivotMode::Center, None);
        assert_eq!(anchor_handle(&frame, &uniform()), None);
        assert_eq!(
            pressed_handle(&frame, &shown, &uniform(), at(50.0, 30.0), 4.0, true),
            Some(Handle::Move)
        );
    }
}
