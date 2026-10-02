//! The viewer: the rendered frame, its overlays, and the transport bar under
//! it, plus the audio monitor (master meter and preview volume) that sits
//! beside the timeline.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme as _, IconName, Selectable as _, Sizable as _};
use gpui_kit::{
    BorderStyle, Bounds, Context, Hsla, IntoElement, MouseButton, ObjectFit, ParentElement as _,
    StyledImage as _, Window, canvas, div, fill, img, outline, point, prelude::*, px, relative,
    size,
};

use crate::icons::CelestaIcon;
use crate::meter::audio_meter;
use crate::timecode::format_timecode;
use crate::{
    ClearExportRange, EditorView, GoToEnd, GoToStart, NextFrame, PreviewPresentation,
    PreviousFrame, SetExportIn, SetExportOut, ToggleLoop, TogglePlayback, ToggleSafeAreas,
};

const KEY_CONTEXT: Option<&str> = Some("CelestaEditor");

/// Width of the preview-volume slider; `master_volume_from_drag` maps drags
/// across it onto 0–200 %.
pub(crate) const VOLUME_SLIDER_WIDTH: f32 = 88.0;

/// Width of the audio monitor column beside the timeline.
pub(crate) const AUDIO_PANEL_WIDTH: f32 = 112.0;

/// Below these transport-bar widths the In/Out readout and the duration,
/// then the In/Out buttons, give way to the playback controls.
const TRANSPORT_FULL_WIDTH: f32 = 820.0;
const TRANSPORT_MARKS_WIDTH: f32 = 600.0;

impl EditorView {
    pub(crate) fn viewer_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = &self.document.project().settings;
        let frame_size = (settings.width as f32, settings.height as f32);
        let overlay_color = cx.theme().foreground.opacity(0.55);
        let canvas_area = div()
            .relative()
            .flex()
            .flex_1()
            .min_h_0()
            .w_full()
            .items_center()
            .justify_center()
            .overflow_hidden()
            .bg(cx.theme().background)
            .when_some(self.preview.clone(), |area, preview| match preview {
                PreviewPresentation::Image(preview) => {
                    area.child(img(preview).size_full().object_fit(ObjectFit::Contain))
                }
                #[cfg(target_os = "macos")]
                PreviewPresentation::Surface(preview) => area.child(
                    gpui_kit::surface(preview.pixel_buffer())
                        .size_full()
                        .object_fit(ObjectFit::Contain),
                ),
            })
            .when(self.show_safe_areas, |area| {
                area.child(
                    canvas(
                        |_, _, _| (),
                        move |bounds, _, window, _| {
                            paint_safe_areas(bounds, frame_size, overlay_color, window)
                        },
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                )
            })
            .when_some(self.preview_error.clone(), |area, error| {
                area.child(
                    div()
                        .p_4()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .child(format!("Preview unavailable: {error}")),
                )
            })
            .when(!self.preview_warnings.is_empty(), |area| {
                area.child(
                    div()
                        .absolute()
                        .bottom(px(8.0))
                        .left(px(8.0))
                        .right(px(8.0))
                        .flex()
                        .flex_col()
                        .gap_1()
                        .rounded(cx.theme().radius)
                        .border_1()
                        .border_color(cx.theme().warning.opacity(0.5))
                        .bg(cx.theme().warning.opacity(0.12))
                        .p_2()
                        .text_xs()
                        .text_color(cx.theme().warning)
                        .children(self.preview_warnings.iter().cloned()),
                )
            });
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(canvas_area)
            .child(self.transport_bar(cx))
    }

    /// Playhead timecode, transport controls, and the In/Out marks.
    fn transport_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let frame_rate = self.frame_rate_value;
        let mono = cx.theme().mono_font_family.clone();
        let muted = cx.theme().muted_foreground;
        let width = self
            .transport_width
            .get()
            .map_or(TRANSPORT_FULL_WIDTH, f32::from);
        let full = width >= TRANSPORT_FULL_WIDTH;
        let show_marks = width >= TRANSPORT_MARKS_WIDTH;
        let has_range = full && (self.export_in_frame.is_some() || self.export_out_frame.is_some());
        let mark = |frame: Option<i64>| {
            frame.map_or_else(
                || "--:--:--:--".to_owned(),
                |frame| format_timecode(frame, frame_rate),
            )
        };
        let range_duration = self
            .export_in_frame
            .zip(self.export_out_frame)
            .filter(|(start, end)| start < end)
            .map(|(start, end)| format_timecode(end - start, frame_rate));
        // Both side columns grow equally, which keeps the controls centered,
        // but neither shrinks below its content: the timecode never clips.
        let timecode = div()
            .flex()
            .flex_1()
            .items_baseline()
            .gap_2()
            .whitespace_nowrap()
            .font_family(mono.clone())
            .child(
                div()
                    .text_lg()
                    .text_color(cx.theme().foreground)
                    .child(format_timecode(self.clock.frame(), frame_rate)),
            )
            .when(full, |timecode| {
                timecode.child(div().text_xs().text_color(muted).child(format!(
                    "/ {}",
                    format_timecode(self.clock.end_frame(), frame_rate)
                )))
            });
        let transport = div()
            .flex()
            .flex_none()
            .items_center()
            .gap_1()
            .when(show_marks, |transport| {
                transport
                    .child(
                        Button::new("mark-in")
                            .small()
                            .ghost()
                            .label("In")
                            .selected(self.export_in_frame.is_some())
                            .tooltip_with_action("Mark In", &SetExportIn, KEY_CONTEXT)
                            .on_click(cx.listener(|this, _, _, cx| this.set_export_in(cx))),
                    )
                    .child(
                        Button::new("mark-out")
                            .small()
                            .ghost()
                            .label("Out")
                            .selected(self.export_out_frame.is_some())
                            .tooltip_with_action("Mark Out", &SetExportOut, KEY_CONTEXT)
                            .on_click(cx.listener(|this, _, _, cx| this.set_export_out(cx))),
                    )
                    .child(transport_separator(cx))
            })
            .child(
                Button::new("go-to-start")
                    .small()
                    .ghost()
                    .icon(CelestaIcon::SkipBack)
                    .tooltip_with_action("Go to start", &GoToStart, KEY_CONTEXT)
                    .on_click(cx.listener(|this, _, _, cx| this.go_to_frame(0, cx))),
            )
            .child(
                Button::new("previous-frame")
                    .small()
                    .ghost()
                    .icon(IconName::ChevronLeft)
                    .tooltip_with_action("Previous frame", &PreviousFrame, KEY_CONTEXT)
                    .on_click(cx.listener(|this, _, _, cx| this.step_frames(-1, cx))),
            )
            .child({
                let button = Button::new("toggle-playback").small();
                // Waiting for the preview audio: still clickable, to cancel.
                let button = if self.play_when_audio_ready {
                    button.icon(Spinner::new()).tooltip_with_action(
                        "Preparing audio…",
                        &TogglePlayback,
                        KEY_CONTEXT,
                    )
                } else if self.playing {
                    button.icon(IconName::Pause).tooltip_with_action(
                        "Pause",
                        &TogglePlayback,
                        KEY_CONTEXT,
                    )
                } else {
                    button.icon(IconName::Play).tooltip_with_action(
                        "Play",
                        &TogglePlayback,
                        KEY_CONTEXT,
                    )
                };
                button.on_click(cx.listener(Self::toggle_playback))
            })
            .child(
                Button::new("next-frame")
                    .small()
                    .ghost()
                    .icon(IconName::ChevronRight)
                    .tooltip_with_action("Next frame", &NextFrame, KEY_CONTEXT)
                    .on_click(cx.listener(|this, _, _, cx| this.step_frames(1, cx))),
            )
            .child(
                Button::new("go-to-end")
                    .small()
                    .ghost()
                    .icon(CelestaIcon::SkipForward)
                    .tooltip_with_action("Go to end", &GoToEnd, KEY_CONTEXT)
                    .on_click(
                        cx.listener(|this, _, _, cx| this.go_to_frame(this.clock.end_frame(), cx)),
                    ),
            )
            .child(transport_separator(cx))
            .child(
                Button::new("toggle-loop")
                    .small()
                    .ghost()
                    .icon(CelestaIcon::Repeat)
                    .selected(self.loop_playback)
                    .tooltip_with_action("Loop playback", &ToggleLoop, KEY_CONTEXT)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_loop(cx))),
            );
        let trailing = div()
            .flex()
            .flex_1()
            .justify_end()
            .items_center()
            .gap_1()
            .when(has_range, |trailing| {
                trailing
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_end()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .font_family(mono.clone())
                            .text_xs()
                            .child(div().text_color(cx.theme().foreground).child(format!(
                                "{} – {}",
                                mark(self.export_in_frame),
                                mark(self.export_out_frame)
                            )))
                            .when_some(range_duration, |column, duration| {
                                column.child(div().text_color(muted).child(duration))
                            }),
                    )
                    .child(
                        Button::new("clear-export-range")
                            .xsmall()
                            .ghost()
                            .icon(IconName::Close)
                            .tooltip_with_action("Clear In and Out", &ClearExportRange, KEY_CONTEXT)
                            .on_click(cx.listener(|this, _, _, cx| this.clear_export_range(cx))),
                    )
            })
            .child(
                Button::new("toggle-safe-areas")
                    .small()
                    .ghost()
                    .icon(CelestaIcon::Scan)
                    .selected(self.show_safe_areas)
                    .tooltip_with_action("Safe areas", &ToggleSafeAreas, KEY_CONTEXT)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_safe_areas(cx))),
            );
        div()
            .relative()
            .flex()
            .flex_none()
            .h(px(44.0))
            .w_full()
            .items_center()
            .gap_3()
            .px_3()
            .bg(cx.theme().secondary)
            .border_t_1()
            .border_color(cx.theme().border)
            .child({
                let transport_width = self.transport_width.clone();
                canvas(
                    move |bounds, _, _| transport_width.set(Some(bounds.size.width)),
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full()
            })
            .child(timecode)
            .child(transport)
            .child(trailing)
    }

    /// The master meter over the preview volume. Sits at the timeline's
    /// trailing edge, as a mixer's master strip does.
    pub(crate) fn audio_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let volume = self.monitor_volume.clamp(0.0, 2.0);
        let reading = self
            .master_levels
            .as_ref()
            .map(|levels| levels.reading(self.current_time().as_seconds().unwrap_or(0.0)));
        let has_audio = self.master_levels.is_some();
        div()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(AUDIO_PANEL_WIDTH))
            .h_full()
            .bg(cx.theme().secondary)
            .border_l_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex()
                    .flex_none()
                    .h(px(crate::timeline::RULER_HEIGHT))
                    .items_center()
                    .justify_between()
                    .px_3()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .text_xs()
                    .text_color(cx.theme().foreground)
                    .child("Master"),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .px_3()
                    .py_2()
                    .when(!has_audio && !self.audio_pending, |body| body.opacity(0.5))
                    .child(audio_meter(reading, cx)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_none()
                    .gap_1()
                    .px_3()
                    .py_2()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .child("Volume")
                            .child(format!("{}%", (volume * 100.0).round() as i32)),
                    )
                    .child(self.volume_slider(volume, cx)),
            )
    }

    fn volume_slider(&self, volume: f64, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("preview-volume-slider")
            .relative()
            .w(px(VOLUME_SLIDER_WIDTH))
            .h(px(20.0))
            .rounded(cx.theme().radius)
            .bg(cx.theme().background)
            .tooltip(|window, cx| {
                Tooltip::new("Preview volume (doesn’t affect exports)").build(window, cx)
            })
            .when_some(self.master_volume_focus.as_ref(), |slider, focus| {
                slider.track_focus(focus)
            })
            .focus(|slider| slider.border_1().border_color(cx.theme().ring))
            .hover(|style| style.bg(cx.theme().secondary_hover))
            .child(
                div()
                    .absolute()
                    .left(px(5.0))
                    .right(px(5.0))
                    .top(px(8.0))
                    .h(px(4.0))
                    .rounded_full()
                    .overflow_hidden()
                    .bg(cx.theme().border)
                    .child(
                        div()
                            .h_full()
                            .w(relative((volume / 2.0) as f32))
                            .rounded_full()
                            .bg(cx.theme().primary),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .left(px((volume / 2.0) as f32 * (VOLUME_SLIDER_WIDTH - 10.0)))
                    .top(px(5.0))
                    .size(px(10.0))
                    .rounded_full()
                    .bg(cx.theme().foreground),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(Self::begin_master_volume_drag),
            )
            .on_key_down(cx.listener(Self::master_volume_key_down))
    }

    pub(crate) fn go_to_frame(&mut self, frame: i64, cx: &mut Context<Self>) {
        self.pause();
        self.seek_frame(frame);
        self.keep_playhead_visible();
        cx.notify();
    }

    pub(crate) fn step_frames(&mut self, delta: i64, cx: &mut Context<Self>) {
        self.go_to_frame(self.clock.frame().saturating_add(delta), cx);
    }

    pub(crate) fn toggle_loop(&mut self, cx: &mut Context<Self>) {
        self.loop_playback = !self.loop_playback;
        cx.notify();
    }

    pub(crate) fn toggle_safe_areas(&mut self, cx: &mut Context<Self>) {
        self.show_safe_areas = !self.show_safe_areas;
        cx.notify();
    }
}

fn transport_separator(cx: &Context<EditorView>) -> impl IntoElement {
    div().mx_1().w(px(1.0)).h(px(16.0)).bg(cx.theme().border)
}

/// Action-safe (90 %) and title-safe (80 %) frames plus a center cross,
/// drawn over the letterboxed frame rather than the whole viewer.
fn paint_safe_areas(
    bounds: Bounds<gpui_kit::Pixels>,
    (frame_width, frame_height): (f32, f32),
    color: Hsla,
    window: &mut Window,
) {
    if frame_width <= 0.0 || frame_height <= 0.0 {
        return;
    }
    let area_width = f32::from(bounds.size.width);
    let area_height = f32::from(bounds.size.height);
    let scale = (area_width / frame_width).min(area_height / frame_height);
    let (width, height) = (frame_width * scale, frame_height * scale);
    let left = f32::from(bounds.origin.x) + (area_width - width) / 2.0;
    let top = f32::from(bounds.origin.y) + (area_height - height) / 2.0;
    for (inset, style) in [(0.05, BorderStyle::Solid), (0.10, BorderStyle::Dashed)] {
        window.paint_quad(outline(
            Bounds::new(
                point(px(left + width * inset), px(top + height * inset)),
                size(
                    px(width * (1.0 - inset * 2.0)),
                    px(height * (1.0 - inset * 2.0)),
                ),
            ),
            color,
            style,
        ));
    }
    let (center_x, center_y) = (left + width / 2.0, top + height / 2.0);
    let arm = (width.min(height) * 0.03).max(6.0);
    window.paint_quad(fill(
        Bounds::new(
            point(px(center_x - arm), px(center_y)),
            size(px(arm * 2.0), px(1.0)),
        ),
        color,
    ));
    window.paint_quad(fill(
        Bounds::new(
            point(px(center_x), px(center_y - arm)),
            size(px(1.0), px(arm * 2.0)),
        ),
        color,
    ));
}
