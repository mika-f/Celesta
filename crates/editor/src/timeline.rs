//! The timeline: a timecode ruler, video tracks stacked above audio tracks in
//! compositing order, the playhead, and zoom/scroll over the composition.
//!
//! The visible window is `(zoom, view_start)`: `view_start` is the fraction
//! of the composition at the lane's leading edge and the lane shows
//! `1 / zoom` of it. Pointer positions map onto the lane through its painted
//! bounds, recorded each frame in `timeline_lane_bounds`.

use celesta_editor_core::{ClipSummary, TrackSummary};
use celesta_project::TrackKind;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, Selectable as _, Sizable as _,
};
use gpui_kit::{
    Bounds, Context, CursorStyle, Hsla, IntoElement, MouseButton, MouseDownEvent,
    ParentElement as _, PathBuilder, Pixels, Point, ScrollDelta, ScrollWheelEvent, SharedString,
    Window, canvas, div, fill, point, prelude::*, px, relative, rgb, size,
};

use crate::icons::CelestaIcon;
use crate::timecode::{format_timecode, nominal_fps, ruler_scale};
use crate::{
    EditorView, ZoomTimelineIn, ZoomTimelineOut, ZoomTimelineToFit, level_at_time, waveform_segment,
};
use celesta_editor_theme as theme;

const KEY_CONTEXT: Option<&str> = Some("CelestaEditor");

pub(crate) const TRACK_HEADER_WIDTH: f32 = 220.0;
/// Space kept clear at the lane's trailing edge for the vertical scrollbar.
pub(crate) const LANE_GUTTER: f32 = 10.0;
pub(crate) const RULER_HEIGHT: f32 = 30.0;
const VIDEO_TRACK_HEIGHT: f32 = 40.0;
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
/// `A1…` in project order.
pub(crate) fn timeline_rows(
    tracks: &[TrackSummary],
) -> (Vec<TimelineRow<'_>>, Vec<TimelineRow<'_>>) {
    let mut visual: Vec<_> = tracks
        .iter()
        .filter(|track| track.kind != TrackKind::Audio)
        .enumerate()
        .map(|(index, track)| TimelineRow {
            label: format!("V{}", index + 1),
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
struct LaneView {
    zoom: f64,
    view_start: f64,
    end_frame: f64,
}

impl LaneView {
    /// Lane fraction (0 at the leading edge, 1 at the trailing edge) of a
    /// whole-composition fraction.
    fn x(self, fraction: f64) -> f32 {
        ((fraction - self.view_start) * self.zoom) as f32
    }

    fn width(self, fraction: f64) -> f32 {
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

impl EditorView {
    fn lane_view(&self) -> LaneView {
        let (zoom, view_start) = self.timeline_view();
        LaneView {
            zoom,
            view_start,
            end_frame: self.clock.end_frame().max(1) as f64,
        }
    }

    fn lane_bounds(&self) -> Option<Bounds<Pixels>> {
        self.timeline_lane_bounds.get()
    }

    fn lane_width(&self) -> f32 {
        self.lane_bounds()
            .map_or(FALLBACK_LANE_WIDTH, |bounds| f32::from(bounds.size.width))
            .max(1.0)
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

    /// Wheel over the clip lanes: Option/Ctrl/Cmd zooms, a horizontal swipe
    /// pans, and plain vertical scrolling falls through to scroll the tracks.
    fn lane_wheel(&mut self, event: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        let delta = wheel_pixels(event.delta);
        let modifiers = event.modifiers;
        if modifiers.alt || modifiers.control || modifiers.platform {
            let anchor = self.timeline_fraction_at(event.position.x);
            self.zoom_timeline_about(anchor, wheel_zoom_factor(delta.y + delta.x));
        } else if delta.x.abs() > delta.y.abs() || modifiers.shift {
            let amount = if delta.x != 0.0 { delta.x } else { delta.y };
            self.pan_timeline_pixels(amount);
        } else {
            return;
        }
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
        let (visual, audio) = timeline_rows(&self.tracks);
        let border = cx.theme().border;
        let tracks_empty = visual.is_empty() && audio.is_empty();
        let mut rows: Vec<gpui_kit::AnyElement> = Vec::new();
        if self.is_react_preview() {
            rows.push(self.react_composition_row(view, cx).into_any_element());
        }
        rows.extend(visual.iter().map(|row| {
            self.track_row(row, VIDEO_TRACK_HEIGHT, view, cx)
                .into_any_element()
        }));
        if !visual.is_empty() && !audio.is_empty() {
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
        let track_area = div()
            .id("timeline-tracks-scroll")
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
            .when(track.kind != TrackKind::Overlay, |header| {
                header
                    .child(
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
            })
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
    /// project tracks, but the ruler, playhead, and In/Out still apply.
    fn react_composition_row(&self, view: LaneView, cx: &mut Context<Self>) -> impl IntoElement {
        let entry = self
            .react_preview
            .as_ref()
            .and_then(|react| react.entry.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        div()
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
                    .relative()
                    .flex_1()
                    .h_full()
                    .mr(px(LANE_GUTTER))
                    .overflow_hidden()
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
                    ),
            )
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

/// Zoom factor for a vertical wheel movement in pixels: scrolling up (away
/// from the user) zooms in.
fn wheel_zoom_factor(delta_y: f32) -> f64 {
    (1.0 + f64::from(delta_y) / 40.0 * 0.15).clamp(0.5, 2.0)
}

#[cfg(test)]
mod tests {
    use super::{edit_points, timeline_rows};
    use celesta_composition::Time;
    use celesta_editor_core::{ClipKind, ClipSummary, TrackSummary};
    use celesta_project::TrackKind;

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
        let (visual, audio) = timeline_rows(&tracks);
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
    fn edit_points_cover_clip_boundaries_inside_the_composition() {
        let tracks = [
            track("a", TrackKind::Video, &[(0, 2), (2, 3)]),
            track("b", TrackKind::Audio, &[(1, 20)]),
        ];
        let points = edit_points(&tracks, |time| Some(time.value * 10), 100);
        assert_eq!(points, [0, 10, 20, 50, 100]);
    }
}
