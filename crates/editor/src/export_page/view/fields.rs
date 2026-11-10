//! The chosen encoders' fields: a control per shown field, kept for the
//! encoder it was built for, and how each is drawn.

use super::super::model::{FieldRow, Track};
use super::choice::Choice;
use super::layout::{group_line, row};
use super::{ExportPage, muted_line, problem_line, track_id};
use crate::property_controls::parse_number_input;
use crate::widgets::format_component_number;
use celesta_media::{ComponentRef, SettingKind, SettingValue};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::{ActiveTheme as _, Sizable as _};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Context, Entity, Focusable as _, SharedString, Subscription, Window, div,
};

/// The control of one encoder field.
pub(super) enum Widget {
    Select(Entity<SelectState<Vec<Choice<String>>>>),
    /// A number or a string; `typo` is why the typed number is not one.
    Text {
        input: Entity<InputState>,
        number: bool,
        typo: Option<String>,
    },
    /// Stateless; rendered from the field's value.
    Checkbox,
}

/// One encoder field's control, for the component it was built for.
pub(super) struct FieldControl {
    pub(super) track: Track,
    component: ComponentRef,
    pub(super) key: String,
    pub(super) label: String,
    pub(super) widget: Widget,
    _subscription: Option<Subscription>,
}

impl ExportPage {
    /// Typing in a text field: a string is taken as typed, a number once
    /// it parses (until then the field says why not).
    fn typed(&mut self, track: Track, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(control) = self
            .fields
            .iter_mut()
            .find(|control| control.track == track && control.key == key)
        else {
            return;
        };
        let Widget::Text {
            input,
            number,
            typo,
        } = &mut control.widget
        else {
            return;
        };
        let text = input.read(cx).value().to_string();
        let value = if *number {
            match parse_number_input(&text, None) {
                Ok(value) => SettingValue::Number(value),
                Err(error) => {
                    *typo = Some(error);
                    cx.notify();
                    return;
                }
            }
        } else {
            SettingValue::String(text)
        };
        *typo = None;
        self.set_setting(track, key, value, window, cx);
    }

    /// Keeps one control per shown field of each chosen encoder, built for
    /// that encoder, showing the field's value (a focused text field keeps
    /// what is being typed).
    pub(super) fn sync_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut kept = Vec::new();
        for track in [Track::Video, Track::Audio] {
            let component = self.model.choice().and_then(|choice| match track {
                Track::Video => Some(choice.video.component.clone()),
                Track::Audio => choice.audio.as_ref().map(|audio| audio.component.clone()),
            });
            let Some(component) = component else {
                continue;
            };
            let rows: Vec<FieldRow> = [false, true]
                .into_iter()
                .flat_map(|advanced| self.model.fields(track, advanced))
                .flat_map(|group| group.rows)
                .collect();
            for row in rows {
                let position = self.fields.iter().position(|control| {
                    control.track == track
                        && control.key == row.field.key
                        && control.component == component
                });
                let mut control = match position {
                    Some(position) => self.fields.swap_remove(position),
                    None => self.field_control(track, &component, &row, window, cx),
                };
                show_value(&mut control, &row.value, window, cx);
                kept.push(control);
            }
        }
        // What is left belongs to a field no longer shown.
        self.fields = kept;
    }

    fn field_control(
        &mut self,
        track: Track,
        component: &ComponentRef,
        row: &FieldRow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> FieldControl {
        let key = row.field.key.clone();
        let (widget, subscription) = match &row.field.kind {
            SettingKind::Select { options } => {
                let choices: Vec<_> = options
                    .iter()
                    .map(|option| Choice::new(option.label.clone(), option.value.clone()))
                    .collect();
                let select = cx.new(|cx| SelectState::new(choices, None, window, cx));
                let key = key.clone();
                let subscription =
                    cx.subscribe_in(&select, window, move |this, _, event, window, cx| {
                        let SelectEvent::Confirm(Some(value)) = event else {
                            return;
                        };
                        let value = SettingValue::String(value.clone());
                        this.set_setting(track, &key, value, window, cx);
                    });
                (Widget::Select(select), Some(subscription))
            }
            SettingKind::Number { .. } | SettingKind::String => {
                let input = cx.new(|cx| InputState::new(window, cx));
                let key = key.clone();
                let subscription =
                    cx.subscribe_in(&input, window, move |this, _, event, window, cx| {
                        // `set_value` emits no change, so this is the person
                        // typing.
                        if let InputEvent::Change = event {
                            this.typed(track, &key, window, cx);
                        }
                    });
                let widget = Widget::Text {
                    input,
                    number: matches!(row.field.kind, SettingKind::Number { .. }),
                    typo: None,
                };
                (widget, Some(subscription))
            }
            SettingKind::Boolean => (Widget::Checkbox, None),
        };
        FieldControl {
            track,
            component: component.clone(),
            key,
            label: row.field.label.clone(),
            widget,
            _subscription: subscription,
        }
    }

    /// `track`'s shown fields whose `advanced` is `advanced`, a row each:
    /// ungrouped ones, then each group after a line with its name.
    pub(super) fn render_groups(
        &self,
        track: Track,
        advanced: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut rows = Vec::new();
        for group in self.model.fields(track, advanced) {
            if let Some(name) = group.group {
                let id = SharedString::from(format!("{}-{name}", track_id(track)));
                rows.push(group_line(id, name.into(), cx));
            }
            rows.extend(
                group
                    .rows
                    .iter()
                    .map(|row| self.render_field(track, row, cx)),
            );
        }
        rows
    }

    fn render_field(
        &self,
        track: Track,
        field_row: &FieldRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let field = &field_row.field;
        let row_id = format!("{}-{}", track_id(track), field.key);
        let id = SharedString::from(format!("export-{row_id}"));
        let control = self
            .fields
            .iter()
            .find(|control| control.track == track && control.key == field.key);
        let (control, typo) = match control.map(|control| &control.widget) {
            Some(Widget::Select(select)) => (
                Select::new(select)
                    .id(id)
                    .small()
                    .accessibility_label(field.label.clone())
                    .into_any_element(),
                None,
            ),
            Some(Widget::Text { input, typo, .. }) => {
                let unit = match &field.kind {
                    SettingKind::Number { unit, .. } => unit.clone(),
                    _ => None,
                };
                let muted = cx.theme().muted_foreground;
                (
                    Input::new(input)
                        .id(id)
                        .small()
                        .when_some(unit, |input, unit| {
                            input.suffix(div().text_sm().text_color(muted).child(unit))
                        })
                        .into_any_element(),
                    typo.clone(),
                )
            }
            Some(Widget::Checkbox) => {
                let key = field.key.clone();
                let checked = matches!(field_row.value, SettingValue::Boolean(true));
                (
                    Checkbox::new(id)
                        .small()
                        .checked(checked)
                        .accessibility_label(field.label.clone())
                        .on_click(cx.listener(move |this, checked: &bool, window, cx| {
                            this.set_setting(
                                track,
                                &key,
                                SettingValue::Boolean(*checked),
                                window,
                                cx,
                            );
                        }))
                        .into_any_element(),
                    None,
                )
            }
            // Built on the next sync.
            None => (div().into_any_element(), None),
        };
        let error = typo.or_else(|| {
            field_row
                .error
                .as_ref()
                .map(|message| format!("{} {message}", field.label))
        });
        let mut lines = Vec::new();
        lines.extend(error.map(|error| problem_line(error, cx.theme().danger).into_any_element()));
        lines.extend(field.description.clone().map(|description| {
            div()
                .debug_selector(|| format!("export-description-{row_id}"))
                .child(muted_line(description, cx))
                .into_any_element()
        }));
        row(
            row_id.clone().into(),
            field.label.clone().into(),
            control,
            lines,
            cx,
        )
    }
}

/// Shows `value` in `control`, unless its text has the focus. Replacing
/// the text drops what was wrong with the text it replaces.
fn show_value(control: &mut FieldControl, value: &SettingValue, window: &mut Window, cx: &mut App) {
    match (&mut control.widget, value) {
        (Widget::Select(select), SettingValue::String(value)) => {
            if select.read(cx).selected_value() != Some(value) {
                select.update(cx, |state, cx| state.set_selected_value(value, window, cx));
            }
        }
        (
            Widget::Text {
                input,
                number,
                typo,
            },
            value,
        ) => {
            if input.focus_handle(cx).is_focused(window) {
                return;
            }
            let text = input.read(cx).value();
            let shown = match (number, value) {
                (true, SettingValue::Number(value)) => {
                    parse_number_input(&text, None) == Ok(*value)
                }
                _ => text.as_ref() == value.to_string(),
            };
            if !shown {
                let text = match value {
                    SettingValue::Number(value) => format_component_number(*value),
                    other => other.to_string(),
                };
                input.update(cx, |input, cx| input.set_value(text, window, cx));
                *typo = None;
            }
        }
        _ => {}
    }
}
