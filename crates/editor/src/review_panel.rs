//! The Comments tab, the comment marks on the timeline ruler, and the pins
//! on the canvas: where reviewers leave notes on the open entry and agents
//! pick them up. The comments themselves are [`crate::review::Review`]'s.

use std::time::Duration;

use celesta_project::review::{Placement, Point};
use celesta_react_bridge::ReactChapter;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Escape, InputEvent, Textarea, TextareaState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Selectable as _, Sizable as _,
};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Bounds, ClickEvent, Context, Entity, Focusable as _, KeyDownEvent,
    MouseButton, MouseDownEvent, Pixels, SharedString, Subscription, Window, div, px, relative,
    rems,
};

use crate::layer_select::letterbox;
use crate::property_controls::field_keeps_key;
use crate::review::{Listed, Shown, folder_stamp, mark_span, pins_at};
use crate::timecode::format_timecode;
use crate::view::{EditorView, RightTab};
use celesta_editor_theme::metrics;

/// How often the comments folder is checked for other programs' writes.
const COMMENTS_POLL: Duration = Duration::from_millis(1500);

/// The Comments tab's text fields: the new comment and the reply to the
/// selected one. Made once the window is, and kept across reloads.
pub(crate) struct ReviewInputs {
    composer: Entity<TextareaState>,
    reply: Entity<TextareaState>,
    _subscriptions: [Subscription; 2],
}

impl ReviewInputs {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<EditorView>) -> Self {
        let text_field =
            |placeholder: &'static str, window: &mut Window, cx: &mut Context<EditorView>| {
                cx.new(|cx| {
                    TextareaState::new(window, cx)
                        .submit_on_enter(true)
                        .auto_grow(2, 8)
                        .placeholder(placeholder)
                })
            };
        let composer = text_field("What should change here?", window, cx);
        let reply = text_field("Reply", window, cx);
        let on_composer = cx.subscribe_in(
            &composer,
            window,
            |this, _, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { shift: false, .. } = event {
                    this.submit_comment(window, cx);
                }
            },
        );
        let on_reply =
            cx.subscribe_in(&reply, window, |this, _, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { shift: false, .. } = event {
                    this.submit_reply(window, cx);
                }
            });
        Self {
            composer,
            reply,
            _subscriptions: [on_composer, on_reply],
        }
    }

    fn focus_composer(&self, window: &mut Window, cx: &mut App) {
        self.composer.read(cx).focus_handle(cx).focus(window, cx);
    }
}

/// Keeps the keys a text field moves its caret with from the editor's
/// shortcuts, and sends Escape to `on_escape`.
fn field(
    input: impl IntoElement,
    cx: &mut Context<EditorView>,
    on_escape: impl Fn(&mut EditorView, &mut Window, &mut Context<EditorView>) + 'static,
) -> impl IntoElement {
    div()
        .w_full()
        .on_action(cx.listener(move |this, _: &Escape, window, cx| on_escape(this, window, cx)))
        .on_key_down(|event: &KeyDownEvent, _, cx| {
            if field_keeps_key(&event.keystroke) {
                cx.stop_propagation();
            }
        })
        .child(input)
}

impl EditorView {
    /// The composition comments are placed on.
    pub(crate) fn review_shown(&self) -> Shown<'_> {
        match &self.react_preview {
            Some(react) => Shown {
                composition: Some(&react.composition_id),
                first: react
                    .compositions
                    .first()
                    .is_none_or(|first| first.id == react.composition_id),
                chapters: &react.chapters,
            },
            None => Shown {
                composition: None,
                first: true,
                chapters: &[],
            },
        }
    }

    fn several_roots(&self) -> bool {
        self.react_preview
            .as_ref()
            .is_some_and(|react| react.compositions.len() > 1)
    }

    fn chapter_list(&self) -> &[ReactChapter] {
        self.react_preview
            .as_ref()
            .map_or(&[][..], |react| react.chapters.as_slice())
    }

    pub(crate) fn add_comment_action(
        &mut self,
        _: &crate::actions::AddComment,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.review.folder().is_some() {
            self.begin_comment(window, cx);
        }
    }

    /// Opens the composer at the playhead (or the In/Out span) on the
    /// selected layer, in the Comments tab.
    pub(crate) fn begin_comment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let frame = u64::try_from(self.clock.frame()).unwrap_or(0);
        let marks = self
            .export_in_frame
            .zip(self.export_out_frame)
            .and_then(|(start, end)| Some((u64::try_from(start).ok()?, u64::try_from(end).ok()?)));
        let layer = self.selected_layer.clone();
        let chapters = self.chapter_list().to_vec();
        self.review.begin(frame, marks, layer, &chapters);
        self.right_tab = RightTab::Comments;
        if let Some(inputs) = &self.review_inputs {
            inputs
                .composer
                .update(cx, |input, cx| input.set_value("", window, cx));
            inputs.focus_composer(window, cx);
        }
        cx.notify();
    }

    fn cancel_comment(&mut self, cx: &mut Context<Self>) {
        self.review.cancel();
        cx.notify();
    }

    pub(crate) fn submit_comment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(inputs) = &self.review_inputs else {
            return;
        };
        let body = inputs.composer.read(cx).value();
        if body.trim().is_empty() {
            return;
        }
        let composer = inputs.composer.clone();
        let several = self.several_roots();
        let (chapters, composition, first) = self.owned_shown();
        let written = self.review.add(
            &body,
            Shown {
                composition: composition.as_deref(),
                first,
                chapters: &chapters,
            },
            several,
        );
        if written.is_ok() {
            composer.update(cx, |input, cx| input.set_value("", window, cx));
        }
        cx.notify();
    }

    pub(crate) fn submit_reply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(inputs), Some(id)) = (&self.review_inputs, self.review.selected.clone()) else {
            return;
        };
        let body = inputs.reply.read(cx).value();
        if body.trim().is_empty() {
            return;
        }
        let reply = inputs.reply.clone();
        let (chapters, composition, first) = self.owned_shown();
        let written = self.review.reply(
            &id,
            &body,
            Shown {
                composition: composition.as_deref(),
                first,
                chapters: &chapters,
            },
        );
        if written.is_ok() {
            reply.update(cx, |input, cx| input.set_value("", window, cx));
        }
        cx.notify();
    }

    fn set_comment_resolved(&mut self, id: &str, resolved: bool, cx: &mut Context<Self>) {
        let (chapters, composition, first) = self.owned_shown();
        let _ = self.review.set_resolved(
            id,
            resolved,
            Shown {
                composition: composition.as_deref(),
                first,
                chapters: &chapters,
            },
        );
        cx.notify();
    }

    /// [`Self::review_shown`] as owned values, for a write to the review
    /// beside the preview it borrows.
    fn owned_shown(&self) -> (Vec<ReactChapter>, Option<String>, bool) {
        let shown = self.review_shown();
        (
            shown.chapters.to_vec(),
            shown.composition.map(str::to_owned),
            shown.first,
        )
    }

    /// Selects comment `id`, seeks to it, and selects its layer; clicking
    /// the selected one again lets it go.
    pub(crate) fn select_comment(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.review.selected.as_deref() == Some(id) {
            self.review.selected = None;
            cx.notify();
            return;
        }
        let listed = self.review.listed(self.review_shown());
        let Some(listed) = listed.into_iter().find(|listed| listed.id() == id) else {
            return;
        };
        self.review.selected = Some(id.to_owned());
        self.right_tab = RightTab::Comments;
        if let Some(inputs) = &self.review_inputs {
            inputs
                .reply
                .update(cx, |input, cx| input.set_value("", window, cx));
        }
        if let Some(frame) = listed.placement.frame() {
            self.seek_to_frame(frame, cx);
        }
        if self.is_react_preview()
            && let Some(layer) = listed.comment.anchor.layer.clone()
        {
            self.change_selection(Some(layer), cx);
        }
        cx.notify();
    }

    fn toggle_pinning(&mut self, cx: &mut Context<Self>) {
        if let Some(draft) = &mut self.review.draft {
            draft.pinning = !draft.pinning;
        }
        cx.notify();
    }

    fn remove_pin(&mut self, cx: &mut Context<Self>) {
        if let Some(draft) = &mut self.review.draft {
            draft.anchor.point = None;
        }
        cx.notify();
    }

    /// Whether a press on the canvas places the new comment's point.
    pub(crate) fn pinning_comment(&self) -> bool {
        self.review
            .draft
            .as_ref()
            .is_some_and(|draft| draft.pinning)
    }

    /// A press on the canvas while pinning: the point under it, when it is
    /// on the frame.
    pub(crate) fn press_pin(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(bounds) = self.viewer_bounds.get() else {
            return;
        };
        let settings = &self.document.project().settings;
        let frame_size = (settings.width as f32, settings.height as f32);
        if let Some(point) = canvas_fraction(bounds, frame_size, event.position) {
            self.review.pin(point);
            if let Some(inputs) = &self.review_inputs {
                inputs.focus_composer(window, cx);
            }
        }
        cx.stop_propagation();
        cx.notify();
    }

    /// Checks the comments folder every [`COMMENTS_POLL`] and reloads it
    /// when another program (an agent, git) changed it.
    pub(crate) fn watch_comments(&self, cx: &mut Context<Self>) {
        let session = self.session;
        cx.spawn(async move |view, cx| {
            loop {
                cx.background_executor().timer(COMMENTS_POLL).await;
                let Ok(Some((folder, stamp))) = view.update(cx, |this, _| {
                    (this.session == session)
                        .then(|| Some((this.review.folder()?, this.review.stamp.clone())))
                        .flatten()
                }) else {
                    break;
                };
                let latest = cx
                    .background_executor()
                    .spawn(async move { folder_stamp(&folder) })
                    .await;
                if latest != stamp
                    && view
                        .update(cx, |this, cx| {
                            this.review.reload();
                            cx.notify();
                        })
                        .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    /// The Comments tab: the composer, then the comments in creation order;
    /// the selected one opens its thread in place.
    pub(crate) fn comments_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let shown = self.review_shown();
        let listed = self.review.listed(shown);
        let can_write = self.review.folder().is_some();
        let show_resolved = self.review.show_resolved;
        let header = div()
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                Button::new("comment-add")
                    .small()
                    .icon(IconName::MessageSquarePlus)
                    .label("Add comment…")
                    .disabled(!can_write || self.review.draft.is_some())
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.begin_comment(window, cx);
                    })),
            )
            .child(
                Switch::new("comments-show-resolved")
                    .small()
                    .checked(show_resolved)
                    .label("Show resolved")
                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                        this.review.show_resolved = *checked;
                        cx.notify();
                    })),
            );
        let empty = listed.is_empty() && self.review.draft.is_none();
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(header)
            .child(
                div()
                    .id("comments-scroll")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .overflow_x_hidden()
                    .overflow_y_scroll()
                    .children(self.review.draft.is_some().then(|| self.composer(cx)))
                    .children(self.review.error.clone().map(|error| {
                        div()
                            .px_3()
                            .py_2()
                            .text_sm()
                            .text_color(cx.theme().danger)
                            .child(format!("Couldn’t save the comment: {error}"))
                    }))
                    .when(empty, |panel| {
                        panel.child(div().px_3().py_3().text_sm().text_color(muted).child(
                            if !can_write {
                                "Open a project to comment on it"
                            } else if show_resolved {
                                "No comments"
                            } else {
                                "No open comments"
                            },
                        ))
                    })
                    .children(listed.iter().map(|listed| self.comment_row(listed, cx)))
                    .children(self.review.problems.iter().map(|problem| {
                        div()
                            .px_3()
                            .py_1()
                            .text_sm()
                            .text_color(cx.theme().warning)
                            .child(format!("Skipped {problem}"))
                    })),
            )
    }

    /// Where a comment points, in a line: its chapter and timecode, and its
    /// layer.
    fn anchor_line(&self, placement: &Placement, layer: Option<&str>) -> String {
        let rate = self.frame_rate_value;
        let timecode = |frame: u64| format_timecode(i64::try_from(frame).unwrap_or(0), rate);
        let mut line = match placement {
            Placement::Placed {
                from,
                to,
                chapter_name,
            } => {
                let time = match to {
                    Some(to) => format!("{} – {}", timecode(*from), timecode(*to)),
                    None => timecode(*from),
                };
                match chapter_name {
                    Some(name) => format!("{name}  {time}"),
                    None => time,
                }
            }
            Placement::Orphaned { last_seen } => match last_seen {
                Some(seen) => format!("Was at {}", timecode(seen.frame)),
                None => "Not placed".to_owned(),
            },
        };
        if let Some(layer) = layer {
            line.push_str("  ·  ");
            line.push_str(layer);
        }
        line
    }

    fn composer(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(draft) = &self.review.draft else {
            return div().into_any_element();
        };
        let muted = cx.theme().muted_foreground;
        let shown = self.review_shown();
        let placement = draft.anchor.place(&spans(shown.chapters));
        let anchor = self.anchor_line(&placement, draft.anchor.layer.as_deref());
        let pinned = draft
            .anchor
            .point
            .map(|point| format!("Pinned at {:.0}%, {:.0}%", point.x * 100.0, point.y * 100.0));
        let pinning = draft.pinning;
        div()
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .py_3()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex()
                    .justify_between()
                    .gap_2()
                    .text_sm()
                    .text_color(muted)
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .font_family(cx.theme().mono_font_family.clone())
                            .child(anchor),
                    )
                    .child(
                        div()
                            .flex_none()
                            .child(format!("As {}", self.review.author())),
                    ),
            )
            .children(self.review_inputs.as_ref().map(|inputs| {
                field(Textarea::new(&inputs.composer), cx, |this, _, cx| {
                    this.cancel_comment(cx);
                })
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Button::new("comment-pin")
                                    .small()
                                    .ghost()
                                    .icon(IconName::MapPin)
                                    .label(if pinning { "Click the canvas" } else { "Pin" })
                                    .selected(pinning)
                                    .tooltip("Place this comment on a point of the canvas")
                                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                        this.toggle_pinning(cx);
                                    })),
                            )
                            .children(pinned.map(|pinned| {
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .text_sm()
                                    .text_color(muted)
                                    .child(pinned)
                                    .child(
                                        Button::new("comment-unpin")
                                            .xsmall()
                                            .ghost()
                                            .icon(IconName::Close)
                                            .accessibility_label("Remove pin")
                                            .tooltip("Remove pin")
                                            .on_click(cx.listener(
                                                |this, _: &ClickEvent, _, cx| {
                                                    this.remove_pin(cx);
                                                },
                                            )),
                                    )
                            })),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                Button::new("comment-cancel")
                                    .small()
                                    .ghost()
                                    .label("Cancel")
                                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                        this.cancel_comment(cx);
                                    })),
                            )
                            .child(
                                Button::new("comment-submit")
                                    .small()
                                    .primary()
                                    .label("Comment")
                                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                        this.submit_comment(window, cx);
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// One comment: its number, where it points, who wrote it and what;
    /// the selected one also shows its replies, the reply field, and
    /// Resolve.
    fn comment_row(&self, listed: &Listed, cx: &mut Context<Self>) -> AnyElement {
        let comment = &listed.comment;
        let id: SharedString = comment.id.clone().into();
        let selected = self.review.selected.as_deref() == Some(comment.id.as_str());
        let muted = cx.theme().muted_foreground;
        let mono = cx.theme().mono_font_family.clone();
        let open = comment.is_open();
        let anchor = self.anchor_line(&listed.placement, comment.anchor.layer.as_deref());
        let orphan = match &listed.placement {
            Placement::Orphaned { .. } => Some(format!(
                "Scene \"{}\" no longer exists",
                comment.anchor.chapter.as_deref().unwrap_or_default()
            )),
            Placement::Placed { .. } => None,
        };
        let replies = comment.replies.len();
        let target = comment.id.clone();
        let summary = div()
            .id(SharedString::from(format!("comment-{id}")))
            .flex()
            .gap_2()
            .px_3()
            .py_2()
            .when(selected, |row| row.bg(cx.theme().list_active))
            .when(!selected, |row| {
                row.hover(|row| row.bg(cx.theme().list_hover))
            })
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.select_comment(&target, window, cx);
            }))
            .child(number_square(listed.number, open, selected, cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap_0p5()
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .gap_2()
                            .text_sm()
                            .child(
                                div()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .font_family(mono.clone())
                                    .text_color(muted)
                                    .child(anchor),
                            )
                            .child(div().flex_none().text_color(muted).child(if open {
                                ""
                            } else {
                                "Resolved"
                            })),
                    )
                    .children(orphan.map(|orphan| {
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .text_sm()
                            .text_color(cx.theme().warning)
                            .child(Icon::new(IconName::TriangleAlert).xsmall())
                            .child(orphan)
                    }))
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().foreground)
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(
                                        div()
                                            .font_weight(gpui_kit::FontWeight::MEDIUM)
                                            .child(comment.author.clone()),
                                    )
                                    .when(replies > 0 && !selected, |line| {
                                        line.child(div().text_color(muted).child(if replies == 1 {
                                            "1 reply".to_owned()
                                        } else {
                                            format!("{replies} replies")
                                        }))
                                    }),
                            )
                            .child(
                                div()
                                    .when(!selected, |body| body.line_clamp(2))
                                    .child(comment.body.clone()),
                            ),
                    ),
            );
        div()
            .flex()
            .flex_col()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(summary)
            .when(selected, |row| row.child(self.thread(listed, cx)))
            .into_any_element()
    }

    /// The selected comment's replies, the reply field, and Resolve or
    /// Reopen.
    fn thread(&self, listed: &Listed, cx: &mut Context<Self>) -> impl IntoElement {
        let comment = &listed.comment;
        let muted = cx.theme().muted_foreground;
        let open = comment.is_open();
        let id = comment.id.clone();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .pl(rems(2.75))
            .pr_3()
            .pb_3()
            .bg(cx.theme().list_active)
            .children(comment.replies.iter().map(|reply| {
                div()
                    .flex()
                    .flex_col()
                    .text_sm()
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                div()
                                    .font_weight(gpui_kit::FontWeight::MEDIUM)
                                    .child(reply.author.clone()),
                            )
                            .child(div().text_color(muted).child(short_time(&reply.created_at))),
                    )
                    .child(div().child(reply.body.clone()))
            }))
            .children(comment.resolved.as_ref().map(|resolved| {
                div().text_sm().text_color(muted).child(format!(
                    "Resolved by {}  {}",
                    resolved.by,
                    short_time(&resolved.at)
                ))
            }))
            .children(self.review_inputs.as_ref().map(|inputs| {
                field(Textarea::new(&inputs.reply), cx, |this, _, cx| {
                    this.review.selected = None;
                    cx.notify();
                })
            }))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("comment-resolve")
                            .small()
                            .when(open, |button| button.icon(IconName::Check).label("Resolve"))
                            .when(!open, |button| {
                                button.icon(IconName::RotateCcw).label("Reopen")
                            })
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.set_comment_resolved(&id, open, cx);
                            })),
                    )
                    .child(
                        Button::new("comment-reply")
                            .small()
                            .primary()
                            .label("Reply")
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.submit_reply(window, cx);
                            })),
                    ),
            )
    }

    /// The comment marks over the ruler: a square at each comment's first
    /// frame and, for a span, a line to its end. A click selects it.
    pub(crate) fn comment_marks(
        &self,
        view: crate::timeline::LaneView,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let listed = self.review.listed(self.review_shown());
        let end_frame = u64::try_from(self.clock.end_frame()).unwrap_or(0);
        let selected = self.review.selected.clone();
        listed
            .iter()
            .filter_map(|listed| {
                let (start, end) = mark_span(listed, end_frame)?;
                let is_selected = selected.as_deref() == Some(listed.id());
                let color = mark_color(listed.comment.is_open(), is_selected, cx);
                let target = listed.id().to_owned();
                let body = listed.comment.body.clone();
                let x = view.x(start);
                let width = view.width(end - start);
                Some(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(relative(x))
                        .w(relative(width.max(0.0)))
                        .child(
                            div()
                                .absolute()
                                .left_0()
                                .right_0()
                                .bottom(rems(0.375))
                                .h(px(2.0)) // physical: a span's line
                                .bg(color),
                        )
                        .child(
                            div()
                                .id(SharedString::from(format!("comment-mark-{target}")))
                                .absolute()
                                .left(rems(-0.5625))
                                .bottom(rems(0.125))
                                .size(rems(1.125))
                                .flex()
                                .items_center()
                                .justify_center()
                                .bg(color)
                                .text_color(cx.theme().background)
                                .text_sm()
                                .font_family(cx.theme().mono_font_family.clone())
                                .child(listed.number.to_string())
                                .tooltip(move |window, cx| {
                                    Tooltip::new(body.clone()).build(window, cx)
                                })
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                                        cx.stop_propagation();
                                        this.select_comment(&target, window, cx);
                                    }),
                                ),
                        )
                        .into_any_element(),
                )
            })
            .collect()
    }

    /// The pins of the comments on the canvas at the playhead, placed by
    /// the viewer's last painted bounds (relative to them), and the new
    /// comment's pin while it is being written.
    pub(crate) fn comment_pins(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(bounds) = self.viewer_bounds.get() else {
            return Vec::new();
        };
        let settings = &self.document.project().settings;
        let frame_size = (settings.width as f32, settings.height as f32);
        let listed = self.review.listed(self.review_shown());
        let frame = u64::try_from(self.clock.frame()).unwrap_or(0);
        let selected = self.review.selected.as_deref();
        let mut pins: Vec<AnyElement> = pins_at(&listed, selected, frame)
            .filter_map(|(listed, point)| {
                let at = canvas_position(bounds, frame_size, point)?;
                let is_selected = selected == Some(listed.id());
                let target = listed.id().to_owned();
                let body = listed.comment.body.clone();
                Some(
                    pin(
                        listed.number.to_string(),
                        at,
                        mark_color(true, is_selected, cx),
                        cx,
                    )
                    .id(SharedString::from(format!("comment-pin-{target}")))
                    .tooltip(move |window, cx| Tooltip::new(body.clone()).build(window, cx))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            this.select_comment(&target, window, cx);
                        }),
                    )
                    .into_any_element(),
                )
            })
            .collect();
        if let Some(point) = self
            .review
            .draft
            .as_ref()
            .and_then(|draft| draft.anchor.point)
            && let Some(at) = canvas_position(bounds, frame_size, point)
        {
            pins.push(
                pin("+".to_owned(), at, mark_color(true, true, cx), cx)
                    .id("comment-pin-draft")
                    .into_any_element(),
            );
        }
        pins
    }
}

fn spans(chapters: &[ReactChapter]) -> Vec<celesta_project::review::ChapterSpan<'_>> {
    chapters
        .iter()
        .map(|chapter| celesta_project::review::ChapterSpan {
            id: &chapter.id,
            name: &chapter.name,
            from: chapter.from,
        })
        .collect()
}

/// A mark's color: the selected one in the accent, open ones in the
/// warning color, resolved ones muted.
fn mark_color(open: bool, selected: bool, cx: &App) -> gpui_kit::Hsla {
    if selected {
        celesta_editor_theme::accent()
    } else if open {
        cx.theme().warning
    } else {
        cx.theme().muted_foreground
    }
}

/// The square that numbers a comment in the list, as on its mark and pin.
fn number_square(number: usize, open: bool, selected: bool, cx: &App) -> impl IntoElement {
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(rems(1.25))
        .mt(rems(0.125))
        .bg(mark_color(open, selected, cx))
        .text_color(cx.theme().background)
        .text_sm()
        .font_family(cx.theme().mono_font_family.clone())
        .child(number.to_string())
}

/// A pin centered on `at`, relative to the canvas area.
fn pin(
    label: String,
    at: gpui_kit::Point<Pixels>,
    color: gpui_kit::Hsla,
    cx: &App,
) -> gpui_kit::Div {
    let size = metrics::comment_pin_size();
    let half = rems(-size.0 / 2.0);
    div()
        .absolute()
        .left(at.x)
        .top(at.y)
        .ml(half)
        .mt(half)
        .size(size)
        .flex()
        .items_center()
        .justify_center()
        .bg(color)
        .border_1()
        .border_color(cx.theme().background)
        .text_color(cx.theme().background)
        .text_sm()
        .font_family(cx.theme().mono_font_family.clone())
        .child(label)
}

/// `point`, a fraction of the frame, in the canvas area's coordinates.
fn canvas_position(
    bounds: Bounds<Pixels>,
    frame: (f32, f32),
    point: Point,
) -> Option<gpui_kit::Point<Pixels>> {
    let (left, top, scale) = letterbox(bounds, frame)?;
    Some(gpui_kit::point(
        px(left - f32::from(bounds.origin.x) + point.x as f32 * frame.0 * scale),
        px(top - f32::from(bounds.origin.y) + point.y as f32 * frame.1 * scale),
    ))
}

/// The fraction of the frame under `position`; `None` on a letterbox bar.
fn canvas_fraction(
    bounds: Bounds<Pixels>,
    frame: (f32, f32),
    position: gpui_kit::Point<Pixels>,
) -> Option<Point> {
    let (left, top, scale) = letterbox(bounds, frame)?;
    let x = (f32::from(position.x) - left) / (frame.0 * scale);
    let y = (f32::from(position.y) - top) / (frame.1 * scale);
    ((0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y)).then(|| Point {
        x: (f64::from(x) * 1000.0).round() / 1000.0,
        y: (f64::from(y) * 1000.0).round() / 1000.0,
    })
}

/// `2026-11-22T10:00:00Z` as `2026-11-22 10:00`.
fn short_time(rfc3339: &str) -> String {
    match rfc3339.get(..16) {
        Some(head) => head.replacen('T', " ", 1),
        None => rfc3339.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds() -> Bounds<Pixels> {
        // A 16:9 frame in a 400×300 area at (100, 50): 400×225, 37.5 down.
        Bounds::new(
            gpui_kit::point(px(100.0), px(50.0)),
            gpui_kit::size(px(400.0), px(300.0)),
        )
    }

    #[test]
    fn canvas_points_round_trip() {
        let frame = (1920.0, 1080.0);
        let at = canvas_position(bounds(), frame, Point { x: 0.5, y: 0.5 }).expect("placed");
        assert_eq!((f32::from(at.x), f32::from(at.y)), (200.0, 150.0));
        let window = gpui_kit::point(px(300.0), px(200.0));
        assert_eq!(
            canvas_fraction(bounds(), frame, window),
            Some(Point { x: 0.5, y: 0.5 })
        );
    }

    #[test]
    fn a_press_on_the_letterbox_places_nothing() {
        let frame = (1920.0, 1080.0);
        assert_eq!(
            canvas_fraction(bounds(), frame, gpui_kit::point(px(300.0), px(60.0))),
            None
        );
    }

    #[test]
    fn times_shorten_to_the_minute() {
        assert_eq!(short_time("2026-11-22T10:00:00Z"), "2026-11-22 10:00");
        assert_eq!(short_time("bad"), "bad");
    }
}
