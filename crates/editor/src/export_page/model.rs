//! What the export page shows and edits, without GPUI: the media
//! components and what each plugin's `describe` said of them, the chosen
//! muxer and encoders with the settings given for them, and the presets
//! files.
//!
//! A preset or a container change is resolved by the view through
//! `resolve_preset_output` and handed to [`ExportModel::apply`];
//! encoder and field edits change the choice here.

use super::wording::PageWarning;
use celesta_exporter::{
    AudioChoice, EncoderChoice, ExportError, NoAudio, OutputChoice, Preset, PresetAudio,
    PresetEncoder, PresetOutput, PresetSource, PresetsFile, ResolvedEncoder, ResolvedOutput,
    SettingInput, serves_video, writes_any_codec,
};
use celesta_media::settings::{
    differences, drop_hidden, effective, is_visible, sanitize, validate,
};
use celesta_media::{
    ComponentDescription, ComponentEntry, ComponentKind, ComponentRef, MediaComponentInfo,
    MediaRegistry, PluginDescription, RegisteredPlugin, SettingField, SettingValue, Settings,
    default_rank,
};
use celesta_plugin_host::{MediaMuxerEntry, MediaOrigin};
use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// What a component shows while its plugin's `describe` runs.
pub(crate) const CHECKING: &str = "Checking…";

/// The resolution error and its recovery action for the page's existing
/// inline error message.
pub(super) fn resolution_error(error: &ExportError) -> String {
    let mut message = error.to_string();
    if matches!(
        error,
        ExportError::ComponentUnavailable { .. }
            | ExportError::ComponentNotInstalled { .. }
            | ExportError::NoDefaultEncoder { .. }
    ) && let Some(hint) =
        celesta_exporter::report::hint(error, celesta_exporter::report::HintStyle::Mcp)
    {
        message.push_str(". ");
        message.push_str(&hint);
    }
    message
}

/// Resolves the first of `outputs` that `resolve` can, for an export
/// left to the defaults: the output it took, and when that is not the
/// first, a warning saying why the first could not be used and how to
/// make it usable. When none resolves, the first output's error, which
/// is the one the user would act on.
pub(super) fn resolve_first_available<R>(
    outputs: &[PathBuf],
    mut resolve: impl FnMut(&Path) -> Result<R, ExportError>,
) -> Result<(R, PathBuf, Option<PageWarning>), ExportError> {
    let mut first_error = None;
    for output in outputs {
        match resolve(output) {
            Ok(resolved) => {
                let warning = first_error.as_ref().map(|error| PageWarning::PassedOver {
                    first: format_name(&outputs[0]),
                    used: format_name(output),
                    reason: resolution_error(error),
                });
                return Ok((resolved, output.clone(), warning));
            }
            Err(error) => {
                first_error.get_or_insert(error);
            }
        }
    }
    Err(first_error.expect("at least one output is tried"))
}

/// `export.mp4` as "MP4": the output's extension, in capitals.
fn format_name(output: &Path) -> String {
    output.extension().map_or_else(String::new, |extension| {
        extension.to_string_lossy().to_uppercase()
    })
}

/// The reason given for an unavailable component the plugin gave none for
/// (as the registry words it).
const NO_REASON: &str = "the plugin gave no reason";

/// The reason given for a component its plugin did not describe.
const NOT_DESCRIBED: &str = "the plugin did not describe it";

/// An export's video or audio.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Track {
    Video,
    Audio,
}

/// What a plugin's `describe` has said so far.
#[derive(Clone, Debug)]
pub(crate) enum Described {
    Checking,
    Done(Arc<PluginDescription>),
}

/// Whether a listed component can be chosen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum OptionState {
    Available,
    /// Its plugin's `describe` has not answered yet.
    Checking,
    /// Why it cannot be used.
    Unavailable(String),
}

/// A component as a list shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ComponentOption {
    pub(crate) reference: ComponentRef,
    /// `"{name} · {plugin name}"`, with ` (project)` for a project plugin's.
    pub(crate) label: String,
    pub(crate) state: OptionState,
}

/// A chosen encoder and the settings given for it: the values that are
/// not left to their defaults, not every field's.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EncoderPick {
    pub(crate) component: ComponentRef,
    pub(crate) settings: Settings,
}

/// The muxer and encoders the page exports with; `audio` is `None`
/// when the export has no audio.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DialogChoice {
    pub(crate) muxer: ComponentRef,
    pub(crate) video: EncoderPick,
    pub(crate) audio: Option<EncoderPick>,
}

/// A presets file as the page read it: where it is (`None` for an
/// untitled project, or a platform without a settings folder), and its
/// contents (`None` when there is no file yet), or why it could not be
/// read.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PresetFileState {
    pub(crate) path: Option<PathBuf>,
    pub(crate) file: Result<Option<PresetsFile>, String>,
}

/// One field of an encoder's settings, with its value (given, or the
/// default) and what is wrong with that value.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FieldRow {
    pub(crate) field: SettingField,
    pub(crate) value: SettingValue,
    pub(crate) error: Option<String>,
}

/// The visible fields of one schema group, `None` for ungrouped fields.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FieldGroup {
    pub(crate) group: Option<String>,
    pub(crate) rows: Vec<FieldRow>,
}

/// What the page first resolves.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum InitialBase {
    /// The project's `default` preset, from the file it was found in.
    Preset(PresetSource, String, PresetOutput),
    /// The user file's `last`.
    Last(PresetOutput),
    /// The defaults.
    None,
}

/// What a resolve the model applies was asked for, which says which of
/// its encoders count as chosen: a container change keeps only those.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Asked {
    /// A preset: the encoders it names count as chosen, as does leaving
    /// the audio out.
    Preset(Box<PresetOutput>),
    /// The user file's `last`, or the defaults: nothing counts as chosen
    /// but audio left out where the muxer takes audio.
    Defaults,
    /// A container change, from [`ExportModel::container_choice`].
    Container,
}

/// Which encoders were chosen in the page or named by the applied
/// preset, rather than left to the muxer's defaults.
#[derive(Clone, Debug, Default, PartialEq)]
struct Chosen {
    /// The choice's video encoder was chosen.
    video: bool,
    audio: ChosenAudio,
}

#[derive(Clone, Debug, Default, PartialEq)]
enum ChosenAudio {
    /// Left to the muxer's default.
    #[default]
    Default,
    /// "None" was chosen, or the preset leaves the audio out.
    None,
    /// The choice's audio encoder was chosen.
    Encoder,
    /// The audio encoder chosen, with its settings, set aside while the
    /// muxer takes no audio.
    SetAside(EncoderPick),
}

/// The file "Save as preset…" writes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SaveTarget {
    /// "This project".
    Project,
    /// "My presets".
    User,
}

pub(crate) struct ExportModel {
    /// The registry's encoders and muxers: bundled plugins' first, then
    /// the user's, then the project's, each in search and manifest order.
    components: Vec<MediaComponentInfo>,
    described: HashMap<usize, Described>,
    choice: Option<DialogChoice>,
    chosen: Chosen,
    pub(crate) project: PresetFileState,
    pub(crate) user: PresetFileState,
    /// The preset the choice came from; `None` ("None" in the list) once
    /// a field or an encoder is changed.
    pub(crate) selected: Option<(PresetSource, String)>,
    /// The last resolve's warnings, kept until the next one.
    pub(crate) warnings: Vec<PageWarning>,
    /// Why the last resolve failed, which the view sets.
    pub(crate) error: Option<String>,
}

impl ExportModel {
    pub(crate) fn new(
        registry: &MediaRegistry,
        project: PresetFileState,
        user: PresetFileState,
    ) -> Self {
        let mut components: Vec<MediaComponentInfo> = registry
            .components()
            .into_iter()
            .filter(|info| info.kind != ComponentKind::Decoder)
            // Built-in and sequence muxers write stills, not a video.
            .filter(|info| info.kind != ComponentKind::Muxer || serves_video(info))
            .collect();
        // Stable: search order, then manifest order, within an origin.
        components.sort_by_key(|info| rank(&registry.plugins()[info.plugin]));
        let described = components
            .iter()
            .map(|info| (info.plugin, Described::Checking))
            .collect();
        Self {
            components,
            described,
            choice: None,
            chosen: Chosen::default(),
            project,
            user,
            selected: None,
            warnings: Vec::new(),
            error: None,
        }
    }

    pub(crate) fn described(&mut self, plugin: usize, description: Arc<PluginDescription>) {
        self.described.insert(plugin, Described::Done(description));
    }

    /// What the page first resolves: the project's default preset,
    /// looked up in the project file and then the user's as an export
    /// looks it up, else the user's `last`, else the defaults; and the
    /// warnings to show with it: a project or user file that could not be
    /// read (whose presets the list then leaves out), or a `default`
    /// naming a preset found in no file (an export's `unknown_preset`),
    /// which falls back to `last`.
    pub(crate) fn initial_base(&self) -> (InitialBase, Vec<PageWarning>) {
        let mut warnings = Vec::new();
        let project = read(&self.project, PresetSource::Project, &mut warnings);
        let user = read(&self.user, PresetSource::User, &mut warnings);
        if let Some(name) = project.and_then(|file| file.default.as_deref()) {
            if let Some(preset) = project.and_then(|file| file.find(name)) {
                return (
                    InitialBase::Preset(
                        PresetSource::Project,
                        name.to_owned(),
                        preset.output.clone(),
                    ),
                    warnings,
                );
            }
            if let Some(preset) = user.and_then(|file| file.find(name)) {
                return (
                    InitialBase::Preset(PresetSource::User, name.to_owned(), preset.output.clone()),
                    warnings,
                );
            }
            warnings.push(PageWarning::UnknownDefault {
                name: name.to_owned(),
            });
        }
        let base = match user.and_then(|file| file.last.clone()) {
            Some(last) => InitialBase::Last(last),
            None => InitialBase::None,
        };
        (base, warnings)
    }

    /// The output the first resolve takes: `export.` and the first
    /// extension of `base`'s muxer when that muxer is installed, so it
    /// does not warn of another extension; else the first of
    /// [`Self::default_outputs`].
    pub(crate) fn initial_output(&self, base: Option<&PresetOutput>) -> PathBuf {
        let named = base
            .and_then(|base| base.muxer.as_deref())
            .and_then(|muxer| muxer.parse().ok())
            .and_then(|muxer| self.extension_of(&muxer));
        match named {
            Some(extension) => PathBuf::from(format!("export.{extension}")),
            None => self.default_outputs().swap_remove(0),
        }
    }

    /// The outputs the first resolve over `base` tries: the one `base`'s
    /// muxer writes, as asked, since a preset or the last export is never
    /// replaced; with no base (the defaults), [`Self::default_outputs`],
    /// so a declared but unavailable default encoder (a native H.264 on
    /// Windows N without the Media Feature Pack) does not open the dialog
    /// on an error when another container works.
    pub(crate) fn first_outputs(&self, base: Option<&PresetOutput>) -> Vec<PathBuf> {
        match base {
            Some(base) => vec![self.initial_output(Some(base))],
            None => self.default_outputs(),
        }
    }

    /// The outputs an export left to the defaults tries, in order, until
    /// one resolves ([`resolve_first_available`]): `export.mp4` when a
    /// muxer writing `.mp4` has matching default encoders, then the first
    /// extension of each other ranked muxer whose default encoders are
    /// installed (never a project's). This reads the manifests only, so a
    /// declared encoder that `describe` finds unavailable still counts:
    /// the resolve finds that out and moves on. Never empty.
    pub(crate) fn default_outputs(&self) -> Vec<PathBuf> {
        let muxers = self
            .components
            .iter()
            .filter(|info| info.origin != MediaOrigin::Project)
            .filter_map(|info| match &info.entry {
                ComponentEntry::Muxer(entry) => Some(entry),
                _ => None,
            })
            .filter(|entry| {
                let installed = |kind, codec: &Option<String>| {
                    codec.as_ref().is_none_or(|codec| {
                        self.components.iter().any(|info| {
                            info.origin != MediaOrigin::Project
                                && info.kind == kind
                                && writes_any_codec(info, std::slice::from_ref(codec))
                        })
                    })
                };
                installed(ComponentKind::VideoEncoder, &entry.default_video)
                    && installed(ComponentKind::AudioEncoder, &entry.default_audio)
            });
        let writes_mp4 = |entry: &MediaMuxerEntry| {
            entry
                .extensions
                .iter()
                .any(|extension| extension.eq_ignore_ascii_case("mp4"))
        };
        let mut extensions: Vec<&str> = Vec::new();
        if muxers.clone().any(writes_mp4) {
            extensions.push("mp4");
        }
        for extension in muxers.filter_map(|entry| entry.extensions.first()) {
            if !extensions
                .iter()
                .any(|known| known.eq_ignore_ascii_case(extension))
            {
                extensions.push(extension);
            }
        }
        if extensions.is_empty() {
            extensions.push("mp4");
        }
        extensions
            .into_iter()
            .map(|extension| PathBuf::from(format!("export.{extension}")))
            .collect()
    }

    /// The muxer and encoders chosen, once a resolve was applied.
    pub(crate) fn choice(&self) -> Option<&DialogChoice> {
        self.choice.as_ref()
    }

    /// Whether the export has audio: an audio encoder is chosen and the
    /// muxer takes audio.
    pub(crate) fn exports_audio(&self) -> bool {
        self.choice.as_ref().is_some_and(|choice| {
            choice.audio.is_some() && self.muxer(&choice.muxer).is_some_and(takes_audio)
        })
    }

    /// Takes a resolve's components, and the settings it chose that are
    /// not their defaults, as the choice; `selected` is the preset it was
    /// resolved from, and `asked` what the resolve was asked for.
    pub(crate) fn apply(
        &mut self,
        resolved: &ResolvedOutput,
        selected: Option<(PresetSource, String)>,
        asked: Asked,
    ) {
        let video = pick(&resolved.video);
        let audio = resolved.audio.as_ref().map(pick);
        let takes_audio = takes_audio(&resolved.muxer.entry);
        let previous = self.choice.take();
        self.chosen = match asked {
            Asked::Preset(base) => preset_chosen(&base, &video, audio.as_ref(), takes_audio),
            Asked::Defaults => Chosen {
                video: false,
                audio: if audio.is_none() && takes_audio {
                    ChosenAudio::None
                } else {
                    ChosenAudio::Default
                },
            },
            Asked::Container => {
                let video_kept = previous
                    .as_ref()
                    .is_some_and(|previous| previous.video.component == video.component);
                let audio = match std::mem::take(&mut self.chosen.audio) {
                    ChosenAudio::Encoder => kept_audio(
                        previous.and_then(|previous| previous.audio),
                        audio.as_ref(),
                        takes_audio,
                    ),
                    ChosenAudio::SetAside(pick) => {
                        kept_audio(Some(pick), audio.as_ref(), takes_audio)
                    }
                    other => other,
                };
                Chosen {
                    video: self.chosen.video && video_kept,
                    audio,
                }
            }
        };
        self.choice = Some(DialogChoice {
            muxer: resolved.muxer.reference.clone(),
            video,
            audio,
        });
        self.selected = selected;
        self.warnings = resolved
            .warnings
            .iter()
            .cloned()
            .map(PageWarning::Resolve)
            .collect();
        self.error = None;
    }

    /// Takes `file` as `target`'s file once "Save as preset…" wrote the
    /// choice into it as `name`, and selects that preset.
    pub(crate) fn saved(&mut self, target: SaveTarget, name: String, file: PresetsFile) {
        // The choice is now that preset's, as picking it from the list
        // would make it: what it names counts as chosen.
        if let (Some(choice), Some(preset)) = (&self.choice, file.find(&name)) {
            let takes_audio = self.muxer(&choice.muxer).is_some_and(takes_audio);
            self.chosen = preset_chosen(
                &preset.output,
                &choice.video,
                choice.audio.as_ref(),
                takes_audio,
            );
        }
        let state = match target {
            SaveTarget::Project => &mut self.project,
            SaveTarget::User => &mut self.user,
        };
        state.file = Ok(Some(file));
        self.selected = Some((source(target), name));
    }

    /// The plugins whose `describe` made none of their components
    /// available, as a failed one does, set back to checking so the view
    /// describes them again.
    pub(crate) fn recheck_failed(&mut self) -> Vec<usize> {
        let mut failed: Vec<usize> = self
            .described
            .iter()
            .filter(|(_, described)| match described {
                Described::Checking => false,
                Described::Done(description) => description
                    .components
                    .iter()
                    .all(|component| !component.available),
            })
            .map(|(plugin, _)| *plugin)
            .collect();
        failed.sort_unstable();
        for plugin in &failed {
            self.described.insert(*plugin, Described::Checking);
        }
        failed
    }

    /// Every muxer, project plugins' too.
    pub(crate) fn muxer_options(&self) -> Vec<ComponentOption> {
        self.components
            .iter()
            .filter(|info| info.kind == ComponentKind::Muxer)
            .map(|info| self.option(info))
            .collect()
    }

    /// The encoders writing a codec the chosen muxer takes: none for audio
    /// when it takes no audio, or before there is a choice.
    pub(crate) fn encoder_options(&self, track: Track) -> Vec<ComponentOption> {
        let Some(entry) = self
            .choice
            .as_ref()
            .and_then(|choice| self.muxer(&choice.muxer))
        else {
            return Vec::new();
        };
        let accepted: &[String] = match track {
            Track::Video => &entry.video,
            Track::Audio => entry.audio.as_deref().unwrap_or_default(),
        };
        self.components
            .iter()
            .filter(|info| info.kind == kind(track) && writes_any_codec(info, accepted))
            .map(|info| self.option(info))
            .collect()
    }

    /// What a container change to `muxer` resolves: each encoder chosen
    /// in the page or named by the applied preset, with its settings,
    /// when `muxer` takes a codec it writes; else the muxer's default, as
    /// for an encoder that was a default. Audio chosen out stays out;
    /// audio a muxer took none of comes back.
    pub(crate) fn container_choice(&self, muxer: &ComponentRef) -> OutputChoice {
        let entry = self.muxer(muxer);
        let kept = |pick: &EncoderPick, track: Track| {
            let accepted: &[String] = match (entry, track) {
                (Some(entry), Track::Video) => &entry.video,
                (Some(entry), Track::Audio) => entry.audio.as_deref().unwrap_or_default(),
                (None, _) => &[],
            };
            match self.info(&pick.component, kind(track)) {
                Some(info) if writes_any_codec(info, accepted) => EncoderChoice {
                    component: Some(pick.component.clone()),
                    options: inputs(&pick.settings),
                },
                _ => EncoderChoice::default(),
            }
        };
        let (video, audio) = match &self.choice {
            Some(choice) => (
                if self.chosen.video {
                    kept(&choice.video, Track::Video)
                } else {
                    EncoderChoice::default()
                },
                match (&self.chosen.audio, &choice.audio) {
                    (ChosenAudio::None, _) => AudioChoice::None,
                    (ChosenAudio::Encoder, Some(audio)) | (ChosenAudio::SetAside(audio), _) => {
                        AudioChoice::Encode(kept(audio, Track::Audio))
                    }
                    _ => AudioChoice::default(),
                },
            ),
            None => Default::default(),
        };
        OutputChoice {
            preset: None,
            muxer: Some(muxer.clone()),
            video,
            audio,
        }
    }

    /// Chooses `track`'s encoder with its default settings; `None` leaves
    /// the audio out (the video always has one).
    pub(crate) fn choose_encoder(&mut self, track: Track, component: Option<ComponentRef>) {
        let Some(choice) = &mut self.choice else {
            return;
        };
        let pick = component.map(|component| EncoderPick {
            component,
            settings: Settings::new(),
        });
        match (track, pick) {
            (Track::Video, Some(pick)) => {
                choice.video = pick;
                self.chosen.video = true;
            }
            (Track::Video, None) => return,
            (Track::Audio, pick) => {
                self.chosen.audio = match pick {
                    Some(_) => ChosenAudio::Encoder,
                    None => ChosenAudio::None,
                };
                choice.audio = pick;
            }
        }
        self.selected = None;
    }

    /// `track`'s visible fields whose `advanced` is `advanced`, by group:
    /// ungrouped fields first, then each group in schema order. Empty while
    /// the encoder's plugin is being described.
    pub(crate) fn fields(&self, track: Track, advanced: bool) -> Vec<FieldGroup> {
        let (Some(pick), Some(schema)) = (self.pick(track), self.encoder_schema(track)) else {
            return Vec::new();
        };
        let values = effective(schema, &pick.settings);
        let (_, invalid) = sanitize(schema, &pick.settings);
        let mut groups: Vec<FieldGroup> = Vec::new();
        for field in schema
            .iter()
            .filter(|field| field.advanced == advanced && is_visible(field, &values))
        {
            let row = FieldRow {
                field: field.clone(),
                value: values[&field.key].clone(),
                error: invalid
                    .iter()
                    .find(|error| error.key == field.key)
                    .map(|error| error.message.clone()),
            };
            match groups.iter_mut().find(|group| group.group == field.group) {
                Some(group) => group.rows.push(row),
                None => groups.push(FieldGroup {
                    group: field.group.clone(),
                    rows: vec![row],
                }),
            }
        }
        // Stable: groups keep their schema order.
        groups.sort_by_key(|group| group.group.is_some());
        groups
    }

    /// Gives `key` `value`, checked when the choice is exported, and drops
    /// the given values it hides.
    pub(crate) fn set_setting(&mut self, track: Track, key: &str, value: SettingValue) {
        let schema = self.encoder_schema(track).unwrap_or_default().to_vec();
        let Some(pick) = self.pick_mut(track) else {
            return;
        };
        pick.settings.insert(key.to_owned(), value);
        drop_hidden(&schema, &mut pick.settings, |given| given == key);
        self.selected = None;
    }

    /// The presets of the project file and of the user file, in file
    /// order; none for a file that is missing or could not be read.
    pub(crate) fn presets(&self) -> (Vec<&Preset>, Vec<&Preset>) {
        (listed(&self.project), listed(&self.user))
    }

    /// The choice as an export takes it: every component named and every
    /// given setting a value. `Err` says why it cannot be exported: a
    /// component still being checked or unavailable, or an invalid field.
    pub(crate) fn output_choice(&self) -> Result<OutputChoice, String> {
        let Some(choice) = &self.choice else {
            return Err(self.error.clone().unwrap_or_else(|| CHECKING.to_owned()));
        };
        self.usable(&choice.muxer, ComponentKind::Muxer)?;
        let video = self.encoder_choice(&choice.video, ComponentKind::VideoEncoder)?;
        let audio = match &choice.audio {
            Some(audio) => {
                AudioChoice::Encode(self.encoder_choice(audio, ComponentKind::AudioEncoder)?)
            }
            None => AudioChoice::None,
        };
        Ok(OutputChoice {
            preset: None,
            muxer: Some(choice.muxer.clone()),
            video,
            audio,
        })
    }

    /// The chosen muxer's first extension.
    pub(crate) fn extension(&self) -> Option<String> {
        self.extension_of(&self.choice.as_ref()?.muxer)
    }

    /// The first extension of the installed muxer `muxer`: the output a
    /// container change resolves against.
    pub(crate) fn extension_of(&self, muxer: &ComponentRef) -> Option<String> {
        self.muxer(muxer)?.extensions.first().cloned()
    }

    /// The choice as a preset (or `last`) saves it: its components, and of
    /// each encoder the visible settings that differ from their defaults.
    pub(crate) fn saved_output(&self) -> PresetOutput {
        let Some(choice) = &self.choice else {
            return PresetOutput::default();
        };
        let saved = |pick: &EncoderPick, kind: ComponentKind| PresetEncoder {
            component: Some(pick.component.to_string()),
            options: match self.schema(&pick.component, kind) {
                Some(schema) => differences(schema, &pick.settings)
                    .into_iter()
                    .map(|(field, value)| (field.key.clone(), value.clone()))
                    .collect(),
                None => pick.settings.clone(),
            },
            other: Default::default(),
        };
        PresetOutput {
            muxer: Some(choice.muxer.to_string()),
            video: Some(saved(&choice.video, ComponentKind::VideoEncoder)),
            audio: Some(match &choice.audio {
                Some(audio) => PresetAudio::Encode(saved(audio, ComponentKind::AudioEncoder)),
                None => PresetAudio::None(NoAudio::None),
            }),
            other: Default::default(),
        }
    }

    /// Whether "Save as preset…" can write `target`'s file: not for an
    /// untitled project, and never over a file that could not be read.
    pub(crate) fn can_save(&self, target: SaveTarget) -> Result<(), String> {
        let state = self.file(target);
        if state.path.is_none() {
            return Err(match target {
                SaveTarget::Project => "Save the project first",
                SaveTarget::User => "This system has no settings folder to save presets in",
            }
            .to_owned());
        }
        state.file.as_ref().map(|_| ()).map_err(Clone::clone)
    }

    /// Whether `target`'s file already has a preset named `name`
    /// (trimmed, case-sensitive), which saving replaces.
    pub(crate) fn replaces(&self, target: SaveTarget, name: &str) -> bool {
        loaded(self.file(target)).is_some_and(|file| file.find(name.trim()).is_some())
    }

    fn file(&self, target: SaveTarget) -> &PresetFileState {
        match target {
            SaveTarget::Project => &self.project,
            SaveTarget::User => &self.user,
        }
    }

    fn info(&self, reference: &ComponentRef, kind: ComponentKind) -> Option<&MediaComponentInfo> {
        self.components
            .iter()
            .find(|info| info.kind == kind && info.reference == *reference)
    }

    /// The label of the listed component `reference` of `kind`.
    pub(super) fn component_label(
        &self,
        reference: &ComponentRef,
        kind: ComponentKind,
    ) -> Option<String> {
        self.info(reference, kind).map(label)
    }

    /// The name of the listed component `reference`, of any kind.
    pub(super) fn component_name(&self, reference: &ComponentRef) -> Option<String> {
        self.components
            .iter()
            .find(|info| info.reference == *reference)
            .map(|info| info.name.clone())
    }

    /// The manifest entry of the installed muxer `reference`.
    fn muxer(&self, reference: &ComponentRef) -> Option<&MediaMuxerEntry> {
        match &self.info(reference, ComponentKind::Muxer)?.entry {
            ComponentEntry::Muxer(entry) => Some(entry),
            _ => None,
        }
    }

    fn pick(&self, track: Track) -> Option<&EncoderPick> {
        let choice = self.choice.as_ref()?;
        match track {
            Track::Video => Some(&choice.video),
            Track::Audio => choice.audio.as_ref(),
        }
    }

    fn pick_mut(&mut self, track: Track) -> Option<&mut EncoderPick> {
        let choice = self.choice.as_mut()?;
        match track {
            Track::Video => Some(&mut choice.video),
            Track::Audio => choice.audio.as_mut(),
        }
    }

    /// What `info`'s plugin described of it; `None` while it is checking.
    fn description(&self, info: &MediaComponentInfo) -> Option<Option<&ComponentDescription>> {
        match self.described.get(&info.plugin)? {
            Described::Checking => None,
            Described::Done(description) => Some(description.components.iter().find(|described| {
                described.kind == info.kind && described.id == info.reference.component
            })),
        }
    }

    fn state(&self, info: &MediaComponentInfo) -> OptionState {
        match self.description(info) {
            None => OptionState::Checking,
            Some(Some(described)) if described.available => OptionState::Available,
            Some(Some(described)) => OptionState::Unavailable(
                described
                    .reason
                    .clone()
                    .unwrap_or_else(|| NO_REASON.to_owned()),
            ),
            Some(None) => OptionState::Unavailable(NOT_DESCRIBED.to_owned()),
        }
    }

    fn option(&self, info: &MediaComponentInfo) -> ComponentOption {
        ComponentOption {
            reference: info.reference.clone(),
            label: label(info),
            state: self.state(info),
        }
    }

    /// The settings schema `reference`'s plugin described for it.
    fn schema(&self, reference: &ComponentRef, kind: ComponentKind) -> Option<&[SettingField]> {
        let described = self.description(self.info(reference, kind)?)??;
        Some(&described.settings)
    }

    fn encoder_schema(&self, track: Track) -> Option<&[SettingField]> {
        self.schema(&self.pick(track)?.component, kind(track))
    }

    /// `Err` with the list's text for a component that is not available.
    fn usable(&self, reference: &ComponentRef, kind: ComponentKind) -> Result<String, String> {
        let Some(info) = self.info(reference, kind) else {
            return Err(format!("{reference} — not installed"));
        };
        let label = label(info);
        match self.state(info) {
            OptionState::Available => Ok(label),
            OptionState::Checking => Err(format!("{label} — {CHECKING}")),
            OptionState::Unavailable(reason) => Err(format!("{label} — {reason}")),
        }
    }

    fn encoder_choice(
        &self,
        pick: &EncoderPick,
        kind: ComponentKind,
    ) -> Result<EncoderChoice, String> {
        let label = self.usable(&pick.component, kind)?;
        let schema = self.schema(&pick.component, kind).unwrap_or_default();
        validate(schema, &pick.settings).map_err(|error| {
            let field = schema
                .iter()
                .find(|field| field.key == error.key)
                .map_or(error.key.as_str(), |field| field.label.as_str());
            format!("{label}: {field} {}", error.message)
        })?;
        Ok(EncoderChoice {
            component: Some(pick.component.clone()),
            options: inputs(&pick.settings),
        })
    }
}

/// Whether `muxer` takes any audio.
fn takes_audio(muxer: &MediaMuxerEntry) -> bool {
    muxer
        .audio
        .as_ref()
        .is_some_and(|accepted| !accepted.is_empty())
}

/// What counts as chosen once `base`, a preset, resolved to `video` and
/// `audio`: the encoders it names, and leaving the audio out (by `"none"`,
/// or where the muxer takes audio).
fn preset_chosen(
    base: &PresetOutput,
    video: &EncoderPick,
    audio: Option<&EncoderPick>,
    takes_audio: bool,
) -> Chosen {
    let names = |text: Option<&str>, component: &ComponentRef| {
        text.and_then(|text| text.parse::<ComponentRef>().ok())
            .is_some_and(|named| named == *component)
    };
    Chosen {
        video: names(
            base.video
                .as_ref()
                .and_then(|video| video.component.as_deref()),
            &video.component,
        ),
        audio: match (&base.audio, audio) {
            (Some(PresetAudio::None(_)), _) => ChosenAudio::None,
            (_, None) if takes_audio => ChosenAudio::None,
            (Some(PresetAudio::Encode(named)), Some(audio))
                if names(named.component.as_deref(), &audio.component) =>
            {
                ChosenAudio::Encoder
            }
            _ => ChosenAudio::Default,
        },
    }
}

/// The file `target` saves to, as a preset's source.
fn source(target: SaveTarget) -> PresetSource {
    match target {
        SaveTarget::Project => PresetSource::Project,
        SaveTarget::User => PresetSource::User,
    }
}

/// Components by [`default_rank`] (built-in, bundled, then the user's,
/// each by `media.priority`), a project's last.
fn rank(plugin: &RegisteredPlugin) -> (bool, Option<(u8, Reverse<i32>)>) {
    let rank = default_rank(plugin);
    (rank.is_none(), rank)
}

fn kind(track: Track) -> ComponentKind {
    match track {
        Track::Video => ComponentKind::VideoEncoder,
        Track::Audio => ComponentKind::AudioEncoder,
    }
}

pub(super) fn label(info: &MediaComponentInfo) -> String {
    let mut label = format!("{} · {}", info.name, info.plugin_name);
    if info.origin == MediaOrigin::Project {
        label.push_str(" (project)");
    }
    label
}

fn loaded(state: &PresetFileState) -> Option<&PresetsFile> {
    state.file.as_ref().ok()?.as_ref()
}

/// [`loaded`], adding a warning for a file that could not be read to
/// `warnings`.
fn read<'a>(
    state: &'a PresetFileState,
    source: PresetSource,
    warnings: &mut Vec<PageWarning>,
) -> Option<&'a PresetsFile> {
    match &state.file {
        Ok(file) => file.as_ref(),
        Err(message) => {
            warnings.push(PageWarning::UnreadableFile {
                source,
                message: message.clone(),
            });
            None
        }
    }
}

fn listed(state: &PresetFileState) -> Vec<&Preset> {
    loaded(state)
        .map(|file| file.presets.iter().collect())
        .unwrap_or_default()
}

/// Given settings as an export takes them: each one an explicit value.
fn inputs(settings: &Settings) -> BTreeMap<String, SettingInput> {
    settings
        .iter()
        .map(|(key, value)| (key.clone(), SettingInput::Value(value.clone())))
        .collect()
}

/// What a container change leaves of an audio encoder chosen before it
/// (`chosen`): set aside while the new muxer takes no audio, still chosen
/// when the resolve kept it, else the muxer's default.
fn kept_audio(
    chosen: Option<EncoderPick>,
    resolved: Option<&EncoderPick>,
    takes_audio: bool,
) -> ChosenAudio {
    match (chosen, resolved) {
        (Some(chosen), None) if !takes_audio => ChosenAudio::SetAside(chosen),
        (Some(chosen), Some(resolved)) if chosen.component == resolved.component => {
            ChosenAudio::Encoder
        }
        _ => ChosenAudio::Default,
    }
}

/// A resolved encoder as the page holds it: its settings that differ
/// from their defaults.
fn pick(encoder: &ResolvedEncoder) -> EncoderPick {
    EncoderPick {
        component: encoder.reference.clone(),
        settings: differences(&encoder.schema, &encoder.settings)
            .into_iter()
            .map(|(field, value)| (field.key.clone(), value.clone()))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_exporter::resolve_preset_output;
    use celesta_media::{MediaError, RegisteredPlugin};
    use celesta_plugin_host::{MediaAudioEncoderEntry, MediaManifest, MediaVideoEncoderEntry};
    use serde_json::json;
    use std::path::Path;

    #[test]
    fn an_unavailable_component_gives_its_reason_and_any_hint_in_the_dialog() {
        let failing = ExportError::ComponentUnavailable {
            component: "mediafoundation/h264".parse().unwrap(),
            reason: "Media Feature Pack is missing".into(),
        };
        assert_eq!(resolution_error(&failing), failing.to_string());
        let missing = ExportError::ComponentNotInstalled {
            component: "some-plugin/h264".parse().unwrap(),
        };
        let message = resolution_error(&missing);
        assert!(message.starts_with(&missing.to_string()), "{message}");
        assert!(message.contains("Plugins tab"), "{message}");
        let no_encoder = ExportError::NoDefaultEncoder {
            muxer: "some-plugin/mp4".parse().unwrap(),
            kind: celesta_media::ComponentKind::VideoEncoder,
            codec: "avc1".into(),
        };
        let message = resolution_error(&no_encoder);
        assert!(message.starts_with(&no_encoder.to_string()), "{message}");
        assert!(message.contains("install a media plugin"), "{message}");
    }

    fn reference(text: &str) -> ComponentRef {
        text.parse().unwrap()
    }

    fn video_encoder(id: &str, name: &str, codec: &str) -> MediaVideoEncoderEntry {
        MediaVideoEncoderEntry {
            id: id.into(),
            name: name.into(),
            codecs: vec![codec.into()],
            input_formats: None,
        }
    }

    fn mp4() -> MediaMuxerEntry {
        MediaMuxerEntry {
            id: "mp4".into(),
            name: "MP4".into(),
            extensions: vec!["mp4".into()],
            video: vec!["avc1".into()],
            audio: Some(vec!["mp4a".into()]),
            default_video: Some("avc1".into()),
            default_audio: Some("mp4a".into()),
            trims_leading_audio: true,
            sequence: false,
        }
    }

    fn ivf() -> MediaMuxerEntry {
        MediaMuxerEntry {
            id: "ivf".into(),
            name: "IVF".into(),
            extensions: vec!["ivf".into()],
            video: vec!["vp09".into()],
            audio: None,
            default_video: Some("vp09".into()),
            default_audio: None,
            trims_leading_audio: false,
            sequence: false,
        }
    }

    fn plugin(id: &str, origin: MediaOrigin, media: MediaManifest) -> RegisteredPlugin {
        RegisteredPlugin {
            id: id.into(),
            name: match id {
                "b" => "Bundled".into(),
                "p" => "Project plugin".into(),
                other => other.into(),
            },
            version: "1.0.0".into(),
            origin,
            directory: PathBuf::from(format!("/plugins/{id}")),
            executable: None,
            media,
            factory: Arc::new(|_, _| Err(MediaError::Codec("never started".into()))),
        }
    }

    /// `b`, bundled: muxers `mp4` and `ivf`, video encoders `x264` and
    /// `vp9`, audio encoder `aac`.
    fn bundled() -> RegisteredPlugin {
        plugin(
            "b",
            MediaOrigin::Bundled,
            MediaManifest {
                priority: 0,
                decoders: Vec::new(),
                video_encoders: vec![
                    video_encoder("x264", "H.264 (x264)", "avc1"),
                    video_encoder("vp9", "VP9", "vp09"),
                ],
                audio_encoders: vec![MediaAudioEncoderEntry {
                    id: "aac".into(),
                    name: "AAC".into(),
                    codecs: vec!["mp4a".into()],
                }],
                muxers: vec![mp4(), ivf()],
            },
        )
    }

    /// `p`, a project's: muxer `mp4` and video encoder `h264`.
    fn project() -> RegisteredPlugin {
        plugin(
            "p",
            MediaOrigin::Project,
            MediaManifest {
                priority: 0,
                decoders: Vec::new(),
                video_encoders: vec![video_encoder("h264", "H.264 (project)", "avc1.640028")],
                audio_encoders: Vec::new(),
                muxers: vec![mp4()],
            },
        )
    }

    /// `p` first, as a registry in search order may hold it, so the
    /// bundled-first ranking has to sort.
    fn registry() -> MediaRegistry {
        MediaRegistry::new(vec![project(), bundled()])
    }

    /// P2a's x264 fields, with a `preset` select before `crf`.
    fn x264_settings() -> Vec<SettingField> {
        [
            json!({ "key": "rateControl", "type": "select", "label": "Rate control",
                "group": "Rate control",
                "options": [ { "value": "crf", "label": "Constant quality (CRF)" },
                             { "value": "vbr", "label": "Average bitrate" } ],
                "default": "crf" }),
            json!({ "key": "preset", "type": "select", "label": "Preset",
                "options": [ { "value": "medium", "label": "Medium" },
                             { "value": "slow", "label": "Slow" } ],
                "default": "medium" }),
            json!({ "key": "crf", "type": "number", "integer": true, "min": 0, "max": 51,
                "step": 1, "default": 18, "label": "CRF", "group": "Rate control",
                "visibleWhen": { "rateControl": "crf" } }),
            json!({ "key": "bitrate", "type": "number", "integer": true, "min": 100000,
                "unit": "bit/s", "default": 8000000, "label": "Bitrate",
                "group": "Rate control", "visibleWhen": { "rateControl": "vbr" } }),
            json!({ "key": "tune", "type": "string", "default": "", "label": "Tune",
                "advanced": true }),
            json!({ "key": "faststart", "type": "boolean", "default": true,
                "label": "Fast start" }),
        ]
        .into_iter()
        .map(|field| serde_json::from_value(field).unwrap())
        .collect()
    }

    /// One AAC field: the bitrate.
    fn aac_settings() -> Vec<SettingField> {
        vec![
            serde_json::from_value(json!({ "key": "bitrate", "type": "number", "integer": true,
                "min": 32000, "default": 128000, "label": "Bitrate", "unit": "bit/s" }))
            .unwrap(),
        ]
    }

    /// What `plugin` describes: every component available, `b/x264` with
    /// [`x264_settings`] and `b/aac` with [`aac_settings`], except
    /// `unavailable`'s, each for its reason.
    fn description(
        registry: &MediaRegistry,
        plugin: usize,
        unavailable: &[(&str, &str)],
    ) -> Arc<PluginDescription> {
        let components = registry
            .components()
            .into_iter()
            .filter(|info| info.plugin == plugin)
            .map(|info| {
                let id = info.entry.id();
                match unavailable
                    .iter()
                    .find(|(component, _)| reference(component) == info.reference)
                {
                    Some((_, reason)) => ComponentDescription::unavailable(info.kind, id, *reason),
                    None => {
                        let mut description = ComponentDescription::unavailable(info.kind, id, "");
                        description.available = true;
                        description.reason = None;
                        if info.reference == reference("b/x264") {
                            description.settings = x264_settings();
                        }
                        if info.reference == reference("b/aac") {
                            description.settings = aac_settings();
                        }
                        description
                    }
                }
            })
            .collect();
        Arc::new(PluginDescription { components })
    }

    fn resolve(
        registry: &MediaRegistry,
        choice: &OutputChoice,
        base: Option<(&str, &PresetOutput)>,
        output: &str,
    ) -> ResolvedOutput {
        resolve_preset_output(choice, base, Path::new(output), registry, &mut |plugin| {
            description(registry, plugin, &[])
        })
        .unwrap()
    }

    fn file(path: Option<&str>, file: Option<serde_json::Value>) -> PresetFileState {
        PresetFileState {
            path: path.map(PathBuf::from),
            file: Ok(file.map(|file| serde_json::from_value(file).unwrap())),
        }
    }

    fn no_files() -> (PresetFileState, PresetFileState) {
        (
            file(Some("/project/export-presets.json"), None),
            file(Some("/home/export-presets.json"), None),
        )
    }

    /// A model over [`registry`] with no presets files, nothing described.
    fn model(registry: &MediaRegistry) -> ExportModel {
        let (project, user) = no_files();
        ExportModel::new(registry, project, user)
    }

    /// [`model`] with every plugin described and `output`'s defaults
    /// applied.
    fn resolved_model(registry: &MediaRegistry, output: &str) -> ExportModel {
        let mut model = model(registry);
        for plugin in 0..registry.plugins().len() {
            model.described(plugin, description(registry, plugin, &[]));
        }
        model.apply(
            &resolve(registry, &OutputChoice::default(), None, output),
            None,
            Asked::Defaults,
        );
        model
    }

    fn labels(options: &[ComponentOption]) -> Vec<&str> {
        options.iter().map(|option| option.label.as_str()).collect()
    }

    fn keys(groups: &[FieldGroup], group: Option<&str>) -> Vec<String> {
        groups
            .iter()
            .find(|found| found.group.as_deref() == group)
            .map(|found| found.rows.iter().map(|row| row.field.key.clone()).collect())
            .unwrap_or_default()
    }

    fn number(value: f64) -> SettingValue {
        SettingValue::Number(value)
    }

    fn string(value: &str) -> SettingValue {
        SettingValue::String(value.into())
    }

    #[test]
    fn components_show_their_plugin_and_origin() {
        let registry = registry();
        assert_eq!(
            labels(&model(&registry).muxer_options()),
            [
                "MP4 · Bundled",
                "IVF · Bundled",
                "MP4 · Project plugin (project)"
            ]
        );
    }

    #[test]
    fn still_muxers_are_not_containers() {
        let registry = celesta_exporter::with_builtin(vec![bundled()]);
        let model = model(&registry);
        assert_eq!(
            labels(&model.muxer_options()),
            ["MP4 · Bundled", "IVF · Bundled"]
        );
    }

    #[test]
    fn components_are_checking_until_described() {
        let registry = registry();
        let b = registry.plugin_index("b").unwrap();
        let mut model = model(&registry);
        model.apply(
            &resolve(&registry, &OutputChoice::default(), None, "export.mp4"),
            None,
            Asked::Defaults,
        );
        let every = [
            model.muxer_options(),
            model.encoder_options(Track::Video),
            model.encoder_options(Track::Audio),
        ]
        .concat();
        assert!(!every.is_empty());
        assert!(
            every
                .iter()
                .all(|option| option.state == OptionState::Checking)
        );
        assert!(model.output_choice().is_err());

        model.described(b, description(&registry, b, &[("b/vp9", "no libvpx")]));
        let video = model.encoder_options(Track::Video);
        let state = |options: &[ComponentOption], component: &str| {
            options
                .iter()
                .find(|option| option.reference == reference(component))
                .map(|option| option.state.clone())
        };
        assert_eq!(state(&video, "b/x264"), Some(OptionState::Available));
        assert_eq!(state(&video, "p/h264"), Some(OptionState::Checking));

        let ivf = OutputChoice {
            muxer: Some(reference("b/ivf")),
            ..OutputChoice::default()
        };
        model.apply(
            &resolve(&registry, &ivf, None, "export.ivf"),
            None,
            Asked::Container,
        );
        assert_eq!(
            state(&model.encoder_options(Track::Video), "b/vp9"),
            Some(OptionState::Unavailable("no libvpx".into()))
        );
    }

    #[test]
    fn encoders_follow_the_muxer() {
        let registry = registry();
        let mp4 = resolved_model(&registry, "export.mp4");
        assert_eq!(
            labels(&mp4.encoder_options(Track::Video)),
            [
                "H.264 (x264) · Bundled",
                "H.264 (project) · Project plugin (project)"
            ]
        );
        assert_eq!(
            labels(&mp4.encoder_options(Track::Audio)),
            ["AAC · Bundled"]
        );

        let ivf = resolved_model(&registry, "export.ivf");
        assert_eq!(
            labels(&ivf.encoder_options(Track::Video)),
            ["VP9 · Bundled"]
        );
        assert!(ivf.encoder_options(Track::Audio).is_empty());
        assert_eq!(ivf.extension().as_deref(), Some("ivf"));
    }

    #[test]
    fn audio_is_exported_with_an_encoder_and_a_muxer_taking_it() {
        let registry = registry();
        assert!(!model(&registry).exports_audio(), "nothing chosen yet");
        let mut mp4 = resolved_model(&registry, "export.mp4");
        assert!(mp4.exports_audio());
        mp4.choose_encoder(Track::Audio, None);
        assert!(!mp4.exports_audio());
        assert!(!resolved_model(&registry, "export.ivf").exports_audio());
    }

    #[test]
    fn fields_follow_visible_when_live() {
        let registry = registry();
        let mut model = resolved_model(&registry, "export.mp4");
        let basic = model.fields(Track::Video, false);
        let groups: Vec<Option<&str>> = basic.iter().map(|group| group.group.as_deref()).collect();
        assert_eq!(groups, [None, Some("Rate control")]);
        assert_eq!(keys(&basic, None), ["preset", "faststart"]);
        assert_eq!(keys(&basic, Some("Rate control")), ["rateControl", "crf"]);
        assert_eq!(basic[1].rows[1].value, number(18.0));

        model.set_setting(Track::Video, "rateControl", string("vbr"));
        let basic = model.fields(Track::Video, false);
        assert_eq!(
            keys(&basic, Some("Rate control")),
            ["rateControl", "bitrate"]
        );
        assert!(!keys(&basic, None).contains(&"tune".to_owned()));
        assert_eq!(keys(&model.fields(Track::Video, true), None), ["tune"]);
    }

    #[test]
    fn an_invalid_field_blocks_export() {
        let registry = registry();
        let mut model = resolved_model(&registry, "export.mp4");
        model.set_setting(Track::Video, "crf", number(99.0));
        let fields = model.fields(Track::Video, false);
        let crf = fields
            .iter()
            .flat_map(|group| &group.rows)
            .find(|row| row.field.key == "crf")
            .unwrap();
        assert_eq!(crf.value, number(99.0));
        assert!(crf.error.is_some());
        assert_eq!(
            model.output_choice(),
            Err("H.264 (x264) · Bundled: CRF must be at most 51".to_owned())
        );
    }

    #[test]
    fn output_choice_is_explicit() {
        let registry = registry();
        let mut model = resolved_model(&registry, "export.mp4");
        assert_eq!(model.error, None);
        model.set_setting(Track::Video, "crf", number(20.0));
        let choice = model.output_choice().unwrap();
        assert_eq!(choice.preset, None);
        assert_eq!(choice.muxer, Some(reference("b/mp4")));
        assert_eq!(choice.video.component, Some(reference("b/x264")));
        assert_eq!(
            choice.video.options.into_iter().collect::<Vec<_>>(),
            [("crf".to_owned(), SettingInput::Value(number(20.0)))]
        );
        let AudioChoice::Encode(audio) = choice.audio else {
            panic!("the audio is encoded");
        };
        assert_eq!(audio.component, Some(reference("b/aac")));
        assert!(audio.options.is_empty());
    }

    #[test]
    fn editing_after_a_preset_clears_the_selection() {
        let registry = registry();
        let mut model = resolved_model(&registry, "export.mp4");
        let web: PresetOutput = serde_json::from_value(json!({
            "video": { "component": "b/x264", "options": { "crf": 22, "nope": 1 } }
        }))
        .unwrap();
        let resolved = resolve(
            &registry,
            &OutputChoice::default(),
            Some(("preset `web`", &web)),
            "export.mp4",
        );
        assert_eq!(resolved.warnings.len(), 1);
        model.apply(
            &resolved,
            Some((PresetSource::Project, "web".into())),
            Asked::Preset(Box::new(web)),
        );
        assert_eq!(model.selected, Some((PresetSource::Project, "web".into())));

        model.set_setting(Track::Video, "preset", string("slow"));
        assert_eq!(model.selected, None);
        assert_eq!(
            model.warnings,
            resolved
                .warnings
                .iter()
                .cloned()
                .map(PageWarning::Resolve)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn choosing_an_encoder_resets_its_settings() {
        let registry = registry();
        let mut model = resolved_model(&registry, "export.mp4");
        model.apply(
            &resolve(&registry, &OutputChoice::default(), None, "export.mp4"),
            Some((PresetSource::User, "web".into())),
            Asked::Defaults,
        );
        model.set_setting(Track::Video, "crf", number(20.0));
        model.choose_encoder(Track::Video, Some(reference("p/h264")));
        model.choose_encoder(Track::Audio, None);
        assert_eq!(model.selected, None);
        let choice = model.output_choice().unwrap();
        assert_eq!(choice.video.component, Some(reference("p/h264")));
        assert!(choice.video.options.is_empty());
        assert_eq!(choice.audio, AudioChoice::None);
        assert_eq!(
            model.saved_output().audio,
            Some(PresetAudio::None(NoAudio::None))
        );
    }

    #[test]
    fn a_container_change_keeps_the_encoders_it_takes() {
        let registry = registry();
        let mut model = resolved_model(&registry, "export.mp4");
        model.choose_encoder(Track::Video, Some(reference("b/x264")));
        model.choose_encoder(Track::Audio, Some(reference("b/aac")));
        model.set_setting(Track::Video, "crf", number(20.0));

        // The project's MP4 takes the x264 and AAC chosen: both are kept,
        // with the CRF.
        let same = model.container_choice(&reference("p/mp4"));
        assert_eq!(same.preset, None);
        assert_eq!(same.muxer, Some(reference("p/mp4")));
        assert_eq!(same.video.component, Some(reference("b/x264")));
        assert_eq!(
            same.video.options.into_iter().collect::<Vec<_>>(),
            [("crf".to_owned(), SettingInput::Value(number(20.0)))]
        );
        let AudioChoice::Encode(audio) = same.audio else {
            panic!("the audio is kept");
        };
        assert_eq!(audio.component, Some(reference("b/aac")));

        // IVF takes neither: the muxer's defaults are resolved.
        let ivf = model.container_choice(&reference("b/ivf"));
        assert_eq!(ivf.video, EncoderChoice::default());
        assert_eq!(ivf.audio, AudioChoice::default());

        // No audio stays no audio.
        model.choose_encoder(Track::Audio, None);
        assert_eq!(
            model.container_choice(&reference("p/mp4")).audio,
            AudioChoice::None
        );
    }

    /// `b` with a QuickTime muxer whose default video is ProRes, as
    /// FFmpeg's manifest has it, though it also takes H.264.
    fn registry_with_mov() -> MediaRegistry {
        let mut b = bundled();
        b.media
            .video_encoders
            .push(video_encoder("prores", "ProRes", "ap4h"));
        b.media.muxers.push(MediaMuxerEntry {
            id: "mov".into(),
            name: "QuickTime".into(),
            extensions: vec!["mov".into()],
            video: vec!["avc1".into(), "ap4h".into()],
            audio: Some(vec!["mp4a".into()]),
            default_video: Some("ap4h".into()),
            default_audio: Some("mp4a".into()),
            trims_leading_audio: true,
            sequence: false,
        });
        MediaRegistry::new(vec![project(), b])
    }

    /// Changes `model`'s container to `muxer` as the view does: resolves
    /// what [`ExportModel::container_choice`] asks and applies it.
    fn change_container(registry: &MediaRegistry, model: &mut ExportModel, muxer: &str) {
        let muxer = reference(muxer);
        let output = format!("export.{}", model.extension_of(&muxer).unwrap());
        let choice = model.container_choice(&muxer);
        model.apply(
            &resolve(registry, &choice, None, &output),
            None,
            Asked::Container,
        );
    }

    #[test]
    fn a_container_change_takes_the_muxers_default_encoders() {
        let registry = registry_with_mov();
        let mut model = resolved_model(&registry, "export.mp4");
        assert_eq!(model.choice().unwrap().video.component, reference("b/x264"));

        // x264 was the default, not a choice: QuickTime's default is ProRes.
        change_container(&registry, &mut model, "b/mov");
        assert_eq!(
            model.choice().unwrap().video.component,
            reference("b/prores")
        );

        // An encoder chosen in the page stays, with its settings, where
        // the new muxer takes it.
        change_container(&registry, &mut model, "b/mp4");
        model.choose_encoder(Track::Video, Some(reference("b/x264")));
        model.set_setting(Track::Video, "crf", number(20.0));
        change_container(&registry, &mut model, "b/mov");
        let video = &model.choice().unwrap().video;
        assert_eq!(video.component, reference("b/x264"));
        assert_eq!(
            video.settings,
            Settings::from([("crf".to_owned(), number(20.0))])
        );
    }

    #[test]
    fn audio_left_out_by_a_container_comes_back() {
        let registry = registry();
        let mut model = resolved_model(&registry, "export.mp4");
        change_container(&registry, &mut model, "b/ivf");
        assert_eq!(model.choice().unwrap().audio, None);
        change_container(&registry, &mut model, "b/mp4");
        assert_eq!(
            model
                .choice()
                .unwrap()
                .audio
                .as_ref()
                .map(|audio| &audio.component),
            Some(&reference("b/aac"))
        );

        // "None" chosen stays none through the same round trip.
        model.choose_encoder(Track::Audio, None);
        change_container(&registry, &mut model, "b/ivf");
        change_container(&registry, &mut model, "b/mp4");
        assert_eq!(model.choice().unwrap().audio, None);

        // And an encoder chosen comes back as chosen.
        model.choose_encoder(Track::Audio, Some(reference("b/aac")));
        change_container(&registry, &mut model, "b/ivf");
        assert_eq!(
            model.container_choice(&reference("b/mp4")).audio,
            AudioChoice::Encode(EncoderChoice {
                component: Some(reference("b/aac")),
                options: BTreeMap::new(),
            })
        );
    }

    #[test]
    fn audio_set_aside_comes_back_with_its_settings() {
        let registry = registry();
        let mut model = resolved_model(&registry, "export.mp4");
        model.choose_encoder(Track::Audio, Some(reference("b/aac")));
        model.set_setting(Track::Audio, "bitrate", number(192000.0));
        change_container(&registry, &mut model, "b/ivf");
        assert_eq!(model.choice().unwrap().audio, None);
        change_container(&registry, &mut model, "b/mp4");
        assert_eq!(
            model.choice().unwrap().audio,
            Some(EncoderPick {
                component: reference("b/aac"),
                settings: Settings::from([("bitrate".to_owned(), number(192000.0))]),
            })
        );
    }

    #[test]
    fn a_saved_preset_is_chosen_as_picking_it_would_be() {
        let registry = registry_with_mov();
        let mut model = resolved_model(&registry, "export.mp4");
        model.set_setting(Track::Video, "crf", number(22.0));
        let mut file = PresetsFile::new();
        file.put(Preset {
            name: "web".into(),
            output: model.saved_output(),
        });
        model.saved(SaveTarget::User, "web".into(), file.clone());
        assert_eq!(model.selected, Some((PresetSource::User, "web".into())));
        assert_eq!(model.user.file, Ok(Some(file)));
        assert_eq!(model.presets().1[0].name, "web");

        // The saved preset names x264, so QuickTime keeps it with its CRF.
        change_container(&registry, &mut model, "b/mov");
        let choice = model.choice().unwrap();
        assert_eq!(choice.video.component, reference("b/x264"));
        assert_eq!(
            choice.video.settings,
            Settings::from([("crf".to_owned(), number(22.0))])
        );
        assert_eq!(
            choice.audio.as_ref().map(|audio| &audio.component),
            Some(&reference("b/aac"))
        );
    }

    #[test]
    fn a_failed_describe_is_checked_again() {
        let registry = registry();
        let (b, p) = (
            registry.plugin_index("b").unwrap(),
            registry.plugin_index("p").unwrap(),
        );
        let mut model = model(&registry);
        assert!(model.recheck_failed().is_empty(), "nothing described yet");
        // `p`'s describe failed: every component unavailable.
        model.described(
            p,
            description(
                &registry,
                p,
                &[
                    ("p/mp4", "the plugin exited"),
                    ("p/h264", "the plugin exited"),
                ],
            ),
        );
        model.described(b, description(&registry, b, &[("b/vp9", "no libvpx")]));
        assert_eq!(model.recheck_failed(), [p]);
        let state = |model: &ExportModel, component: &str| {
            model
                .muxer_options()
                .into_iter()
                .find(|option| option.reference == reference(component))
                .map(|option| option.state)
        };
        assert_eq!(state(&model, "p/mp4"), Some(OptionState::Checking));
        assert_eq!(state(&model, "b/mp4"), Some(OptionState::Available));
        assert!(model.recheck_failed().is_empty(), "already checking");
    }

    #[test]
    fn a_preset_names_what_a_container_change_keeps() {
        let registry = registry_with_mov();
        let mut model = model(&registry);
        for plugin in 0..registry.plugins().len() {
            model.described(plugin, description(&registry, plugin, &[]));
        }
        let web: PresetOutput = serde_json::from_value(json!({
            "muxer": "b/mp4", "video": { "component": "b/x264" }, "audio": "none"
        }))
        .unwrap();
        model.apply(
            &resolve(
                &registry,
                &OutputChoice::default(),
                Some(("preset `web`", &web)),
                "export.mp4",
            ),
            Some((PresetSource::User, "web".into())),
            Asked::Preset(Box::new(web)),
        );
        change_container(&registry, &mut model, "b/mov");
        let choice = model.choice().unwrap();
        assert_eq!(choice.video.component, reference("b/x264"));
        assert_eq!(choice.audio, None);

        // `last` names components too, but none counts as chosen: only its
        // audio left out where the muxer takes audio.
        let last: PresetOutput = serde_json::from_value(json!({
            "muxer": "b/mp4", "video": { "component": "b/x264" }, "audio": "none"
        }))
        .unwrap();
        model.apply(
            &resolve(
                &registry,
                &OutputChoice::default(),
                Some(("your last export", &last)),
                "export.mp4",
            ),
            None,
            Asked::Defaults,
        );
        change_container(&registry, &mut model, "b/mov");
        let choice = model.choice().unwrap();
        assert_eq!(choice.video.component, reference("b/prores"));
        assert_eq!(choice.audio, None);
    }

    fn output(muxer: &str) -> PresetOutput {
        PresetOutput {
            muxer: Some(muxer.into()),
            ..PresetOutput::default()
        }
    }

    #[test]
    fn the_initial_base_prefers_the_project_default() {
        let registry = registry();
        let project = |default: Option<&str>| {
            let mut contents =
                json!({ "version": 1, "presets": [ { "name": "web", "muxer": "b/mp4" } ] });
            if let Some(default) = default {
                contents["default"] = json!(default);
            }
            file(Some("/project/export-presets.json"), Some(contents))
        };
        let user = file(
            Some("/home/export-presets.json"),
            Some(json!({ "version": 1,
                "presets": [ { "name": "small", "muxer": "b/ivf" } ],
                "last": { "muxer": "b/ivf" } })),
        );
        let base = |project: PresetFileState, user: PresetFileState| {
            ExportModel::new(&registry, project, user).initial_base()
        };

        assert_eq!(
            base(project(Some("web")), user.clone()),
            (
                InitialBase::Preset(PresetSource::Project, "web".into(), output("b/mp4")),
                Vec::new()
            )
        );
        assert_eq!(
            base(project(Some("small")), user.clone()),
            (
                InitialBase::Preset(PresetSource::User, "small".into(), output("b/ivf")),
                Vec::new()
            )
        );
        assert_eq!(
            base(project(None), user.clone()),
            (InitialBase::Last(output("b/ivf")), Vec::new())
        );
        let (none, nothing) = no_files();
        assert_eq!(base(none, nothing), (InitialBase::None, Vec::new()));
    }

    #[test]
    fn a_default_naming_no_preset_warns_and_falls_back() {
        let registry = registry();
        let user = file(
            Some("/home/export-presets.json"),
            Some(json!({ "version": 1, "last": { "muxer": "b/ivf" } })),
        );
        let project = file(
            Some("/project/export-presets.json"),
            Some(json!({ "version": 1, "default": "gone" })),
        );
        assert_eq!(
            ExportModel::new(&registry, project, user.clone()).initial_base(),
            (
                InitialBase::Last(output("b/ivf")),
                vec![PageWarning::UnknownDefault {
                    name: "gone".to_owned()
                }]
            )
        );

        let message = "/project/export-presets.json is not valid JSON: expected value";
        let broken = PresetFileState {
            path: Some(PathBuf::from("/project/export-presets.json")),
            file: Err(message.to_owned()),
        };
        assert_eq!(
            ExportModel::new(&registry, broken, user).initial_base(),
            (
                InitialBase::Last(output("b/ivf")),
                vec![unreadable(PresetSource::Project, message)]
            )
        );
    }

    fn unreadable(source: PresetSource, message: &str) -> PageWarning {
        PageWarning::UnreadableFile {
            source,
            message: message.to_owned(),
        }
    }

    #[test]
    fn a_broken_user_file_warns_when_the_dialog_opens() {
        let registry = registry();
        let message = "/home/export-presets.json is not valid JSON: expected value";
        let broken = PresetFileState {
            path: Some(PathBuf::from("/home/export-presets.json")),
            file: Err(message.to_owned()),
        };
        // With no project default, `last` is looked for in the user file.
        assert_eq!(
            ExportModel::new(&registry, no_files().0, broken.clone()).initial_base(),
            (
                InitialBase::None,
                vec![unreadable(PresetSource::User, message)]
            )
        );
        // The project default found in the project file still says so.
        let project = file(
            Some("/project/export-presets.json"),
            Some(json!({ "version": 1, "default": "web",
                "presets": [ { "name": "web", "muxer": "b/mp4" } ] })),
        );
        assert_eq!(
            ExportModel::new(&registry, project, broken).initial_base(),
            (
                InitialBase::Preset(PresetSource::Project, "web".into(), output("b/mp4")),
                vec![unreadable(PresetSource::User, message)]
            )
        );
    }

    #[test]
    fn the_initial_output_takes_the_bases_muxer_or_the_mp4_one() {
        let dialog = model(&registry());
        assert_eq!(dialog.initial_output(None), PathBuf::from("export.mp4"));
        assert_eq!(
            dialog.initial_output(Some(&output("b/ivf"))),
            PathBuf::from("export.ivf")
        );
        assert_eq!(
            dialog.initial_output(Some(&output("gone/mov"))),
            PathBuf::from("export.mp4")
        );
        assert_eq!(
            dialog.extension_of(&reference("b/ivf")).as_deref(),
            Some("ivf")
        );
        assert_eq!(dialog.extension_of(&reference("b/x264")), None);

        let mut ivf_only = bundled();
        ivf_only.media.muxers = vec![ivf()];
        let registry = MediaRegistry::new(vec![project(), ivf_only]);
        assert_eq!(
            model(&registry).initial_output(None),
            PathBuf::from("export.ivf")
        );
    }

    #[test]
    fn a_standard_only_install_starts_with_a_resolvable_mov_output() {
        let mut standard = registry_with_mov().plugins()[1].clone();
        standard
            .media
            .video_encoders
            .retain(|encoder| encoder.id == "prores");
        // A project's H.264 encoder must not supply the implicit default.
        let registry = MediaRegistry::new(vec![project(), standard]);
        let path = model(&registry).initial_output(None);
        assert_eq!(path, PathBuf::from("export.mov"));
        let resolved = resolve(
            &registry,
            &OutputChoice::default(),
            None,
            path.to_str().unwrap(),
        );
        assert_eq!(resolved.video.reference, reference("b/prores"));
    }

    #[test]
    fn an_installed_native_h264_encoder_keeps_the_mp4_default() {
        let mut standard = registry_with_mov().plugins()[1].clone();
        standard
            .media
            .video_encoders
            .retain(|encoder| encoder.id == "prores");
        let mut native = bundled();
        native.id = "native".into();
        native.media.muxers.clear();
        let registry = MediaRegistry::new(vec![standard, native]);
        assert_eq!(
            model(&registry).initial_output(None),
            PathBuf::from("export.mp4")
        );
    }

    /// The standard plugin `b` (ProRes and AAC, with MP4, IVF and MOV
    /// muxers) and a native `mediafoundation` plugin whose only component
    /// is an H.264 encoder, as on Windows.
    fn registry_with_native_h264() -> MediaRegistry {
        let mut standard = registry_with_mov().plugins()[1].clone();
        standard
            .media
            .video_encoders
            .retain(|encoder| encoder.id == "prores");
        let native = plugin(
            "mediafoundation",
            MediaOrigin::Bundled,
            MediaManifest {
                priority: 0,
                decoders: Vec::new(),
                video_encoders: vec![video_encoder("h264", "H.264 (Media Foundation)", "avc1")],
                audio_encoders: Vec::new(),
                muxers: Vec::new(),
            },
        );
        MediaRegistry::new(vec![standard, native])
    }

    /// The first of `outputs` that resolves over `registry` with
    /// `unavailable`'s components described as unavailable.
    fn first_available(
        registry: &MediaRegistry,
        outputs: &[PathBuf],
        unavailable: &[(&str, &str)],
    ) -> Result<(ResolvedOutput, PathBuf, Option<PageWarning>), ExportError> {
        resolve_first_available(outputs, |output| {
            resolve_preset_output(
                &OutputChoice::default(),
                None,
                output,
                registry,
                &mut |plugin| description(registry, plugin, unavailable),
            )
        })
    }

    const NO_FEATURE_PACK: (&str, &str) = ("mediafoundation/h264", "Media Feature Pack is missing");

    #[test]
    fn an_unavailable_native_h264_opens_on_mov_and_says_why() {
        let registry = registry_with_native_h264();
        let mut dialog = model(&registry);
        let outputs = dialog.first_outputs(None);
        assert_eq!(
            outputs,
            [PathBuf::from("export.mp4"), PathBuf::from("export.mov")]
        );
        let (resolved, output, warning) =
            first_available(&registry, &outputs, &[NO_FEATURE_PACK]).unwrap();
        assert_eq!(output, PathBuf::from("export.mov"));
        assert_eq!(resolved.video.reference, reference("b/prores"));
        // As the view applies it: no error, and the reason among the warnings.
        dialog.apply(&resolved, None, Asked::Defaults);
        dialog.warnings.extend(warning);
        assert_eq!(dialog.error, None);
        assert_eq!(dialog.choice().unwrap().muxer, reference("b/mov"));
        let [warning] = dialog.warnings.as_slice() else {
            panic!("one warning: {:?}", dialog.warnings);
        };
        let warning = dialog.warning_text(warning);
        assert!(
            warning.starts_with("MP4 cannot be exported here, so the export starts as MOV"),
            "{warning}"
        );
        assert!(
            warning.contains("Media Feature Pack is missing"),
            "{warning}"
        );
    }

    #[test]
    fn an_available_native_h264_still_opens_on_mp4() {
        let registry = registry_with_native_h264();
        let outputs = model(&registry).first_outputs(None);
        let (resolved, output, warning) = first_available(&registry, &outputs, &[]).unwrap();
        assert_eq!(output, PathBuf::from("export.mp4"));
        assert_eq!(resolved.video.reference, reference("mediafoundation/h264"));
        assert_eq!(warning, None);
    }

    #[test]
    fn with_nothing_usable_the_mp4_error_is_shown() {
        let registry = registry_with_native_h264();
        let outputs = model(&registry).first_outputs(None);
        let error = first_available(
            &registry,
            &outputs,
            &[NO_FEATURE_PACK, ("b/prores", "no prores_ks")],
        )
        .unwrap_err();
        let message = resolution_error(&error);
        assert!(message.contains("mediafoundation/h264"), "{message}");
        assert!(
            message.contains("Media Feature Pack is missing"),
            "{message}"
        );
    }

    #[test]
    fn a_third_party_mp4_muxer_without_h264_still_opens_on_a_resolvable_output() {
        // Linux without ffmpeg-x264: the bundled MP4 muxer's H.264 default
        // has no encoder, while a user plugin's own MP4 muxer has its HEVC
        // default installed, so `.mp4` is offered first. Whichever MP4
        // muxer `.mp4` resolves to, the dialog must not open on an error.
        let mut standard = registry_with_mov().plugins()[1].clone();
        standard
            .media
            .video_encoders
            .retain(|encoder| encoder.id == "prores");
        let mut hevc_mp4 = mp4();
        hevc_mp4.video = vec!["hvc1".into()];
        hevc_mp4.default_video = Some("hvc1".into());
        let third = plugin(
            "third",
            MediaOrigin::User,
            MediaManifest {
                priority: 0,
                decoders: Vec::new(),
                video_encoders: vec![video_encoder("hevc", "HEVC", "hvc1")],
                audio_encoders: Vec::new(),
                muxers: vec![hevc_mp4],
            },
        );
        let registry = MediaRegistry::new(vec![standard, third]);
        let page = model(&registry);
        let outputs = page.first_outputs(None);
        assert_eq!(outputs[0], PathBuf::from("export.mp4"));
        let (resolved, output, warning) = first_available(&registry, &outputs, &[]).unwrap();
        // `.mp4` resolves to the bundled muxer, ranked first, which has no
        // H.264 encoder; the dialog moves on to MOV and says why.
        assert_eq!(output, PathBuf::from("export.mov"));
        assert_eq!(resolved.muxer.reference, reference("b/mov"));
        assert_eq!(resolved.video.reference, reference("b/prores"));
        let warning = page.warning_text(&warning.expect("the reason MP4 was passed over"));
        assert!(warning.contains("b/mp4 is unavailable"), "{warning}");
    }

    #[test]
    fn a_preset_or_last_export_naming_mp4_is_not_replaced() {
        let registry = registry_with_native_h264();
        let dialog = model(&registry);
        let outputs = dialog.first_outputs(Some(&output("b/mp4")));
        assert_eq!(outputs, [PathBuf::from("export.mp4")]);
        let error = first_available(&registry, &outputs, &[NO_FEATURE_PACK]).unwrap_err();
        assert!(
            resolution_error(&error).contains("mediafoundation/h264"),
            "{error}"
        );
    }

    #[test]
    fn the_choice_is_what_was_applied_and_edited() {
        let registry = registry();
        assert_eq!(model(&registry).choice(), None);
        let mut dialog = resolved_model(&registry, "export.mp4");
        dialog.set_setting(Track::Video, "crf", number(20.0));
        assert_eq!(
            dialog.choice(),
            Some(&DialogChoice {
                muxer: reference("b/mp4"),
                video: EncoderPick {
                    component: reference("b/x264"),
                    settings: Settings::from([("crf".to_owned(), number(20.0))]),
                },
                audio: Some(EncoderPick {
                    component: reference("b/aac"),
                    settings: Settings::new(),
                }),
            })
        );
    }

    #[test]
    fn saved_output_keeps_only_changes() {
        let registry = registry();
        let mut model = resolved_model(&registry, "export.mp4");
        model.set_setting(Track::Video, "crf", number(18.0));
        model.set_setting(Track::Video, "preset", string("slow"));
        let saved = model.saved_output();
        assert_eq!(saved.muxer.as_deref(), Some("b/mp4"));
        let video = saved.video.unwrap();
        assert_eq!(video.component.as_deref(), Some("b/x264"));
        assert_eq!(
            video.options,
            Settings::from([("preset".to_owned(), string("slow"))])
        );
        assert_eq!(
            saved.audio,
            Some(PresetAudio::Encode(PresetEncoder {
                component: Some("b/aac".into()),
                options: Settings::new(),
                other: Default::default(),
            }))
        );
    }

    #[test]
    fn a_broken_presets_file_is_shown_and_never_written() {
        let registry = registry();
        let (project, _) = no_files();
        let message = "/home/export-presets.json is not valid JSON: expected value";
        let user = PresetFileState {
            path: Some(PathBuf::from("/home/export-presets.json")),
            file: Err(message.to_owned()),
        };
        let model = ExportModel::new(&registry, project, user);
        assert_eq!(model.can_save(SaveTarget::User), Err(message.to_owned()));
        assert_eq!(model.can_save(SaveTarget::Project), Ok(()));
        assert!(model.presets().1.is_empty());
    }

    #[test]
    fn an_untitled_project_cannot_take_a_preset() {
        let registry = registry();
        let (_, user) = no_files();
        let model = ExportModel::new(&registry, file(None, None), user);
        assert_eq!(
            model.can_save(SaveTarget::Project),
            Err("Save the project first".to_owned())
        );
        assert_eq!(model.can_save(SaveTarget::User), Ok(()));
    }

    #[test]
    fn saving_an_existing_name_is_a_replace() {
        let registry = registry();
        let user = file(
            Some("/home/export-presets.json"),
            Some(json!({ "version": 1, "presets": [ { "name": "web" } ] })),
        );
        let model = ExportModel::new(&registry, no_files().0, user);
        assert!(model.replaces(SaveTarget::User, " web "));
        assert!(!model.replaces(SaveTarget::User, "Web"));
        assert!(!model.replaces(SaveTarget::Project, "web"));
        let (project, user) = model.presets();
        assert!(project.is_empty());
        assert_eq!(user[0].name, "web");
    }
}
