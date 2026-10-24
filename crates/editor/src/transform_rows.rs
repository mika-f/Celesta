//! The Layer section's Transform group: one row per transform prop of the
//! selected layer, a field that writes the prop into the JSX source on Enter
//! or blur, and the target's state (keyframe, between keyframes, locked).

use crate::icons::CelestaIcon;
use crate::layer_select::display_path;
use crate::transform_edit::{
    Resolved, Target, TransformProp, editable, inner_target, keyframe_jump_frames, reason_text,
};
use crate::view::EditorView;
use crate::widgets::inspector_section;
use celesta_react_bridge::{LayerSourceReport, SourcePos};
use celesta_source_edit::{EditTarget, EditValue, LockReason, format_number};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Escape, Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Context, Entity, SharedString, Subscription, Window, div, px};
use std::collections::HashMap;
use std::path::Path;

const PROPS: [TransformProp; 6] = [
    TransformProp::X,
    TransformProp::Y,
    TransformProp::Rotation,
    TransformProp::Scale,
    TransformProp::ScaleX,
    TransformProp::ScaleY,
];

/// One prop's text field.
struct Field {
    input: Entity<InputState>,
    /// Typed since the field last showed the source's value.
    edited: bool,
    /// The text and placeholder last put into the field, and the layer they
    /// belong to.
    shown: Option<(String, String)>,
    layer: Option<String>,
    _subscription: Subscription,
}

/// The Transform fields, built once (inputs need a window) and kept.
#[derive(Default)]
pub(crate) struct TransformInputs {
    fields: HashMap<TransformProp, Field>,
}

/// What a field shows for `target`: its text, and the dimmed placeholder
/// shown when the prop is absent (the value it is drawn with).
pub(crate) fn field_text(
    prop: TransformProp,
    target: &EditTarget,
    report: &LayerSourceReport,
) -> (String, String) {
    match target {
        EditTarget::Literal { value, .. } => (
            value.as_number().map(format_number).unwrap_or_default(),
            String::new(),
        ),
        EditTarget::Keyframe { value, .. } => (format_number(*value), String::new()),
        EditTarget::Missing {
            default: Some(default),
            ..
        } => (
            String::new(),
            format_number(absent_value(prop, Some(default), report)),
        ),
        // A parameter without a literal default: the element draws what it
        // received (`{ scale = base * 2 }`), or its own default when it
        // received nothing (`{ scale }`).
        EditTarget::Missing { default: None, .. } => (
            String::new(),
            format_number(
                prop.received(&report.props)
                    .unwrap_or_else(|| prop_default(prop, report)),
            ),
        ),
        EditTarget::PassThrough { target, .. } => field_text(prop, target, report),
        EditTarget::Locked(_) => match prop.received(&report.props) {
            Some(value) => (format_number(value), String::new()),
            None => (String::new(), format_number(prop_default(prop, report))),
        },
    }
}

/// [`absent_value`] with the prop's own default.
fn prop_default(prop: TransformProp, report: &LayerSourceReport) -> f64 {
    absent_value(
        prop,
        celesta_source_edit::default_for(prop.name()).as_ref(),
        report,
    )
}

/// A pass-through row's line naming the call site its value is written at,
/// the file relative to the entry's directory as [`display_path`] shows it.
pub(crate) fn set_at_text(call_site: &SourcePos, entry_dir: &Path) -> String {
    format!(
        "Set at {}:{}",
        display_path(&call_site.file, entry_dir),
        call_site.line
    )
}

/// The value an absent prop is drawn with: a missing scale axis takes the
/// `scale` the element received (`<Rect scale={2} scaleX={3} />` draws Y at
/// 2), anything else its default (every transform prop has a numeric one).
fn absent_value(
    prop: TransformProp,
    default: Option<&EditValue>,
    report: &LayerSourceReport,
) -> f64 {
    let default = default.and_then(EditValue::as_number).unwrap_or(0.0);
    match prop {
        TransformProp::ScaleX | TransformProp::ScaleY => report.props.scale.unwrap_or(default),
        _ => default,
    }
}

impl EditorView {
    /// Builds the fields once and puts newly resolved values (or, until the
    /// reload after an own write resolves, the written ones) into those not
    /// being typed in. Runs from `render`, which has the window inputs need.
    pub(crate) fn sync_transform_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_react_preview() {
            return;
        }
        if self.transform_inputs.fields.is_empty() {
            for prop in PROPS {
                let field = Self::transform_field(prop, window, cx);
                self.transform_inputs.fields.insert(prop, field);
            }
        }
        let Some((resolved, _)) = self.transform.resolved(self.selected_layer.as_deref()) else {
            return;
        };
        let Ok(targets) = &resolved.outcome else {
            return;
        };
        for (prop, target) in &targets.targets {
            let Some(field) = self.transform_inputs.fields.get_mut(prop) else {
                continue;
            };
            let shown = match self.transform.written_value(Some(&resolved.layer), *prop) {
                Some(value) => (format_number(value), String::new()),
                None => field_text(*prop, &target.target, &resolved.report),
            };
            // Typed text stays until committed, but never under another layer.
            let same_layer = field.layer.as_deref() == Some(resolved.layer.as_str());
            if same_layer && (field.edited || field.shown.as_ref() == Some(&shown)) {
                continue;
            }
            field.input.update(cx, |input, cx| {
                input.set_value(shown.0.clone(), window, cx);
                input.set_placeholder(shown.1.clone(), window, cx);
            });
            field.shown = Some(shown);
            field.layer = Some(resolved.layer.clone());
            field.edited = false;
        }
    }

    fn transform_field(prop: TransformProp, window: &mut Window, cx: &mut Context<Self>) -> Field {
        let input = cx.new(|cx| InputState::new(window, cx));
        let subscription =
            cx.subscribe_in(&input, window, move |this, _, event: &InputEvent, _, cx| {
                match event {
                    // `set_value` emits no change, so this is the person typing.
                    InputEvent::Change => {
                        if let Some(field) = this.transform_inputs.fields.get_mut(&prop) {
                            field.edited = true;
                        }
                    }
                    InputEvent::PressEnter { .. } => this.submit_transform_field(prop, false, cx),
                    InputEvent::Blur => this.submit_transform_field(prop, true, cx),
                    InputEvent::Focus => {}
                }
            });
        Field {
            input,
            edited: false,
            shown: None,
            layer: None,
            _subscription: subscription,
        }
    }

    /// Shows the source's value in `prop`'s field again on the next render.
    fn restore_transform_field(&mut self, prop: TransformProp, cx: &mut Context<Self>) {
        if let Some(field) = self.transform_inputs.fields.get_mut(&prop) {
            field.edited = false;
            field.shown = None;
        }
        cx.notify();
    }

    /// Enter (`blur` false) or blur: writes typed text that parses as a
    /// number. Unedited text is never written; empty or invalid text on blur
    /// shows the source's value again.
    fn submit_transform_field(&mut self, prop: TransformProp, blur: bool, cx: &mut Context<Self>) {
        let Some(field) = self.transform_inputs.fields.get_mut(&prop) else {
            return;
        };
        if !field.edited {
            return;
        }
        // Clicking another layer blurs the field after the selection moved;
        // the text was typed for the old one.
        if field.layer != self.selected_layer {
            return self.restore_transform_field(prop, cx);
        }
        let text = field.input.read(cx).value().trim().to_owned();
        let layer = self.selected_layer.clone();
        let value = text.parse::<f64>().ok().filter(|value| value.is_finite());
        let Some(value) = value else {
            if blur {
                self.restore_transform_field(prop, cx);
            }
            if !text.is_empty() {
                self.transform
                    .set_error(layer.as_deref(), Some("Enter a number".to_owned()));
            }
            return cx.notify();
        };
        field.edited = false;
        self.commit_transform(
            vec![(prop, value)],
            move |this, result, cx| {
                // The field keeps the typed text until the reload shows the
                // written value; on failure it shows the source's value.
                if result.is_err() {
                    this.restore_transform_field(prop, cx);
                }
            },
            cx,
        );
        cx.notify();
    }

    /// The Transform group under the source rows: a row per target of the
    /// selected layer, or why there are none.
    pub(crate) fn transform_group(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let note = |text: SharedString| div().px_3().py_2().text_xs().text_color(muted).child(text);
        let group = div()
            .flex()
            .flex_col()
            .child(inspector_section("Transform", cx));
        let resolved = self.transform.resolved(self.selected_layer.as_deref());
        match resolved {
            Some((resolved, current)) => match &resolved.outcome {
                Ok(targets) => {
                    let entry_dir = self.entry_dir();
                    group
                        .children(targets.targets.iter().map(|(prop, target)| {
                            self.transform_row(*prop, target, resolved, current, &entry_dir, cx)
                        }))
                        .into_any_element()
                }
                Err(error) => group
                    .child(
                        div()
                            .px_3()
                            .py_2()
                            .text_xs()
                            .text_color(cx.theme().danger)
                            .child(error.clone()),
                    )
                    .into_any_element(),
            },
            None => group
                .child(note("Reading source…".into()))
                .into_any_element(),
        }
    }

    /// `label  [field]  icon`, plus, below, where a pass-through value is set
    /// and the reason or the keyframe jumps when the target cannot be
    /// edited.
    fn transform_row(
        &self,
        prop: TransformProp,
        target: &Target,
        resolved: &Resolved,
        current: bool,
        entry_dir: &Path,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
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
        let field = self.transform_inputs.fields.get(&prop).map(|field| {
            let escape = cx.listener(move |this, _: &Escape, _, cx| {
                this.restore_transform_field(prop, cx);
            });
            div().flex_1().min_w_0().on_action(escape).child(
                Input::new(&field.input)
                    .small()
                    .disabled(!(current && editable(target))),
            )
        });
        let detail = match shown {
            EditTarget::Locked(reason @ LockReason::BetweenKeyframes { .. }) => {
                keyframe_jump_frames(shown, resolved.timeline_frame)
                    .map(|frames| self.keyframe_jumps(prop, reason, frames, cx))
            }
            EditTarget::Locked(reason) => Some(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(reason_text(reason))
                    .into_any_element(),
            ),
            _ => None,
        };
        let set_at = call_site.map(|call_site| {
            div()
                .flex()
                .items_center()
                .gap_1()
                .text_xs()
                .text_color(muted)
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
        });
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
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_none()
                            .w(px(64.0))
                            .text_xs()
                            .text_color(muted)
                            .child(prop.label()),
                    )
                    .children(field)
                    .child(
                        div()
                            .flex_none()
                            .size_4()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(muted)
                            .children(icon.map(|icon| Icon::new(icon).xsmall())),
                    ),
            )
            .children(set_at)
            .children(detail)
            .into_any_element()
    }

    /// "Between keyframes at {previous} and {next}" with buttons moving the
    /// playhead to each, at the timeline frames `keyframe_jump_frames`
    /// converted them to.
    fn keyframe_jumps(
        &self,
        prop: TransformProp,
        reason: &LockReason,
        (previous_frame, next_frame): (i64, i64),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let jump = |id: &'static str, icon: IconName, frame: i64, cx: &mut Context<Self>| {
            Button::new((id, prop as usize))
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
                "transform-previous-keyframe",
                IconName::ChevronLeft,
                previous_frame,
                cx,
            ))
            .child(jump(
                "transform-next-keyframe",
                IconName::ChevronRight,
                next_frame,
                cx,
            ))
            .into_any_element()
    }
}
