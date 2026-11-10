//! The chosen encoders' fields: a control per shown field, kept for the
//! encoder it was built for, and how each is drawn.

use super::super::model::{FieldRow, Track};
use super::choice::Choice;
use super::{ExportDialog, muted_line, problem_line, track_id};
use crate::property_controls::parse_number_input;
use crate::widgets::format_component_number;
use celesta_media::{ComponentRef, SettingKind, SettingValue};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::{ActiveTheme as _, v_flex};
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
    track: Track,
    component: ComponentRef,
    pub(super) key: String,
    pub(super) label: String,
    pub(super) widget: Widget,
    _subscription: Option<Subscription>,
}

impl ExportDialog {
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
            match parse_number_input(&text) {
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

    /// `track`'s shown fields whose `advanced` is `advanced`: ungrouped
    /// ones, then each group under its label.
    pub(super) fn render_groups(
        &self,
        track: Track,
        advanced: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        self.model
            .fields(track, advanced)
            .into_iter()
            .map(|group| {
                v_flex()
                    .gap_3()
                    .when_some(group.group, |column, group| {
                        column.child(div().pt_2().child(muted_line(group, cx)))
                    })
                    .children(
                        group
                            .rows
                            .iter()
                            .map(|row| self.render_field(track, row, cx)),
                    )
                    .into_any_element()
            })
            .collect()
    }

    fn render_field(&self, track: Track, row: &FieldRow, cx: &mut Context<Self>) -> AnyElement {
        let field = &row.field;
        let id = SharedString::from(format!("export-{}-{}", track_id(track), field.key));
        let control = self
            .fields
            .iter()
            .find(|control| control.track == track && control.key == field.key);
        let (control, typo) = match control.map(|control| &control.widget) {
            Some(Widget::Select(select)) => (Select::new(select).id(id).into_any_element(), None),
            Some(Widget::Text { input, typo, .. }) => {
                let unit = match &field.kind {
                    SettingKind::Number { unit, .. } => unit.clone(),
                    _ => None,
                };
                let muted = cx.theme().muted_foreground;
                (
                    Input::new(input)
                        .id(id)
                        .when_some(unit, |input, unit| {
                            input.suffix(div().text_xs().text_color(muted).child(unit))
                        })
                        .into_any_element(),
                    typo.clone(),
                )
            }
            Some(Widget::Checkbox) => {
                let key = field.key.clone();
                let checked = matches!(row.value, SettingValue::Boolean(true));
                (
                    Checkbox::new(id)
                        .checked(checked)
                        .label(field.label.clone())
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
        let checkbox = matches!(field.kind, SettingKind::Boolean);
        let error = typo.or_else(|| {
            row.error
                .as_ref()
                .map(|message| format!("{} {message}", field.label))
        });
        v_flex()
            .gap_1()
            .when(!checkbox, |row| {
                row.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().foreground)
                        .child(field.label.clone()),
                )
            })
            .child(control)
            .when_some(error, |row, error| {
                row.child(problem_line(error, cx.theme().danger))
            })
            .when_some(field.description.clone(), |row, description| {
                row.child(muted_line(description, cx))
            })
            .into_any_element()
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
                (true, SettingValue::Number(value)) => parse_number_input(&text) == Ok(*value),
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
