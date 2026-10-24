//! The Properties tab's Layer sections: what the layer picked in the viewer
//! is and which scenes an edit of it changes, the JSX position that produced
//! it, and the elements around that one, each opening its position in the
//! user's editor.

use crate::editor_launch::open_source;
use crate::element_edit::{ElementTargets, TransformProp, inner_target, reason_text};
use crate::element_rows::row_label;
use crate::icons::CelestaIcon;
use crate::impact::{ImpactView, counted_scenes, impact_line, impact_view, other_scenes_label};
use crate::layer_select::{
    SourceLookup, display_path, launch_outcome_applies, parent_layer_id, source_position,
};
use crate::timecode::format_timecode;
use crate::view::EditorView;
use crate::widgets::{inspector_row, inspector_section};
use celesta_react_bridge::{ElementKind, LayerSource, LayerSourceReport, SourcePos};
use celesta_source_edit::EditTarget;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Context, Div, Hsla, SharedString, Stateful, div, px};
use std::path::{Path, PathBuf};

/// The label column of the header rows and the property rows, so the labels
/// and the values share one edge; wide enough for "Corner radius".
pub(crate) const LABEL_WIDTH: f32 = 84.0;

/// The icon for a primitive element by its tag; any other shape (and an
/// element that is not a primitive) gets the generic shapes icon.
pub(crate) fn primitive_icon(name: &str) -> CelestaIcon {
    match name {
        "Rect" => CelestaIcon::Square,
        "Ellipse" | "Circle" => CelestaIcon::Circle,
        "Text" => CelestaIcon::Type,
        "Image" => CelestaIcon::Image,
        "Video" => CelestaIcon::Film,
        "Group" | "Stack" => CelestaIcon::Group,
        _ => CelestaIcon::Shapes,
    }
}

/// "in <Card>": the nearest Component entry after the first, wrappers skipped.
pub(crate) fn owner_label(report: &LayerSourceReport) -> Option<String> {
    report
        .chain
        .iter()
        .skip(1)
        .find(|entry| entry.kind == ElementKind::Component)
        .map(|entry| format!("in <{}>", entry.name))
}

/// One line per lock reason of the transform props: "X, Y — Written as an
/// expression: W / 2", in target order; editable props and other rows
/// omitted.
pub(crate) fn locked_summary(targets: &ElementTargets) -> Vec<String> {
    let mut reasons: Vec<(String, Vec<String>)> = Vec::new();
    for (key, target) in &targets.targets {
        let Some(prop) = TransformProp::from_key(key) else {
            continue;
        };
        let EditTarget::Locked(reason) = inner_target(&target.target) else {
            continue;
        };
        let reason = reason_text(reason);
        let label = row_label(prop.name());
        match reasons.iter_mut().find(|(text, _)| *text == reason) {
            Some((_, props)) => props.push(label),
            None => reasons.push((reason, vec![label])),
        }
    }
    reasons
        .into_iter()
        .map(|(reason, props)| format!("{} — {reason}", props.join(", ")))
        .collect()
}

/// More ranges than this are counted after the first ones in "Appears in".
const LISTED_RANGES: usize = 3;

/// "Appears in" without chapters: the first [`LISTED_RANGES`] ranges as
/// `from–to` timecodes, then "and N more" for the rest.
fn ranges_label(ranges: &[(u64, u64)], timecode: impl Fn(u64) -> String) -> String {
    let listed = ranges
        .iter()
        .take(LISTED_RANGES)
        .map(|&(from, to)| format!("{}–{}", timecode(from), timecode(to)))
        .collect::<Vec<_>>()
        .join(", ");
    match ranges.len().saturating_sub(LISTED_RANGES) {
        0 => listed,
        more => format!("{listed} and {more} more"),
    }
}

/// "1 instance", "3 instances".
fn instances(count: usize) -> String {
    match count {
        1 => "1 instance".to_owned(),
        count => format!("{count} instances"),
    }
}

impl EditorView {
    /// The selected layer's report while its source is found.
    fn found_report(&self) -> Option<&LayerSourceReport> {
        self.selected_layer.as_ref()?;
        match &self.source_lookup {
            Some(SourceLookup::Found(report)) if !report.chain.is_empty() => Some(report),
            _ => None,
        }
    }

    /// Which scenes an edit of the selected layer changes, with the playhead
    /// where it is now, or `None` until its source is found. An edit that
    /// writes only a call site in the layer's chain changes only the
    /// instances that call makes.
    pub(crate) fn selection_impact(&self) -> Option<ImpactView> {
        let report = self.found_report()?;
        let written_at = self
            .element_edit
            .resolved(self.selected_layer.as_deref())
            .and_then(|(resolved, _)| resolved.outcome.as_ref().ok())
            .and_then(ElementTargets::written_at)
            // Targets resolved for an older answer may name a call this
            // chain no longer has.
            .filter(|site| {
                report
                    .chain
                    .iter()
                    .any(|entry| SourcePos::from(entry) == **site)
            });
        Some(impact_view(
            report,
            self.occurrences.as_ref().map(|(_, state)| state),
            self.react_chapters().unwrap_or_default(),
            u64::try_from(self.clock.frame()).unwrap_or(0),
            written_at,
        ))
    }

    /// What the selection is: its element and owning component, its position
    /// (a button that opens it), the scenes it appears in, the other scenes
    /// that share it, and why its props can't move. `None` until its source
    /// is found.
    pub(crate) fn selection_header(
        &self,
        impact: &ImpactView,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let report = self.found_report()?;
        let source = report.chain.first()?;
        let entry_dir = self.entry_dir();
        let muted = cx.theme().muted_foreground;
        let foreground = cx.theme().foreground;
        let warning = cx.theme().warning;
        let row = |label: &'static str, value: AnyElement| {
            div()
                .flex()
                .gap_2()
                .text_xs()
                .child(
                    div()
                        .flex_none()
                        .w(px(LABEL_WIDTH))
                        .text_color(muted)
                        .child(label),
                )
                .child(div().flex_1().min_w_0().child(value))
        };
        let text =
            |text: String, color: Hsla| div().text_color(color).child(text).into_any_element();

        let title = div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .flex_none()
                    .text_color(muted)
                    .child(Icon::new(primitive_icon(&source.name)).small()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_sm()
                    .text_color(foreground)
                    .child(source.name.clone()),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap_1()
                    .text_xs()
                    .text_color(muted)
                    .children(owner_label(report))
                    // The reloaded code has not answered yet; the rows show
                    // the answer before it, read-only.
                    .when(self.source_lookup_stale, |group| {
                        group.child(Spinner::new().xsmall())
                    }),
            );
        let opened = source.clone();
        let position = Button::new("selection-source")
            .ghost()
            .small()
            .w_full()
            .icon(IconName::ExternalLink)
            .tooltip(format!("Open {}", source_position(source, &entry_dir)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_color(muted)
                    .child(format!(
                        "{}:{}",
                        display_path(&source.file, &entry_dir),
                        source.line
                    )),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_layer_source(opened.clone(), cx);
            }));

        let appears_in = match impact {
            ImpactView::SpanOnly { note, .. } => text(note.clone(), muted),
            ImpactView::Known(impact) if !impact.appears_in.is_empty() => {
                text(impact.appears_in.join(", "), foreground)
            }
            ImpactView::Known(impact) => {
                let timecode = |frame: u64| {
                    format_timecode(
                        i64::try_from(frame).unwrap_or(i64::MAX),
                        self.frame_rate_value,
                    )
                };
                text(ranges_label(&impact.own_ranges, timecode), foreground)
            }
        };
        let shared = match impact {
            ImpactView::Known(impact) if !impact.other_scenes.is_empty() => {
                // A label that only counts the scenes names them first.
                let lines: Vec<String> = counted_scenes(&impact.other_scenes)
                    .into_iter()
                    .chain(impact.call_sites.iter().map(|site| {
                        format!(
                            "{}:{}:{}",
                            display_path(&site.file, &entry_dir),
                            site.line,
                            site.column
                        )
                    }))
                    .collect();
                Some(
                    div()
                        .id("selection-shared")
                        .text_color(warning)
                        .child(format!(
                            "also in {} ({})",
                            other_scenes_label(&impact.other_scenes),
                            instances(impact.instance_count)
                        ))
                        .tooltip(move |window, cx| {
                            let lines = lines.clone();
                            Tooltip::element(move |_, _| {
                                div()
                                    .flex()
                                    .flex_col()
                                    .children(lines.iter().map(|line| div().child(line.clone())))
                            })
                            .build(window, cx)
                        })
                        .into_any_element(),
                )
            }
            _ => None,
        };
        let locked = self
            .element_edit
            .resolved(self.selected_layer.as_deref())
            .and_then(|(resolved, _)| resolved.outcome.as_ref().ok())
            .map(locked_summary)
            .unwrap_or_default();

        Some(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .px_3()
                .py_2()
                .border_b_1()
                .border_color(cx.theme().border)
                .child(title)
                .child(position)
                .child(row("Appears in", appears_in))
                .children(shared.map(|shared| row("Shared", shared)))
                .children(locked.into_iter().enumerate().map(|(index, line)| {
                    row(
                        if index == 0 { "Can't move" } else { "" },
                        text(line, foreground),
                    )
                }))
                .when_some(self.launch_error.clone(), |header, error| {
                    header.child(div().text_xs().text_color(cx.theme().danger).child(error))
                })
                .into_any_element(),
        )
    }

    /// What a commit of the Transform rows changes, in the warning color when
    /// it reaches other scenes; a hover names the scenes it only counts.
    pub(crate) fn impact_line(&self, impact: &ImpactView, cx: &mut Context<Self>) -> Stateful<Div> {
        let (text, warn) = impact_line(impact);
        let counted = match impact {
            ImpactView::Known(impact) => counted_scenes(&impact.other_scenes),
            ImpactView::SpanOnly { .. } => None,
        };
        div()
            .id("selection-impact")
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .text_xs()
            .when(warn, |line| {
                line.bg(cx.theme().warning.opacity(0.12))
                    .text_color(cx.theme().warning)
            })
            .when(!warn, |line| line.text_color(cx.theme().muted_foreground))
            .child(text)
            .when_some(counted, |line, scenes| {
                line.tooltip(move |window, cx| Tooltip::new(scenes.clone()).build(window, cx))
            })
    }

    /// The selected layer's property sections and the error of its last
    /// edit, or `None` until its source is found.
    pub(crate) fn layer_edit_section(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let id = self.selected_layer.as_deref()?;
        let Some(SourceLookup::Found(report)) = &self.source_lookup else {
            return None;
        };
        if report.chain.is_empty() {
            return None;
        }
        Some(
            div()
                .flex()
                .flex_col()
                .child(self.element_sections(cx))
                .when_some(
                    self.element_edit.error(Some(id)).map(str::to_owned),
                    |section, error| {
                        section.child(
                            div()
                                .px_3()
                                .py_2()
                                .text_xs()
                                .text_color(cx.theme().danger)
                                .child(error),
                        )
                    },
                )
                .into_any_element(),
        )
    }

    /// The selected layer's ID and the JSX that produced it, under a "Source
    /// chain" disclosure, or `None` while nothing is selected.
    pub(crate) fn layer_section(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let id = self.selected_layer.clone()?;
        let layer = id.clone();
        let entry_dir = self.entry_dir();
        let muted = cx.theme().muted_foreground;
        let note = |text: SharedString, color: Hsla| {
            div().px_3().py_2().text_xs().text_color(color).child(text)
        };
        let section = div()
            .flex()
            .flex_col()
            .child(inspector_section("Layer", cx))
            .child(inspector_row("ID", layer, cx));
        let section = match &self.source_lookup {
            Some(SourceLookup::Found(report)) => {
                let (source, callers) = report.chain.split_first()?;
                let open = self.source_chain_open;
                section
                    .child(
                        div()
                            .px_3()
                            .py_2()
                            .border_b_1()
                            .border_color(cx.theme().border)
                            .child(
                                Button::new("source-chain-toggle")
                                    .ghost()
                                    .small()
                                    .w_full()
                                    .icon(if open {
                                        IconName::ChevronDown
                                    } else {
                                        IconName::ChevronRight
                                    })
                                    .child(div().flex_1().child("Source chain"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.source_chain_open = !this.source_chain_open;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .when(open, |section| {
                        section
                            .child(self.source_row("Source", 0, source, &entry_dir, cx))
                            .when(!callers.is_empty(), |section| {
                                section
                                    .child(inspector_section("Called from", cx))
                                    .children(callers.iter().enumerate().map(|(index, caller)| {
                                        self.source_row("", index + 1, caller, &entry_dir, cx)
                                    }))
                            })
                    })
            }
            Some(SourceLookup::NotInFrame) => {
                section.child(note("Not in this frame".into(), muted))
            }
            Some(SourceLookup::NoPosition) => {
                section.child(note("No source position".into(), muted))
            }
            Some(SourceLookup::Failed(error)) => {
                section.child(note(error.clone().into(), cx.theme().danger))
            }
            Some(SourceLookup::Pending) | None => {
                section.child(note("Looking up source…".into(), muted))
            }
        };
        // A failed open shows under the selection header's position.
        Some(section.into_any_element())
    }

    /// One position in the chain: `<name>  <file>:<line>:<column>` as a
    /// button that opens it, under `label` when there is one.
    fn source_row(
        &self,
        label: &'static str,
        index: usize,
        source: &LayerSource,
        entry_dir: &Path,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let opened = source.clone();
        let parent = parent_layer_id(source)
            .filter(|_| index > 0)
            .map(str::to_owned);
        div()
            .flex()
            .flex_col()
            .gap_1()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .when(!label.is_empty(), |row| {
                row.child(div().text_xs().text_color(muted).child(label))
            })
            .when_some(parent, |row, parent| {
                row.child(
                    Button::new(("layer-select-parent", index))
                        .ghost()
                        .small()
                        .w_full()
                        .icon(CelestaIcon::MousePointerClick)
                        .child("Select")
                        .tooltip("Select this layer")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.select_layer(Some(parent.clone()), cx);
                        })),
                )
            })
            .child(
                Button::new(("layer-source", index))
                    .ghost()
                    .small()
                    .w_full()
                    .icon(IconName::ExternalLink)
                    .tooltip(format!(
                        "Open {}:{}:{}",
                        source.file.display(),
                        source.line,
                        source.column
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_w_0()
                            .gap_2()
                            .overflow_hidden()
                            .child(div().flex_none().child(source.name.clone()))
                            .child(
                                div()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .text_color(muted)
                                    .child(source_position(source, entry_dir)),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_layer_source(opened.clone(), cx);
                    })),
            )
            .into_any_element()
    }

    /// Opens `source` in the user's editor off the UI thread (finding the
    /// editor may run the login shell), then shows why it failed, if it did.
    fn open_layer_source(&mut self, source: LayerSource, cx: &mut Context<Self>) {
        let cwd = self.entry_dir();
        let session = self.session;
        let layer = self.selected_layer.clone();
        self.launch_error = None;
        cx.notify();
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { open_source(&source, &cwd) })
                .await;
            view.update(cx, |this, cx| {
                // Another file opened or another layer was picked meanwhile;
                // the message belongs to the old one.
                if launch_outcome_applies(
                    (session, layer.as_deref()),
                    (this.session, this.selected_layer.as_deref()),
                ) {
                    this.launch_error = result.err().map(|error| error.to_string());
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// The React entry's directory, which source paths are shown relative to
    /// and the editor runs in.
    pub(crate) fn entry_dir(&self) -> PathBuf {
        self.react_preview
            .as_ref()
            .and_then(|react| react.entry.parent())
            .map(Path::to_path_buf)
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element_edit::Target;
    use celesta_react_bridge::{ElementKind, ReceivedProps, SourcePos};
    use celesta_source_edit::{EditTarget, EditValue, ElementRef, LiteralForm, LockReason, Span};

    fn entry(name: &str, kind: ElementKind) -> LayerSource {
        LayerSource {
            name: name.to_owned(),
            file: "/a/card.tsx".into(),
            line: 3,
            column: 4,
            kind,
            layer_id: None,
        }
    }

    fn report(chain: Vec<LayerSource>) -> LayerSourceReport {
        LayerSourceReport {
            chain,
            local_frame: 0.0,
            candidate_frames: vec![],
            props: ReceivedProps::default(),
            render_count: 1,
            windows: vec![],
            span: None,
        }
    }

    #[test]
    fn primitive_icons() {
        assert_eq!(primitive_icon("Rect"), CelestaIcon::Square);
        assert_eq!(primitive_icon("Circle"), CelestaIcon::Circle);
        assert_eq!(primitive_icon("Ellipse"), CelestaIcon::Circle);
        assert_eq!(primitive_icon("Text"), CelestaIcon::Type);
        assert_eq!(primitive_icon("Image"), CelestaIcon::Image);
        assert_eq!(primitive_icon("Video"), CelestaIcon::Film);
        assert_eq!(primitive_icon("Group"), CelestaIcon::Group);
        assert_eq!(primitive_icon("Stack"), CelestaIcon::Group);
        assert_eq!(primitive_icon("Star"), CelestaIcon::Shapes);
    }

    #[test]
    fn owner_label_skips_wrappers() {
        let owned = report(vec![
            entry("Rect", ElementKind::Primitive),
            entry("Sequence", ElementKind::Wrapper),
            entry("PriceTag", ElementKind::Component),
        ]);
        assert_eq!(owner_label(&owned).as_deref(), Some("in <PriceTag>"));
        let in_scene = report(vec![
            entry("Rect", ElementKind::Primitive),
            entry("Composition", ElementKind::Wrapper),
        ]);
        assert_eq!(owner_label(&in_scene), None);
        // A Rect written in a `defineScenes` scene function.
        let in_scenes = report(vec![
            entry("Rect", ElementKind::Primitive),
            entry("Scenes", ElementKind::Wrapper),
            entry("Composition", ElementKind::Wrapper),
        ]);
        assert_eq!(owner_label(&in_scenes), None);
    }

    #[test]
    fn ranges_past_three_are_counted() {
        let timecode = |frame: u64| frame.to_string();
        assert_eq!(
            ranges_label(&[(0, 1), (2, 3), (4, 5)], timecode),
            "0–1, 2–3, 4–5"
        );
        let every_other: Vec<(u64, u64)> =
            (0..2400).map(|frame| (frame * 2, frame * 2 + 1)).collect();
        assert_eq!(
            ranges_label(&every_other, timecode),
            "0–1, 2–3, 4–5 and 2397 more"
        );
        assert_eq!(ranges_label(&[], timecode), "");
    }

    #[test]
    fn locked_summary_groups_props_by_reason() {
        let expression = || {
            EditTarget::Locked(LockReason::Expression {
                source: "W / 2".to_owned(),
            })
        };
        let element = ElementRef {
            start: 0,
            end_of_attributes: 5,
            children: None,
        };
        let local = |path: &str, target| Target {
            file: "/a/card.tsx".into(),
            path: path.to_owned(),
            target,
            call_site: None,
            inputs: Vec::new(),
        };
        // `y` comes from the call site, where it is written as an expression.
        let passed = Target {
            file: "/a/scene.tsx".into(),
            path: "y".to_owned(),
            target: EditTarget::PassThrough {
                call_site: element,
                param: "y".to_owned(),
                target: Box::new(expression()),
            },
            call_site: Some(SourcePos {
                file: "/a/scene.tsx".into(),
                line: 1,
                column: 1,
            }),
            inputs: Vec::new(),
        };
        let targets = ElementTargets {
            file: "/a/card.tsx".into(),
            files: vec!["/a/card.tsx".into(), "/a/scene.tsx".into()],
            element,
            targets: vec![
                (TransformProp::X.key(), local("x", expression())),
                (TransformProp::Y.key(), passed),
                (
                    TransformProp::Rotation.key(),
                    local(
                        "rotation",
                        EditTarget::Literal {
                            span: Span { start: 1, end: 2 },
                            value: EditValue::Number(0.0),
                            form: LiteralForm::Number,
                        },
                    ),
                ),
                (
                    TransformProp::Scale.key(),
                    local("scale", EditTarget::Locked(LockReason::OutsideKeyframes)),
                ),
            ],
            table: None,
            component: None,
        };
        assert_eq!(
            locked_summary(&targets),
            vec![
                "X, Y — Written as an expression: W / 2".to_owned(),
                "Scale — Outside the keyframes".to_owned(),
            ]
        );
    }
}
