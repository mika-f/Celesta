//! The timeline: a timecode ruler, video tracks stacked above audio tracks in
//! compositing order, the playhead, and zoom/scroll over the composition.
//!
//! The visible window is `(zoom, view_start)`: `view_start` is the fraction
//! of the composition at the lane's leading edge and the lane shows
//! `1 / zoom` of it. Pointer positions map onto the lane through its painted
//! bounds, recorded each frame in `timeline_lane_bounds`.

use celesta_editor_core::{ClipSummary, TrackSummary};
use celesta_project::TrackKind;
use celesta_react_bridge::{ReactChapter, SourcePos, chapter_at, scenes_of};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, Selectable as _, Sizable as _,
};
use gpui_kit::{
    Bounds, Context, CursorStyle, Hsla, IntoElement, Modifiers, MouseButton, MouseDownEvent,
    ParentElement as _, PathBuilder, PinchEvent, Pixels, Point, ScrollDelta, ScrollWheelEvent,
    SharedString, TouchPhase, Window, canvas, div, fill, point, prelude::*, px, relative, rgb,
    size,
};

use crate::actions::{ZoomTimelineIn, ZoomTimelineOut, ZoomTimelineToFit};
use crate::icons::CelestaIcon;
use crate::impact::ImpactView;
use crate::layer_select::display_path;
use crate::scene_entries::SceneEdge;
use crate::timecode::{format_timecode, nominal_fps, ruler_scale};
use crate::timeline_drag::{BandGrip, BandGrips, draggable_band, edge_width};
use crate::timing::{SET_TIMING_NOTE, SetTimingOffer};
use crate::view::EditorView;
use crate::waveform::{level_at_time, waveform_segment};
use celesta_editor_theme as theme;

const KEY_CONTEXT: Option<&str> = Some("CelestaEditor");

pub(crate) const TRACK_HEADER_WIDTH: f32 = 220.0;
/// Space kept clear at the lane's trailing edge for the vertical scrollbar.
pub(crate) const LANE_GUTTER: f32 = 10.0;
pub(crate) const RULER_HEIGHT: f32 = 30.0;
const VIDEO_TRACK_HEIGHT: f32 = 40.0;
/// Height of one lane of the Chapters row.
const CHAPTER_LANE_HEIGHT: f32 = 22.0;
const AUDIO_TRACK_HEIGHT: f32 = 52.0;
const FOOTER_HEIGHT: f32 = 26.0;
/// The most zoomed-in view shows this many pixels per frame.
const MAX_PIXELS_PER_FRAME: f64 = 24.0;
/// Lane width assumed before the lane has been painted once.
const FALLBACK_LANE_WIDTH: f32 = 1000.0;
/// The zoom step for the `=`/`-` keys and the toolbar buttons.
const ZOOM_STEP: f64 = 1.5;

/// One timeline row: the NLE label (`V2`, `A1`) and its track.
pub(crate) struct TimelineRow<'a> {
    pub label: String,
    pub track: &'a TrackSummary,
}

/// Visual tracks (everything that draws, including dialogue) as `V1…`
/// listed topmost-first, the order they composite in; then audio tracks as
/// `A1…` in project order. `taken_visual` video numbers are already in use:
/// a React composition's own row is `V1`.
pub(crate) fn timeline_rows(
    tracks: &[TrackSummary],
    taken_visual: usize,
) -> (Vec<TimelineRow<'_>>, Vec<TimelineRow<'_>>) {
    let mut visual: Vec<_> = tracks
        .iter()
        .filter(|track| track.kind != TrackKind::Audio)
        .enumerate()
        .map(|(index, track)| TimelineRow {
            label: format!("V{}", taken_visual + index + 1),
            track,
        })
        .collect();
    visual.reverse();
    let audio = tracks
        .iter()
        .filter(|track| track.kind == TrackKind::Audio)
        .enumerate()
        .map(|(index, track)| TimelineRow {
            label: format!("A{}", index + 1),
            track,
        })
        .collect();
    (visual, audio)
}

/// Every clip start and end, plus the composition's first and last frame,
/// sorted and deduplicated.
pub(crate) fn edit_points(
    tracks: &[TrackSummary],
    frame_for: impl Fn(celesta_composition::Time) -> Option<i64>,
    end_frame: i64,
) -> Vec<i64> {
    let mut points = vec![0, end_frame];
    for clip in tracks.iter().flat_map(|track| &track.clips) {
        if let Some(start) = frame_for(clip.start) {
            points.push(start);
            if let Some(end) = clip
                .start
                .checked_add(clip.duration)
                .ok()
                .and_then(&frame_for)
            {
                points.push(end);
            }
        }
    }
    points.retain(|point| (0..=end_frame).contains(point));
    points.sort_unstable();
    points.dedup();
    points
}

/// Horizontal geometry of the visible window, shared by everything that
/// positions itself along the lane.
#[derive(Clone, Copy)]
pub(crate) struct LaneView {
    pub(crate) zoom: f64,
    pub(crate) view_start: f64,
    pub(crate) end_frame: f64,
}

impl LaneView {
    /// Lane fraction (0 at the leading edge, 1 at the trailing edge) of a
    /// whole-composition fraction.
    pub(crate) fn x(self, fraction: f64) -> f32 {
        ((fraction - self.view_start) * self.zoom) as f32
    }

    pub(crate) fn width(self, fraction: f64) -> f32 {
        (fraction * self.zoom) as f32
    }

    fn frame_fraction(self, frame: i64) -> f64 {
        frame as f64 / self.end_frame
    }

    fn time_fraction(self, time: celesta_composition::Time, total_seconds: f64) -> f64 {
        if total_seconds <= 0.0 {
            return 0.0;
        }
        (time.as_seconds().unwrap_or(0.0) / total_seconds).clamp(0.0, 1.0)
    }
}

/// The lane each chapter goes on in the Chapters row: the first lane whose
/// chapters all end by the time it starts. `chapters` is ordered by `from`,
/// as the runtime reports it, so a part listed after the scene holding it
/// lands below that scene. A mark of length 0 takes a frame, so two marks or
/// a mark and a chapter on the same frame do not share a lane.
fn chapter_lanes(chapters: &[ReactChapter]) -> Vec<usize> {
    let mut lane_ends: Vec<u64> = Vec::new();
    chapters
        .iter()
        .map(|chapter| {
            let end = chapter.end().max(chapter.from + 1);
            match lane_ends
                .iter()
                .position(|lane_end| *lane_end <= chapter.from)
            {
                Some(lane) => {
                    lane_ends[lane] = end;
                    lane
                }
                None => {
                    lane_ends.push(end);
                    lane_ends.len() - 1
                }
            }
        })
        .collect()
}

/// Left and width of each `[from, to)` frame range as lane fractions, dropping
/// ranges outside the visible window. A range that runs past an edge keeps
/// its full extent; the lane clips it.
fn band_geometry(ranges: &[(u64, u64)], view: LaneView) -> Vec<(f32, f32)> {
    let view_end = view.view_start + 1.0 / view.zoom;
    ranges
        .iter()
        .filter(|(from, to)| from < to)
        .map(|&(from, to)| (from as f64 / view.end_frame, to as f64 / view.end_frame))
        .filter(|&(start, end)| end > view.view_start && start < view_end)
        .map(|(start, end)| (view.x(start), view.width(end - start)))
        .collect()
}

/// One band of the selection on the composition lane.
struct SelectionBand {
    range: (u64, u64),
    /// Filled for this instance in the current scene (or the span before the
    /// scan is ready); outlined for every other occurrence.
    filled: bool,
    /// The instance's index, `None` for the span.
    instance: Option<usize>,
    call_site: Option<SourcePos>,
    /// How many of the instance's ranges the band covers: more than one once
    /// [`merge_close_bands`] joined them.
    ranges: usize,
}

/// `bands` with each run of one instance's bands (filled or outlined alike)
/// less than a pixel apart at `view` on a lane `lane_width` pixels wide
/// joined into one, so an element shown every other frame draws a band per
/// pixel at most, not one per range. Bands of one kind come in frame order,
/// as [`selection_bands`] lists them.
fn merge_close_bands(
    bands: Vec<SelectionBand>,
    view: LaneView,
    lane_width: f32,
) -> Vec<SelectionBand> {
    // The frames one pixel of the lane shows.
    let frames_per_pixel = view.end_frame / (view.zoom * f64::from(lane_width));
    let mut merged: Vec<SelectionBand> = Vec::with_capacity(bands.len());
    for band in bands {
        if let Some(last) = merged.last_mut()
            && last.filled == band.filled
            && last.instance == band.instance
            && (band.range.0.saturating_sub(last.range.1) as f64) < frames_per_pixel
        {
            last.range.1 = last.range.1.max(band.range.1);
            last.ranges += band.ranges;
            continue;
        }
        merged.push(band);
    }
    merged
}

/// The bands `impact` draws: this instance's ranges in the current scene, or
/// the span rounded outward to whole frames, then every other occurrence.
fn selection_bands(impact: &ImpactView) -> Vec<SelectionBand> {
    match impact {
        ImpactView::SpanOnly { span, .. } => span
            .iter()
            .map(|&(from, to)| SelectionBand {
                range: (from.floor() as u64, to.ceil() as u64),
                filled: true,
                instance: None,
                call_site: None,
                ranges: 1,
            })
            .collect(),
        ImpactView::Known(impact) => {
            let band = |filled: bool| {
                move |&(range, instance): &((u64, u64), usize)| SelectionBand {
                    range,
                    filled,
                    instance: Some(instance),
                    call_site: impact.call_sites.get(instance).cloned(),
                    ranges: 1,
                }
            };
            impact
                .own_bands
                .iter()
                .map(band(true))
                .chain(impact.other_bands.iter().map(band(false)))
                .collect()
        }
    }
}

impl EditorView {
    pub(crate) fn lane_view(&self) -> LaneView {
        let (zoom, view_start) = self.timeline_view();
        LaneView {
            zoom,
            view_start,
            end_frame: self.clock.end_frame().max(1) as f64,
        }
    }

    pub(crate) fn lane_bounds(&self) -> Option<Bounds<Pixels>> {
        self.timeline_lane_bounds.get()
    }

    pub(crate) fn lane_width(&self) -> f32 {
        self.lane_bounds()
            .map_or(FALLBACK_LANE_WIDTH, |bounds| f32::from(bounds.size.width))
            .max(1.0)
    }

    /// Window x of composition frame `frame` on the lanes, which may lie
    /// outside them.
    pub(crate) fn frame_x(&self, frame: f64) -> f32 {
        let left = self
            .lane_bounds()
            .map_or(TRACK_HEADER_WIDTH, |bounds| f32::from(bounds.origin.x));
        let view = self.lane_view();
        left + view.x(frame / view.end_frame) * self.lane_width()
    }

    /// How many pixels one frame takes on the lanes at the current zoom.
    pub(crate) fn pixels_per_frame(&self) -> f64 {
        let (zoom, _) = self.timeline_view();
        f64::from(self.lane_width()) * zoom / self.clock.end_frame().max(1) as f64
    }

    /// Composition fraction under window position `x`.
    fn timeline_fraction_at(&self, x: Pixels) -> f64 {
        let (left, width) = self
            .lane_bounds()
            .map_or((TRACK_HEADER_WIDTH, FALLBACK_LANE_WIDTH), |bounds| {
                (f32::from(bounds.origin.x), f32::from(bounds.size.width))
            });
        let local = ((f32::from(x) - left) / width.max(1.0)).clamp(0.0, 1.0) as f64;
        let (zoom, view_start) = self.timeline_view();
        (view_start + local / zoom).clamp(0.0, 1.0)
    }

    fn frame_for_timeline_position(&self, position: Point<Pixels>) -> i64 {
        self.clock
            .frame_at_fraction(self.timeline_fraction_at(position.x) as f32)
    }

    fn max_timeline_zoom(&self) -> f64 {
        (self.clock.end_frame() as f64 * MAX_PIXELS_PER_FRAME / f64::from(self.lane_width()))
            .max(1.0)
    }

    /// Clamped `(zoom, view_start)`: zoom is at least 1, and the visible
    /// window `[view_start, view_start + 1/zoom]` stays inside `[0, 1]`.
    pub(crate) fn timeline_view(&self) -> (f64, f64) {
        let zoom = self.timeline_zoom.clamp(1.0, self.max_timeline_zoom());
        let view_start = self
            .timeline_view_start
            .clamp(0.0, (1.0 - 1.0 / zoom).max(0.0));
        (zoom, view_start)
    }

    /// Zooms by `factor`, keeping composition fraction `anchor` at the same
    /// place on the lane.
    fn zoom_timeline_about(&mut self, anchor: f64, factor: f64) {
        let (zoom, view_start) = self.timeline_view();
        let local = ((anchor - view_start) * zoom).clamp(0.0, 1.0);
        let new_zoom = (zoom * factor).clamp(1.0, self.max_timeline_zoom());
        self.timeline_zoom = new_zoom;
        self.timeline_view_start = (anchor - local / new_zoom).clamp(0.0, 1.0);
    }

    fn playhead_fraction(&self) -> f64 {
        self.clock.frame() as f64 / self.clock.end_frame().max(1) as f64
    }

    /// Zooms about the playhead when it is in view, else about the center.
    fn zoom_timeline_by(&mut self, factor: f64, cx: &mut Context<Self>) {
        let (zoom, view_start) = self.timeline_view();
        let playhead = self.playhead_fraction();
        let anchor = if (view_start..=view_start + 1.0 / zoom).contains(&playhead) {
            playhead
        } else {
            view_start + 0.5 / zoom
        };
        self.zoom_timeline_about(anchor, factor);
        cx.notify();
    }

    fn zoom_timeline_to_fit(&mut self, cx: &mut Context<Self>) {
        self.timeline_zoom = 1.0;
        self.timeline_view_start = 0.0;
        cx.notify();
    }

    pub(crate) fn zoom_timeline_in_action(
        &mut self,
        _: &ZoomTimelineIn,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.zoom_timeline_by(ZOOM_STEP, cx);
    }

    pub(crate) fn zoom_timeline_out_action(
        &mut self,
        _: &ZoomTimelineOut,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.zoom_timeline_by(1.0 / ZOOM_STEP, cx);
    }

    pub(crate) fn zoom_timeline_to_fit_action(
        &mut self,
        _: &ZoomTimelineToFit,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.zoom_timeline_to_fit(cx);
    }

    /// Pages the view so the playhead stays on screen, as an NLE does while
    /// playing or jumping.
    pub(crate) fn keep_playhead_visible(&mut self) {
        let (zoom, view_start) = self.timeline_view();
        let playhead = self.playhead_fraction();
        if playhead < view_start || playhead > view_start + 1.0 / zoom {
            self.timeline_view_start = playhead;
        }
    }

    fn pan_timeline_pixels(&mut self, delta_pixels: f32) {
        let (zoom, view_start) = self.timeline_view();
        let delta = f64::from(delta_pixels) / f64::from(self.lane_width()) / zoom;
        self.timeline_view_start = (view_start - delta).clamp(0.0, 1.0);
    }

    /// Wheel over the ruler: vertical zooms about the pointer, horizontal pans.
    fn ruler_wheel(&mut self, event: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        let delta = wheel_pixels(event.delta);
        if delta.y != 0.0 {
            let anchor = self.timeline_fraction_at(event.position.x);
            self.zoom_timeline_about(anchor, wheel_zoom_factor(delta.y));
        }
        if delta.x != 0.0 {
            self.pan_timeline_pixels(delta.x);
        }
        cx.stop_propagation();
        cx.notify();
    }

    /// Moves the playhead as far along the lane as `delta_pixels`, then
    /// scrolls the view just enough to keep it in sight.
    fn scrub_timeline_pixels(&mut self, delta_pixels: f32) {
        let (zoom, view_start) = self.timeline_view();
        let end_frame = self.clock.end_frame();
        let pixels_per_frame = self.pixels_per_frame();
        let frames = scrub_frames(
            delta_pixels,
            pixels_per_frame,
            &mut self.timeline_scrub_remainder,
        );
        self.pause();
        let frame = self
            .clock
            .frame()
            .saturating_add(frames)
            .clamp(0, end_frame);
        if frame != self.clock.frame() {
            self.seek_frame(frame);
        }
        self.timeline_view_start = follow_playhead(view_start, zoom, self.playhead_fraction());
    }

    /// Scrolls the tracks by `delta_pixels`, the way the scroll container
    /// itself would.
    fn scroll_tracks_pixels(&mut self, delta_pixels: f32) {
        let offset = self.timeline_tracks_scroll.offset();
        let max = self.timeline_tracks_scroll.max_offset();
        let y = (offset.y + px(delta_pixels)).clamp(-max.y, px(0.0));
        self.timeline_tracks_scroll.set_offset(point(offset.x, y));
    }

    /// Wheel over the clip lanes: a horizontal swipe (or Shift with the
    /// wheel) scrubs the playhead, Option pans, Ctrl/Cmd zooms, and plain
    /// vertical scrolling falls through to scroll the tracks.
    fn lane_wheel(&mut self, event: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        if event.touch_phase == TouchPhase::Started {
            self.timeline_scrub_remainder = 0.0;
        }
        let action = self.timeline_lane_gesture.wheel(
            event.touch_phase,
            wheel_pixels(event.delta),
            matches!(event.delta, ScrollDelta::Pixels(_)),
            event.modifiers,
        );
        match action {
            LaneWheel::Scrub(amount) => self.scrub_timeline_pixels(amount),
            LaneWheel::Pan(delta) => {
                self.pan_timeline_pixels(delta.x);
                self.scroll_tracks_pixels(delta.y);
            }
            LaneWheel::Zoom(delta_y) => {
                let anchor = self.timeline_fraction_at(event.position.x);
                self.zoom_timeline_about(anchor, wheel_zoom_factor(delta_y));
            }
            LaneWheel::Ignore => {}
            LaneWheel::Scroll => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    /// Pinch over the clip lanes: zoom about the pinch center.
    fn lane_pinch(&mut self, event: &PinchEvent, _: &mut Window, cx: &mut Context<Self>) {
        if event.delta == 0.0 {
            return;
        }
        let anchor = self.timeline_fraction_at(event.position.x);
        self.zoom_timeline_about(anchor, (1.0 + f64::from(event.delta)).clamp(0.5, 2.0));
        cx.stop_propagation();
        cx.notify();
    }

    /// Middle-button press over the timeline: start a pan.
    fn begin_timeline_pan(
        &mut self,
        event: &MouseDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.timeline_pan = Some((f32::from(event.position.x), self.timeline_view().1));
        cx.stop_propagation();
    }

    pub(crate) fn continue_timeline_pan(
        &mut self,
        position: Point<Pixels>,
        pressed: Option<MouseButton>,
    ) -> bool {
        let Some((grab_x, grab_view_start)) = self.timeline_pan else {
            return false;
        };
        // `MouseMoveEvent::dragging()` is Left-button only, so check the middle
        // button explicitly; the button being released ends the pan.
        if pressed != Some(MouseButton::Middle) {
            self.timeline_pan = None;
            return true;
        }
        let (zoom, _) = self.timeline_view();
        let dx = f64::from(f32::from(position.x) - grab_x) / f64::from(self.lane_width());
        self.timeline_view_start = (grab_view_start - dx / zoom).clamp(0.0, 1.0);
        true
    }

    fn scrub_to(&mut self, position: Point<Pixels>) {
        self.pause();
        let frame = self.frame_for_timeline_position(position);
        if frame != self.clock.frame() {
            self.seek_frame(frame);
        }
    }

    fn begin_scrub(&mut self, event: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.scrubbing = true;
        self.scrub_to(event.position);
        cx.notify();
    }

    pub(crate) fn continue_scrub(&mut self, position: Point<Pixels>) -> bool {
        if !self.scrubbing {
            return false;
        }
        self.scrub_to(position);
        true
    }

    pub(crate) fn end_scrub(&mut self) -> bool {
        std::mem::take(&mut self.scrubbing)
    }

    /// Press on the overview bar: grab the thumb, or jump the view so the
    /// pressed point is centered and grab it there.
    fn begin_overview_drag(
        &mut self,
        event: &MouseDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (zoom, view_start) = self.timeline_view();
        let Some(bounds) = self.timeline_overview_bounds.get() else {
            return;
        };
        let pressed = ((f32::from(event.position.x) - f32::from(bounds.origin.x))
            / f32::from(bounds.size.width).max(1.0)) as f64;
        if !(view_start..=view_start + 1.0 / zoom).contains(&pressed) {
            self.timeline_view_start = pressed - 0.5 / zoom;
        }
        self.timeline_overview_drag = Some((f32::from(event.position.x), self.timeline_view().1));
        cx.stop_propagation();
        cx.notify();
    }

    pub(crate) fn continue_overview_drag(&mut self, position: Point<Pixels>) -> bool {
        let Some((grab_x, grab_view_start)) = self.timeline_overview_drag else {
            return false;
        };
        let width = self
            .timeline_overview_bounds
            .get()
            .map_or(FALLBACK_LANE_WIDTH, |bounds| f32::from(bounds.size.width))
            .max(1.0);
        let delta = f64::from((f32::from(position.x) - grab_x) / width);
        self.timeline_view_start = (grab_view_start + delta).clamp(0.0, 1.0);
        true
    }

    pub(crate) fn end_overview_drag(&mut self) -> bool {
        self.timeline_overview_drag.take().is_some()
    }

    pub(crate) fn timeline(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let view = self.lane_view();
        let react = self.is_react_preview();
        let (visual, audio) = timeline_rows(self.timeline_tracks(), usize::from(react));
        let border = cx.theme().border;
        let tracks_empty = visual.is_empty() && audio.is_empty();
        // `None` while nothing is selected, so the highlight clears with it.
        // A band drag moves the bands it moves.
        let impact = self.timeline_impact();
        let mut rows: Vec<gpui_kit::AnyElement> = Vec::new();
        if let Some(chapters) = self.react_chapters() {
            // A scene drag moves the chapters it moves.
            let chapters = self.drawn_chapters(chapters);
            rows.push(
                self.chapter_row(&chapters, impact.as_ref(), view, cx)
                    .into_any_element(),
            );
        }
        rows.extend(visual.iter().map(|row| {
            self.track_row(row, VIDEO_TRACK_HEIGHT, view, cx)
                .into_any_element()
        }));
        // A React composition's own row is V1, under its videos.
        if react {
            let label_above = self.react_chapters().is_some() || !visual.is_empty();
            rows.push(
                self.react_composition_row(impact.as_ref(), view, label_above, cx)
                    .into_any_element(),
            );
        }
        if (self.is_react_preview() || !visual.is_empty()) && !audio.is_empty() {
            rows.push(
                div()
                    .flex_none()
                    .w_full()
                    .h(px(3.0))
                    .bg(border)
                    .into_any_element(),
            );
        }
        rows.extend(audio.iter().map(|row| {
            self.track_row(row, AUDIO_TRACK_HEIGHT, view, cx)
                .into_any_element()
        }));
        if !rows.is_empty() {
            // The space below the last track takes the same lane gestures.
            rows.push(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(div().flex_none().w(px(TRACK_HEADER_WIDTH)))
                    .child(
                        div()
                            .id("timeline-empty-lane")
                            .flex_1()
                            .mr(px(LANE_GUTTER))
                            .on_scroll_wheel(cx.listener(Self::lane_wheel))
                            .on_pinch(cx.listener(Self::lane_pinch))
                            .on_mouse_down(
                                MouseButton::Middle,
                                cx.listener(Self::begin_timeline_pan),
                            ),
                    )
                    .into_any_element(),
            );
        }
        let track_area = div()
            .id("timeline-tracks-scroll")
            .track_scroll(&self.timeline_tracks_scroll)
            .flex()
            .flex_col()
            .size_full()
            .overflow_x_hidden()
            .overflow_y_scroll()
            .when(tracks_empty && !self.is_react_preview(), |area| {
                area.child(
                    div()
                        .flex()
                        .flex_1()
                        .items_center()
                        .justify_center()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("This project has no tracks"),
                )
            })
            .children(rows);
        div()
            .id("timeline-panel")
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .h_full()
            .bg(cx.theme().secondary)
            .child(self.ruler_row(view, cx))
            .child(
                // The playhead and In/Out shading are drawn over the lanes by
                // a plain (non-interactive) layer, so clip clicks pass
                // through to the clips beneath.
                div().relative().flex_1().min_h_0().child(track_area).child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(px(TRACK_HEADER_WIDTH))
                        .right(px(LANE_GUTTER))
                        .overflow_hidden()
                        .when_some(self.export_range_fractions(), |layer, (start, end)| {
                            layer.child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .bottom_0()
                                    .left(relative(view.x(start)))
                                    .w(relative(view.width(end - start)))
                                    .bg(theme::export_range_fill().opacity(0.35)),
                            )
                        })
                        .child(
                            div()
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .left(relative(view.x(self.playhead_fraction())))
                                .w(px(1.0))
                                .bg(theme::accent()),
                        ),
                ),
            )
            .child(self.timeline_footer(view, cx))
    }

    /// The tracks the timeline lists: the project's, or a standalone React
    /// composition's videos and audio laid out on tracks.
    pub(crate) fn timeline_tracks(&self) -> &[TrackSummary] {
        if self.is_react_preview() {
            &self.react_tracks
        } else {
            &self.tracks
        }
    }

    /// `[start, end)` composition fractions of the In/Out marks when both are
    /// set in order.
    fn export_range_fractions(&self) -> Option<(f64, f64)> {
        let (start, end) = self.export_in_frame.zip(self.export_out_frame)?;
        let span = self.clock.end_frame().max(1) as f64;
        (start < end).then(|| {
            (
                (start as f64 / span).clamp(0.0, 1.0),
                (end as f64 / span).clamp(0.0, 1.0),
            )
        })
    }

    fn ruler_row(&self, view: LaneView, cx: &mut Context<Self>) -> impl IntoElement {
        let frame_rate = self.frame_rate_value;
        let fps = nominal_fps(frame_rate);
        let lane_width = self.lane_width();
        let pixels_per_frame = f64::from(lane_width) * view.zoom / view.end_frame;
        let scale = ruler_scale(pixels_per_frame, fps, 104.0, 7.0);
        let first_frame = (view.view_start * view.end_frame).floor() as i64;
        let last_frame = ((view.view_start + 1.0 / view.zoom) * view.end_frame).ceil() as i64;
        let ticks: Vec<(f32, bool)> = (first_frame.div_euclid(scale.minor)
            ..=last_frame.div_euclid(scale.minor) + 1)
            .map(|index| index * scale.minor)
            .filter(|frame| (0..=self.clock.end_frame()).contains(frame))
            .map(|frame| (view.x(view.frame_fraction(frame)), frame % scale.major == 0))
            .filter(|(x, _)| (-0.01..=1.01).contains(x))
            .collect();
        let labels = ticks
            .iter()
            .filter(|(_, major)| *major)
            .map(|(x, _)| *x)
            .map(|x| {
                let frame =
                    ((view.view_start + f64::from(x) / view.zoom) * view.end_frame).round() as i64;
                div()
                    .absolute()
                    .top(px(3.0))
                    .left(relative(x))
                    .ml(px(4.0))
                    .text_size(px(10.0))
                    .text_color(cx.theme().muted_foreground)
                    .whitespace_nowrap()
                    .child(format_timecode(frame, frame_rate))
            })
            .collect::<Vec<_>>();
        let tick_color = cx.theme().muted_foreground.opacity(0.6);
        let lane_bounds = self.timeline_lane_bounds.clone();
        let playhead_x = view.x(self.playhead_fraction());
        let range = self.export_range_fractions();
        let marks = [self.export_in_frame, self.export_out_frame]
            .map(|frame| frame.map(|frame| view.x(view.frame_fraction(frame))));
        let ruler = div()
            .id("timeline-ruler")
            .relative()
            .flex_1()
            .h_full()
            .mr(px(LANE_GUTTER))
            .overflow_hidden()
            .cursor(CursorStyle::ResizeLeftRight)
            .on_scroll_wheel(cx.listener(Self::ruler_wheel))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::begin_scrub))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::begin_timeline_pan))
            .when_some(range, |ruler, (start, end)| {
                ruler.child(
                    div()
                        .absolute()
                        .bottom_0()
                        .h(px(6.0))
                        .left(relative(view.x(start)))
                        .w(relative(view.width(end - start)))
                        .bg(theme::export_range_fill())
                        .border_t_1()
                        .border_color(theme::export_range_border()),
                )
            })
            .children(marks.into_iter().flatten().map(|x| {
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(relative(x))
                    .w(px(1.0))
                    .bg(theme::export_range_border())
            }))
            .child(
                canvas(
                    move |bounds, _, _| lane_bounds.set(Some(bounds)),
                    move |bounds, _, window, _| {
                        paint_ruler(bounds, &ticks, playhead_x, tick_color, window)
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .children(labels);
        div()
            .flex()
            .flex_none()
            .h(px(RULER_HEIGHT))
            .w_full()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .w(px(TRACK_HEADER_WIDTH))
                    .h_full()
                    .px_3()
                    .border_r_1()
                    .border_color(cx.theme().border)
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_sm()
                    .text_color(theme::accent())
                    .child(format_timecode(self.clock.frame(), frame_rate)),
            )
            .child(ruler)
    }

    fn track_header(&self, row: &TimelineRow<'_>, cx: &mut Context<Self>) -> impl IntoElement {
        let track = row.track;
        let muted = cx.theme().muted_foreground;
        let current_time = self.current_time();
        let track_level = track
            .clips
            .iter()
            .filter_map(|clip| {
                self.clip_levels
                    .get(&clip.id)
                    .map(|levels| level_at_time(levels, clip, current_time))
            })
            .fold(0.0_f32, f32::max)
            .sqrt()
            .clamp(0.0, 1.0);
        let mute_track_id = track.id.clone();
        let solo_track_id = track.id.clone();
        let state_icon = |id: String, icon: Icon, tooltip: &'static str| {
            div()
                .id(SharedString::from(id))
                .flex_none()
                .tooltip(move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(tooltip).build(window, cx)
                })
                .child(icon.size_3().text_color(muted))
        };
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap_2()
            .w(px(TRACK_HEADER_WIDTH))
            .h_full()
            .pl_3()
            .pr_2()
            .border_r_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex_none()
                    .w(px(22.0))
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_xs()
                    .text_color(muted)
                    .child(row.label.clone()),
            )
            .child(
                div()
                    .flex_none()
                    .size(px(8.0))
                    .rounded_sm()
                    .bg(rgb(theme::clip_fill(track.kind))),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_sm()
                    .text_color(if track.enabled {
                        cx.theme().foreground
                    } else {
                        muted
                    })
                    .child(track.name.clone()),
            )
            .when(!track.enabled, |header| {
                header.child(state_icon(
                    format!("track-disabled-{}", track.id),
                    Icon::new(IconName::EyeOff),
                    "Disabled in the project",
                ))
            })
            .when(track.locked, |header| {
                header.child(state_icon(
                    format!("track-locked-{}", track.id),
                    Icon::new(CelestaIcon::Lock),
                    "Locked in the project",
                ))
            })
            // A React video's sound plays, and meters, on its audio track.
            .when(
                track.kind != TrackKind::Overlay
                    && !(self.is_react_preview() && track.kind != TrackKind::Audio),
                |header| {
                    header.child(
                        div()
                            .flex_none()
                            .w(px(4.0))
                            .h(px(18.0))
                            .rounded_sm()
                            .overflow_hidden()
                            .flex()
                            .flex_col()
                            .justify_end()
                            .bg(cx.theme().background)
                            .child(
                                div()
                                    .w_full()
                                    .h(relative(track_level))
                                    .bg(theme::meter(track_level)),
                            ),
                    )
                },
            )
            // Mute and solo change the project; a React composition's tracks
            // are only a view of its source.
            .when(
                track.kind != TrackKind::Overlay && !self.is_react_preview(),
                |header| {
                    header
                        .child(
                            Button::new(SharedString::from(format!("track-mute-{}", track.id)))
                                .xsmall()
                                .ghost()
                                .label("M")
                                .selected(track.muted)
                                .tooltip("Mute in the preview")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.toggle_track_mute(&mute_track_id, cx);
                                })),
                        )
                        .child(
                            Button::new(SharedString::from(format!("track-solo-{}", track.id)))
                                .xsmall()
                                .ghost()
                                .label("S")
                                .selected(track.solo)
                                .tooltip("Solo in the preview")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.toggle_track_solo(&solo_track_id, cx);
                                })),
                        )
                },
            )
    }

    fn track_row(
        &self,
        row: &TimelineRow<'_>,
        height: f32,
        view: LaneView,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let track = row.track;
        let selected = self.selected_track_id.as_deref() == Some(track.id.as_str());
        let select_track_id = track.id.clone();
        let total_seconds = self.document.duration().as_seconds().unwrap_or(0.0);
        let clips = track
            .clips
            .iter()
            .map(|clip| {
                self.clip_block(clip, track.kind, total_seconds, view, cx)
                    .into_any_element()
            })
            .collect::<Vec<_>>();
        div()
            .id(SharedString::from(format!("timeline-track-{}", track.id)))
            .flex()
            .flex_none()
            .h(px(height))
            .w_full()
            .border_b_1()
            .border_color(cx.theme().border)
            .when(selected, |row| row.bg(cx.theme().list_active))
            .when(!track.enabled, |row| row.opacity(0.6))
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.selected_track_id.as_deref() == Some(select_track_id.as_str()) {
                    this.selected_track_id = None;
                } else {
                    this.selected_track_id = Some(select_track_id.clone());
                    this.selected_asset_id = None;
                }
                cx.notify();
            }))
            .child(self.track_header(row, cx))
            .child(
                div()
                    .id(SharedString::from(format!("clip-lane-{}", track.id)))
                    .relative()
                    .flex_1()
                    .h_full()
                    .mr(px(LANE_GUTTER))
                    .overflow_hidden()
                    .on_scroll_wheel(cx.listener(Self::lane_wheel))
                    .on_pinch(cx.listener(Self::lane_pinch))
                    .on_mouse_down(MouseButton::Middle, cx.listener(Self::begin_timeline_pan))
                    .children(clips),
            )
    }

    fn clip_block(
        &self,
        clip: &ClipSummary,
        kind: TrackKind,
        total_seconds: f64,
        view: LaneView,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let start = view.time_fraction(clip.start, total_seconds);
        let duration = view.time_fraction(clip.duration, total_seconds);
        let clip_id = clip.id.clone();
        let selected = self.selected_clip_id.as_deref() == Some(clip.id.as_str());
        let label_color = theme::waveform();
        // Keep the name readable when the clip starts left of the view.
        let label_offset = if duration > 0.0 {
            ((view.view_start - start) / duration).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        let clip_pixels = view.width(duration) * self.lane_width();
        let waveform = self.clip_waveforms.get(&clip.id).map(|peaks| {
            let bars = ((clip_pixels / 3.0) as usize).clamp(1, peaks.len().max(1));
            waveform_segment(peaks, 0.0, 1.0, bars)
        });
        div()
            .id(SharedString::from(format!("timeline-clip-{clip_id}")))
            .absolute()
            .left(relative(view.x(start)))
            .top(px(3.0))
            .bottom(px(3.0))
            .w(relative(view.width(duration)))
            .min_w(px(2.0))
            .overflow_hidden()
            .rounded_sm()
            .bg(rgb(theme::clip_fill(kind)))
            .when(selected, |block| {
                block.border_2().border_color(theme::clip_selected_border())
            })
            .when(!clip.enabled, |clip| clip.opacity(0.4))
            .when_some(waveform.filter(|bars| !bars.is_empty()), |block, bars| {
                block.child(
                    canvas(
                        |_, _, _| (),
                        move |bounds, _, window, _| {
                            paint_waveform(bounds, &bars, label_color.opacity(0.45), window)
                        },
                    )
                    .absolute()
                    .top(px(14.0))
                    .bottom(px(2.0))
                    .left_0()
                    .right_0(),
                )
            })
            .child(
                div()
                    .absolute()
                    .top(px(1.0))
                    .left(relative(label_offset))
                    .px_1()
                    .text_xs()
                    .text_color(label_color)
                    .whitespace_nowrap()
                    .child(clip.name.clone()),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.selected_clip_id = Some(clip_id.clone());
                this.selected_asset_id = None;
                cx.notify();
            }))
    }

    /// The single row a standalone React composition shows: it has no
    /// project tracks, but the ruler, playhead, and In/Out still apply. Over
    /// the composition block, the selection's bands: filled where an edit
    /// lands in this scene, outlined everywhere else it shows.
    fn react_composition_row(
        &self,
        impact: Option<&ImpactView>,
        view: LaneView,
        label_above: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entry = self
            .react_preview
            .as_ref()
            .and_then(|react| react.entry.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let bands = impact.map(selection_bands).unwrap_or_default();
        let bands = merge_close_bands(bands, view, self.lane_width());
        let ranges: Vec<(f64, f64)> = bands
            .iter()
            .map(|band| (band.range.0 as f64, band.range.1 as f64))
            .collect();
        // This instance's bands in the current scene come first.
        let filled = bands.iter().take_while(|band| band.filled).count();
        let grips = self.band_grips(&ranges[..filled]);
        let bands = bands
            .into_iter()
            .filter_map(|band| {
                let (left, width) = *band_geometry(&[band.range], view).first()?;
                Some(self.selection_band(band, left, width, cx))
            })
            .collect::<Vec<_>>();
        let label = grips
            .as_ref()
            .and_then(|grips| grips.label.clone())
            .map(|(at, lines)| (at, lines, false))
            .or_else(|| {
                let notice = self.band_notice.as_ref()?;
                Some((notice.at, vec![notice.text.clone()], true))
            });
        // Without a window of its own, the band offers one instead of grips.
        let set_timing = match &grips {
            Some(_) => None,
            None => self.set_timing_overlay(&ranges[..filled], view, cx),
        };
        let grips = grips.map(|grips| self.band_grip_overlay(&grips, ranges, filled, view, cx));
        div()
            .relative()
            .flex()
            .flex_none()
            .h(px(VIDEO_TRACK_HEIGHT))
            .w_full()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap_2()
                    .w(px(TRACK_HEADER_WIDTH))
                    .h_full()
                    .pl_3()
                    .border_r_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .w(px(22.0))
                            .font_family(cx.theme().mono_font_family.clone())
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("V1"),
                    )
                    .child(
                        div()
                            .size(px(8.0))
                            .rounded_sm()
                            .bg(rgb(theme::clip_fill(TrackKind::Overlay))),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().foreground)
                            .child("Composition"),
                    ),
            )
            .child(
                div()
                    .id("composition-lane")
                    .relative()
                    .flex_1()
                    .h_full()
                    .mr(px(LANE_GUTTER))
                    .overflow_hidden()
                    .on_scroll_wheel(cx.listener(Self::lane_wheel))
                    .on_pinch(cx.listener(Self::lane_pinch))
                    .on_mouse_down(MouseButton::Middle, cx.listener(Self::begin_timeline_pan))
                    .child(
                        div()
                            .absolute()
                            .top(px(3.0))
                            .bottom(px(3.0))
                            .left(relative(view.x(0.0)))
                            .w(relative(view.width(1.0)))
                            .rounded_sm()
                            .bg(rgb(theme::clip_fill(TrackKind::Overlay)))
                            .px_1()
                            .text_xs()
                            .text_color(theme::waveform())
                            .child(entry),
                    )
                    .children(bands)
                    .children(grips)
                    .children(set_timing),
            )
            // Outside the lane, so its edges don't cut the label.
            .when_some(label, |row, (at, lines, notice)| {
                let x =
                    (view.x(at / view.end_frame) * self.lane_width()).clamp(0.0, self.lane_width());
                let label = div()
                    .absolute()
                    .left(px(TRACK_HEADER_WIDTH + x))
                    .px_2()
                    .py_1()
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().popover)
                    .text_xs()
                    .whitespace_nowrap()
                    .map(|label| {
                        if notice {
                            label.text_color(cx.theme().muted_foreground)
                        } else {
                            label
                                .font_family(cx.theme().mono_font_family.clone())
                                .text_color(cx.theme().foreground)
                        }
                    })
                    .children(lines.into_iter().map(|line| div().child(line)));
                // Over the row above when there is one in the scroll area,
                // else over the band.
                row.child(if label_above {
                    label.bottom(px(VIDEO_TRACK_HEIGHT))
                } else {
                    label.top(px(3.0))
                })
            })
    }

    /// The grips of the selected element's own timing window, at its
    /// edges over the band holding the lookup frame: `bands` are the bands
    /// drawn (frames), the first `filled` of them this instance's in the
    /// current scene. A press on a grip that drags starts a band drag; a
    /// locked grip shows why.
    fn band_grip_overlay(
        &self,
        grips: &BandGrips,
        bands: Vec<(f64, f64)>,
        filled: usize,
        view: LaneView,
        cx: &mut Context<Self>,
    ) -> gpui_kit::AnyElement {
        let (from, to) = grips.window;
        let start = from / view.end_frame;
        let length = (to - from) / view.end_frame;
        let edge = edge_width((view.width(length) * self.lane_width()).max(2.0));
        let zone = |grip: BandGrip| {
            let (id, zone) = match grip {
                BandGrip::Start => ("band-grip-start", div().left_0().w(px(edge))),
                BandGrip::End => ("band-grip-end", div().right_0().w(px(edge))),
                BandGrip::Body => ("band-grip-body", div().left(px(edge)).right(px(edge))),
            };
            let zone = zone.id(id).absolute().top_0().bottom_0();
            match (grips.lock(grip).map(str::to_owned), grip) {
                (Some(reason), _) => zone
                    .cursor(CursorStyle::OperationNotAllowed)
                    .tooltip(move |window, cx| Tooltip::new(reason.clone()).build(window, cx)),
                (None, BandGrip::Body) => zone.cursor_pointer(),
                (None, BandGrip::Start | BandGrip::End) => zone
                    .cursor(CursorStyle::ResizeLeftRight)
                    // The edge, drawn as a bar.
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .w(px(2.0))
                            .when(grip == BandGrip::Start, |bar| bar.left_0())
                            .when(grip == BandGrip::End, |bar| bar.right_0())
                            .bg(theme::accent()),
                    ),
            }
        };
        div()
            .id("band-grips")
            .absolute()
            .top(px(3.0))
            .bottom(px(3.0))
            .left(relative(view.x(start)))
            .w(relative(view.width(length)))
            .min_w(px(2.0))
            .child(zone(BandGrip::Start))
            .child(zone(BandGrip::Body))
            .child(zone(BandGrip::End))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    // The band below doesn't take the press as a click: the
                    // drag seeks when it ends as one.
                    if this.begin_band_drag(f32::from(event.position.x), &bands, filled) {
                        cx.stop_propagation();
                        cx.notify();
                    }
                }),
            )
            .into_any_element()
    }

    /// "Set timing" at the start of the band holding the lookup frame, for
    /// an element without a window of its own: `filled` are this instance's
    /// bands in the current scene (frames). Disabled with why, as the
    /// action's offer says; `None` when it is hidden or no band holds the
    /// frame.
    fn set_timing_overlay(
        &self,
        filled: &[(f64, f64)],
        view: LaneView,
        cx: &mut Context<Self>,
    ) -> Option<gpui_kit::AnyElement> {
        let offer = self.set_timing_action()?;
        let band = draggable_band(filled, self.source_lookup_frame as f64)?;
        let start = filled[band].0 / view.end_frame;
        let button = Button::new("timeline-set-timing")
            .xsmall()
            .label("Set timing");
        let action = match offer {
            SetTimingOffer::Hidden => return None,
            // The button keeps its tooltip when disabled, and lays the
            // cursor given here over its own.
            SetTimingOffer::Disabled(reason) => button
                .disabled(true)
                .cursor(CursorStyle::OperationNotAllowed)
                .tooltip(reason)
                .into_any_element(),
            SetTimingOffer::Enabled { .. } => button
                .tooltip(SET_TIMING_NOTE)
                .on_click(cx.listener(|this, _, _, cx| {
                    // The band below doesn't take the click as a seek.
                    cx.stop_propagation();
                    this.set_timing(cx);
                    cx.notify();
                }))
                .into_any_element(),
        };
        Some(
            div()
                .absolute()
                .top(px(3.0))
                .bottom(px(3.0))
                .left(relative(view.x(start).max(0.0)))
                .flex()
                .items_center()
                .pl_1()
                .child(action)
                .into_any_element(),
        )
    }

    /// One selection band at `left`/`width` lane fractions; its tooltip names
    /// the scene (or the timecodes), the frames, and the instance's call
    /// site, and a click seeks to its first frame.
    fn selection_band(
        &self,
        band: SelectionBand,
        left: f32,
        width: f32,
        cx: &mut Context<Self>,
    ) -> gpui_kit::AnyElement {
        let (from, to) = band.range;
        let scenes = scenes_of(&[band.range], self.react_chapters().unwrap_or_default())
            .into_iter()
            .map(|chapter| chapter.name.as_str())
            .collect::<Vec<_>>();
        let place = if scenes.is_empty() {
            let timecode = |frame: u64| {
                format_timecode(
                    i64::try_from(frame).unwrap_or(i64::MAX),
                    self.frame_rate_value,
                )
            };
            format!("{}–{}", timecode(from), timecode(to))
        } else {
            scenes.join(", ")
        };
        let mut lines = vec![match band.ranges {
            1 => format!("{place} · frames {from}–{to}"),
            ranges => format!("{place} · frames {from}–{to} · {ranges} ranges"),
        }];
        let entry_dir = self.entry_dir();
        lines.extend(band.call_site.map(|site| {
            format!(
                "{}:{}:{}",
                display_path(&site.file, &entry_dir),
                site.line,
                site.column
            )
        }));
        let id = match band.instance {
            Some(instance) if band.filled => format!("timeline-band-own-{instance}-{from}-{to}"),
            Some(instance) => format!("timeline-band-other-{instance}-{from}-{to}"),
            None => format!("timeline-band-span-{from}-{to}"),
        };
        div()
            .id(SharedString::from(id))
            .absolute()
            .top(px(3.0))
            .bottom(px(3.0))
            .left(relative(left))
            .w(relative(width))
            .min_w(px(2.0))
            .rounded_sm()
            .map(|block| {
                if band.filled {
                    block.bg(theme::accent().opacity(0.42))
                } else {
                    block.border_1().border_color(theme::accent())
                }
            })
            .cursor_pointer()
            .tooltip(move |window, cx| {
                let lines = lines.clone();
                Tooltip::element(move |_, _| {
                    div()
                        .flex()
                        .flex_col()
                        .children(lines.iter().map(|line| div().child(line.clone())))
                })
                .build(window, cx)
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.seek_to_frame(from, cx);
            }))
            .into_any_element()
    }

    /// The chapters a standalone React composition declares, or `None` when
    /// it declares none (or this is a project).
    pub(crate) fn react_chapters(&self) -> Option<&[ReactChapter]> {
        self.react_preview
            .as_ref()
            .map(|react| react.chapters.as_slice())
            .filter(|chapters| !chapters.is_empty())
    }

    /// Pauses and moves the playhead to the start of `chapter`.
    pub(crate) fn seek_chapter(&mut self, chapter: &ReactChapter, cx: &mut Context<Self>) {
        self.seek_to_frame(chapter.from, cx);
    }

    /// Pauses and moves the playhead to `frame`.
    pub(crate) fn seek_to_frame(&mut self, frame: u64, cx: &mut Context<Self>) {
        self.pause();
        self.seek_frame(i64::try_from(frame).unwrap_or(i64::MAX));
        cx.notify();
    }

    /// The end of `chapter` (a block at `start`/`duration` lane fractions on
    /// lane `lane`): a press on an end that drags starts a scene drag; a
    /// locked one shows why.
    fn chapter_end_grip(
        &self,
        chapter: &ReactChapter,
        lane: usize,
        (start, duration): (f64, f64),
        view: LaneView,
        cx: &mut Context<Self>,
    ) -> gpui_kit::AnyElement {
        let edge = edge_width((view.width(duration) * self.lane_width()).max(2.0));
        let dragged = self
            .scene_drag
            .as_ref()
            .is_some_and(|drag| drag.gesture.id == chapter.id);
        let id = chapter.id.clone();
        let zone = div()
            .id(SharedString::from(format!(
                "timeline-chapter-end-{}",
                chapter.id
            )))
            .absolute()
            .top_0()
            .bottom_0()
            .right_0()
            .w(px(edge));
        let zone = match self.scene_edge(&chapter.id) {
            SceneEdge::Locked(reason) => {
                zone.cursor(CursorStyle::OperationNotAllowed)
                    .tooltip(move |window, cx| {
                        let lines: Vec<String> = reason.lines().map(str::to_owned).collect();
                        Tooltip::element(move |_, _| {
                            div()
                                .flex()
                                .flex_col()
                                .children(lines.iter().map(|line| div().child(line.clone())))
                        })
                        .build(window, cx)
                    })
            }
            SceneEdge::Editable { .. } => zone
                .cursor(CursorStyle::ResizeLeftRight)
                .when(dragged, |zone| zone.bg(theme::accent()))
                .hover(|zone| zone.bg(theme::accent()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                        // The chapter below doesn't take the press as a
                        // click: the drag seeks when it ends as one.
                        if this.begin_scene_drag(&id, f32::from(event.position.x)) {
                            cx.stop_propagation();
                            cx.notify();
                        }
                    }),
                ),
        };
        // Over the chapter's block, so the zone sits inside its end.
        div()
            .absolute()
            .left(relative(view.x(start)))
            .top(px(3.0 + lane as f32 * CHAPTER_LANE_HEIGHT))
            .h(px(CHAPTER_LANE_HEIGHT - 2.0))
            .w(relative(view.width(duration)))
            .min_w(px(2.0))
            .child(zone)
            .into_any_element()
    }

    /// The row above the tracks that shows `<Composition chapters>`: one
    /// block per chapter, the current one highlighted; a click seeks to its
    /// start. A chapter of length 0 is a thin mark. Overlapping chapters (a
    /// scene and the parts listed inside it) stack on lanes of their own, so
    /// none hides another. The chapters the selection appears in are
    /// underlined. A chapter's end drags when its `defineScenes` entry can
    /// be written (see `chapter_end_grip`).
    fn chapter_row(
        &self,
        chapters: &[ReactChapter],
        impact: Option<&ImpactView>,
        view: LaneView,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let frame = u64::try_from(self.clock.frame()).unwrap_or(0);
        let current = chapter_at(chapters, frame).map(|chapter| chapter.id.clone());
        let appears_in: &[String] = match impact {
            Some(ImpactView::Known(impact)) => &impact.appears_in_ids,
            _ => &[],
        };
        let lanes = chapter_lanes(chapters);
        let lane_count = lanes.iter().max().map_or(1, |lane| lane + 1);
        let grips = chapters
            .iter()
            .zip(&lanes)
            .map(|(chapter, &lane)| {
                let span = (
                    chapter.from as f64 / view.end_frame,
                    chapter.duration_in_frames as f64 / view.end_frame,
                );
                self.chapter_end_grip(chapter, lane, span, view, cx)
            })
            .collect::<Vec<_>>();
        let label = self.scene_label();
        let blocks = chapters
            .iter()
            .zip(lanes)
            .map(|(chapter, lane)| {
                let start = chapter.from as f64 / view.end_frame;
                let duration = chapter.duration_in_frames as f64 / view.end_frame;
                let is_current = current.as_deref() == Some(chapter.id.as_str());
                // Keep the name readable when the chapter starts left of the view.
                let label_offset = if duration > 0.0 {
                    ((view.view_start - start) / duration).clamp(0.0, 1.0) as f32
                } else {
                    0.0
                };
                let target = chapter.clone();
                div()
                    .id(SharedString::from(format!(
                        "timeline-chapter-{}",
                        chapter.id
                    )))
                    .absolute()
                    .left(relative(view.x(start)))
                    .top(px(3.0 + lane as f32 * CHAPTER_LANE_HEIGHT))
                    .h(px(CHAPTER_LANE_HEIGHT - 2.0))
                    .w(relative(view.width(duration)))
                    .min_w(px(2.0))
                    .overflow_hidden()
                    .rounded_sm()
                    .border_l_2()
                    .when(appears_in.contains(&chapter.id), |block| block.border_b_2())
                    .border_color(theme::accent())
                    .bg(theme::accent().opacity(if is_current { 0.42 } else { 0.16 }))
                    .cursor_pointer()
                    .child(
                        div()
                            .absolute()
                            .top(px(1.0))
                            .left(relative(label_offset))
                            .px_1()
                            .text_xs()
                            .text_color(cx.theme().foreground)
                            .whitespace_nowrap()
                            .child(chapter.name.clone()),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.seek_chapter(&target, cx);
                    }))
                    .into_any_element()
            })
            .collect::<Vec<_>>();
        div()
            .relative()
            .flex()
            .flex_none()
            .h(px(6.0 + lane_count as f32 * CHAPTER_LANE_HEIGHT))
            .w_full()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap_2()
                    .w(px(TRACK_HEADER_WIDTH))
                    .h_full()
                    .pl_3()
                    .border_r_1()
                    .border_color(cx.theme().border)
                    .child(div().w(px(22.0)))
                    .child(div().size(px(8.0)).rounded_sm().bg(theme::accent()))
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().foreground)
                            .child("Chapters"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(chapters.len().to_string()),
                    ),
            )
            .child(
                div()
                    .id("chapter-lane")
                    .relative()
                    .flex_1()
                    .h_full()
                    .mr(px(LANE_GUTTER))
                    .overflow_hidden()
                    .on_scroll_wheel(cx.listener(Self::lane_wheel))
                    .on_pinch(cx.listener(Self::lane_pinch))
                    .on_mouse_down(MouseButton::Middle, cx.listener(Self::begin_timeline_pan))
                    .children(blocks)
                    .children(grips),
            )
            // Beside the dragged end, outside the lane so its edges don't
            // cut the label; on one line, inside the row.
            .when_some(label, |row, (at, text, notice)| {
                let x =
                    (view.x(at / view.end_frame) * self.lane_width()).clamp(0.0, self.lane_width());
                row.child(
                    div()
                        .absolute()
                        .left(px(TRACK_HEADER_WIDTH + x + 4.0))
                        .top(px(3.0))
                        .px_2()
                        .border_1()
                        .border_color(cx.theme().border)
                        .bg(cx.theme().popover)
                        .text_xs()
                        .whitespace_nowrap()
                        .map(|label| {
                            if notice {
                                label.text_color(cx.theme().muted_foreground)
                            } else {
                                label
                                    .font_family(cx.theme().mono_font_family.clone())
                                    .text_color(cx.theme().foreground)
                            }
                        })
                        .child(text),
                )
            })
    }

    /// Zoom controls under the track headers and an overview bar under the
    /// lanes that shows (and drags) the visible window.
    fn timeline_footer(&self, view: LaneView, cx: &mut Context<Self>) -> impl IntoElement {
        let overview_bounds = self.timeline_overview_bounds.clone();
        let fitted = view.zoom <= 1.0001;
        div()
            .flex()
            .flex_none()
            .h(px(FOOTER_HEIGHT))
            .w_full()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap_1()
                    .w(px(TRACK_HEADER_WIDTH))
                    .h_full()
                    .px_2()
                    .border_r_1()
                    .border_color(cx.theme().border)
                    .child(
                        Button::new("timeline-zoom-out")
                            .xsmall()
                            .ghost()
                            .icon(IconName::Minus)
                            .disabled(fitted)
                            .tooltip_with_action("Zoom out", &ZoomTimelineOut, KEY_CONTEXT)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.zoom_timeline_by(1.0 / ZOOM_STEP, cx)
                            })),
                    )
                    .child(
                        Button::new("timeline-zoom-in")
                            .xsmall()
                            .ghost()
                            .icon(IconName::Plus)
                            .tooltip_with_action("Zoom in", &ZoomTimelineIn, KEY_CONTEXT)
                            .on_click(
                                cx.listener(|this, _, _, cx| this.zoom_timeline_by(ZOOM_STEP, cx)),
                            ),
                    )
                    .child(
                        Button::new("timeline-zoom-fit")
                            .xsmall()
                            .ghost()
                            .label("Fit")
                            .disabled(fitted)
                            .tooltip_with_action("Zoom to fit", &ZoomTimelineToFit, KEY_CONTEXT)
                            .on_click(cx.listener(|this, _, _, cx| this.zoom_timeline_to_fit(cx))),
                    )
                    .child(
                        div()
                            .ml_auto()
                            .font_family(cx.theme().mono_font_family.clone())
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("{:.1}×", view.zoom)),
                    ),
            )
            .child(
                div()
                    .id("timeline-overview")
                    .relative()
                    .flex_1()
                    .h_full()
                    .mr(px(LANE_GUTTER))
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::begin_overview_drag))
                    .child(
                        canvas(
                            move |bounds, _, _| overview_bounds.set(Some(bounds)),
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                    )
                    .when_some(self.export_range_fractions(), |bar, (start, end)| {
                        bar.child(
                            div()
                                .absolute()
                                .top(px(9.0))
                                .h(px(6.0))
                                .left(relative(start as f32))
                                .w(relative((end - start) as f32))
                                .bg(theme::export_range_fill()),
                        )
                    })
                    .child(
                        div()
                            .absolute()
                            .top(px(6.0))
                            .bottom(px(6.0))
                            .left(relative(view.view_start as f32))
                            .w(relative((1.0 / view.zoom) as f32))
                            .min_w(px(12.0))
                            .rounded(cx.theme().radius)
                            .bg(if self.timeline_overview_drag.is_some() {
                                cx.theme().scrollbar_thumb_hover
                            } else {
                                cx.theme().scrollbar_thumb
                            }),
                    )
                    .child(
                        div()
                            .absolute()
                            .top(px(4.0))
                            .bottom(px(4.0))
                            .left(relative(self.playhead_fraction() as f32))
                            .w(px(1.0))
                            .bg(theme::accent()),
                    ),
            )
    }
}

/// Ruler ticks (tall for labelled marks) and the playhead's head.
fn paint_ruler(
    bounds: Bounds<Pixels>,
    ticks: &[(f32, bool)],
    playhead_x: f32,
    tick_color: Hsla,
    window: &mut Window,
) {
    let width = bounds.size.width;
    let bottom = bounds.origin.y + bounds.size.height;
    for (x, major) in ticks {
        let height = if *major { bounds.size.height } else { px(6.0) };
        window.paint_quad(fill(
            Bounds::new(
                point(bounds.origin.x + width * *x, bottom - height),
                size(px(1.0), height),
            ),
            tick_color,
        ));
    }
    if !(0.0..=1.0).contains(&playhead_x) {
        return;
    }
    let x = bounds.origin.x + width * playhead_x;
    let accent = theme::accent();
    window.paint_quad(fill(
        Bounds::new(point(x, bounds.origin.y), size(px(1.0), bounds.size.height)),
        accent,
    ));
    // A downward pentagon at the bottom of the ruler, pointing into the lanes.
    let mut head = PathBuilder::fill();
    let half = px(6.0);
    let top = bottom - px(12.0);
    head.move_to(point(x - half + px(0.5), top));
    head.line_to(point(x + half + px(0.5), top));
    head.line_to(point(x + half + px(0.5), bottom - px(5.0)));
    head.line_to(point(x + px(0.5), bottom));
    head.line_to(point(x - half + px(0.5), bottom - px(5.0)));
    head.close();
    if let Ok(path) = head.build() {
        window.paint_path(path, accent);
    }
}

/// Mirrored peak bars centered on the clip's midline.
fn paint_waveform(bounds: Bounds<Pixels>, bars: &[f32], color: Hsla, window: &mut Window) {
    let count = bars.len().max(1) as f32;
    let step = bounds.size.width / count;
    let bar_width = (step - px(1.0)).max(px(1.0));
    let middle = bounds.origin.y + bounds.size.height / 2.0;
    for (index, amplitude) in bars.iter().enumerate() {
        let height = (bounds.size.height * amplitude.clamp(0.02, 1.0)).max(px(1.0));
        window.paint_quad(fill(
            Bounds::new(
                point(bounds.origin.x + step * index as f32, middle - height / 2.0),
                size(bar_width, height),
            ),
            color,
        ));
    }
}

fn wheel_pixels(delta: ScrollDelta) -> Point<f32> {
    match delta {
        ScrollDelta::Lines(lines) => point(lines.x * 40.0, lines.y * 40.0),
        ScrollDelta::Pixels(pixels) => point(f32::from(pixels.x), f32::from(pixels.y)),
    }
}

/// Trackpad travel, in pixels along either axis, before a gesture over the
/// lanes commits to scrubbing or scrolling.
const AXIS_LOCK_PIXELS: f32 = 6.0;

/// What a trackpad gesture over the lanes has committed to.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum LaneAxis {
    /// No trackpad gesture yet: each wheel event picks its own axis.
    #[default]
    Free,
    /// Fingers down but not yet moved far enough to pick; the travel so far.
    Undecided(Point<f32>),
    /// Scrubbing by the sideways travel.
    Scrub,
    /// Scrubbing by the vertical travel: a vertical gesture begun with Shift.
    ScrubVertical,
    Scroll,
    /// Panning both ways: a gesture begun with Option.
    Pan,
}

/// Locks a trackpad gesture over the lanes to one axis, so a vertical
/// scroll that drifts sideways never scrubs and a scrub never scrolls.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct LaneGesture {
    axis: LaneAxis,
    /// The fingers have lifted: pixel events until the next touch are the
    /// trackpad's momentum.
    coasting: bool,
}

#[derive(Debug, PartialEq)]
enum LaneWheel {
    /// Move the playhead this many pixels along the lane.
    Scrub(f32),
    /// Move the view this many pixels sideways and the tracks this many
    /// pixels vertically.
    Pan(Point<f32>),
    /// Zoom by this vertical wheel movement.
    Zoom(f32),
    /// Let the tracks scroll.
    Scroll,
    /// Swallow the event.
    Ignore,
}

impl LaneGesture {
    /// The action for one wheel event; `precise` is a pixel delta (a
    /// trackpad) rather than wheel lines.
    fn wheel(
        &mut self,
        phase: TouchPhase,
        delta: Point<f32>,
        precise: bool,
        modifiers: Modifiers,
    ) -> LaneWheel {
        match phase {
            TouchPhase::Started => {
                self.axis = LaneAxis::Undecided(point(0.0, 0.0));
                self.coasting = false;
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                self.coasting = true;
                if matches!(self.axis, LaneAxis::Undecided(_)) {
                    self.axis = LaneAxis::Scroll;
                }
            }
            TouchPhase::Moved => {}
        }
        if modifiers.control || modifiers.platform {
            return if delta.y == 0.0 {
                LaneWheel::Ignore
            } else {
                LaneWheel::Zoom(delta.y)
            };
        }
        let shift = modifiers.shift;
        // Momentum arrives as phaseless pixel deltas after the lift.
        let momentum = self.coasting && precise && phase == TouchPhase::Moved;
        let locked = (!self.coasting || momentum) && self.axis != LaneAxis::Free;
        if !locked {
            // A wheel turns only one way, so with Option it pans sideways.
            return if modifiers.alt {
                let amount = if delta.x == 0.0 { delta.y } else { delta.x };
                pan(point(amount, 0.0))
            } else if delta.x.abs() > delta.y.abs() || shift {
                scrub(scrub_amount(delta))
            } else {
                LaneWheel::Scroll
            };
        }
        // A trackpad scrub follows one axis for the whole gesture: mixing
        // the two would jerk the playhead back and forth. Shift and Option
        // count only when the gesture starts moving: Shift turns a vertical
        // gesture into a scrub, and Option pans both ways instead.
        match self.axis {
            LaneAxis::Undecided(travel) if modifiers.alt => {
                self.axis = LaneAxis::Pan;
                pan(travel + delta)
            }
            LaneAxis::Undecided(travel) => {
                let travel = travel + delta;
                if travel.x.abs().max(travel.y.abs()) < AXIS_LOCK_PIXELS {
                    self.axis = LaneAxis::Undecided(travel);
                    // With Shift the gesture scrubs either way, so hold the
                    // tracks still.
                    if shift {
                        LaneWheel::Ignore
                    } else {
                        LaneWheel::Scroll
                    }
                } else if travel.x.abs() > travel.y.abs() {
                    self.axis = LaneAxis::Scrub;
                    scrub(travel.x)
                } else if shift {
                    self.axis = LaneAxis::ScrubVertical;
                    scrub(-travel.y)
                } else {
                    self.axis = LaneAxis::Scroll;
                    LaneWheel::Scroll
                }
            }
            // Dropping momentum stops the playhead where the fingers did.
            LaneAxis::Scrub | LaneAxis::ScrubVertical if momentum => LaneWheel::Ignore,
            LaneAxis::Scrub => scrub(delta.x),
            LaneAxis::ScrubVertical => scrub(-delta.y),
            LaneAxis::Pan => pan(delta),
            LaneAxis::Scroll | LaneAxis::Free => LaneWheel::Scroll,
        }
    }
}

fn pan(delta: Point<f32>) -> LaneWheel {
    if delta.x == 0.0 && delta.y == 0.0 {
        LaneWheel::Ignore
    } else {
        LaneWheel::Pan(delta)
    }
}

/// A swipe moves the playhead with the scroll direction (the fingers' under
/// natural scrolling); a wheel that only moves vertically advances it when
/// rolled down.
fn scrub_amount(delta: Point<f32>) -> f32 {
    if delta.x == 0.0 { -delta.y } else { delta.x }
}

fn scrub(amount: f32) -> LaneWheel {
    if amount == 0.0 {
        LaneWheel::Ignore
    } else {
        LaneWheel::Scrub(amount)
    }
}

/// Whole frames a scrub of `delta_pixels` covers at `pixels_per_frame`. The
/// partial frame is carried in `remainder`, so slow swipes at a deep zoom
/// still step through every frame.
fn scrub_frames(delta_pixels: f32, pixels_per_frame: f64, remainder: &mut f64) -> i64 {
    let frames = *remainder + f64::from(delta_pixels) / pixels_per_frame.max(f64::EPSILON);
    let whole = frames.trunc();
    *remainder = frames - whole;
    whole as i64
}

/// `view_start` moved just far enough that the window `1 / zoom` wide
/// shows composition fraction `playhead`.
fn follow_playhead(view_start: f64, zoom: f64, playhead: f64) -> f64 {
    let span = 1.0 / zoom;
    if playhead < view_start {
        playhead
    } else if playhead > view_start + span {
        playhead - span
    } else {
        view_start
    }
}

/// Zoom factor for a vertical wheel movement in pixels: scrolling up (away
/// from the user) zooms in.
fn wheel_zoom_factor(delta_y: f32) -> f64 {
    (1.0 + f64::from(delta_y) / 40.0 * 0.15).clamp(0.5, 2.0)
}

#[cfg(test)]
mod tests {
    use super::{
        LaneGesture, LaneView, LaneWheel, SelectionBand, band_geometry, chapter_lanes, edit_points,
        follow_playhead, merge_close_bands, scrub_frames, selection_bands, timeline_rows,
    };
    use crate::impact::ImpactView;
    use celesta_composition::Time;
    use celesta_editor_core::{ClipKind, ClipSummary, TrackSummary};
    use celesta_project::TrackKind;
    use celesta_react_bridge::ReactChapter;
    use gpui_kit::{Modifiers, TouchPhase, point};

    #[test]
    fn overlapping_chapters_stack_on_lanes_of_their_own() {
        let chapters: Vec<ReactChapter> = [
            ("intro", 0, 60),
            ("demo", 60, 90),
            ("demo-setup", 60, 45),
            ("demo-result", 105, 45),
            ("mark", 150, 0),
            ("outro", 150, 60),
        ]
        .into_iter()
        .map(|(id, from, duration_in_frames)| ReactChapter {
            id: id.to_owned(),
            name: id.to_owned(),
            from,
            duration_in_frames,
        })
        .collect();
        assert_eq!(chapter_lanes(&chapters), [0, 0, 1, 1, 0, 1]);
        assert!(chapter_lanes(&[]).is_empty());
    }

    fn assert_bands(actual: Vec<(f32, f32)>, expected: &[(f32, f32)]) {
        assert_eq!(actual.len(), expected.len(), "{actual:?}");
        for ((left, width), (expected_left, expected_width)) in actual.iter().zip(expected) {
            assert!(
                (left - expected_left).abs() < 1e-5 && (width - expected_width).abs() < 1e-5,
                "{actual:?} != {expected:?}"
            );
        }
    }

    #[test]
    fn band_geometry_places_ranges_on_the_lane() {
        let view = LaneView {
            zoom: 1.0,
            view_start: 0.0,
            end_frame: 100.0,
        };
        assert_bands(band_geometry(&[(25, 50)], view), &[(0.25, 0.25)]);
        assert!(band_geometry(&[(50, 50), (60, 40)], view).is_empty());
    }

    #[test]
    fn band_geometry_drops_ranges_outside_the_view() {
        // The lane shows frames 50 to 75.
        let view = LaneView {
            zoom: 4.0,
            view_start: 0.5,
            end_frame: 100.0,
        };
        assert_bands(
            band_geometry(&[(0, 10), (40, 60), (75, 90)], view),
            &[(-0.4, 0.8)],
        );
    }

    #[test]
    fn the_span_before_the_scan_is_one_filled_band_rounded_outward() {
        let bands = selection_bands(&ImpactView::SpanOnly {
            span: Some((10.4, 20.2)),
            note: String::new(),
        });
        assert_eq!(bands.len(), 1);
        assert_eq!(bands[0].range, (10, 21));
        assert!(bands[0].filled);
        assert!(bands[0].call_site.is_none());
    }

    /// Every other frame of `[0, 40)` for instance 0, outlined, then a
    /// filled band and instance 1's band.
    fn alternating() -> Vec<SelectionBand> {
        let band = |range, filled, instance| SelectionBand {
            range,
            filled,
            instance: Some(instance),
            call_site: None,
            ranges: 1,
        };
        let mut bands: Vec<SelectionBand> = (0..20)
            .map(|frame| band((frame * 2, frame * 2 + 1), false, 0))
            .collect();
        bands.push(band((40, 41), true, 0));
        bands.push(band((42, 43), false, 1));
        bands
    }

    fn ranges(bands: &[SelectionBand]) -> Vec<((u64, u64), usize)> {
        bands.iter().map(|band| (band.range, band.ranges)).collect()
    }

    #[test]
    fn bands_less_than_a_pixel_apart_merge() {
        // 1000 frames on a 100 px lane: 10 frames a pixel.
        let view = LaneView {
            zoom: 1.0,
            view_start: 0.0,
            end_frame: 1000.0,
        };
        let merged = merge_close_bands(alternating(), view, 100.0);
        // Only bands of one instance and one kind merge.
        assert_eq!(
            ranges(&merged),
            vec![((0, 39), 20), ((40, 41), 1), ((42, 43), 1)]
        );
    }

    #[test]
    fn bands_a_pixel_apart_or_more_stay_apart() {
        // Zoomed in to 100 frames over 1000 px: 10 px a frame.
        let view = LaneView {
            zoom: 10.0,
            view_start: 0.0,
            end_frame: 1000.0,
        };
        let merged = merge_close_bands(alternating(), view, 1000.0);
        assert_eq!(merged.len(), 22);
        assert!(merged.iter().all(|band| band.ranges == 1));
    }

    fn track(id: &str, kind: TrackKind, clips: &[(i64, i64)]) -> TrackSummary {
        TrackSummary {
            id: id.to_owned(),
            name: id.to_owned(),
            kind,
            item_count: clips.len(),
            clips: clips
                .iter()
                .map(|(start, duration)| ClipSummary {
                    id: format!("{id}-{start}"),
                    name: String::new(),
                    start: Time::new(*start, 1),
                    duration: Time::new(*duration, 1),
                    kind: ClipKind::Video,
                    enabled: true,
                    volume: None,
                    component: None,
                    dialogue: None,
                })
                .collect(),
            enabled: true,
            locked: false,
            muted: false,
            solo: false,
        }
    }

    #[test]
    fn video_tracks_stack_topmost_first_above_audio() {
        let tracks = [
            track("background", TrackKind::Video, &[]),
            track("music", TrackKind::Audio, &[]),
            track("titles", TrackKind::Overlay, &[]),
            track("dialogue", TrackKind::Dialogue, &[]),
            track("sfx", TrackKind::Audio, &[]),
        ];
        let (visual, audio) = timeline_rows(&tracks, 0);
        let labels = |rows: &[super::TimelineRow<'_>]| {
            rows.iter()
                .map(|row| format!("{} {}", row.label, row.track.id))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            labels(&visual),
            ["V3 dialogue", "V2 titles", "V1 background"]
        );
        assert_eq!(labels(&audio), ["A1 music", "A2 sfx"]);
    }

    #[test]
    fn a_react_composition_s_videos_number_from_v2() {
        let tracks = [
            track("clip", TrackKind::Video, &[]),
            track("intro", TrackKind::Video, &[]),
            track("score", TrackKind::Audio, &[]),
            track("clip-sound", TrackKind::Audio, &[]),
        ];
        let (visual, audio) = timeline_rows(&tracks, 1);
        let labels = |rows: &[super::TimelineRow<'_>]| {
            rows.iter()
                .map(|row| format!("{} {}", row.label, row.track.id))
                .collect::<Vec<_>>()
        };
        assert_eq!(labels(&visual), ["V3 intro", "V2 clip"]);
        assert_eq!(labels(&audio), ["A1 score", "A2 clip-sound"]);
    }

    #[test]
    fn edit_points_cover_clip_boundaries_inside_the_composition() {
        let tracks = [
            track("a", TrackKind::Video, &[(0, 2), (2, 3)]),
            track("b", TrackKind::Audio, &[(1, 20)]),
        ];
        let points = edit_points(&tracks, |time| Some(time.value * 10), 100);
        assert_eq!(points, [0, 10, 20, 50, 100]);
    }

    #[test]
    fn scrub_frames_carry_the_partial_frame_between_events() {
        let mut remainder = 0.0;
        assert_eq!(scrub_frames(25.0, 10.0, &mut remainder), 2);
        assert_eq!(scrub_frames(5.0, 10.0, &mut remainder), 1);
        assert_eq!(remainder, 0.0);
        assert_eq!(scrub_frames(-15.0, 10.0, &mut remainder), -1);
        assert_eq!(scrub_frames(5.0, 10.0, &mut remainder), 0);
        assert_eq!(remainder, 0.0);
    }

    #[test]
    fn scrub_frames_skip_frames_when_zoomed_out() {
        let mut remainder = 0.0;
        assert_eq!(scrub_frames(3.0, 0.25, &mut remainder), 12);
    }

    fn wheel(gesture: &mut LaneGesture, phase: TouchPhase, dx: f32, dy: f32) -> LaneWheel {
        gesture.wheel(phase, point(dx, dy), true, Modifiers::none())
    }

    #[test]
    fn a_vertical_swipe_stays_a_scroll_through_sideways_jitter() {
        let mut gesture = LaneGesture::default();
        wheel(&mut gesture, TouchPhase::Started, 0.0, 0.0);
        assert_eq!(
            wheel(&mut gesture, TouchPhase::Moved, 1.0, -4.0),
            LaneWheel::Scroll
        );
        assert_eq!(
            wheel(&mut gesture, TouchPhase::Moved, 1.0, -4.0),
            LaneWheel::Scroll
        );
        assert_eq!(
            wheel(&mut gesture, TouchPhase::Moved, 6.0, -1.0),
            LaneWheel::Scroll
        );
    }

    #[test]
    fn a_sideways_swipe_scrubs_from_its_first_movement() {
        let mut gesture = LaneGesture::default();
        wheel(&mut gesture, TouchPhase::Started, 0.0, 0.0);
        assert_eq!(
            wheel(&mut gesture, TouchPhase::Moved, 4.0, 1.0),
            LaneWheel::Scroll
        );
        assert_eq!(
            wheel(&mut gesture, TouchPhase::Moved, 4.0, 0.0),
            LaneWheel::Scrub(8.0)
        );
        assert_eq!(
            wheel(&mut gesture, TouchPhase::Moved, 1.0, 5.0),
            LaneWheel::Scrub(1.0)
        );
    }

    #[test]
    fn momentum_after_the_fingers_lift_follows_the_locked_axis() {
        let mut gesture = LaneGesture::default();
        wheel(&mut gesture, TouchPhase::Started, 0.0, 0.0);
        wheel(&mut gesture, TouchPhase::Moved, 10.0, 0.0);
        wheel(&mut gesture, TouchPhase::Ended, 0.0, 0.0);
        assert_eq!(
            wheel(&mut gesture, TouchPhase::Moved, 5.0, 0.0),
            LaneWheel::Ignore
        );

        wheel(&mut gesture, TouchPhase::Started, 0.0, 0.0);
        wheel(&mut gesture, TouchPhase::Moved, 0.0, -10.0);
        wheel(&mut gesture, TouchPhase::Ended, 0.0, 0.0);
        assert_eq!(
            wheel(&mut gesture, TouchPhase::Moved, 5.0, -1.0),
            LaneWheel::Scroll
        );
    }

    #[test]
    fn the_mouse_wheel_picks_its_axis_per_event_even_after_a_swipe() {
        let mut gesture = LaneGesture::default();
        wheel(&mut gesture, TouchPhase::Started, 0.0, 0.0);
        wheel(&mut gesture, TouchPhase::Moved, 10.0, 0.0);
        wheel(&mut gesture, TouchPhase::Ended, 0.0, 0.0);
        let lines = point(0.0, -40.0);
        assert_eq!(
            gesture.wheel(TouchPhase::Moved, lines, false, Modifiers::none()),
            LaneWheel::Scroll
        );
        assert_eq!(
            gesture.wheel(TouchPhase::Moved, lines, false, Modifiers::shift()),
            LaneWheel::Scrub(40.0)
        );
    }

    #[test]
    fn shift_scrubs_a_vertical_trackpad_gesture_by_its_vertical_travel_alone() {
        let mut gesture = LaneGesture::default();
        let shift = Modifiers::shift();
        gesture.wheel(TouchPhase::Started, point(0.0, 0.0), true, shift);
        assert_eq!(
            gesture.wheel(TouchPhase::Moved, point(1.0, -10.0), true, shift),
            LaneWheel::Scrub(10.0)
        );
        assert_eq!(
            gesture.wheel(TouchPhase::Moved, point(5.0, -2.0), true, shift),
            LaneWheel::Scrub(2.0)
        );
        // Releasing Shift mid-gesture keeps the gesture's axis.
        assert_eq!(
            wheel(&mut gesture, TouchPhase::Moved, 5.0, 0.0),
            LaneWheel::Ignore
        );
        gesture.wheel(TouchPhase::Ended, point(0.0, 0.0), true, shift);
        assert_eq!(
            gesture.wheel(TouchPhase::Moved, point(0.0, -4.0), true, shift),
            LaneWheel::Ignore
        );
    }

    #[test]
    fn shift_with_a_sideways_trackpad_gesture_scrubs_by_its_sideways_travel() {
        let mut gesture = LaneGesture::default();
        let shift = Modifiers::shift();
        gesture.wheel(TouchPhase::Started, point(0.0, 0.0), true, shift);
        assert_eq!(
            gesture.wheel(TouchPhase::Moved, point(10.0, 1.0), true, shift),
            LaneWheel::Scrub(10.0)
        );
        assert_eq!(
            gesture.wheel(TouchPhase::Moved, point(0.0, 3.0), true, shift),
            LaneWheel::Ignore
        );
    }

    #[test]
    fn a_vertical_trackpad_gesture_without_shift_scrolls() {
        let mut gesture = LaneGesture::default();
        wheel(&mut gesture, TouchPhase::Started, 0.0, 0.0);
        assert_eq!(
            wheel(&mut gesture, TouchPhase::Moved, 0.0, -10.0),
            LaneWheel::Scroll
        );
        // Pressing Shift mid-gesture keeps the gesture's axis.
        assert_eq!(
            gesture.wheel(
                TouchPhase::Moved,
                point(0.0, -4.0),
                true,
                Modifiers::shift()
            ),
            LaneWheel::Scroll
        );
    }

    #[test]
    fn option_with_the_trackpad_pans_both_ways_and_keeps_its_momentum() {
        let mut gesture = LaneGesture::default();
        let option = Modifiers::alt();
        gesture.wheel(TouchPhase::Started, point(0.0, 0.0), true, option);
        assert_eq!(
            gesture.wheel(TouchPhase::Moved, point(3.0, -2.0), true, option),
            LaneWheel::Pan(point(3.0, -2.0))
        );
        // Releasing Option mid-gesture keeps panning.
        assert_eq!(
            wheel(&mut gesture, TouchPhase::Moved, 5.0, 4.0),
            LaneWheel::Pan(point(5.0, 4.0))
        );
        wheel(&mut gesture, TouchPhase::Ended, 0.0, 0.0);
        assert_eq!(
            wheel(&mut gesture, TouchPhase::Moved, 2.0, 1.0),
            LaneWheel::Pan(point(2.0, 1.0))
        );
    }

    #[test]
    fn option_with_the_wheel_pans_sideways() {
        let mut gesture = LaneGesture::default();
        assert_eq!(
            gesture.wheel(
                TouchPhase::Moved,
                point(0.0, -40.0),
                false,
                Modifiers::alt()
            ),
            LaneWheel::Pan(point(-40.0, 0.0))
        );
    }

    #[test]
    fn command_or_control_zooms_by_the_vertical_scroll_alone() {
        let mut gesture = LaneGesture::default();
        assert_eq!(
            gesture.wheel(
                TouchPhase::Moved,
                point(0.0, -40.0),
                false,
                Modifiers::command()
            ),
            LaneWheel::Zoom(-40.0)
        );
        assert_eq!(
            gesture.wheel(
                TouchPhase::Moved,
                point(5.0, 0.0),
                true,
                Modifiers::control()
            ),
            LaneWheel::Ignore
        );
    }

    #[test]
    fn following_the_playhead_moves_the_view_only_as_far_as_needed() {
        // The window shows a quarter of the composition.
        assert_eq!(follow_playhead(0.5, 4.0, 0.6), 0.5);
        assert_eq!(follow_playhead(0.5, 4.0, 0.8), 0.55);
        assert_eq!(follow_playhead(0.5, 4.0, 0.4), 0.4);
    }
}
