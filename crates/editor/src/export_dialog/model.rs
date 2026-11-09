//! What the export dialog shows and edits, without GPUI: the media
//! components and what each plugin's `describe` said of them, the chosen
//! muxer and encoders with the settings given for them, and the presets
//! files.
//!
//! A preset or a container change is resolved by the view through
//! `resolve_preset_output` and handed to [`ExportDialogModel::apply`];
//! encoder and field edits change the choice here.

use celesta_exporter::{
    AudioChoice, EncoderChoice, NoAudio, OutputChoice, Preset, PresetAudio, PresetEncoder,
    PresetOutput, PresetSource, PresetsFile, ResolvedEncoder, ResolvedOutput, SettingInput,
};
use celesta_media::settings::{differences, is_visible, sanitize, validate};
use celesta_media::{
    ComponentDescription, ComponentEntry, ComponentKind, ComponentRef, MediaComponentInfo,
    MediaRegistry, PluginDescription, SettingField, SettingValue, Settings,
};
use celesta_plugin_host::{MediaMuxerEntry, MediaOrigin, codec_family};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// The name a preset warning gives the user file's `last`.
const LAST: &str = "last";

/// What a component shows while its plugin's `describe` runs.
const CHECKING: &str = "Checking…";

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
    /// `"{name} · {plugin id}"`, with ` (project)` for a project plugin's.
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

/// The muxer and encoders the dialog exports with; `audio` is `None`
/// when the export has no audio.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DialogChoice {
    pub(crate) muxer: ComponentRef,
    pub(crate) video: EncoderPick,
    pub(crate) audio: Option<EncoderPick>,
}

/// A presets file as the dialog read it: where it is (`None` for an
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

/// The file "Save as preset…" writes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SaveTarget {
    /// "This project".
    Project,
    /// "My presets".
    User,
}

pub(crate) struct ExportDialogModel {
    /// The registry's encoders and muxers: bundled plugins' first, then
    /// the user's, then the project's, each in search and manifest order.
    components: Vec<MediaComponentInfo>,
    described: HashMap<usize, Described>,
    choice: Option<DialogChoice>,
    pub(crate) project: PresetFileState,
    pub(crate) user: PresetFileState,
    /// The preset the choice came from; `None` ("None" in the list) once
    /// a field or an encoder is changed.
    pub(crate) selected: Option<(PresetSource, String)>,
    /// The last resolve's warnings, kept until the next one.
    pub(crate) warnings: Vec<String>,
    /// Why the last resolve failed, which the view sets.
    pub(crate) error: Option<String>,
}

impl ExportDialogModel {
    pub(crate) fn new(
        registry: &MediaRegistry,
        project: PresetFileState,
        user: PresetFileState,
    ) -> Self {
        let mut components: Vec<MediaComponentInfo> = registry
            .components()
            .into_iter()
            .filter(|info| info.kind != ComponentKind::Decoder)
            .collect();
        // Stable: search order, then manifest order, within an origin.
        components.sort_by_key(|info| rank(info.origin));
        let described = components
            .iter()
            .map(|info| (info.plugin, Described::Checking))
            .collect();
        Self {
            components,
            described,
            choice: None,
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

    /// What the dialog first resolves: the project's default preset, else
    /// `last`, else nothing (defaults). The default's name is looked up in
    /// the project file, then the user's, as an export's is.
    pub(crate) fn initial_base(&self) -> Option<(String, PresetOutput)> {
        let project = loaded(&self.project);
        let user = loaded(&self.user);
        if let Some(name) = project.and_then(|file| file.default.as_deref())
            && let Some(preset) = [project, user]
                .into_iter()
                .flatten()
                .find_map(|file| file.find(name))
        {
            return Some((preset.name.clone(), preset.output.clone()));
        }
        let last = user.and_then(|file| file.last.clone())?;
        Some((LAST.to_owned(), last))
    }

    /// The muxer to resolve defaults against: the default for `.mp4`, else
    /// the first ranked muxer (bundled, then the user's; never a
    /// project's), as `export.<extension>`.
    pub(crate) fn initial_output(&self) -> PathBuf {
        let mut muxers = self
            .components
            .iter()
            .filter(|info| info.origin != MediaOrigin::Project)
            .filter_map(|info| match &info.entry {
                ComponentEntry::Muxer(entry) => Some(entry),
                _ => None,
            });
        let writes_mp4 = |entry: &MediaMuxerEntry| {
            entry
                .extensions
                .iter()
                .any(|extension| extension.eq_ignore_ascii_case("mp4"))
        };
        let extension = if muxers.clone().any(writes_mp4) {
            "mp4"
        } else {
            muxers
                .find_map(|entry| entry.extensions.first())
                .map_or("mp4", String::as_str)
        };
        PathBuf::from(format!("export.{extension}"))
    }

    /// Takes a resolve's components, and the settings it chose that are
    /// not their defaults, as the choice; `selected` is the preset it was
    /// resolved from.
    pub(crate) fn apply(
        &mut self,
        resolved: &ResolvedOutput,
        selected: Option<(PresetSource, String)>,
    ) {
        self.choice = Some(DialogChoice {
            muxer: resolved.muxer.reference.clone(),
            video: pick(&resolved.video),
            audio: resolved.audio.as_ref().map(pick),
        });
        self.selected = selected;
        self.warnings = resolved.warnings.clone();
        self.error = None;
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
        let Some((_, entry)) = self.muxer() else {
            return Vec::new();
        };
        let accepted: &[String] = match track {
            Track::Video => &entry.video,
            Track::Audio => entry.audio.as_deref().unwrap_or_default(),
        };
        self.components
            .iter()
            .filter(|info| info.kind == kind(track) && writes_any(info, accepted))
            .map(|info| self.option(info))
            .collect()
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
            (Track::Video, Some(pick)) => choice.video = pick,
            (Track::Video, None) => return,
            (Track::Audio, pick) => choice.audio = pick,
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
        // Dropping one value can hide another (its `visibleWhen` named the
        // first), so drop until none is hidden.
        loop {
            let values = effective(&schema, &pick.settings);
            let hidden: Vec<String> = pick
                .settings
                .keys()
                .filter(|given| *given != key)
                .filter(|given| {
                    schema
                        .iter()
                        .find(|field| field.key == **given)
                        .is_some_and(|field| !is_visible(field, &values))
                })
                .cloned()
                .collect();
            if hidden.is_empty() {
                break;
            }
            for given in hidden {
                pick.settings.remove(&given);
            }
        }
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
        let (_, entry) = self.muxer()?;
        entry.extensions.first().cloned()
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

    fn muxer(&self) -> Option<(&MediaComponentInfo, &MediaMuxerEntry)> {
        let info = self.info(&self.choice.as_ref()?.muxer, ComponentKind::Muxer)?;
        match &info.entry {
            ComponentEntry::Muxer(entry) => Some((info, entry)),
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
            options: pick
                .settings
                .iter()
                .map(|(key, value)| (key.clone(), SettingInput::Value(value.clone())))
                .collect(),
        })
    }
}

/// Bundled plugins' components first, then the user's, then a project's.
fn rank(origin: MediaOrigin) -> u8 {
    match origin {
        MediaOrigin::Bundled => 0,
        MediaOrigin::User => 1,
        MediaOrigin::Project => 2,
    }
}

fn kind(track: Track) -> ComponentKind {
    match track {
        Track::Video => ComponentKind::VideoEncoder,
        Track::Audio => ComponentKind::AudioEncoder,
    }
}

fn label(info: &MediaComponentInfo) -> String {
    let mut label = format!("{} · {}", info.name, info.reference.plugin);
    if info.origin == MediaOrigin::Project {
        label.push_str(" (project)");
    }
    label
}

fn loaded(state: &PresetFileState) -> Option<&PresetsFile> {
    state.file.as_ref().ok()?.as_ref()
}

fn listed(state: &PresetFileState) -> Vec<&Preset> {
    loaded(state)
        .map(|file| file.presets.iter().collect())
        .unwrap_or_default()
}

/// A resolved encoder as the dialog holds it: its settings that differ
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

/// Every field's default, overridden by `settings`.
fn effective(schema: &[SettingField], settings: &Settings) -> Settings {
    let mut values: Settings = schema
        .iter()
        .map(|field| (field.key.clone(), field.default.clone()))
        .collect();
    values.extend(
        settings
            .iter()
            .map(|(key, value)| (key.clone(), value.clone())),
    );
    values
}

/// Whether `component` writes a codec of a family `accepted` lists.
fn writes_any(component: &MediaComponentInfo, accepted: &[String]) -> bool {
    let codecs: &[String] = match &component.entry {
        ComponentEntry::VideoEncoder(entry) => &entry.codecs,
        ComponentEntry::AudioEncoder(entry) => &entry.codecs,
        ComponentEntry::Decoder(_) | ComponentEntry::Muxer(_) => &[],
    };
    codecs.iter().any(|codec| {
        let family = codec_family(codec);
        accepted
            .iter()
            .any(|accepted| codec_family(accepted) == family)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_exporter::resolve_preset_output;
    use celesta_media::{MediaError, RegisteredPlugin};
    use celesta_plugin_host::{MediaAudioEncoderEntry, MediaManifest, MediaVideoEncoderEntry};
    use serde_json::json;
    use std::path::Path;

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

    /// What `plugin` describes: every component available, `b/x264` with
    /// [`x264_settings`], except `unavailable`'s, each for its reason.
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
    fn model(registry: &MediaRegistry) -> ExportDialogModel {
        let (project, user) = no_files();
        ExportDialogModel::new(registry, project, user)
    }

    /// [`model`] with every plugin described and `output`'s defaults
    /// applied.
    fn resolved_model(registry: &MediaRegistry, output: &str) -> ExportDialogModel {
        let mut model = model(registry);
        for plugin in 0..registry.plugins().len() {
            model.described(plugin, description(registry, plugin, &[]));
        }
        model.apply(
            &resolve(registry, &OutputChoice::default(), None, output),
            None,
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
            ["MP4 · b", "IVF · b", "MP4 · p (project)"]
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
        model.apply(&resolve(&registry, &ivf, None, "export.ivf"), None);
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
            ["H.264 (x264) · b", "H.264 (project) · p (project)"]
        );
        assert_eq!(labels(&mp4.encoder_options(Track::Audio)), ["AAC · b"]);

        let ivf = resolved_model(&registry, "export.ivf");
        assert_eq!(labels(&ivf.encoder_options(Track::Video)), ["VP9 · b"]);
        assert!(ivf.encoder_options(Track::Audio).is_empty());
        assert_eq!(ivf.extension().as_deref(), Some("ivf"));
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
            Err("H.264 (x264) · b: CRF must be at most 51".to_owned())
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
            Some(("web", &web)),
            "export.mp4",
        );
        assert_eq!(resolved.warnings.len(), 1);
        model.apply(&resolved, Some((PresetSource::Project, "web".into())));
        assert_eq!(model.selected, Some((PresetSource::Project, "web".into())));

        model.set_setting(Track::Video, "preset", string("slow"));
        assert_eq!(model.selected, None);
        assert_eq!(model.warnings, resolved.warnings);
    }

    #[test]
    fn choosing_an_encoder_resets_its_settings() {
        let registry = registry();
        let mut model = resolved_model(&registry, "export.mp4");
        model.apply(
            &resolve(&registry, &OutputChoice::default(), None, "export.mp4"),
            Some((PresetSource::User, "web".into())),
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
    fn the_initial_base_prefers_the_project_default() {
        let registry = registry();
        let web = json!({ "name": "web", "muxer": "b/mp4" });
        let project = |default: Option<&str>| {
            let mut contents = json!({ "version": 1, "presets": [web] });
            if let Some(default) = default {
                contents["default"] = json!(default);
            }
            file(Some("/project/export-presets.json"), Some(contents))
        };
        let user = file(
            Some("/home/export-presets.json"),
            Some(json!({ "version": 1, "last": { "muxer": "b/ivf" } })),
        );
        let muxer = |base: Option<(String, PresetOutput)>| {
            base.map(|(name, output)| (name, output.muxer.unwrap()))
        };

        let model = ExportDialogModel::new(&registry, project(Some("web")), user.clone());
        assert_eq!(
            muxer(model.initial_base()),
            Some(("web".into(), "b/mp4".into()))
        );
        let model = ExportDialogModel::new(&registry, project(None), user);
        assert_eq!(
            muxer(model.initial_base()),
            Some(("last".into(), "b/ivf".into()))
        );
        let (project, user) = no_files();
        let model = ExportDialogModel::new(&registry, project, user);
        assert_eq!(model.initial_base(), None);
    }

    #[test]
    fn the_initial_output_takes_the_mp4_muxer() {
        assert_eq!(
            model(&registry()).initial_output(),
            PathBuf::from("export.mp4")
        );
        let mut ivf_only = bundled();
        ivf_only.media.muxers = vec![ivf()];
        let registry = MediaRegistry::new(vec![project(), ivf_only]);
        assert_eq!(
            model(&registry).initial_output(),
            PathBuf::from("export.ivf")
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
        let model = ExportDialogModel::new(&registry, project, user);
        assert_eq!(model.can_save(SaveTarget::User), Err(message.to_owned()));
        assert_eq!(model.can_save(SaveTarget::Project), Ok(()));
        assert!(model.presets().1.is_empty());
    }

    #[test]
    fn an_untitled_project_cannot_take_a_preset() {
        let registry = registry();
        let (_, user) = no_files();
        let model = ExportDialogModel::new(&registry, file(None, None), user);
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
        let model = ExportDialogModel::new(&registry, no_files().0, user);
        assert!(model.replaces(SaveTarget::User, " web "));
        assert!(!model.replaces(SaveTarget::User, "Web"));
        assert!(!model.replaces(SaveTarget::Project, "web"));
        let (project, user) = model.presets();
        assert!(project.is_empty());
        assert_eq!(user[0].name, "web");
    }
}
