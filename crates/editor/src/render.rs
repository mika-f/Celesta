use crate::helpers::export_progress_label;
use crate::view::EditorView;
use gpui_kit::component::resizable::{h_resizable, resizable_panel, v_resizable};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::status_bar::StatusBar;
use gpui_kit::component::{ActiveTheme as _, Sizable as _};
use gpui_kit::prelude::*;
use gpui_kit::{Context, MouseButton, Window, div, px};

impl Render for EditorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.poll_background_work();
        self.update_playback(window);
        if self.preview_pending
            || self.media_pending
            || self.audio_pending
            || self.component_schema_pending
            || self.export_cancellation.is_some()
        {
            window.request_animation_frame();
        }
        window.set_window_title(&format!("{} — Celesta", self.project_name));
        div()
            .key_context("CelestaEditor")
            .on_action(cx.listener(Self::open_project_action))
            .on_action(cx.listener(Self::create_new_project_action))
            .on_action(cx.listener(Self::reload_project_action))
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
            .on_action(cx.listener(Self::zoom_timeline_in_action))
            .on_action(cx.listener(Self::zoom_timeline_out_action))
            .on_action(cx.listener(Self::zoom_timeline_to_fit_action))
            .when_some(self.focus_handle.as_ref(), |view, focus_handle| {
                view.track_focus(focus_handle)
            })
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
            .child(self.workspace(cx))
            .child(self.status_bar(cx))
    }
}

impl EditorView {
    /// The resizable shell: the asset list, the viewer, and the inspector,
    /// stacked above the timeline and its master audio meter. In
    /// React-preview mode the asset list is hidden and the right dock shows
    /// composition facts instead of the inspector.
    pub(crate) fn workspace(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let react = self.is_react_preview();
        let dock_state = self.dock_split.clone();
        let body_state = self.body_split.clone();
        let border = cx.theme().border;
        let dock_row = h_resizable("celesta-dock-row")
            .when_some(dock_state.as_ref(), |group, state| group.with_state(state))
            .child(
                resizable_panel()
                    .size(px(260.0))
                    .size_range(px(200.0)..px(440.0))
                    .visible(!react)
                    .child(
                        // A flex column, so the panel's `flex_1` / `min_h_0`
                        // bound its height and its list scrolls.
                        div()
                            .flex()
                            .flex_col()
                            .size_full()
                            .min_h_0()
                            .overflow_hidden()
                            .border_r_1()
                            .border_color(border)
                            .child(self.asset_panel(cx)),
                    ),
            )
            .child(resizable_panel().child(self.viewer_panel(cx)))
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
                            .child(if react {
                                self.react_preview_panel(cx).into_any_element()
                            } else {
                                self.inspector_panel(cx).into_any_element()
                            }),
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
                                    .child(self.timeline(cx))
                                    .child(self.audio_panel(cx)),
                            ),
                    ),
            )
    }

    /// What is still loading in the background, for the status bar:
    /// `Preparing audio · Loading assets…`, or `None` once everything is in.
    pub(crate) fn loading_label(&self) -> Option<String> {
        let tasks: Vec<&str> = [
            (self.opening, "Opening file"),
            (self.audio_pending, "Preparing audio"),
            (self.media_pending, "Loading assets"),
            (
                self.component_schema_pending,
                "Loading component properties",
            ),
        ]
        .into_iter()
        .filter_map(|(pending, label)| pending.then_some(label))
        .collect();
        (!tasks.is_empty()).then(|| format!("{}…", tasks.join(" · ")))
    }

    pub(crate) fn status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        StatusBar::new()
            .left(
                div()
                    .text_xs()
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
                        .child(Spinner::new().xsmall())
                        .child(label),
                )
            })
            .when_some(
                self.export_progress.as_ref().map(export_progress_label),
                |bar, label| {
                    bar.right(
                        div()
                            .text_xs()
                            .text_color(cx.theme().foreground)
                            .child(label),
                    )
                },
            )
    }
}
