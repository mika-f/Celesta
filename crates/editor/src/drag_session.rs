//! The flow every timeline drag shares: a press starts it, pointer moves
//! snap and clamp its delta, the button coming up seeks (a click), cancels
//! (back where it started, or Escape) or writes it as one history entry, and
//! a released drag stays drawn until the reload of its write. What differs
//! between a band's and a chapter end's drag sits behind [`TimelineDrag`].
//! A drag released where the view did not see it ends through
//! [`EditorView::release_unseen`], the timeline's drags and the others alike.

use crate::edit_history::Texts;
use crate::timeline_drag::{
    BandNotice, DragEnd, drag_end, drag_frames, not_written, snapped_delta,
};
use crate::view::EditorView;
use gpui_kit::{Context, MouseMoveEvent};
use std::path::PathBuf;

/// What a released drag writes: the files it edits, and the edit.
pub(crate) type DragWrite = (
    Vec<PathBuf>,
    Box<dyn FnOnce(&Texts) -> Result<Vec<(PathBuf, String)>, String> + Send>,
);

/// A running (or released) timeline drag: what the pointer did, and the
/// drag's own state.
#[derive(Debug)]
pub(crate) struct DragSession<T> {
    pub(crate) gesture: T,
    /// Window x of the press.
    start_x: f32,
    /// The farthest the pointer went from `start_x`, in pixels.
    moved_px: f32,
    /// What the moved edges snap to, read at the press.
    snap_targets: Vec<f64>,
    /// The button came up: drawn while the write runs and, after a
    /// successful one, until its reload, so the drawing does not flash
    /// back.
    pub(crate) released: bool,
}

impl<T> DragSession<T> {
    /// A drag pressed at window x `start_x`.
    pub(crate) fn new(gesture: T, start_x: f32, snap_targets: Vec<f64>) -> Self {
        Self {
            gesture,
            start_x,
            moved_px: 0.0,
            snap_targets,
            released: false,
        }
    }
}

impl<T: DragGesture> DragSession<T> {
    /// Follows the pointer to window x `x`: the edges move by whole frames,
    /// snapped unless `shift`, within the drag's bounds. True when what is
    /// drawn changed.
    fn follow(&mut self, x: f32, pixels_per_frame: f64, shift: bool) -> bool {
        self.moved_px = self.moved_px.max((x - self.start_x).abs());
        let raw = drag_frames(self.start_x, x, pixels_per_frame);
        let delta = snapped_delta(
            &self.gesture.edges(),
            raw,
            &self.snap_targets,
            pixels_per_frame,
            shift,
        );
        self.gesture.follow(delta)
    }

    /// What the button coming up does (see [`drag_end`]).
    fn end(&self) -> DragEnd {
        drag_end(self.moved_px, self.gesture.delta(), false)
    }
}

/// What a drag moves: the edges that snap, its bounds, where a click seeks.
pub(crate) trait DragGesture {
    /// The frames of the edges the drag moves, as at the press.
    fn edges(&self) -> Vec<f64>;
    /// Whole frames, within the bounds.
    fn delta(&self) -> i64;
    /// Takes `delta` (snapped) within the bounds. True when what is drawn
    /// changed.
    fn follow(&mut self, delta: i64) -> bool;
    /// The frame a click seeks to, if any.
    fn seek(&self) -> Option<u64>;
    /// The frame the notice of a release that wrote nothing sits at.
    fn at(&self) -> f64;
}

/// Where a drag lives on the view, and what its release writes.
pub(crate) trait TimelineDrag: DragGesture + Sized + 'static {
    fn slot(view: &EditorView) -> &Option<DragSession<Self>>;
    fn slot_mut(view: &mut EditorView) -> &mut Option<DragSession<Self>>;
    /// Where the notice of a release that wrote nothing goes.
    fn notice_mut(view: &mut EditorView) -> &mut Option<BandNotice>;
    /// What writing the released drag takes; `None` when it would write
    /// nothing; or why it can't be written now.
    fn write(view: &EditorView) -> Result<Option<DragWrite>, String>;
    /// After a successful write: no reload comes to drop the drag (the
    /// text came out the same).
    fn written_nothing(view: &EditorView) -> bool;
}

impl EditorView {
    /// Whether a `D` drag runs with the button down.
    pub(crate) fn timeline_drag_active<D: TimelineDrag>(&self) -> bool {
        D::slot(self).as_ref().is_some_and(|drag| !drag.released)
    }

    /// Follows the pointer: moves the drag by whole frames, snapped unless
    /// Shift is held, within its bounds. Writes nothing.
    pub(crate) fn continue_timeline_drag<D: TimelineDrag>(
        &mut self,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) {
        if !self.timeline_drag_active::<D>()
            || self.release_unseen(event, cx, Self::end_timeline_drag::<D>)
        {
            return;
        }
        let pixels_per_frame = self.pixels_per_frame();
        let x = f32::from(event.position.x);
        let Some(drag) = D::slot_mut(self).as_mut() else {
            return;
        };
        if drag.follow(x, pixels_per_frame, event.modifiers.shift) {
            cx.notify();
        }
    }

    /// The button came up: a click seeks, a drag writes as one history
    /// entry, and a drag back to where it started writes nothing. True when
    /// a drag was running.
    pub(crate) fn end_timeline_drag<D: TimelineDrag>(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(drag) = D::slot(self).as_ref().filter(|drag| !drag.released) else {
            return false;
        };
        match drag.end() {
            DragEnd::Seek => {
                let seek = drag.gesture.seek();
                *D::slot_mut(self) = None;
                if let Some(frame) = seek {
                    self.seek_to_frame(frame, cx);
                }
            }
            DragEnd::Cancel => *D::slot_mut(self) = None,
            DragEnd::Commit => self.commit_timeline_drag::<D>(cx),
        }
        true
    }

    /// Escape (or playback starting) during a drag: drops it, writing
    /// nothing. True when a pressed drag was cancelled.
    pub(crate) fn escape_timeline_drag<D: TimelineDrag>(&mut self) -> bool {
        if !self.timeline_drag_active::<D>() {
            return false;
        }
        *D::slot_mut(self) = None;
        true
    }

    /// Writes the released drag: one that can't be written now, or a failed
    /// write, leaves the notice why beside it. The released drag stays drawn
    /// while the write runs and, after a successful one, until its reload
    /// (see [`Self::drop_released_timeline_drag`]).
    fn commit_timeline_drag<D: TimelineDrag>(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = D::slot_mut(self).as_mut() else {
            return;
        };
        drag.released = true;
        let at = drag.gesture.at();
        let (files, edit) = match D::write(self) {
            Ok(Some(write)) => write,
            Ok(None) => {
                *D::slot_mut(self) = None;
                return;
            }
            Err(text) => {
                *D::notice_mut(self) = Some(BandNotice { at, text });
                *D::slot_mut(self) = None;
                return;
            }
        };
        self.record_own_writes(
            files,
            edit,
            move |this, result, cx| {
                if let Err(text) = &result {
                    *D::notice_mut(this) = Some(BandNotice {
                        at,
                        text: not_written(text),
                    });
                }
                if result.is_err() || D::written_nothing(this) {
                    *D::slot_mut(this) = None;
                }
                cx.notify();
            },
            cx,
        );
    }

    /// The reload after a drag's write draws its values: the released drag
    /// goes (a pressed one goes on).
    pub(crate) fn drop_released_timeline_drag<D: TimelineDrag>(&mut self) {
        if !self.timeline_drag_active::<D>() {
            *D::slot_mut(self) = None;
        }
    }

    /// A pointer move without the button: it came up where the view did
    /// not see it. Ends the drag there with `end`, redrawing when one
    /// ended. True when the move was such a release, which the drag then
    /// does not follow.
    pub(crate) fn release_unseen(
        &mut self,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
        end: impl FnOnce(&mut Self, &mut Context<Self>) -> bool,
    ) -> bool {
        if event.dragging() {
            return false;
        }
        if end(self, cx) {
            cx.notify();
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One edge at frame 100, kept within ±10 frames; a click seeks to 7.
    #[derive(Debug, Default)]
    struct Edge {
        delta: i64,
    }

    impl DragGesture for Edge {
        fn edges(&self) -> Vec<f64> {
            vec![100.0]
        }

        fn delta(&self) -> i64 {
            self.delta
        }

        fn follow(&mut self, delta: i64) -> bool {
            let delta = delta.clamp(-10, 10);
            let moved = delta != self.delta;
            self.delta = delta;
            moved
        }

        fn seek(&self) -> Option<u64> {
            Some(7)
        }

        fn at(&self) -> f64 {
            100.0 + self.delta as f64
        }
    }

    /// Pressed at x 50, 2 px per frame, snapping to frame 120.
    fn session() -> DragSession<Edge> {
        DragSession::new(Edge::default(), 50.0, vec![120.0])
    }

    #[test]
    fn a_press_that_barely_moves_seeks() {
        // A frame on, but under the click distance.
        let mut drag = session();
        assert!(drag.follow(52.0, 2.0, false));
        assert_eq!(drag.end(), DragEnd::Seek);
    }

    #[test]
    fn a_drag_moves_by_whole_frames_within_the_bounds() {
        let mut drag = session();
        assert!(drag.follow(60.0, 2.0, false));
        assert_eq!(drag.gesture.delta, 5);
        assert!(drag.follow(200.0, 2.0, true));
        assert_eq!(drag.gesture.delta, 10);
        assert!(!drag.follow(400.0, 2.0, true));
        assert_eq!(drag.end(), DragEnd::Commit);
    }

    #[test]
    fn the_edge_snaps_unless_shift_is_held() {
        // 4 frames on, 1 frame (2 px) from the target at 105.
        let mut drag = DragSession::new(Edge::default(), 50.0, vec![105.0]);
        drag.follow(58.0, 2.0, false);
        assert_eq!(drag.gesture.delta, 5);
        drag.follow(58.0, 2.0, true);
        assert_eq!(drag.gesture.delta, 4);
    }

    #[test]
    fn a_drag_back_to_its_start_writes_nothing() {
        let mut drag = session();
        drag.follow(80.0, 2.0, false);
        drag.follow(50.0, 2.0, false);
        assert_eq!(drag.gesture.delta, 0);
        assert_eq!(drag.end(), DragEnd::Cancel);
    }
}
