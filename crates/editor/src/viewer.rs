//! The viewer: the canvas tools' row above it while a layer is selected in
//! interactive mode, the rendered frame and its overlays, and the transport
//! bar under it, plus the audio monitor (master meter and preview volume)
//! that sits beside the timeline.

use celesta_composition::Point;
use celesta_editor_theme as theme;
use celesta_gpu_renderer::hit_test;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Selectable as _, Sizable as _};
use gpui_kit::{
    AnyElement, App, BorderStyle, Bounds, ClickEvent, Context, CursorStyle, Hitbox, HitboxBehavior,
    Hsla, IntoElement, MouseButton, MouseDownEvent, ObjectFit, ParentElement as _, PathBuilder,
    Pixels, StyledImage as _, Window, canvas, div, fill, img, outline, point, prelude::*, px,
    relative, rems, size,
};

use crate::actions::{
    ClearExportRange, GoToEnd, GoToStart, NextFrame, PreviousFrame, SetExportIn, SetExportOut,
    ToggleInteractive, ToggleLoop, TogglePlayback, ToggleSafeAreas,
};
use crate::element_rows::{InteractiveToggle, interactive_toggle};
use crate::icons::CelestaIcon;
use crate::layer_select::{SourceLookup, letterbox, outline_quads, scene_point};
use crate::meter::audio_meter;
use crate::preview::PreviewPresentation;
use crate::problems::{ActiveProblem, Source, flow_line_breaks, summarize_warnings};
use crate::problems_panel::{detail_block, show_in_problems_row};
use crate::timecode::format_timecode;
use crate::transform_drag::{
    DragState, HANDLE_SIZE, NO_AFFINE_FRAME, ROTATE_STEM, TransformDrag, pivot_note_text,
    turns_about_pivot,
};
use crate::transform_handles::{
    Handle, Marker, PivotMode, Resize, ResizeTool, anchor_handle, handle_quad, marker, pivot_cross,
    rotate_stem, shown_handles,
};
use crate::view::EditorView;
use celesta_gpu_renderer::LayerFrame;

const KEY_CONTEXT: Option<&str> = Some("CelestaEditor");

/// Width of the preview-volume slider; `master_volume_from_drag` maps drags
/// across it onto 0–200 %.
pub(crate) const VOLUME_SLIDER_WIDTH: f32 = 88.0;

/// Width of the audio monitor column beside the timeline.
pub(crate) const AUDIO_PANEL_WIDTH: f32 = 112.0;

/// The transport bar's layout at `width` (viewer pixels): whether the In/Out
/// readout and the duration show (`full`), then whether the In/Out buttons
/// do; below these widths they give way to the playback controls.
fn transport_layout(width: f32) -> (bool, bool) {
    (
        width >= TRANSPORT_FULL_WIDTH,
        width >= TRANSPORT_MARKS_WIDTH,
    )
}

const TRANSPORT_FULL_WIDTH: f32 = 820.0;
const TRANSPORT_MARKS_WIDTH: f32 = 600.0;

/// Whether the canvas-tool row shows above the viewer: in a React preview's
/// interactive mode (Select layers on) with a layer selected, which the
/// tools act on.
fn shows_tool_row(react_preview: bool, interactive: bool, selected: bool) -> bool {
    react_preview && interactive && selected
}

/// Height of the canvas-tool row docked above the viewer.
const TOOL_ROW_HEIGHT: f32 = 34.0;
/// Width of a canvas-tool tab, fixed so a warning or a selection never
/// changes the row.
const TAB_WIDTH: f32 = 58.0;
/// Height of a tab's pill inside its list.
const TAB_HEIGHT: f32 = 22.0;
/// The gap between a tab list's edge and its selected pill.
const TAB_LIST_INSET: f32 = 3.0;
/// The slot a toggle's warning sits in, reserved whether it shows or not.
const NOTE_SLOT_WIDTH: f32 = 20.0;

/// What clicking a tab does.
type TabClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// One tab of a canvas-tool toggle.
struct ToolTab {
    id: &'static str,
    label: &'static str,
    tooltip: &'static str,
    /// The chosen one: the selected pill.
    chosen: bool,
    on_click: TabClick,
}

/// A canvas-tool toggle as a tabs list (shadcn's Tabs): a rounded, darker
/// container with a few pixels of padding, the chosen tab an inset, smaller
/// rounded pill in a lighter surface with a border and foreground text, so
/// the gap around it shows which is chosen; the other tab unfilled with
/// muted text. Fixed widths; then a warning icon (with `note` as its
/// tooltip) in a reserved slot when the choice is not in effect on the
/// selected layer. The row is in the muted chrome color already, so the
/// list takes the theme's background to stand apart from it, and the pill
/// the secondary surface with the input border.
fn mode_toggle(
    note_id: &'static str,
    tabs: [ToolTab; 2],
    note: Option<String>,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let (warning, muted, foreground) = (theme.warning, theme.muted_foreground, theme.foreground);
    let (pill, pill_border) = (theme.secondary, theme.input);
    // An unselected tab under the pointer: a step from muted toward the
    // foreground.
    let muted_hover = foreground.opacity(0.8);
    let slot = div()
        .id(note_id)
        .flex_none()
        .w(px(NOTE_SLOT_WIDTH))
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .when_some(note, |slot, note| {
            slot.child(
                Icon::new(IconName::TriangleAlert)
                    .xsmall()
                    .text_color(warning),
            )
            .tooltip(move |window, cx| Tooltip::new(note.clone()).build(window, cx))
        });
    let list = div()
        .flex()
        .flex_none()
        .items_center()
        .p(px(TAB_LIST_INSET))
        .rounded_md()
        .bg(theme.background)
        .children(tabs.map(|tab| {
            let tooltip = tab.tooltip;
            div()
                .id(tab.id)
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .w(px(TAB_WIDTH))
                .h(px(TAB_HEIGHT))
                .rounded_sm()
                .border_1()
                .text_xs()
                .cursor_pointer()
                // Both tabs carry a hover style, the chosen one restating its
                // own colors: GPUI tracks a div's hover flag (by its id, one
                // per value) only while it has a hover style, and reads that
                // flag for text colors before the frame's hitboxes exist. A
                // tab without one keeps a stale "hovered" from when it was
                // clicked, and would show it the next time it is unselected.
                .map(|this| {
                    if tab.chosen {
                        this.bg(pill)
                            .border_color(pill_border)
                            .text_color(foreground)
                            .shadow_sm()
                            .hover(move |this| {
                                this.bg(pill)
                                    .border_color(pill_border)
                                    .text_color(foreground)
                            })
                    } else {
                        this.border_color(gpui_kit::transparent_black())
                            .text_color(muted)
                            .hover(move |this| this.text_color(muted_hover))
                    }
                })
                .child(tab.label)
                .tooltip(move |window, cx| Tooltip::new(tooltip).build(window, cx))
                .on_click(tab.on_click)
        }));
    div()
        .flex()
        .flex_none()
        .items_center()
        .child(list)
        .child(slot)
}

/// One group of the tool row: a muted label and its toggle.
fn tool_group(label: &'static str, toggle: impl IntoElement, cx: &App) -> impl IntoElement {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap_2()
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
        .child(toggle)
}

impl EditorView {
    pub(crate) fn viewer_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = &self.document.project().settings;
        let frame_size = (settings.width as f32, settings.height as f32);
        let overlay_color = cx.theme().foreground.opacity(0.55);
        let preview_problem = self.problems.active(Source::Preview).cloned();
        let has_warnings = !self.problems.preview_warnings().is_empty();
        let show_safe_areas = self.show_safe_areas;
        let drag_state = self.drag_state();
        // Why a chosen Center is not in effect: the toggle's warning and the
        // handles' note.
        let pivot_note = self.pivot_note();
        // The canvas tools' row docks above the canvas while Select layers
        // is on and a layer is selected: they act on the selection.
        let shows_tools = shows_tool_row(
            self.is_react_preview(),
            self.interactive,
            self.selected_layer.is_some(),
        );
        // Every region of the selected layer in the shown frame; none when
        // the frame does not draw it. The Size tool's handles sit on the box
        // they resize, which is then the outline drawn (a stroked text's
        // layout box, a rotated dashed rect's own corners).
        // An anchor drag holds the overlay on its start frame, the outline
        // too (the preview's regions would jitter).
        let held = self
            .transform_drag
            .as_ref()
            .is_some_and(TransformDrag::holds_overlay);
        let resized_box = match &drag_state {
            DragState::Editable(frame, _, _, resize) if held => {
                Some(handle_quad(frame, resize.as_ref()))
            }
            DragState::Editable(frame, _, _, resize @ Some(_)) if frame.anchor_box.is_some() => {
                Some(handle_quad(frame, resize.as_ref()))
            }
            _ => None,
        };
        let selected_quads: Vec<[Point; 4]> = self
            .selected_layer
            .as_ref()
            .filter(|_| resized_box.is_none())
            .map(|id| {
                let regions = self
                    .hit_regions
                    .iter()
                    .filter(|region| region.layer_id == *id)
                    .map(|region| region.corners)
                    .collect();
                // A group draws nothing itself; outline its children's box.
                let frame = match self.selected_frame() {
                    Some(Some(frame)) => Some(frame.outline),
                    _ => None,
                };
                outline_quads(regions, frame)
            })
            .or_else(|| resized_box.map(|quad| vec![quad]))
            .unwrap_or_default();
        let viewer_bounds = self.viewer_bounds.clone();
        let view = cx.entity_id();
        let dashed = matches!(drag_state, DragState::Locked(..));
        let handle_fill = cx.theme().background;
        let editable_frame = match &drag_state {
            DragState::Editable(frame, editable, pivot, resize) => {
                Some((frame.clone(), editable.clone(), *pivot, resize.clone()))
            }
            _ => None,
        };
        // The cross of every selected layer with a frame: the point a drag
        // turns about (the box's middle with Center in effect, else the
        // pivot), and whether Alt drags it.
        // During an anchor drag the overlay stays on the start frame and only
        // the cross follows, where the written anchor puts the pivot.
        let anchor_cross = self
            .transform_drag
            .as_ref()
            .and_then(TransformDrag::anchor_cross);
        let marked = match &drag_state {
            DragState::Editable(..) if anchor_cross.is_some() => anchor_cross.map(Marker::Pivot),
            DragState::Editable(frame, _, pivot, _) => Some(marker(frame, *pivot)),
            DragState::Locked(frame, _) => Some(Marker::Pivot(pivot_cross(frame))),
            _ => None,
        };
        // Why it can't, when it can't: every cause, shown on hover; the
        // center marks a point only.
        let cross_reason = match (&drag_state, marked) {
            (DragState::Editable(frame, ..), Some(Marker::Center(_))) => {
                Some(self.center_cross_text(frame))
            }
            (DragState::Editable(frame, ..) | DragState::Locked(frame, _), _) => {
                self.anchor_lock_reason(frame)
            }
            _ => None,
        };
        let cross = match &drag_state {
            DragState::Editable(frame, editable, ..) => Some((
                marked.map_or_else(|| pivot_cross(frame), Marker::at),
                CrossDrag::of(anchor_handle(frame, editable).is_some(), &cross_reason),
            )),
            DragState::Locked(frame, _) => {
                Some((pivot_cross(frame), CrossDrag::of(false, &cross_reason)))
            }
            _ => None,
        };
        let notice = match drag_state {
            DragState::NoFrame => Some(NO_AFFINE_FRAME.to_owned()),
            _ => None,
        }
        .or_else(|| {
            self.element_edit
                .error(self.selected_layer.as_deref())
                .map(str::to_owned)
        });
        // Placed by the viewer's last painted bounds, relative to them.
        let last_bounds = self.viewer_bounds.get();
        let locked_box = match &drag_state {
            DragState::Locked(frame, reason) => last_bounds
                .and_then(|bounds| outline_box(bounds, frame_size, frame))
                .map(|area| (area, reason.clone())),
            _ => None,
        };
        let label = self.transform_drag_label().and_then(|label| {
            let bounds = last_bounds?;
            let (frame, ..) = editable_frame.as_ref()?;
            Some((outline_box(bounds, frame_size, frame)?, label))
        });
        // An anchor Alt can't move: the cross says why it stays.
        let cross_lock = cross.and_then(|(at, _)| {
            let reason = cross_reason.clone()?;
            Some((cross_box(last_bounds?, frame_size, at)?, reason))
        });
        // Center chosen but not in effect: the handles that turn or scale
        // about the pivot say why. While a drag runs, its label says it
        // instead.
        let handle_note = pivot_note
            .filter(|_| self.transform_drag.is_none())
            .map(pivot_note_text);
        let note_boxes: Vec<Bounds<Pixels>> = match (&handle_note, &editable_frame, last_bounds) {
            (Some(_), Some((frame, editable, pivot, resize)), Some(bounds)) => {
                letterbox(bounds, frame_size)
                    .map(|(_, _, scale)| {
                        let stem = f64::from(ROTATE_STEM / scale);
                        shown_handles(frame, editable, stem, *pivot, resize.as_ref())
                            .into_iter()
                            .filter(|(handle, _)| turns_about_pivot(*handle, resize.as_ref()))
                            .filter_map(|(_, at)| cross_box(bounds, frame_size, at))
                            .collect()
                    })
                    .unwrap_or_default()
            }
            _ => Vec::new(),
        };
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
            .child(
                canvas(
                    move |bounds, window, _| {
                        // The overlays placed by these bounds (the cross's
                        // and the locked box's tooltips, the drag label, the
                        // "Pivot —" notes) were laid out with the previous
                        // ones. When they change (the tool row appears or
                        // goes), schedule one more frame that re-renders the
                        // view with them: a notify made during the draw only
                        // marks the view dirty and would wait for some other
                        // redraw, while a next-frame callback wakes the
                        // window.
                        if viewer_bounds.replace(Some(bounds)) != Some(bounds) {
                            window.on_next_frame(move |_, cx| cx.notify(view));
                        }
                        // Each handle's square, with a hitbox for its cursor.
                        let handles =
                            editable_frame.and_then(|(frame, editable, pivot, resize)| {
                                let tools = (pivot, resize.as_ref());
                                place_handles(bounds, frame_size, &frame, &editable, tools, window)
                            });
                        let cross = cross.and_then(|(at, drag)| {
                            place_cross(bounds, frame_size, at, drag, window)
                        });
                        (handles, cross)
                    },
                    move |bounds, (handles, cross), window, _| {
                        if show_safe_areas {
                            paint_safe_areas(bounds, frame_size, overlay_color, window);
                        }
                        for corners in &selected_quads {
                            paint_layer_outline(bounds, frame_size, corners, dashed, window);
                        }
                        if let Some(handles) = handles {
                            paint_handles(&handles, handle_fill, window);
                        }
                        if let Some(cross) = cross {
                            paint_cross(&cross, window);
                        }
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .when(self.picks_layers(), |area| {
                area.on_mouse_down(MouseButton::Left, cx.listener(Self::select_layer_at))
                    // Alt over the pivot cross changes its cursor.
                    .on_modifiers_changed(cx.listener(|this, _, _, cx| {
                        if this.selected_layer.is_some() {
                            cx.notify();
                        }
                    }))
            })
            .when_some(cross_lock, |area, (cross, reason)| {
                area.child(
                    div()
                        .id("pivot-locked")
                        .absolute()
                        .left(cross.origin.x)
                        .top(cross.origin.y)
                        .w(cross.size.width)
                        .h(cross.size.height)
                        .tooltip(move |window, cx| Tooltip::new(reason.clone()).build(window, cx)),
                )
            })
            .children(note_boxes.into_iter().enumerate().map(|(i, square)| {
                let note = handle_note.clone().unwrap_or_default();
                div()
                    .id(("pivot-note", i))
                    .absolute()
                    .left(square.origin.x)
                    .top(square.origin.y)
                    .w(square.size.width)
                    .h(square.size.height)
                    .tooltip(move |window, cx| Tooltip::new(note.clone()).build(window, cx))
            }))
            // Nothing to drag: say why where the pointer is.
            .when_some(locked_box, |area, (locked, reason)| {
                area.child(
                    div()
                        .id("transform-locked")
                        .absolute()
                        .left(locked.origin.x)
                        .top(locked.origin.y)
                        .w(locked.size.width)
                        .h(locked.size.height)
                        .cursor(CursorStyle::OperationNotAllowed)
                        .tooltip(move |window, cx| Tooltip::new(reason.clone()).build(window, cx)),
                )
            })
            .when_some(label, |area, (beside, label)| {
                area.child(
                    div()
                        .absolute()
                        .left(beside.origin.x + beside.size.width + px(8.0))
                        .top(beside.origin.y)
                        .px_1()
                        .border_1()
                        .border_color(cx.theme().border)
                        .bg(cx.theme().popover)
                        .text_xs()
                        .font_family(cx.theme().mono_font_family.clone())
                        .text_color(cx.theme().foreground)
                        .whitespace_nowrap()
                        .child(label),
                )
            })
            .when_some(notice, |area, notice| {
                area.child(
                    div()
                        .absolute()
                        .top_2()
                        .left_2()
                        .px_2()
                        .py_1()
                        .border_1()
                        .border_color(cx.theme().border)
                        .bg(cx.theme().popover)
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(notice),
                )
            })
            // One column over the frame: the error panel takes the space the
            // warnings strip leaves, so the two never overlap.
            .when(preview_problem.is_some() || has_warnings, |area| {
                area.child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .flex_col()
                        .justify_end()
                        .when_some(preview_problem, |overlay, active| {
                            overlay.child(self.preview_error_panel(&active, cx))
                        })
                        .when(has_warnings, |overlay| {
                            overlay.child(self.preview_warnings_strip(cx))
                        }),
                )
            });
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .h_full()
            .when(shows_tools, |panel| {
                panel.child(self.tool_row(pivot_note, cx))
            })
            .child(canvas_area)
            .child(self.transport_bar(cx))
    }

    /// The active preview error, centered in the space above the warnings
    /// strip: its title, the whole detail (selectable, with its own copy
    /// button), and a way into the Problems tab. Only the detail scrolls, so
    /// the title and the button stay in view.
    fn preview_error_panel(
        &self,
        active: &ActiveProblem,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_1()
            .min_h_0()
            .items_center()
            .justify_center()
            .p_6()
            .child(
                div()
                    .id("preview-error")
                    .flex()
                    .flex_col()
                    .gap_2()
                    .w_full()
                    .max_w(rems(35.))
                    .max_h_full()
                    .rounded(cx.theme().radius)
                    .border_1()
                    .border_color(cx.theme().danger.opacity(0.5))
                    .bg(cx.theme().popover)
                    .p_3()
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .gap_2()
                            .text_sm()
                            .text_color(cx.theme().danger)
                            .child(Icon::new(IconName::CircleX).small())
                            .child(active.problem.title.clone()),
                    )
                    // The detail keeps its own height until the surface hits
                    // its max height, then shrinks and scrolls. A plain scroll
                    // container rather than `overflow_y_scrollbar()`: that
                    // wrapper sizes its scroll area at 100% of a height this
                    // auto-sized column never makes definite, so it collapses.
                    .child(
                        div()
                            .relative()
                            .flex()
                            .flex_col()
                            .flex_initial()
                            .min_h_0()
                            .child(
                                div()
                                    .id("preview-error-detail-scroll")
                                    .flex_initial()
                                    .min_h_0()
                                    .overflow_y_scroll()
                                    .track_scroll(&self.preview_error_scroll)
                                    .child(detail_block(
                                        "preview-error-detail".into(),
                                        &active.problem.detail,
                                        cx,
                                    )),
                            )
                            .vertical_scrollbar(&self.preview_error_scroll),
                    )
                    .child(show_in_problems_row("preview-error-show-problems", cx)),
            )
    }

    /// Scrolls the preview error's detail back to the top when a different
    /// error replaces the one it was scrolled for.
    pub(crate) fn sync_preview_error_scroll(&mut self) {
        let time = self
            .problems
            .active(Source::Preview)
            .map(|active| active.time);
        if time != self.preview_error_time {
            self.preview_error_time = time;
            self.preview_error_scroll.set_offset(point(px(0.), px(0.)));
        }
    }

    /// The current frame's preview warnings (font fallbacks), along the
    /// bottom of the viewer.
    fn preview_warnings_strip(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex_none()
            .mx_2()
            .mb_2()
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
            .children(self.preview_warnings_summary("preview-warnings-show-problems", cx))
    }

    /// The preview warnings as a summary for a warnings box: the first
    /// [`SUMMARY_WARNING_LIMIT`](crate::problems::SUMMARY_WARNING_LIMIT),
    /// each flowed into two lines at most, how many more there are, and a
    /// way into the Problems tab, which has the full text.
    pub(crate) fn preview_warnings_summary(
        &self,
        button_id: &'static str,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let summary = summarize_warnings(self.problems.preview_warnings());
        let mut lines: Vec<AnyElement> = summary
            .shown
            .iter()
            .map(|warning| {
                div()
                    .line_clamp(2)
                    .text_ellipsis()
                    .child(flow_line_breaks(warning).into_owned())
                    .into_any_element()
            })
            .collect();
        if summary.hidden > 0 {
            lines.push(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("{} more", summary.hidden))
                    .into_any_element(),
            );
        }
        if !summary.shown.is_empty() {
            lines.push(show_in_problems_row(button_id, cx).into_any_element());
        }
        lines
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
        let (full, show_marks) = transport_layout(width);
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
            )
            .when(self.is_react_preview(), |bar| {
                let button = Button::new("toggle-interactive")
                    .small()
                    .ghost()
                    .selected(self.interactive);
                // Leaving waits for held commits: still on, and clickable to
                // stay.
                let button = if self.element_rows.leaving_interactive {
                    button.icon(Spinner::new()).tooltip_with_action(
                        "Turning off Select layers once saved…",
                        &ToggleInteractive,
                        KEY_CONTEXT,
                    )
                } else {
                    button
                        .icon(CelestaIcon::MousePointerClick)
                        .tooltip_with_action("Select layers", &ToggleInteractive, KEY_CONTEXT)
                };
                bar.child(
                    button.on_click(cx.listener(|this, _, _, cx| this.toggle_interactive(cx))),
                )
            });
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

    /// The canvas tools, one row docked above the viewer (part of its
    /// layout, so the canvas shrinks while it shows; see `shows_tool_row`):
    /// "Rotate about" [Center | Pivot] (what rotation and scale turn about)
    /// and "Handles" [Size | Scale] (what the corner and edge handles do).
    /// The chosen tab is the selected pill; when it is not in effect on the
    /// selected layer, a warning after its tabs gives the reason
    /// (`pivot_note`, see `EditorView::pivot_note`, and `resize_note`).
    fn tool_row(
        &self,
        pivot_note: Option<&'static str>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let pivot = |id, label, tooltip, mode: PivotMode| ToolTab {
            id,
            label,
            tooltip,
            chosen: self.pivot_mode == mode,
            on_click: Box::new(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.set_pivot_mode(mode, cx);
            })),
        };
        let pivot_tabs = [
            pivot(
                "pivot-center",
                "Center",
                "Rotate and scale about the layer's center",
                PivotMode::Center,
            ),
            pivot(
                "pivot-pivot",
                "Pivot",
                "Rotate and scale about the pivot (anchor + origin)",
                PivotMode::Pivot,
            ),
        ];
        let tool = |id, label, tooltip, tool: ResizeTool| ToolTab {
            id,
            label,
            tooltip,
            chosen: self.resize_tool == tool,
            on_click: Box::new(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.set_resize_tool(tool, cx);
            })),
        };
        let tool_tabs = [
            tool(
                "handles-size",
                "Size",
                "Resize the layer's box, keeping the opposite edge",
                ResizeTool::Size,
            ),
            tool(
                "handles-scale",
                "Scale",
                "Scale the layer",
                ResizeTool::Scale,
            ),
        ];
        let rotate = mode_toggle(
            "pivot-mode-note",
            pivot_tabs,
            pivot_note.map(str::to_owned),
            cx,
        );
        let handles = mode_toggle("handles-note", tool_tabs, self.resize_note(), cx);
        div()
            .id("canvas-tools")
            .flex()
            .flex_none()
            .items_center()
            .gap_6()
            .h(px(TOOL_ROW_HEIGHT))
            .w_full()
            // A narrow viewer clips the row rather than painting it over
            // the dock beside it.
            .min_w_0()
            .overflow_hidden()
            .px_3()
            .bg(cx.theme().secondary)
            .border_b_1()
            .border_color(cx.theme().border)
            .child(tool_group("Rotate about", rotate, cx))
            .child(tool_group("Handles", handles, cx))
    }

    fn set_pivot_mode(&mut self, mode: PivotMode, cx: &mut Context<Self>) {
        if self.pivot_mode != mode {
            self.pivot_mode = mode;
            cx.notify();
        }
    }

    fn set_resize_tool(&mut self, tool: ResizeTool, cx: &mut Context<Self>) {
        if self.resize_tool != tool {
            self.resize_tool = tool;
            cx.notify();
        }
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

    /// Switches the React preview between its plain mode and the
    /// interactive mode that picks layers. The bridge respawns with or
    /// without source positions; entering interactive mode sweeps the
    /// composition for scenes, and leaving it drops the selection and the
    /// sweep's index.
    ///
    /// Leaving is a click on nothing first: the rows' typed text is
    /// submitted, and when their commits are held the mode stays until they
    /// are written (frames go on looking the source up, which they wait
    /// for); toggling again meanwhile stays in the mode.
    pub(crate) fn toggle_interactive(&mut self, cx: &mut Context<Self>) {
        if !self.is_react_preview() {
            return;
        }
        let leaving = self.element_rows.leaving_interactive;
        if self.interactive && !leaving {
            self.select_layer(None, cx);
        }
        let waits = self.element_rows.selection.waiting();
        match interactive_toggle(self.interactive, leaving, waits) {
            InteractiveToggle::Enter => {
                self.interactive = true;
                self.refresh_preview();
                self.refresh_occurrences(false);
            }
            InteractiveToggle::Leave => {
                self.leave_interactive();
                self.settle_interactive_mode();
            }
            InteractiveToggle::LeaveAfterCommits => self.element_rows.leaving_interactive = true,
            InteractiveToggle::StayInteractive => {
                self.element_rows.leaving_interactive = false;
                self.element_rows.selection.clear();
            }
        }
        cx.notify();
    }

    /// Leaves the interactive mode, its selection already cleared. The
    /// frame and the sweep without it are asked for once, not here: a
    /// reload handler that clears the selection asks for both right after,
    /// and `settle_interactive_mode` does otherwise.
    fn leave_interactive(&mut self) {
        self.element_rows.leaving_interactive = false;
        self.interactive = false;
        self.hit_regions.clear();
        self.interactive_refresh_due = true;
    }

    /// Asks for the frame and the sweep a leave of the interactive mode
    /// left due, unless a reload runs: its answer asks for both.
    pub(crate) fn settle_interactive_mode(&mut self) {
        if self.interactive_refresh_due && !self.react_reloads.is_running() {
            self.refresh_preview();
            self.refresh_occurrences(false);
        }
    }

    /// Selects the topmost layer under the click in the shown frame, or
    /// clears the selection when the click hits none, and looks up the new
    /// selection's source.
    fn select_layer_at(&mut self, event: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(bounds) = self.viewer_bounds.get() else {
            return;
        };
        let settings = &self.document.project().settings;
        let frame_size = (settings.width as f32, settings.height as f32);
        let hit = scene_point(bounds, frame_size, event.position)
            .and_then(|point| hit_test(&self.hit_regions, point))
            .map(str::to_owned);
        // A press on the selection's handles, or on its body, drags it.
        if self.begin_transform_drag(event.position, hit.as_deref(), event.modifiers.alt) {
            cx.notify();
            return;
        }
        self.select_layer(hit, cx);
    }

    /// Makes `layer` the selection (or clears it) at once, looking up its
    /// source; `select_layer` is what a person's request goes through.
    pub(crate) fn change_selection(&mut self, layer: Option<String>, cx: &mut Context<Self>) {
        if layer == self.selected_layer {
            return;
        }
        self.clear_layer_selection();
        self.selected_layer = layer;
        if self.selected_layer.is_some() {
            self.source_lookup = Some(SourceLookup::Pending);
            self.refresh_preview();
        } else {
            self.source_lookup = None;
        }
        cx.notify();
    }

    /// Forgets the selected layer and its source, the commits held for it,
    /// and a selection change waiting for them.
    pub(crate) fn clear_layer_selection(&mut self) {
        self.forget_held_rows();
        self.selected_layer = None;
        self.source_lookup = None;
        self.source_lookup_stale = false;
        self.launch_error = None;
        self.element_edit.reset();
        self.layer_frame = None;
        self.transform_drag = None;
        self.element_rows.scrub = None;
        // A leave of the interactive mode waited for this.
        if self.element_rows.leaving_interactive {
            self.leave_interactive();
        }
    }
}

fn transport_separator(cx: &Context<EditorView>) -> impl IntoElement {
    div().mx_1().w(px(1.0)).h(px(16.0)).bg(cx.theme().border)
}

/// Action-safe (90 %) and title-safe (80 %) frames plus a center cross,
/// drawn over the letterboxed frame rather than the whole viewer.
fn paint_safe_areas(
    bounds: Bounds<Pixels>,
    (frame_width, frame_height): (f32, f32),
    color: Hsla,
    window: &mut Window,
) {
    let Some((left, top, scale)) = letterbox(bounds, (frame_width, frame_height)) else {
        return;
    };
    let (width, height) = (frame_width * scale, frame_height * scale);
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

/// A 1px outline around a selected layer's quad, its scene-pixel corners
/// placed on the letterboxed frame; dashed when nothing of it can be dragged.
fn paint_layer_outline(
    bounds: Bounds<Pixels>,
    frame_size: (f32, f32),
    corners: &[Point; 4],
    dashed: bool,
    window: &mut Window,
) {
    let Some((left, top, scale)) = letterbox(bounds, frame_size) else {
        return;
    };
    let [first, rest @ ..] = corners.map(|corner| {
        point(
            px(left + corner.x as f32 * scale),
            px(top + corner.y as f32 * scale),
        )
    });
    let mut path = PathBuilder::stroke(px(1.0));
    if dashed {
        path = path.dash_array(&[px(4.0), px(3.0)]);
    }
    path.move_to(first);
    for corner in rest {
        path.line_to(corner);
    }
    path.close();
    if let Ok(path) = path.build() {
        window.paint_path(path, theme::accent());
    }
}

/// The handles of a draggable selection, placed on the viewer.
#[derive(Default)]
struct PlacedHandles {
    /// Each handle's square and the hitbox that sets its cursor.
    squares: Vec<(Handle, Bounds<Pixels>, Hitbox)>,
    /// The rotation handle's stem, from the top edge's middle.
    stem: Option<(gpui_kit::Point<Pixels>, gpui_kit::Point<Pixels>)>,
}

/// The pivot cross, placed on the viewer.
struct PlacedCross {
    at: gpui_kit::Point<Pixels>,
    /// Sets the cursor while Alt is held: a grab when Alt-dragging it moves
    /// the anchor, not-allowed when something keeps the anchor.
    hitbox: Option<(Hitbox, CrossDrag)>,
}

/// What Alt does on the pivot cross.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CrossDrag {
    /// Alt-dragging it moves the anchor.
    Moves,
    /// Something keeps the anchor; its tooltip says what.
    Blocked,
    /// The element has no anchor to move (a `<Path>`).
    NoAnchor,
}

impl CrossDrag {
    fn of(draggable: bool, reason: &Option<String>) -> Self {
        match (draggable, reason) {
            (true, _) => Self::Moves,
            (false, Some(_)) => Self::Blocked,
            (false, None) => Self::NoAnchor,
        }
    }
}

/// The square a handle centered on `at` (scene pixels) covers on the frame
/// fitted into `bounds`, in window pixels.
fn handle_square(
    bounds: Bounds<Pixels>,
    frame_size: (f32, f32),
    at: Point,
) -> Option<Bounds<Pixels>> {
    let (left, top, scale) = letterbox(bounds, frame_size)?;
    let half = px(HANDLE_SIZE / 2.0);
    let center = point(
        px(left + at.x as f32 * scale),
        px(top + at.y as f32 * scale),
    );
    Some(Bounds::new(
        point(center.x - half, center.y - half),
        size(px(HANDLE_SIZE), px(HANDLE_SIZE)),
    ))
}

/// Places the pivot cross at `at` (scene pixels), with a hitbox for its
/// cursor when Alt does something there (see `CrossDrag`).
fn place_cross(
    bounds: Bounds<Pixels>,
    frame_size: (f32, f32),
    at: Point,
    drag: CrossDrag,
    window: &mut Window,
) -> Option<PlacedCross> {
    let square = handle_square(bounds, frame_size, at)?;
    Some(PlacedCross {
        at: square.center(),
        hitbox: (drag != CrossDrag::NoAnchor).then(|| {
            let hitbox = window.insert_hitbox(square.dilate(px(2.0)), HitboxBehavior::Normal);
            (hitbox, drag)
        }),
    })
}

/// The pivot cross, a handle wide, in the accent color; a grab cursor over
/// it while Alt is held, when that drags the anchor.
fn paint_cross(cross: &PlacedCross, window: &mut Window) {
    let accent = theme::accent();
    let arm = px(HANDLE_SIZE / 2.0);
    let at = cross.at;
    window.paint_quad(fill(
        Bounds::new(point(at.x - arm, at.y), size(arm * 2.0, px(1.0))),
        accent,
    ));
    window.paint_quad(fill(
        Bounds::new(point(at.x, at.y - arm), size(px(1.0), arm * 2.0)),
        accent,
    ));
    if let Some((hitbox, drag)) = &cross.hitbox
        && window.modifiers().alt
    {
        let cursor = match drag {
            CrossDrag::Moves => CursorStyle::OpenHand,
            _ => CursorStyle::OperationNotAllowed,
        };
        window.set_cursor_style(cursor, hitbox);
    }
}

/// The square of the pivot cross at `at` (scene pixels) on the frame fitted
/// into `bounds`, relative to `bounds`' origin.
fn cross_box(bounds: Bounds<Pixels>, frame_size: (f32, f32), at: Point) -> Option<Bounds<Pixels>> {
    let square = handle_square(bounds, frame_size, at)?;
    Some(Bounds::new(square.origin - bounds.origin, square.size))
}

/// Places the handles `editable` allows on the frame fitted into `bounds`,
/// for the pivot mode and the Size tool in use.
fn place_handles(
    bounds: Bounds<Pixels>,
    frame_size: (f32, f32),
    frame: &LayerFrame,
    editable: &[crate::element_edit::PropKey],
    (pivot, resize): (PivotMode, Option<&Resize>),
    window: &mut Window,
) -> Option<PlacedHandles> {
    let (left, top, scale) = letterbox(bounds, frame_size)?;
    let on_viewer = |p: Point| point(px(left + p.x as f32 * scale), px(top + p.y as f32 * scale));
    let stem = f64::from(ROTATE_STEM / scale);
    let squares = shown_handles(frame, editable, stem, pivot, resize)
        .into_iter()
        .filter_map(|(handle, at)| {
            let square = handle_square(bounds, frame_size, at)?;
            let hitbox = window.insert_hitbox(square.dilate(px(2.0)), HitboxBehavior::Normal);
            Some((handle, square, hitbox))
        })
        .collect::<Vec<_>>();
    let rotates = squares
        .iter()
        .any(|(handle, _, _)| *handle == Handle::Rotate);
    Some(PlacedHandles {
        squares,
        stem: rotates.then(|| {
            let (from, to) = rotate_stem(frame, stem, resize);
            (on_viewer(from), on_viewer(to))
        }),
    })
}

/// Square handles in the accent color and a stem to the rotation handle.
/// Sets each handle's cursor.
fn paint_handles(placed: &PlacedHandles, handle_fill: Hsla, window: &mut Window) {
    let accent = theme::accent();
    if let Some((from, to)) = placed.stem {
        let mut path = PathBuilder::stroke(px(1.0));
        path.move_to(from);
        path.line_to(to);
        if let Ok(path) = path.build() {
            window.paint_path(path, accent);
        }
    }
    for (handle, square, hitbox) in &placed.squares {
        window.paint_quad(fill(*square, handle_fill));
        window.paint_quad(outline(*square, accent, BorderStyle::Solid));
        window.set_cursor_style(handle_cursor(*handle), hitbox);
    }
}

fn handle_cursor(handle: Handle) -> CursorStyle {
    match handle {
        Handle::Corner(0 | 2) => CursorStyle::ResizeUpLeftDownRight,
        Handle::Corner(_) => CursorStyle::ResizeUpRightDownLeft,
        Handle::Edge(1 | 3) => CursorStyle::ResizeLeftRight,
        Handle::Edge(_) => CursorStyle::ResizeUpDown,
        Handle::Rotate => CursorStyle::Crosshair,
        Handle::Anchor => CursorStyle::OpenHand,
        Handle::Move => CursorStyle::Arrow,
    }
}

/// The box around `frame`'s outline on the frame fitted into `bounds`,
/// relative to `bounds`' origin.
fn outline_box(
    bounds: Bounds<Pixels>,
    frame_size: (f32, f32),
    frame: &LayerFrame,
) -> Option<Bounds<Pixels>> {
    let (left, top, scale) = letterbox(bounds, frame_size)?;
    let xs = frame.outline.map(|p| left + p.x as f32 * scale);
    let ys = frame.outline.map(|p| top + p.y as f32 * scale);
    let min = |values: [f32; 4]| values.into_iter().fold(f32::INFINITY, f32::min);
    let max = |values: [f32; 4]| values.into_iter().fold(f32::NEG_INFINITY, f32::max);
    Some(Bounds::new(
        point(px(min(xs)) - bounds.origin.x, px(min(ys)) - bounds.origin.y),
        size(px(max(xs) - min(xs)), px(max(ys) - min(ys))),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tool_row_shows_only_with_a_selection_in_interactive_mode() {
        // (React preview, Select layers on, a layer selected)
        assert!(shows_tool_row(true, true, true));
        assert!(!shows_tool_row(true, true, false));
        assert!(!shows_tool_row(true, false, true));
        assert!(!shows_tool_row(false, true, true));
    }

    #[test]
    fn the_transport_bar_collapses_at_its_own_widths() {
        // The canvas tools sit in a row above the viewer, not in the bar:
        // its breakpoints are the bar's own.
        assert_eq!(transport_layout(820.0), (true, true));
        assert_eq!(transport_layout(819.0), (false, true));
        assert_eq!(transport_layout(600.0), (false, true));
        assert_eq!(transport_layout(599.0), (false, false));
    }
}
