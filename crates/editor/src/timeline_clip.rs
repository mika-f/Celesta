//! A clip on the timeline: a name strip along its top in the kind's
//! darker color, and the body below in the kind's color, holding an audio
//! clip's waveform.

use std::rc::Rc;

use celesta_editor_theme as theme;
use celesta_editor_theme::metrics;
use celesta_project::TrackKind;
use gpui_kit::prelude::*;
use gpui_kit::{
    App, Bounds, ClickEvent, ElementId, Hsla, Pixels, SharedString, Window, canvas, div, fill,
    point, px, relative, rems, rgb, size,
};

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// One clip, filling its parent; the caller places and sizes it on the
/// lane. See the module documentation.
#[derive(IntoElement)]
pub(crate) struct ClipBlock {
    id: ElementId,
    name: SharedString,
    kind: TrackKind,
    label_offset: f32,
    selected: bool,
    enabled: bool,
    waveform: Vec<f32>,
    on_click: Option<ClickHandler>,
}

impl ClipBlock {
    pub(crate) fn new(
        id: impl Into<ElementId>,
        name: impl Into<SharedString>,
        kind: TrackKind,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind,
            label_offset: 0.0,
            selected: false,
            enabled: true,
            waveform: Vec::new(),
            on_click: None,
        }
    }

    /// Where the name starts, as a fraction of the clip: past the part
    /// left of the view, so the name stays readable.
    pub(crate) fn label_offset(mut self, fraction: f32) -> Self {
        self.label_offset = fraction;
        self
    }

    pub(crate) fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub(crate) fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Peak bars (0..1) drawn in the body.
    pub(crate) fn waveform(mut self, bars: Vec<f32>) -> Self {
        self.waveform = bars;
        self
    }

    pub(crate) fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for ClipBlock {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let selector = format!("{}-label", self.id);
        let ink = theme::waveform();
        let bars = self.waveform;
        div()
            .id(self.id)
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(rgb(theme::clip_fill(self.kind)))
            .when(!self.enabled, |block| block.opacity(0.4))
            .child(
                div()
                    .debug_selector(move || selector.clone())
                    .relative()
                    .flex_none()
                    .w_full()
                    .h(metrics::clip_label_height())
                    .overflow_hidden()
                    .bg(theme::clip_label_fill(self.kind))
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left(relative(self.label_offset))
                            .flex()
                            .items_center()
                            .px_1()
                            .text_sm()
                            .text_color(ink)
                            .whitespace_nowrap()
                            .child(self.name),
                    ),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .when(!bars.is_empty(), |body| {
                        body.child(
                            canvas(
                                |_, _, _| (),
                                move |bounds, _, window, _| {
                                    paint_waveform(bounds, &bars, ink.opacity(0.45), window)
                                },
                            )
                            .absolute()
                            .top(rems(0.125))
                            .bottom(rems(0.125))
                            .left_0()
                            .right_0(),
                        )
                    }),
            )
            // Over the strip and the body alike.
            .when(self.selected, |block| {
                block.child(
                    div()
                        .absolute()
                        .inset_0()
                        .border_2()
                        .border_color(theme::clip_selected_border()),
                )
            })
            .when_some(self.on_click, |block, on_click| {
                block.on_click(move |event, window, cx| on_click(event, window, cx))
            })
    }
}

/// Mirrored peak bars centered on the clip's midline.
fn paint_waveform(bounds: Bounds<Pixels>, bars: &[f32], color: Hsla, window: &mut Window) {
    let count = bars.len().max(1) as f32;
    let step = bounds.size.width / count;
    let bar_width = (step - px(1.0)).max(px(1.0)); // physical: one-pixel gaps
    let middle = bounds.origin.y + bounds.size.height / 2.0;
    for (index, amplitude) in bars.iter().enumerate() {
        let height = (bounds.size.height * amplitude.clamp(0.02, 1.0)).max(px(1.0)); // physical: at least a pixel
        window.paint_quad(fill(
            Bounds::new(
                point(bounds.origin.x + step * index as f32, middle - height / 2.0),
                size(bar_width, height),
            ),
            color,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_editor_theme::metrics::BASE_FONT_SIZE;
    use gpui_kit::component::Root;
    use gpui_kit::{
        Context, IntoElement, Render, TestAppContext, VisualTestContext, Window, div, px, size,
    };

    struct Host {
        width: f32,
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().relative().w(px(self.width)).h(px(40.)).child(
                ClipBlock::new(
                    "c",
                    "a clip name far longer than the clip",
                    TrackKind::Audio,
                )
                .waveform(vec![0.2, 0.8, 0.5]),
            )
        }
    }

    fn window_with(cx: &mut TestAppContext, width: f32) -> VisualTestContext {
        cx.update(|cx| {
            gpui_kit::init(cx);
            celesta_editor_theme::init(cx);
        });
        let handle = cx.open_window(size(px(400.), px(200.)), |window, cx| {
            let host = cx.new(|_| Host { width });
            Root::new(host, window, cx)
        });
        let cx = VisualTestContext::from_window(handle.into(), cx);
        cx.run_until_parked();
        cx
    }

    #[gpui_kit::test]
    fn the_name_strip_is_its_token_tall(cx: &mut TestAppContext) {
        let cx = &mut window_with(cx, 300.);
        let strip = cx.debug_bounds("c-label").expect("the name strip");
        // Layout rounds to whole pixels.
        let token = metrics::clip_label_height().0 * BASE_FONT_SIZE;
        assert!(
            (f32::from(strip.size.height) - token).abs() <= 0.5,
            "{:?} for {token}",
            strip.size.height
        );
    }

    #[gpui_kit::test]
    fn a_narrow_clip_clips_its_name(cx: &mut TestAppContext) {
        let cx = &mut window_with(cx, 40.);
        let strip = cx.debug_bounds("c-label").expect("the name strip");
        assert!(strip.size.width <= px(40.), "{:?}", strip.size.width);
    }
}
