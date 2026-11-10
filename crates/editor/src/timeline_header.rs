//! A timeline row's header: fixed lanes for the track's label, kind,
//! name, state, level and mix buttons, so optional content never moves the
//! rest.

use std::rc::Rc;

use celesta_editor_theme as theme;
use celesta_editor_theme::metrics;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme as _, Icon, Selectable as _, Sizable as _};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, App, ClickEvent, Hsla, SharedString, Window, div, relative, rems};

use crate::widgets::CompactLabel as _;

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// A track's Mute and Solo buttons.
pub(crate) struct MixButtons {
    pub mute_id: SharedString,
    pub solo_id: SharedString,
    pub muted: bool,
    pub solo: bool,
    pub on_mute: ClickHandler,
    pub on_solo: ClickHandler,
}

/// One row's header. See the module documentation.
#[derive(IntoElement)]
pub(crate) struct TrackHeader {
    id: SharedString,
    label: SharedString,
    name: SharedString,
    swatch: Hsla,
    count: Option<usize>,
    disabled: bool,
    locked: bool,
    level: Option<f32>,
    mix: Option<MixButtons>,
    mix_lane: bool,
}

impl TrackHeader {
    pub(crate) fn new(
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        name: impl Into<SharedString>,
        swatch: Hsla,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            name: name.into(),
            swatch,
            count: None,
            disabled: false,
            locked: false,
            level: None,
            mix: None,
            mix_lane: true,
        }
    }

    /// A count after the name (the Chapters row's chapters).
    pub(crate) fn count(mut self, count: usize) -> Self {
        self.count = Some(count);
        self
    }

    /// Disabled in the project: an `EyeOff` in the state lane, a muted name.
    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Locked in the project: a `Lock` in the state lane.
    pub(crate) fn locked(mut self, locked: bool) -> Self {
        self.locked = locked;
        self
    }

    /// The track's level (0..1); `None` leaves the level slot empty.
    pub(crate) fn level(mut self, level: Option<f32>) -> Self {
        self.level = level;
        self
    }

    /// Mute and Solo; `None` leaves their slots empty.
    pub(crate) fn mix(mut self, mix: Option<MixButtons>) -> Self {
        self.mix = mix;
        self
    }

    /// Whether the M/S lane exists at all. A React preview's rows have
    /// none, so its names get the room.
    pub(crate) fn mix_lane(mut self, shown: bool) -> Self {
        self.mix_lane = shown;
        self
    }
}

/// A fixed-width state slot holding `icon` with its tooltip, or nothing.
fn state_slot(
    id: String,
    shown: bool,
    icon: IconName,
    tooltip: &'static str,
    cx: &App,
) -> AnyElement {
    let slot = div().flex_none().w(rems(0.75));
    if !shown {
        return slot.into_any_element();
    }
    slot.id(SharedString::from(id))
        .tooltip(move |window, cx| Tooltip::new(tooltip).build(window, cx))
        .child(
            Icon::new(icon)
                .size_3()
                .text_color(cx.theme().muted_foreground),
        )
        .into_any_element()
}

impl RenderOnce for TrackHeader {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let id = self.id;
        let level_slot = div()
            .debug_selector({
                let id = id.clone();
                move || format!("{id}-level-slot")
            })
            .flex_none()
            .w(rems(0.25))
            .h(rems(1.25))
            .when_some(self.level, |slot, level| {
                slot.overflow_hidden()
                    .flex()
                    .flex_col()
                    .justify_end()
                    .bg(theme.background)
                    .child(div().w_full().h(relative(level)).bg(theme::meter(level)))
            });
        let mix_slot = |button: Option<Button>, selector: String| {
            div()
                .flex_none()
                .size(rems(1.25))
                .when_some(button, |slot, button| {
                    slot.debug_selector(move || selector.clone()).child(button)
                })
        };
        let (mute, solo) = match self.mix {
            Some(mix) => {
                let on_mute = mix.on_mute;
                let on_solo = mix.on_solo;
                (
                    Some(
                        Button::new(mix.mute_id)
                            .xsmall()
                            .ghost()
                            .compact_label("M")
                            .selected(mix.muted)
                            .tooltip("Mute in the preview")
                            .on_click(move |event, window, cx| on_mute(event, window, cx)),
                    ),
                    Some(
                        Button::new(mix.solo_id)
                            .xsmall()
                            .ghost()
                            .compact_label("S")
                            .selected(mix.solo)
                            .tooltip("Solo in the preview")
                            .on_click(move |event, window, cx| on_solo(event, window, cx)),
                    ),
                )
            }
            None => (None, None),
        };
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap_3()
            .w(metrics::track_header_width())
            .h_full()
            .pl(metrics::panel_padding())
            .pr(metrics::panel_padding())
            .border_r_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex_none()
                    .w(rems(2.0))
                    .font_family(theme.mono_font_family.clone())
                    .text_sm()
                    .text_color(muted)
                    .child(self.label),
            )
            .child(div().flex_none().size(rems(0.5)).bg(self.swatch))
            .child(
                div()
                    .debug_selector({
                        let id = id.clone();
                        move || format!("{id}-name")
                    })
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .items_baseline()
                    .gap_1()
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_base()
                            .text_color(if self.disabled {
                                muted
                            } else {
                                theme.foreground
                            })
                            .child(self.name),
                    )
                    .when_some(self.count, |lane, count| {
                        lane.child(
                            div()
                                .flex_none()
                                .font_family(theme.mono_font_family.clone())
                                .text_sm()
                                .text_color(muted)
                                .child(count.to_string()),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .gap_1()
                    .child(state_slot(
                        format!("{id}-disabled"),
                        self.disabled,
                        IconName::EyeOff,
                        "Disabled in the project",
                        cx,
                    ))
                    .child(state_slot(
                        format!("{id}-locked"),
                        self.locked,
                        IconName::Lock,
                        "Locked in the project",
                        cx,
                    )),
            )
            .child(level_slot)
            .when(self.mix_lane, |header| {
                header
                    .child(mix_slot(mute, format!("{id}-mute")))
                    .child(mix_slot(solo, format!("{id}-solo")))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::component::Root;
    use gpui_kit::{
        Context, Hsla, IntoElement, Render, TestAppContext, VisualTestContext, Window, div, px,
        rems, size,
    };

    struct Host {
        rows: Rc<dyn Fn() -> Vec<TrackHeader>>,
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().flex().flex_col().w(px(300.)).children(
                (self.rows)()
                    .into_iter()
                    .map(|row| div().h(rems(3.0)).child(row)),
            )
        }
    }

    fn window_with(
        cx: &mut TestAppContext,
        rows: impl Fn() -> Vec<TrackHeader> + 'static,
    ) -> VisualTestContext {
        cx.update(|cx| {
            gpui_kit::init(cx);
            celesta_editor_theme::init(cx);
        });
        let rows: Rc<dyn Fn() -> Vec<TrackHeader>> = Rc::new(rows);
        let handle = cx.open_window(size(px(400.), px(300.)), |window, cx| {
            let host = cx.new(|_| Host { rows });
            Root::new(host, window, cx)
        });
        let cx = VisualTestContext::from_window(handle.into(), cx);
        cx.run_until_parked();
        cx
    }

    fn mix() -> MixButtons {
        MixButtons {
            mute_id: "track-mute-a".into(),
            solo_id: "track-solo-a".into(),
            muted: true,
            solo: false,
            on_mute: Rc::new(|_, _, _| {}),
            on_solo: Rc::new(|_, _, _| {}),
        }
    }

    #[gpui_kit::test]
    fn lanes_line_up_across_rows(cx: &mut TestAppContext) {
        let cx = &mut window_with(cx, || {
            vec![
                TrackHeader::new("a", "V2", "Titles", Hsla::red())
                    .disabled(true)
                    .locked(true)
                    .level(Some(0.5))
                    .mix(Some(mix())),
                TrackHeader::new("b", "A1", "Music", Hsla::green())
                    .level(None)
                    .mix(None),
            ]
        });
        let a = cx.debug_bounds("a-name").expect("a's name");
        let b = cx.debug_bounds("b-name").expect("b's name");
        assert_eq!(a.origin.x, b.origin.x);
        assert_eq!(a.size.width, b.size.width, "empty slots keep the name lane");
        assert!(cx.debug_bounds("a-mute").is_some());
        assert!(cx.debug_bounds("b-mute").is_none());
    }

    #[gpui_kit::test]
    fn without_a_mix_lane_the_name_widens(cx: &mut TestAppContext) {
        let cx = &mut window_with(cx, || {
            vec![
                TrackHeader::new("a", "V2", "Titles", Hsla::red()).mix(None),
                TrackHeader::new("b", "V1", "Composition", Hsla::blue()).mix_lane(false),
            ]
        });
        let a = cx.debug_bounds("a-name").expect("a's name");
        let b = cx.debug_bounds("b-name").expect("b's name");
        assert!(b.size.width > a.size.width);
    }
}
