//! Renders the Properties section of the React Preview panel; the state and
//! the commit flow live in `property_panel.rs`.

use crate::element_edit::schema_field;
use crate::element_rows_view::{field_unit, number_text};
use crate::inspector::InspectorRow;
use crate::property_controls::{Control, ControlKind, number_hint, render_control, text_input};
use crate::property_panel::{
    PropertyRow, TextUpdate, differs_from_default, field_label, issues_by_key, ordered_keys,
    property_control_handler, value_text,
};
use crate::scrub_value::ScrubValue;
use crate::view::EditorView;
use crate::widgets::format_component_number;
use celesta_react_bridge::{ComponentPropertyField, EffectiveValue, ValueSource, effective_values};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{ActiveTheme as _, Sizable as _};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Context, ElementId, SharedString, div};

/// A row control's id, namespaced by the property key.
fn row_id(control: &str, key: &str) -> ElementId {
    SharedString::from(format!("property-{control}-{key}")).into()
}

impl EditorView {
    /// The Properties section, or `None` when the entry declares none.
    pub(crate) fn property_section(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let react = self.react_preview.as_ref()?;
        let schema = react
            .property_schema
            .as_ref()
            .filter(|schema| !schema.fields.is_empty())?;
        let values = effective_values(&schema.fields, &react.disk_properties);
        let (issues, mut general) = issues_by_key(&react.property_issues);
        // The props file could not be read at all: that belongs to no key.
        general.splice(0..0, react.property_load_error.clone());
        // Issues for keys the schema does not declare have no row to sit on.
        general.extend(
            react
                .property_issues
                .iter()
                .filter(|issue| !issue.key.is_empty() && !schema.fields.contains_key(&issue.key))
                .map(ToString::to_string),
        );
        let busy = self.property_panel.is_writing() || self.react_reloads.is_running();
        let danger = cx.theme().danger;
        let rows: Vec<AnyElement> = ordered_keys(schema)
            .into_iter()
            .filter_map(|key| {
                let row = self.property_panel.rows.get(key)?;
                let effective = values.get(key)?;
                let issues = issues.get(key).map_or(&[][..], Vec::as_slice);
                Some(self.property_row(key, row, effective, issues, cx))
            })
            .collect();
        let section = self
            .section("properties/properties", "Properties", cx)
            .children(general.into_iter().map(|message| {
                div()
                    .px(celesta_editor_theme::metrics::inspector_padding())
                    .text_sm()
                    .text_color(danger)
                    .child(message)
            }))
            .children(rows);
        let section = if busy {
            section.push_trailing(Spinner::new().xsmall())
        } else {
            section
        };
        Some(section.into_any_element())
    }

    /// One property in three lanes: its label, its control, and `Reset to
    /// default` when the props file holds a non-default value; its messages
    /// under it.
    fn property_row(
        &self,
        key: &str,
        row: &PropertyRow,
        effective: &EffectiveValue,
        issues: &[String],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // A file edit has no effect on a key --props sets.
        let locked = effective.source == ValueSource::Props;
        let resettable = effective.source == ValueSource::File
            && differs_from_default(&row.field, &effective.value);
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let reset_key = key.to_owned();
        let reset = resettable.then(|| {
            Button::new(row_id("reset", key))
                .ghost()
                .xsmall()
                .icon(IconName::RotateCcw)
                .accessibility_label("Reset to default")
                .tooltip("Reset to default")
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.commit_property(&reset_key, None, TextUpdate::Always, cx);
                }))
        });
        let messages: Vec<AnyElement> = row
            .invalid
            .iter()
            .chain(&row.state.write_error)
            .chain(issues)
            .cloned()
            .map(move |message| div().text_color(danger).child(message).into_any_element())
            .collect();
        let multi_line = row.kind == ControlKind::Text;
        let line = InspectorRow::new(
            format!("property-{key}"),
            field_label(key, &row.field).to_owned(),
        )
        .top_aligned(multi_line)
        .value(self.property_control(key, row, locked, cx));
        let line = match reset {
            Some(reset) => line.trailing(reset),
            None => line,
        };
        let line = if locked {
            line.sub_line(div().text_color(muted).child("Set by --props"))
        } else {
            line
        };
        messages
            .into_iter()
            .fold(line, |line, message| line.sub_line(message))
            .into_any_element()
    }

    fn property_control(
        &self,
        key: &str,
        row: &PropertyRow,
        locked: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hint = match &row.field {
            ComponentPropertyField::Number { min, max, .. } => number_hint(*min, *max),
            // What the select shows when the value is none of its options.
            ComponentPropertyField::Select { .. } => Some(value_text(&row.state.shown)),
            _ => None,
        };
        if let (ControlKind::Number, Control::Text(input)) = (&row.kind, &row.control) {
            let field = schema_field(&row.field);
            let text = match self.property_scrub_shown(key) {
                Some(value) => format_component_number(value),
                None => row
                    .control
                    .text(cx)
                    .map(|text| text.to_string())
                    .unwrap_or_default(),
            };
            let typing = row.state.typing_open(row.invalid.is_some());
            let view = cx.entity().downgrade();
            let type_view = view.clone();
            let (press_key, type_key) = (key.to_owned(), key.to_owned());
            let mut value = ScrubValue::new(
                SharedString::from(format!("property-value-{key}")),
                input.clone(),
            )
            .text(number_text(&text, Some(&field)))
            .unit(field_unit(Some(&field)))
            .typing(typing)
            .locked(locked)
            .on_press(move |x, window, cx| {
                view.update(cx, |this, cx| {
                    this.begin_property_scrub(&press_key, x, window, cx)
                })
                .ok();
            })
            .on_type(move |_, cx| {
                type_view
                    .update(cx, |this, cx| this.type_property_row(&type_key, cx))
                    .ok();
            });
            if typing {
                value = value.field(|input| {
                    text_input(input, locked, hint, cx, property_control_handler(key))
                });
            }
            return value.into_any_element();
        }
        render_control(
            row_id("control", key),
            &row.kind,
            &row.control,
            locked,
            row.state.shown.as_bool().unwrap_or(false),
            hint,
            cx,
            property_control_handler(key),
        )
    }
}
