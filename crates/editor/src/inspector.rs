//! The Inspector's shared pieces, built like the timeline's rows: a
//! section with a collapsible header band, and a row of three lanes (the
//! label, the value, a trailing icon lane), so every tab of the right dock
//! lines its labels and values up on the same edges.

use std::collections::HashSet;
use std::rc::Rc;

use celesta_editor_theme::metrics;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, ElementId, FontWeight, MouseButton, MouseDownEvent, SharedString, Window, div,
    rems,
};

type Toggle = Rc<dyn Fn(&mut Window, &mut App)>;
type LabelPress = Rc<dyn Fn(&MouseDownEvent, &mut Window, &mut App)>;

/// The sections collapsed this session, by section id. Not saved: every
/// section starts open on the next launch.
#[derive(Debug, Default)]
pub(crate) struct CollapsedSections(HashSet<SharedString>);

impl CollapsedSections {
    pub(crate) fn is_collapsed(&self, id: &str) -> bool {
        self.0.contains(id)
    }

    pub(crate) fn toggle(&mut self, id: &str) {
        if !self.0.remove(id) {
            self.0.insert(SharedString::from(id.to_owned()));
        }
    }
}

/// A section of the Inspector: a header band with a chevron and the title,
/// which collapses the section, then its rows. The id is the caller's
/// stable name for the section (`properties/transform`), not its title.
#[derive(IntoElement)]
pub(crate) struct InspectorSection {
    id: SharedString,
    title: SharedString,
    collapsed: bool,
    on_toggle: Option<Toggle>,
    trailing: Vec<AnyElement>,
    children: Vec<AnyElement>,
}

impl InspectorSection {
    pub(crate) fn new(id: impl Into<SharedString>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            collapsed: false,
            on_toggle: None,
            trailing: Vec::new(),
            children: Vec::new(),
        }
    }

    pub(crate) fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }

    /// Called when the header is clicked, or Space or Enter pressed on it.
    pub(crate) fn on_toggle(mut self, toggle: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_toggle = Some(Rc::new(toggle));
        self
    }

    /// Appended to the header's trailing slot, after what is there.
    pub(crate) fn push_trailing(mut self, element: impl IntoElement) -> Self {
        self.trailing.push(element.into_any_element());
        self
    }
}

impl ParentElement for InspectorSection {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for InspectorSection {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let id = self.id;
        let focus = window
            .use_keyed_state(
                ElementId::Name(format!("{id}-toggle-focus").into()),
                cx,
                |_, cx| cx.focus_handle(),
            )
            .read(cx)
            .clone();
        let theme = cx.theme();
        let toggle = self.on_toggle;
        let collapsed = self.collapsed;
        let chevron = if self.collapsed {
            IconName::ChevronRight
        } else {
            IconName::ChevronDown
        };
        let header = div()
            .flex()
            .flex_none()
            .items_center()
            .gap_1()
            .h(metrics::inspector_row_height())
            // The button's own `px_1` makes up the panel padding, so the
            // chevron starts on the rows' label edge.
            .pl(metrics::inspector_padding() - rems(0.25))
            .pr(metrics::inspector_padding())
            .bg(theme.title_bar)
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .items_center()
                    .debug_selector({
                        let id = id.clone();
                        move || format!("{id}-header")
                    })
                    .child(
                        Button::new(ElementId::Name(format!("{id}-toggle").into()))
                            .ghost()
                            .xsmall()
                            .w_full()
                            .child(
                                div()
                                    .flex()
                                    .flex_1()
                                    .min_w_0()
                                    .items_center()
                                    .gap_1()
                                    .child(
                                        Icon::new(chevron)
                                            .xsmall()
                                            .text_color(theme.muted_foreground),
                                    )
                                    .child(
                                        div()
                                            .min_w_0()
                                            .overflow_hidden()
                                            .whitespace_nowrap()
                                            .text_ellipsis()
                                            .text_sm()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(theme.foreground)
                                            .child(self.title),
                                    ),
                            )
                            .track_focus(&focus)
                            // The editor's Space shortcut steps aside here,
                            // so Space toggles the focused header.
                            .key_context("InspectorHeader")
                            .when_some(toggle, |button, toggle| {
                                button.on_click(move |_, window, cx| {
                                    // A click does not move the focus to a
                                    // button, so a field inside the section
                                    // would keep it while hidden: the header
                                    // takes it, and the field submits as any
                                    // blur does.
                                    if !collapsed {
                                        focus.focus(window, cx);
                                    }
                                    toggle(window, cx);
                                })
                            }),
                    ),
            )
            .children(self.trailing);
        div()
            .id(ElementId::Name(id.clone()))
            .flex()
            .flex_col()
            .child(header)
            .when(!self.collapsed, |section| {
                section.child(
                    div()
                        .flex()
                        .flex_col()
                        .py_1()
                        .debug_selector(move || format!("{id}-body"))
                        .children(self.children),
                )
            })
    }
}

impl crate::view::EditorView {
    /// The section `id` titled `title`, collapsed and toggled through
    /// [`crate::view::EditorView::collapsed_sections`].
    pub(crate) fn section(
        &self,
        id: impl Into<SharedString>,
        title: impl Into<SharedString>,
        cx: &mut gpui_kit::Context<Self>,
    ) -> InspectorSection {
        let id: SharedString = id.into();
        let view = cx.entity().downgrade();
        let toggled = id.clone();
        InspectorSection::new(id.clone(), title)
            .collapsed(self.collapsed_sections.is_collapsed(&id))
            .on_toggle(move |_, cx| {
                view.update(cx, |this, cx| {
                    this.collapsed_sections.toggle(&toggled);
                    cx.notify();
                })
                .ok();
            })
    }
}

/// A row of the Inspector: `[label][value][trailing]`. The label lane has
/// one width for every row and the trailing lane is always there, so the
/// values of every row share one left edge whatever the row holds.
#[derive(IntoElement)]
pub(crate) struct InspectorRow {
    id: SharedString,
    label: SharedString,
    value: Option<AnyElement>,
    trailing: Option<AnyElement>,
    indented: bool,
    top_aligned: bool,
    sub_lines: Vec<AnyElement>,
    label_press: Option<LabelPress>,
}

/// A read-only row: `label` and `value` as text, the value in monospace
/// (`mono`) when it is a number, a size, a timecode or an identifier. A long
/// value wraps; `top_aligned` keeps the label on its first line.
pub(crate) fn fact(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    value: impl Into<SharedString>,
    mono: bool,
    cx: &App,
) -> InspectorRow {
    let value = div()
        .when(mono, |value| {
            value.font_family(cx.theme().mono_font_family.clone())
        })
        .child(value.into());
    InspectorRow::new(id, label).value(value)
}

/// How far an indented row's label moves in: a whole rem, so the label lane
/// it takes from and the padding it adds snap to the same pixel.
fn indent() -> gpui_kit::Rems {
    rems(1.0)
}

impl InspectorRow {
    pub(crate) fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            value: None,
            trailing: None,
            indented: false,
            top_aligned: false,
            sub_lines: Vec::new(),
            label_press: None,
        }
    }

    pub(crate) fn value(mut self, value: impl IntoElement) -> Self {
        self.value = Some(value.into_any_element());
        self
    }

    /// The state icon or the button in the trailing lane.
    pub(crate) fn trailing(mut self, element: impl IntoElement) -> Self {
        self.trailing = Some(element.into_any_element());
        self
    }

    /// An input sub-row: the label moves in one step; the value lane stays
    /// on the others' edge.
    pub(crate) fn indented(mut self, indented: bool) -> Self {
        self.indented = indented;
        self
    }

    /// For a value of several lines: the label sits at the row's top.
    pub(crate) fn top_aligned(mut self, top_aligned: bool) -> Self {
        self.top_aligned = top_aligned;
        self
    }

    /// A line under the row, starting at the value lane's edge.
    pub(crate) fn sub_line(mut self, line: impl IntoElement) -> Self {
        self.sub_lines.push(line.into_any_element());
        self
    }

    /// A mouse down on the label (a number row's label scrubs its value),
    /// with the left-right resize cursor over it.
    pub(crate) fn on_label_press(
        mut self,
        press: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.label_press = Some(Rc::new(press));
        self
    }
}

impl RenderOnce for InspectorRow {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let id = self.id;
        let selector = |lane: &'static str| {
            let id = id.clone();
            move || format!("{id}-{lane}")
        };
        let label_width = if self.indented {
            metrics::inspector_label_width() - indent()
        } else {
            metrics::inspector_label_width()
        };
        let full_label = self.label.clone();
        let label = div()
            .id(ElementId::Name(format!("{id}-label").into()))
            .flex_none()
            .w(label_width)
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .text_sm()
            .text_color(theme.muted_foreground)
            .debug_selector(selector("label"))
            .tooltip(move |window, cx| Tooltip::new(full_label.clone()).build(window, cx))
            .when_some(self.label_press, |label, press| {
                label
                    .cursor(gpui_kit::CursorStyle::ResizeLeftRight)
                    .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                        press(event, window, cx)
                    })
            })
            .child(self.label);
        let line = div()
            .flex()
            .min_h(metrics::inspector_row_height())
            .when(self.top_aligned, |line| line.items_start().py_1())
            .when(!self.top_aligned, |line| line.items_center())
            .gap_2()
            .child(label)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_base()
                    .text_color(theme.foreground)
                    .debug_selector(selector("value"))
                    .children(self.value),
            )
            .child(
                div()
                    .flex_none()
                    .w(metrics::inspector_trailing_width())
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(theme.muted_foreground)
                    .debug_selector(selector("trailing"))
                    .children(self.trailing),
            );
        let has_sub_lines = !self.sub_lines.is_empty();
        div()
            .flex()
            .flex_col()
            .pr(metrics::inspector_padding())
            .when(self.indented, |row| {
                row.pl(metrics::inspector_padding() + indent())
            })
            .when(!self.indented, |row| row.pl(metrics::inspector_padding()))
            .child(line)
            .when(has_sub_lines, |row| {
                row.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .pb_1()
                        // The value lane's edge: the label lane and the gap.
                        .pl(label_width + rems(0.5))
                        .pr(metrics::inspector_trailing_width() + rems(0.5))
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .children(self.sub_lines),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::Context;
    use gpui_kit::component::Root;
    use gpui_kit::component::input::{Input, InputEvent, InputState};
    use gpui_kit::{
        Entity, KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, TestAppContext, VisualTestContext,
        WindowHandle, px, size,
    };

    type Build = Rc<dyn Fn(&mut Host, &mut Window, &mut Context<Host>) -> AnyElement>;

    /// A window rebuilding one element from `build` on every frame, with
    /// the collapse state and an input the tests may use.
    struct Host {
        sections: CollapsedSections,
        input: Option<Entity<InputState>>,
        blurred: bool,
        /// Whether the editor's Space shortcut fired.
        played: bool,
        build: Build,
    }

    impl Render for Host {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let build = self.build.clone();
            div()
                .size_full()
                // The editor's key context, so its shortcuts reach the host.
                .key_context("CelestaEditor")
                .on_action(
                    cx.listener(|host, _: &crate::actions::TogglePlayback, _, _| {
                        host.played = true;
                    }),
                )
                .child(build(self, window, cx))
        }
    }

    fn window_with(
        cx: &mut TestAppContext,
        width: f32,
        with_input: bool,
        build: impl Fn(&mut Host, &mut Window, &mut Context<Host>) -> AnyElement + 'static,
    ) -> WindowHandle<Root> {
        cx.update(|cx| {
            gpui_kit::init(cx);
            celesta_editor_theme::init(cx);
        });
        let build: Build = Rc::new(build);
        let handle = cx.open_window(size(px(width), px(400.)), |window, cx| {
            let host = cx.new(|cx| {
                let input = with_input.then(|| cx.new(|cx| InputState::new(window, cx)));
                if let Some(input) = &input {
                    cx.subscribe(input, |host: &mut Host, _, event: &InputEvent, _| {
                        if matches!(event, InputEvent::Blur) {
                            host.blurred = true;
                        }
                    })
                    .detach();
                }
                Host {
                    sections: CollapsedSections::default(),
                    input,
                    blurred: false,
                    played: false,
                    build,
                }
            });
            Root::new(host, window, cx)
        });
        // Focus events (a field's blur) reach only an active window.
        handle
            .update(cx, |_, window, _| window.activate_window())
            .expect("the window");
        handle
    }

    /// One section `s` toggled through the host's collapse state, holding
    /// `rows`.
    fn section(host: &mut Host, cx: &mut Context<Host>, rows: Vec<AnyElement>) -> AnyElement {
        let this = cx.entity().downgrade();
        InspectorSection::new("s", "Transform")
            .collapsed(host.sections.is_collapsed("s"))
            .on_toggle(move |_, cx| {
                this.update(cx, |host, cx| {
                    host.sections.toggle("s");
                    cx.notify();
                })
                .ok();
            })
            .children(rows)
            .into_any_element()
    }

    /// Space pressed and released: a button acts on the release.
    fn press_space(cx: &mut VisualTestContext) {
        let keystroke = Keystroke::parse("space").expect("a keystroke");
        cx.simulate_event(KeyDownEvent {
            keystroke: keystroke.clone(),
            is_held: false,
            prefer_character_input: false,
        });
        cx.simulate_event(KeyUpEvent { keystroke });
        cx.run_until_parked();
    }

    #[test]
    fn a_section_toggles_by_id() {
        let mut sections = CollapsedSections::default();
        assert!(!sections.is_collapsed("properties/transform"));
        sections.toggle("properties/transform");
        assert!(sections.is_collapsed("properties/transform"));
        assert!(!sections.is_collapsed("properties/size"));
        sections.toggle("properties/transform");
        assert!(!sections.is_collapsed("properties/transform"));
    }

    #[gpui_kit::test]
    fn clicking_the_header_hides_and_shows_the_rows(cx: &mut TestAppContext) {
        let handle = window_with(cx, 400., false, |host, _, cx| {
            let row = InspectorRow::new("r", "X").value("0").into_any_element();
            section(host, cx, vec![row])
        });
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        cx.run_until_parked();
        assert!(cx.debug_bounds("s-body").is_some());
        assert!(cx.debug_bounds("r-value").is_some());

        let header = cx.debug_bounds("s-header").expect("the header");
        cx.simulate_click(header.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(cx.debug_bounds("s-body").is_none(), "collapsed");
        assert!(cx.debug_bounds("r-value").is_none());

        cx.simulate_click(header.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(cx.debug_bounds("s-body").is_some(), "open again");
    }

    #[gpui_kit::test]
    fn space_toggles_a_focused_header(cx: &mut TestAppContext) {
        // With the editor's own shortcuts, where Space also means play.
        cx.update(|cx| cx.bind_keys(crate::key_bindings()));
        let handle = window_with(cx, 400., false, |host, _, cx| {
            let row = InspectorRow::new("r", "X").value("0").into_any_element();
            section(host, cx, vec![row])
        });
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        cx.run_until_parked();
        // A click does not focus a button; Tab does.
        cx.update(|window, cx| window.focus_next(cx));
        press_space(cx);
        assert!(cx.debug_bounds("s-body").is_none(), "space collapses it");
        let played = handle
            .read_with(cx, |root, cx| {
                root.view()
                    .clone()
                    .downcast::<Host>()
                    .expect("the host")
                    .read(cx)
                    .played
            })
            .expect("the window");
        assert!(!played, "space on a header does not start playback");

        press_space(cx);
        cx.run_until_parked();
        assert!(cx.debug_bounds("s-body").is_some(), "space opens it again");
    }

    #[gpui_kit::test]
    fn collapsing_blurs_a_field_inside_the_section(cx: &mut TestAppContext) {
        let handle = window_with(cx, 400., true, |host, _, cx| {
            let input = host.input.clone().expect("an input");
            let row = InspectorRow::new("r", "Gap")
                .value(
                    div()
                        .debug_selector(|| "r-field".into())
                        .child(Input::new(&input)),
                )
                .into_any_element();
            section(host, cx, vec![row])
        });
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        cx.run_until_parked();
        let field = cx.debug_bounds("r-field").expect("the field");
        cx.simulate_click(field.center(), Modifiers::none());
        cx.run_until_parked();

        let header = cx.debug_bounds("s-header").expect("the header");
        cx.simulate_click(header.center(), Modifiers::none());
        cx.run_until_parked();
        let blurred = handle
            .read_with(cx, |root, cx| {
                root.view()
                    .clone()
                    .downcast::<Host>()
                    .expect("the host")
                    .read(cx)
                    .blurred
            })
            .expect("the window");
        assert!(blurred, "the field blurred, so it submitted");
    }

    #[gpui_kit::test]
    fn value_lanes_share_one_edge(cx: &mut TestAppContext) {
        let handle = window_with(cx, 400., false, |_, _, _| {
            div()
                .flex()
                .flex_col()
                .child(InspectorRow::new("plain", "X").value("0"))
                .child(
                    InspectorRow::new("icon", "Rotation")
                        .value("0")
                        .trailing(Icon::new(IconName::Lock).xsmall()),
                )
                .child(
                    InspectorRow::new("sub", "padding")
                        .value("8")
                        .indented(true),
                )
                .into_any_element()
        });
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        cx.run_until_parked();
        let value = |cx: &mut VisualTestContext, id: &str| {
            cx.debug_bounds(Box::leak(format!("{id}-value").into_boxed_str()))
                .expect("a value lane")
        };
        let trailing = |cx: &mut VisualTestContext, id: &str| {
            cx.debug_bounds(Box::leak(format!("{id}-trailing").into_boxed_str()))
                .expect("a trailing lane")
        };
        let plain = value(cx, "plain");
        assert_eq!(value(cx, "icon").origin.x, plain.origin.x);
        assert_eq!(value(cx, "sub").origin.x, plain.origin.x);
        let width = trailing(cx, "plain").size.width;
        assert_eq!(trailing(cx, "icon").size.width, width);
        assert_eq!(trailing(cx, "sub").size.width, width);
    }

    #[gpui_kit::test]
    fn a_narrow_row_keeps_its_trailing_lane(cx: &mut TestAppContext) {
        let long = "A label far too long for any inspector label lane at all";
        let value = "a value far too long for the space left beside the label";
        let handle = window_with(cx, 220., false, move |_, _, _| {
            InspectorRow::new("r", long).value(value).into_any_element()
        });
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        cx.run_until_parked();
        let trailing = cx.debug_bounds("r-trailing").expect("the trailing lane");
        let value = cx.debug_bounds("r-value").expect("the value lane");
        // 1.25rem at the 13px base, snapped to whole pixels by layout.
        assert!((f32::from(trailing.size.width) - 16.25).abs() < 0.5);
        assert!(value.origin.x + value.size.width <= trailing.origin.x);
        assert!(trailing.origin.x + trailing.size.width <= px(220.));
    }
}
