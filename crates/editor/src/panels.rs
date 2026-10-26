use crate::helpers::{ExportLoudness, clip_kind_label};
use crate::problems::{Source, flow_line_breaks};
use crate::problems_panel::show_in_problems_row;
use crate::react_audio::react_audio_clip_id;
use crate::timecode::format_timecode;
use crate::view::EditorView;
use crate::waveform::{clip_local_time, format_media_asset_info};
use crate::widgets::{
    inspector_note, inspector_row, inspector_section, json_display, panel_header, property_display,
};
use celesta_composition::{Animatable, evaluate_f64};
use celesta_editor_core::{
    CharacterSummary, ClipSummary, ComponentClipSummary, DialogueClipSummary,
};
use celesta_editor_theme as theme;
use celesta_project::AssetKind;
use celesta_react_bridge::{ENTRY_PROPS_FILE, chapter_at, resolve_props_file};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, IconName, Sizable as _, TitleBar};
use gpui_kit::prelude::*;
use gpui_kit::{ClickEvent, Context, ObjectFit, SharedString, Window, div, img, px};

impl EditorView {
    /// The window's title bar: the menu bar (outside macOS, which shows it
    /// natively), the open file, and the window-wide commands — Create New
    /// Project…, Open…, the export loudness, and Export….
    pub(crate) fn title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let exporting = self.export_cancellation.is_some();
        let muted = cx.theme().muted_foreground;
        let leading = div()
            .flex()
            .items_center()
            .gap_2()
            .min_w_0()
            .overflow_hidden();
        #[cfg(not(target_os = "macos"))]
        let leading = leading.when_some(self.app_menu_bar.clone(), |leading, menu_bar| {
            leading.child(menu_bar)
        });
        let leading = leading
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().foreground)
                    .whitespace_nowrap()
                    .child(self.project_name.clone()),
            )
            .when(self.is_react_preview(), |leading| {
                leading.child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .whitespace_nowrap()
                        .child("React composition"),
                )
            });
        let leading = leading.when_some(
            self.react_preview
                .as_ref()
                .filter(|react| react.compositions.len() > 1),
            |leading, react| {
                let compositions = react.compositions.clone();
                let selected = react.composition_id.clone();
                let view = cx.entity().downgrade();
                leading.child(
                    div().occlude().child(
                        Button::new("select-composition")
                            .small()
                            .outline()
                            .label(format!("Composition: {}", selected))
                            .loading(self.react_reloads.is_running())
                            .icon(IconName::ChevronDown)
                            .disabled(self.opening || exporting || self.react_reloads.is_running())
                            .dropdown_menu(move |mut menu, _, _| {
                                for composition in &compositions {
                                    let id = composition.id.clone();
                                    let view = view.clone();
                                    menu = menu.item(
                                        PopupMenuItem::new(id.clone())
                                            .checked(id == selected)
                                            .on_click(move |_, _, cx| {
                                                let _ = view.update(cx, |this, cx| {
                                                    this.select_composition(id.clone(), cx)
                                                });
                                            }),
                                    );
                                }
                                menu
                            }),
                    ),
                )
            },
        );
        TitleBar::new().child(leading).child(
            div()
                .id("title-bar-actions")
                // The title bar is a window drag area. On Windows a click
                // there is the caption's (`HTCAPTION`) unless something on
                // top blocks the mouse, as `AppMenuBar`'s items do.
                .occlude()
                .flex()
                .items_center()
                .gap_2()
                .pr_2()
                .child(
                    Button::new("create-new-project")
                        .small()
                        .ghost()
                        .label("Create New Project…")
                        .disabled(self.opening || exporting)
                        .on_click(cx.listener(Self::create_new_project_click)),
                )
                .child(
                    Button::new("open-project")
                        .small()
                        .ghost()
                        .icon(IconName::FolderOpen)
                        .label(if self.opening {
                            "Opening…"
                        } else {
                            "Open…"
                        })
                        .loading(self.opening)
                        .disabled(self.opening || exporting)
                        .on_click(cx.listener(Self::open_project_click)),
                )
                .child(self.export_loudness_button(exporting, cx))
                .child(if exporting {
                    Button::new("export-project")
                        .small()
                        .danger()
                        .label(if self.export_cancelling {
                            "Cancelling…"
                        } else {
                            "Cancel export"
                        })
                        .disabled(self.export_cancelling)
                        .on_click(cx.listener(Self::cancel_export_click))
                } else {
                    Button::new("export-project")
                        .small()
                        .outline()
                        .label("Export…")
                        .disabled(
                            self.choosing_export_path
                                || self.opening
                                || self.react_reloads.is_running(),
                        )
                        .on_click(cx.listener(Self::export_project_click))
                }),
        )
    }

    /// Picks the loudness Export… brings the mix to; the choice lasts for
    /// the session.
    fn export_loudness_button(&self, exporting: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.export_loudness;
        let view = cx.entity().downgrade();
        Button::new("export-loudness")
            .small()
            .ghost()
            .label(format!("Loudness: {}", selected.name()))
            .dropdown_caret(true)
            .disabled(exporting || self.choosing_export_path)
            .dropdown_menu(move |mut menu, _, _| {
                for loudness in ExportLoudness::ALL {
                    let view = view.clone();
                    menu = menu.item(
                        PopupMenuItem::new(loudness.menu_label())
                            .checked(loudness == selected)
                            .on_click(move |_, _, cx| {
                                let _ = view
                                    .update(cx, |this, cx| this.set_export_loudness(loudness, cx));
                            }),
                    );
                }
                menu
            })
    }

    pub(crate) fn asset_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let missing_color = cx.theme().danger;
        let meta_color = cx.theme().muted_foreground;
        let rows = self.assets.iter().map(|asset| {
            let asset_id = asset.id.clone();
            let selected = self.selected_asset_id.as_deref() == Some(asset.id.as_str());
            let element_id: SharedString = format!("asset-row-{}", asset.id).into();
            let media_detail = match (asset.missing, self.media_cache.get(&asset.id)) {
                (true, _) => Some("File not found".to_owned()),
                (false, Some(Ok(info))) => Some(format_media_asset_info(info)),
                (false, Some(Err(_))) => Some("Probe failed".to_owned()),
                (false, None) if matches!(asset.kind, AssetKind::Video | AssetKind::Audio) => {
                    Some("Probing…".to_owned())
                }
                (false, None) => None,
            };
            let thumbnail = (asset.kind == AssetKind::Image && !asset.missing)
                .then(|| asset.path.clone())
                .flatten();
            div()
                .id(element_id)
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py_2()
                .cursor_pointer()
                .when(selected, |row| row.bg(cx.theme().list_active))
                .hover(|style| style.bg(cx.theme().list_hover))
                .text_sm()
                .text_color(cx.theme().foreground)
                .child(
                    div()
                        .flex_none()
                        .w(px(44.0))
                        .h(px(26.0))
                        .rounded_sm()
                        .overflow_hidden()
                        .bg(cx.theme().background)
                        .flex()
                        .items_center()
                        .justify_center()
                        .map(|slot| match thumbnail {
                            Some(path) => {
                                slot.child(img(path).size_full().object_fit(ObjectFit::Cover))
                            }
                            None => slot.child(
                                div()
                                    .text_xs()
                                    .text_color(theme::accent())
                                    .child(asset.kind.to_string().to_uppercase()),
                            ),
                        }),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .overflow_hidden()
                        .child(asset.id.clone())
                        .when_some(media_detail, |column, detail| {
                            column.child(
                                div()
                                    .text_xs()
                                    .text_color(if asset.missing {
                                        missing_color
                                    } else {
                                        meta_color
                                    })
                                    .child(detail),
                            )
                        }),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.selected_asset_id = Some(asset_id.clone());
                    cx.notify();
                }))
        });
        let contents = div()
            .id("asset-list-scroll")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_x_hidden()
            .overflow_y_scroll()
            .when(self.assets.is_empty(), |contents| {
                contents.child(
                    div()
                        .p_3()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("No assets in this project"),
                )
            })
            .children(rows);
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .w_full()
            .bg(cx.theme().sidebar)
            .child(panel_header("Assets", self.assets.len(), cx))
            .child(contents)
    }

    /// Read-only facts about the composition and whatever is selected. The
    /// project is edited in its source file; File > Reload picks up changes.
    pub(crate) fn inspector_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let characters = self.document.characters();
        let selected_asset = self.selected_asset_id.as_deref().and_then(|selected| {
            self.assets
                .iter()
                .find(|asset| asset.id == selected)
                .cloned()
        });
        let selected_track = self.selected_track_id.as_deref().and_then(|selected| {
            self.tracks
                .iter()
                .find(|track| track.id == selected)
                .cloned()
        });
        let selected_clip = self.selected_clip_id.as_deref().and_then(|selected| {
            self.tracks
                .iter()
                .flat_map(|track| &track.clips)
                .find(|clip| clip.id == selected)
                .cloned()
        });
        let contents = div()
            .id("inspector-scroll")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_x_hidden()
            .overflow_y_scroll()
            .child(inspector_row("Canvas", self.dimensions.clone(), cx))
            .child(inspector_row(
                "Frame rate",
                self.frame_rate_label.clone(),
                cx,
            ))
            .child(inspector_row(
                "Duration",
                format_timecode(self.clock.end_frame(), self.frame_rate_value),
                cx,
            ))
            .when_some(self.document.react_entry(), |panel, entry| {
                panel.child(inspector_row("React entry", entry.to_owned(), cx))
            })
            .when_some(
                self.problems.active(Source::Schemas).cloned(),
                |panel, active| {
                    // Flowed into two lines at most; Problems has the full
                    // text.
                    panel.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .px_3()
                            .py_2()
                            .text_xs()
                            .text_color(cx.theme().danger)
                            .child(div().line_clamp(2).text_ellipsis().child(format!(
                                "{}: {}",
                                active.problem.title,
                                flow_line_breaks(&active.problem.detail)
                            )))
                            .child(show_in_problems_row("inspector-schemas-show-problems", cx)),
                    )
                },
            )
            .when(self.document.react_entry().is_some(), |panel| {
                self.render_project_properties(panel, cx)
            })
            .when(!characters.is_empty(), |panel| {
                self.render_characters(panel, &characters, cx)
            })
            .when_some(selected_asset, |panel, asset| {
                let media = match (asset.missing, self.media_cache.get(&asset.id)) {
                    (true, _) => Some("File not found".to_owned()),
                    (false, Some(Ok(info))) => Some(format_media_asset_info(info)),
                    (false, Some(Err(error))) => Some(format!("Probe failed: {error}")),
                    (false, None) => None,
                };
                panel
                    .child(inspector_section("Asset", cx))
                    .child(inspector_row("ID", asset.id.clone(), cx))
                    .child(inspector_row("Type", asset.kind.to_string(), cx))
                    .when_some(asset.path.clone(), |panel, path| {
                        panel.child(inspector_row("File", path.display().to_string(), cx))
                    })
                    .when_some(media, |panel, media| {
                        panel.child(inspector_row("Media", media, cx))
                    })
            })
            .when_some(selected_track, |panel, track| {
                let mut state = vec![if track.enabled { "Enabled" } else { "Disabled" }];
                if track.locked {
                    state.push("Locked");
                }
                if track.muted {
                    state.push("Muted");
                }
                if track.solo {
                    state.push("Solo");
                }
                panel
                    .child(inspector_section("Track", cx))
                    .child(inspector_row("Name", track.name.clone(), cx))
                    .child(inspector_row("ID", track.id.clone(), cx))
                    .child(inspector_row("Type", format!("{:?}", track.kind), cx))
                    .child(inspector_row("State", state.join(" · "), cx))
                    .child(inspector_row("Clips", track.item_count.to_string(), cx))
            })
            .when_some(selected_clip, |panel, clip| {
                panel
                    .child(inspector_section("Clip", cx))
                    .child(inspector_row("Name", clip.name.clone(), cx))
                    .child(inspector_row("Type", clip_kind_label(clip.kind), cx))
                    .child(inspector_row("Start", self.timecode_for(clip.start), cx))
                    .child(inspector_row(
                        "Length",
                        self.timecode_for(clip.duration),
                        cx,
                    ))
                    .when(!clip.enabled, |panel| {
                        panel.child(inspector_row("State", "Disabled", cx))
                    })
                    .when_some(self.clip_volume_label(&clip), |panel, volume| {
                        panel.child(inspector_row("Volume", volume, cx))
                    })
                    .when_some(clip.component.clone(), |panel, component| {
                        self.render_component_props(panel, &component, cx)
                    })
                    .when_some(clip.dialogue.clone(), |panel, dialogue| {
                        self.render_dialogue_fields(panel, &dialogue, &characters, cx)
                    })
            });
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .w_full()
            .bg(cx.theme().sidebar)
            .child(contents)
    }

    /// A clip's volume at the playhead, and how many keyframes change it.
    fn clip_volume_label(&self, clip: &ClipSummary) -> Option<String> {
        let volume = clip.volume.as_ref()?;
        let local_time = clip_local_time(self.current_time(), clip);
        let current = evaluate_f64(volume, local_time).unwrap_or(1.0);
        let current = format!("{}%", (current * 100.0).round() as i32);
        Some(match volume {
            Animatable::Keyframes(animation) => {
                format!("{current} · {} keyframes", animation.keyframes.len())
            }
            Animatable::Static(_) => current,
        })
    }

    /// The React composition's track and audio clip selected in the
    /// timeline, read-only: they change in the entry's source.
    fn react_audio_selection<E: ParentElement + FluentBuilder>(
        &self,
        panel: E,
        cx: &mut Context<Self>,
    ) -> E {
        let selected_track = self
            .selected_track_id
            .as_deref()
            .and_then(|selected| self.react_tracks.iter().find(|track| track.id == selected));
        let selected_clip = self.selected_clip_id.as_deref().and_then(|selected| {
            let summary = self
                .react_tracks
                .iter()
                .flat_map(|track| &track.clips)
                .find(|clip| clip.id == selected)?;
            // A video's picture shows the clip its sound plays.
            let id = react_audio_clip_id(selected);
            let clip = self.react_audio_clips.iter().find(|clip| clip.id == id)?;
            Some((summary, clip))
        });
        let panel = match selected_track {
            Some(track) => panel
                .child(inspector_section("Track", cx))
                .child(inspector_row("Name", track.name.clone(), cx))
                .child(inspector_row("Clips", track.item_count.to_string(), cx)),
            None => panel,
        };
        let Some((summary, clip)) = selected_clip else {
            return panel;
        };
        let rate = match &clip.playback_rate {
            Animatable::Static(rate) => format!("{rate}×"),
            Animatable::Keyframes(animation) => {
                format!("{} keyframes", animation.keyframes.len())
            }
        };
        panel
            .child(inspector_section("Clip", cx))
            .child(inspector_row("Source", clip.asset.id.clone(), cx))
            .child(inspector_row("Start", self.timecode_for(summary.start), cx))
            .child(inspector_row(
                "Length",
                self.timecode_for(summary.duration),
                cx,
            ))
            .child(inspector_row(
                "Starts in the file at",
                format!("{} s", clip.source_start.as_seconds().unwrap_or(0.0)),
                cx,
            ))
            .when_some(self.clip_volume_label(summary), |panel, volume| {
                panel.child(inspector_row("Volume", volume, cx))
            })
            .child(inspector_row("Playback rate", rate, cx))
            .when(clip.muted, |panel| {
                panel.child(inspector_row("State", "Muted", cx))
            })
    }

    pub(crate) fn render_characters<E: ParentElement + Sized>(
        &self,
        panel: E,
        characters: &[CharacterSummary],
        cx: &mut Context<Self>,
    ) -> E {
        let panel = panel.child(inspector_section(
            format!("Characters ({})", characters.len()),
            cx,
        ));
        characters.iter().fold(panel, |panel, character| {
            panel.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().foreground)
                            .child(character.name.clone()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!(
                                "{} · {} expression(s)",
                                character.id,
                                character.expressions.len()
                            )),
                    )
                    .when_some(character.lip_sync.clone(), |details, lip_sync| {
                        details.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(format!(
                                    "Lip sync: a={} i={} u={} e={} o={} closed={}",
                                    lip_sync.a,
                                    lip_sync.i,
                                    lip_sync.u,
                                    lip_sync.e,
                                    lip_sync.o,
                                    lip_sync.closed.as_deref().unwrap_or("portrait")
                                )),
                        )
                    }),
            )
        })
    }

    pub(crate) fn render_dialogue_fields<E: ParentElement + Sized>(
        &self,
        panel: E,
        dialogue: &DialogueClipSummary,
        characters: &[CharacterSummary],
        cx: &mut Context<Self>,
    ) -> E {
        let character = characters
            .iter()
            .find(|character| character.id == dialogue.character);
        let expression = dialogue
            .expression
            .clone()
            .or_else(|| character.and_then(|character| character.default_expression.clone()))
            .unwrap_or_else(|| "Default".to_owned());
        panel
            .child(inspector_section("Dialogue", cx))
            .child(inspector_row("Text", dialogue.text.clone(), cx))
            .child(inspector_row(
                "Character",
                character.map_or_else(
                    || dialogue.character.clone(),
                    |character| character.name.clone(),
                ),
                cx,
            ))
            .child(inspector_row("Expression", expression, cx))
            .child(inspector_row(
                "Voice",
                dialogue.audio.clone().unwrap_or_else(|| "None".to_owned()),
                cx,
            ))
            .children(dialogue.audio.is_some().then(|| {
                inspector_row(
                    "Lip sync",
                    if dialogue.lip_sync_cue_count == 0 {
                        "None".to_owned()
                    } else {
                        format!("{} mouth cues", dialogue.lip_sync_cue_count)
                    },
                    cx,
                )
            }))
    }

    /// One row per field of a registered component's property schema, or the
    /// raw configured props when no schema is available.
    pub(crate) fn render_component_props<E: ParentElement + Sized>(
        &self,
        panel: E,
        component: &ComponentClipSummary,
        cx: &mut Context<Self>,
    ) -> E {
        let panel = panel
            .child(inspector_section("Component", cx))
            .child(inspector_row("Registered as", component.name.clone(), cx));
        match self.component_schemas.get(&component.name) {
            Some(schema) => schema.iter().fold(panel, |panel, (key, entry)| {
                let (label, value) = property_display(key, &entry.field, component.props.get(key));
                panel.child(inspector_row(label, value, cx))
            }),
            None if self.component_schema_pending => {
                panel.child(inspector_note("Loading property schema…", cx))
            }
            None => component.props.iter().fold(panel, |panel, (key, value)| {
                panel.child(inspector_row(key.clone(), json_display(value), cx))
            }),
        }
    }

    /// The entry-declared project properties with their current values.
    /// Values not set in the project fall back to each field's declared
    /// default — the same rule `useProjectProperty` applies in React.
    pub(crate) fn render_project_properties<E: ParentElement + Sized>(
        &self,
        panel: E,
        cx: &mut Context<Self>,
    ) -> E {
        match self.project_property_schema.as_ref() {
            Some(schema) if !schema.is_empty() => schema.iter().fold(
                panel.child(inspector_section("Project properties", cx)),
                |panel, (key, field)| {
                    let (label, value) =
                        property_display(key, field, self.document.project_properties().get(key));
                    panel.child(inspector_row(label, value, cx))
                },
            ),
            _ => panel,
        }
    }

    pub(crate) fn reload_react_click(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request_react_reload(cx);
        cx.notify();
    }

    /// Right-hand info panel shown instead of the asset/inspector panels while
    /// previewing a standalone React composition: read-only facts, a manual
    /// Reload button, and the audio clips. What can be edited is in the
    /// Properties tab (`properties_panel`).
    pub(crate) fn react_preview_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let entry = self
            .react_preview
            .as_ref()
            .map(|react| react.entry.display().to_string())
            .unwrap_or_default();
        // Where the values come from: the props file's name (`props.json (auto)`
        // when discovered beside the entry) and/or --props.
        let properties = self
            .react_preview
            .as_ref()
            .map_or_else(String::new, |react| {
                let args = &react.property_args;
                let sources: Vec<String> = args
                    .props_file
                    .iter()
                    .map(|file| {
                        file.file_name()
                            .unwrap_or(file.as_os_str())
                            .to_string_lossy()
                            .into_owned()
                    })
                    .chain(
                        resolve_props_file(&react.entry, args.props_file.as_deref())
                            .filter(|choice| choice.auto)
                            .map(|_| format!("{ENTRY_PROPS_FILE} (auto)")),
                    )
                    .chain(args.props.iter().map(|_| "--props".to_owned()))
                    .collect();
                if sources.is_empty() {
                    "No overrides".to_owned()
                } else {
                    sources.join(" + ")
                }
            });
        div()
            .flex()
            .flex_1()
            .min_h_0()
            .w_full()
            .flex_col()
            .bg(cx.theme().sidebar)
            .child(
                div()
                    .id("react-preview-scroll")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .overflow_x_hidden()
                    .overflow_y_scroll()
                    .child(self.react_preview_facts(entry, properties, cx))
                    .children(self.react_chapter_list(cx))
                    .map(|panel| self.react_audio_selection(panel, cx)),
            )
    }

    /// The composition's chapters with where each starts and how long it
    /// lasts; the current one is highlighted and a click seeks to its start.
    /// `None` when the composition declares none.
    fn react_chapter_list(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let chapters = self.react_chapters()?;
        let frame = u64::try_from(self.clock.frame()).unwrap_or(0);
        let current = chapter_at(chapters, frame).map(|chapter| chapter.id.clone());
        let frame_rate = self.frame_rate_value;
        let muted = cx.theme().muted_foreground;
        let rows = chapters
            .iter()
            .map(|chapter| {
                let target = chapter.clone();
                let is_current = current.as_deref() == Some(chapter.id.as_str());
                let start = format_timecode(i64::try_from(chapter.from).unwrap_or(0), frame_rate);
                let length = format_timecode(
                    i64::try_from(chapter.duration_in_frames).unwrap_or(0),
                    frame_rate,
                );
                div()
                    .id(SharedString::from(format!("react-chapter-{}", chapter.id)))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .px_3()
                    .py_1()
                    .text_xs()
                    .cursor_pointer()
                    .when(is_current, |row| row.bg(cx.theme().list_active))
                    .hover(|row| row.bg(cx.theme().list_hover))
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_color(cx.theme().foreground)
                            .child(chapter.name.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .font_family(cx.theme().mono_font_family.clone())
                            .text_color(muted)
                            .child(format!("{start}  {length}")),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.seek_chapter(&target, cx)))
            })
            .collect::<Vec<_>>();
        Some(
            div()
                .flex()
                .flex_col()
                .child(inspector_section("Chapters", cx))
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .px_3()
                        .py_1()
                        .text_xs()
                        .text_color(muted)
                        .child("Name")
                        .child("Start  Length"),
                )
                .children(rows),
        )
    }

    /// The Properties tab in React-preview mode: which scenes an edit of the
    /// selected layer changes (the scope control, or the impact line), kept
    /// in view at the top while the rest scrolls beneath it: what the layer
    /// is, what can be edited (its Transform rows, then the props file's
    /// properties), then where it was written.
    pub(crate) fn properties_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let impact = self.selection_impact();
        let layer = self.layer_section(cx).unwrap_or_else(|| {
            div()
                .flex()
                .flex_col()
                .child(inspector_section("Layer", cx))
                .child(
                    div()
                        .px_3()
                        .py_2()
                        .text_xs()
                        .text_color(muted)
                        .child("Select a layer in the preview"),
                )
                .into_any_element()
        });
        div()
            .flex()
            .flex_1()
            .min_h_0()
            .w_full()
            .flex_col()
            .bg(cx.theme().sidebar)
            // Outside the scroll area, so the choice of what an edit changes
            // stays in view while editing rows further down. The header
            // scrolls with the rows.
            .children(
                impact
                    .as_ref()
                    .map(|impact| div().flex_none().child(self.impact_line(impact, cx))),
            )
            .child(
                div()
                    .id("properties-scroll")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .overflow_x_hidden()
                    .overflow_y_scroll()
                    .children(
                        impact
                            .as_ref()
                            .and_then(|impact| self.selection_header(impact, cx)),
                    )
                    .children(self.layer_edit_section(cx))
                    .children(self.property_section(cx))
                    .child(layer),
            )
    }

    /// The entry, its composition facts, where property values come from,
    /// and the Reload button.
    fn react_preview_facts(
        &self,
        entry: String,
        properties: String,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let label_color = cx.theme().muted_foreground;
        let value_color = cx.theme().foreground;
        let row = move |label: &str, value: String| {
            div()
                .flex()
                .justify_between()
                .gap_2()
                .text_xs()
                .child(div().text_color(label_color).child(label.to_owned()))
                .child(div().text_color(value_color).text_right().child(value))
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .child(div().text_xs().text_color(label_color).child("Entry"))
            .child(div().text_xs().text_color(value_color).child(entry))
            .child(div().h(px(4.0)))
            .child(row("Size", self.dimensions.to_string()))
            .child(row("Frame rate", self.frame_rate_label.to_string()))
            .child(row(
                "Duration",
                format_timecode(self.clock.end_frame(), self.frame_rate_value),
            ))
            .child(row("Renderer", self.gpu_name.to_string()))
            .child(row("Values from", properties))
            .child(div().h(px(4.0)))
            .child(
                Button::new("reload-react-entry")
                    .small()
                    .w_full()
                    .label("Reload composition")
                    .on_click(cx.listener(Self::reload_react_click)),
            )
            .when(!self.problems.preview_warnings().is_empty(), |panel| {
                panel.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .mt_2()
                        .rounded(cx.theme().radius)
                        .border_1()
                        .border_color(cx.theme().warning.opacity(0.5))
                        .bg(cx.theme().warning.opacity(0.12))
                        .p_2()
                        .text_xs()
                        .text_color(cx.theme().warning)
                        .children(
                            self.preview_warnings_summary(
                                "react-preview-warnings-show-problems",
                                cx,
                            ),
                        ),
                )
            })
    }
}
