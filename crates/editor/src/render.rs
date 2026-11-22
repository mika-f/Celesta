use crate::export_control::{LeftPanel, left_panel};
use crate::inline_text_view::EditEvent;
use crate::timeline_drag::{BandDrag, SceneDrag};
use crate::view::EditorView;
use celesta_editor_theme::metrics;
use gpui_kit::component::progress::Progress;
use gpui_kit::component::resizable::{h_resizable, resizable_panel, v_resizable};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::status_bar::StatusBar;
use gpui_kit::component::{ActiveTheme as _, Sizable as _};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyView, Context, CursorStyle, KeyDownEvent, MouseButton, Pixels, Window, canvas, div, px,
};
use std::time::Instant;

impl Render for EditorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.poll_background_work();
        // After the export polled above, which may just have ended.
        self.settle_changed_plugins(window, cx);
        // Replaced CPU frames leave the sprite atlas only through `drop_image`.
        for image in self.preview.take_retired() {
            cx.drop_image(image, Some(window));
        }
        self.settle_added_selection(cx);
        self.watch_occurrence_sweep(cx);
        self.settle_row_requests(cx);
        self.settle_resting_rows(window, cx);
        self.settle_interactive_mode();
        self.sync_element_targets(cx);
        self.sync_scene_calls(cx);
        self.settle_scope_hover();
        self.sync_add_facts(cx);
        self.ask_add_space();
        self.settle_add_release(cx);
        self.settle_inline_text(window, cx);
        self.sync_element_rows(window, cx);
        self.update_playback(window);
        self.sync_property_panel(window, cx);
        self.sync_decoder_notice();
        self.announce_problems(window, cx);
        self.announce_blocks(window, cx);
        self.sync_preview_error_scroll();
        if self.preview_worker.pending()
            || self.media_probe_worker.pending()
            || self.audio_pending()
            || self.component_schema_worker.pending()
            // A sweep held back for the reloaded frame starts from the poll
            // that takes that frame in; a running one is watched instead.
            || self.occurrence_sweep_after.is_some()
            || self.export_cancellation.is_some()
        {
            window.request_animation_frame();
        }
        window.set_window_title(&format!("{} — Celesta", self.project_name));
        // `Root` mounts the dialog and notification layers beside this view,
        // outside the `CelestaEditor` key context, so editor shortcuts (Space,
        // arrows, ⌘N…) don't fire from inside an overlay or behind a dialog.
        self.editor_shell(window, cx)
    }
}

impl EditorView {
    /// The editor itself, with its shortcuts and focus.
    fn editor_shell(&mut self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context("CelestaEditor")
            .on_action(cx.listener(Self::open_project_action))
            .on_action(cx.listener(Self::create_new_project_action))
            .on_action(cx.listener(Self::reload_project_action))
            .on_action(cx.listener(Self::reload_plugins_action))
            .on_action(cx.listener(Self::find_plugins_action))
            .on_action(cx.listener(Self::close_window_action))
            .on_action(cx.listener(Self::export_project_action))
            .on_action(cx.listener(Self::set_export_in_action))
            .on_action(cx.listener(Self::set_export_out_action))
            .on_action(cx.listener(Self::clear_export_range_action))
            .on_action(cx.listener(Self::set_up_typescript_action))
            .on_action(cx.listener(Self::set_up_typescript_in_folder_action))
            .on_action(cx.listener(Self::toggle_playback_action))
            .on_action(cx.listener(Self::play_forward_action))
            .on_action(cx.listener(Self::pause_playback_action))
            .on_action(cx.listener(Self::previous_frame_action))
            .on_action(cx.listener(Self::next_frame_action))
            .on_action(cx.listener(Self::jump_backward_action))
            .on_action(cx.listener(Self::jump_forward_action))
            .on_action(cx.listener(Self::go_to_start_action))
            .on_action(cx.listener(Self::go_to_end_action))
            .on_action(cx.listener(Self::previous_edit_point_action))
            .on_action(cx.listener(Self::next_edit_point_action))
            .on_action(cx.listener(Self::go_to_in_action))
            .on_action(cx.listener(Self::go_to_out_action))
            .on_action(cx.listener(Self::toggle_loop_action))
            .on_action(cx.listener(Self::toggle_safe_areas_action))
            .on_action(cx.listener(Self::show_problems_action))
            .on_action(cx.listener(Self::toggle_interactive_action))
            .on_action(cx.listener(Self::zoom_timeline_in_action))
            .on_action(cx.listener(Self::zoom_timeline_out_action))
            .on_action(cx.listener(Self::zoom_timeline_to_fit_action))
            .on_action(cx.listener(Self::undo_action))
            .on_action(cx.listener(Self::redo_action))
            .when_some(self.focus_handle.as_ref(), |view, focus_handle| {
                view.track_focus(focus_handle)
            })
            // Escape cancels a canvas drag, a band or scene drag, a row's
            // scrub or the add mode before the editor's other key-down
            // handling (a focused input's Escape action runs first). Ending
            // the add mode also cancels a canvas text edit open with it,
            // which would otherwise take a second Escape. Last, it closes
            // the export page or the plugin page, from one of its text
            // fields too.
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key != "escape" {
                    return;
                }
                let escaped = this.escape_transform_drag()
                    || this.escape_timeline_drag::<BandDrag>()
                    || this.escape_timeline_drag::<SceneDrag>()
                    || this.escape_row_scrub()
                    || this.escape_property_scrub();
                let added = !escaped && this.escape_add();
                if added {
                    this.end_inline_text(EditEvent::Escape, cx);
                }
                let closed = !escaped
                    && !added
                    && (this.export_page.is_open() || this.plugin_page.is_open());
                if closed && this.export_page.is_open() {
                    this.close_export_page(window, cx);
                } else if closed {
                    this.close_plugin_page(window, cx);
                }
                if escaped || added || closed {
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            // Any press ends a note on why a double-click did not edit a
            // text, why an edit was cancelled or an addition not written,
            // and drops a double-click waiting for its layer's source (the
            // canvas sets them again); likewise a note on why a band or
            // scene drag wrote nothing.
            .capture_any_mouse_down(cx.listener(|this, _, _, cx| {
                let note = this.inline_note.take().is_some();
                let notice = this.inline_notice.take().is_some();
                let band = this.band_notice.take().is_some();
                let scene = this.scene_notice.take().is_some();
                let add_notice = this.add.notice.take().is_some();
                this.inline_pending = None;
                if note || notice || band || scene || add_notice {
                    cx.notify();
                }
            }))
            .on_mouse_move(cx.listener(Self::drag_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::drag_end))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::middle_drag_end))
            .flex()
            .flex_col()
            .size_full()
            .overflow_hidden()
            .bg(cx.theme().background)
            .font_family(".SystemUIFont")
            .child(self.title_bar(cx))
            .child(self.workspace(window, cx))
            .child(self.status_bar(cx))
            // A scrub or a band or scene drag keeps its cursor wherever the
            // pointer goes.
            .when(
                self.row_scrub_active()
                    || self.timeline_drag_active::<BandDrag>()
                    || self.timeline_drag_active::<SceneDrag>(),
                |root| {
                    root.child(
                        canvas(
                            |_, _, _| {},
                            |_, _, window, _| {
                                window.set_window_cursor_style(CursorStyle::ResizeLeftRight);
                            },
                        )
                        .absolute()
                        .size_0(),
                    )
                },
            )
    }

    /// The resizable shell: the asset list, the viewer, and the inspector,
    /// stacked above the timeline and its master audio meter. In
    /// React-preview mode the asset list is hidden and the right dock shows
    /// composition facts instead of the inspector. While the export page
    /// or the plugin page is open its panel takes the asset list's place,
    /// with a width of its own.
    pub(crate) fn workspace(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let left = left_panel(
            self.is_react_preview(),
            self.export_page.is_open(),
            self.plugin_page.is_open(),
        );
        let page = match left {
            LeftPanel::Export => self.export_page.page().cloned().map(AnyView::from),
            LeftPanel::Plugins => self.plugin_page.page().cloned().map(AnyView::from),
            LeftPanel::Hidden | LeftPanel::Assets => None,
        };
        let body_state = self.body_split.clone();
        let border = cx.theme().border;
        // A flex column, so the panel's `flex_1` / `min_h_0` bound its
        // height and its list scrolls.
        let left_column = div()
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .border_r_1()
            .border_color(border);
        // physical: the docks' and the timeline's split sizes stay in pixels.
        // The window does not shrink with the rem, so keeping the regions'
        // sizes is what lets the denser type show more in them (and the
        // timeline's pixel geometry keeps the same number of tracks). The
        // export panel's width starts at 26rem, within 22–34rem, and the
        // plugin page's, which shows a list and a plugin's details side by
        // side, at 40rem, within 32–56rem, in the rem of the frame the row
        // is first laid out in; after that a rem change moves only its
        // range, which the width held is kept in. The row keeps one id
        // either way, so what its panels keep by id (the right dock's and
        // the viewer's scroll positions) stays; the split sizes live in the
        // state, one per layout.
        let rem = window.rem_size();
        let (row_state, first_panel) = match (left, page) {
            (LeftPanel::Export, Some(page)) => (
                self.export_split.clone(),
                resizable_panel()
                    .size(rem * 26.0)
                    .size_range(rem * 22.0..rem * 34.0)
                    .child(left_column.child(page)),
            ),
            (LeftPanel::Plugins, Some(page)) => (
                self.plugins_split.clone(),
                resizable_panel()
                    .size(rem * 40.0)
                    .size_range(rem * 32.0..rem * 56.0)
                    .child(left_column.child(page)),
            ),
            _ => (
                self.dock_split.clone(),
                resizable_panel()
                    .size(px(260.0))
                    .size_range(px(200.0)..px(440.0))
                    .visible(left == LeftPanel::Assets)
                    .child(left_column.child(self.asset_panel(cx))),
            ),
        };
        let dock_row = h_resizable("celesta-dock-row")
            .when_some(row_state.as_ref(), |group, state| group.with_state(state))
            .child(first_panel)
            .child(
                resizable_panel()
                    .size_range(px(260.0)..Pixels::MAX)
                    .child(self.viewer_panel(window, cx)),
            )
            .child(
                resizable_panel()
                    .size(px(300.0))
                    .size_range(px(240.0)..px(520.0))
                    .child(
                        // A flex column, so the panel's `flex_1` / `min_h_0`
                        // bound its height and its list scrolls.
                        div()
                            .flex()
                            .flex_col()
                            .size_full()
                            .min_h_0()
                            .overflow_hidden()
                            .border_l_1()
                            .border_color(border)
                            .child(self.right_dock(cx)),
                    ),
            );
        div()
            .flex()
            .flex_1()
            .w_full()
            .min_h_0()
            .overflow_hidden()
            .child(
                v_resizable("celesta-body")
                    .when_some(body_state.as_ref(), |group, state| group.with_state(state))
                    .child(resizable_panel().child(dock_row))
                    .child(
                        resizable_panel()
                            .size(px(300.0))
                            .size_range(px(180.0)..px(640.0))
                            .child(
                                div()
                                    .flex()
                                    .size_full()
                                    .border_t_1()
                                    .border_color(border)
                                    .child(self.timeline(window, cx))
                                    .child(self.audio_panel(window, cx)),
                            ),
                    ),
            )
    }

    /// What is still loading in the background, for the status bar:
    /// `Preparing audio · Loading assets…`, or `None` once everything is in.
    pub(crate) fn loading_label(&self) -> Option<String> {
        let tasks: Vec<&str> = [
            (self.opening, "Opening file"),
            (self.audio_pending(), "Preparing audio"),
            (self.media_probe_worker.pending(), "Loading assets"),
            (
                self.component_schema_worker.pending(),
                "Loading component properties",
            ),
        ]
        .into_iter()
        .filter_map(|(pending, label)| pending.then_some(label))
        .collect();
        (!tasks.is_empty()).then(|| format!("{}…", tasks.join(" · ")))
    }

    pub(crate) fn status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mono = cx.theme().mono_font_family.clone();
        StatusBar::new()
            .h(metrics::toolbar_row_height())
            .left(
                div()
                    .font_family(mono.clone())
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "{}  ·  {}  ·  {}",
                        self.dimensions, self.frame_rate_label, self.gpu_name
                    )),
            )
            .when_some(self.loading_label(), |bar, label| {
                bar.right(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .text_sm()
                        .child(Spinner::new().xsmall())
                        .child(label),
                )
            })
            .when_some(
                self.export_status
                    .as_ref()
                    .map(|status| status.line(Instant::now(), self.export_cancelling)),
                |bar, line| {
                    bar.right(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .font_family(mono.clone())
                            .text_sm()
                            .text_color(cx.theme().foreground)
                            .child(
                                Progress::new("export-progress")
                                    .xsmall()
                                    .w(metrics::export_progress_width())
                                    .value(line.percent.unwrap_or(0.))
                                    .loading(line.percent.is_none()),
                            )
                            .child(line.text),
                    )
                },
            )
            .right(self.problems_indicator(cx))
    }
}
