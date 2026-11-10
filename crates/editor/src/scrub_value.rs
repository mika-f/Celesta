//! The number control of an Inspector row: at rest the value and its unit
//! as text, dragged left or right to change it; a click without a drag (or
//! Tab, or Enter on it) opens the row's text field to type.
//!
//! The control only reports: a press (where, so the panel can start its
//! scrub) and a request to type. The panel owns the scrub, decides between a
//! drag and a click on release, and says whether the row is typing.

// The rows move onto this over P7b's tasks 5 and 6; task 7 removes this.
#![allow(dead_code)]

use std::rc::Rc;

use crate::widgets::format_component_number;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, CursorStyle, ElementId, Entity, Focusable as _, KeyDownEvent, MouseButton,
    MouseDownEvent, SharedString, Window, div,
};

/// How far the pointer moves (window pixels) before a press is a drag.
pub(crate) const DRAG_THRESHOLD: f32 = 3.0;

/// The distance a scrub counts for a pointer `dx` pixels from the press:
/// none inside the threshold, then from the threshold on, so the value does
/// not jump when the drag starts.
pub(crate) fn drag_distance(dx: f32) -> Option<f32> {
    (dx.abs() >= DRAG_THRESHOLD).then(|| dx - DRAG_THRESHOLD * dx.signum())
}

/// The digits after the point of `step` written plainly (0.01 → 2).
fn step_places(step: f64) -> usize {
    let text = format!("{step}");
    text.split_once('.')
        .map_or(0, |(_, fraction)| fraction.len())
}

/// A number as the resting control shows it: whole for an `integer` field,
/// with as many places as its `step` has, else as a value is written.
pub(crate) fn display_number(value: f64, step: Option<f64>, integer: bool) -> String {
    if integer {
        return format!("{value:.0}");
    }
    match step {
        Some(step) => format!("{value:.*}", step_places(step)),
        None => format_component_number(value),
    }
}

/// `number` followed by its unit after one space, or alone.
pub(crate) fn with_unit(number: &str, unit: Option<&str>) -> String {
    match unit {
        Some(unit) => format!("{number} {unit}"),
        None => number.to_owned(),
    }
}

/// What a press on a row started from: its label drags only; its value
/// drags, or opens typing when released without a drag.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ScrubOrigin {
    #[default]
    Label,
    Value,
}

/// What a released press does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ScrubEnd {
    /// Write the dragged value, once.
    Write(f64),
    /// A drag back at its start, or a label pressed without a drag.
    Nothing,
    /// A value pressed without a drag: open the row's field to type.
    Type,
}

/// Whether a number row shows its text field rather than its resting value.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct RowTyping(bool);

impl RowTyping {
    /// Follows the row's field: open once it has the focus, closed by Enter,
    /// a blur or Escape.
    pub(crate) fn on(&mut self, event: &crate::property_controls::ControlEvent) {
        use crate::property_controls::ControlEvent;
        match event {
            ControlEvent::Focused => self.0 = true,
            ControlEvent::Submit { .. } | ControlEvent::Escape => self.0 = false,
            _ => {}
        }
    }

    /// Enter on the focused resting value.
    pub(crate) fn request(&mut self) {
        self.0 = true;
    }

    /// Whether the field shows: while typing, and while it holds typed text
    /// (`edited`) or text that was not written (`invalid`).
    pub(crate) fn open(self, edited: bool, invalid: bool) -> bool {
        self.0 || edited || invalid
    }
}

type Press = Rc<dyn Fn(f32, &mut Window, &mut App)>;
type Type = Rc<dyn Fn(&mut Window, &mut App)>;

/// See the module documentation. Debug selectors `{id}-rest` and
/// `{id}-input`.
#[derive(IntoElement)]
pub(crate) struct ScrubValue {
    id: SharedString,
    input: Entity<InputState>,
    field: Option<AnyElement>,
    text: String,
    placeholder: bool,
    typing: bool,
    locked: bool,
    on_press: Option<Press>,
    on_type: Option<Type>,
}

impl ScrubValue {
    pub(crate) fn new(id: impl Into<SharedString>, input: Entity<InputState>) -> Self {
        Self {
            id: id.into(),
            input,
            field: None,
            text: String::new(),
            placeholder: false,
            typing: false,
            locked: false,
            on_press: None,
            on_type: None,
        }
    }

    /// The resting text, the number with its unit.
    pub(crate) fn text(mut self, text: String) -> Self {
        self.text = text;
        self
    }

    /// The text is the dimmed value an absent prop takes, not a written one.
    pub(crate) fn placeholder(mut self, placeholder: bool) -> Self {
        self.placeholder = placeholder;
        self
    }

    /// The field shown while typing (the panel's escapable text input);
    /// a plain `Input` over the same state without one.
    pub(crate) fn field(mut self, field: impl IntoElement) -> Self {
        self.field = Some(field.into_any_element());
        self
    }

    pub(crate) fn typing(mut self, typing: bool) -> Self {
        self.typing = typing;
        self
    }

    /// Muted, not pressable, not focusable.
    pub(crate) fn locked(mut self, locked: bool) -> Self {
        self.locked = locked;
        self
    }

    /// A left press on the resting value, at the pointer's window x.
    pub(crate) fn on_press(mut self, press: impl Fn(f32, &mut Window, &mut App) + 'static) -> Self {
        self.on_press = Some(Rc::new(press));
        self
    }

    /// Enter on the focused resting value.
    pub(crate) fn on_type(mut self, request: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_type = Some(Rc::new(request));
        self
    }
}

impl RenderOnce for ScrubValue {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let id = self.id;
        if self.typing && !self.locked {
            let field = self
                .field
                .unwrap_or_else(|| Input::new(&self.input).small().into_any_element());
            return div()
                .w_full()
                .debug_selector(move || format!("{id}-input"))
                .child(field)
                .into_any_element();
        }
        let theme = cx.theme();
        let focus = self.input.read(cx).focus_handle(cx);
        let locked = self.locked;
        let press = self.on_press.filter(|_| !locked);
        let request = self.on_type.filter(|_| !locked);
        div()
            .id(ElementId::Name(format!("{id}-rest").into()))
            .debug_selector({
                let id = id.clone();
                move || format!("{id}-rest")
            })
            .flex()
            .items_center()
            .w_full()
            .h_6()
            .px_2()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .font_family(theme.mono_font_family.clone())
            .text_base()
            .text_color(if locked || self.placeholder {
                theme.muted_foreground
            } else {
                theme.foreground
            })
            .when(!locked, |rest| {
                rest.track_focus(&focus)
                    .cursor(CursorStyle::ResizeLeftRight)
                    .hover(|rest| rest.bg(theme.secondary_hover))
            })
            .when_some(press, |rest, press| {
                rest.on_mouse_down(
                    MouseButton::Left,
                    move |event: &MouseDownEvent, window, cx| {
                        // The panel decides where the focus goes once it knows
                        // whether this is a drag or a click.
                        window.prevent_default();
                        press(f32::from(event.position.x), window, cx);
                    },
                )
            })
            .when_some(request, |rest, request| {
                rest.on_key_down(move |event: &KeyDownEvent, window, cx| {
                    if event.keystroke.key == "enter" && !event.keystroke.modifiers.modified() {
                        cx.stop_propagation();
                        request(window, cx);
                    }
                })
            })
            .child(self.text)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::property_controls::parse_number_input;
    use gpui_kit::component::Root;
    use gpui_kit::{
        Context, KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, TestAppContext, VisualTestContext,
        WindowHandle, px, size,
    };
    use std::cell::Cell;

    #[test]
    fn a_press_becomes_a_drag_after_three_pixels() {
        assert_eq!(drag_distance(2.9), None);
        assert_eq!(drag_distance(-2.9), None);
        assert_eq!(drag_distance(3.0), Some(0.0));
        assert_eq!(drag_distance(10.0), Some(7.0));
        assert_eq!(drag_distance(-10.0), Some(-7.0));
    }

    #[test]
    fn numbers_show_their_steps_places_and_unit() {
        assert_eq!(display_number(3.0, None, true), "3");
        assert_eq!(display_number(1.234, Some(0.01), false), "1.23");
        assert_eq!(display_number(0.5, Some(0.5), false), "0.5");
        assert_eq!(display_number(400.0, Some(100.0), false), "400");
        assert_eq!(display_number(0.125, None, false), "0.125");
        assert_eq!(with_unit("120", Some("px")), "120 px");
        assert_eq!(with_unit("1.00", None), "1.00");
    }

    #[test]
    fn typed_numbers_may_carry_their_own_unit() {
        assert_eq!(parse_number_input("120px", Some("px")), Ok(120.0));
        assert_eq!(parse_number_input(" 120 px ", Some("px")), Ok(120.0));
        assert_eq!(parse_number_input("120", Some("px")), Ok(120.0));
        assert_eq!(parse_number_input("45°", Some("°")), Ok(45.0));
        assert!(parse_number_input("120 em", Some("px")).is_err());
        assert!(parse_number_input("120px", None).is_err());
    }

    /// A window showing one `ScrubValue` over its own input; records the
    /// presses and the requests to type.
    struct Host {
        input: Entity<InputState>,
        typing: bool,
        locked: bool,
        pressed: Rc<Cell<Option<f32>>>,
        typed: Rc<Cell<bool>>,
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let pressed = self.pressed.clone();
            let typed = self.typed.clone();
            div().size_full().child(
                ScrubValue::new("v", self.input.clone())
                    .text("120 px".to_owned())
                    .typing(self.typing)
                    .locked(self.locked)
                    .on_press(move |x, _, _| pressed.set(Some(x)))
                    .on_type(move |_, _| typed.set(true)),
            )
        }
    }

    fn window_with(cx: &mut TestAppContext, locked: bool) -> WindowHandle<Root> {
        cx.update(|cx| {
            gpui_kit::init(cx);
            celesta_editor_theme::init(cx);
        });
        let handle = cx.open_window(size(px(300.), px(100.)), |window, cx| {
            let host = cx.new(|cx| Host {
                input: cx.new(|cx| InputState::new(window, cx)),
                typing: false,
                locked,
                pressed: Rc::new(Cell::new(None)),
                typed: Rc::new(Cell::new(false)),
            });
            Root::new(host, window, cx)
        });
        handle
            .update(cx, |_, window, _| window.activate_window())
            .expect("the window");
        handle
    }

    fn host(cx: &mut VisualTestContext, handle: WindowHandle<Root>) -> Entity<Host> {
        handle
            .read_with(cx, |root, _| root.view().clone().downcast::<Host>())
            .expect("the window")
            .expect("the host")
    }

    #[gpui_kit::test]
    fn the_resting_value_reports_a_press_and_typing_shows_the_field(cx: &mut TestAppContext) {
        let handle = window_with(cx, false);
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        cx.run_until_parked();
        assert!(cx.debug_bounds("v-input").is_none());
        let rest = cx.debug_bounds("v-rest").expect("the resting value");
        cx.simulate_mouse_down(
            rest.center(),
            gpui_kit::MouseButton::Left,
            Modifiers::none(),
        );
        cx.run_until_parked();
        let host = host(cx, handle);
        let pressed = host.read_with(cx, |host, _| host.pressed.get());
        assert_eq!(pressed, Some(f32::from(rest.center().x)));

        host.update(cx, |host, cx| {
            host.typing = true;
            cx.notify();
        });
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("v-input").is_some(),
            "typing shows the field"
        );
        assert!(cx.debug_bounds("v-rest").is_none());
    }

    #[gpui_kit::test]
    fn a_locked_value_ignores_presses(cx: &mut TestAppContext) {
        let handle = window_with(cx, true);
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        cx.run_until_parked();
        let rest = cx.debug_bounds("v-rest").expect("the resting value");
        cx.simulate_mouse_down(
            rest.center(),
            gpui_kit::MouseButton::Left,
            Modifiers::none(),
        );
        cx.run_until_parked();
        let host = host(cx, handle);
        assert_eq!(host.read_with(cx, |host, _| host.pressed.get()), None);
    }

    #[gpui_kit::test]
    fn enter_on_the_focused_resting_value_opens_typing(cx: &mut TestAppContext) {
        let handle = window_with(cx, false);
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        cx.run_until_parked();
        let host = host(cx, handle);
        // The state after Escape: the field's handle keeps the focus while
        // the row rests.
        let input = host.read_with(cx, |host, _| host.input.clone());
        cx.update(|window, cx| input.read(cx).focus_handle(cx).focus(window, cx));
        cx.run_until_parked();
        let keystroke = Keystroke::parse("enter").expect("a keystroke");
        cx.simulate_event(KeyDownEvent {
            keystroke: keystroke.clone(),
            is_held: false,
            prefer_character_input: false,
        });
        cx.simulate_event(KeyUpEvent { keystroke });
        cx.run_until_parked();
        assert!(host.read_with(cx, |host, _| host.typed.get()));
    }
}
