//! Renders the selected layer's property sections; the row state and the
//! commit flow live in `element_rows.rs`.

use crate::element_edit::{
    ElementTargets, PropKey, Resolved, Target, editable, inner_target, keyframe_jump_frames,
    reason_text, row_removable_under, table_key,
};
use crate::element_rows::{
    component_row_label, element_control_handler, row_field, row_label, scrubs, set_at_text,
};
use crate::icons::DiamondFilled;
use crate::inspector::InspectorRow;
use crate::property_controls::{Control, ControlKind, number_hint, render_control, text_input};
use crate::scope::{
    SceneCalls, Scope, ScopedRow, component_section_note, gesture_lock, remove_tooltip, scoped_row,
};
use crate::scrub_value::{ScrubOrigin, ScrubValue, display_number};
use crate::view::EditorView;
use celesta_editor_theme::metrics;
use celesta_react_bridge::{PrimitivePropertyField, SourcePos};
use celesta_source_edit::{EditTarget, LockReason};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Context, Div, ElementId, MouseDownEvent, SharedString, Window, div,
};
use std::path::Path;

/// The hover group of a row, which shows its remove button.
const ROW_GROUP: &str = "element-row";

/// A row's id part: the table key, `row.input` for an input, `call.name`
/// for a component prop.
fn key_id(key: &PropKey) -> String {
    match key {
        PropKey::Prop(name) => name.clone(),
        PropKey::Input { row, name } => format!("{row}.{name}"),
        PropKey::CallSite(name) => format!("call.{name}"),
    }
}

/// An element id for `control` of the row `key`.
fn row_id(control: &str, key: &PropKey) -> ElementId {
    SharedString::from(format!("element-{control}-{}", key_id(key))).into()
}

/// The rows of each section: the table's groups in order with the rows
/// resolved for them (`scaleX`, `scaleY` under `scale`'s group), or one
/// "Transform" section without a table.
fn sections(targets: &ElementTargets) -> Vec<(String, Vec<&(PropKey, Target)>)> {
    let Some(table) = &targets.table else {
        return vec![("Transform".to_owned(), targets.targets.iter().collect())];
    };
    table
        .groups
        .iter()
        .map(|group| {
            let rows = targets
                .targets
                .iter()
                .filter(|(key, _)| {
                    matches!(key, PropKey::Prop(name)
                        if group.fields.iter().any(|field| field.key == table_key(name)))
                })
                .collect::<Vec<_>>();
            (group.name.clone(), rows)
        })
        .filter(|(_, rows)| !rows.is_empty())
        .collect()
}

/// A number row's resting number: the shown value with as many places as
/// the field's step has; text that is not a number as it is.
pub(crate) fn number_text(text: &str, field: Option<&PrimitivePropertyField>) -> String {
    let (step, integer) = match field {
        Some(PrimitivePropertyField::Number { step, integer, .. }) => {
            (*step, integer.unwrap_or(false))
        }
        _ => (None, false),
    };
    match text.trim().parse::<f64>() {
        Ok(value) => display_number(value, step, integer),
        Err(_) => text.to_owned(),
    }
}

/// The unit a number field declares.
pub(crate) fn field_unit(field: Option<&PrimitivePropertyField>) -> Option<SharedString> {
    match field {
        Some(PrimitivePropertyField::Number {
            unit: Some(unit), ..
        }) => Some(unit.clone().into()),
        _ => None,
    }
}

/// "Set at `<file>`:`<line>`" after a `corner-down-right` icon, on one
/// line.
fn set_at_line(call_site: &SourcePos, entry_dir: &Path, cx: &Context<EditorView>) -> Div {
    icon_line(IconName::CornerDownRight, cx).child(
        div()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .child(set_at_text(call_site, entry_dir)),
    )
}

/// [`component_section_note`]'s text after an `info` icon, wrapping in a
/// narrow panel, the icon at the top of the first line.
fn component_note_line(note: &'static str, cx: &Context<EditorView>) -> Div {
    icon_line(IconName::Info, cx)
        .items_start()
        .child(div().flex_1().min_w_0().child(note))
}

/// A muted line starting with `icon`, centered on the text to follow.
fn icon_line(icon: IconName, cx: &Context<EditorView>) -> Div {
    div()
        .flex()
        .items_center()
        .gap_1()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(Icon::new(icon).xsmall())
}

impl EditorView {
    /// The selected layer's property sections, or why there are none yet.
    pub(crate) fn element_sections(&self, cx: &mut Context<Self>) -> AnyElement {
        let note = |text: SharedString, color| {
            div()
                .px(metrics::inspector_padding())
                .py_2()
                .text_sm()
                .text_color(color)
                .child(text)
        };
        let muted = cx.theme().muted_foreground;
        let resolved = self.element_edit.resolved(self.selected_layer.as_deref());
        let Some((resolved, current)) = resolved else {
            return note("Reading source…".into(), muted).into_any_element();
        };
        let targets = match (&resolved.outcome, self.shown_targets()) {
            (Ok(_), Some(targets)) => targets,
            (Err(error), _) => {
                return note(error.clone().into(), cx.theme().danger).into_any_element();
            }
            (Ok(_), None) => return note("Reading source…".into(), muted).into_any_element(),
        };
        let entry_dir = self.entry_dir();
        // The scope the rows show and write under, read once.
        let scope = self.rows_scope(targets);
        let mut column = div().flex().flex_col();
        for (name, rows) in sections(targets) {
            let mut section_rows: Vec<AnyElement> = Vec::new();
            for (key, target) in rows {
                let PropKey::Prop(prop) = key else {
                    continue;
                };
                let field = row_field(targets, key);
                section_rows.push(self.element_row(
                    key,
                    row_label(prop),
                    target,
                    field,
                    false,
                    resolved,
                    current,
                    scope,
                    &entry_dir,
                    cx,
                ));
                for (input, input_target) in &target.inputs {
                    let input_key = PropKey::Input {
                        row: prop.clone(),
                        name: input.clone(),
                    };
                    section_rows.push(self.element_row(
                        &input_key,
                        input.clone(),
                        input_target,
                        None,
                        true,
                        resolved,
                        current,
                        scope,
                        &entry_dir,
                        cx,
                    ));
                }
            }
            let id = format!("properties/{}", name.to_lowercase());
            column = column.child(self.section(id, name, cx).children(section_rows));
        }
        // The props written at the component's call, under its name; none
        // when every prop written there already shows "Set at …" above.
        // Under "Edits change", a line says the choice does not reach them.
        if let Some(component) = targets
            .component
            .as_ref()
            .filter(|component| !component.props.is_empty())
        {
            let state = self.shown_scope_context().map(|context| context.state);
            let note =
                component_section_note(state.as_ref()).map(|note| component_note_line(note, cx));
            let mut section_rows: Vec<AnyElement> = vec![
                div()
                    .px(metrics::inspector_padding())
                    .pb_1()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(set_at_line(&component.call_site, &entry_dir, cx))
                    .children(note)
                    .into_any_element(),
            ];
            for prop in &component.props {
                let key = PropKey::CallSite(prop.name.clone());
                section_rows.push(self.element_row(
                    &key,
                    component_row_label(component, &prop.name),
                    &prop.target,
                    row_field(targets, &key),
                    false,
                    resolved,
                    current,
                    scope,
                    &entry_dir,
                    cx,
                ));
            }
            let title = format!("Component · {}", component.name);
            column = column.child(
                self.section("properties/component", title, cx)
                    .children(section_rows),
            );
        }
        column.into_any_element()
    }

    /// `label  [control]  icon`, plus, below, where a pass-through value is
    /// set, the reason or the keyframe jumps when the target cannot be
    /// edited, and why typed text was not written. An input sub-row
    /// (`indented`) keeps its control on its row's spine. Under `scope`
    /// (with the scene calls that count) the row shows, locks and removes
    /// as [`scoped_row`] says: a row writing a parameter's default is
    /// edited there, not at the call site.
    #[allow(clippy::too_many_arguments)]
    fn element_row(
        &self,
        key: &PropKey,
        label: String,
        target: &Target,
        field: Option<&PrimitivePropertyField>,
        indented: bool,
        resolved: &Resolved,
        current: bool,
        scope: Option<(Scope, Option<&SceneCalls>)>,
        entry_dir: &Path,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let row = self.element_rows.get(key);
        let targets = self.shown_targets();
        // A Component-section row is outside the scope.
        let in_scope = scope.filter(|_| !matches!(key, PropKey::CallSite(_)));
        let scoped = in_scope
            .zip(targets)
            .map(|((scope, calls), targets)| scoped_row(scope, targets, key, calls));
        let scope_locked = matches!(scoped, Some(ScopedRow::Locked(_)));
        let default = matches!(scoped, Some(ScopedRow::Default { .. }));
        // Why the scope can't write the row, nor drag or scrub it: a row
        // this scene sets itself under "Shared value" too, which shows this
        // scene's value and where it is set.
        let scope_note = in_scope
            .zip(targets)
            .and_then(|((scope, calls), targets)| gesture_lock(Some(scope), targets, key, calls));
        // Dragging the label scrubs an editable number (not while playing),
        // unless the scope refuses the scrub.
        let scrubbable = current
            && !self.playing
            && scope_note.is_none()
            && targets.is_some_and(|targets| scrubs(targets, key, target));
        // A written optional value offers its removal on hover, except while
        // a scrub runs or the removal is already being written.
        let removing = row.is_some_and(|row| row.removing())
            || self
                .element_edit
                .written_value(self.selected_layer.as_deref(), key)
                .is_some_and(Option::is_none);
        let can_remove = !removing
            && self.element_rows.scrub.is_none()
            && targets.is_some_and(|targets| row_removable_under(scope, targets, key, target));
        let remove_tooltip = remove_tooltip(
            in_scope.map(|(scope, _)| scope),
            scoped.as_ref().unwrap_or(&ScopedRow::AsResolved),
        );
        // A default is written in the component, not at the call site.
        let call_site = target.call_site.as_ref().filter(|_| !default);
        let target = &target.target;
        // A pass-through row shows the state of the value at the call site.
        let shown = inner_target(target);
        let icon = match shown {
            _ if scope_locked => Some(Icon::new(IconName::Lock)),
            _ if default => None,
            EditTarget::Keyframe { .. } => Some(Icon::new(DiamondFilled)),
            EditTarget::Locked(LockReason::BetweenKeyframes { .. }) => {
                Some(Icon::new(IconName::Diamond))
            }
            EditTarget::Locked(_) => Some(Icon::new(IconName::Lock)),
            EditTarget::Literal { .. }
            | EditTarget::Missing { .. }
            | EditTarget::PassThrough { .. } => None,
        };
        // Only a target that can't be written disables its control: while an
        // own write's reload runs, the field keeps its focus and caret, and
        // a commit made meanwhile waits for the reload. A default is written
        // whatever the call site holds.
        let locked = scope_locked || !default && !editable(target);
        let control = row.map(|row| {
            let hint = match (&row.kind, field) {
                (ControlKind::Number, Some(PrimitivePropertyField::Number { min, max, .. })) => {
                    number_hint(*min, *max)
                }
                // What the select shows when the value is none of its options.
                (ControlKind::Select(_), _) => row.shown.as_ref().map(|(text, placeholder)| {
                    if text.is_empty() {
                        placeholder.clone()
                    } else {
                        text.clone()
                    }
                }),
                _ => None,
            };
            match (&row.kind, &row.control) {
                (ControlKind::Number, Control::Text(input)) => {
                    let (text, placeholder) = match row.control.text(cx) {
                        Some(text) if !text.is_empty() => (text.to_string(), false),
                        _ => (
                            row.shown
                                .as_ref()
                                .map(|(_, placeholder)| placeholder.clone())
                                .unwrap_or_default(),
                            true,
                        ),
                    };
                    let typing = row.typing_open();
                    let view = cx.entity().downgrade();
                    let press_key = key.clone();
                    let type_key = key.clone();
                    let type_view = view.clone();
                    let mut value = ScrubValue::new(
                        SharedString::from(format!("element-value-{}", key_id(key))),
                        input.clone(),
                    )
                    .text(number_text(&text, field))
                    .unit(field_unit(field))
                    .placeholder(placeholder)
                    .typing(typing)
                    .locked(locked)
                    .on_press(move |x, window, cx| {
                        view.update(cx, |this, cx| {
                            this.begin_row_scrub(&press_key, x, ScrubOrigin::Value, window, cx);
                        })
                        .ok();
                    })
                    .on_type(move |_, cx| {
                        type_view
                            .update(cx, |this, cx| this.type_element_row(&type_key, cx))
                            .ok();
                    });
                    if typing {
                        value = value.field(|input| {
                            text_input(
                                input,
                                locked,
                                hint,
                                cx,
                                element_control_handler(key.clone()),
                            )
                        });
                    }
                    value.into_any_element()
                }
                _ => render_control(
                    row_id("control", key),
                    &row.kind,
                    &row.control,
                    locked,
                    row.shown_bool(),
                    hint,
                    cx,
                    element_control_handler(key.clone()),
                ),
            }
        });
        // One line: a long expression is cut by `reason_text`, and anything
        // still wider than the panel ends in an ellipsis.
        let reason_line = |text: String| {
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_sm()
                .text_color(muted)
                .child(text)
                .into_any_element()
        };
        let detail = match shown {
            _ if scope_note.is_some() => scope_note.map(|lock| reason_line(lock.text())),
            _ if default => None,
            EditTarget::Locked(reason @ LockReason::BetweenKeyframes { .. }) => {
                keyframe_jump_frames(shown, resolved.timeline_frame)
                    .map(|frames| self.keyframe_jumps(key, reason, frames, cx))
            }
            EditTarget::Locked(reason) => Some(reason_line(reason_text(reason))),
            _ => None,
        };
        let set_at = call_site.map(|call_site| set_at_line(call_site, entry_dir, cx));
        // Why typed text was not written, else a commit waiting its turn
        // (an own write's reload, say), so a value the source does not have
        // yet does not pass for written.
        let saving = self
            .element_rows
            .saving(self.selected_layer.as_deref(), key);
        let message = match row.and_then(|row| row.invalid.clone()) {
            Some(message) => Some(div().text_sm().text_color(cx.theme().danger).child(message)),
            None => saving.then(|| {
                // Waiting for the scenes to be scanned says so.
                let note = row
                    .and_then(|row| row.held_note.clone())
                    .unwrap_or_else(|| "Saving…".to_owned());
                div().text_sm().text_color(muted).child(note)
            }),
        };
        let remove = can_remove.then(|| {
            let key = key.clone();
            div()
                .invisible()
                .group_hover(ROW_GROUP, |remove| remove.visible())
                .child(
                    Button::new(row_id("remove", &key))
                        .ghost()
                        .xsmall()
                        .icon(IconName::Close)
                        .accessibility_label(remove_tooltip)
                        .tooltip(remove_tooltip)
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.remove_element_row(&key, cx)),
                        ),
                )
        });
        let multi_line = row.is_some_and(|row| row.kind == ControlKind::Text);
        let label_press = scrubbable.then(|| {
            let key = key.clone();
            let view = cx.entity().downgrade();
            move |event: &MouseDownEvent, window: &mut Window, cx: &mut App| {
                // The scrub decides where the focus goes: the root's
                // `track_focus` would take it and blur (and submit) a row's
                // field holding typed text.
                window.prevent_default();
                let x = f32::from(event.position.x);
                view.update(cx, |this, cx| {
                    this.begin_row_scrub(&key, x, ScrubOrigin::Label, window, cx);
                })
                .ok();
            }
        });
        // The state icon, or the remove button of a written value (a row
        // with one has no icon).
        let trailing = div()
            .flex()
            .items_center()
            .justify_center()
            .children(icon.map(|icon| icon.xsmall()))
            .children(remove);
        let row = InspectorRow::new(format!("element-{}", key_id(key)), label)
            .indented(indented)
            .top_aligned(multi_line)
            .trailing(trailing);
        let row = match control {
            Some(control) => row.value(control),
            None => row,
        };
        let row = match label_press {
            Some(press) => row.on_label_press(press),
            None => row,
        };
        let row = [
            set_at.map(IntoElement::into_any_element),
            detail,
            message.map(IntoElement::into_any_element),
        ]
        .into_iter()
        .flatten()
        .fold(row, |row, line| row.sub_line(line));
        div().group(ROW_GROUP).child(row).into_any_element()
    }

    /// "Between keyframes at {previous} and {next}" with buttons moving the
    /// playhead to each, at the timeline frames `keyframe_jump_frames`
    /// converted them to.
    fn keyframe_jumps(
        &self,
        key: &PropKey,
        reason: &LockReason,
        (previous_frame, next_frame): (i64, i64),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let jump = |control: &str, icon: IconName, frame: i64, cx: &mut Context<Self>| {
            let name = format!("Go to frame {frame}");
            Button::new(row_id(control, key))
                .ghost()
                .xsmall()
                .icon(icon)
                .accessibility_label(name.clone())
                .tooltip(name)
                .on_click(cx.listener(move |this, _, _, cx| this.go_to_frame(frame, cx)))
        };
        div()
            .flex()
            .items_center()
            .gap_1()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(reason_text(reason)),
            )
            .child(jump(
                "previous-keyframe",
                IconName::ChevronLeft,
                previous_frame,
                cx,
            ))
            .child(jump(
                "next-keyframe",
                IconName::ChevronRight,
                next_frame,
                cx,
            ))
            .into_any_element()
    }
}
