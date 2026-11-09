//! The export dialog: what Export… shows before the save panel. From top
//! to bottom it holds the preset, the container, the video encoder and its
//! fields, the audio encoder (or none) and its fields, the range and the
//! loudness. The footer holds `Save as preset…`, `Cancel`, and `Export…`,
//! and above them, once asked for, the inline "Save as preset…" form.
//!
//! The view owns the controls and the work in flight; what they show and
//! edit is [`ExportDialogModel`]'s. A preset or container change is resolved
//! on the background executor, each plugin's `describe` too.

use super::model::{CHECKING, ExportDialogModel, InitialBase, PresetFileState, Track};
use crate::helpers::ExportLoudness;
use celesta_exporter::{
    OutputChoice, Preset, PresetOutput, PresetSource, PresetsFile, resolve_preset_output,
};
use celesta_media::{ComponentKind, ComponentRef, MediaRegistry, SettingValue};
use celesta_plugin_host::PluginKillSwitch;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::collapsible::Collapsible;
use gpui_kit::component::dialog::{DialogAction, DialogClose, DialogFooter};
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, IndexPath, Sizable as _, WindowExt as _,
    h_flex, v_flex,
};
use gpui_kit::prelude::*;
use gpui_kit::{
    App, Context, Entity, EventEmitter, Focusable as _, FontWeight, Hsla, Pixels, SharedString,
    Subscription, Task, Window, div, px,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Wide enough for a component's label and its plugin beside a field.
const EXPORT_DIALOG_WIDTH: Pixels = px(560.);

mod choice;
mod fields;
mod save;

use choice::{Choice, Sections, component_choice, sync_select};
use fields::{FieldControl, Widget};
use save::{SaveForm, save_section};

/// What the dialog tells its owner.
#[expect(
    clippy::large_enum_variant,
    reason = "one event per click, matched by reference where it arrives"
)]
pub(crate) enum ExportDialogEvent {
    /// `Export…`: the choice, the muxer's first extension for the save
    /// panel, and the choice as `last` saves it.
    Export {
        choice: OutputChoice,
        extension: String,
        last: PresetOutput,
    },
    /// The loudness was changed, for the export and the next dialog.
    Loudness(ExportLoudness),
    /// The range's `Clear`: drop the in- and out-points.
    ClearRange,
}

/// The part of the dialog a resolve was asked from, under which its
/// warnings or failure show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Section {
    Preset,
    Container,
}

/// A preset in the list: `None` for "None", the defaults.
type PresetKey = Option<(PresetSource, String)>;

pub(crate) struct ExportDialog {
    model: ExportDialogModel,
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
    save: Option<SaveForm>,
    /// The running resolve; replacing it drops a stale one.
    resolving: Option<Task<()>>,
    _describing: Vec<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<ExportDialogEvent> for ExportDialog {}

impl Drop for ExportDialog {
    fn drop(&mut self) {
        self.kill.kill();
    }
}

/// A presets file as the dialog reads it: `None` when there is no path.
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

impl ExportDialog {
    /// A dialog over `registry`'s media plugins, with the presets of
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
        let model = ExportDialogModel::new(
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
                    cx.emit(ExportDialogEvent::Loudness(*loudness));
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
        let describing = plugins
            .into_iter()
            .map(|plugin| {
                let (registry, kill) = (registry.clone(), kill.clone());
                let described = cx
                    .background_executor()
                    .spawn(async move { registry.describe(plugin, &kill) });
                cx.spawn_in(window, async move |this, cx| {
                    let description = described.await;
                    this.update_in(cx, |this, window, cx| {
                        this.model.described(plugin, description);
                        this.sync(window, cx);
                    })
                    .ok();
                })
            })
            .collect();

        let mut this = Self {
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
            save: None,
            resolving: None,
            _describing: describing,
            _subscriptions: subscriptions,
        };
        let (base, warnings) = this.model.initial_base();
        let (selected, base) = match base {
            InitialBase::Preset(source, name, output) => {
                (Some((source, name.clone())), Some((name, output)))
            }
            InitialBase::Last(output) => (None, Some(("last".to_owned(), output))),
            InitialBase::None => (None, None),
        };
        let output = this
            .model
            .initial_output(base.as_ref().map(|(_, output)| output));
        this.resolve(
            OutputChoice::default(),
            base,
            output,
            selected,
            Section::Preset,
            warnings,
            window,
            cx,
        );
        this.sync(window, cx);
        this
    }

    /// The project presets file this dialog read; `None` for an untitled
    /// document.
    pub(crate) fn project_file(&self) -> Option<&Path> {
        self.model.project.path.as_deref()
    }

    /// Shows `range` (the in- and out-points' timecodes) as what is
    /// exported; `None` is the whole composition.
    pub(crate) fn set_range(&mut self, range: Option<SharedString>, cx: &mut Context<Self>) {
        self.range = range;
        cx.notify();
    }

    /// Resolves `choice` over `base` (its name for warnings, and the
    /// preset) for `output` in the background, and applies the result with
    /// `selected` and `warnings` added; a failure is shown under `section`
    /// and keeps the previous choice.
    #[allow(clippy::too_many_arguments)]
    fn resolve(
        &mut self,
        choice: OutputChoice,
        base: Option<(String, PresetOutput)>,
        output: PathBuf,
        selected: Option<(PresetSource, String)>,
        section: Section,
        warnings: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (registry, kill) = (self.registry.clone(), self.kill.clone());
        let resolved = cx.background_executor().spawn(async move {
            let base = base.as_ref().map(|(name, output)| (name.as_str(), output));
            resolve_preset_output(&choice, base, &output, &registry, &mut |plugin| {
                registry.describe(plugin, &kill)
            })
        });
        self.resolving = Some(cx.spawn_in(window, async move |this, cx| {
            let resolved = resolved.await;
            this.update_in(cx, |this, window, cx| {
                this.resolving = None;
                match resolved {
                    Ok(resolved) => {
                        this.model.apply(&resolved, selected);
                        // `apply` replaces the warnings with the resolve's.
                        this.model.warnings.extend(warnings);
                        this.warned = section;
                        this.failure = None;
                    }
                    Err(error) => {
                        let message = error.to_string();
                        if this.model.choice().is_none() {
                            this.model.error = Some(message.clone());
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
            Some((name.clone(), preset.output.clone()))
        });
        let output = self
            .model
            .initial_output(base.as_ref().map(|(_, output)| output));
        let selected = key.filter(|_| base.is_some());
        self.resolve(
            OutputChoice::default(),
            base,
            output,
            selected,
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
            output,
            None,
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
    /// resolve still running, or what the model says.
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
        self.model.output_choice().err()
    }

    /// Enter or `Export…`: saves the preset when its name has the focus,
    /// else exports. `true` closes the dialog.
    fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self
            .save
            .as_ref()
            .is_some_and(|save| save.name.focus_handle(cx).is_focused(window))
        {
            self.save_preset(window, cx);
            return false;
        }
        self.export(cx)
    }

    /// Emits [`ExportDialogEvent::Export`] when the choice can be
    /// exported.
    fn export(&mut self, cx: &mut Context<Self>) -> bool {
        if self.export_blocked().is_some() {
            return false;
        }
        let (Ok(choice), Some(extension)) = (self.model.output_choice(), self.model.extension())
        else {
            return false;
        };
        cx.emit(ExportDialogEvent::Export {
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
        .text_xs()
        .text_color(color)
        .child(
            Icon::new(IconName::TriangleAlert)
                .xsmall()
                .text_color(color)
                .flex_none(),
        )
        .child(div().min_w_0().child(text.into()))
}

fn muted_line(text: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    div()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}

/// A section below a hairline, as far from it as from the section above.
fn separated(section: impl IntoElement, border: Hsla) -> impl IntoElement {
    div()
        .pt_6()
        .border_t_1()
        .border_color(border)
        .child(section)
}

/// A section's label above its control.
fn section_label(label: &'static str, cx: &App) -> impl IntoElement {
    div()
        .text_sm()
        .font_weight(FontWeight::MEDIUM)
        .text_color(cx.theme().foreground)
        .child(label)
}

impl ExportDialog {
    fn failure_under(&self, section: Section, cx: &App) -> Option<impl IntoElement> {
        let (failed, message) = self.failure.as_ref()?;
        (*failed == section).then(|| problem_line(message.clone(), cx.theme().danger))
    }

    fn warnings_under(&self, section: Section, cx: &App) -> Vec<impl IntoElement> {
        if self.warned != section {
            return Vec::new();
        }
        self.model
            .warnings
            .iter()
            .map(|warning| problem_line(warning.clone(), cx.theme().warning))
            .collect()
    }

    fn render_preset(&self, cx: &App) -> impl IntoElement {
        v_flex()
            .gap_1()
            .child(section_label("Preset", cx))
            .child(Select::new(&self.preset).id("export-preset"))
            .children(self.warnings_under(Section::Preset, cx))
            .children(self.failure_under(Section::Preset, cx))
    }

    fn render_container(&self, cx: &App) -> impl IntoElement {
        v_flex()
            .gap_1()
            .child(section_label("Container", cx))
            .child(
                Select::new(&self.muxer)
                    .id("export-container")
                    .placeholder(CHECKING),
            )
            .children(self.warnings_under(Section::Container, cx))
            .children(self.failure_under(Section::Container, cx))
    }

    /// `track`'s encoder list and its fields: ordinary ones first, by
    /// group, then the advanced ones under a closed "Advanced".
    fn render_encoder(&self, track: Track, cx: &mut Context<Self>) -> impl IntoElement {
        let (label, select) = match track {
            Track::Video => (
                "Video encoder",
                Select::new(&self.video)
                    .id("export-video-encoder")
                    .placeholder(CHECKING)
                    .into_any_element(),
            ),
            Track::Audio => (
                "Audio encoder",
                Select::new(&self.audio)
                    .id("export-audio-encoder")
                    .into_any_element(),
            ),
        };
        let open = self.advanced[track as usize];
        let advanced = self.model.fields(track, true);
        let toggle = format!("export-{}-advanced", track_id(track));
        v_flex()
            .gap_3()
            .child(
                v_flex()
                    .gap_1()
                    .child(section_label(label, cx))
                    .child(select),
            )
            .children(self.render_groups(track, false, cx))
            .when(!advanced.is_empty(), |section| {
                section.child(
                    Collapsible::new()
                        .gap_3()
                        .open(open)
                        .child(
                            h_flex().child(
                                Button::new(SharedString::from(toggle))
                                    .ghost()
                                    .small()
                                    .icon(if open {
                                        IconName::ChevronDown
                                    } else {
                                        IconName::ChevronRight
                                    })
                                    .label("Advanced")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.advanced[track as usize] = !open;
                                        cx.notify();
                                    })),
                            ),
                        )
                        .content(
                            v_flex()
                                .gap_3()
                                .children(self.render_groups(track, true, cx)),
                        ),
                )
            })
    }

    fn render_range_and_loudness(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let range = match &self.range {
            Some(range) => h_flex()
                .gap_2()
                .child(div().text_sm().child(range.clone()))
                .child(
                    Button::new("export-range-clear")
                        .ghost()
                        .small()
                        .label("Clear")
                        .on_click(cx.listener(|_, _, _, cx| {
                            cx.emit(ExportDialogEvent::ClearRange);
                        })),
                )
                .into_any_element(),
            None => div()
                .text_sm()
                .child("Whole composition")
                .into_any_element(),
        };
        v_flex()
            .gap_4()
            .child(
                v_flex()
                    .gap_1()
                    .child(section_label("Range", cx))
                    .child(range),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(section_label("Loudness", cx))
                    .child(Select::new(&self.loudness).id("export-loudness")),
            )
    }
}

impl Render for ExportDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let border = cx.theme().border;
        v_flex()
            .id("export-dialog")
            .gap_6()
            .pb_1()
            .child(
                v_flex()
                    .gap_4()
                    .child(self.render_preset(cx))
                    .child(self.render_container(cx)),
            )
            .child(separated(self.render_encoder(Track::Video, cx), border))
            .child(separated(self.render_encoder(Track::Audio, cx), border))
            .child(separated(self.render_range_and_loudness(cx), border))
    }
}

/// The footer: the open "Save as preset…" form, then `Save as preset…`
/// leading, `Cancel` and `Export…` (the Enter default) trailing, and why
/// `Export…` is disabled under them.
fn footer(dialog: &Entity<ExportDialog>, cx: &App) -> impl IntoElement {
    let this = dialog.read(cx);
    let blocked = this.export_blocked();
    let disabled = blocked.is_some();
    let saving = this.save.is_some();
    let reason = blocked.map(|reason| {
        let checking = reason.ends_with(CHECKING);
        h_flex().justify_end().child(if checking {
            muted_line(reason, cx).into_any_element()
        } else {
            problem_line(reason, cx.theme().danger).into_any_element()
        })
    });
    let open_save = dialog.downgrade();
    v_flex()
        .w_full()
        .gap_3()
        .children(save_section(dialog, cx))
        .child(
            DialogFooter::new()
                .justify_between()
                .child(
                    Button::new("export-save-as")
                        .outline()
                        .label("Save as preset…")
                        .disabled(saving)
                        .on_click(move |_, window, cx| {
                            open_save
                                .update(cx, |this, cx| this.open_save(window, cx))
                                .ok();
                        }),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            DialogClose::new().child(
                                Button::new("export-dialog-cancel")
                                    .outline()
                                    .label("Cancel"),
                            ),
                        )
                        .child(
                            DialogAction::new().child(
                                Button::new("export-dialog-export")
                                    .primary()
                                    .label("Export…")
                                    .disabled(disabled),
                            ),
                        ),
                ),
        )
        .children(reason)
}

/// Shows `dialog` as the window's dialog. Escape and `Cancel` close it and
/// keep its state; `Export…` (or Enter) emits its choice and closes it.
pub(crate) fn open_export_dialog(dialog: &Entity<ExportDialog>, window: &mut Window, cx: &mut App) {
    let dialog = dialog.downgrade();
    window.open_dialog(cx, move |shell, _, cx| {
        let Some(dialog) = dialog.upgrade() else {
            return shell;
        };
        let confirm = dialog.clone();
        shell
            .title("Export")
            .w(EXPORT_DIALOG_WIDTH)
            .footer(footer(&dialog, cx))
            .child(dialog)
            .on_ok(move |_, window, cx| confirm.update(cx, |this, cx| this.confirm(window, cx)))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_exporter::SettingInput;
    use gpui_kit::component::Root;
    use gpui_kit::{TestAppContext, VisualTestContext, WindowHandle, size};
    use std::cell::RefCell;
    use std::rc::Rc;

    /// What the window shows under the dialog.
    struct Blank;

    impl Render for Blank {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full()
        }
    }

    fn reference(text: &str) -> ComponentRef {
        text.parse().unwrap()
    }

    /// A window holding a dialog over `plugins`, with no presets files.
    fn window_with_dialog(
        cx: &mut TestAppContext,
        plugins: Vec<celesta_media::RegisteredPlugin>,
    ) -> (WindowHandle<Root>, Entity<ExportDialog>) {
        cx.update(gpui_kit::init);
        let registry = Arc::new(MediaRegistry::new(plugins));
        let mut dialog = None;
        let handle = cx.open_window(size(px(800.), px(1000.)), |window, cx| {
            dialog = Some(cx.new(|cx| {
                ExportDialog::new(registry, None, None, ExportLoudness::Off, window, cx)
            }));
            let blank = cx.new(|_| Blank);
            Root::new(blank, window, cx)
        });
        (handle, dialog.unwrap())
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
        cx.update(|window, cx| {
            open_export_dialog(&dialog, window, cx);
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
        cx.update(|window, cx| {
            open_export_dialog(&dialog, window, cx);
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
    fn export_dialog_emits_the_chosen_output(cx: &mut TestAppContext) {
        let (handle, dialog) = window_with_dialog(cx, vec![celesta_codec_test::registered()]);
        let exported = Rc::new(RefCell::new(Vec::new()));
        cx.update(|cx| {
            let exported = exported.clone();
            cx.subscribe(&dialog, move |_, event: &ExportDialogEvent, _| {
                if let ExportDialogEvent::Export {
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

        // Enter, the dialog's default, does nothing while the plugin is
        // being described and the first choice resolved.
        cx.update(|window, cx| open_export_dialog(&dialog, window, cx));
        cx.update(|window, cx| {
            let reason = dialog.read(cx).export_blocked();
            assert_eq!(reason.as_deref(), Some(CHECKING));
            dialog.update(cx, |dialog, cx| assert!(!dialog.confirm(window, cx)));
        });
        assert!(exported.borrow().is_empty());

        cx.run_until_parked();
        cx.update(|_, cx| {
            let dialog = dialog.read(cx);
            assert_eq!(dialog.export_blocked(), None);
            // The container list shows the test codec's muxer, chosen.
            let muxer = dialog.muxer.read(cx);
            assert_eq!(
                muxer.selected_value(),
                Some(&reference("celesta-test-codec/file"))
            );
        });
        cx.update(|window, cx| {
            // The test codec's video field, set as its Select sets it.
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
            let input = dialog
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

        // Enter dispatches the dialog's confirm, as a click on Export… does.
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
        cx.update(|window, cx| {
            assert!(!window.has_active_dialog(cx), "Export… closes the dialog");
        });
    }
}
