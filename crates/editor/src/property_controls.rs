//! The controls a property row edits its value with, shared by the panels
//! that show property rows. Each panel keeps its own row state and maps a
//! control's `ControlEvent`s to it.

use crate::view::EditorView;
use crate::widgets::format_component_number;
use celesta_composition::Color;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState};
use gpui_kit::component::input::{Escape, Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Context, ElementId, Entity, EventEmitter, Focusable as _, Hsla, KeyDownEvent,
    Keystroke, Rgba, SharedString, Subscription, Window, div,
};
use serde_json::Value;
use std::sync::Arc;

/// A typed number: surrounding spaces are ignored, and only finite values
/// count (`1e3` is fine, `NaN`, `inf`, and `1,5` are not).
pub(crate) fn parse_number_input(text: &str) -> Result<f64, String> {
    text.trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .ok_or_else(|| "Enter a number".to_owned())
}

/// A typed color: `#RRGGBB` or `#RRGGBBAA` in either case, returned as typed
/// (without surrounding spaces).
pub(crate) fn parse_color_input(text: &str) -> Result<String, String> {
    let text = text.trim();
    if Color::from_hex(text).is_some() {
        Ok(text.to_owned())
    } else {
        Err("Enter a color as #RRGGBB or #RRGGBBAA".to_owned())
    }
}

/// `#RRGGBB`, or `#RRGGBBAA` when the color is not opaque.
pub(crate) fn color_hex(color: Hsla) -> String {
    let rgba = Rgba::from(color);
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    let rgb = format!(
        "#{:02X}{:02X}{:02X}",
        channel(rgba.r),
        channel(rgba.g),
        channel(rgba.b)
    );
    if channel(rgba.a) == u8::MAX {
        rgb
    } else {
        format!("{rgb}{:02X}", channel(rgba.a))
    }
}

/// A `#RRGGBB`/`#RRGGBBAA` string as a color, for the swatch.
pub(crate) fn parse_color(value: &Value) -> Option<Hsla> {
    let color = Color::from_hex(value.as_str()?.trim())?;
    let [r, g, b, a] = color.channels().map(|channel| f32::from(channel) / 255.0);
    Some(Hsla::from(Rgba { r, g, b, a }))
}

/// The range hint a number field shows: `0–1`, `≥ 0`, or `≤ 10`.
pub(crate) fn number_hint(min: Option<f64>, max: Option<f64>) -> Option<String> {
    let number = format_component_number;
    match (min, max) {
        (Some(min), Some(max)) => Some(format!("{}–{}", number(min), number(max))),
        (Some(min), None) => Some(format!("≥ {}", number(min))),
        (None, Some(max)) => Some(format!("≤ {}", number(max))),
        (None, None) => None,
    }
}

/// When a color control's picker closes: the color picked since it last
/// closed, as [`color_hex`] writes it, and `picked` cleared; `None` when
/// nothing was picked (or the control is not a color's).
pub(crate) fn take_picked_color(control: &Control, picked: &mut bool, cx: &App) -> Option<String> {
    let Control::Color { picker, .. } = control else {
        return None;
    };
    let color = picker.read(cx).value();
    if !std::mem::take(picked) {
        return None;
    }
    color.map(color_hex)
}

/// What a control is for.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ControlKind {
    Number,
    String,
    /// Multi-line text.
    Text,
    Boolean,
    Color,
    Select(Vec<String>),
    Path,
}

/// What the person did in a control; each panel maps it to its own rows.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ControlEvent {
    /// The person typed in the text.
    Edited,
    /// Enter (`blur` false) or blur in the text.
    Submit {
        blur: bool,
    },
    /// Escape in the text.
    Escape,
    Toggled(bool),
    Chosen(String),
    /// A color was picked; the pick is committed when the picker closes.
    ColorPicked,
    /// The picker went from open to closed; sent only on that change.
    ColorPickerClosed,
    /// `Choose…` on a path control.
    ChoosePath,
    /// `None` on a path control.
    ClearPath,
}

/// The control a row edits its value with.
pub(crate) enum Control {
    /// `Number`, `String`, and `Path`.
    Text(Entity<InputState>),
    /// `Text`: gpui-kit keeps multi-line text a separate state type.
    Textarea(Entity<TextareaState>),
    /// `Boolean`; the panel passes the shown state when rendering.
    Switch,
    Select(Entity<SelectState<Vec<String>>>),
    Color {
        text: Entity<InputState>,
        picker: Entity<ColorPickerState>,
    },
}

impl Control {
    /// The typed text, for controls with a text input.
    pub(crate) fn text(&self, cx: &App) -> Option<SharedString> {
        match self {
            Control::Text(input) | Control::Color { text: input, .. } => {
                Some(input.read(cx).value())
            }
            Control::Textarea(input) => Some(input.read(cx).value()),
            Control::Switch | Control::Select(_) => None,
        }
    }

    /// The control has a text input and it has focus.
    pub(crate) fn text_focused(&self, window: &Window, cx: &App) -> bool {
        match self {
            Control::Text(input) | Control::Color { text: input, .. } => {
                input.focus_handle(cx).is_focused(window)
            }
            Control::Textarea(input) => input.focus_handle(cx).is_focused(window),
            Control::Switch | Control::Select(_) => false,
        }
    }

    /// Replaces the text of a control with a text input; false for one
    /// without.
    pub(crate) fn set_text(&self, text: String, window: &mut Window, cx: &mut App) -> bool {
        match self {
            Control::Text(input) | Control::Color { text: input, .. } => {
                input.update(cx, |input, cx| input.set_value(text, window, cx));
            }
            Control::Textarea(input) => {
                input.update(cx, |input, cx| input.set_value(text, window, cx));
            }
            Control::Switch | Control::Select(_) => return false,
        }
        true
    }

    /// Replaces the dimmed placeholder of the control's text input; does
    /// nothing for a control without one.
    pub(crate) fn set_placeholder(&self, placeholder: String, window: &mut Window, cx: &mut App) {
        match self {
            Control::Text(input) | Control::Color { text: input, .. } => {
                input.update(cx, |input, cx| {
                    input.set_placeholder(placeholder, window, cx)
                });
            }
            Control::Textarea(input) => {
                input.update(cx, |input, cx| {
                    input.set_placeholder(placeholder, window, cx)
                });
            }
            Control::Switch | Control::Select(_) => {}
        }
    }
}

/// A built control and the subscriptions that send its events.
pub(crate) struct BuiltControl {
    pub(crate) control: Control,
    pub(crate) subscriptions: Vec<Subscription>,
}

/// The text input a kind uses, as `(multi_line, submit_on_enter)`; `None`
/// for kinds without one. Multi-line text submits on Enter and takes a
/// newline on Shift+Enter.
pub(crate) fn input_options(kind: &ControlKind) -> Option<(bool, bool)> {
    match kind {
        ControlKind::Text => Some((true, true)),
        ControlKind::Number | ControlKind::String | ControlKind::Path | ControlKind::Color => {
            Some((false, false))
        }
        ControlKind::Boolean | ControlKind::Select(_) => None,
    }
}

/// Builds the control for `kind`; `on` receives what the person does in it.
pub(crate) fn build_control(
    kind: &ControlKind,
    window: &mut Window,
    cx: &mut Context<EditorView>,
    on: impl Fn(&mut EditorView, ControlEvent, &mut Window, &mut Context<EditorView>) + Clone + 'static,
) -> BuiltControl {
    let (control, subscriptions) = match kind {
        ControlKind::Boolean => (Control::Switch, Vec::new()),
        ControlKind::Select(options) => {
            let select = cx.new(|cx| SelectState::new(options.clone(), None, window, cx));
            let subscription = cx.subscribe_in(
                &select,
                window,
                move |this, _, event: &SelectEvent<Vec<String>>, window, cx| {
                    if let SelectEvent::Confirm(Some(choice)) = event {
                        on(this, ControlEvent::Chosen(choice.clone()), window, cx);
                    }
                },
            );
            (Control::Select(select), vec![subscription])
        }
        ControlKind::Color => {
            let text = cx.new(|cx| InputState::new(window, cx));
            let typed = subscribe_text(&text, false, window, cx, on.clone());
            let picker = cx.new(|cx| ColorPickerState::new(window, cx));
            let picked_on = on.clone();
            let picked = cx.subscribe_in(
                &picker,
                window,
                move |this, _, _: &ColorPickerEvent, window, cx| {
                    picked_on(this, ControlEvent::ColorPicked, window, cx);
                },
            );
            let mut was_open = false;
            let closed = cx.observe_in(&picker, window, move |this, picker, window, cx| {
                let open = picker.read(cx).is_open();
                if std::mem::replace(&mut was_open, open) && !open {
                    on(this, ControlEvent::ColorPickerClosed, window, cx);
                }
            });
            (Control::Color { text, picker }, vec![typed, picked, closed])
        }
        ControlKind::Number | ControlKind::String | ControlKind::Text | ControlKind::Path => {
            match input_options(kind) {
                Some((true, submit_on_enter)) => {
                    let input = cx.new(|cx| {
                        TextareaState::new(window, cx)
                            .submit_on_enter(submit_on_enter)
                            .auto_grow(1, 6)
                    });
                    let subscription = subscribe_text(&input, true, window, cx, on);
                    (Control::Textarea(input), vec![subscription])
                }
                _ => {
                    let input = cx.new(|cx| InputState::new(window, cx));
                    let subscription = subscribe_text(&input, false, window, cx, on);
                    (Control::Text(input), vec![subscription])
                }
            }
        }
    };
    BuiltControl {
        control,
        subscriptions,
    }
}

/// Sends a text input's typing, Enter, and blur to `on`.
fn subscribe_text<S: EventEmitter<InputEvent>>(
    input: &Entity<S>,
    multi_line: bool,
    window: &mut Window,
    cx: &mut Context<EditorView>,
    on: impl Fn(&mut EditorView, ControlEvent, &mut Window, &mut Context<EditorView>) + 'static,
) -> Subscription {
    cx.subscribe_in(
        input,
        window,
        move |this, _, event: &InputEvent, window, cx| {
            let event = match event {
                // `set_value` emits no change, so this is the person typing.
                InputEvent::Change => ControlEvent::Edited,
                InputEvent::PressEnter { shift, .. } => enter_event(multi_line, *shift),
                InputEvent::Blur => ControlEvent::Submit { blur: true },
                InputEvent::Focus => return,
            };
            on(this, event, window, cx);
        },
    )
}

/// What Enter in a text input is: a submit, except Shift+Enter in
/// multi-line text, which gpui-base reports after inserting the newline it
/// typed (an edit, not a submit).
pub(crate) fn enter_event(multi_line: bool, shift: bool) -> ControlEvent {
    if multi_line && shift {
        ControlEvent::Edited
    } else {
        ControlEvent::Submit { blur: false }
    }
}

/// Renders `control`, disabled when `locked`. `shown_bool` is the switch
/// state; `hint` is the dimmed suffix of a single-line text (a number's
/// range) and the placeholder of a select (the value it shows when none of
/// the options is chosen).
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_control(
    id: impl Into<ElementId>,
    kind: &ControlKind,
    control: &Control,
    locked: bool,
    shown_bool: bool,
    hint: Option<String>,
    cx: &mut Context<EditorView>,
    on: impl Fn(&mut EditorView, ControlEvent, &mut Window, &mut Context<EditorView>) + Clone + 'static,
) -> AnyElement {
    let id = Arc::new(id.into());
    let child_id = |name: &'static str| ElementId::NamedChild(id.clone(), name.into());
    match control {
        Control::Text(input) if *kind == ControlKind::Path => {
            let choose = on.clone();
            let clear = on.clone();
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(div().flex_1().min_w_0().child(text_input(
                    Input::new(input),
                    locked,
                    hint,
                    cx,
                    on,
                )))
                .child(
                    Button::new(child_id("choose"))
                        .outline()
                        .small()
                        .label("Choose…")
                        .disabled(locked)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            choose(this, ControlEvent::ChoosePath, window, cx);
                        })),
                )
                .child(
                    Button::new(child_id("none"))
                        .ghost()
                        .small()
                        .label("None")
                        .disabled(locked)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            clear(this, ControlEvent::ClearPath, window, cx);
                        })),
                )
                .into_any_element()
        }
        Control::Text(input) => text_input(Input::new(input), locked, hint, cx, on),
        Control::Textarea(input) => escapable(Textarea::new(input).disabled(locked), cx, on),
        Control::Switch => Switch::new(child_id("switch"))
            .small()
            .checked(shown_bool)
            .disabled(locked)
            .on_click(cx.listener(move |this, checked: &bool, window, cx| {
                on(this, ControlEvent::Toggled(*checked), window, cx);
            }))
            .into_any_element(),
        Control::Select(select) => Select::new(select)
            .small()
            .disabled(locked)
            .when_some(hint, |select, hint| select.placeholder(hint))
            .into_any_element(),
        Control::Color { text, picker } => div()
            .flex()
            .items_center()
            .gap_2()
            .child(if locked {
                color_swatch(picker.read(cx).value(), cx).into_any_element()
            } else {
                ColorPicker::new(picker).small().into_any_element()
            })
            .child(div().flex_1().min_w_0().child(text_input(
                Input::new(text),
                locked,
                None,
                cx,
                on,
            )))
            .into_any_element(),
    }
}

/// A single-line text input with `hint` as its dimmed suffix.
fn text_input(
    input: Input,
    locked: bool,
    hint: Option<String>,
    cx: &mut Context<EditorView>,
    on: impl Fn(&mut EditorView, ControlEvent, &mut Window, &mut Context<EditorView>) + 'static,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let input = input
        .small()
        .disabled(locked)
        .when_some(hint, |input, hint| {
            input.suffix(div().text_xs().text_color(muted).child(hint))
        });
    escapable(input, cx, on)
}

/// Whether a text field keeps `keystroke` though it does nothing with it.
/// The input passes on Left at the start of its text and Right at the end,
/// and macOS then gives an unhandled key to the menu, whose Previous Frame
/// and Next Frame items take those keys: the playhead moved while typing.
pub(crate) fn field_keeps_key(keystroke: &Keystroke) -> bool {
    matches!(keystroke.key.as_str(), "left" | "right") && !keystroke.modifiers.modified()
}

/// Wraps a text input so Escape in it reaches `on`, and the arrow keys it
/// passes on stay with it ([`field_keeps_key`]).
fn escapable(
    input: impl IntoElement,
    cx: &mut Context<EditorView>,
    on: impl Fn(&mut EditorView, ControlEvent, &mut Window, &mut Context<EditorView>) + 'static,
) -> AnyElement {
    div()
        .w_full()
        .on_action(cx.listener(move |this, _: &Escape, window, cx| {
            on(this, ControlEvent::Escape, window, cx);
        }))
        .on_key_down(|event: &KeyDownEvent, _, cx| {
            if field_keeps_key(&event.keystroke) {
                cx.stop_propagation();
            }
        })
        .child(input)
        .into_any_element()
}

/// The color a disabled color control shows in place of the picker.
fn color_swatch(color: Option<Hsla>, cx: &mut Context<EditorView>) -> impl IntoElement {
    div()
        .flex_none()
        .size_5()
        .rounded(cx.theme().radius)
        .border_1()
        .border_color(cx.theme().input)
        .when_some(color, |swatch, color| swatch.bg(color))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_field_keeps_the_plain_arrows_it_passes_on() {
        let keeps = |key: &'static str| field_keeps_key(&Keystroke::parse(key).unwrap());
        assert!(keeps("left"));
        assert!(keeps("right"));
        // The input handles these itself, or they are the editor's own.
        assert!(!keeps("shift-left"));
        assert!(!keeps("cmd-right"));
        assert!(!keeps("up"));
        assert!(!keeps("space"));
    }

    #[test]
    fn shift_enter_breaks_the_line_in_multi_line_text() {
        // gpui-base inserts the newline and still reports the Enter.
        assert_eq!(enter_event(true, true), ControlEvent::Edited);
        assert_eq!(
            enter_event(true, false),
            ControlEvent::Submit { blur: false }
        );
        // A single-line input submits on any Enter.
        assert_eq!(
            enter_event(false, true),
            ControlEvent::Submit { blur: false }
        );
        assert_eq!(
            enter_event(false, false),
            ControlEvent::Submit { blur: false }
        );
    }

    #[test]
    fn number_input_accepts_finite_numbers_only() {
        assert_eq!(parse_number_input("1.5"), Ok(1.5));
        assert_eq!(parse_number_input(" 2 "), Ok(2.0));
        assert_eq!(parse_number_input("1e3"), Ok(1000.0));
        for text in ["1,5", "", "NaN", "inf"] {
            assert!(parse_number_input(text).is_err(), "{text:?} was accepted");
        }
    }

    #[test]
    fn color_input_accepts_six_or_eight_hex_digits() {
        assert_eq!(parse_color_input("#AABBCC"), Ok("#AABBCC".to_owned()));
        assert_eq!(parse_color_input("#aabbccdd"), Ok("#aabbccdd".to_owned()));
        for text in ["#fff", "red", "AABBCC", "#aéaaa"] {
            assert!(parse_color_input(text).is_err(), "{text:?} was accepted");
        }
    }

    #[test]
    fn picked_colors_are_written_as_hex() {
        let opaque = Hsla::from(Rgba {
            r: 1.0,
            g: 0.0,
            b: 0x80 as f32 / 255.0,
            a: 1.0,
        });
        assert_eq!(color_hex(opaque), "#FF0080");
        let translucent = Hsla::from(Rgba {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0x40 as f32 / 255.0,
        });
        assert_eq!(color_hex(translucent), "#00000040");
    }

    #[test]
    fn number_hints_show_the_declared_range() {
        assert_eq!(number_hint(Some(0.0), Some(1.0)), Some("0–1".to_owned()));
        assert_eq!(number_hint(Some(0.5), None), Some("≥ 0.5".to_owned()));
        assert_eq!(number_hint(None, Some(10.0)), Some("≤ 10".to_owned()));
        assert_eq!(number_hint(None, None), None);
    }

    #[test]
    fn input_options_per_kind() {
        assert_eq!(input_options(&ControlKind::Text), Some((true, true)));
        for kind in [
            ControlKind::Number,
            ControlKind::String,
            ControlKind::Path,
            ControlKind::Color,
        ] {
            assert_eq!(input_options(&kind), Some((false, false)), "{kind:?}");
        }
        assert_eq!(input_options(&ControlKind::Boolean), None);
        assert_eq!(
            input_options(&ControlKind::Select(vec!["a".to_owned()])),
            None
        );
    }
}
