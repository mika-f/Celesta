//! Renders the selected layer's property sections; the row state and the
//! commit flow live in `element_rows.rs`.

use crate::element_edit::{
    ElementTargets, PropKey, Resolved, Target, editable, inner_target, keyframe_jump_frames,
    reason_text, table_key,
};
use crate::element_rows::{
    component_row_label, element_control_handler, row_field, row_label, scrubs, set_at_text,
};
use crate::icons::CelestaIcon;
use crate::layer_panel::LABEL_WIDTH;
use crate::property_controls::{ControlKind, number_hint, render_control};
use crate::view::EditorView;
use crate::widgets::inspector_section;
use celesta_react_bridge::{PrimitivePropertyField, SourcePos};
use celesta_source_edit::{EditTarget, LockReason};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Context, CursorStyle, Div, ElementId, MouseButton, MouseDownEvent, SharedString,
    div, px,
};
use std::path::Path;

/// How far an input sub-row is indented under its row.
const INDENT: f32 = 12.0;

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

/// "Set at `<file>`:`<line>`" after a `corner-down-right` icon.
fn set_at_line(call_site: &SourcePos, entry_dir: &Path, cx: &Context<EditorView>) -> Div {
    div()
        .flex()
        .items_center()
        .gap_1()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(Icon::new(CelestaIcon::CornerDownRight).xsmall())
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(set_at_text(call_site, entry_dir)),
        )
}

impl EditorView {
    /// The selected layer's property sections, or why there are none yet.
    pub(crate) fn element_sections(&self, cx: &mut Context<Self>) -> AnyElement {
        let note =
            |text: SharedString, color| div().px_3().py_2().text_xs().text_color(color).child(text);
        let muted = cx.theme().muted_foreground;
        let resolved = self.element_edit.resolved(self.selected_layer.as_deref());
        let Some((resolved, current)) = resolved else {
            return note("Reading source…".into(), muted).into_any_element();
        };
        let targets = match &resolved.outcome {
            Ok(targets) => targets,
            Err(error) => {
                return note(error.clone().into(), cx.theme().danger).into_any_element();
            }
        };
        let entry_dir = self.entry_dir();
        let mut column = div().flex().flex_col();
        for (name, rows) in sections(targets) {
            column = column.child(inspector_section(name, cx));
            for (key, target) in rows {
                let PropKey::Prop(prop) = key else {
                    continue;
                };
                let field = row_field(targets, key);
                column = column.child(self.element_row(
                    key,
                    row_label(prop),
                    target,
                    field,
                    false,
                    resolved,
                    current,
                    &entry_dir,
                    cx,
                ));
                for (input, input_target) in &target.inputs {
                    let input_key = PropKey::Input {
                        row: prop.clone(),
                        name: input.clone(),
                    };
                    column = column.child(self.element_row(
                        &input_key,
                        input.clone(),
                        input_target,
                        None,
                        true,
                        resolved,
                        current,
                        &entry_dir,
                        cx,
                    ));
                }
            }
        }
        // The props written at the component's call, under its name; none
        // when every prop written there already shows "Set at …" above.
        if let Some(component) = targets
            .component
            .as_ref()
            .filter(|component| !component.props.is_empty())
        {
            column = column
                .child(inspector_section(
                    format!("Component · {}", component.name),
                    cx,
                ))
                .child(
                    div()
                        .px_3()
                        .py_2()
                        .border_b_1()
                        .border_color(cx.theme().border)
                        .child(set_at_line(&component.call_site, &entry_dir, cx)),
                );
            for prop in &component.props {
                let key = PropKey::CallSite(prop.name.clone());
                column = column.child(self.element_row(
                    &key,
                    component_row_label(component, &prop.name),
                    &prop.target,
                    row_field(targets, &key),
                    false,
                    resolved,
                    current,
                    &entry_dir,
                    cx,
                ));
            }
        }
        column.into_any_element()
    }

    /// `label  [control]  icon`, plus, below, where a pass-through value is
    /// set, the reason or the keyframe jumps when the target cannot be
    /// edited, and why typed text was not written. An input sub-row
    /// (`indented`) keeps its control on its row's spine.
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
        entry_dir: &Path,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let row = self.element_rows.get(key);
        // Dragging the label scrubs an editable number (not while playing).
        let scrubbable = current
            && !self.playing
            && resolved
                .outcome
                .as_ref()
                .is_ok_and(|targets| scrubs(targets, key, target));
        // A written optional value offers its removal on hover, except while
        // a scrub runs or the removal is already being written.
        let removing = row.is_some_and(|row| row.removing())
            || self
                .element_edit
                .written_value(self.selected_layer.as_deref(), key)
                .is_some_and(Option::is_none);
        let can_remove = !removing
            && self.element_rows.scrub.is_none()
            && resolved
                .outcome
                .as_ref()
                .is_ok_and(|targets| self.row_removable(targets, key, target));
        let (call_site, target) = (target.call_site.as_ref(), &target.target);
        // A pass-through row shows the state of the value at the call site.
        let shown = inner_target(target);
        let icon = match shown {
            EditTarget::Keyframe { .. } => Some(CelestaIcon::DiamondFilled),
            EditTarget::Locked(LockReason::BetweenKeyframes { .. }) => Some(CelestaIcon::Diamond),
            EditTarget::Locked(_) => Some(CelestaIcon::Lock),
            EditTarget::Literal { .. }
            | EditTarget::Missing { .. }
            | EditTarget::PassThrough { .. } => None,
        };
        // Only a target that can't be written disables its control: while an
        // own write's reload runs, the field keeps its focus and caret, and
        // a commit made meanwhile waits for the reload.
        let locked = !editable(target);
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
            div().flex_1().min_w_0().child(render_control(
                row_id("control", key),
                &row.kind,
                &row.control,
                locked,
                row.shown_bool(),
                hint,
                cx,
                element_control_handler(key.clone()),
            ))
        });
        let detail = match shown {
            EditTarget::Locked(reason @ LockReason::BetweenKeyframes { .. }) => {
                keyframe_jump_frames(shown, resolved.timeline_frame)
                    .map(|frames| self.keyframe_jumps(key, reason, frames, cx))
            }
            // One line: a long expression is cut by `reason_text`, and
            // anything still wider than the panel ends in an ellipsis.
            EditTarget::Locked(reason) => Some(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_xs()
                    .text_color(muted)
                    .child(reason_text(reason))
                    .into_any_element(),
            ),
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
            Some(message) => Some(div().text_xs().text_color(cx.theme().danger).child(message)),
            None => saving.then(|| {
                // Waiting for the scenes to be scanned says so.
                let note = row
                    .and_then(|row| row.held_note.clone())
                    .unwrap_or_else(|| "Saving…".to_owned());
                div().text_xs().text_color(muted).child(note)
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
                        .tooltip("Remove")
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.remove_element_row(&key, cx)),
                        ),
                )
        });
        let multi_line = row.is_some_and(|row| row.kind == ControlKind::Text);
        let label_width = if indented {
            LABEL_WIDTH - INDENT
        } else {
            LABEL_WIDTH
        };
        div()
            .group(ROW_GROUP)
            .flex()
            .flex_col()
            .gap_1()
            .pr_3()
            // `pl_6` is `pl_3` plus `INDENT`.
            .when(indented, |row| row.pl_6())
            .when(!indented, |row| row.pl_3())
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex()
                    .when(multi_line, |line| line.items_start())
                    .when(!multi_line, |line| line.items_center())
                    .gap_2()
                    .child(
                        div()
                            .flex_none()
                            .w(px(label_width))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_xs()
                            .text_color(muted)
                            .when(scrubbable, |label| {
                                let key = key.clone();
                                label.cursor(CursorStyle::ResizeLeftRight).on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                        // The scrub decides where the focus
                                        // goes: the root's `track_focus` would
                                        // take it and blur (and submit) a
                                        // row's field holding typed text.
                                        window.prevent_default();
                                        let x = f32::from(event.position.x);
                                        this.begin_row_scrub(&key, x, window, cx);
                                    }),
                                )
                            })
                            .child(label),
                    )
                    .children(control)
                    // The state icon, or the remove button of a written
                    // value (a row with one has no icon).
                    .child(
                        div()
                            .flex_none()
                            .size_5()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(muted)
                            .children(icon.map(|icon| Icon::new(icon).xsmall()))
                            .children(remove),
                    ),
            )
            .children(set_at)
            .children(detail)
            .children(message)
            .into_any_element()
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
            Button::new(row_id(control, key))
                .ghost()
                .xsmall()
                .icon(icon)
                .tooltip(format!("Go to frame {frame}"))
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
                    .text_xs()
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
