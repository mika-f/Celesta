//! The editor's toasts: gpui-kit `Notification`s with one look.
//!
//! gpui-kit 0.9 places a typed notification's icon absolutely, 18px from
//! its top, beside a text column, and gives the surface the `border` token,
//! one step from the popover fill. At the editor's 13px `rem` and on its
//! dark greys that leaves a title-only toast's icon below its title and the
//! toast without a visible edge, so it reads as part of the panels it
//! covers. The editor builds the body itself instead: the icon in the
//! title's row, the message and an action indented under the title, and
//! the popover family's hairline (the foreground at a tenth, the ink
//! gpui-kit gives menus and popovers) as the edge.
//!
//! gpui-kit's own shadow stays: the toast's enter animation sets it on
//! every frame, so a style set here cannot replace it.

use gpui_kit::component::button::Button;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, ClickEvent, FontWeight, Hsla, Rems, SharedString, Window, div, rems,
};
use std::rc::Rc;

use crate::widgets::CompactLabel as _;

/// How much of the foreground shows as the toast's edge: the ink gpui-kit
/// gives the hairline ring of its menus and popovers.
const EDGE_INK: f32 = 0.1;

/// The line height of a toast's first line, and the height of the icon's
/// row beside it, so the icon centers on that line.
fn line_height() -> Rems {
    rems(1.25)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToastKind {
    /// A note about what the user just tried, such as an action that has
    /// to wait.
    Info,
    /// A finished operation.
    Success,
    Warning,
    Error,
}

impl ToastKind {
    fn icon(self, cx: &App) -> Icon {
        let theme = cx.theme();
        let (name, color): (IconName, Hsla) = match self {
            Self::Info => (IconName::Info, theme.info),
            Self::Success => (IconName::CircleCheck, theme.success),
            Self::Warning => (IconName::TriangleAlert, theme.warning),
            Self::Error => (IconName::CircleX, theme.danger),
        };
        Icon::new(name).text_color(color)
    }
}

type OnAction = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// A toast's content, built into a [`Notification`] by [`Toast::build`].
pub(crate) struct Toast {
    kind: ToastKind,
    title: SharedString,
    message: SharedString,
    action: Option<(SharedString, OnAction)>,
}

impl Toast {
    /// A toast with a `title`, such as "Export finished".
    pub(crate) fn new(kind: ToastKind, title: impl Into<SharedString>) -> Self {
        Self {
            kind,
            title: title.into(),
            message: SharedString::default(),
            action: None,
        }
    }

    /// A one-sentence note with no title, such as "Cancel the export
    /// before opening another file."
    pub(crate) fn note(message: impl Into<SharedString>, cx: &App) -> Notification {
        Self::new(ToastKind::Info, SharedString::default())
            .message(message)
            .build(cx)
    }

    /// The detail under the title; an empty one is left out.
    pub(crate) fn message(mut self, message: impl Into<SharedString>) -> Self {
        self.message = message.into();
        self
    }

    /// A button under the message. A toast with an action stays until it
    /// is closed.
    pub(crate) fn action(
        mut self,
        label: impl Into<SharedString>,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.action = Some((label.into(), Rc::new(on_click)));
        self
    }

    /// The notification, edged in the popover family's hairline.
    pub(crate) fn build(self, cx: &App) -> Notification {
        let stays = self.action.is_some();
        let toast = Rc::new(self);
        Notification::new()
            .autohide(!stays)
            .border_color(edge(cx))
            .content(move |_, _, cx| toast.body(cx))
    }

    fn body(&self, cx: &App) -> AnyElement {
        let theme = cx.theme();
        let has_title = !self.title.is_empty();
        let has_message = !self.message.is_empty();
        let first_line = if has_title {
            div()
                .text_base()
                .font_weight(FontWeight::MEDIUM)
                .debug_selector(|| "toast-title".into())
                .child(self.title.clone())
        } else {
            div()
                .text_base()
                .debug_selector(|| "toast-title".into())
                .child(self.message.clone())
        };
        v_flex()
            .gap_1()
            .child(
                h_flex()
                    .items_start()
                    .gap_2()
                    // Room for gpui-kit's close button, shown on hover in the
                    // top right corner.
                    .pr_4()
                    .child(
                        // As tall as the first line, so the icon centers on it.
                        h_flex()
                            .flex_none()
                            .h(line_height())
                            .debug_selector(|| "toast-icon".into())
                            .child(self.kind.icon(cx)),
                    )
                    .child(first_line.min_w_0().line_height(line_height())),
            )
            .when(has_title && has_message, |this| {
                this.child(
                    div().pl_6().child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .debug_selector(|| "toast-message".into())
                            .child(self.message.clone()),
                    ),
                )
            })
            .when_some(self.action.clone(), |this, (label, on_click)| {
                this.child(
                    h_flex().pl_6().pt_1().child(
                        div().debug_selector(|| "toast-action".into()).child(
                            Button::new("toast-action")
                                .xsmall()
                                .outline()
                                .compact_label(label)
                                .on_click(move |event, window, cx| on_click(event, window, cx)),
                        ),
                    ),
                )
            })
            .into_any_element()
    }
}

/// The toast's edge: the foreground at [`EDGE_INK`].
fn edge(cx: &App) -> Hsla {
    cx.theme().foreground.alpha(EDGE_INK)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::component::{Root, WindowExt as _};
    use gpui_kit::{
        Bounds, Context, Pixels, TestAppContext, VisualTestContext, WindowHandle, px, size,
    };
    use std::time::Duration;

    struct Empty;

    impl Render for Empty {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full()
        }
    }

    fn window(cx: &mut TestAppContext) -> WindowHandle<Root> {
        cx.update(|cx| {
            gpui_kit::init(cx);
            celesta_editor_theme::init(cx);
        });
        cx.open_window(size(px(1440.), px(900.)), |window, cx| {
            let view = cx.new(|_| Empty);
            Root::new(view, window, cx)
        })
    }

    fn show(
        cx: &mut TestAppContext,
        build: impl FnOnce(&App) -> Notification,
    ) -> VisualTestContext {
        let handle = window(cx);
        let mut cx = VisualTestContext::from_window(handle.into(), cx);
        cx.update(|window, cx| {
            let note = build(cx);
            window.push_notification(note, cx);
        });
        cx.run_until_parked();
        cx
    }

    /// The icon's row is one line tall and level with the first line, so
    /// the icon (centered in its row) centers on that line.
    fn assert_icon_on_first_line(icon: Bounds<Pixels>, line: Bounds<Pixels>) {
        let one_line = line_height().to_pixels(px(celesta_editor_theme::metrics::BASE_FONT_SIZE));
        assert_eq!(icon.origin.y, line.origin.y, "icon {icon:?}, line {line:?}");
        // Layout snaps to whole device pixels.
        assert!(
            (f32::from(icon.size.height) - f32::from(one_line)).abs() <= 0.5,
            "one line tall: {icon:?}"
        );
        assert!(icon.origin.x + icon.size.width <= line.origin.x);
    }

    #[gpui_kit::test]
    fn a_title_only_toast_centers_its_icon_on_the_title(cx: &mut TestAppContext) {
        let mut cx = show(cx, |cx| {
            Toast::new(ToastKind::Success, "Export finished").build(cx)
        });
        let icon = cx.debug_bounds("toast-icon").expect("the icon");
        let title = cx.debug_bounds("toast-title").expect("the title");
        assert_icon_on_first_line(icon, title);
        assert_eq!(title.size.height, icon.size.height, "one line");
        assert!(cx.debug_bounds("toast-message").is_none());
    }

    #[gpui_kit::test]
    fn the_message_sits_under_the_title(cx: &mut TestAppContext) {
        let mut cx = show(cx, |cx| {
            Toast::new(ToastKind::Warning, "Some plugins were skipped")
                .message("celesta-codec-avfoundation needs a newer Celesta.")
                .build(cx)
        });
        let title = cx.debug_bounds("toast-title").expect("the title");
        let message = cx.debug_bounds("toast-message").expect("the message");
        assert_eq!(message.origin.x, title.origin.x, "aligned with the title");
        assert!(message.origin.y >= title.origin.y + title.size.height);
    }

    #[gpui_kit::test]
    fn an_action_takes_its_own_row_and_the_toast_stays(cx: &mut TestAppContext) {
        let mut cx = show(cx, |cx| {
            Toast::new(ToastKind::Error, "Couldn’t load component schemas")
                .message("schema.json: unexpected end of input at line 12, column 4.")
                .action("Show in Problems", |_, _, _| {})
                .build(cx)
        });
        let title = cx.debug_bounds("toast-title").expect("the title");
        let message = cx.debug_bounds("toast-message").expect("the message");
        let action = cx.debug_bounds("toast-action").expect("the action");
        assert!(
            title.size.height < px(30.),
            "the title keeps one line: {title:?}"
        );
        assert!(action.origin.y >= message.origin.y + message.size.height);
        assert_eq!(action.origin.x, message.origin.x, "aligned with the text");
        cx.executor().advance_clock(Duration::from_secs(10));
        cx.run_until_parked();
        assert!(cx.debug_bounds("toast-title").is_some(), "still shown");
    }

    #[gpui_kit::test]
    fn a_note_starts_beside_its_icon(cx: &mut TestAppContext) {
        let mut cx = show(cx, |cx| {
            Toast::note("Cancel the export before opening another file.", cx)
        });
        let icon = cx.debug_bounds("toast-icon").expect("the icon");
        let line = cx.debug_bounds("toast-title").expect("the note");
        assert_icon_on_first_line(icon, line);
        assert!(cx.debug_bounds("toast-message").is_none());
    }

    #[gpui_kit::test]
    fn a_toast_without_an_action_fades(cx: &mut TestAppContext) {
        let mut cx = show(cx, |cx| {
            Toast::new(ToastKind::Success, "Export finished").build(cx)
        });
        assert!(cx.debug_bounds("toast-title").is_some());
        cx.executor().advance_clock(Duration::from_secs(10));
        cx.run_until_parked();
        assert!(cx.debug_bounds("toast-title").is_none(), "autohidden");
    }
}
