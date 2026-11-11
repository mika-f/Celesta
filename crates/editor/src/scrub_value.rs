//! The number control of an Inspector row: at rest the value and its unit
//! as text, dragged left or right to change it; a click without a drag (or
//! Tab, or Enter on it) opens the row's text field to type.
//!
//! The control only reports: a press (where, so the panel can start its
//! scrub) and a request to type. The panel owns the scrub, decides between a
//! drag and a click on release, and says whether the row is typing.

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
pub(crate) fn step_places(step: f64) -> usize {
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RowTyping {
    open: bool,
    /// Enter or Escape closed the row and its field kept the focus: the
    /// focus the window sends again when it comes back to the front does
    /// not reopen it; Enter on the resting value ([`Self::request`]) does.
    /// Going to the back blurs the field too, so only a blur in the active
    /// window clears it.
    rests_focused: bool,
}

impl RowTyping {
    /// Follows the row's field: open once it gains the focus, closed by
    /// Enter, a blur or Escape. `window_active` is whether the window was
    /// in front when `event` came: a blur from going to the back does not
    /// count as the focus leaving the row.
    pub(crate) fn on(
        &mut self,
        event: &crate::property_controls::ControlEvent,
        window_active: bool,
    ) {
        use crate::property_controls::ControlEvent;
        match event {
            ControlEvent::Focused if !self.rests_focused => self.open = true,
            ControlEvent::Submit { blur: false } | ControlEvent::Escape => {
                self.open = false;
                self.rests_focused = true;
            }
            ControlEvent::Submit { blur: true } => {
                self.open = false;
                if window_active {
                    self.rests_focused = false;
                }
            }
            _ => {}
        }
    }

    /// Opens the row: a click on its value, a refused press, or Enter on the
    /// focused resting value.
    pub(crate) fn request(&mut self) {
        self.open = true;
    }

    /// Whether the field shows: while typing, and while it holds typed text
    /// (`edited`) or text that was not written (`invalid`).
    pub(crate) fn open(self, edited: bool, invalid: bool) -> bool {
        self.open || edited || invalid
    }
}

/// What a press does when the scope refuses its drag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RefusedPress {
    /// A value pressed: open its field to type; a typed commit is held or
    /// refused as any typed commit is.
    Type,
    /// A label pressed: say why under the row.
    Say,
}

/// See [`RefusedPress`].
pub(crate) fn refused_press(origin: ScrubOrigin) -> RefusedPress {
    match origin {
        ScrubOrigin::Value => RefusedPress::Type,
        ScrubOrigin::Label => RefusedPress::Say,
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
    unit: Option<SharedString>,
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
            unit: None,
            placeholder: false,
            typing: false,
            locked: false,
            on_press: None,
            on_type: None,
        }
    }

    /// The resting number.
    pub(crate) fn text(mut self, text: String) -> Self {
        self.text = text;
        self
    }

    /// The unit after the number, muted; a narrow lane cuts the number, not
    /// the unit.
    pub(crate) fn unit(mut self, unit: Option<SharedString>) -> Self {
        self.unit = unit;
        self
    }

    /// The text is the dimmed value an absent prop takes, not a written one.
    pub(crate) fn placeholder(mut self, placeholder: bool) -> Self {
        self.placeholder = placeholder;
        self
    }

    /// The field shown while typing: `wrap` dresses an `Input` over this
    /// value's own state (the panel's escapable text input), so the field
    /// is the one whose focus the resting value tracks. A plain small
    /// `Input` without one.
    pub(crate) fn field<E: IntoElement>(mut self, wrap: impl FnOnce(Input) -> E) -> Self {
        self.field = Some(wrap(Input::new(&self.input)).into_any_element());
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
            .gap_1()
            .w_full()
            .h_6()
            .px_2()
            .overflow_hidden()
            .whitespace_nowrap()
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
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(self.text),
            )
            .when_some(self.unit, |rest, unit| {
                rest.child(
                    div()
                        .flex_none()
                        .text_color(theme.muted_foreground)
                        .debug_selector({
                            let id = id.clone();
                            move || format!("{id}-unit")
                        })
                        .child(unit),
                )
            })
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
    fn numbers_show_their_steps_places() {
        assert_eq!(display_number(3.0, None, true), "3");
        assert_eq!(display_number(1.234, Some(0.01), false), "1.23");
        assert_eq!(display_number(0.5, Some(0.5), false), "0.5");
        assert_eq!(display_number(400.0, Some(100.0), false), "400");
        assert_eq!(display_number(0.125, None, false), "0.125");
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
                    .text("120".to_owned())
                    .unit(Some("px".into()))
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

    /// One `ScrubValue` whose row follows its field's focus as the panels'
    /// rows do.
    struct Tabbed {
        input: Entity<InputState>,
        row: RowTyping,
    }

    impl Render for Tabbed {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(
                ScrubValue::new("t", self.input.clone())
                    .text("120".to_owned())
                    .typing(self.row.open(false, false))
                    .field(|input| input),
            )
        }
    }

    /// A window holding one [`Tabbed`] row, active so focus events reach
    /// it.
    fn tabbed_window(cx: &mut TestAppContext) -> gpui_kit::WindowHandle<Root> {
        use crate::property_controls::ControlEvent;
        use gpui_kit::component::input::InputEvent;
        cx.update(|cx| {
            gpui_kit::init(cx);
            celesta_editor_theme::init(cx);
        });
        let handle = cx.open_window(size(px(300.), px(100.)), |window, cx| {
            let host = cx.new(|cx| {
                let input = cx.new(|cx| InputState::new(window, cx));
                cx.subscribe_in(
                    &input,
                    window,
                    |host: &mut Tabbed, _, event: &InputEvent, window, cx| {
                        let event = match event {
                            InputEvent::Focus => ControlEvent::Focused,
                            InputEvent::PressEnter { .. } => ControlEvent::Submit { blur: false },
                            InputEvent::Blur => ControlEvent::Submit { blur: true },
                            InputEvent::Change => return,
                        };
                        host.row.on(&event, window.is_window_active());
                        cx.notify();
                    },
                )
                .detach();
                Tabbed {
                    input,
                    row: RowTyping::default(),
                }
            });
            Root::new(host, window, cx)
        });
        handle
            .update(cx, |_, window, _| window.activate_window())
            .expect("the window");
        handle
    }

    #[gpui_kit::test]
    fn tab_opens_a_resting_number_for_typing(cx: &mut TestAppContext) {
        let handle = tabbed_window(cx);
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        cx.run_until_parked();
        assert!(cx.debug_bounds("t-input").is_none(), "it rests");

        cx.update(|window, cx| window.focus_next(cx));
        cx.run_until_parked();
        assert!(cx.debug_bounds("t-input").is_some(), "Tab opens it");
    }

    #[gpui_kit::test]
    fn coming_back_to_the_window_leaves_a_resting_number_resting(cx: &mut TestAppContext) {
        let handle = tabbed_window(cx);
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        cx.run_until_parked();
        cx.update(|window, cx| window.focus_next(cx));
        cx.run_until_parked();
        let keystroke = Keystroke::parse("enter").expect("a keystroke");
        cx.simulate_event(KeyDownEvent {
            keystroke: keystroke.clone(),
            is_held: false,
            prefer_character_input: false,
        });
        cx.simulate_event(KeyUpEvent { keystroke });
        cx.run_until_parked();
        assert!(cx.debug_bounds("t-input").is_none(), "Enter rests it");

        // Going to the back blurs the field; coming back focuses it again.
        cx.deactivate_window();
        cx.run_until_parked();
        cx.update(|window, _| window.activate_window());
        cx.run_until_parked();
        assert!(cx.debug_bounds("t-input").is_none(), "it still rests");
    }

    struct Narrow {
        input: Entity<InputState>,
    }

    impl Render for Narrow {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().w(px(90.)).child(
                ScrubValue::new("n", self.input.clone())
                    .text("123456789.123456".to_owned())
                    .unit(Some("px".into())),
            )
        }
    }

    #[gpui_kit::test]
    fn a_narrow_value_cuts_the_number_before_its_unit(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            celesta_editor_theme::init(cx);
        });
        let handle = cx.open_window(size(px(300.), px(100.)), |window, cx| {
            let host = cx.new(|cx| Narrow {
                input: cx.new(|cx| InputState::new(window, cx)),
            });
            Root::new(host, window, cx)
        });
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        cx.run_until_parked();
        let rest = cx.debug_bounds("n-rest").expect("the resting value");
        let unit = cx.debug_bounds("n-unit").expect("the unit");
        assert!(unit.size.width > px(0.), "the unit keeps its width");
        assert!(unit.origin.x + unit.size.width <= rest.origin.x + rest.size.width);
    }
}
