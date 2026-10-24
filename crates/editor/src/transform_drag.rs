//! Dragging the selected React layer on the canvas: which props the drag may
//! change, the prop overrides the preview evaluates the scene with while it
//! runs, and the one write on release.

use crate::element_edit::{
    ElementTargets, PropKey, TransformProp, editable, inner_target, reason_text,
};
use crate::layer_select::{letterbox, to_scene};
use crate::transform_handles::{
    Handle, drag_label, drag_values, handle_at, preview_values, prop_values, shown_handles, turn,
};
use crate::view::EditorView;
use celesta_composition::Point;
use celesta_gpu_renderer::LayerFrame;
use celesta_react_bridge::{LayerSourceReport, PropOverride, ReceivedProps, SourcePos};
use celesta_source_edit::{EditTarget, EditValue};
use gpui_kit::{Context, MouseMoveEvent};

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
    editable: Vec<TransformProp>,
    /// The props the runtime saw when the drag started.
    received: ReceivedProps,
    /// Where the runtime takes each editable prop's dragged value, kept from
    /// the start: the targets are gone while the write runs.
    sites: Vec<(PropKey, OverrideSite)>,
    /// How far a rotation drag has turned so far, unsnapped, in degrees.
    turned: f64,
    /// The transform values the pointer has made so far.
    values: Vec<(TransformProp, f64)>,
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
        editable: Vec<TransformProp>,
        received: ReceivedProps,
        sites: Vec<(PropKey, OverrideSite)>,
    ) -> Self {
        Self {
            frame,
            handle,
            start,
            editable,
            received,
            sites,
            turned: 0.0,
            values: Vec::new(),
            released: false,
        }
    }

    /// Takes the transform values the pointer made; true when they changed.
    pub(crate) fn follow(&mut self, values: Vec<(TransformProp, f64)>) -> bool {
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

/// The overrides that draw the transform values `values` (see
/// `prop_values`).
pub(crate) fn prop_overrides(
    sites: &[(PropKey, OverrideSite)],
    values: &[(TransformProp, f64)],
) -> Vec<PropOverride> {
    values
        .iter()
        .filter_map(|&(prop, value)| {
            let key = prop.key();
            let (_, site) = sites.iter().find(|(candidate, _)| *candidate == key)?;
            Some(site.with(value))
        })
        .collect()
}

/// What the canvas can do with the selected layer.
pub(crate) enum DragState {
    /// No frame or no targets yet: the outline only.
    Unknown,
    /// The layer has no affine frame.
    NoFrame,
    /// Every prop is locked; the first reason.
    Locked(LayerFrame, String),
    /// Draggable with these props.
    Editable(LayerFrame, Vec<TransformProp>),
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

    /// What dragging the selected layer can change, for the canvas.
    pub(crate) fn drag_state(&self) -> DragState {
        if !self.picks_layers() || self.playing {
            return DragState::Unknown;
        }
        let frame = match self.selected_frame() {
            None => return DragState::Unknown,
            Some(None) => return DragState::NoFrame,
            Some(Some(frame)) => frame.clone(),
        };
        // While dragging, the props stay those the drag began with; the
        // frame is the one drawn with the overrides, so the handles follow.
        if let Some(drag) = &self.transform_drag {
            return DragState::Editable(frame, drag.editable.clone());
        }
        let Some((targets, _)) = self.element_targets() else {
            return DragState::Unknown;
        };
        // The transform rows, in row order.
        let transform: Vec<(TransformProp, &EditTarget)> = TransformProp::ALL
            .into_iter()
            .filter_map(|prop| Some((prop, targets.transform(prop)?)))
            .collect();
        let props: Vec<TransformProp> = transform
            .iter()
            .filter(|(_, target)| editable(target))
            .map(|(prop, _)| *prop)
            .collect();
        if props.is_empty() {
            let reason = transform
                .iter()
                .find_map(|(_, target)| match inner_target(target) {
                    EditTarget::Locked(reason) => Some(reason_text(reason)),
                    _ => None,
                })
                .unwrap_or_default();
            return DragState::Locked(frame, reason);
        }
        DragState::Editable(frame, props)
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

    /// The drag's `x 120 → 184` label, once the pointer changed something.
    pub(crate) fn transform_drag_label(&self) -> Option<String> {
        let drag = self.transform_drag.as_ref()?;
        let label = drag_label(&drag.frame, &drag.values, &drag.received);
        (!label.is_empty()).then_some(label)
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
    /// selected layer, or on its body with the layer topmost there. True when
    /// a drag started.
    pub(crate) fn begin_transform_drag(
        &mut self,
        position: gpui_kit::Point<gpui_kit::Pixels>,
        topmost: Option<&str>,
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
        let DragState::Editable(frame, editable) = self.drag_state() else {
            return false;
        };
        let Some((received, sites)) = self
            .element_targets()
            .map(|(targets, report)| (report.props.clone(), override_sites(targets, report)))
        else {
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
        let shown = shown_handles(&frame, &editable, f64::from(ROTATE_STEM) / scale);
        let handle = match handle_at(&frame, &shown, start, f64::from(HANDLE_REACH) / scale) {
            Some(Handle::Move)
                if presses_body(&frame, topmost)
                    && (editable.contains(&TransformProp::X)
                        || editable.contains(&TransformProp::Y)) =>
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
        self.transform_drag = Some(TransformDrag::new(
            frame, handle, start, editable, received, sites,
        ));
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
        );
        if drag.handle == Handle::Rotate
            && let Some(turned) = turn(&drag.frame, drag.start, now, drag.turned)
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
        let values = prop_values(&drag.frame, &drag.values, &drag.received);
        if values.is_empty() {
            self.cancel_transform_drag();
            return true;
        }
        drag.released = true;
        if self.element_targets().is_none() {
            // The targets went stale mid-drag (a reload); nothing to write to.
            let message = self
                .element_edit
                .unavailable(self.selected_layer.as_deref());
            self.element_edit
                .set_error(self.selected_layer.as_deref(), Some(message));
            self.cancel_transform_drag();
            return true;
        }
        // A success reloads the code, which draws the written values; the
        // released drag stays until that reload so a redraw before it still
        // shows the dragged position. A failure (the source changed while
        // dragging, the history is busy) puts the layer back where it was.
        // The error shows in the Layer section and over the canvas.
        let values = values
            .into_iter()
            .map(|(prop, value)| (prop.key(), Some(EditValue::Number(value))))
            .collect();
        self.commit_element(
            values,
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
