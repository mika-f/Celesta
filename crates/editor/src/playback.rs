use crate::actions::{
    GoToEnd, GoToIn, GoToOut, GoToStart, JumpBackward, JumpForward, NextEditPoint, NextFrame,
    PausePlayback, PlayForward, PreviousEditPoint, PreviousFrame, ToggleInteractive, ToggleLoop,
    TogglePlayback, ToggleSafeAreas,
};
use crate::helpers::loop_range_for;
use crate::inline_text_view::EditEvent;
use crate::problems::{Source, error_chain};
use crate::view::EditorView;
use crate::{timecode, timeline};
use celesta_composition::Time;
use gpui_kit::{ClickEvent, Context, KeyContext, MouseMoveEvent, MouseUpEvent, Window};
use std::time::Instant;

/// Whether the focus, by its key contexts, is in a text field (a property
/// row, the props file's Properties). The playhead's arrow, Home, and End
/// actions do nothing then, however they come: macOS hands a key the field
/// does not use to the menu, whose items take those keys.
pub(crate) fn in_text_field(contexts: &[KeyContext]) -> bool {
    contexts.iter().any(|context| context.contains("Input"))
}

/// [`in_text_field`] for the window's focus.
fn typing(window: &Window) -> bool {
    in_text_field(&window.context_stack())
}

impl EditorView {
    pub(crate) fn pause(&mut self) {
        let was_playing = self.playing;
        self.playing = false;
        self.play_when_audio_ready = false;
        self.playback_started_at = None;
        if let Some(audio) = &self.audio_preview {
            audio.pause();
        }
        if was_playing {
            // The last frame was drawn in draft quality; show the final one.
            self.refresh_preview();
        }
    }

    pub(crate) fn seek_frame(&mut self, frame: i64) {
        // A seek by hand (playback seeks every frame) leaves the rows: their
        // typed text is submitted on the next render, as a blur would.
        if !self.playing {
            self.element_rows.submit_requested = true;
        }
        // A text edited on the canvas ends, as on a blur.
        if self.inline_text.is_some() {
            self.inline_end_due = Some(EditEvent::Seek);
        }
        self.clock.seek(frame);
        self.refresh_preview();
    }

    pub(crate) fn toggle_playback(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_playback_state();
        // GPUI only permits `request_animation_frame` during rendering. This
        // notification enters `render`, where `update_playback` requests it.
        cx.notify();
    }

    pub(crate) fn toggle_playback_state(&mut self) {
        if self.playing || self.play_when_audio_ready {
            self.pause();
        } else {
            self.start_playback();
        }
    }

    /// Starts playback, or — while the preview audio is still being prepared
    /// (a React entry's `<Audio>` sweep and the mix take a moment after
    /// opening) — waits for it, so the soundtrack starts with the picture
    /// instead of joining part-way through.
    pub(crate) fn start_playback(&mut self) {
        // Playing leaves the rows, as a seek does, and a text edited on the
        // canvas.
        self.element_rows.submit_requested = true;
        if self.inline_text.is_some() {
            self.inline_end_due = Some(EditEvent::Play);
        }
        if self.playing || self.clock.end_frame() <= 0 {
            return;
        }
        if self.audio_pending {
            self.play_when_audio_ready = true;
            return;
        }
        self.play_when_audio_ready = false;
        if self.clock.is_at_end() {
            // Looping restarts where the loop does, as the wrap in
            // `update_playback` would.
            let restart = if self.loop_playback {
                self.loop_range().0
            } else {
                0
            };
            self.seek_frame(restart);
        }
        // Playback draws the code's own frames: a drag or a scrub stops,
        // unwritten.
        self.transform_drag = None;
        self.element_rows.scrub = None;
        if self.band_drag_active() {
            self.band_drag = None;
        }
        self.playing = true;
        self.playback_started_at = Some(Instant::now());
        self.playback_started_frame = self.clock.frame();
        if let Some(audio) = &mut self.audio_preview
            && let Err(error) = audio.seek(self.clock.time().unwrap_or(Time::ZERO), true)
        {
            self.problems
                .fail(Source::Audio, error_chain(error.as_ref()));
            self.audio_preview = None;
        }
    }

    pub(crate) fn update_playback(&mut self, window: &mut Window) {
        if self.play_when_audio_ready && !self.audio_pending {
            self.start_playback();
        }
        if !self.playing {
            return;
        }
        let Some(started_at) = self.playback_started_at else {
            self.pause();
            return;
        };
        let frames_per_second = f64::from(self.frame_rate_value.numerator)
            / f64::from(self.frame_rate_value.denominator);
        let elapsed_frames = (started_at.elapsed().as_secs_f64() * frames_per_second) as i64;
        let target = self.playback_started_frame.saturating_add(elapsed_frames);
        let (loop_start, loop_end) = self.loop_range();
        if self.loop_playback && target >= loop_end {
            self.restart_playback_at(loop_start);
        } else if target != self.clock.frame() {
            self.seek_frame(target);
        }
        self.keep_playhead_visible();
        if self.clock.is_at_end() && !self.loop_playback {
            self.pause();
        } else {
            window.request_animation_frame();
        }
    }

    /// `[start, end)` frames looped playback repeats: the In/Out range when
    /// both marks are set in order, else the whole composition.
    pub(crate) fn loop_range(&self) -> (i64, i64) {
        loop_range_for(
            self.export_in_frame,
            self.export_out_frame,
            self.clock.end_frame(),
        )
    }

    /// Restarts the playback clock (and audio) from `frame` while playing.
    pub(crate) fn restart_playback_at(&mut self, frame: i64) {
        self.seek_frame(frame);
        self.playback_started_at = Some(Instant::now());
        self.playback_started_frame = self.clock.frame();
        let time = self.current_time();
        if let Some(audio) = &mut self.audio_preview
            && let Err(error) = audio.seek(time, true)
        {
            self.problems
                .fail(Source::Audio, error_chain(error.as_ref()));
            self.audio_preview = None;
        }
    }

    /// Frames from the playhead to the next (`forward`) or previous edit
    /// point: a clip start or end, or the composition's first or last frame.
    pub(crate) fn adjacent_edit_point(&self, forward: bool) -> Option<i64> {
        let clock = self.clock;
        let points = timeline::edit_points(
            self.timeline_tracks(),
            |time| clock.frame_for_time(time).ok(),
            clock.end_frame(),
        );
        let current = clock.frame();
        if forward {
            points.into_iter().find(|point| *point > current)
        } else {
            points.into_iter().rev().find(|point| *point < current)
        }
    }

    pub(crate) fn play_forward_action(
        &mut self,
        _: &PlayForward,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_playback();
        cx.notify();
    }

    pub(crate) fn pause_playback_action(
        &mut self,
        _: &PausePlayback,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pause();
        cx.notify();
    }

    pub(crate) fn jump_backward_action(
        &mut self,
        _: &JumpBackward,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if typing(window) {
            return;
        }
        self.step_frames(-timecode::nominal_fps(self.frame_rate_value), cx);
    }

    pub(crate) fn jump_forward_action(
        &mut self,
        _: &JumpForward,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if typing(window) {
            return;
        }
        self.step_frames(timecode::nominal_fps(self.frame_rate_value), cx);
    }

    pub(crate) fn go_to_start_action(
        &mut self,
        _: &GoToStart,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if typing(window) {
            return;
        }
        self.go_to_frame(0, cx);
    }

    pub(crate) fn go_to_end_action(
        &mut self,
        _: &GoToEnd,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if typing(window) {
            return;
        }
        self.go_to_frame(self.clock.end_frame(), cx);
    }

    pub(crate) fn previous_edit_point_action(
        &mut self,
        _: &PreviousEditPoint,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if typing(window) {
            return;
        }
        if let Some(frame) = self.adjacent_edit_point(false) {
            self.go_to_frame(frame, cx);
        }
    }

    pub(crate) fn next_edit_point_action(
        &mut self,
        _: &NextEditPoint,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if typing(window) {
            return;
        }
        if let Some(frame) = self.adjacent_edit_point(true) {
            self.go_to_frame(frame, cx);
        }
    }

    pub(crate) fn go_to_in_action(&mut self, _: &GoToIn, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(frame) = self.export_in_frame {
            self.go_to_frame(frame, cx);
        }
    }

    /// The Out mark is exclusive, so its last included frame is one before.
    pub(crate) fn go_to_out_action(&mut self, _: &GoToOut, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(frame) = self.export_out_frame {
            self.go_to_frame(frame.saturating_sub(1), cx);
        }
    }

    pub(crate) fn toggle_loop_action(
        &mut self,
        _: &ToggleLoop,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_loop(cx);
    }

    pub(crate) fn toggle_safe_areas_action(
        &mut self,
        _: &ToggleSafeAreas,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_safe_areas(cx);
    }

    pub(crate) fn toggle_interactive_action(
        &mut self,
        _: &ToggleInteractive,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_interactive(cx);
    }

    /// Pointer moves anywhere in the window continue whichever drag is in
    /// progress, so a drag that leaves its control keeps tracking.
    pub(crate) fn drag_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let position = event.position;
        let handled = if event.dragging() {
            self.continue_scrub(position) || self.continue_overview_drag(position)
        } else {
            false
        };
        let handled = self.continue_timeline_pan(position, event.pressed_button) || handled;
        if handled {
            cx.notify();
        }
        self.continue_master_volume_drag(event, window, cx);
        self.continue_transform_drag(event, cx);
        self.continue_band_drag(event, cx);
        self.continue_row_scrub(event, cx);
        self.continue_inline_selection(event, window, cx);
    }

    pub(crate) fn drag_end(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.end_inline_selection();
        let ended = self.end_scrub() | self.end_overview_drag();
        let ended = self.master_volume_drag.take().is_some() | ended;
        let ended = self.end_transform_drag(cx) | ended;
        let ended = self.end_band_drag(cx) | ended;
        let ended = self.end_row_scrub(cx) | ended;
        if ended {
            cx.notify();
        }
    }

    pub(crate) fn middle_drag_end(
        &mut self,
        _: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.timeline_pan.take().is_some() {
            cx.notify();
        }
    }
}

impl EditorView {
    pub(crate) fn toggle_playback_action(
        &mut self,
        _: &TogglePlayback,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_playback_state();
        cx.notify();
    }

    pub(crate) fn previous_frame_action(
        &mut self,
        _: &PreviousFrame,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if typing(window) {
            return;
        }
        self.step_frames(-1, cx);
    }

    pub(crate) fn next_frame_action(
        &mut self,
        _: &NextFrame,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if typing(window) {
            return;
        }
        self.step_frames(1, cx);
    }
}
