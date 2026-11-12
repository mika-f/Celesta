//! The Export panel's rows, section headers and group lines, laid out as
//! the timeline lays out its rows: a label lane of
//! [`metrics::inspector_label_width`] and a value lane with the rest, so
//! every label and every control starts on one edge.

use celesta_editor_theme::metrics;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, App, ElementId, FontWeight, SharedString, div};

/// A row: `label` in the label lane (muted, one line, ending in an
/// ellipsis when it does not fit; the full text is always in a tooltip),
/// `control` in the value lane, and `lines` (a description, a warning, an
/// error) under the control, from the value lane's edge. The label is
/// `export-label-{id}`, the value lane `export-control-{id}`, and an empty
/// marker right after the control, where it ends, `export-control-end-{id}`.
pub(super) fn row(
    id: SharedString,
    label: SharedString,
    control: AnyElement,
    lines: Vec<AnyElement>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let label_id = SharedString::from(format!("export-label-{id}"));
    let tooltip = label.clone();
    let has_lines = !lines.is_empty();
    v_flex()
        .w_full()
        .child(
            h_flex()
                .w_full()
                .min_h(metrics::list_row_height())
                .child(
                    div()
                        .id(ElementId::Name(label_id.clone()))
                        .debug_selector(|| label_id.to_string())
                        .flex_none()
                        .w(metrics::inspector_label_width())
                        .pr_2()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
                        .child(label),
                )
                .child(
                    h_flex()
                        .debug_selector(|| format!("export-control-{id}"))
                        .flex_1()
                        .min_w_0()
                        .child(control)
                        // Placed right after the control, so it is where
                        // the control ends, inside the lane or past it.
                        .child(
                            div()
                                .debug_selector(|| format!("export-control-end-{id}"))
                                .flex_none(),
                        ),
                ),
        )
        .when(has_lines, |row| {
            row.child(
                v_flex()
                    .pl(metrics::inspector_label_width())
                    .pb_1()
                    .gap_1()
                    .children(lines),
            )
        })
        .into_any_element()
}

/// A section's header: a band of the header color, a row tall, with one
/// hairline under it and `title` at the panel's inset. It does not
/// collapse.
pub(super) fn section_header(title: &'static str, cx: &App) -> AnyElement {
    let theme = cx.theme();
    h_flex()
        .flex_none()
        .w_full()
        .h(metrics::list_row_height())
        .px(metrics::panel_padding())
        .bg(theme.title_bar)
        .border_b_1()
        .border_color(theme.border)
        .text_sm()
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme.foreground)
        .child(title)
        .into_any_element()
}

/// A group's name on a line of its own, on the label lane's edge, with a
/// gap above it; its fields follow in the same lanes.
pub(super) fn group_line(id: SharedString, title: SharedString, cx: &App) -> AnyElement {
    let theme = cx.theme();
    h_flex()
        .debug_selector(|| format!("export-group-{id}"))
        .w_full()
        .mt_3()
        .min_h(metrics::list_row_height())
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .text_sm()
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme.foreground)
        .child(title)
        .into_any_element()
}
