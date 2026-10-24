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

use crate::element_edit::{TransformProp, received_number};
use celesta_composition::{EvaluatedTransform, Point};
use celesta_gpu_renderer::{LayerFrame, invert_affine};
use celesta_react_bridge::ReceivedProps;
use celesta_source_edit::format_number;

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
    let outline = frame.outline;
    let mut handles: Vec<(Handle, Point)> = (0..4u8)
        .map(|i| (Handle::Corner(i), outline[usize::from(i)]))
        .collect();
    if !uniform {
        handles.extend((0..4u8).map(|i| (Handle::Edge(i), edge_middle(&outline, i))));
    }
    handles.push((Handle::Rotate, rotate_handle(&outline, stem)));
    handles
}

/// The middle of the top edge and the rotation handle at the stem's end.
pub(crate) fn rotate_stem(frame: &LayerFrame, stem: f64) -> (Point, Point) {
    (
        edge_middle(&frame.outline, 0),
        rotate_handle(&frame.outline, stem),
    )
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

fn apply(m: &[f64; 6], p: Point) -> Point {
    Point {
        x: m[0] * p.x + m[2] * p.y + m[4],
        y: m[1] * p.x + m[3] * p.y + m[5],
    }
}

/// How far a rotation drag from `start` to `now` (scene pixels) has turned
/// about the anchor, in degrees: the angle between them, plus the whole turns
/// that keep it nearest `previous` (the turn at the previous move), so a drag
/// that goes on past a half turn keeps counting instead of wrapping.
pub(crate) fn turn(frame: &LayerFrame, start: Point, now: Point, previous: f64) -> Option<f64> {
    let inverse = invert_affine(frame.parent)?;
    let (start, now) = (apply(&inverse, start), apply(&inverse, now));
    let anchor = frame.transform.position;
    let angle = |p: Point| (p.y - anchor.y).atan2(p.x - anchor.x).to_degrees();
    let raw = angle(now) - angle(start);
    Some(raw + ((previous - raw) / 360.0).round() * 360.0)
}

/// The transform values a drag of `handle` from `start` to `now` (scene
/// pixels) makes, for the props in `editable` only. The pointer moves are
/// taken into the layer's parent space; rotation turns about the anchor by
/// [`turn`] from `previous_turn` (the result snapped to 15° with `shift`),
/// and scaling keeps the anchor in place, in one ratio for a uniform `Scale`.
pub(crate) fn drag_values(
    frame: &LayerFrame,
    handle: Handle,
    (start, now): (Point, Point),
    shift: bool,
    previous_turn: f64,
    editable: &[TransformProp],
) -> Vec<(TransformProp, f64)> {
    let pointer = (start, now);
    let Some(inverse) = invert_affine(frame.parent) else {
        return Vec::new();
    };
    let (start, now) = (apply(&inverse, start), apply(&inverse, now));
    let transform = &frame.transform;
    let anchor = transform.position;
    let can = |prop| editable.contains(&prop);
    let mut values = Vec::new();
    match handle {
        Handle::Move => {
            if can(TransformProp::X) {
                values.push((TransformProp::X, anchor.x + now.x - start.x));
            }
            if can(TransformProp::Y) {
                values.push((TransformProp::Y, anchor.y + now.y - start.y));
            }
        }
        Handle::Rotate => {
            if can(TransformProp::Rotation)
                && let Some(delta) = turn(frame, pointer.0, pointer.1, previous_turn)
            {
                let mut rotation = transform.rotation + delta;
                if shift {
                    rotation = (rotation / SNAP_DEGREES).round() * SNAP_DEGREES;
                }
                values.push((TransformProp::Rotation, rotation));
            }
        }
        Handle::Corner(_) | Handle::Edge(_) => {
            let Some(unturn) = unturn(transform) else {
                return values;
            };
            let local = |p: Point| {
                let v = (p.x - anchor.x, p.y - anchor.y);
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
                    values.push((TransformProp::Scale, transform.scale.x * ratio));
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
                    values.push((TransformProp::ScaleX, transform.scale.x * ratio));
                }
                if y_axis
                    && can(TransformProp::ScaleY)
                    && let Some(ratio) = ratio(from.y, to.y)
                {
                    values.push((TransformProp::ScaleY, transform.scale.y * ratio));
                }
            }
        }
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

/// The value the drag started from for `prop`.
fn start_value(frame: &LayerFrame, prop: TransformProp) -> f64 {
    let transform = &frame.transform;
    match prop {
        TransformProp::X => transform.position.x,
        TransformProp::Y => transform.position.y,
        TransformProp::Rotation => transform.rotation,
        TransformProp::Scale | TransformProp::ScaleX => transform.scale.x,
        TransformProp::ScaleY => transform.scale.y,
    }
}

/// Each prop of the drag's `values` as the source has it (`was`) and as the
/// drag writes it (`now`): the prop the runtime saw (`received`) moved by the
/// same amount (position, rotation) or ratio (scale) as the transform; the
/// transform's value when it saw none.
fn prop_changes(
    frame: &LayerFrame,
    values: &[(TransformProp, f64)],
    received: &ReceivedProps,
) -> Vec<(TransformProp, f64, f64)> {
    values
        .iter()
        .map(|&(prop, value)| {
            let start = start_value(frame, prop);
            let was = received_number(received, prop.name()).unwrap_or(start);
            let now = match prop {
                TransformProp::Scale | TransformProp::ScaleX | TransformProp::ScaleY => {
                    was * ratio(start, value)
                }
                _ => was + value - start,
            };
            (prop, was, now)
        })
        .collect()
}

/// The props to write for the drag's `values` (see `prop_changes`), leaving
/// out those the source would keep as written (same formatted number). Empty
/// when the drag changed nothing.
pub(crate) fn prop_values(
    frame: &LayerFrame,
    values: &[(TransformProp, f64)],
    received: &ReceivedProps,
) -> Vec<(TransformProp, f64)> {
    prop_changes(frame, values, received)
        .into_iter()
        .filter(|&(_, was, now)| format_number(was) != format_number(now))
        .map(|(prop, _, now)| (prop, now))
        .collect()
}

/// The value of every prop `handle` changes in a drag (of those in
/// `editable`), for the preview: the drag's for the props it changed (see
/// `prop_values`), and the one the runtime saw when the drag started for the
/// others. The props are the same on every frame of the drag, so the
/// preview's overrides keep their positions and the runtime never remounts
/// for a new one.
pub(crate) fn preview_values(
    frame: &LayerFrame,
    handle: Handle,
    editable: &[TransformProp],
    values: &[(TransformProp, f64)],
    received: &ReceivedProps,
) -> Vec<(TransformProp, f64)> {
    let changed = prop_values(frame, values, received);
    handle_props(handle, editable)
        .into_iter()
        .map(|prop| {
            let value = changed
                .iter()
                .find(|(candidate, _)| *candidate == prop)
                .map_or_else(
                    || {
                        received_number(received, prop.name())
                            .unwrap_or_else(|| start_value(frame, prop))
                    },
                    |&(_, value)| value,
                );
            (prop, value)
        })
        .collect()
}

/// The props of `editable` a drag of `handle` may change (see `drag_values`).
fn handle_props(handle: Handle, editable: &[TransformProp]) -> Vec<TransformProp> {
    let uniform = editable.contains(&TransformProp::Scale);
    let props: &[TransformProp] = match handle {
        Handle::Move => &[TransformProp::X, TransformProp::Y],
        Handle::Rotate => &[TransformProp::Rotation],
        _ if uniform => &[TransformProp::Scale],
        Handle::Edge(i) if i % 2 == 1 => &[TransformProp::ScaleX],
        Handle::Edge(_) => &[TransformProp::ScaleY],
        Handle::Corner(_) => &[TransformProp::ScaleX, TransformProp::ScaleY],
    };
    props
        .iter()
        .copied()
        .filter(|prop| editable.contains(prop))
        .collect()
}

/// `x 120 → 184` for each prop the drag writes, with the values as written
/// to the source.
pub(crate) fn drag_label(
    frame: &LayerFrame,
    values: &[(TransformProp, f64)],
    received: &ReceivedProps,
) -> String {
    prop_changes(frame, values, received)
        .into_iter()
        .filter(|&(_, was, now)| format_number(was) != format_number(now))
        .map(|(prop, was, now)| {
            format!(
                "{} {} → {}",
                prop.name(),
                format_number(was),
                format_number(now)
            )
        })
        .collect::<Vec<_>>()
        .join("  ")
}

/// The offset of `point` (scene pixels) from the anchor in the layer's own
/// axes, still scaled; `None` when the parent or the layer collapses.
fn local_offset(frame: &LayerFrame, point: Point) -> Option<Point> {
    let inverse = invert_affine(frame.parent)?;
    let unturn = unturn(&frame.transform)?;
    let p = apply(&inverse, point);
    let anchor = frame.transform.position;
    let v = (p.x - anchor.x, p.y - anchor.y);
    Some(Point {
        x: unturn[0] * v.0 + unturn[2] * v.1,
        y: unturn[1] * v.0 + unturn[3] * v.1,
    })
}

/// Below this many scene pixels from the anchor's lines, a scale handle has
/// nothing to scale.
const ON_ANCHOR: f64 = 1e-6;

/// The handles that can change something for `editable`: corners and edges
/// whose drag scales an editable axis (not on the anchor, or on its line for
/// that axis), edges only for a per-axis scale, and the rotation handle with
/// `Rotation`.
pub(crate) fn shown_handles(
    frame: &LayerFrame,
    editable: &[TransformProp],
    stem: f64,
) -> Vec<(Handle, Point)> {
    let uniform = editable.contains(&TransformProp::Scale);
    let scales_x = uniform || editable.contains(&TransformProp::ScaleX);
    let scales_y = uniform || editable.contains(&TransformProp::ScaleY);
    let rotates = editable.contains(&TransformProp::Rotation);
    handles(frame, uniform, stem)
        .into_iter()
        .filter(|&(handle, at)| {
            let reaches = |x_axis: bool, y_axis: bool| {
                local_offset(frame, at).is_some_and(|offset| {
                    (x_axis && scales_x && offset.x.abs() > ON_ANCHOR)
                        || (y_axis && scales_y && offset.y.abs() > ON_ANCHOR)
                })
            };
            match handle {
                Handle::Corner(_) => reaches(true, true),
                Handle::Edge(i) => reaches(i % 2 == 1, i % 2 == 0),
                Handle::Rotate => rotates,
                Handle::Move => true,
            }
        })
        .collect()
}
