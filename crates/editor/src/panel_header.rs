//! The header band every editor panel shares.
//!
//! Built like the timeline's ruler row: the header band color, one hairline
//! below, the title on the leading edge, then an optional count, the panel's
//! own actions, and a `•••` menu of the panel's options in a fixed trailing
//! lane. A panel without options shows no `•••`; there are no empty menus.

use std::rc::Rc;

use celesta_editor_theme::metrics;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu};
use gpui_kit::component::{ActiveTheme as _, IconName, Sizable as _};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, App, Context, ElementId, FontWeight, SharedString, Window, div};

type OptionsMenu = Rc<dyn Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu>;

/// A panel's header band. See the module documentation.
#[derive(IntoElement)]
pub(crate) struct PanelHeader {
    id: SharedString,
    title: SharedString,
    count: usize,
    actions: Vec<AnyElement>,
    options: Option<OptionsMenu>,
}

impl PanelHeader {
    pub(crate) fn new(id: impl Into<SharedString>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            count: 0,
            actions: Vec::new(),
            options: None,
        }
    }

    /// Shown, monospace and muted, only when greater than zero.
    pub(crate) fn count(mut self, count: usize) -> Self {
        self.count = count;
        self
    }

    /// Appended to the trailing slot, before `•••`.
    #[allow(dead_code)] // P7b–d give panels their actions.
    pub(crate) fn action(mut self, action: impl IntoElement) -> Self {
        self.actions.push(action.into_any_element());
        self
    }

    /// Adds the `•••` button opening this menu of the panel's options.
    #[allow(dead_code)] // No panel has options yet; P7b–d add them.
    pub(crate) fn options(
        mut self,
        menu: impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static,
    ) -> Self {
        self.options = Some(Rc::new(menu));
        self
    }
}

impl RenderOnce for PanelHeader {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let id = self.id;
        div()
            .id(ElementId::Name(id.clone()))
            .flex()
            .flex_none()
            .items_center()
            .gap_2()
            .h(metrics::panel_header_height())
            .px(metrics::panel_padding())
            .bg(theme.title_bar)
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .debug_selector(|| format!("{id}-title"))
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_base()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.foreground)
                    .child(self.title),
            )
            .when(self.count > 0, |header| {
                header.child(
                    div()
                        .debug_selector(|| format!("{id}-count"))
                        .flex_none()
                        .font_family(theme.mono_font_family.clone())
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child(self.count.to_string()),
                )
            })
            .child(div().flex_1())
            .children(self.actions)
            .when_some(self.options, |header, menu| {
                header.child(
                    div()
                        .debug_selector(|| format!("{id}-options"))
                        .flex_none()
                        .child(
                            Button::new(SharedString::from(format!("{id}-options")))
                                .ghost()
                                .xsmall()
                                .icon(IconName::Ellipsis)
                                .tooltip("Options")
                                .dropdown_menu(move |popup, window, cx| menu(popup, window, cx)),
                        ),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::component::Root;
    use gpui_kit::component::menu::PopupMenuItem;
    use gpui_kit::{
        Context, Modifiers, TestAppContext, VisualTestContext, Window, WindowHandle, div, px, size,
    };

    /// A window showing one header, rebuilt from `build` on every frame.
    struct Host {
        build: Rc<dyn Fn() -> PanelHeader>,
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child((self.build)())
        }
    }

    fn window_with(
        cx: &mut TestAppContext,
        build: impl Fn() -> PanelHeader + 'static,
    ) -> WindowHandle<Root> {
        cx.update(|cx| {
            gpui_kit::init(cx);
            celesta_editor_theme::init(cx);
        });
        let build: Rc<dyn Fn() -> PanelHeader> = Rc::new(build);
        cx.open_window(size(px(400.), px(300.)), |window, cx| {
            let host = cx.new(|_| Host { build });
            Root::new(host, window, cx)
        })
    }

    #[gpui_kit::test]
    fn no_options_means_no_menu_button(cx: &mut TestAppContext) {
        let handle = window_with(cx, || PanelHeader::new("h", "Assets"));
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        cx.run_until_parked();
        assert!(cx.debug_bounds("h-title").is_some());
        assert!(cx.debug_bounds("h-options").is_none());
    }

    #[gpui_kit::test]
    fn a_zero_count_is_not_shown(cx: &mut TestAppContext) {
        let handle = window_with(cx, || PanelHeader::new("h", "Assets").count(0));
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        cx.run_until_parked();
        assert!(cx.debug_bounds("h-count").is_none());
    }

    #[gpui_kit::test]
    fn a_count_is_shown(cx: &mut TestAppContext) {
        let handle = window_with(cx, || PanelHeader::new("h", "Assets").count(3));
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        cx.run_until_parked();
        assert!(cx.debug_bounds("h-count").is_some());
    }

    #[gpui_kit::test]
    fn a_long_title_gives_way_to_the_count_and_the_menu(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            celesta_editor_theme::init(cx);
        });
        let handle = cx.open_window(size(px(160.), px(100.)), |window, cx| {
            let build: Rc<dyn Fn() -> PanelHeader> = Rc::new(|| {
                PanelHeader::new("h", "A panel title far too long for its band")
                    .count(12)
                    .options(|menu, _, _| menu)
            });
            let host = cx.new(|_| Host { build });
            Root::new(host, window, cx)
        });
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        cx.run_until_parked();
        let options = cx.debug_bounds("h-options").expect("the menu button");
        let count = cx.debug_bounds("h-count").expect("the count");
        let title = cx.debug_bounds("h-title").expect("the title");
        assert!(
            options.origin.x + options.size.width <= px(160.),
            "the menu stays in the band"
        );
        assert!(count.origin.x + count.size.width <= options.origin.x);
        assert!(
            title.origin.x + title.size.width <= count.origin.x,
            "the title truncates"
        );
    }

    #[gpui_kit::test]
    fn options_open_and_escape_closes_them(cx: &mut TestAppContext) {
        let handle = window_with(cx, || {
            PanelHeader::new("h", "Assets").options(|menu, _, _| {
                menu.item(PopupMenuItem::element(|_, _| {
                    div()
                        .debug_selector(|| "h-menu-item".into())
                        .child("Reveal")
                }))
            })
        });
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        cx.run_until_parked();
        assert!(cx.debug_bounds("h-menu-item").is_none());

        let button = cx.debug_bounds("h-options").expect("the options button");
        cx.simulate_click(button.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(cx.debug_bounds("h-menu-item").is_some(), "the menu opens");

        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert!(cx.debug_bounds("h-menu-item").is_none(), "escape closes it");
    }
}
