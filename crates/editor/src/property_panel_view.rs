//! Renders the Properties section of the React Preview panel; the state and
//! the commit flow live in `property_panel.rs`.

use crate::property_controls::{number_hint, render_control};
use crate::property_panel::{
    PropertyRow, TextUpdate, differs_from_default, field_label, issues_by_key, ordered_keys,
    property_control_handler, value_text,
};
use crate::view::EditorView;
use celesta_react_bridge::{ComponentPropertyField, EffectiveValue, ValueSource, effective_values};
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
        let heading = div()
            .flex()
            .items_center()
            .gap_2()
            .text_sm()
            .text_color(cx.theme().foreground)
            .child("Properties")
            .when(busy, |heading| heading.child(Spinner::new().xsmall()));
        let rows: Vec<AnyElement> = ordered_keys(schema)
            .into_iter()
            .filter_map(|key| {
                let row = self.property_panel.rows.get(key)?;
                let effective = values.get(key)?;
                let issues = issues.get(key).map_or(&[][..], Vec::as_slice);
                Some(self.property_row(key, row, effective, issues, cx))
            })
            .collect();
        Some(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .mx_3()
                .pt_3()
                .pb_3()
                .border_t_1()
                .border_color(cx.theme().border)
                .child(heading)
                .children(
                    general
                        .into_iter()
                        .map(|message| div().text_xs().text_color(danger).child(message)),
                )
                .children(rows)
                .into_any_element(),
        )
    }

    /// One property: its label (with `Reset to default` when the props file
    /// holds a non-default value), its control, and its messages.
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
        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .h_5()
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .text_color(muted)
                    .child(field_label(key, &row.field).to_owned()),
            )
            .when(resettable, |header| {
                header.child(
                    Button::new(row_id("reset", key))
                        .ghost()
                        .xsmall()
                        .label("Reset to default")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.commit_property(&reset_key, None, TextUpdate::Always, cx);
                        })),
                )
            });
        let messages = row
            .invalid
            .iter()
            .chain(&row.state.write_error)
            .chain(issues)
            .cloned()
            .map(move |message| div().text_xs().text_color(danger).child(message));
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(header)
            .child(self.property_control(key, row, locked, cx))
            .when(locked, |row| {
                row.child(div().text_xs().text_color(muted).child("Set by --props"))
            })
            .children(messages)
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
