//! Dragging the selected React layer on the canvas: which props the drag may
//! change, the prop overrides the preview evaluates the scene with while it
//! runs, and the one write on release.

use crate::canvas_geometry::{anchor_box, apply, local_box, pivot, with_anchor_box};
use crate::element_edit::{
    ElementTargets, PropKey, TransformProp, editable, field_of, inner_target, reason_text,
};
use crate::layer_select::{letterbox, to_scene};
use crate::scope::{CalledHere, CommitGate, Scope, ScopeState, scope_in_effect};
use crate::transform_handles::{
    Handle, Marker, PivotMode, Resize, ResizeTool, SizeField, SizeProps, anchor_handle,
    anchor_keys, drag_label, drag_values, marker, pressed_handle, preview_values, prop_values,
    resizes_any, shown_handles, size_animated, turn,
};
use crate::view::EditorView;
use celesta_composition::Point;
use celesta_gpu_renderer::LayerFrame;
use celesta_react_bridge::{
    LayerSourceReport, PrimitivePropertyField, PropOverride, ReceivedProps, ReceivedValue,
    SourcePos,
};
use celesta_source_edit::{EditTarget, EditValue};
use gpui_kit::{Context, MouseMoveEvent};
use std::collections::HashMap;

/// Side of a square handle, in viewer pixels.
pub(crate) const HANDLE_SIZE: f32 = 7.0;
/// Length of the rotation handle's stem, in viewer pixels.
pub(crate) const ROTATE_STEM: f32 = 20.0;
/// How far from a handle's center a press still takes it, in viewer pixels.
const HANDLE_REACH: f32 = 6.0;

pub(crate) const NO_AFFINE_FRAME: &str = "Can't drag inside a 3D-transformed or collapsed group";

/// A running (or released and being written) drag of the selected layer.
#[derive(Clone, Debug)]
pub(crate) struct TransformDrag {
    /// The layer's frame when the drag started; every move is measured
    /// against it, not against the frames drawn with the overrides.
    frame: LayerFrame,
    handle: Handle,
    start: Point,
    editable: Vec<PropKey>,
    /// What a rotation or scale turns about, as used (see `pivot_used`).
    pivot: PivotMode,
    /// The Size tool, when the corners and edges resize (see
    /// `resize_used`); `None` when they scale.
    resize: Option<Resize>,
    /// The element's name (see `primitive_of`), which says how its anchor
    /// box is laid out; kept, as the targets are gone while the write runs.
    primitive: String,
    /// Why a rotation or scale turns about the pivot though Center is
    /// chosen (see `EditorView::pivot_note`), shown in the label.
    note: Option<&'static str>,
    /// The edit scope the control showed when the drag started (see
    /// `EditorView::kept_scope`): the drag is written under it or refused,
    /// and the label's second line says it.
    scope: Option<Scope>,
    /// The scene's calls of the element's component when it calls it more
    /// than once (see `ScopeOptions::called_here`): "This scene" then
    /// writes the selected one, as the label says.
    called_here: Option<CalledHere>,
    /// The props the runtime saw when the drag started.
    received: ReceivedProps,
    /// Where the runtime takes each editable prop's dragged value, kept from
    /// the start: the targets are gone while the write runs.
    sites: Vec<(PropKey, OverrideSite)>,
    /// How far a rotation drag has turned so far, unsnapped, in degrees.
    turned: f64,
    /// The values the pointer has made so far, by prop.
    values: Vec<(PropKey, f64)>,
    /// Released; the overrides stay while the write runs and, after a
    /// successful one, until its reload, so the old position does not flash
    /// before the reloaded code draws the new one.
    released: bool,
}

impl TransformDrag {
    /// A drag of `handle` pressed at `start` (scene pixels), before the
    /// pointer moves.
    pub(crate) fn new(
        frame: LayerFrame,
        handle: Handle,
        start: Point,
        editable: Vec<PropKey>,
        pivot: PivotMode,
        received: ReceivedProps,
        sites: Vec<(PropKey, OverrideSite)>,
    ) -> Self {
        Self {
            frame,
            handle,
            start,
            editable,
            pivot,
            resize: None,
            primitive: String::new(),
            note: None,
            scope: None,
            called_here: None,
            received,
            sites,
            turned: 0.0,
            values: Vec::new(),
            released: false,
        }
    }

    /// The drag with the Size tool's `resize` (`None`: the Scale tool).
    pub(crate) fn with_resize(self, resize: Option<Resize>) -> Self {
        Self { resize, ..self }
    }

    /// The drag of the element `primitive` names (see `primitive_of`).
    pub(crate) fn with_primitive(self, primitive: &str) -> Self {
        Self {
            primitive: primitive.to_owned(),
            ..self
        }
    }

    /// The drag with the reason Center is not in effect, kept for a handle
    /// that turns or scales about the pivot (see `turns_about_pivot`).
    pub(crate) fn with_note(self, note: Option<&'static str>) -> Self {
        let note = note.filter(|_| turns_about_pivot(self.handle, self.resize.as_ref()));
        Self { note, ..self }
    }

    /// The drag of an element whose edits write under `scope`, in a scene
    /// whose calls of its component are `called_here` (see `called_here`).
    pub(crate) fn with_scope(self, scope: Option<Scope>, called_here: Option<CalledHere>) -> Self {
        Self {
            scope,
            called_here,
            ..self
        }
    }

    /// What the drag's edit changes (see `scope_in_effect`); `None` without
    /// a scope.
    pub(crate) fn in_effect(&self) -> Option<String> {
        self.scope
            .map(|scope| scope_in_effect(scope, self.called_here.as_ref()))
    }

    /// The label beside the layer: the props the drag writes (`x 120 →
    /// 184`), after the note why it turns about the pivot (see
    /// `with_note`), and under them what the edit changes (see
    /// `with_scope`); `None` while it has neither props nor note.
    pub(crate) fn label(&self) -> Option<String> {
        let written = drag_label(
            &self.frame,
            &self.values,
            &self.received,
            self.resize.as_ref(),
        );
        let note = self.note.map(pivot_note_text);
        let line = match (note, written.is_empty()) {
            (None, true) => None,
            (None, false) => Some(written),
            (Some(note), true) => Some(note),
            (Some(note), false) => Some(format!("{note}  {written}")),
        }?;
        Some(match self.in_effect() {
            Some(in_effect) => format!("{line}\n{in_effect}"),
            None => line,
        })
    }

    /// `frame` (drawn with the drag's overrides) with the box its element's
    /// anchor refers to, built from the props the drag draws (see
    /// `received_now`), so the handles follow the box, also while a
    /// released drag is written.
    pub(crate) fn anchored(&self, frame: LayerFrame) -> LayerFrame {
        with_anchor_box(frame, &self.primitive, &self.received_now())
    }

    /// The frame the overlay (outline, handles, stem) is drawn from while
    /// this drag runs or is written, given the one the preview drew
    /// (`drawn`). An anchor drag keeps its start frame: the anchor is a
    /// reference point and the layer doesn't move, while preview frames,
    /// rounded as written and lagging the pointer, would jitter. Any other
    /// drag follows the drawn frame (see `anchored`).
    pub(crate) fn overlay_frame(&self, drawn: LayerFrame) -> LayerFrame {
        if self.holds_overlay() {
            self.frame.clone()
        } else {
            self.anchored(drawn)
        }
    }

    /// Whether the overlay stays on the start frame (an anchor drag; see
    /// `overlay_frame`), the selection outline too.
    pub(crate) fn holds_overlay(&self) -> bool {
        self.handle == Handle::Anchor
    }

    /// Where the cross sits during an anchor drag (scene pixels): the pivot
    /// of the start frame moved to the drag's `x`/`y`, which keep the box
    /// still for the anchor as written (the anchor is not part of the map).
    /// `None` for any other drag, and before the pointer moves.
    pub(crate) fn anchor_cross(&self) -> Option<Point> {
        if self.handle != Handle::Anchor {
            return None;
        }
        let value = |prop: TransformProp| {
            self.values
                .iter()
                .find(|(key, _)| *key == prop.key())
                .map(|(_, value)| *value)
        };
        let mut after = self.frame.transform;
        after.position = Point {
            x: value(TransformProp::X)?,
            y: value(TransformProp::Y)?,
        };
        Some(apply(&self.frame.parent, pivot(&after)))
    }

    /// The props the runtime sees with the drag's overrides: those it saw
    /// at the start, with every prop the preview overrides at its value now
    /// (a shape's box follows a running resize; see `with_anchor_box`).
    pub(crate) fn received_now(&self) -> ReceivedProps {
        let mut received = self.received.clone();
        if self.values.is_empty() {
            return received;
        }
        let values = preview_values(
            &self.frame,
            self.handle,
            &self.editable,
            &self.values,
            &self.received,
            self.pivot,
            self.resize.as_ref(),
        );
        for (key, value) in values {
            let PropKey::Prop(name) = key else {
                continue;
            };
            let field = match name.as_str() {
                "x" => &mut received.x,
                "y" => &mut received.y,
                "rotation" => &mut received.rotation,
                "scale" => &mut received.scale,
                "scaleX" => &mut received.scale_x,
                "scaleY" => &mut received.scale_y,
                _ => {
                    received.others.insert(name, ReceivedValue::Number(value));
                    continue;
                }
            };
            *field = Some(value);
        }
        received
    }

    /// Takes the values the pointer made; true when they changed.
    pub(crate) fn follow(&mut self, values: Vec<(PropKey, f64)>) -> bool {
        let changed = values != self.values;
        self.values = values;
        changed
    }

    /// The prop overrides the preview evaluates the scene with: the values
    /// the write puts into the source, so the frame shows what the write will
    /// produce. None before the pointer moves, nor while `playing`. From the
    /// first move on, every prop the handle can change is overridden, changed
    /// or not, so the positions stay the same for the whole drag: the runtime
    /// remounts the entry when they change, which is then only when the drag
    /// starts and when it is cancelled.
    pub(crate) fn overrides(&self, playing: bool) -> Vec<PropOverride> {
        if playing || self.values.is_empty() {
            return Vec::new();
        }
        prop_overrides(
            &self.sites,
            &preview_values(
                &self.frame,
                self.handle,
                &self.editable,
                &self.values,
                &self.received,
                self.pivot,
                self.resize.as_ref(),
            ),
        )
    }
}

/// The prop `prop` of the elements written at `position`: where the runtime
/// takes a dragged or scrubbed value while the pointer moves.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct OverrideSite {
    position: SourcePos,
    prop: String,
}

impl OverrideSite {
    /// The override drawing `value` here.
    pub(crate) fn with(&self, value: f64) -> PropOverride {
        PropOverride {
            file: self.position.file.clone(),
            line: self.position.line,
            column: self.position.column,
            prop: self.prop.clone(),
            value,
        }
    }
}

/// Where each editable row and component prop of `targets` takes a value
/// the pointer makes: the prop at the row's resolved path on the element
/// (`report`'s innermost entry; `style.fontSize` for a `Text` font row the
/// `style` object sets) for a value written there or absent, the
/// parameter's prop at the call site for a value passed through, which also
/// moves everything else computed from it, and a component prop at the
/// component's call. The same prop the write changes. Inputs have none: a
/// value an expression reads is not a prop.
pub(crate) fn override_sites(
    targets: &ElementTargets,
    report: &LayerSourceReport,
) -> Vec<(PropKey, OverrideSite)> {
    let element = report.chain.first().map(SourcePos::from);
    let component = targets
        .component
        .as_ref()
        .map(|component| &component.call_site);
    targets
        .entries()
        .into_iter()
        .filter(|(_, target)| editable(&target.target))
        .filter_map(|(key, target)| {
            // Where a value not passed through is written.
            let written = match &key {
                PropKey::Prop(_) => element.as_ref(),
                PropKey::CallSite(_) => component,
                PropKey::Input { .. } => return None,
            };
            let site = match (&target.target, &target.call_site) {
                (EditTarget::PassThrough { param, .. }, Some(call_site)) => OverrideSite {
                    position: call_site.clone(),
                    prop: param.clone(),
                },
                (EditTarget::PassThrough { .. }, None) => return None,
                _ => OverrideSite {
                    position: written?.clone(),
                    prop: target.path.clone(),
                },
            };
            Some((key, site))
        })
        .collect()
}

/// The overrides that draw the drag values `values` (see `prop_values`).
pub(crate) fn prop_overrides(
    sites: &[(PropKey, OverrideSite)],
    values: &[(PropKey, f64)],
) -> Vec<PropOverride> {
    values
        .iter()
        .filter_map(|(key, value)| {
            let (_, site) = sites.iter().find(|(candidate, _)| candidate == key)?;
            Some(site.with(*value))
        })
        .collect()
}

/// The note shown at the rotation and scale handles, and in a drag's
/// label, when Center is chosen but `reason` keeps the layer turning about
/// its pivot.
pub(crate) fn pivot_note_text(reason: &str) -> String {
    format!("Pivot — {reason}")
}

/// Whether `handle` turns or scales the layer about the point the pivot
/// mode names: the rotation handle, and the corners and edges of the Scale
/// tool (`resize` is `None`).
pub(crate) fn turns_about_pivot(handle: Handle, resize: Option<&Resize>) -> bool {
    match handle {
        Handle::Rotate => true,
        Handle::Corner(_) | Handle::Edge(_) => resize.is_none(),
        Handle::Move | Handle::Anchor => false,
    }
}

/// What the canvas can do with the selected layer.
pub(crate) enum DragState {
    /// No frame or no targets yet: the outline only.
    Unknown,
    /// The layer has no affine frame.
    NoFrame,
    /// Every prop is locked; the first reason.
    Locked(LayerFrame, String),
    /// Draggable with these props, rotating and scaling about what the
    /// `PivotMode` names (the mode used, see `pivot_used`), and resizing
    /// with the Size tool when it is used (see `resize_used`).
    Editable(LayerFrame, Vec<PropKey>, PivotMode, Option<Resize>),
}

/// The handles and cross shown: none while editing (the outline stays).
pub(crate) fn editing_drag_state(state: DragState, editing: bool) -> DragState {
    if editing { DragState::Unknown } else { state }
}

/// The props that hold the element's box (its table's `size`), each with
/// its field's bounds, when every one is editable (a literal, a keyframe,
/// or absent); `None` otherwise, and the Size tool falls back to Scale.
pub(crate) fn size_props(targets: &ElementTargets) -> Option<SizeProps> {
    let table = targets.table.as_ref()?;
    let size = table.size.as_ref()?;
    let field = |name: &String| {
        let key = PropKey::Prop(name.clone());
        let target = targets.target(&key).filter(|target| editable(target))?;
        let absent = matches!(inner_target(target), EditTarget::Missing { .. });
        let keyframed = matches!(inner_target(target), EditTarget::Keyframe { .. });
        let (min, max, integer) = match field_of(table, name) {
            Some(PrimitivePropertyField::Number {
                min, max, integer, ..
            }) => (*min, *max, integer.unwrap_or(false)),
            _ => (None, None, false),
        };
        Some(SizeField {
            name: name.clone(),
            min,
            max,
            integer,
            absent,
            keyframed,
        })
    };
    match (&size.width, &size.height, &size.radius) {
        (_, _, Some(radius)) => Some(SizeProps::Radius(field(radius)?)),
        (Some(width), Some(height), None) => Some(SizeProps::Box {
            width: field(width)?,
            height: field(height)?,
        }),
        (Some(width), None, None) => Some(SizeProps::Width(field(width)?)),
        (None, _, None) => None,
    }
}

/// Why a drag of the layer `frame` measures can't use the Size tool.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ResizeFallback {
    /// The element has no size props (a `<Group>`).
    NoSize,
    /// A size prop is locked: the row and its reason.
    Locked(String),
    /// A size prop has no row the editor can write, without a lock to name.
    Uneditable,
    /// Every corner and edge moves the anchor point, and `x` or `y` can't
    /// compensate (a keyframe, or locked).
    Pinned,
    /// Every corner and edge moves the anchor point, and something the
    /// compensation uses is a keyframe (see `resizes`): the reason, one of
    /// [`TURN_ANIMATED`], [`ANCHOR_ANIMATED`], [`SIZE_ANIMATED`].
    Animated(&'static str),
    /// The layer turns in its own perspective: no flat box to resize.
    NoBox,
}

impl ResizeFallback {
    /// The toggle's tooltip for it.
    pub(crate) fn reason(&self) -> String {
        match self {
            Self::NoSize => "No size to resize".to_owned(),
            Self::Locked(reason) => reason.clone(),
            Self::Uneditable => "Size can't be edited here".to_owned(),
            Self::Pinned => "x or y can't move".to_owned(),
            Self::Animated(reason) => (*reason).to_owned(),
            Self::NoBox => "No flat box to resize".to_owned(),
        }
    }
}

/// Why a compensation is refused: `x`/`y` written as literals beside a
/// value that changes between keyframes would hold at that keyframe only,
/// and the layer would drift on the others.
pub(crate) const TURN_ANIMATED: &str = "Rotation or scale is animated";
pub(crate) const SIZE_ANIMATED: &str = "Size is animated";
pub(crate) const ANCHOR_ANIMATED: &str = "Anchor is animated";
/// The box's center moves with its size and its anchor.
pub(crate) const BOX_ANIMATED: &str = "Size or anchor is animated";
/// The cross's tooltip when the anchor has no box to be a fraction of.
pub(crate) const NO_ANCHOR_BOX: &str = "No box to anchor in";

/// Why a drag of the layer `frame` measures can't use `choice`: the Size
/// tool needs editable size props (see `size_props`), the box they describe
/// (from the props the runtime reported, for a shape), and a corner or edge
/// that can resize it (see `resizes_any`). `None` when it can.
pub(crate) fn resize_fallback(
    choice: ResizeTool,
    targets: &ElementTargets,
    frame: &LayerFrame,
) -> Option<ResizeFallback> {
    if choice == ResizeTool::Scale {
        return None;
    }
    if let Some(props) = size_props(targets) {
        if anchor_box(frame).is_none() {
            // Drawn flat, a layer without a box is one whose size the
            // runtime didn't report.
            return Some(if local_box(frame).is_some() {
                ResizeFallback::NoSize
            } else {
                ResizeFallback::NoBox
            });
        }
        let resize = Resize {
            props,
            compensable: compensable(targets),
            animated: resize_animated(targets),
        };
        return (!resizes_any(frame, &resize)).then(|| {
            let animated = [
                (turns_animated(targets), TURN_ANIMATED),
                (anchors_animated(targets), ANCHOR_ANIMATED),
                (size_animated(&resize), SIZE_ANIMATED),
            ];
            match animated.into_iter().find(|(animated, _)| *animated) {
                Some((_, reason)) if resize.compensable => ResizeFallback::Animated(reason),
                _ => ResizeFallback::Pinned,
            }
        });
    }
    let size = targets.table.as_ref().and_then(|table| table.size.as_ref());
    let names: Vec<&String> = size
        .into_iter()
        .flat_map(|size| [&size.width, &size.height, &size.radius])
        .flatten()
        .collect();
    if names.is_empty() {
        return Some(ResizeFallback::NoSize);
    }
    let locked = names.into_iter().find_map(|name| {
        let key = PropKey::Prop(name.clone());
        let EditTarget::Locked(reason) = inner_target(targets.target(&key)?) else {
            return None;
        };
        Some(format!("{}: {}", key.label(), reason_text(reason)))
    });
    Some(locked.map_or(ResizeFallback::Uneditable, ResizeFallback::Locked))
}

/// The Size tool a drag of the layer `frame` measures uses when `choice` is
/// chosen; `None` (the Scale tool) when it falls back (see
/// `resize_fallback`) or Scale is chosen.
pub(crate) fn resize_used(
    choice: ResizeTool,
    targets: &ElementTargets,
    frame: &LayerFrame,
) -> Option<Resize> {
    if choice == ResizeTool::Scale || resize_fallback(choice, targets, frame).is_some() {
        return None;
    }
    Some(Resize {
        props: size_props(targets)?,
        compensable: compensable(targets),
        animated: resize_animated(targets),
    })
}

/// Whether a resize that moves the anchor point would compensate with a
/// value that changes between keyframes: the compensation turns and scales
/// with `rotation` and the scales, and is measured from the anchor.
fn resize_animated(targets: &ElementTargets) -> bool {
    turns_animated(targets) || anchors_animated(targets)
}

/// Whether `anchorX` or `anchorY` is a keyframe.
fn anchors_animated(targets: &ElementTargets) -> bool {
    anchor_keys()
        .iter()
        .any(|key| keyframed(targets.target(key)))
}

/// Whether `target` is a keyframe (directly or through a pass-through).
fn keyframed(target: Option<&EditTarget>) -> bool {
    matches!(target.map(inner_target), Some(EditTarget::Keyframe { .. }))
}

/// Whether `x` and `y` can absorb a compensation: `Literal` or `Missing`
/// (directly or through a pass-through), never `Keyframe` or locked. A
/// keyframe would move at that keyframe only, and the layer would orbit
/// between them.
pub(crate) fn compensable(targets: &ElementTargets) -> bool {
    [TransformProp::X, TransformProp::Y]
        .into_iter()
        .all(|prop| settable(targets.transform(prop)))
}

/// Whether `target` holds one value for every frame: `Literal` or
/// `Missing` (directly or through a pass-through).
fn settable(target: Option<&EditTarget>) -> bool {
    matches!(
        target.map(inner_target),
        Some(EditTarget::Literal { .. } | EditTarget::Missing { .. })
    )
}

/// Whether the layer's `rotation` or a scale is a keyframe: `x`/`y`
/// compensating a change of it (or of anything it turns or scales) would
/// hold at that keyframe only.
fn turns_animated(targets: &ElementTargets) -> bool {
    [
        TransformProp::Rotation,
        TransformProp::Scale,
        TransformProp::ScaleX,
        TransformProp::ScaleY,
    ]
    .into_iter()
    .any(|prop| keyframed(targets.transform(prop)))
}

/// Why a drag with Center chosen turns about the pivot instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PivotFallback {
    /// `x` or `y` is a keyframe or locked: nothing can keep the center.
    NotCompensable,
    /// `rotation` or a scale is a keyframe: literal `x`/`y` written beside
    /// it would keep the center at this keyframe only.
    Animated,
    /// A size prop or an anchor is a keyframe: the center, a point of the
    /// box, is another one on the other keyframes.
    AnimatedBox,
    /// The layer turns in its own perspective: no flat box to center on.
    NoFlatBox,
}

impl PivotFallback {
    /// The toggle's tooltip for it, and the note beside the handles.
    pub(crate) fn reason(self) -> &'static str {
        match self {
            Self::NotCompensable => "x or y can't be edited",
            Self::Animated => TURN_ANIMATED,
            Self::AnimatedBox => BOX_ANIMATED,
            Self::NoFlatBox => "No flat box to center on",
        }
    }
}

/// Why a drag of the layer `frame` measures can't use `choice`: Center needs
/// `x`/`y` that can compensate, a rotation, scale, size and anchor that hold
/// for every frame, and a known box. `None` when it can.
pub(crate) fn pivot_fallback(
    choice: PivotMode,
    targets: &ElementTargets,
    frame: &LayerFrame,
) -> Option<PivotFallback> {
    match choice {
        PivotMode::Pivot => None,
        PivotMode::Center if !compensable(targets) => Some(PivotFallback::NotCompensable),
        PivotMode::Center if turns_animated(targets) => Some(PivotFallback::Animated),
        PivotMode::Center if sizes_animated(targets) || anchors_animated(targets) => {
            Some(PivotFallback::AnimatedBox)
        }
        PivotMode::Center if local_box(frame).is_none() => Some(PivotFallback::NoFlatBox),
        PivotMode::Center => None,
    }
}

/// The mode a drag of the layer `frame` measures uses when `choice` is
/// chosen: the pivot when Center falls back (see `pivot_fallback`).
pub(crate) fn pivot_used(
    choice: PivotMode,
    targets: &ElementTargets,
    frame: &LayerFrame,
) -> PivotMode {
    match pivot_fallback(choice, targets, frame) {
        Some(_) => PivotMode::Pivot,
        None => choice,
    }
}

/// The props a canvas drag of the layer `frame` measures may change: its
/// editable transform props, in row order, then `anchorX`/`anchorY` when
/// nothing keeps the anchor (see `anchor_blocker`).
pub(crate) fn drag_props(targets: &ElementTargets, frame: &LayerFrame) -> Vec<PropKey> {
    let mut props: Vec<PropKey> = TransformProp::ALL
        .into_iter()
        .filter(|prop| targets.transform(*prop).is_some_and(editable))
        .map(TransformProp::key)
        .collect();
    if anchor_blocker(targets, frame).is_none() {
        props.extend(anchor_keys());
    }
    props
}

/// [`drag_props`] for a drag in the mode `pivot` (the one used, see
/// `pivot_used`): in Center mode the cross marks the box's middle and moves
/// no anchor, so the anchors are left out.
/// `props` and the Size tool's `resize` without what the release would
/// refuse (`refusal` of a key, see `EditorView::gesture_refusals`): a
/// refused prop is not dragged, and the size handles are not offered when
/// any size prop is refused. The first refusal, for a drag left with
/// nothing to change.
pub(crate) fn without_refused(
    props: Vec<PropKey>,
    resize: Option<Resize>,
    refusal: impl Fn(&PropKey) -> Option<String>,
) -> (Vec<PropKey>, Option<Resize>, Option<String>) {
    let mut first = None;
    let mut refused = |key: &PropKey| match refusal(key) {
        Some(reason) => {
            first.get_or_insert(reason);
            true
        }
        None => false,
    };
    let props = props.into_iter().filter(|key| !refused(key)).collect();
    let resize = resize.filter(|resize| !resize.keys().iter().any(&mut refused));
    (props, resize, first)
}

pub(crate) fn drag_props_in(
    targets: &ElementTargets,
    frame: &LayerFrame,
    pivot: PivotMode,
) -> Vec<PropKey> {
    let mut props = drag_props(targets, frame);
    if pivot == PivotMode::Center {
        let anchors = anchor_keys();
        props.retain(|key| !anchors.contains(key));
    }
    props
}

/// The cross's tooltip in Center mode, where it marks the box's middle.
pub(crate) const CENTER_MARKER: &str =
    "Rotates about the center; switch to Pivot to move the anchor";

/// The center cross's tooltip: [`CENTER_MARKER`]; when the anchor couldn't
/// move in Pivot mode either (`blocked`, see `anchor_reason`), that reason
/// instead of the promise; and only "Rotates about the center" for an
/// element without anchor rows (`has_anchor` false: a `<Path>`), which has
/// no anchor to move.
pub(crate) fn center_marker_text(has_anchor: bool, blocked: Option<String>) -> String {
    match (has_anchor, blocked) {
        (false, _) => "Rotates about the center".to_owned(),
        (true, None) => CENTER_MARKER.to_owned(),
        (true, Some(reason)) => {
            format!("Rotates about the center; the anchor can't move: {reason}")
        }
    }
}

/// Whether the element has anchor rows (`anchorX`/`anchorY`), so its cross
/// can tell about its anchor; a `<Path>` has none.
pub(crate) fn has_anchor_rows(targets: &ElementTargets) -> bool {
    anchor_keys()
        .iter()
        .any(|key| targets.target(key).is_some())
}

/// Whether a press at `point` (scene pixels, within `reach` of the cross the
/// canvas draws for `pivot`, see `marker`) with `alt` held is taken and
/// starts nothing: Alt there means the anchor, and when no anchor drag is
/// offered but the cross says why (Center's marker, always; a pivot cross
/// with a `reason`, see `anchor_reason`), falling through to the body would
/// move the layer under the not-allowed cursor. A pivot cross that shows
/// nothing (an element without anchor rows) lets the press through.
pub(crate) fn swallows_alt_press(
    frame: &LayerFrame,
    editable: &[PropKey],
    pivot: PivotMode,
    point: Point,
    reach: f64,
    alt: bool,
    reason: bool,
) -> bool {
    let marked = marker(frame, pivot);
    let at = marked.at();
    let shows = matches!(marked, Marker::Center(_)) || reason;
    alt && shows
        && (at.x - point.x).hypot(at.y - point.y) <= reach
        && anchor_handle(frame, editable).is_none()
}

/// Whether a size prop of the element (its table's `size`) is a keyframe:
/// its box is another on the other keyframes.
fn sizes_animated(targets: &ElementTargets) -> bool {
    let size = targets.table.as_ref().and_then(|table| table.size.as_ref());
    size.into_iter()
        .flat_map(|size| [&size.width, &size.height, &size.radius])
        .flatten()
        .any(|name| keyframed(targets.target(&PropKey::Prop(name.clone()))))
}

/// Why `prop` (`x`, `y`, an anchor) can't take one value for every frame:
/// its row's lock reason ("X: Comes from DX, which can't be traced"), that
/// it is a keyframe, or that it has no row. `None` when it can.
fn unsettable(targets: &ElementTargets, key: &PropKey, animated: &str) -> Option<String> {
    let target = targets.target(key);
    if settable(target) {
        return None;
    }
    Some(match target.map(inner_target) {
        Some(EditTarget::Locked(reason)) => format!("{}: {}", key.label(), reason_text(reason)),
        Some(EditTarget::Keyframe { .. }) => animated.to_owned(),
        _ => format!("{} can't be edited", key.label()),
    })
}

/// Why an Alt-drag of the cross can't move the layer's anchor, the first of:
/// an anchor that can't take one value for every frame (locked, as
/// `anchorY="baseline"` on a `Text`, or a keyframe); `x` or `y` that can't
/// compensate, with its own reason; a keyframed `rotation` or scale (the
/// compensation turns and scales with them) or size (it is the anchor's
/// change times the box); no box the anchor refers to (`with_anchor_box`;
/// none for a `<Group>`, a projective layer, or a shape whose size the
/// runtime didn't report). `None` when the drag is offered.
fn anchor_blocker(targets: &ElementTargets, frame: &LayerFrame) -> Option<String> {
    let anchors = anchor_keys()
        .into_iter()
        .find_map(|key| unsettable(targets, &key, ANCHOR_ANIMATED));
    let position = [TransformProp::X, TransformProp::Y]
        .into_iter()
        .find_map(|prop| {
            let key = prop.key();
            unsettable(targets, &key, &format!("{} is animated", key.label()))
        });
    anchors
        .or(position)
        .or_else(|| turns_animated(targets).then(|| TURN_ANIMATED.to_owned()))
        .or_else(|| sizes_animated(targets).then(|| SIZE_ANIMATED.to_owned()))
        .or_else(|| {
            anchor_box(frame)
                .is_none()
                .then(|| NO_ANCHOR_BOX.to_owned())
        })
}

pub(crate) const GROUP_ANCHOR: &str = "A group's anchor has no effect";

/// Why the pivot cross of the element `primitive` names can't be
/// Alt-dragged (see `anchor_blocker`), its tooltip; a `<Group>`'s anchor has
/// no effect at all (the renderer places its children by its position
/// alone). `None` when it can, and for an element without anchor rows
/// (a `<Path>`), which has no anchor to move.
pub(crate) fn anchor_reason(
    targets: &ElementTargets,
    frame: &LayerFrame,
    primitive: &str,
) -> Option<String> {
    let anchors = anchor_keys();
    if !has_anchor_rows(targets) {
        return None;
    }
    let locked = anchors.iter().find_map(|key| {
        let EditTarget::Locked(reason) = inner_target(targets.target(key)?) else {
            return None;
        };
        Some(format!("{}: {}", key.label(), reason_text(reason)))
    });
    locked
        .or_else(|| (primitive == "Group").then(|| GROUP_ANCHOR.to_owned()))
        .or_else(|| anchor_blocker(targets, frame))
}

/// The name of the element `report` found (its innermost entry), which
/// says how its anchor is laid out (see `with_anchor_box`).
fn primitive_of(report: &LayerSourceReport) -> &str {
    report
        .chain
        .first()
        .map_or("", |element| element.name.as_str())
}

impl EditorView {
    /// The selected layer's measured frame, when it is the selected one.
    pub(crate) fn selected_frame(&self) -> Option<Option<&LayerFrame>> {
        let selected = self.selected_layer.as_deref()?;
        self.layer_frame
            .as_ref()
            .filter(|(id, _)| id == selected)
            .map(|(_, frame)| frame.as_ref())
    }

    /// What dragging the selected layer can change, for the canvas; nothing
    /// while its text is edited there.
    pub(crate) fn drag_state(&self) -> DragState {
        editing_drag_state(self.transform_state(), self.inline_text.is_some())
    }

    fn transform_state(&self) -> DragState {
        if !self.picks_layers() || self.playing {
            return DragState::Unknown;
        }
        let frame = match self.selected_frame() {
            None => return DragState::Unknown,
            Some(None) => return DragState::NoFrame,
            Some(Some(frame)) => frame.clone(),
        };
        let resolved = self.element_targets();
        // The box the source element's anchor refers to, which the drag
        // keeps with it; while one runs (or is written), the frame the
        // overlay is drawn from (see `TransformDrag::overlay_frame`).
        let frame = match (&self.transform_drag, resolved) {
            (Some(drag), _) => drag.overlay_frame(frame),
            (None, Some((_, report))) => {
                with_anchor_box(frame, primitive_of(report), &report.props)
            }
            (None, None) => frame,
        };
        // While dragging, the props stay those the drag began with; the
        // frame is the one drawn with the overrides, so the handles follow.
        if let Some(drag) = &self.transform_drag {
            return DragState::Editable(
                frame,
                drag.editable.clone(),
                drag.pivot,
                drag.resize.clone(),
            );
        }
        let Some((targets, _)) = resolved else {
            return DragState::Unknown;
        };
        let resize = resize_used(self.resize_tool, targets, &frame);
        let pivot = pivot_used(self.pivot_mode, targets, &frame);
        let props = drag_props_in(targets, &frame, pivot);
        // A prop the release would refuse (the scope the drag keeps can't
        // write it, or would not show it in this scene, or the scenes are
        // being scanned) is not dragged, nor resized.
        let keys: Vec<PropKey> = props
            .iter()
            .cloned()
            .chain(resize.iter().flat_map(Resize::keys))
            .collect();
        let refusals: HashMap<PropKey, String> = keys
            .iter()
            .cloned()
            .zip(self.gesture_refusals(&keys))
            .filter_map(|(key, refusal)| Some((key, refusal?)))
            .collect();
        let (mut props, resize, scope_lock) =
            without_refused(props, resize, |key| refusals.get(key).cloned());
        props.extend(resize.iter().flat_map(Resize::keys));
        if props.is_empty() {
            let reason = TransformProp::ALL
                .into_iter()
                .filter_map(|prop| targets.transform(prop))
                .find_map(|target| match inner_target(target) {
                    EditTarget::Locked(reason) => Some(reason_text(reason)),
                    _ => None,
                })
                .or(scope_lock)
                .unwrap_or_default();
            return DragState::Locked(frame, reason);
        }
        DragState::Editable(frame, props, pivot, resize)
    }

    /// Why the chosen Size tool is not in effect on the selected layer (its
    /// handles scale instead), for the toggle's warning; `None` when it is,
    /// or Scale is chosen.
    pub(crate) fn resize_note(&self) -> Option<String> {
        if self.resize_tool != ResizeTool::Size {
            return None;
        }
        let DragState::Editable(frame, _, _, None) = self.drag_state() else {
            return None;
        };
        let (targets, _) = self.element_targets()?;
        resize_fallback(ResizeTool::Size, targets, &frame).map(|fallback| fallback.reason())
    }

    /// Why the chosen Center is not in effect on the selected layer (its
    /// rotation and scale turn about the pivot), for the toggle's warning and
    /// the note at the rotation and scale handles; `None` when it is, or
    /// Pivot is chosen.
    pub(crate) fn pivot_note(&self) -> Option<&'static str> {
        if self.pivot_mode != PivotMode::Center {
            return None;
        }
        let DragState::Editable(frame, _, PivotMode::Pivot, _) = self.drag_state() else {
            return None;
        };
        let (targets, _) = self.element_targets()?;
        pivot_fallback(PivotMode::Center, targets, &frame).map(PivotFallback::reason)
    }

    /// Why the selected layer's pivot cross can't move its anchor (see
    /// `anchor_reason`), for the layer `frame` measures (see `drag_state`).
    pub(crate) fn anchor_lock_reason(&self, frame: &LayerFrame) -> Option<String> {
        let (targets, report) = self.element_targets()?;
        anchor_reason(targets, frame, primitive_of(report))
    }

    /// The center cross's tooltip for the selected layer (see
    /// `center_marker_text`).
    pub(crate) fn center_cross_text(&self, frame: &LayerFrame) -> String {
        let has_anchor = self
            .element_targets()
            .is_some_and(|(targets, _)| has_anchor_rows(targets));
        center_marker_text(has_anchor, self.anchor_lock_reason(frame))
    }

    /// The prop overrides the preview evaluates the scene with while a drag
    /// or a row's scrub runs (see `TransformDrag::overrides`,
    /// `Scrub::overrides`); the two never run together.
    pub(crate) fn prop_overrides(&self) -> Vec<PropOverride> {
        let drag = self
            .transform_drag
            .iter()
            .flat_map(|drag| drag.overrides(self.playing));
        let scrub = self
            .element_rows
            .scrub
            .iter()
            .flat_map(|scrub| scrub.overrides(self.playing));
        drag.chain(scrub).collect()
    }

    /// The running drag's label (see `TransformDrag::label`).
    pub(crate) fn transform_drag_label(&self) -> Option<String> {
        self.transform_drag.as_ref()?.label()
    }

    /// What the running drag's edit changes under the scope it writes with,
    /// for the Properties header (see `TransformDrag::in_effect`).
    pub(crate) fn transform_drag_in_effect(&self) -> Option<String> {
        self.transform_drag.as_ref()?.in_effect()
    }

    /// Whether a drag is running with the button down (not one released and
    /// being written).
    pub(crate) fn transform_drag_active(&self) -> bool {
        self.transform_drag
            .as_ref()
            .is_some_and(|drag| !drag.released)
    }

    /// Whether a released drag is being written, or waits for the reload
    /// that draws its values.
    pub(crate) fn transform_drag_released(&self) -> bool {
        self.transform_drag
            .as_ref()
            .is_some_and(|drag| drag.released)
    }

    /// Starts a drag when `position` (window pixels) is on a handle of the
    /// selected layer, on its pivot cross with `alt` held, or on its body
    /// with the layer topmost there. True when a drag started, and when the
    /// press is taken without one (Alt on a cross that moves nothing, see
    /// `swallows_alt_press`): the selection doesn't change either.
    pub(crate) fn begin_transform_drag(
        &mut self,
        position: gpui_kit::Point<gpui_kit::Pixels>,
        topmost: Option<&str>,
        alt: bool,
    ) -> bool {
        // Only a pressed drag takes the click; while a released one is being
        // written, clicks select as usual (and no second drag starts).
        if let Some(drag) = &self.transform_drag {
            return !drag.released;
        }
        // Nor while a released scrub is: its override stays until the reload
        // draws the written value.
        if self.row_scrub_released() {
            return false;
        }
        let DragState::Editable(frame, editable, pivot, resize) = self.drag_state() else {
            return false;
        };
        let Some((received, sites, primitive)) = self.element_targets().map(|(targets, report)| {
            (
                report.props.clone(),
                override_sites(targets, report),
                primitive_of(report).to_owned(),
            )
        }) else {
            return false;
        };
        let Some(bounds) = self.viewer_bounds.get() else {
            return false;
        };
        let size = self.frame_size();
        let (Some((_, _, scale)), Some(start)) =
            (letterbox(bounds, size), to_scene(bounds, size, position))
        else {
            return false;
        };
        let scale = f64::from(scale);
        let shown = shown_handles(
            &frame,
            &editable,
            f64::from(ROTATE_STEM) / scale,
            pivot,
            resize.as_ref(),
        );
        let reach = f64::from(HANDLE_REACH) / scale;
        // Alt on a cross that moves nothing and says why: take the press,
        // start nothing.
        let reason = self.anchor_lock_reason(&frame).is_some();
        if swallows_alt_press(&frame, &editable, pivot, start, reach, alt, reason) {
            return true;
        }
        let handle = match pressed_handle(&frame, &shown, &editable, start, reach, alt) {
            Some(Handle::Move)
                if presses_body(&frame, topmost)
                    && (editable.contains(&TransformProp::X.key())
                        || editable.contains(&TransformProp::Y.key())) =>
            {
                Handle::Move
            }
            Some(Handle::Move) | None => return false,
            Some(handle) => handle,
        };
        // A pressed scrub ends unwritten: the two never run together.
        if self.row_scrub_active() {
            self.cancel_row_scrub();
        }
        let note = self.pivot_note();
        let scope = self.kept_scope();
        let called_here = self
            .shown_scope_context()
            .and_then(|context| match context.state {
                ScopeState::Options(options) => options.called_here,
                ScopeState::NotShared | ScopeState::Unknown { .. } => None,
            });
        self.transform_drag = Some(
            TransformDrag::new(frame, handle, start, editable, pivot, received, sites)
                .with_resize(resize)
                .with_primitive(&primitive)
                .with_note(note)
                .with_scope(scope, called_here),
        );
        true
    }

    /// Follows the pointer: recomputes the values and redraws the frame with
    /// them. Writes nothing.
    pub(crate) fn continue_transform_drag(
        &mut self,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) {
        // The button came up where the view did not see it: end there.
        if !event.dragging() {
            if self.end_transform_drag(cx) {
                cx.notify();
            }
            return;
        }
        let Some(bounds) = self.viewer_bounds.get() else {
            return;
        };
        let size = self.frame_size();
        let Some(drag) = self.transform_drag.as_mut().filter(|drag| !drag.released) else {
            return;
        };
        let Some(now) = to_scene(bounds, size, event.position) else {
            return;
        };
        let values = drag_values(
            &drag.frame,
            drag.handle,
            (drag.start, now),
            event.modifiers.shift,
            drag.turned,
            &drag.editable,
            drag.pivot,
            drag.resize.as_ref(),
        );
        if drag.handle == Handle::Rotate
            && let Some(turned) = turn(&drag.frame, drag.pivot, drag.start, now, drag.turned)
        {
            drag.turned = turned;
        }
        if drag.follow(values) {
            self.refresh_preview();
            cx.notify();
        }
    }

    /// Ends the drag: writes its values as one undo entry. True when a drag
    /// was running.
    pub(crate) fn end_transform_drag(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(drag) = self.transform_drag.as_mut().filter(|drag| !drag.released) else {
            return false;
        };
        // Back where it started (or never moved): nothing to write, and no
        // history entry.
        let values = prop_values(
            &drag.frame,
            &drag.values,
            &drag.received,
            drag.resize.as_ref(),
        );
        if values.is_empty() {
            self.cancel_transform_drag();
            return true;
        }
        drag.released = true;
        let drag_scope = drag.scope;
        let refuse = |this: &mut Self, message: String| {
            this.element_edit
                .set_error(this.selected_layer.as_deref(), Some(message));
            this.cancel_transform_drag();
            true
        };
        if self.element_targets().is_none() {
            // The targets went stale mid-drag (a reload); nothing to write to.
            let message = self
                .element_edit
                .unavailable(self.selected_layer.as_deref());
            return refuse(self, message);
        }
        // Written under the scope the drag started with (its label said
        // it), never held and never under another (see `gesture_gate`):
        // refused with the note while the scenes are scanned, with the lock
        // when that scope can't write a prop by now (a sweep that landed
        // mid-drag, say), and when the drag started with no scope shown
        // but the scan has since found one.
        let keys: Vec<PropKey> = values.iter().map(|(key, _)| key.clone()).collect();
        let scope = match self.gesture_gate_for(drag_scope, &keys) {
            CommitGate::Go(scope) => scope,
            CommitGate::Hold(note) | CommitGate::Refuse(note) => return refuse(self, note),
        };
        // A success reloads the code, which draws the written values; the
        // released drag stays until that reload so a redraw before it still
        // shows the dragged position. A failure (the source changed while
        // dragging, the history is busy) puts the layer back where it was.
        // The error shows in the Layer section and over the canvas.
        let values = values
            .into_iter()
            .map(|(key, value)| (key, Some(EditValue::Number(value))))
            .collect();
        self.commit_element(
            values,
            scope,
            |this, result, cx| {
                // Targets still current after a success mean nothing was
                // written (the text came out the same), so no reload comes
                // to drop the drag.
                if result.is_err() || this.element_targets().is_some() {
                    this.cancel_transform_drag();
                }
                cx.notify();
            },
            cx,
        );
        true
    }

    /// Escape during a drag: drops it and its overrides, writing nothing.
    /// True when a pressed drag was cancelled.
    pub(crate) fn escape_transform_drag(&mut self) -> bool {
        if !self.transform_drag_active() {
            return false;
        }
        self.cancel_transform_drag();
        true
    }

    /// Drops the drag and redraws the frame without the overrides.
    pub(crate) fn cancel_transform_drag(&mut self) {
        if self.transform_drag.take().is_some() {
            self.refresh_preview();
        }
    }

    pub(crate) fn frame_size(&self) -> (f32, f32) {
        let settings = &self.document.project().settings;
        (settings.width as f32, settings.height as f32)
    }
}

/// Whether a press whose topmost hit is `topmost` lands on the body of the
/// layer `frame` measures. Hit tests name leaves only, so a group's body is
/// any of its leaves.
pub(crate) fn presses_body(frame: &LayerFrame, topmost: Option<&str>) -> bool {
    topmost.is_some_and(|hit| frame.leaves.iter().any(|leaf| leaf == hit))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas_geometry::{box_frame, with_anchor_box};
    use crate::edit_history::{EditHistory, HistoryEntry, write_all_recorded};
    use crate::element_edit::{Target, commit_files, resolve_targets};
    use crate::transform_handles::{SizeField, SizeProps};
    use celesta_composition::EvaluatedTransform;
    use celesta_react_bridge::{
        ElementKind, LayerSource, PrimitivePropertyField, PrimitivePropertyGroup,
        PrimitivePropertyRow, PrimitivePropertyTable, PrimitiveSize, ReceivedValue,
    };
    use celesta_source_edit::{EditValue, ElementRef, LiteralForm, LockReason, Span};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn at(x: f64, y: f64) -> Point {
        Point { x, y }
    }

    const IDENTITY: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

    /// A 100 × 50 `Rect` anchored at its top-left corner at (40, 20).
    fn rect_frame() -> LayerFrame {
        let transform = EvaluatedTransform {
            position: at(40.0, 20.0),
            anchor: at(0.0, 0.0),
            ..EvaluatedTransform::default()
        };
        box_frame(transform, IDENTITY, 100.0, 50.0)
    }

    fn literal(value: f64) -> EditTarget {
        EditTarget::Literal {
            span: Span { start: 0, end: 1 },
            value: EditValue::Number(value),
            form: LiteralForm::Number,
        }
    }

    /// Targets with `rows` (name, target), written on one element.
    fn targets(rows: Vec<(&str, EditTarget)>) -> ElementTargets {
        let file = PathBuf::from("/project/scene.tsx");
        ElementTargets {
            file: file.clone(),
            files: vec![file.clone()],
            element: ElementRef {
                start: 0,
                end_of_attributes: 0,
                children: None,
            },
            targets: rows
                .into_iter()
                .map(|(name, target)| {
                    (
                        PropKey::Prop(name.to_owned()),
                        Target {
                            file: file.clone(),
                            path: name.to_owned(),
                            target,
                            call_site: None,
                            inputs: Vec::new(),
                        },
                    )
                })
                .collect(),
            table: None,
            component: None,
            enclosing: None,
            defaults: Vec::new(),
            lift_lock: None,
            outside: Vec::new(),
        }
    }

    fn with_x(x: EditTarget) -> ElementTargets {
        targets(vec![
            ("x", x),
            ("y", literal(20.0)),
            ("rotation", literal(0.0)),
            ("scale", literal(1.0)),
        ])
    }

    #[test]
    fn a_keyframed_x_falls_back_to_the_pivot() {
        let frame = rect_frame();
        let keyframed = with_x(EditTarget::Keyframe {
            span: Span { start: 0, end: 1 },
            value: 40.0,
            frame: 0.0,
        });
        assert!(!compensable(&keyframed));
        let pivot = pivot_used(PivotMode::Center, &keyframed, &frame);
        assert_eq!(pivot, PivotMode::Pivot);
        let values = drag_values(
            &frame,
            Handle::Rotate,
            (at(200.0, 20.0), at(40.0, 180.0)),
            false,
            0.0,
            &[
                TransformProp::X.key(),
                TransformProp::Y.key(),
                TransformProp::Rotation.key(),
            ],
            pivot,
            None,
        );
        assert_eq!(values.len(), 1, "{values:?}");
        assert_eq!(values[0].0, TransformProp::Rotation.key());

        // A literal, an absent prop, and one passed through to a literal
        // take the compensation; a locked one does not.
        assert_eq!(
            pivot_used(PivotMode::Center, &with_x(literal(40.0)), &frame),
            PivotMode::Center
        );
        let missing = EditTarget::Missing {
            insert_at: 0,
            default: None,
        };
        assert!(compensable(&with_x(missing)));
        let passed = EditTarget::PassThrough {
            call_site: ElementRef {
                start: 0,
                end_of_attributes: 0,
                children: None,
            },
            param: "x".to_owned(),
            target: Box::new(literal(40.0)),
        };
        assert!(compensable(&with_x(passed)));
        assert!(!compensable(&with_x(EditTarget::Locked(
            LockReason::Spread
        ))));
        assert!(!compensable(&targets(vec![("y", literal(20.0))])));
        // Pivot chosen is Pivot used.
        assert_eq!(
            pivot_used(PivotMode::Pivot, &with_x(literal(40.0)), &frame),
            PivotMode::Pivot
        );
    }

    fn number(min: Option<f64>) -> PrimitivePropertyRow {
        PrimitivePropertyRow {
            key: String::new(),
            field: PrimitivePropertyField::Number {
                default_value: None,
                min,
                max: None,
                step: None,
                integer: None,
            },
            required: false,
        }
    }

    /// `rows` with a table whose `size` names `size` (width, height,
    /// radius) and whose Size section declares `fields` (name, min).
    fn sized(
        rows: Vec<(&str, EditTarget)>,
        size: [Option<&str>; 3],
        fields: &[(&str, Option<f64>)],
    ) -> ElementTargets {
        let [width, height, radius] = size.map(|name| name.map(str::to_owned));
        let mut targets = targets(rows);
        targets.table = Some(PrimitivePropertyTable {
            size: Some(PrimitiveSize {
                width,
                height,
                radius,
            }),
            groups: vec![PrimitivePropertyGroup {
                name: "Size".to_owned(),
                fields: fields
                    .iter()
                    .map(|(name, min)| PrimitivePropertyRow {
                        key: (*name).to_owned(),
                        ..number(*min)
                    })
                    .collect(),
            }],
        });
        targets
    }

    fn missing() -> EditTarget {
        EditTarget::Missing {
            insert_at: 0,
            default: None,
        }
    }

    fn size_field(name: &str, min: Option<f64>) -> SizeField {
        SizeField {
            name: name.to_owned(),
            min,
            max: None,
            integer: false,
            absent: false,
            keyframed: false,
        }
    }

    #[test]
    fn size_props_read_the_table() {
        let rect = sized(
            vec![
                ("x", literal(40.0)),
                ("width", literal(100.0)),
                ("height", missing()),
            ],
            [Some("width"), Some("height"), None],
            &[("width", Some(1.0)), ("height", None)],
        );
        assert_eq!(
            size_props(&rect),
            Some(SizeProps::Box {
                width: size_field("width", Some(1.0)),
                // Not written: an edge drag pins it.
                height: SizeField {
                    absent: true,
                    ..size_field("height", None)
                },
            })
        );
        let circle = sized(
            vec![("radius", literal(30.0))],
            [None, None, Some("radius")],
            &[("radius", Some(0.0))],
        );
        assert_eq!(
            size_props(&circle),
            Some(SizeProps::Radius(size_field("radius", Some(0.0))))
        );
        // A keyframed size is written at its keyframe.
        let text = sized(
            vec![(
                "maxWidth",
                EditTarget::Keyframe {
                    span: Span { start: 0, end: 1 },
                    value: 300.0,
                    frame: 0.0,
                },
            )],
            [Some("maxWidth"), None, None],
            &[("maxWidth", Some(0.0))],
        );
        assert_eq!(
            size_props(&text),
            Some(SizeProps::Width(SizeField {
                keyframed: true,
                ..size_field("maxWidth", Some(0.0))
            }))
        );

        // The Size tool on a layer with a box: its sizes join the drag, and
        // x/y compensate when they can.
        let frame = rect_frame();
        let resize = resize_used(ResizeTool::Size, &rect, &frame).expect("the Size tool");
        assert!(!resize.compensable, "y has no row");
        assert_eq!(resize_fallback(ResizeTool::Size, &rect, &frame), None);
        assert_eq!(resize_used(ResizeTool::Scale, &rect, &frame), None);
    }

    #[test]
    fn locked_size_falls_back_to_scale() {
        let rect = sized(
            vec![
                ("width", literal(100.0)),
                (
                    "height",
                    EditTarget::Locked(LockReason::Expression {
                        source: "h".to_owned(),
                    }),
                ),
            ],
            [Some("width"), Some("height"), None],
            &[("width", None), ("height", None)],
        );
        let frame = rect_frame();
        assert_eq!(size_props(&rect), None);
        assert_eq!(resize_used(ResizeTool::Size, &rect, &frame), None);
        let fallback = resize_fallback(ResizeTool::Size, &rect, &frame).expect("a fallback");
        assert_eq!(fallback.reason(), "Height: Written as an expression: h");
        // Scale chosen falls back from nothing.
        assert_eq!(resize_fallback(ResizeTool::Scale, &rect, &frame), None);
    }

    #[test]
    fn no_size_falls_back_to_scale() {
        let frame = rect_frame();
        // A `<Group>`'s table names no size; without a table there is none.
        let mut group = sized(vec![("x", literal(40.0))], [None, None, None], &[]);
        group.table.as_mut().unwrap().size = None;
        for targets in [group, with_x(literal(40.0))] {
            assert_eq!(size_props(&targets), None);
            assert_eq!(resize_used(ResizeTool::Size, &targets, &frame), None);
            assert_eq!(
                resize_fallback(ResizeTool::Size, &targets, &frame),
                Some(ResizeFallback::NoSize)
            );
        }
        // Nor without a flat box to resize.
        let rect = sized(
            vec![("width", literal(100.0)), ("height", literal(50.0))],
            [Some("width"), Some("height"), None],
            &[],
        );
        let mut tilted = frame.clone();
        tilted.transform.rotation_y = 30.0;
        tilted.transform.perspective = Some(800.0);
        assert_eq!(resize_used(ResizeTool::Size, &rect, &tilted), None);
        assert_eq!(
            resize_fallback(ResizeTool::Size, &rect, &tilted),
            Some(ResizeFallback::NoBox)
        );
    }

    fn keyframe(value: f64) -> EditTarget {
        EditTarget::Keyframe {
            span: Span { start: 0, end: 1 },
            value,
            frame: 0.0,
        }
    }

    #[test]
    fn a_keyframed_rotation_or_scale_falls_back_to_the_pivot() {
        let frame = rect_frame();
        let rows = |rotation, scale| {
            targets(vec![
                ("x", literal(40.0)),
                ("y", literal(20.0)),
                ("rotation", rotation),
                ("scale", scale),
            ])
        };
        // Literal x/y written beside a keyframed rotation would hold at
        // this keyframe only, and the layer would orbit between them.
        let animated = rows(keyframe(0.0), literal(1.0));
        assert!(compensable(&animated));
        assert_eq!(
            pivot_fallback(PivotMode::Center, &animated, &frame),
            Some(PivotFallback::Animated)
        );
        assert_eq!(
            PivotFallback::Animated.reason(),
            "Rotation or scale is animated"
        );
        let pivot = pivot_used(PivotMode::Center, &animated, &frame);
        assert_eq!(pivot, PivotMode::Pivot);
        let values = drag_values(
            &frame,
            Handle::Rotate,
            (at(200.0, 20.0), at(40.0, 180.0)),
            false,
            0.0,
            &drag_props(&animated, &frame),
            pivot,
            None,
        );
        assert_eq!(values.len(), 1, "{values:?}");
        assert_eq!(values[0].0, TransformProp::Rotation.key());
        // A keyframed scale too; a locked one is no drag at all.
        assert_eq!(
            pivot_fallback(
                PivotMode::Center,
                &rows(literal(0.0), keyframe(1.0)),
                &frame
            ),
            Some(PivotFallback::Animated)
        );
        let locked = EditTarget::Locked(LockReason::Spread);
        assert_eq!(
            pivot_fallback(PivotMode::Center, &rows(literal(0.0), locked), &frame),
            None
        );
    }

    #[test]
    fn a_keyframed_size_hides_the_handles_that_move_x_y() {
        let rect = sized(
            vec![
                ("x", literal(40.0)),
                ("y", literal(20.0)),
                ("width", keyframe(100.0)),
                ("height", literal(50.0)),
            ],
            [Some("width"), Some("height"), None],
            &[],
        );
        // Anchored at the top-left: only a width drag that moves the left
        // edge needs x, which would then hold at this keyframe only.
        let frame = rect_frame();
        let resize = resize_used(ResizeTool::Size, &rect, &frame).expect("the Size tool");
        let mut editable = drag_props(&rect, &frame);
        editable.extend(resize.keys());
        let shown: Vec<Handle> =
            shown_handles(&frame, &editable, 20.0, PivotMode::Center, Some(&resize))
                .into_iter()
                .map(|(handle, _)| handle)
                .collect();
        assert_eq!(
            shown,
            [
                Handle::Corner(1),
                Handle::Corner(2),
                Handle::Edge(0),
                Handle::Edge(1),
                Handle::Edge(2),
            ]
        );
    }

    #[test]
    fn alt_on_a_cross_that_moves_nothing_starts_nothing() {
        // A 100 x 50 box anchored at its top-left at (40, 20): its middle,
        // where Center puts the cross, is inside the outline, so a press
        // there would otherwise move the layer.
        let frame = rect_frame();
        let swallows = |targets: &ElementTargets, pivot, point, alt| {
            let editable = drag_props_in(targets, &frame, pivot);
            let reason = anchor_reason(targets, &frame, "Rect").is_some();
            swallows_alt_press(&frame, &editable, pivot, point, 4.0, alt, reason)
        };
        let anchors = anchors_with(literal(40.0), literal(0.0));
        let middle = at(90.0, 45.0);
        assert!(swallows(&anchors, PivotMode::Center, middle, true));
        // Without Alt, or away from the cross, the press drags as usual.
        assert!(!swallows(&anchors, PivotMode::Center, middle, false));
        assert!(!swallows(&anchors, PivotMode::Center, at(60.0, 30.0), true));
        // In Pivot mode Alt on the cross drags the anchor: not swallowed.
        assert!(!swallows(&anchors, PivotMode::Pivot, at(40.0, 20.0), true));
        // A pivot cross whose anchor can't move, and says why, swallows Alt.
        let keyframed = anchors_with(keyframe(40.0), literal(0.0));
        assert!(swallows(&keyframed, PivotMode::Pivot, at(41.0, 21.0), true));
        // An element without anchor rows (a `<Path>`): its pivot cross shows
        // nothing, so the press goes through as before; its center marker
        // still shows "Rotates about the center", so Alt there is taken.
        let no_anchor = with_x(literal(40.0));
        assert!(!swallows(
            &no_anchor,
            PivotMode::Pivot,
            at(40.0, 20.0),
            true
        ));
        assert!(swallows(&no_anchor, PivotMode::Center, middle, true));
    }

    #[test]
    fn the_center_cross_says_why_the_anchor_stays_in_pivot_mode_too() {
        assert_eq!(center_marker_text(true, None), CENTER_MARKER);
        assert_eq!(
            center_marker_text(
                true,
                Some("X: Comes from DX, which can't be traced".to_owned())
            ),
            "Rotates about the center; the anchor can't move: X: Comes from DX, which can't be traced"
        );
        // Without anchor rows there is no anchor to promise.
        assert_eq!(center_marker_text(false, None), "Rotates about the center");
    }

    #[test]
    fn an_anchor_drag_holds_the_overlay_still() {
        // A 100 x 50 box anchored at its top-left at (40, 20); the cross
        // dragged to its middle.
        let frame = rect_frame();
        let anchors = anchors_with(literal(40.0), literal(0.0));
        let editable = drag_props_in(&anchors, &frame, PivotMode::Pivot);
        let drag = |handle| {
            TransformDrag::new(
                frame.clone(),
                handle,
                at(40.0, 20.0),
                editable.clone(),
                PivotMode::Pivot,
                ReceivedProps::default(),
                Vec::new(),
            )
        };
        let mut anchor = drag(Handle::Anchor);
        assert_eq!(anchor.anchor_cross(), None, "before the pointer moves");
        anchor.follow(drag_values(
            &frame,
            Handle::Anchor,
            (at(40.0, 20.0), at(90.0, 45.0)),
            false,
            0.0,
            &editable,
            PivotMode::Pivot,
            None,
        ));
        // A preview frame drawn mid-drag, a hair off from the rounding.
        let mut drawn = frame.clone();
        drawn.transform.position = at(90.004, 44.996);
        drawn.transform.anchor = at(0.5, 0.5);
        // The outline, handles and stem stay on the drag's start frame; only
        // the cross follows, where the written anchor puts the pivot.
        assert!(anchor.holds_overlay());
        assert_eq!(anchor.overlay_frame(drawn.clone()), frame);
        let cross = anchor.anchor_cross().expect("the cross follows");
        assert!(
            (cross.x - 90.0).abs() < 1e-9 && (cross.y - 45.0).abs() < 1e-9,
            "{cross:?}"
        );
        // Any other drag draws the frame the preview drew.
        let moving = drag(Handle::Move);
        assert!(!moving.holds_overlay());
        assert_eq!(
            moving.overlay_frame(drawn.clone()).transform,
            drawn.transform
        );
        assert_eq!(moving.anchor_cross(), None);
    }

    #[test]
    fn the_anchor_moves_only_in_pivot_mode() {
        // In Center mode the cross marks the box's middle: a marker only.
        let frame = rect_frame();
        let anchors = anchors_with(literal(40.0), literal(0.0));
        let offered = |pivot| anchor_handle(&frame, &drag_props_in(&anchors, &frame, pivot));
        assert!(offered(PivotMode::Pivot).is_some());
        assert_eq!(offered(PivotMode::Center), None);
        assert!(
            !drag_props_in(&anchors, &frame, PivotMode::Center).contains(&anchor_key("X")),
            "no anchor prop in Center mode"
        );
        assert_eq!(
            CENTER_MARKER,
            "Rotates about the center; switch to Pivot to move the anchor"
        );
    }

    fn anchor_key(axis: &str) -> PropKey {
        PropKey::Prop(format!("anchor{axis}"))
    }

    #[test]
    fn a_keyframed_anchor_or_turn_has_no_anchor_handle() {
        let frame = rect_frame();
        let rows = |anchor_x, rotation| {
            targets(vec![
                ("x", literal(40.0)),
                ("y", literal(20.0)),
                ("rotation", rotation),
                ("anchorX", anchor_x),
                ("anchorY", literal(0.0)),
            ])
        };
        assert!(has_anchor(&rows(literal(0.0), literal(0.0)), &frame));
        assert!(!has_anchor(&rows(keyframe(0.0), literal(0.0)), &frame));
        assert!(!has_anchor(&rows(literal(0.0), keyframe(0.0)), &frame));
    }

    /// A `Rect` with `x`, `y`, `rotation`, `anchorX`, `anchorY`, `width`,
    /// `height`, each literal unless `animated` names it as a keyframe.
    fn rect_rows(animated: &str) -> ElementTargets {
        let row = |name: &'static str, value: f64| {
            let target = if name == animated {
                keyframe(value)
            } else {
                literal(value)
            };
            (name, target)
        };
        sized(
            vec![
                row("x", 40.0),
                row("y", 20.0),
                row("rotation", 0.0),
                row("anchorX", 0.0),
                row("anchorY", 0.0),
                row("width", 100.0),
                row("height", 50.0),
            ],
            [Some("width"), Some("height"), None],
            &[],
        )
    }

    fn shown_with(targets: &ElementTargets, frame: &LayerFrame) -> Vec<Handle> {
        let resize = resize_used(ResizeTool::Size, targets, frame).expect("the Size tool");
        let mut editable = drag_props(targets, frame);
        editable.extend(resize.keys());
        shown_handles(frame, &editable, 20.0, PivotMode::Pivot, Some(&resize))
            .into_iter()
            .map(|(handle, _)| handle)
            .collect()
    }

    #[test]
    fn the_drag_label_says_what_edits_change() {
        let drag_in = |scope, called_here| {
            TransformDrag::new(
                rect_frame(),
                Handle::Rotate,
                at(200.0, 20.0),
                vec![TransformProp::Rotation.key()],
                PivotMode::Pivot,
                ReceivedProps::default(),
                Vec::new(),
            )
            .with_scope(scope, called_here)
        };
        let drag = |scope| drag_in(scope, None);
        let mut shared = drag(Some(Scope::Shared));
        // Nothing written yet: no label.
        assert_eq!(shared.label(), None);
        shared.follow(vec![(TransformProp::Rotation.key(), 30.0)]);
        assert_eq!(
            shared.label().as_deref(),
            Some("rotation 0 → 30\nEdits change the shared value")
        );
        let mut this_scene = drag(Some(Scope::ThisScene));
        this_scene.follow(vec![(TransformProp::Rotation.key(), 30.0)]);
        assert_eq!(
            this_scene.label().as_deref(),
            Some("rotation 0 → 30\nEdits change this scene")
        );
        // A scene calling the component twice: this call only.
        let twice = || {
            Some(CalledHere {
                component: "PriceTag".to_owned(),
                times: 2,
            })
        };
        let mut this_call = drag_in(Some(Scope::ThisScene), twice());
        this_call.follow(vec![(TransformProp::Rotation.key(), 30.0)]);
        assert_eq!(
            this_call.label().as_deref(),
            Some("rotation 0 → 30\nEdits change this `<PriceTag>` only")
        );
        let mut shared = drag_in(Some(Scope::Shared), twice());
        shared.follow(vec![(TransformProp::Rotation.key(), 30.0)]);
        assert_eq!(
            shared.label().as_deref(),
            Some("rotation 0 → 30\nEdits change the shared value")
        );
        // An element that is not shared: one line.
        let mut alone = drag(None);
        alone.follow(vec![(TransformProp::Rotation.key(), 30.0)]);
        assert_eq!(alone.label().as_deref(), Some("rotation 0 → 30"));
    }

    #[test]
    fn a_drag_about_the_pivot_says_why_center_is_not_in_effect() {
        let frame = rect_frame();
        let editable = vec![TransformProp::Rotation.key()];
        let reason = PivotFallback::NotCompensable.reason();
        let drag = |handle, resize: Option<Resize>| {
            TransformDrag::new(
                frame.clone(),
                handle,
                at(200.0, 20.0),
                editable.clone(),
                PivotMode::Pivot,
                ReceivedProps::default(),
                Vec::new(),
            )
            .with_resize(resize)
            .with_note(Some(reason))
        };
        // From the press, before anything is written.
        let mut rotation = drag(Handle::Rotate, None);
        assert_eq!(
            rotation.label().as_deref(),
            Some("Pivot — x or y can't be edited")
        );
        rotation.follow(vec![(TransformProp::Rotation.key(), 30.0)]);
        assert_eq!(
            rotation.label().as_deref(),
            Some("Pivot — x or y can't be edited  rotation 0 → 30")
        );
        // The Scale tool's handles turn about it too; the Size tool's and a
        // move don't.
        assert!(drag(Handle::Corner(2), None).label().is_some());
        let resize = Resize {
            props: SizeProps::Width(size_field("maxWidth", None)),
            compensable: true,
            animated: false,
        };
        assert_eq!(drag(Handle::Edge(1), Some(resize)).label(), None);
        assert_eq!(drag(Handle::Move, None).label(), None);
    }

    #[test]
    fn a_resize_that_moves_the_anchor_needs_a_still_turn_and_anchor() {
        // Anchored at the top-left: the handles that keep the top-left
        // corner need no x/y and stay; the others would combine literal
        // x/y with a value that changes between keyframes.
        let frame = rect_frame();
        let still = [
            Handle::Corner(2),
            Handle::Edge(1),
            Handle::Edge(2),
            Handle::Rotate,
        ];
        assert_eq!(shown_with(&rect_rows("rotation"), &frame), still);
        assert_eq!(shown_with(&rect_rows("anchorX"), &frame), still);
        assert_eq!(shown_with(&rect_rows(""), &frame).len(), 9);
        // Anchored at the middle, no handle is left: Scale, with the reason.
        let mut centered = rect_frame();
        centered.transform.anchor = at(0.5, 0.5);
        for (animated, reason) in [("rotation", TURN_ANIMATED), ("anchorX", ANCHOR_ANIMATED)] {
            let fallback = resize_fallback(ResizeTool::Size, &rect_rows(animated), &centered);
            assert_eq!(fallback, Some(ResizeFallback::Animated(reason)));
            assert_eq!(fallback.unwrap().reason(), reason);
        }
        // A keyframed width hides only the handles that move the anchor
        // along x: the top and bottom edges still resize the height.
        assert_eq!(
            shown_with(&rect_rows("width"), &centered),
            [Handle::Edge(0), Handle::Edge(2), Handle::Rotate]
        );
    }

    #[test]
    fn center_needs_a_still_box() {
        // The center is a point of the box, which a keyframed size or
        // anchor moves between keyframes.
        let frame = rect_frame();
        for animated in ["width", "anchorX"] {
            let fallback = pivot_fallback(PivotMode::Center, &rect_rows(animated), &frame);
            assert_eq!(fallback, Some(PivotFallback::AnimatedBox));
            assert_eq!(fallback.unwrap().reason(), BOX_ANIMATED);
            assert_eq!(BOX_ANIMATED, "Size or anchor is animated");
        }
        assert_eq!(
            pivot_fallback(PivotMode::Center, &rect_rows(""), &frame),
            None
        );
        assert_eq!(
            PivotFallback::NotCompensable.reason(),
            "x or y can't be edited"
        );
        assert_eq!(PivotFallback::Animated.reason(), TURN_ANIMATED);
    }

    #[test]
    fn the_cross_says_why_it_stays() {
        let frame = rect_frame();
        let reason = |targets: &ElementTargets, frame: &LayerFrame| {
            let reason = anchor_reason(targets, frame, "Rect");
            // A reason exactly when the cross can't be Alt-dragged.
            assert_eq!(reason.is_none(), has_anchor(targets, frame), "{reason:?}");
            reason
        };
        assert_eq!(reason(&rect_rows(""), &frame), None);
        // `x` an imported constant: the row's own reason, named.
        let mut traced = rect_rows("");
        traced.targets[0].1.target = EditTarget::Locked(LockReason::UnresolvedIdentifier {
            name: "DX".to_owned(),
        });
        assert_eq!(
            reason(&traced, &frame).as_deref(),
            Some("X: Comes from DX, which can't be traced")
        );
        assert_eq!(
            reason(&rect_rows("y"), &frame).as_deref(),
            Some("Y is animated")
        );
        assert_eq!(
            reason(&rect_rows("rotation"), &frame).as_deref(),
            Some(TURN_ANIMATED)
        );
        assert_eq!(
            reason(&rect_rows("anchorX"), &frame).as_deref(),
            Some(ANCHOR_ANIMATED)
        );
        assert_eq!(
            reason(&rect_rows("width"), &frame).as_deref(),
            Some(SIZE_ANIMATED)
        );
        let mut tilted = frame.clone();
        tilted.transform.rotation_y = 30.0;
        tilted.transform.perspective = Some(800.0);
        assert_eq!(
            reason(&rect_rows(""), &tilted).as_deref(),
            Some(NO_ANCHOR_BOX)
        );
        // Without anchor rows (a `<Path>`) there is no cross to explain.
        assert_eq!(anchor_reason(&with_x(literal(40.0)), &frame, "Path"), None);
    }

    #[test]
    fn an_anchor_whose_box_is_animated_stays() {
        // `x`/`y` would move by the anchor's change times the box's width,
        // which is another on the other keyframes.
        let rect = |width| {
            sized(
                vec![
                    ("x", literal(40.0)),
                    ("y", literal(20.0)),
                    ("anchorX", literal(0.0)),
                    ("anchorY", literal(0.0)),
                    ("width", width),
                    ("height", literal(50.0)),
                ],
                [Some("width"), Some("height"), None],
                &[],
            )
        };
        let frame = rect_frame();
        assert!(has_anchor(&rect(literal(100.0)), &frame));
        assert_eq!(anchor_reason(&rect(literal(100.0)), &frame, "Rect"), None);
        let animated = rect(keyframe(100.0));
        assert!(!has_anchor(&animated, &frame));
        assert_eq!(
            anchor_reason(&animated, &frame, "Rect").as_deref(),
            Some("Size is animated")
        );
    }

    #[test]
    fn a_box_no_handle_can_resize_falls_back_to_scale() {
        let centered = || {
            let mut frame = rect_frame();
            frame.transform.anchor = at(0.5, 0.5);
            frame
        };
        // Anchored at its middle, every handle moves x and y, which are
        // keyframes.
        let rect = sized(
            vec![
                ("x", keyframe(40.0)),
                ("y", literal(20.0)),
                ("width", literal(100.0)),
                ("height", literal(50.0)),
            ],
            [Some("width"), Some("height"), None],
            &[],
        );
        assert_eq!(resize_used(ResizeTool::Size, &rect, &centered()), None);
        let fallback = resize_fallback(ResizeTool::Size, &rect, &centered());
        assert_eq!(fallback, Some(ResizeFallback::Pinned));
        assert_eq!(fallback.unwrap().reason(), "x or y can't move");
        // Anchored at its top-left, the right and bottom edges still work.
        assert!(resize_used(ResizeTool::Size, &rect, &rect_frame()).is_some());
        // A keyframed radius about its middle: x and y could move, but
        // only on this keyframe.
        let circle = sized(
            vec![
                ("x", literal(40.0)),
                ("y", literal(20.0)),
                ("radius", keyframe(30.0)),
            ],
            [None, None, Some("radius")],
            &[],
        );
        let fallback = resize_fallback(ResizeTool::Size, &circle, &centered());
        assert_eq!(fallback, Some(ResizeFallback::Animated(SIZE_ANIMATED)));
        assert_eq!(fallback.unwrap().reason(), "Size is animated");
    }

    #[test]
    fn an_unreported_size_has_no_size_to_resize() {
        // A shape whose `width` the runtime didn't report: no box from its
        // props, and its path has none of its own.
        let rect = sized(
            vec![("width", literal(100.0)), ("height", literal(50.0))],
            [Some("width"), Some("height"), None],
            &[],
        );
        let mut path = rect_frame();
        path.anchor_box = None;
        assert_eq!(
            resize_fallback(ResizeTool::Size, &rect, &path),
            Some(ResizeFallback::NoSize)
        );
    }

    #[test]
    fn a_released_drag_keeps_its_box() {
        // The targets are gone while the write runs: the drag still knows
        // the element and the props it draws.
        let received = number_props(&[("width", 100.0), ("height", 50.0)]);
        let mut path = rect_frame();
        path.anchor_box = None;
        let drag = TransformDrag::new(
            path.clone(),
            Handle::Edge(1),
            at(140.0, 45.0),
            Vec::new(),
            PivotMode::Pivot,
            received,
            Vec::new(),
        )
        .with_primitive("Ellipse");
        let anchored = drag.anchored(path);
        assert_eq!(anchored.anchor_box, rect_frame().anchor_box);
    }

    #[test]
    fn a_size_without_an_editable_row_says_so() {
        // The table names a size, but the element has no row to write it.
        let rect = sized(
            vec![("x", literal(40.0))],
            [Some("width"), Some("height"), None],
            &[],
        );
        let fallback = resize_fallback(ResizeTool::Size, &rect, &rect_frame());
        assert_eq!(fallback, Some(ResizeFallback::Uneditable));
        assert_eq!(fallback.unwrap().reason(), "Size can't be edited here");
    }

    fn number_props(props: &[(&str, f64)]) -> ReceivedProps {
        ReceivedProps {
            others: props
                .iter()
                .map(|(name, value)| ((*name).to_owned(), ReceivedValue::Number(*value)))
                .collect(),
            ..ReceivedProps::default()
        }
    }

    #[test]
    fn a_running_resize_rebuilds_the_box_from_its_values() {
        let received = number_props(&[("width", 100.0), ("height", 50.0)]);
        let frame = with_anchor_box(rect_frame(), "Rect", &received);
        let resize = Resize {
            props: SizeProps::Box {
                width: size_field("width", None),
                height: size_field("height", None),
            },
            compensable: true,
            animated: false,
        };
        let mut editable = vec![TransformProp::X.key(), TransformProp::Y.key()];
        editable.extend(resize.keys());
        // The right edge, from (140, 45) to (190, 45): 150 wide.
        let mut drag = TransformDrag::new(
            frame.clone(),
            Handle::Edge(1),
            at(140.0, 45.0),
            editable.clone(),
            PivotMode::Pivot,
            received.clone(),
            Vec::new(),
        )
        .with_resize(Some(resize.clone()));
        assert_eq!(drag.received_now(), received);
        drag.follow(drag_values(
            &frame,
            Handle::Edge(1),
            (at(140.0, 45.0), at(190.0, 45.0)),
            false,
            0.0,
            &editable,
            PivotMode::Pivot,
            Some(&resize),
        ));
        // The frame drawn mid-drag, its box built from the dragged width.
        let now = with_anchor_box(rect_frame(), "Rect", &drag.received_now());
        let corner = shown_handles(&now, &editable, 20.0, PivotMode::Pivot, Some(&resize))
            .into_iter()
            .find(|(handle, _)| *handle == Handle::Corner(2))
            .map(|(_, point)| point);
        assert_eq!(corner, Some(at(190.0, 70.0)));
    }

    /// A table for a `<Rect>` (`x`, `y`, `width`, `height`) or a `<Text>`
    /// (`x`, `y`, `maxWidth`).
    fn table_of(size: [Option<&str>; 3]) -> PrimitivePropertyTable {
        let [width, height, radius] = size.map(|name| name.map(str::to_owned));
        let row = |name: &str| PrimitivePropertyRow {
            key: name.to_owned(),
            ..number(None)
        };
        let names = [Some("x"), Some("y"), size[0], size[1]];
        PrimitivePropertyTable {
            size: Some(PrimitiveSize {
                width,
                height,
                radius,
            }),
            groups: vec![PrimitivePropertyGroup {
                name: "Transform".to_owned(),
                fields: names.into_iter().flatten().map(row).collect(),
            }],
        }
    }

    /// A one-element scene `source` written to a fresh directory, and the
    /// report of its element `name` with the props the runtime saw.
    fn one_file(source: &str, name: &str, props: ReceivedProps) -> (PathBuf, LayerSourceReport) {
        let dir = std::env::temp_dir().join(format!(
            "celesta-resize-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let scene = dir.join("scene.tsx");
        fs::write(&scene, source).unwrap();
        let report = LayerSourceReport {
            chain: vec![entry(name, ElementKind::Primitive, &scene)],
            local_frame: 0.0,
            candidate_frames: vec![],
            props,
            render_count: 1,
            windows: vec![],
            span: None,
        };
        (scene, report)
    }

    /// Resizes the element `report` names by `handle` from `from` to `to`
    /// (scene pixels) with the Size tool, and writes the result as one
    /// history entry.
    fn resize_and_write(
        report: &LayerSourceReport,
        table: &PrimitivePropertyTable,
        frame: LayerFrame,
        handle: Handle,
        (from, to): (Point, Point),
    ) -> HistoryEntry {
        let read = |path: &Path| fs::read_to_string(path).map_err(|error| error.to_string());
        let targets = resolve_targets(read, report, Some(table)).expect("the element resolves");
        let frame = with_anchor_box(frame, &report.chain[0].name, &report.props);
        let resize = resize_used(ResizeTool::Size, &targets, &frame).expect("the Size tool");
        let mut editable = drag_props(&targets, &frame);
        editable.extend(resize.keys());
        let pivot = pivot_used(PivotMode::Center, &targets, &frame);
        let values = drag_values(
            &frame,
            handle,
            (from, to),
            false,
            0.0,
            &editable,
            pivot,
            Some(&resize),
        );
        let values: Vec<_> = prop_values(&frame, &values, &report.props, Some(&resize))
            .into_iter()
            .map(|(key, value)| (key, Some(EditValue::Number(value))))
            .collect();
        let files = write_all_recorded(targets.files.clone(), |texts| {
            commit_files(texts, &targets, report, None, None, &values)
                .map_err(|error| error.to_string())
        })
        .expect("written");
        HistoryEntry { files }
    }

    #[test]
    fn a_left_edge_resize_is_one_undo() {
        const SOURCE: &str =
            "export const Scene = () => (\n  <Rect x={40} y={20} width={100} height={50} />\n);\n";
        let props = ReceivedProps {
            x: Some(40.0),
            y: Some(20.0),
            ..number_props(&[("width", 100.0), ("height", 50.0)])
        };
        let (scene, report) = one_file(SOURCE, "Rect", props);
        // The left edge, 20 px out: the right edge stays at 140.
        let entry = resize_and_write(
            &report,
            &table_of([Some("width"), Some("height"), None]),
            rect_frame(),
            Handle::Edge(3),
            (at(40.0, 45.0), at(20.0, 45.0)),
        );
        assert_eq!(
            fs::read_to_string(&scene).unwrap(),
            SOURCE.replace("x={40} y={20} width={100}", "x={20} y={20} width={120}")
        );
        let mut history = EditHistory::default();
        history.push(entry);
        history.undo().unwrap();
        assert_eq!(fs::read_to_string(&scene).unwrap(), SOURCE);
        fs::remove_dir_all(scene.parent().unwrap()).ok();
    }

    #[test]
    fn a_missing_max_width_is_inserted() {
        const SOURCE: &str = "export const Scene = () => (\n  <Text x={40} y={20}>Hi</Text>\n);\n";
        let props = ReceivedProps {
            x: Some(40.0),
            y: Some(20.0),
            ..ReceivedProps::default()
        };
        let (scene, report) = one_file(SOURCE, "Text", props);
        // Laid out 100 wide; the right edge 50 px out.
        resize_and_write(
            &report,
            &table_of([Some("maxWidth"), None, None]),
            rect_frame(),
            Handle::Edge(1),
            (at(140.0, 45.0), at(190.0, 45.0)),
        );
        let written = fs::read_to_string(&scene).unwrap();
        assert!(written.contains("maxWidth={150}"), "{written}");
        fs::remove_dir_all(scene.parent().unwrap()).ok();
    }

    fn anchors_with(x: EditTarget, anchor_y: EditTarget) -> ElementTargets {
        targets(vec![
            ("x", x),
            ("y", literal(20.0)),
            ("rotation", literal(0.0)),
            ("scale", literal(1.0)),
            ("anchorX", literal(0.0)),
            ("anchorY", anchor_y),
        ])
    }

    fn has_anchor(targets: &ElementTargets, frame: &LayerFrame) -> bool {
        anchor_handle(frame, &drag_props(targets, frame)).is_some()
    }

    #[test]
    fn an_anchor_drag_needs_compensable_x_y() {
        let frame = rect_frame();
        let anchors = anchors_with(literal(40.0), literal(0.0));
        assert!(has_anchor(&anchors, &frame));
        assert_eq!(
            drag_props(&anchors, &frame)[4..],
            [
                PropKey::Prop("anchorX".to_owned()),
                PropKey::Prop("anchorY".to_owned())
            ]
        );
        let keyframed = anchors_with(
            EditTarget::Keyframe {
                span: Span { start: 0, end: 1 },
                value: 40.0,
                frame: 0.0,
            },
            literal(0.0),
        );
        assert!(!has_anchor(&keyframed, &frame));
        // Nor without a box to take fractions of.
        let mut tilted = frame.clone();
        tilted.transform.rotation_y = 30.0;
        tilted.transform.perspective = Some(800.0);
        assert!(!has_anchor(&anchors, &tilted));
        // Without anchor rows (no property table) there is nothing to drag.
        assert!(!has_anchor(&with_x(literal(40.0)), &frame));
    }

    #[test]
    fn the_toggle_says_why_center_falls_back() {
        let frame = rect_frame();
        let plain = with_x(literal(40.0));
        assert_eq!(pivot_fallback(PivotMode::Center, &plain, &frame), None);
        let keyframed = with_x(EditTarget::Keyframe {
            span: Span { start: 0, end: 1 },
            value: 40.0,
            frame: 0.0,
        });
        assert_eq!(
            pivot_fallback(PivotMode::Center, &keyframed, &frame),
            Some(PivotFallback::NotCompensable)
        );
        let mut tilted = frame.clone();
        tilted.transform.rotation_y = 30.0;
        tilted.transform.perspective = Some(800.0);
        assert_eq!(
            pivot_fallback(PivotMode::Center, &plain, &tilted),
            Some(PivotFallback::NoFlatBox)
        );
        // Pivot chosen falls back from nothing.
        assert_eq!(pivot_fallback(PivotMode::Pivot, &keyframed, &tilted), None);
    }

    #[test]
    fn a_group_s_anchor_is_inert() {
        // A `<Group>`'s anchor rows are editable, but the renderer places its
        // children by its position alone.
        let anchors = anchors_with(literal(40.0), literal(0.0));
        let received = ReceivedProps::default();
        let group = with_anchor_box(rect_frame(), "Group", &received);
        assert!(!has_anchor(&anchors, &group));
        assert_eq!(
            anchor_reason(&anchors, &group, "Group").as_deref(),
            Some(GROUP_ANCHOR)
        );
        // Any other element anchors in its box.
        let rect = with_anchor_box(rect_frame(), "Rect", &received);
        assert!(has_anchor(&anchors, &rect));
        assert_eq!(anchor_reason(&anchors, &rect, "Rect"), None);
    }

    #[test]
    fn a_baseline_text_anchor_is_inert() {
        let frame = rect_frame();
        let baseline = anchors_with(
            literal(40.0),
            EditTarget::Locked(LockReason::Expression {
                source: "\"baseline\"".to_owned(),
            }),
        );
        assert!(!has_anchor(&baseline, &frame));
        assert_eq!(
            anchor_reason(&baseline, &frame, "Text").as_deref(),
            Some("Anchor Y: Written as an expression: \"baseline\"")
        );
        assert_eq!(
            anchor_reason(&anchors_with(literal(40.0), literal(0.0)), &frame, "Text"),
            None
        );
    }

    const COMPONENT: &str =
        "export const PriceTag = ({ x, y }) => (\n  <Rect x={x} y={y} rotation={0} />\n);\n";
    const SCENE: &str = "export const Scene = () => (\n  <PriceTag x={40} y={20} />\n);\n";

    fn entry(name: &str, kind: ElementKind, file: &Path) -> LayerSource {
        LayerSource {
            name: name.to_owned(),
            file: file.to_path_buf(),
            line: 2,
            column: 3,
            kind,
            layer_id: None,
        }
    }

    #[test]
    fn a_two_file_center_rotation_is_one_undo() {
        let dir = std::env::temp_dir().join(format!(
            "celesta-center-rotation-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let (component, scene) = (dir.join("component.tsx"), dir.join("scene.tsx"));
        fs::write(&component, COMPONENT).unwrap();
        fs::write(&scene, SCENE).unwrap();
        // `<Rect>` in `PriceTag`, called from the scene with `x`, `y`.
        let report = LayerSourceReport {
            chain: vec![
                entry("Rect", ElementKind::Primitive, &component),
                entry("PriceTag", ElementKind::Component, &scene),
            ],
            local_frame: 0.0,
            candidate_frames: vec![],
            props: ReceivedProps {
                x: Some(40.0),
                y: Some(20.0),
                rotation: Some(0.0),
                ..ReceivedProps::default()
            },
            render_count: 1,
            windows: vec![],
            span: None,
        };
        let read = |path: &Path| fs::read_to_string(path).map_err(|error| error.to_string());
        let targets = resolve_targets(read, &report, None).expect("the element resolves");
        assert!(compensable(&targets));

        // A quarter turn about the box center (90, 45): the top-left corner
        // goes from (40, 20) to (115, −5).
        let frame = rect_frame();
        let pivot = pivot_used(PivotMode::Center, &targets, &frame);
        assert_eq!(pivot, PivotMode::Center);
        let editable = drag_props(&targets, &frame);
        let values = drag_values(
            &frame,
            Handle::Rotate,
            (at(190.0, 45.0), at(90.0, 145.0)),
            false,
            0.0,
            &editable,
            pivot,
            None,
        );
        let values: Vec<_> = prop_values(&frame, &values, &report.props, None)
            .into_iter()
            .map(|(key, value)| (key, Some(EditValue::Number(value))))
            .collect();
        let files = write_all_recorded(targets.files.clone(), |texts| {
            commit_files(texts, &targets, &report, None, None, &values)
                .map_err(|error| error.to_string())
        })
        .expect("both files are written");
        assert_eq!(files.len(), 2);
        assert_eq!(
            fs::read_to_string(&component).unwrap(),
            COMPONENT.replace("rotation={0}", "rotation={90}")
        );
        assert_eq!(
            fs::read_to_string(&scene).unwrap(),
            SCENE.replace("x={40} y={20}", "x={115} y={-5}")
        );

        // One entry: one undo puts both files back.
        let mut history = EditHistory::default();
        history.push(HistoryEntry { files });
        history.undo().unwrap();
        assert_eq!(fs::read_to_string(&component).unwrap(), COMPONENT);
        assert_eq!(fs::read_to_string(&scene).unwrap(), SCENE);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_drag_offers_no_size_handles_for_a_refused_size_prop() {
        let resize = Resize {
            props: SizeProps::Box {
                width: size_field("width", None),
                height: size_field("height", None),
            },
            compensable: true,
            animated: false,
        };
        let moves = vec![TransformProp::X.key(), TransformProp::Y.key()];
        let width = resize.keys()[0].clone();
        let sets_own = "This scene sets its own `rectWidth`".to_owned();
        let refused = |key: &PropKey| (*key == width).then(|| sets_own.clone());

        // A size prop the release would refuse: no size handles, the moves
        // stay, and the refusal is the reason when nothing else is left.
        assert_eq!(
            without_refused(moves.clone(), Some(resize.clone()), refused),
            (moves.clone(), None, Some(sets_own.clone()))
        );
        // A moved prop refused: dropped, the size handles stay.
        let x = TransformProp::X.key();
        let refused_x = |key: &PropKey| (*key == x).then(|| sets_own.clone());
        assert_eq!(
            without_refused(moves.clone(), Some(resize.clone()), refused_x),
            (
                vec![TransformProp::Y.key()],
                Some(resize.clone()),
                Some(sets_own.clone())
            )
        );
        // Nothing refused: as offered.
        assert_eq!(
            without_refused(moves.clone(), Some(resize.clone()), |_| None),
            (moves, Some(resize), None)
        );
    }
}
