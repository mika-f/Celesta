//! Dragging the selected React layer on the canvas: which props the drag may
//! change, the transform the preview draws while it runs, and the one write
//! on release.

use crate::layer_select::{letterbox, to_scene};
use crate::transform_edit::{TransformProp, reason_text};
use crate::transform_handles::{
    Handle, drag_label, drag_values, handle_at, override_transform, prop_values, shown_handles,
    turn,
};
use crate::view::EditorView;
use celesta_composition::{EvaluatedTransform, Point};
use celesta_gpu_renderer::LayerFrame;
use celesta_react_bridge::ReceivedProps;
use celesta_source_edit::EditTarget;
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
    layer: String,
    /// The layer's frame when the drag started; every move is measured
    /// against it, not against the frames drawn with the override.
    frame: LayerFrame,
    handle: Handle,
    start: Point,
    editable: Vec<TransformProp>,
    /// The props the runtime saw when the drag started.
    received: ReceivedProps,
    /// How far a rotation drag has turned so far, unsnapped, in degrees.
    turned: f64,
    /// The transform values the pointer has made so far.
    values: Vec<(TransformProp, f64)>,
    /// Released; the override stays while the write runs and, after a
    /// successful one, until its reload, so the old position does not flash
    /// before the reloaded code draws the new one.
    released: bool,
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
        // frame is the one drawn with the override, so the handles follow.
        if let Some(drag) = &self.transform_drag {
            return DragState::Editable(frame, drag.editable.clone());
        }
        let Some((targets, _)) = self.transform_targets() else {
            return DragState::Unknown;
        };
        let editable: Vec<TransformProp> = targets
            .targets
            .iter()
            .filter(|(_, target)| !matches!(target, EditTarget::Locked(_)))
            .map(|(prop, _)| *prop)
            .collect();
        if editable.is_empty() {
            let reason = targets
                .targets
                .iter()
                .find_map(|(_, target)| match target {
                    EditTarget::Locked(reason) => Some(reason_text(reason)),
                    _ => None,
                })
                .unwrap_or_default();
            return DragState::Locked(frame, reason);
        }
        DragState::Editable(frame, editable)
    }

    /// The transform the preview draws for the dragged layer in place of its
    /// own.
    pub(crate) fn transform_override(&self) -> Option<(String, EvaluatedTransform)> {
        if self.playing {
            return None;
        }
        let drag = self.transform_drag.as_ref()?;
        Some((
            drag.layer.clone(),
            override_transform(&drag.frame, &drag.values),
        ))
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
        let DragState::Editable(frame, editable) = self.drag_state() else {
            return false;
        };
        let Some(received) = self.transform_targets().map(|(_, report)| report.props) else {
            return false;
        };
        let (Some(layer), Some(bounds)) = (self.selected_layer.clone(), self.viewer_bounds.get())
        else {
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
        self.transform_drag = Some(TransformDrag {
            layer,
            frame,
            handle,
            start,
            editable,
            received,
            turned: 0.0,
            values: Vec::new(),
            released: false,
        });
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
        if values != drag.values {
            drag.values = values;
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
        if self.transform_targets().is_none() {
            // The targets went stale mid-drag (a reload); nothing to write to.
            let message = self.transform.unavailable(self.selected_layer.as_deref());
            self.transform
                .set_error(self.selected_layer.as_deref(), Some(message));
            self.cancel_transform_drag();
            return true;
        }
        // A success reloads the code, which draws the written values; the
        // released drag stays until that reload so a redraw before it still
        // shows the dragged position. A failure (the source changed while
        // dragging, the history is busy) puts the layer back where it was.
        // The error shows in the Layer section and over the canvas.
        self.commit_transform(
            values,
            |this, result, cx| {
                // Targets still current after a success mean nothing was
                // written (the text came out the same), so no reload comes
                // to drop the drag.
                if result.is_err() || this.transform_targets().is_some() {
                    this.cancel_transform_drag();
                }
                cx.notify();
            },
            cx,
        );
        true
    }

    /// Escape during a drag: drops it and its override, writing nothing.
    /// True when a pressed drag was cancelled.
    pub(crate) fn escape_transform_drag(&mut self) -> bool {
        if !self.transform_drag_active() {
            return false;
        }
        self.cancel_transform_drag();
        true
    }

    /// Drops the drag and redraws the frame without the override.
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
