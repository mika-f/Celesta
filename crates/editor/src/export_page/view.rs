//! The Export panel: what Export… shows beside the viewer before the save
//! panel. Under its header one column scrolls: the preset, the container
//! and the range, then the "Video" and "Audio" sections, each with its
//! encoder and that encoder's fields (the audio's ending with the
//! loudness). The footer, pinned under it, holds `Save as preset…` and
//! `Export…`, and above them, once asked for, the inline "Save as
//! preset…" form. Enter anywhere in the panel runs `Export…`.
//!
//! The view owns the controls and the work in flight; what they show and
//! edit is [`ExportModel`]'s. A preset or container change is resolved
//! on the background executor, each plugin's `describe` too.

use super::model::{Asked, CHECKING, ExportModel, InitialBase, PresetFileState, Track};
use super::wording::{PageWarning, plain};
use crate::actions::ConfirmExport;
use crate::helpers::ExportLoudness;
use crate::panel_header::PanelHeader;
use crate::widgets::CompactLabel as _;
use celesta_editor_theme::metrics;
use celesta_exporter::{
    OutputChoice, Preset, PresetOutput, PresetSource, PresetsFile, resolve_preset_output,
};
use celesta_media::{ComponentKind, ComponentRef, MediaRegistry, SettingValue};
use celesta_plugin_host::PluginKillSwitch;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, IndexPath, Sizable as _, h_flex, v_flex,
};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Context, Entity, EventEmitter, FocusHandle, Focusable, Hsla, KeyBinding,
    SharedString, Subscription, Task, Window, div,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// What a warning about the user file's `last` calls it, where it calls a
/// preset ``preset `web` ``.
const LAST_EXPORT: &str = "your last export";

/// Why `Export…` waits: the save panel it opened is up.
const WAITING: &str = "Choosing where to save…";

/// What a warning about the preset `name` calls it.
fn preset_label(name: &str) -> String {
    format!("preset `{name}`")
}

mod choice;
mod fields;
mod layout;
mod save;

use choice::{Choice, Sections, component_choice, sync_select};
use fields::{FieldControl, Widget};
use layout::{row, section_header};
use save::SaveForm;

/// What the page tells its owner.
#[expect(
    clippy::large_enum_variant,
    reason = "one event per click, matched by reference where it arrives"
)]
pub(crate) enum ExportPageEvent {
    /// `Export…`: the choice, the muxer's first extension for the save
    /// panel, and the choice as `last` saves it.
    Export {
        choice: OutputChoice,
        extension: String,
        last: PresetOutput,
    },
    /// The loudness was changed, for the export and the next page.
    Loudness(ExportLoudness),
    /// The range's `Clear`: drop the in- and out-points.
    ClearRange,
    /// The header's close button: back to editing.
    Close,
}

/// The part of the page a resolve was asked from, under which its
/// warnings or failure show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Section {
    Preset,
    Container,
}

/// A preset in the list: `None` for "None", the defaults.
type PresetKey = Option<(PresetSource, String)>;

pub(crate) struct ExportPage {
    /// The panel's; it takes the focus when shown, so Enter exports.
    focus_handle: FocusHandle,
    model: ExportModel,
    registry: Arc<MediaRegistry>,
    kill: PluginKillSwitch,
    preset: Entity<SelectState<Sections<PresetKey>>>,
    muxer: Entity<SelectState<Vec<Choice<ComponentRef>>>>,
    video: Entity<SelectState<Vec<Choice<ComponentRef>>>>,
    audio: Entity<SelectState<Vec<Choice<Option<ComponentRef>>>>>,
    loudness: Entity<SelectState<Vec<Choice<ExportLoudness>>>>,
    fields: Vec<FieldControl>,
    /// Whether each track's "Advanced" fields are shown (video, audio).
    advanced: [bool; 2],
    /// The section the shown warnings came from.
    warned: Section,
    /// Why the last preset or container change could not be resolved, and
    /// where it was asked from; the previous choice stays.
    failure: Option<(Section, String)>,
    /// The in- and out-points' timecodes; `None` exports the whole
    /// composition.
    range: Option<SharedString>,
    /// Whether In and Out can be set for what is exported: not for a
    /// React entry, which always exports all of it.
    range_settable: bool,
    save: Option<SaveForm>,
    /// Whether the save panel `Export…` opened is up: a second `Export…`
    /// waits for it.
    waiting: bool,
    /// The running resolve; replacing it drops a stale one.
    resolving: Option<Task<()>>,
    describing: Vec<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<ExportPageEvent> for ExportPage {}

impl Drop for ExportPage {
    fn drop(&mut self) {
        self.kill.kill();
    }
}

/// A presets file as the page reads it: `None` when there is no path.
fn read_presets(path: Option<PathBuf>) -> PresetFileState {
    let file = match &path {
        Some(path) => PresetsFile::load_if_present(path).map_err(|error| error.to_string()),
        None => Ok(None),
    };
    PresetFileState { path, file }
}

fn track_id(track: Track) -> &'static str {
    match track {
        Track::Video => "video",
        Track::Audio => "audio",
    }
}

impl ExportPage {
    /// A page over `registry`'s media plugins, with the presets of
    /// `project_file` (none for an untitled document) and `user_file`;
    /// starts describing each plugin and resolving the first choice.
    pub(crate) fn new(
        registry: Arc<MediaRegistry>,
        project_file: Option<PathBuf>,
        user_file: Option<PathBuf>,
        loudness: ExportLoudness,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let model = ExportModel::new(
            &registry,
            read_presets(project_file),
            read_presets(user_file),
        );
        let kill = PluginKillSwitch::new();

        let preset =
            cx.new(|cx| SelectState::new(Sections::<PresetKey>(Vec::new()), None, window, cx));
        let muxer =
            cx.new(|cx| SelectState::new(Vec::<Choice<ComponentRef>>::new(), None, window, cx));
        let video =
            cx.new(|cx| SelectState::new(Vec::<Choice<ComponentRef>>::new(), None, window, cx));
        let audio = cx.new(|cx| {
            SelectState::new(Vec::<Choice<Option<ComponentRef>>>::new(), None, window, cx)
        });
        let loudness_choices: Vec<_> = ExportLoudness::ALL
            .iter()
            .map(|choice| Choice::new(choice.menu_label(), *choice))
            .collect();
        let shown = ExportLoudness::ALL
            .iter()
            .position(|choice| *choice == loudness)
            .map(IndexPath::new);
        let loudness = cx.new(|cx| SelectState::new(loudness_choices, shown, window, cx));
        let subscriptions = vec![
            cx.subscribe_in(&preset, window, |this, _, event, window, cx| {
                let SelectEvent::Confirm(Some(key)) = event else {
                    return;
                };
                this.choose_preset(key.clone(), window, cx);
            }),
            cx.subscribe_in(&muxer, window, |this, _, event, window, cx| {
                let SelectEvent::Confirm(Some(muxer)) = event else {
                    return;
                };
                this.choose_container(muxer.clone(), window, cx);
            }),
            cx.subscribe_in(&video, window, |this, _, event, window, cx| {
                let SelectEvent::Confirm(Some(encoder)) = event else {
                    return;
                };
                this.choose_encoder(Track::Video, Some(encoder.clone()), window, cx);
            }),
            cx.subscribe_in(&audio, window, |this, _, event, window, cx| {
                let SelectEvent::Confirm(Some(encoder)) = event else {
                    return;
                };
                this.choose_encoder(Track::Audio, encoder.clone(), window, cx);
            }),
            cx.subscribe_in(&loudness, window, |_, _, event, _, cx| {
                if let SelectEvent::Confirm(Some(loudness)) = event {
                    cx.emit(ExportPageEvent::Loudness(*loudness));
                }
            }),
        ];

        // Each media plugin with an encoder or a muxer is described once,
        // on its own, so a slow plugin does not hold up the others' lists.
        let mut plugins: Vec<usize> = registry
            .components()
            .into_iter()
            .filter(|info| info.kind != ComponentKind::Decoder)
            .map(|info| info.plugin)
            .collect();
        plugins.dedup();

        let mut this = Self {
            focus_handle: cx.focus_handle(),
            model,
            registry,
            kill,
            preset,
            muxer,
            video,
            audio,
            loudness,
            fields: Vec::new(),
            advanced: [false; 2],
            warned: Section::Preset,
            failure: None,
            range: None,
            range_settable: true,
            save: None,
            waiting: false,
            resolving: None,
            describing: Vec::new(),
            _subscriptions: subscriptions,
        };
        this.describe(plugins, window, cx);
        this.resolve_first(window, cx);
        this.sync(window, cx);
        this
    }

    /// Describes each of `plugins` in the background, each on its own.
    fn describe(&mut self, plugins: Vec<usize>, window: &mut Window, cx: &mut Context<Self>) {
        for plugin in plugins {
            let (registry, kill) = (self.registry.clone(), self.kill.clone());
            let described = cx
                .background_executor()
                .spawn(async move { registry.describe(plugin, &kill) });
            self.describing
                .push(cx.spawn_in(window, async move |this, cx| {
                    let description = described.await;
                    this.update_in(cx, |this, window, cx| {
                        this.model.described(plugin, description);
                        this.sync(window, cx);
                    })
                    .ok();
                }));
        }
    }

    /// Resolves the first choice: the project's default preset, else the
    /// user's `last`, else the defaults, with the warnings opening found.
    fn resolve_first(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (base, warnings) = self.model.initial_base();
        let (selected, base, asked) = match base {
            InitialBase::Preset(source, name, output) => (
                Some((source, name.clone())),
                Some((preset_label(&name), output.clone())),
                Asked::Preset(Box::new(output)),
            ),
            InitialBase::Last(output) => (
                None,
                Some((LAST_EXPORT.to_owned(), output)),
                Asked::Defaults,
            ),
            InitialBase::None => (None, None, Asked::Defaults),
        };
        let outputs = self
            .model
            .first_outputs(base.as_ref().map(|(_, output)| output));
        self.resolve(
            OutputChoice::default(),
            base,
            outputs,
            (selected, asked),
            Section::Preset,
            warnings,
            window,
            cx,
        );
    }

    /// The page is shown again: a plugin whose `describe` failed (none
    /// of its components available) is described again, and a first
    /// choice that could not be resolved is resolved again, so a passing
    /// failure does not keep `Export…` blocked while the page is kept.
    pub(crate) fn reopen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let failed = self.model.recheck_failed();
        self.describe(failed, window, cx);
        if self.model.choice().is_none() && self.resolving.is_none() {
            self.failure = None;
            self.model.error = None;
            self.resolve_first(window, cx);
        }
        self.sync(window, cx);
    }

    /// The project presets file this page read; `None` for an untitled
    /// document.
    pub(crate) fn project_file(&self) -> Option<&Path> {
        self.model.project.path.as_deref()
    }

    /// Shows `range` (the in- and out-points' timecodes) as what is
    /// exported; `None` is the whole composition, with a note on setting
    /// In and Out where `settable` (not for a React entry).
    pub(crate) fn set_range(
        &mut self,
        range: Option<SharedString>,
        settable: bool,
        cx: &mut Context<Self>,
    ) {
        self.range = range;
        self.range_settable = settable;
        cx.notify();
    }

    /// Whether the save panel `Export…` opened is up; while it is,
    /// `Export…` is disabled and Enter does nothing.
    pub(crate) fn set_waiting(&mut self, waiting: bool, cx: &mut Context<Self>) {
        self.waiting = waiting;
        cx.notify();
    }

    /// Resolves `choice` over `base` (what its warnings call it, and the
    /// preset) for the first of `outputs` that resolves, in the
    /// background, and applies the result as `applied` says (the preset
    /// selected, and what was asked) with `warnings` added, and why an
    /// earlier output was passed over; a failure (the first output's) is
    /// shown under `section` and keeps the previous choice.
    #[allow(clippy::too_many_arguments)]
    fn resolve(
        &mut self,
        choice: OutputChoice,
        base: Option<(String, PresetOutput)>,
        outputs: Vec<PathBuf>,
        (selected, asked): (Option<(PresetSource, String)>, Asked),
        section: Section,
        warnings: Vec<PageWarning>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (registry, kill) = (self.registry.clone(), self.kill.clone());
        let resolved = cx.background_executor().spawn(async move {
            let base = base.as_ref().map(|(name, output)| (name.as_str(), output));
            super::model::resolve_first_available(&outputs, |output| {
                resolve_preset_output(&choice, base, output, &registry, &mut |plugin| {
                    registry.describe(plugin, &kill)
                })
            })
        });
        self.resolving = Some(cx.spawn_in(window, async move |this, cx| {
            let resolved = resolved.await;
            this.update_in(cx, |this, window, cx| {
                this.resolving = None;
                match resolved {
                    Ok((resolved, _, passed_over)) => {
                        this.model.apply(&resolved, selected, asked);
                        // `apply` replaces the warnings with the resolve's.
                        this.model.warnings.extend(passed_over);
                        this.model.warnings.extend(warnings);
                        this.warned = section;
                        this.failure = None;
                    }
                    Err(error) => {
                        let message = super::model::resolution_error(&error);
                        if this.model.choice().is_none() {
                            this.model.error = Some(message.clone());
                        }
                        // The opening's warnings (an unreadable presets
                        // file) still say why its presets are missing.
                        if !warnings.is_empty() {
                            this.model.warnings = warnings;
                            this.warned = section;
                        }
                        this.failure = Some((section, message));
                    }
                }
                this.sync(window, cx);
            })
            .ok();
        }));
        cx.notify();
    }

    fn choose_preset(&mut self, key: PresetKey, window: &mut Window, cx: &mut Context<Self>) {
        let base = key.as_ref().and_then(|(source, name)| {
            let (project, user) = self.model.presets();
            let file = match source {
                PresetSource::Project => project,
                PresetSource::User | PresetSource::File => user,
            };
            let preset = file.into_iter().find(|preset| preset.name == *name)?;
            Some((preset_label(name), preset.output.clone()))
        });
        let outputs = self
            .model
            .first_outputs(base.as_ref().map(|(_, output)| output));
        let selected = key.filter(|_| base.is_some());
        let asked = match &base {
            Some((_, output)) => Asked::Preset(Box::new(output.clone())),
            None => Asked::Defaults,
        };
        self.resolve(
            OutputChoice::default(),
            base,
            outputs,
            (selected, asked),
            Section::Preset,
            Vec::new(),
            window,
            cx,
        );
    }

    fn choose_container(
        &mut self,
        muxer: ComponentRef,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .model
            .choice()
            .is_some_and(|choice| choice.muxer == muxer)
        {
            return;
        }
        let output =
            Path::new("export").with_extension(self.model.extension_of(&muxer).unwrap_or_default());
        let choice = self.model.container_choice(&muxer);
        self.resolve(
            choice,
            None,
            vec![output],
            (None, Asked::Container),
            Section::Container,
            Vec::new(),
            window,
            cx,
        );
    }

    fn choose_encoder(
        &mut self,
        track: Track,
        encoder: Option<ComponentRef>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.model.choose_encoder(track, encoder);
        self.sync(window, cx);
    }

    fn set_setting(
        &mut self,
        track: Track,
        key: &str,
        value: SettingValue,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.model.set_setting(track, key, value);
        self.sync(window, cx);
    }

    /// Brings every list and field control to the model: the items, what
    /// is selected, and the fields of the chosen encoders.
    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (project, user) = self.model.presets();
        let group = |title: &str, source: PresetSource, presets: Vec<&Preset>| {
            let choices: Vec<_> = presets
                .into_iter()
                .map(|preset| Choice::new(preset.name.clone(), Some((source, preset.name.clone()))))
                .collect();
            (!choices.is_empty()).then(|| (Some(SharedString::from(title.to_owned())), choices))
        };
        let mut sections = vec![(None, vec![Choice::new("None", None)])];
        sections.extend(group("This project", PresetSource::Project, project));
        sections.extend(group("My presets", PresetSource::User, user));
        let selected = self.model.selected.clone();
        self.preset.update(cx, |state, cx| {
            state.set_items(Sections(sections), window, cx);
            state.set_selected_value(&selected, window, cx);
        });

        let choice = self.model.choice().cloned();
        let muxers: Vec<_> = self
            .model
            .muxer_options()
            .iter()
            .map(|option| component_choice(option, option.reference.clone()))
            .collect();
        let muxer = choice.as_ref().map(|choice| choice.muxer.clone());
        sync_select(&self.muxer, muxers, muxer.as_ref(), window, cx);

        let videos: Vec<_> = self
            .model
            .encoder_options(Track::Video)
            .iter()
            .map(|option| component_choice(option, option.reference.clone()))
            .collect();
        let video = choice.as_ref().map(|choice| choice.video.component.clone());
        sync_select(&self.video, videos, video.as_ref(), window, cx);

        let mut audios = vec![Choice::new("None", None)];
        audios.extend(
            self.model
                .encoder_options(Track::Audio)
                .iter()
                .map(|option| component_choice(option, Some(option.reference.clone()))),
        );
        let audio = choice
            .as_ref()
            .map(|choice| choice.audio.as_ref().map(|audio| audio.component.clone()));
        sync_select(&self.audio, audios, audio.as_ref(), window, cx);

        self.sync_fields(window, cx);
        cx.notify();
    }

    /// Why `Export…` cannot run now: a typed number that is not one, a
    /// resolve still running, the save panel it opened still up, or what
    /// the model says.
    fn export_blocked(&self) -> Option<String> {
        if let Some((label, typo)) = self
            .fields
            .iter()
            .find_map(|control| match &control.widget {
                Widget::Text {
                    typo: Some(typo), ..
                } => Some((&control.label, typo)),
                _ => None,
            })
        {
            return Some(format!("{label}: {typo}"));
        }
        if self.resolving.is_some() {
            return Some(CHECKING.to_owned());
        }
        if self.waiting {
            return Some(WAITING.to_owned());
        }
        self.model.output_choice().err()
    }

    /// `Export…` or Enter ([`ConfirmExport`]; Enter in the save form's
    /// name saves instead, and never reaches it): emits
    /// [`ExportPageEvent::Export`] when the choice can be exported, and
    /// says whether it did.
    fn export(&mut self, cx: &mut Context<Self>) -> bool {
        if self.export_blocked().is_some() {
            return false;
        }
        let (Ok(choice), Some(extension)) = (self.model.output_choice(), self.model.extension())
        else {
            return false;
        };
        cx.emit(ExportPageEvent::Export {
            choice,
            extension,
            last: self.model.saved_output(),
        });
        true
    }
}

/// A warning or an error under what it is about: the icon and the text
/// in `color`, never the color alone.
fn problem_line(text: impl Into<SharedString>, color: Hsla) -> impl IntoElement {
    h_flex()
        .items_start()
        .gap_1()
        .text_sm()
        .text_color(color)
        .child(
            Icon::new(IconName::TriangleAlert)
                .xsmall()
                .text_color(color)
                .flex_none(),
        )
        .child(div().min_w_0().child(text.into()))
}

/// Why `Export…` (and `Save`) cannot run: "Checking…" and the wait for
/// the save panel muted, anything else as an error.
fn blocked_line(reason: String, cx: &App) -> impl IntoElement {
    if reason.ends_with(CHECKING) || reason == WAITING {
        muted_line(reason, cx).into_any_element()
    } else {
        problem_line(reason, cx.theme().danger).into_any_element()
    }
}

/// `warnings` split into the ones listed as they are (an unreadable
/// presets file, an unknown default, a resolve's about no base or about
/// another preset) and the resolve's about the last export, which go under
/// one line naming it.
fn split_last_export(warnings: &[PageWarning]) -> (Vec<&PageWarning>, Vec<&PageWarning>) {
    warnings
        .iter()
        .partition(|warning| warning.base() != Some(LAST_EXPORT))
}

/// The page's keys: Enter runs its `Export…` from anywhere in it.
pub(crate) fn key_bindings() -> [KeyBinding; 1] {
    [KeyBinding::new("enter", ConfirmExport, Some("ExportPage"))]
}

fn muted_line(text: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    div()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}

impl ExportPage {
    fn failure_under(&self, section: Section, cx: &App) -> Option<AnyElement> {
        let (failed, message) = self.failure.as_ref()?;
        (*failed == section)
            .then(|| problem_line(plain(message), cx.theme().danger).into_any_element())
    }

    /// The warnings `section` asked for; those about the last export
    /// after the rest, under one line that says so, since no row names it.
    fn warnings_under(&self, section: Section, cx: &App) -> Vec<AnyElement> {
        if self.warned != section {
            return Vec::new();
        }
        let (others, from_last) = split_last_export(&self.model.warnings);
        let line = |warning: &PageWarning| {
            problem_line(self.model.warning_text(warning), cx.theme().warning).into_any_element()
        };
        let mut lines: Vec<AnyElement> = others.into_iter().map(line).collect();
        if !from_last.is_empty() {
            lines.push(
                div()
                    .debug_selector(|| "export-warnings-from-last".to_owned())
                    .child(muted_line("From your last export:", cx))
                    .into_any_element(),
            );
            lines.extend(from_last.into_iter().map(line));
        }
        lines
    }

    /// What is wrong with what `section` asked for, under its row.
    fn problems_under(&self, section: Section, cx: &App) -> Vec<AnyElement> {
        let mut lines = self.warnings_under(section, cx);
        lines.extend(self.failure_under(section, cx));
        lines
    }

    fn render_preset(&self, cx: &App) -> AnyElement {
        row(
            "preset".into(),
            "Preset".into(),
            Select::new(&self.preset)
                .id("export-preset")
                .small()
                .accessibility_label("Preset")
                .into_any_element(),
            self.problems_under(Section::Preset, cx),
            cx,
        )
    }

    fn render_container(&self, cx: &App) -> AnyElement {
        row(
            "container".into(),
            "Container".into(),
            Select::new(&self.muxer)
                .id("export-container")
                .small()
                .accessibility_label("Container")
                .placeholder(CHECKING)
                .into_any_element(),
            self.problems_under(Section::Container, cx),
            cx,
        )
    }

    /// The in- and out-points in monospace with `Clear`, or the whole
    /// composition, with how to set them where they can be.
    fn render_range(&self, cx: &mut Context<Self>) -> AnyElement {
        let (control, lines) = match &self.range {
            Some(range) => (
                h_flex()
                    .gap_2()
                    .min_w_0()
                    .child(
                        div()
                            .min_w_0()
                            .text_base()
                            .font_family(cx.theme().mono_font_family.clone())
                            .child(range.clone()),
                    )
                    .child(
                        Button::new("export-range-clear")
                            .ghost()
                            .small()
                            .label("Clear")
                            .debug_selector(|| "export-range-clear".to_owned())
                            .on_click(cx.listener(|_, _, _, cx| {
                                cx.emit(ExportPageEvent::ClearRange);
                            })),
                    )
                    .into_any_element(),
                Vec::new(),
            ),
            None => (
                div()
                    .text_base()
                    .child("Whole composition")
                    .into_any_element(),
                self.range_settable
                    .then(|| muted_line("Set In and Out on the timeline.", cx).into_any_element())
                    .into_iter()
                    .collect(),
            ),
        };
        row("range".into(), "Range".into(), control, lines, cx)
    }

    /// `track`'s section under its header: the encoder, its fields
    /// (ungrouped first, then each group), then "Advanced" and, once
    /// open, the advanced fields; the audio's ends with the loudness.
    fn render_track(&self, track: Track, cx: &mut Context<Self>) -> impl IntoElement {
        let id = track_id(track);
        let select = match track {
            Track::Video => Select::new(&self.video)
                .id("export-video-encoder")
                .small()
                .accessibility_label("Video encoder")
                .placeholder(CHECKING)
                .into_any_element(),
            Track::Audio => Select::new(&self.audio)
                .id("export-audio-encoder")
                .small()
                .accessibility_label("Audio encoder")
                .into_any_element(),
        };
        let open = self.advanced[track as usize];
        let has_advanced = !self.model.fields(track, true).is_empty();
        let toggle = format!("export-{id}-advanced");
        v_flex()
            .w_full()
            .px(metrics::panel_padding())
            .py_2()
            .child(row(
                format!("{id}-encoder").into(),
                "Encoder".into(),
                select,
                Vec::new(),
                cx,
            ))
            .children(self.render_groups(track, false, cx))
            .when(has_advanced, |section| {
                section.child(
                    h_flex().min_h(metrics::list_row_height()).child(
                        Button::new(SharedString::from(toggle.clone()))
                            .ghost()
                            .xsmall()
                            // The icon starts on the labels' edge.
                            .pl_0()
                            // A child rather than `icon`, so where it starts
                            // can be measured.
                            .child(
                                div()
                                    .debug_selector(|| format!("{toggle}-icon"))
                                    .flex_none()
                                    .child(
                                        Icon::new(if open {
                                            IconName::ChevronDown
                                        } else {
                                            IconName::ChevronRight
                                        })
                                        .xsmall(),
                                    ),
                            )
                            .compact_label("Advanced")
                            .debug_selector(|| toggle)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.advanced[track as usize] = !open;
                                cx.notify();
                            })),
                    ),
                )
            })
            .when(has_advanced && open, |section| {
                section.children(self.render_groups(track, true, cx))
            })
            .when(track == Track::Audio, |section| {
                section.child(self.render_loudness(cx))
            })
    }

    /// The loudness, disabled (its value kept) while the chosen output
    /// exports no audio; before there is a choice it is left as it is.
    fn render_loudness(&self, cx: &App) -> AnyElement {
        let silent = self.model.choice().is_some() && !self.model.exports_audio();
        let note = silent.then(|| {
            div()
                .debug_selector(|| "export-note-loudness".to_owned())
                .child(muted_line("No audio is exported.", cx))
                .into_any_element()
        });
        row(
            "loudness".into(),
            "Loudness".into(),
            Select::new(&self.loudness)
                .id("export-loudness")
                .small()
                .accessibility_label("Loudness")
                .disabled(silent)
                .into_any_element(),
            note.into_iter().collect(),
            cx,
        )
    }

    /// The footer: the open "Save as preset…" form, why `Export…` is
    /// disabled, then `Save as preset…` leading and `Export…` trailing.
    fn render_footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let blocked = self.export_blocked();
        let disabled = blocked.is_some();
        let save = self.render_save(cx);
        let saving = save.is_some();
        // The open form says it above its `Save`, which it disables too.
        let reason = blocked
            .filter(|_| !saving)
            .map(|reason| blocked_line(reason, cx).into_any_element());
        let theme = cx.theme();
        v_flex()
            .flex_none()
            .w_full()
            .gap_2()
            .p(metrics::panel_padding())
            .bg(theme.title_bar)
            .border_t_1()
            .border_color(theme.border)
            .children(save)
            .children(reason)
            .child(
                h_flex()
                    .justify_between()
                    .gap_2()
                    .child(
                        Button::new("export-save-as")
                            .ghost()
                            .small()
                            .label("Save as preset…")
                            .disabled(saving)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.open_save(window, cx)),
                            ),
                    )
                    .child(
                        Button::new("export-page-export")
                            .primary()
                            .small()
                            .label("Export…")
                            .disabled(disabled)
                            .debug_selector(|| "export-page-export".to_owned())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.export(cx);
                            })),
                    ),
            )
    }
}

impl Focusable for ExportPage {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ExportPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let close = Button::new("export-page-close")
            .ghost()
            .xsmall()
            .icon(IconName::Close)
            .tooltip("Back to editing")
            .accessibility_label("Back to editing")
            .debug_selector(|| "export-page-close".to_owned())
            .on_click(cx.listener(|_, _, _, cx| cx.emit(ExportPageEvent::Close)));
        v_flex()
            .id("export-page")
            .key_context("ExportPage")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &ConfirmExport, _, cx| {
                this.export(cx);
            }))
            .size_full()
            .min_h_0()
            .bg(cx.theme().sidebar)
            .child(PanelHeader::new("export-page", "Export").action(close))
            .child(
                div()
                    .id("export-page-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(
                        v_flex()
                            .w_full()
                            .child(
                                v_flex()
                                    .w_full()
                                    .px(metrics::panel_padding())
                                    .py_2()
                                    .child(self.render_preset(cx))
                                    .child(self.render_container(cx))
                                    .child(self.render_range(cx)),
                            )
                            .child(section_header("Video", cx))
                            .child(self.render_track(Track::Video, cx))
                            .child(section_header("Audio", cx))
                            .child(self.render_track(Track::Audio, cx)),
                    ),
            )
            .child(self.render_footer(cx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_exporter::{ResolveWarning, SettingInput};
    use gpui_kit::component::Root;
    use gpui_kit::{Modifiers, Pixels, TestAppContext, VisualTestContext, WindowHandle, px, size};
    use std::cell::RefCell;
    use std::rc::Rc;

    /// The test window's height: tall enough for every row.
    /// physical: a test window's size, not a layout length.
    const WINDOW_HEIGHT: f32 = 1000.;
    /// physical: a test window's size, not a layout length.
    const WINDOW_WIDTH: f32 = 800.;

    fn reference(text: &str) -> ComponentRef {
        text.parse().unwrap()
    }

    /// A window `width` wide showing a page over `plugins`, with the
    /// project's and the user's presets files, and the page's Enter bound
    /// as the editor binds it.
    fn window_with_page(
        cx: &mut TestAppContext,
        width: Pixels,
        plugins: Vec<celesta_media::RegisteredPlugin>,
        project_file: Option<PathBuf>,
        user_file: Option<PathBuf>,
    ) -> (WindowHandle<Root>, Entity<ExportPage>) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            celesta_editor_theme::init(cx);
            cx.bind_keys(key_bindings());
        });
        let registry = Arc::new(MediaRegistry::new(plugins));
        let mut page = None;
        let handle = cx.open_window(size(width, px(WINDOW_HEIGHT)), |window, cx| {
            let view = cx.new(|cx| {
                ExportPage::new(
                    registry,
                    project_file,
                    user_file,
                    ExportLoudness::Off,
                    window,
                    cx,
                )
            });
            page = Some(view.clone());
            Root::new(view, window, cx)
        });
        (handle, page.unwrap())
    }

    /// A page over `plugins`, with no presets files.
    fn window_with_dialog(
        cx: &mut TestAppContext,
        plugins: Vec<celesta_media::RegisteredPlugin>,
    ) -> (WindowHandle<Root>, Entity<ExportPage>) {
        window_with_user_file(cx, plugins, None)
    }

    /// [`window_with_dialog`] with the user presets file at `user_file`.
    fn window_with_user_file(
        cx: &mut TestAppContext,
        plugins: Vec<celesta_media::RegisteredPlugin>,
        user_file: Option<PathBuf>,
    ) -> (WindowHandle<Root>, Entity<ExportPage>) {
        window_with_files(cx, plugins, None, user_file)
    }

    /// [`window_with_dialog`] with the project's and the user's presets
    /// files.
    fn window_with_files(
        cx: &mut TestAppContext,
        plugins: Vec<celesta_media::RegisteredPlugin>,
        project_file: Option<PathBuf>,
        user_file: Option<PathBuf>,
    ) -> (WindowHandle<Root>, Entity<ExportPage>) {
        window_with_page(cx, px(WINDOW_WIDTH), plugins, project_file, user_file)
    }

    /// Gives the page the focus, as showing it does.
    fn focus_page(page: &Entity<ExportPage>, cx: &mut VisualTestContext) {
        cx.update(|window, cx| page.focus_handle(cx).focus(window, cx));
        cx.run_until_parked();
    }

    /// A folder of its own under the system's temporary folder.
    fn scratch(name: &str) -> PathBuf {
        crate::scratch::scratch_dir(&format!("export-page-{name}"))
    }

    /// `selector` as [`VisualTestContext::debug_bounds`] takes it.
    fn selector(text: String) -> &'static str {
        Box::leak(text.into_boxed_str())
    }

    #[gpui_kit::test]
    fn a_broken_user_file_shows_under_preset(cx: &mut TestAppContext) {
        let folder = scratch("broken");
        let user_file = folder.join("export-presets.json");
        std::fs::write(&user_file, "{").unwrap();
        let (handle, dialog) = window_with_user_file(
            cx,
            vec![celesta_codec_test::registered()],
            Some(user_file.clone()),
        );
        cx.run_until_parked();
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        cx.update(|_, cx| {
            let dialog = dialog.read(cx);
            assert_eq!(dialog.warned, Section::Preset);
            let [warning] = dialog.model.warnings.as_slice() else {
                panic!("one warning: {:?}", dialog.model.warnings);
            };
            let PageWarning::UnreadableFile { source, message } = warning else {
                panic!("an unreadable file: {warning:?}");
            };
            assert_eq!(*source, PresetSource::User);
            assert!(
                message.starts_with(&user_file.display().to_string())
                    && message.contains("is not valid JSON"),
                "{message}"
            );
            // The first choice is still made, from the defaults.
            assert_eq!(dialog.export_blocked(), None);
        });
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[gpui_kit::test]
    fn a_failed_first_resolve_keeps_the_opening_warnings(cx: &mut TestAppContext) {
        let folder = scratch("broken-no-plugin");
        let user_file = folder.join("export-presets.json");
        std::fs::write(&user_file, "{").unwrap();
        let (handle, dialog) = window_with_user_file(cx, Vec::new(), Some(user_file.clone()));
        cx.run_until_parked();
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        cx.update(|_, cx| {
            let dialog = dialog.read(cx);
            assert_eq!(
                dialog.failure,
                Some((Section::Preset, "no media plugin offers a muxer".to_owned()))
            );
            assert_eq!(dialog.warned, Section::Preset);
            let [warning] = dialog.model.warnings.as_slice() else {
                panic!("one warning: {:?}", dialog.model.warnings);
            };
            assert!(
                matches!(warning, PageWarning::UnreadableFile { message, .. }
                    if message.starts_with(&user_file.display().to_string())),
                "{warning:?}"
            );
        });
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[gpui_kit::test]
    fn warnings_name_no_preset(cx: &mut TestAppContext) {
        let folder = scratch("last");
        let user_file = folder.join("export-presets.json");
        std::fs::write(
            &user_file,
            r#"{"version":1,"last":{"muxer":"celesta-test-codec/file",
                "video":{"component":"celesta-test-codec/video","options":{"nope":1}}},
                "presets":[{"name":"web","muxer":"celesta-test-codec/file",
                "video":{"component":"celesta-test-codec/video","options":{"nope":1}}}]}"#,
        )
        .unwrap();
        let (handle, dialog) =
            window_with_user_file(cx, vec![celesta_codec_test::registered()], Some(user_file));
        cx.run_until_parked();
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        // One line above the warnings says they come from the last export.
        focus_page(&dialog, cx);
        assert!(
            cx.debug_bounds("export-warnings-from-last").is_some(),
            "From your last export: is shown"
        );
        cx.update(|_, cx| {
            let model = &dialog.read(cx).model;
            assert!(!model.warnings.is_empty());
            for warning in &model.warnings {
                let text = model.warning_text(warning);
                assert!(
                    !text.starts_with("preset") && !text.contains("your last export"),
                    "{text}"
                );
            }
            assert_eq!(
                model.warning_text(&model.warnings[0]),
                "Test video encoder has no setting “nope”. It is left out."
            );
        });

        // A named preset's warnings sit under the row naming it: no line.
        cx.update(|window, cx| {
            dialog.update(cx, |dialog, cx| {
                dialog.choose_preset(Some((PresetSource::User, "web".to_owned())), window, cx);
            });
        });
        cx.run_until_parked();
        focus_page(&dialog, cx);
        cx.update(|_, cx| {
            let model = &dialog.read(cx).model;
            assert_eq!(model.selected, Some((PresetSource::User, "web".to_owned())));
            assert!(!model.warnings.is_empty());
        });
        assert!(
            cx.debug_bounds("export-warnings-from-last").is_none(),
            "no context line under a named preset"
        );
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn only_the_last_exports_warnings_go_under_its_line() {
        let stale = PageWarning::Resolve(ResolveWarning::AudioOptionsIgnored {
            base: LAST_EXPORT.to_owned(),
        });
        let preset = PageWarning::Resolve(ResolveWarning::AudioOptionsIgnored {
            base: "preset `web`".to_owned(),
        });
        let unknown = PageWarning::UnknownDefault {
            name: "web".to_owned(),
        };
        let unreadable = PageWarning::UnreadableFile {
            source: PresetSource::User,
            message: "broken".to_owned(),
        };
        let warnings = vec![
            stale.clone(),
            preset.clone(),
            unknown.clone(),
            unreadable.clone(),
        ];
        // The project's default is in no file, so the page opened with the
        // last export: the opening's own warnings are not about it.
        assert_eq!(
            split_last_export(&warnings),
            (vec![&preset, &unknown, &unreadable], vec![&stale])
        );
    }

    /// The test codec's video `input` the page's choice has.
    fn video_input(dialog: &ExportPage) -> Option<SettingValue> {
        let choice = dialog.model.choice()?;
        choice.video.settings.get("input").cloned()
    }

    #[gpui_kit::test]
    fn opens_with_the_project_default_and_switches_presets(cx: &mut TestAppContext) {
        let folder = scratch("project-default");
        let (project_file, user_file) = (
            folder.join("project").join("export-presets.json"),
            folder.join("user").join("export-presets.json"),
        );
        let preset = |name: &str, input: &str| {
            format!(
                r#"{{"name":"{name}","muxer":"celesta-test-codec/file",
                    "video":{{"component":"celesta-test-codec/video",
                    "options":{{"input":"{input}"}}}}}}"#
            )
        };
        std::fs::create_dir_all(project_file.parent().unwrap()).unwrap();
        std::fs::create_dir_all(user_file.parent().unwrap()).unwrap();
        std::fs::write(
            &project_file,
            format!(
                r#"{{"version":1,"default":"web","presets":[{}]}}"#,
                preset("web", "rgba8")
            ),
        )
        .unwrap();
        std::fs::write(
            &user_file,
            format!(r#"{{"version":1,"presets":[{}]}}"#, preset("mine", "nv12")),
        )
        .unwrap();
        let (handle, dialog) = window_with_files(
            cx,
            vec![celesta_codec_test::registered()],
            Some(project_file),
            Some(user_file),
        );
        cx.run_until_parked();
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        let string = |text: &str| Some(SettingValue::String(text.to_owned()));
        cx.update(|_, cx| {
            let dialog = dialog.read(cx);
            assert_eq!(
                dialog.model.selected,
                Some((PresetSource::Project, "web".to_owned()))
            );
            assert_eq!(video_input(dialog), string("rgba8"));
            assert_eq!(
                dialog.preset.read(cx).selected_value(),
                Some(&Some((PresetSource::Project, "web".to_owned())))
            );
        });

        cx.update(|window, cx| {
            dialog.update(cx, |dialog, cx| {
                dialog.choose_preset(Some((PresetSource::User, "mine".to_owned())), window, cx);
            });
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            let dialog = dialog.read(cx);
            assert_eq!(
                dialog.model.selected,
                Some((PresetSource::User, "mine".to_owned()))
            );
            assert_eq!(video_input(dialog), string("nv12"));
        });

        // None goes back to the defaults.
        cx.update(|window, cx| {
            dialog.update(cx, |dialog, cx| dialog.choose_preset(None, window, cx));
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            let dialog = dialog.read(cx);
            assert_eq!(dialog.model.selected, None);
            // No value: the field is at its default.
            assert_eq!(video_input(dialog), None);
        });
        std::fs::remove_dir_all(folder).unwrap();
    }

    /// A plugin named `mediafoundation`, made in this process, with MP4
    /// (H.264) and MOV (ProRes) muxers and their encoders, whose
    /// `describe` says the H.264 encoder is unavailable when `h264_reason`
    /// is given, as on Windows N without the Media Feature Pack.
    fn native_plugin(h264_reason: Option<&'static str>) -> celesta_media::RegisteredPlugin {
        use celesta_media::{ComponentDescription, MediaError, MediaProvider};
        struct Native(Option<&'static str>);
        impl MediaProvider for Native {
            fn describe(&mut self) -> Result<Vec<ComponentDescription>, MediaError> {
                let available = |kind, id: &str| {
                    let mut component = ComponentDescription::unavailable(kind, id, "");
                    component.available = true;
                    component.reason = None;
                    component
                };
                Ok(vec![
                    match self.0 {
                        Some(reason) => ComponentDescription::unavailable(
                            ComponentKind::VideoEncoder,
                            "h264",
                            reason,
                        ),
                        None => available(ComponentKind::VideoEncoder, "h264"),
                    },
                    available(ComponentKind::VideoEncoder, "prores"),
                    available(ComponentKind::Muxer, "mp4"),
                    available(ComponentKind::Muxer, "mov"),
                ])
            }
        }
        let (media, problems) = celesta_plugin_host::MediaManifest::from_json(serde_json::json!({
            "videoEncoders": [
                { "id": "h264", "name": "H.264", "codecs": ["avc1"] },
                { "id": "prores", "name": "ProRes", "codecs": ["ap4h"] },
            ],
            "muxers": [
                { "id": "mp4", "name": "MP4", "extensions": ["mp4"], "video": ["avc1"],
                  "defaultVideo": "avc1" },
                { "id": "mov", "name": "QuickTime", "extensions": ["mov"],
                  "video": ["avc1", "ap4h"], "defaultVideo": "ap4h" },
            ],
        }));
        assert!(problems.is_empty(), "{problems:?}");
        celesta_media::RegisteredPlugin {
            id: "mediafoundation".to_owned(),
            name: "Media Foundation".to_owned(),
            version: "1.0.0".to_owned(),
            origin: celesta_plugin_host::MediaOrigin::Bundled,
            directory: PathBuf::new(),
            executable: None,
            media,
            factory: Arc::new(move |_, _| {
                Ok(Box::new(Native(h264_reason)) as Box<dyn MediaProvider>)
            }),
        }
    }

    #[gpui_kit::test]
    fn an_unavailable_native_h264_opens_on_mov_with_a_warning(cx: &mut TestAppContext) {
        let (handle, dialog) = window_with_dialog(
            cx,
            vec![native_plugin(Some("Media Feature Pack is missing"))],
        );
        cx.run_until_parked();
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        cx.update(|_, cx| {
            let dialog = dialog.read(cx);
            assert_eq!(dialog.failure, None);
            assert_eq!(dialog.model.error, None);
            assert_eq!(dialog.export_blocked(), None);
            assert_eq!(
                dialog.model.choice().map(|choice| &choice.muxer),
                Some(&reference("mediafoundation/mov"))
            );
            assert_eq!(dialog.warned, Section::Preset);
            let [warning] = dialog.model.warnings.as_slice() else {
                panic!("one warning: {:?}", dialog.model.warnings);
            };
            let warning = dialog.model.warning_text(warning);
            assert!(
                warning.starts_with("MP4 cannot be exported here, so the export starts as MOV")
                    && warning.contains("Media Feature Pack is missing")
                    && warning.contains("ffmpeg-x264/libx264"),
                "{warning}"
            );
        });
    }

    #[gpui_kit::test]
    fn an_available_native_h264_opens_on_mp4(cx: &mut TestAppContext) {
        let (handle, dialog) = window_with_dialog(cx, vec![native_plugin(None)]);
        cx.run_until_parked();
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        cx.update(|_, cx| {
            let dialog = dialog.read(cx);
            assert_eq!(
                dialog.model.choice().map(|choice| &choice.muxer),
                Some(&reference("mediafoundation/mp4"))
            );
            assert!(
                dialog.model.warnings.is_empty(),
                "{:?}",
                dialog.model.warnings
            );
        });
    }

    #[gpui_kit::test]
    fn reopening_describes_a_failed_plugin_again(cx: &mut TestAppContext) {
        use std::sync::atomic::{AtomicBool, Ordering};
        // The test codec, failing to start until `up` is set.
        let up = Arc::new(AtomicBool::new(false));
        let mut plugin = celesta_codec_test::registered();
        let factory = plugin.factory.clone();
        plugin.factory = Arc::new({
            let up = up.clone();
            move |kill, using| {
                if up.load(Ordering::SeqCst) {
                    factory(kill, using)
                } else {
                    Err(celesta_media::MediaError::Codec(
                        "the plugin exited".to_owned(),
                    ))
                }
            }
        });
        let (handle, dialog) = window_with_dialog(cx, vec![plugin]);
        cx.run_until_parked();
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        cx.update(|_, cx| {
            let dialog = dialog.read(cx);
            assert!(dialog.export_blocked().is_some(), "nothing can export");
            assert!(dialog.model.choice().is_none());
        });

        up.store(true, Ordering::SeqCst);
        cx.update(|window, cx| dialog.update(cx, |dialog, cx| dialog.reopen(window, cx)));
        cx.run_until_parked();
        cx.update(|_, cx| {
            let dialog = dialog.read(cx);
            assert_eq!(dialog.export_blocked(), None);
            assert_eq!(
                dialog.model.choice().map(|choice| &choice.muxer),
                Some(&reference("celesta-test-codec/file"))
            );
        });
    }

    #[gpui_kit::test]
    fn export_exports_while_the_preset_name_has_the_focus(cx: &mut TestAppContext) {
        let folder = scratch("click-export");
        let user_file = folder.join("export-presets.json");
        let (handle, dialog) = window_with_user_file(
            cx,
            vec![celesta_codec_test::registered()],
            Some(user_file.clone()),
        );
        let exported = Rc::new(RefCell::new(0));
        cx.update(|cx| {
            let exported = exported.clone();
            cx.subscribe(&dialog, move |_, event: &ExportPageEvent, _| {
                if let ExportPageEvent::Export { .. } = event {
                    *exported.borrow_mut() += 1;
                }
            })
            .detach();
        });
        cx.run_until_parked();
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        let open_form = |cx: &mut VisualTestContext| {
            cx.update(|window, cx| {
                dialog.update(cx, |dialog, cx| {
                    dialog.open_save(window, cx);
                    let name = dialog.save.as_ref().unwrap().name.clone();
                    name.update(cx, |name, cx| name.set_value("Web", window, cx));
                });
            });
            cx.run_until_parked();
            cx.update(|window, cx| {
                let name = dialog.read(cx).save.as_ref().unwrap().name.clone();
                assert!(name.focus_handle(cx).is_focused(window));
            });
        };

        // A click on Export… does not take the focus from the name; it
        // still exports, and saves nothing.
        open_form(cx);
        let export = cx
            .debug_bounds("export-page-export")
            .expect("the Export… button");
        cx.simulate_click(export.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(*exported.borrow(), 1);
        assert!(!user_file.exists());

        // Enter in the name saves, and does not export.
        dialog.update_in(cx, |dialog, _, cx| {
            dialog.save = None;
            cx.notify();
        });
        open_form(cx);
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(*exported.borrow(), 1);
        let saved = PresetsFile::load(&user_file).unwrap();
        assert_eq!(saved.presets.len(), 1);
        assert_eq!(saved.presets[0].name, "Web");
        cx.update(|_, cx| {
            let dialog = dialog.read(cx);
            assert!(dialog.save.is_none());
            assert_eq!(
                dialog.model.selected,
                Some((PresetSource::User, "Web".to_owned()))
            );
        });
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[gpui_kit::test]
    fn save_as_preset_waits_for_what_blocks_export(cx: &mut TestAppContext) {
        let folder = scratch("save");
        let (handle, dialog) = window_with_user_file(
            cx,
            vec![celesta_codec_test::registered()],
            Some(folder.join("export-presets.json")),
        );
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        cx.update(|window, cx| {
            dialog.update(cx, |dialog, cx| {
                dialog.open_save(window, cx);
                let name = dialog.save.as_ref().unwrap().name.clone();
                name.update(cx, |name, cx| name.set_value("Web", window, cx));
                // The first choice is still being resolved.
                assert_eq!(dialog.export_blocked().as_deref(), Some(CHECKING));
                assert_eq!(dialog.save_request(cx), None);
            });
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            assert!(dialog.read(cx).save_request(cx).is_some());
            dialog.update(cx, |dialog, cx| {
                dialog.advanced[Track::Audio as usize] = true;
                cx.notify();
            });
            let delay = dialog
                .read(cx)
                .fields
                .iter()
                .find_map(|control| match &control.widget {
                    Widget::Text { input, .. } if control.key == "delay" => Some(input.clone()),
                    _ => None,
                })
                .expect("the audio encoder's delay field");
            delay.update(cx, |input, cx| input.focus(window, cx));
        });
        cx.simulate_input("1x");
        cx.run_until_parked();
        cx.update(|window, cx| {
            assert_eq!(dialog.read(cx).save_request(cx), None);
            // `Save` does nothing while the typo stands.
            dialog.update(cx, |dialog, cx| dialog.save_preset(window, cx));
            assert!(dialog.read(cx).save.is_some());
            assert!(!folder.join("export-presets.json").exists());
        });
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[gpui_kit::test]
    fn without_a_media_plugin_export_says_why(cx: &mut TestAppContext) {
        let (handle, dialog) = window_with_dialog(cx, Vec::new());
        cx.run_until_parked();
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        cx.update(|_, cx| {
            assert_eq!(
                dialog.read(cx).export_blocked().as_deref(),
                Some("no media plugin offers a muxer")
            );
            assert!(dialog.read(cx).model.choice().is_none());
        });
    }

    #[gpui_kit::test]
    fn a_typed_number_that_is_not_one_blocks_export(cx: &mut TestAppContext) {
        let (handle, dialog) = window_with_dialog(cx, vec![celesta_codec_test::registered()]);
        cx.run_until_parked();
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        cx.update(|_, cx| {
            // The delay is one of the audio encoder's advanced fields.
            dialog.update(cx, |dialog, cx| {
                dialog.advanced[Track::Audio as usize] = true;
                cx.notify();
            });
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            let delay = dialog
                .read(cx)
                .fields
                .iter()
                .find_map(|control| match &control.widget {
                    Widget::Text { input, .. } if control.key == "delay" => Some(input.clone()),
                    _ => None,
                })
                .expect("the audio encoder's delay field");
            delay.update(cx, |input, cx| input.focus(window, cx));
        });
        cx.simulate_input("1x");
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(
                dialog.read(cx).export_blocked().as_deref(),
                Some("Delay: Enter a number")
            );
        });
        cx.simulate_keystrokes("backspace");
        cx.run_until_parked();
        cx.update(|_, cx| {
            let dialog = dialog.read(cx);
            assert_eq!(dialog.export_blocked(), None);
            let choice = dialog.model.output_choice().unwrap();
            let celesta_exporter::AudioChoice::Encode(audio) = choice.audio else {
                panic!("the audio is encoded");
            };
            assert_eq!(
                audio.options.get("delay"),
                Some(&SettingInput::Value(SettingValue::Number(1.0)))
            );
        });
    }

    #[gpui_kit::test]
    fn a_typo_left_behind_is_replaced_by_the_fields_value(cx: &mut TestAppContext) {
        let (handle, dialog) = window_with_dialog(cx, vec![celesta_codec_test::registered()]);
        cx.run_until_parked();
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        cx.update(|_, cx| {
            dialog.update(cx, |dialog, cx| {
                dialog.advanced[Track::Audio as usize] = true;
                cx.notify();
            });
        });
        cx.run_until_parked();
        let delay = cx.update(|window, cx| {
            let delay = dialog
                .read(cx)
                .fields
                .iter()
                .find_map(|control| match &control.widget {
                    Widget::Text { input, .. } if control.key == "delay" => Some(input.clone()),
                    _ => None,
                })
                .expect("the audio encoder's delay field");
            delay.update(cx, |input, cx| input.focus(window, cx));
            delay
        });
        cx.simulate_input("1x");
        cx.run_until_parked();
        cx.update(|window, cx| {
            assert_eq!(
                dialog.read(cx).export_blocked().as_deref(),
                Some("Delay: Enter a number")
            );
            // The field loses the focus, and another field's change syncs
            // every control to the model.
            window.blur(cx);
            dialog.update(cx, |dialog, cx| {
                dialog.set_setting(
                    Track::Video,
                    "input",
                    SettingValue::String("rgba8".to_owned()),
                    window,
                    cx,
                );
            });
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            // "1" was a number and was taken; "1x" was not.
            assert_eq!(delay.read(cx).value().as_ref(), "1");
            assert_eq!(dialog.read(cx).export_blocked(), None);
        });
    }

    #[gpui_kit::test]
    fn enter_in_the_page_exports(cx: &mut TestAppContext) {
        let (handle, page) = window_with_dialog(cx, vec![celesta_codec_test::registered()]);
        let exported = Rc::new(RefCell::new(Vec::new()));
        cx.update(|cx| {
            let exported = exported.clone();
            cx.subscribe(&page, move |_, event: &ExportPageEvent, _| {
                if let ExportPageEvent::Export {
                    choice, extension, ..
                } = event
                {
                    exported
                        .borrow_mut()
                        .push((choice.clone(), extension.clone()));
                }
            })
            .detach();
        });
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);

        // Enter does nothing while the plugin is being described and the
        // first choice resolved. The key is handled before the executor
        // runs the resolve.
        cx.update(|window, cx| {
            page.focus_handle(cx).focus(window, cx);
            window.draw(cx).clear(cx);
            let reason = page.read(cx).export_blocked();
            assert_eq!(reason.as_deref(), Some(CHECKING));
        });
        cx.simulate_keystrokes("enter");
        assert!(
            exported.borrow().is_empty(),
            "nothing exports while Checking…"
        );

        cx.run_until_parked();
        cx.update(|_, cx| {
            let page = page.read(cx);
            assert_eq!(page.export_blocked(), None);
            // The container list shows the test codec's muxer, chosen.
            let muxer = page.muxer.read(cx);
            assert_eq!(
                muxer.selected_value(),
                Some(&reference("celesta-test-codec/file"))
            );
        });
        cx.update(|window, cx| {
            // The test codec's video field, set as its Select sets it.
            page.update(cx, |page, cx| {
                page.set_setting(
                    Track::Video,
                    "input",
                    SettingValue::String("rgba8".to_owned()),
                    window,
                    cx,
                );
            });
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            let input = page
                .read(cx)
                .fields
                .iter()
                .find(|control| control.key == "input")
                .map(|control| match &control.widget {
                    Widget::Select(select) => select.read(cx).selected_value().cloned(),
                    _ => None,
                });
            assert_eq!(input, Some(Some("rgba8".to_owned())));
        });

        // Enter in the focused page runs Export….
        focus_page(&page, cx);
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        let exported = exported.borrow();
        let [(choice, extension)] = exported.as_slice() else {
            panic!("one export: {} emitted", exported.len());
        };
        assert_eq!(extension, "mp4");
        assert_eq!(choice.muxer, Some(reference("celesta-test-codec/file")));
        assert_eq!(
            choice.video.component,
            Some(reference("celesta-test-codec/video"))
        );
        assert_eq!(
            choice.video.options.get("input"),
            Some(&SettingInput::Value(SettingValue::String(
                "rgba8".to_owned()
            )))
        );
    }

    #[gpui_kit::test]
    fn export_waits_for_the_save_panel(cx: &mut TestAppContext) {
        let (handle, page) = window_with_dialog(cx, vec![celesta_codec_test::registered()]);
        let exported = Rc::new(RefCell::new(0));
        cx.update(|cx| {
            let exported = exported.clone();
            cx.subscribe(&page, move |_, event: &ExportPageEvent, _| {
                if let ExportPageEvent::Export { .. } = event {
                    *exported.borrow_mut() += 1;
                }
            })
            .detach();
        });
        cx.run_until_parked();
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        page.update_in(cx, |page, _, cx| page.set_waiting(true, cx));
        focus_page(&page, cx);
        cx.update(|_, cx| {
            assert_eq!(
                page.read(cx).export_blocked().as_deref(),
                Some(WAITING),
                "Export… is disabled while the save panel is up"
            );
        });
        let export = cx
            .debug_bounds("export-page-export")
            .expect("the Export… button");
        cx.simulate_click(export.center(), Modifiers::none());
        focus_page(&page, cx);
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(*exported.borrow(), 0, "nothing exports while waiting");

        // The panel closed, a path chosen or not: Enter exports again.
        page.update_in(cx, |page, _, cx| page.set_waiting(false, cx));
        focus_page(&page, cx);
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(*exported.borrow(), 1);
    }

    #[gpui_kit::test]
    fn the_close_button_emits_close(cx: &mut TestAppContext) {
        let (handle, page) = window_with_dialog(cx, vec![celesta_codec_test::registered()]);
        let closed = Rc::new(RefCell::new(0));
        cx.update(|cx| {
            let closed = closed.clone();
            cx.subscribe(&page, move |_, event: &ExportPageEvent, _| {
                if let ExportPageEvent::Close = event {
                    *closed.borrow_mut() += 1;
                }
            })
            .detach();
        });
        cx.run_until_parked();
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        let close = cx
            .debug_bounds("export-page-close")
            .expect("the header's close button");
        cx.simulate_click(close.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(*closed.borrow(), 1);
    }

    #[gpui_kit::test]
    fn loudness_is_disabled_without_audio(cx: &mut TestAppContext) {
        let (handle, page) = window_with_dialog(cx, vec![celesta_codec_test::registered()]);
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        // While the first choice is being resolved nothing is known to
        // have no audio: no note.
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            assert!(page.read(cx).model.choice().is_none());
        });
        assert!(
            cx.debug_bounds("export-control-loudness").is_some(),
            "the loudness row is drawn"
        );
        assert!(
            cx.debug_bounds("export-note-loudness").is_none(),
            "no note before there is a choice"
        );
        cx.run_until_parked();
        cx.update(|window, cx| {
            page.update(cx, |page, cx| {
                page.choose_encoder(Track::Audio, None, window, cx);
            });
        });
        focus_page(&page, cx);
        assert!(
            cx.debug_bounds("export-note-loudness").is_some(),
            "No audio is exported."
        );
        // A click on the disabled list neither opens it nor takes the
        // focus from the page.
        let loudness = cx
            .debug_bounds("export-control-loudness")
            .expect("the loudness row");
        cx.simulate_click(loudness.center(), Modifiers::none());
        cx.run_until_parked();
        cx.update(|window, cx| assert!(page.focus_handle(cx).is_focused(window)));

        cx.update(|window, cx| {
            page.update(cx, |page, cx| {
                page.choose_encoder(
                    Track::Audio,
                    Some(reference("celesta-test-codec/pcm")),
                    window,
                    cx,
                );
            });
        });
        focus_page(&page, cx);
        assert!(cx.debug_bounds("export-note-loudness").is_none());
        let loudness = cx
            .debug_bounds("export-control-loudness")
            .expect("the loudness row");
        cx.simulate_click(loudness.center(), Modifiers::none());
        cx.run_until_parked();
        cx.update(|window, cx| {
            assert!(
                !page.focus_handle(cx).is_focused(window),
                "the enabled list takes the click"
            );
        });
    }

    /// Puts the test codec's video `input` in a "Rate control" group, as
    /// a plugin describing it so would.
    fn group_the_video_input(page: &Entity<ExportPage>, cx: &mut VisualTestContext) {
        cx.update(|window, cx| {
            page.update(cx, |page, cx| {
                let mut description = (*page.registry.describe(0, &page.kill)).clone();
                let video = description
                    .components
                    .iter_mut()
                    .find(|component| component.kind == ComponentKind::VideoEncoder)
                    .expect("the test video encoder");
                for field in &mut video.settings {
                    if field.key == "input" {
                        field.group = Some("Rate control".to_owned());
                    }
                }
                page.model.described(0, Arc::new(description));
                page.advanced[Track::Video as usize] = true;
                page.sync(window, cx);
            });
        });
        cx.run_until_parked();
    }

    #[gpui_kit::test]
    fn labels_share_one_edge(cx: &mut TestAppContext) {
        let (handle, page) = window_with_dialog(cx, vec![celesta_codec_test::registered()]);
        cx.run_until_parked();
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        group_the_video_input(&page, cx);
        let left = |cx: &mut VisualTestContext, selector: &'static str| {
            cx.debug_bounds(selector)
                .unwrap_or_else(|| panic!("{selector} is drawn"))
                .left()
        };
        let encoder = left(cx, "export-label-video-encoder");
        assert_eq!(left(cx, "export-group-video-Rate control"), encoder);
        assert_eq!(left(cx, "export-label-video-input"), encoder);
        // The disclosure's icon starts on the labels' edge: the button has
        // no leading padding.
        assert_eq!(left(cx, "export-video-advanced-icon"), encoder);
        assert_eq!(left(cx, "export-label-video-alpha"), encoder);
        assert_eq!(left(cx, "export-label-preset"), encoder);
        // Lines under a control start at its lane.
        assert_eq!(
            left(cx, "export-description-video-input"),
            left(cx, "export-control-video-input")
        );
    }

    #[gpui_kit::test]
    fn controls_fit_the_narrowest_panel(cx: &mut TestAppContext) {
        // The Export panel's narrowest: 22rem at the editor's 13px `rem`.
        // physical: a test window's size, not a layout length.
        let width = px(22. * celesta_editor_theme::metrics::BASE_FONT_SIZE);
        let (handle, page) = window_with_page(
            cx,
            width,
            vec![celesta_codec_test::registered()],
            None,
            None,
        );
        cx.run_until_parked();
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        group_the_video_input(&page, cx);
        cx.update(|window, cx| {
            page.update(cx, |page, cx| {
                page.advanced = [true; 2];
                page.set_range(Some("00:00:01:00 – 00:00:02:00".into()), true, cx);
                page.open_save(window, cx);
            });
        });
        cx.run_until_parked();
        let mut ids: Vec<String> = [
            "preset",
            "container",
            "range",
            "video-encoder",
            "audio-encoder",
            "loudness",
            "save-name",
            "save-target",
        ]
        .map(str::to_owned)
        .to_vec();
        cx.update(|_, cx| {
            ids.extend(
                page.read(cx)
                    .fields
                    .iter()
                    .map(|control| format!("{}-{}", track_id(control.track), control.key)),
            );
        });
        assert!(ids.len() > 8, "the encoders' fields are listed: {ids:?}");
        for id in ids {
            // The marker after the control: where the control ends.
            let end = cx
                .debug_bounds(selector(format!("export-control-end-{id}")))
                .unwrap_or_else(|| panic!("export-control-end-{id} is drawn"))
                .left();
            assert!(
                end <= width,
                "export-control-{id} ends at {end:?}, past {width:?}"
            );
        }
        let clear = cx
            .debug_bounds("export-range-clear")
            .expect("the range's Clear");
        assert!(
            clear.right() <= width,
            "Clear ends at {:?}, past {width:?}",
            clear.right()
        );
    }
}
