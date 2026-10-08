use celesta_react_bridge::ComponentPropertyField;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::*;
use gpui_kit::{App, SharedString, div, px};

pub(crate) fn panel_header(title: &'static str, count: usize, cx: &App) -> impl IntoElement {
    div()
        .flex()
        .flex_none()
        .h(px(40.0))
        .items_center()
        .justify_between()
        .px_3()
        .border_b_1()
        .border_color(cx.theme().border)
        .text_sm()
        .text_color(cx.theme().foreground)
        .child(title)
        .when(count > 0, |header| {
            header.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(count.to_string()),
            )
        })
}

pub(crate) fn inspector_row(
    label: impl Into<SharedString>,
    value: impl Into<SharedString>,
    cx: &App,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .px_3()
        .py_2()
        .border_b_1()
        .border_color(cx.theme().border)
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(label.into()),
        )
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().foreground)
                .child(value.into()),
        )
}

pub(crate) fn inspector_section(title: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    div()
        .mt_3()
        .px_3()
        .py_2()
        .border_t_1()
        .border_b_1()
        .border_color(cx.theme().border)
        .text_sm()
        .text_color(cx.theme().foreground)
        .child(title.into())
}

pub(crate) fn inspector_note(text: &'static str, cx: &App) -> impl IntoElement {
    div()
        .px_3()
        .py_2()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(text)
}

/// The label and current value of one schema-described property, falling
/// back to the field's declared default when the value is unset.
pub(crate) fn property_display(
    key: &str,
    field: &ComponentPropertyField,
    current: Option<&serde_json::Value>,
) -> (SharedString, String) {
    let (label, value) = match field {
        ComponentPropertyField::Boolean {
            label,
            default_value,
            ..
        } => {
            let value = current
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(*default_value);
            (label, if value { "On" } else { "Off" }.to_owned())
        }
        ComponentPropertyField::Number {
            label,
            default_value,
            ..
        } => (
            label,
            format_component_number(
                current
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(*default_value),
            ),
        ),
        ComponentPropertyField::String {
            label,
            default_value,
            ..
        }
        | ComponentPropertyField::Color {
            label,
            default_value,
            ..
        }
        | ComponentPropertyField::Select {
            label,
            default_value,
            ..
        }
        | ComponentPropertyField::Path {
            label,
            default_value,
        } => (
            label,
            current
                .and_then(serde_json::Value::as_str)
                .map_or_else(|| default_value.clone(), str::to_owned),
        ),
    };
    (
        label
            .clone()
            .map_or_else(|| key.to_owned().into(), Into::into),
        value,
    )
}

/// A prop value without a schema: strings unquoted, everything else as JSON.
pub(crate) fn json_display(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

pub(crate) fn format_component_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{value:.0}")
    } else {
        format!("{value}")
    }
}
