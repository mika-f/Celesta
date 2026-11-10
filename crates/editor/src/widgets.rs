use celesta_react_bridge::ComponentPropertyField;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::Button;
use gpui_kit::prelude::*;
use gpui_kit::{App, SharedString, div};

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
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(label.into()),
        )
        .child(
            div()
                .text_base()
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
        .text_base()
        .text_color(cx.theme().foreground)
        .child(title.into())
}

pub(crate) fn inspector_note(text: &'static str, cx: &App) -> impl IntoElement {
    div()
        .px_3()
        .py_2()
        .text_sm()
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

/// A label for an `xsmall` button at the `text_sm` floor. gpui-kit sizes an
/// xsmall button's label `text_xs` (9.75px at the editor's 13px base); this
/// keeps the button's height and reads the label at `text_sm`.
pub(crate) trait CompactLabel {
    fn compact_label(self, label: impl Into<SharedString>) -> Self;
}

impl CompactLabel for Button {
    fn compact_label(self, label: impl Into<SharedString>) -> Self {
        let label = label.into();
        self.accessibility_label(label.clone())
            .child(div().text_sm().child(label))
    }
}
