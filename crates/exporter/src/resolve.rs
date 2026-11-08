//! Turning an [`OutputChoice`] into the components that write an export:
//! the spec's "Resolving an output".

use crate::error::ExportError;
use crate::options::{AudioChoice, EncoderChoice, OutputChoice, SettingInput};
use celesta_media::settings::{differences, parse_setting, validate};
use celesta_media::{
    ComponentDescription, ComponentEntry, ComponentKind, ComponentRef, InvalidSetting,
    MediaComponentInfo, MediaRegistry, PluginDescription, SettingField, SettingKind, SettingValue,
    Settings,
};
use celesta_plugin_host::{MediaMuxerEntry, MediaOrigin, codec_family};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::sync::Arc;

/// libx264's preset names, fastest first: `--preset` once named these,
/// and an [`ExportError::UnknownPreset`] of one hints at the encoder option.
pub const X264_PRESETS: [&str; 10] = [
    "ultrafast",
    "superfast",
    "veryfast",
    "faster",
    "fast",
    "medium",
    "slow",
    "slower",
    "veryslow",
    "placebo",
];

/// A chosen video or audio encoder: its plugin's index in the registry,
/// the codec families it writes, its validated settings (defaults filled
/// in, hidden fields left out), and the schema they were checked against.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedEncoder {
    pub plugin: usize,
    pub reference: ComponentRef,
    pub name: String,
    pub codecs: Vec<String>,
    pub settings: Settings,
    pub schema: Vec<SettingField>,
}

/// The chosen muxer, as [`ResolvedEncoder`] is an encoder, with its
/// manifest entry.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedMuxer {
    pub plugin: usize,
    pub reference: ComponentRef,
    pub name: String,
    pub entry: MediaMuxerEntry,
    pub settings: Settings,
    pub schema: Vec<SettingField>,
}

/// The components an export writes with; `audio` is `None` when the
/// export has no audio track.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedOutput {
    pub muxer: ResolvedMuxer,
    pub video: ResolvedEncoder,
    pub audio: Option<ResolvedEncoder>,
}

/// An encoder as the progress display names it: the settings that differ
/// from their defaults, each as a short label, in schema order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncoderPlan {
    pub component: ComponentRef,
    pub name: String,
    pub settings: Vec<String>,
}

/// [`ResolvedOutput`] as the progress display names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputPlan {
    pub muxer: ComponentRef,
    pub muxer_name: String,
    pub video: EncoderPlan,
    pub audio: Option<EncoderPlan>,
}

impl ResolvedOutput {
    pub fn plan(&self) -> OutputPlan {
        OutputPlan {
            muxer: self.muxer.reference.clone(),
            muxer_name: self.muxer.name.clone(),
            video: encoder_plan(&self.video),
            audio: self.audio.as_ref().map(encoder_plan),
        }
    }
}

fn encoder_plan(encoder: &ResolvedEncoder) -> EncoderPlan {
    EncoderPlan {
        component: encoder.reference.clone(),
        name: encoder.name.clone(),
        settings: differences(&encoder.schema, &encoder.settings)
            .into_iter()
            .map(|(field, value)| setting_label(field, value))
            .collect(),
    }
}

/// A changed setting as the progress display shows it: "CRF 20",
/// "Bitrate 4000000 bit/s", a select's option label ("Slow"), or a
/// boolean's label ("Fast start", "No Fast start").
fn setting_label(field: &SettingField, value: &SettingValue) -> String {
    let label = &field.label;
    match (&field.kind, value) {
        (SettingKind::Select { options }, SettingValue::String(chosen)) => options
            .iter()
            .find(|option| option.value == *chosen)
            .map_or_else(
                || format!("{label} {chosen}"),
                |option| option.label.clone(),
            ),
        (SettingKind::Boolean, SettingValue::Boolean(true)) => label.clone(),
        (SettingKind::Boolean, SettingValue::Boolean(false)) => format!("No {label}"),
        (
            SettingKind::Number {
                unit: Some(unit), ..
            },
            value,
        ) => format!("{label} {value} {unit}"),
        (_, value) => format!("{label} {value}"),
    }
}

/// The encoder's name and changed settings, joined with " · ":
/// "H.264 (x264) · Slow · CRF 20".
pub fn encoding_label(plan: &EncoderPlan) -> String {
    std::iter::once(plan.name.as_str())
        .chain(plan.settings.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// The components `choice` names for writing `output`, with their options
/// validated, as the spec's "Resolving an output" lays out:
///
/// 1. a preset is [`ExportError::UnknownPreset`] (presets arrive with P3);
/// 2. the muxer is the named one, which may be any installed, or else the
///    first bundled, then user, muxer writing `output`'s extension;
/// 3. the video encoder is the named one, which must write a codec the
///    muxer takes, or else the first available one writing the muxer's
///    default video codec, from the muxer's plugin, then bundled, then
///    user plugins;
/// 4. the audio encoder likewise, against the default audio codec, or none
///    for [`AudioChoice::None`] or a muxer that takes no audio;
/// 5. each chosen component must be described as available;
/// 6. options are read by each field's type and validated against the
///    schema the description brought; the muxer takes its defaults.
///
/// `describe` answers what plugin `plugin` (an index into `registry`) says
/// of its components; it is asked at most once per plugin. A project
/// plugin's components are chosen only when named, or as the encoders of
/// a named project muxer.
pub fn resolve_output(
    choice: &OutputChoice,
    output: &Path,
    registry: &MediaRegistry,
    describe: &mut dyn FnMut(usize) -> Arc<PluginDescription>,
) -> Result<ResolvedOutput, ExportError> {
    if let Some(name) = &choice.preset {
        return Err(ExportError::UnknownPreset { name: name.clone() });
    }
    let components = registry.components();
    let mut descriptions = Descriptions {
        describe,
        asked: HashMap::new(),
    };

    let muxer = match &choice.muxer {
        Some(reference) => named(&components, reference, ComponentKind::Muxer)?,
        None => default_muxer(&components, output)?,
    };
    let ComponentEntry::Muxer(entry) = &muxer.entry else {
        unreachable!("a muxer's entry is a muxer's");
    };

    let video = match &choice.video.component {
        Some(reference) => {
            let video = named(&components, reference, ComponentKind::VideoEncoder)?;
            check_writes(video, muxer, &entry.video)?;
            video
        }
        None => {
            let codec = entry.default_video.as_ref().or(entry.video.first());
            default_encoder(
                &components,
                ComponentKind::VideoEncoder,
                muxer,
                codec,
                &mut descriptions,
            )?
        }
    };

    let accepted_audio = entry.audio.as_ref().filter(|audio| !audio.is_empty());
    let audio = match (&choice.audio, accepted_audio) {
        (AudioChoice::None, _) => None,
        (AudioChoice::Encode(encoder), accepted) => {
            let audio = match (&encoder.component, accepted) {
                (None, None) => None,
                (Some(reference), None) => {
                    let audio = named(&components, reference, ComponentKind::AudioEncoder)?;
                    return Err(ExportError::IncompatibleComponents {
                        component: audio.reference.clone(),
                        muxer: muxer.reference.clone(),
                        reason: format!("{} takes no audio", muxer.reference),
                    });
                }
                (Some(reference), Some(accepted)) => {
                    let audio = named(&components, reference, ComponentKind::AudioEncoder)?;
                    check_writes(audio, muxer, accepted)?;
                    Some(audio)
                }
                (None, Some(accepted)) => {
                    let codec = entry.default_audio.as_ref().or(accepted.first());
                    Some(default_encoder(
                        &components,
                        ComponentKind::AudioEncoder,
                        muxer,
                        codec,
                        &mut descriptions,
                    )?)
                }
            };
            audio.map(|audio| (audio, encoder))
        }
    };

    let muxer_description = descriptions.available(muxer)?;
    let video_description = descriptions.available(video)?;
    let audio = match audio {
        Some((audio, encoder)) => Some((audio, encoder, descriptions.available(audio)?)),
        None => None,
    };

    let muxer = ResolvedMuxer {
        plugin: muxer.plugin,
        reference: muxer.reference.clone(),
        name: muxer.name.clone(),
        entry: entry.clone(),
        settings: settings(muxer, &muxer_description, &BTreeMap::new())?,
        schema: muxer_description.settings,
    };
    let video = resolved_encoder(video, video_description, &choice.video)?;
    let audio = audio
        .map(|(audio, encoder, description)| resolved_encoder(audio, description, encoder))
        .transpose()?;
    Ok(ResolvedOutput {
        muxer,
        video,
        audio,
    })
}

/// Each plugin's description, asked of `describe` once.
struct Descriptions<'a> {
    describe: &'a mut dyn FnMut(usize) -> Arc<PluginDescription>,
    asked: HashMap<usize, Arc<PluginDescription>>,
}

impl Descriptions<'_> {
    /// What `component`'s plugin says of it, when it says it is available.
    fn available(
        &mut self,
        component: &MediaComponentInfo,
    ) -> Result<ComponentDescription, ExportError> {
        let description = self
            .asked
            .entry(component.plugin)
            .or_insert_with(|| (self.describe)(component.plugin));
        let found = description.components.iter().find(|described| {
            described.kind == component.kind && described.id == component.reference.component
        });
        let reason = match found {
            Some(described) if described.available => return Ok(described.clone()),
            Some(described) => described
                .reason
                .clone()
                .unwrap_or_else(|| "the plugin gave no reason".to_owned()),
            None => "the plugin did not describe it".to_owned(),
        };
        Err(ExportError::ComponentUnavailable {
            component: component.reference.clone(),
            reason,
        })
    }
}

/// The `kind` component `reference` names: [`ExportError::ComponentUnavailable`]
/// when there is none, "not installed" or, when `reference` names another
/// kind of component, "not a video encoder" and so on.
fn named<'a>(
    components: &'a [MediaComponentInfo],
    reference: &ComponentRef,
    kind: ComponentKind,
) -> Result<&'a MediaComponentInfo, ExportError> {
    let mut referenced = components
        .iter()
        .filter(|info| info.reference == *reference)
        .peekable();
    let installed = referenced.peek().is_some();
    if let Some(info) = referenced.find(|info| info.kind == kind) {
        return Ok(info);
    }
    let reason = if !installed {
        "not installed"
    } else {
        match kind {
            ComponentKind::Decoder => "not a decoder",
            ComponentKind::VideoEncoder => "not a video encoder",
            ComponentKind::AudioEncoder => "not an audio encoder",
            ComponentKind::Muxer => "not a muxer",
        }
    };
    Err(ExportError::ComponentUnavailable {
        component: reference.clone(),
        reason: reason.to_owned(),
    })
}

/// The first muxer writing `output`'s extension (ignoring case): bundled
/// plugins' first, then user plugins' in search order; never a project's.
fn default_muxer<'a>(
    components: &'a [MediaComponentInfo],
    output: &Path,
) -> Result<&'a MediaComponentInfo, ExportError> {
    let rank = |info: &MediaComponentInfo| match info.origin {
        MediaOrigin::Bundled => Some(0),
        MediaOrigin::User => Some(1),
        MediaOrigin::Project => None,
    };
    let mut candidates: Vec<(&MediaComponentInfo, &MediaMuxerEntry)> = components
        .iter()
        .filter(|info| rank(info).is_some())
        .filter_map(|info| match &info.entry {
            ComponentEntry::Muxer(entry) => Some((info, entry)),
            _ => None,
        })
        .collect();
    // Stable: search order, then manifest order, within an origin.
    candidates.sort_by_key(|(info, _)| rank(info));
    let extension = output.extension().and_then(|extension| extension.to_str());
    let writes = |entry: &MediaMuxerEntry| {
        extension.is_some_and(|extension| {
            entry
                .extensions
                .iter()
                .any(|written| written.eq_ignore_ascii_case(extension))
        })
    };
    if let Some((info, _)) = candidates.iter().find(|(_, entry)| writes(entry)) {
        return Ok(info);
    }
    let extensions: BTreeSet<String> = candidates
        .iter()
        .flat_map(|(_, entry)| entry.extensions.iter().cloned())
        .collect();
    Err(ExportError::UnsupportedOutput {
        path: output.to_owned(),
        extensions: extensions.into_iter().collect(),
    })
}

/// The codec families a video or audio encoder writes.
fn codecs(component: &MediaComponentInfo) -> &[String] {
    match &component.entry {
        ComponentEntry::VideoEncoder(entry) => &entry.codecs,
        ComponentEntry::AudioEncoder(entry) => &entry.codecs,
        ComponentEntry::Decoder(_) | ComponentEntry::Muxer(_) => &[],
    }
}

fn writes_any(component: &MediaComponentInfo, accepted: &[String]) -> bool {
    codecs(component).iter().any(|codec| {
        let family = codec_family(codec);
        accepted
            .iter()
            .any(|accepted| codec_family(accepted) == family)
    })
}

/// [`ExportError::IncompatibleComponents`] unless `encoder` writes one of
/// the codecs `muxer` takes, `accepted`.
fn check_writes(
    encoder: &MediaComponentInfo,
    muxer: &MediaComponentInfo,
    accepted: &[String],
) -> Result<(), ExportError> {
    if writes_any(encoder, accepted) {
        return Ok(());
    }
    Err(ExportError::IncompatibleComponents {
        component: encoder.reference.clone(),
        muxer: muxer.reference.clone(),
        reason: format!(
            "it writes {}, and {} takes {}",
            codecs(encoder).join(", "),
            muxer.reference,
            accepted.join(", ")
        ),
    })
}

/// The first available `kind` encoder writing `codec`: the muxer's own
/// plugin's first, then bundled plugins', then user plugins' in search
/// order; a project's only when it is the muxer's. When none is
/// available, the first candidate's reason; with none at all, the muxer
/// is unavailable.
fn default_encoder<'a>(
    components: &'a [MediaComponentInfo],
    kind: ComponentKind,
    muxer: &MediaComponentInfo,
    codec: Option<&String>,
    descriptions: &mut Descriptions<'_>,
) -> Result<&'a MediaComponentInfo, ExportError> {
    let rank = |info: &MediaComponentInfo| {
        if info.plugin == muxer.plugin {
            return Some(0);
        }
        match info.origin {
            MediaOrigin::Bundled => Some(1),
            MediaOrigin::User => Some(2),
            MediaOrigin::Project => None,
        }
    };
    let accepted: Vec<String> = codec.into_iter().cloned().collect();
    let mut candidates: Vec<&MediaComponentInfo> = components
        .iter()
        .filter(|info| info.kind == kind && rank(info).is_some() && writes_any(info, &accepted))
        .collect();
    candidates.sort_by_key(|info| rank(info));
    let mut first_reason = None;
    for candidate in candidates {
        match descriptions.available(candidate) {
            Ok(_) => return Ok(candidate),
            Err(error) => {
                first_reason.get_or_insert(error);
            }
        }
    }
    let kind = match kind {
        ComponentKind::AudioEncoder => "audio encoder",
        _ => "video encoder",
    };
    Err(
        first_reason.unwrap_or_else(|| ExportError::ComponentUnavailable {
            component: muxer.reference.clone(),
            reason: match codec {
                Some(codec) => format!("no {kind} writes {codec} by default"),
                None => format!("it names no default {kind} codec"),
            },
        }),
    )
}

fn resolved_encoder(
    component: &MediaComponentInfo,
    description: ComponentDescription,
    choice: &EncoderChoice,
) -> Result<ResolvedEncoder, ExportError> {
    Ok(ResolvedEncoder {
        plugin: component.plugin,
        reference: component.reference.clone(),
        name: component.name.clone(),
        codecs: codecs(component).to_vec(),
        settings: settings(component, &description, &choice.options)?,
        schema: description.settings,
    })
}

/// `options` read by each field's type (text) or as given (values), then
/// validated against `description`'s schema.
fn settings(
    component: &MediaComponentInfo,
    description: &ComponentDescription,
    options: &BTreeMap<String, SettingInput>,
) -> Result<Settings, ExportError> {
    let schema = &description.settings;
    let invalid = |error: InvalidSetting| ExportError::InvalidOption {
        component: component.reference.clone(),
        key: error.key,
        message: error.message,
    };
    let given = options
        .iter()
        .map(|(key, input)| {
            let value = match input {
                SettingInput::Text(text) => parse_setting(schema, key, text).map_err(invalid)?,
                SettingInput::Value(value) => value.clone(),
            };
            Ok((key.clone(), value))
        })
        .collect::<Result<Settings, ExportError>>()?;
    validate(schema, &given).map_err(invalid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{self, HintStyle};
    use celesta_media::{MediaError, RegisteredPlugin, SelectOption};
    use celesta_plugin_host::{MediaAudioEncoderEntry, MediaManifest, MediaVideoEncoderEntry};
    use serde_json::json;
    use std::path::PathBuf;

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

    fn audio_encoder(id: &str, name: &str, codec: &str) -> MediaAudioEncoderEntry {
        MediaAudioEncoderEntry {
            id: id.into(),
            name: name.into(),
            codecs: vec![codec.into()],
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
                audio_encoders: vec![audio_encoder("aac", "AAC", "mp4a")],
                muxers: vec![mp4(), ivf()],
            },
        )
    }

    /// `u`, the user's: muxer `mp4` and video encoder `h264`.
    fn user() -> RegisteredPlugin {
        plugin(
            "u",
            MediaOrigin::User,
            MediaManifest {
                decoders: Vec::new(),
                video_encoders: vec![video_encoder("h264", "H.264 (user)", "avc1")],
                audio_encoders: Vec::new(),
                muxers: vec![mp4()],
            },
        )
    }

    /// `p`, a project's: muxer `mp4` and encoders `h264` and `aac`.
    fn project() -> RegisteredPlugin {
        plugin(
            "p",
            MediaOrigin::Project,
            MediaManifest {
                decoders: Vec::new(),
                video_encoders: vec![video_encoder("h264", "H.264 (project)", "avc1")],
                audio_encoders: vec![audio_encoder("aac", "AAC (project)", "mp4a")],
                muxers: vec![mp4()],
            },
        )
    }

    /// The spec's five fields, with a `preset` select before `crf`.
    fn x264_settings() -> Vec<SettingField> {
        let mut fields: Vec<SettingField> = [
            json!({ "key": "rateControl", "type": "select", "label": "Rate control",
                "group": "Rate control",
                "options": [ { "value": "crf", "label": "Constant quality (CRF)" },
                             { "value": "vbr", "label": "Average bitrate" } ],
                "default": "crf" }),
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
        .collect();
        fields.insert(
            1,
            SettingField {
                key: "preset".into(),
                label: "Preset".into(),
                default: SettingValue::String("medium".into()),
                kind: SettingKind::Select {
                    options: vec![
                        SelectOption {
                            value: "medium".into(),
                            label: "Medium".into(),
                        },
                        SelectOption {
                            value: "slow".into(),
                            label: "Slow".into(),
                        },
                    ],
                },
                description: None,
                group: None,
                advanced: false,
                visible_when: BTreeMap::new(),
            },
        );
        fields
    }

    fn available(info: &MediaComponentInfo) -> ComponentDescription {
        let mut description = ComponentDescription::unavailable(info.kind, info.entry.id(), "");
        description.available = true;
        description.reason = None;
        if info.reference == reference("b/x264") {
            description.settings = x264_settings();
        }
        description
    }

    /// Every component available, `b/x264` with [`x264_settings`], except
    /// `unavailable`'s, each described as unavailable for its reason.
    fn describer(
        registry: &MediaRegistry,
        unavailable: &[(&str, &str)],
    ) -> impl FnMut(usize) -> Arc<PluginDescription> + use<> {
        let components = registry.components();
        let unavailable: Vec<(ComponentRef, String)> = unavailable
            .iter()
            .map(|(component, reason)| (reference(component), (*reason).to_owned()))
            .collect();
        move |plugin| {
            let components = components
                .iter()
                .filter(|info| info.plugin == plugin)
                .map(
                    |info| match unavailable.iter().find(|(r, _)| *r == info.reference) {
                        Some((_, reason)) => {
                            ComponentDescription::unavailable(info.kind, info.entry.id(), reason)
                        }
                        None => available(info),
                    },
                )
                .collect();
            Arc::new(PluginDescription { components })
        }
    }

    fn registry(plugins: Vec<RegisteredPlugin>) -> MediaRegistry {
        MediaRegistry::new(plugins)
    }

    fn all() -> MediaRegistry {
        registry(vec![bundled(), user(), project()])
    }

    fn resolve(
        registry: &MediaRegistry,
        choice: &OutputChoice,
        output: &str,
    ) -> Result<ResolvedOutput, ExportError> {
        resolve_output(
            choice,
            Path::new(output),
            registry,
            &mut describer(registry, &[]),
        )
    }

    fn with_video(component: &str) -> OutputChoice {
        OutputChoice {
            video: EncoderChoice {
                component: Some(reference(component)),
                options: BTreeMap::new(),
            },
            ..OutputChoice::default()
        }
    }

    fn with_audio(component: &str) -> OutputChoice {
        OutputChoice {
            audio: AudioChoice::Encode(EncoderChoice {
                component: Some(reference(component)),
                options: BTreeMap::new(),
            }),
            ..OutputChoice::default()
        }
    }

    fn with_video_options(options: &[(&str, SettingInput)]) -> OutputChoice {
        OutputChoice {
            video: EncoderChoice {
                component: None,
                options: options
                    .iter()
                    .map(|(key, value)| ((*key).to_owned(), value.clone()))
                    .collect(),
            },
            ..OutputChoice::default()
        }
    }

    fn text(value: &str) -> SettingInput {
        SettingInput::Text(value.into())
    }

    #[test]
    fn defaults_follow_the_extension_and_origin() {
        let registry = all();
        let resolved = resolve(&registry, &OutputChoice::default(), "film.MP4").unwrap();
        assert_eq!(resolved.muxer.reference, reference("b/mp4"));
        assert_eq!(resolved.muxer.plugin, 0);
        assert_eq!(resolved.muxer.entry, mp4());
        assert_eq!(resolved.video.reference, reference("b/x264"));
        assert_eq!(resolved.video.name, "H.264 (x264)");
        assert_eq!(resolved.video.codecs, ["avc1"]);
        assert_eq!(resolved.video.schema, x264_settings());
        assert_eq!(
            resolved.audio.map(|audio| audio.reference),
            Some(reference("b/aac"))
        );

        let error = resolve(&registry, &OutputChoice::default(), "film.mkv").unwrap_err();
        assert!(
            matches!(&error, ExportError::UnsupportedOutput { path, extensions }
                if path == Path::new("film.mkv") && extensions == &["ivf", "mp4"]),
            "{error:?}"
        );
        assert_eq!(error.to_string(), "no muxer writes .mkv files");
    }

    #[test]
    fn the_muxers_plugin_ranks_first() {
        let registry = all();
        let choice = OutputChoice {
            muxer: Some(reference("u/mp4")),
            ..OutputChoice::default()
        };
        let resolved = resolve(&registry, &choice, "film.mp4").unwrap();
        assert_eq!(resolved.muxer.reference, reference("u/mp4"));
        assert_eq!(resolved.muxer.plugin, 1);
        assert_eq!(resolved.video.reference, reference("u/h264"));
        // `u` has no audio encoder, so a bundled one is next.
        assert_eq!(
            resolved.audio.map(|audio| audio.reference),
            Some(reference("b/aac"))
        );
    }

    #[test]
    fn project_components_are_never_defaults() {
        let only_project = registry(vec![project()]);
        let error = resolve(&only_project, &OutputChoice::default(), "film.mp4").unwrap_err();
        assert!(
            matches!(&error, ExportError::UnsupportedOutput { extensions, .. } if extensions.is_empty()),
            "{error:?}"
        );
        assert_eq!(error.to_string(), "no media plugin offers a muxer");

        let choice = OutputChoice {
            muxer: Some(reference("p/mp4")),
            video: EncoderChoice {
                component: Some(reference("p/h264")),
                options: BTreeMap::new(),
            },
            audio: AudioChoice::Encode(EncoderChoice {
                component: Some(reference("p/aac")),
                options: BTreeMap::new(),
            }),
            preset: None,
        };
        let resolved = resolve(&only_project, &choice, "film.mp4").unwrap();
        assert_eq!(resolved.muxer.reference, reference("p/mp4"));
        assert_eq!(resolved.video.reference, reference("p/h264"));
        assert_eq!(
            resolved.audio.map(|audio| audio.reference),
            Some(reference("p/aac"))
        );

        // Beside bundled and user plugins, the project's never wins a default.
        let registry = all();
        let resolved = resolve(&registry, &OutputChoice::default(), "film.mp4").unwrap();
        assert_eq!(resolved.muxer.reference, reference("b/mp4"));
        let resolved = resolve(&registry, &with_video("p/h264"), "film.mp4").unwrap();
        assert_eq!(resolved.video.reference, reference("p/h264"));
    }

    #[test]
    fn an_incompatible_encoder_is_refused() {
        let registry = all();
        let error = resolve(&registry, &with_video("b/vp9"), "film.mp4").unwrap_err();
        assert!(
            matches!(&error, ExportError::IncompatibleComponents { component, muxer, .. }
                if *component == reference("b/vp9") && *muxer == reference("b/mp4")),
            "{error:?}"
        );

        let error = resolve(&registry, &with_video("b/aac"), "film.mp4").unwrap_err();
        assert!(
            matches!(&error, ExportError::ComponentUnavailable { component, reason }
                if *component == reference("b/aac") && reason == "not a video encoder"),
            "{error:?}"
        );
        let error = resolve(&registry, &with_audio("b/x264"), "film.mp4").unwrap_err();
        assert!(
            matches!(&error, ExportError::ComponentUnavailable { reason, .. }
                if reason == "not an audio encoder"),
            "{error:?}"
        );
        let choice = OutputChoice {
            muxer: Some(reference("b/x264")),
            ..OutputChoice::default()
        };
        let error = resolve(&registry, &choice, "film.mp4").unwrap_err();
        assert!(
            matches!(&error, ExportError::ComponentUnavailable { reason, .. }
                if reason == "not a muxer"),
            "{error:?}"
        );
        for missing in ["x/x264", "b/nothing"] {
            let error = resolve(&registry, &with_video(missing), "film.mp4").unwrap_err();
            assert!(
                matches!(&error, ExportError::ComponentUnavailable { component, reason }
                    if *component == reference(missing) && reason == "not installed"),
                "{error:?}"
            );
        }
    }

    #[test]
    fn audio_none_and_muxers_without_audio() {
        let registry = all();
        let choice = OutputChoice {
            audio: AudioChoice::None,
            ..OutputChoice::default()
        };
        let resolved = resolve(&registry, &choice, "film.mp4").unwrap();
        assert_eq!(resolved.audio, None);

        let resolved = resolve(&registry, &OutputChoice::default(), "film.ivf").unwrap();
        assert_eq!(resolved.muxer.reference, reference("b/ivf"));
        assert_eq!(resolved.video.reference, reference("b/vp9"));
        assert_eq!(resolved.audio, None);

        let error = resolve(&registry, &with_audio("b/aac"), "film.ivf").unwrap_err();
        assert!(
            matches!(&error, ExportError::IncompatibleComponents { component, muxer, .. }
                if *component == reference("b/aac") && *muxer == reference("b/ivf")),
            "{error:?}"
        );
    }

    #[test]
    fn an_unavailable_component_names_its_reason() {
        let reason = "this FFmpeg build has no libx264";
        let bundled_only = registry(vec![bundled()]);
        for choice in [OutputChoice::default(), with_video("b/x264")] {
            let error = resolve_output(
                &choice,
                Path::new("film.mp4"),
                &bundled_only,
                &mut describer(&bundled_only, &[("b/x264", reason)]),
            )
            .unwrap_err();
            assert!(
                matches!(&error, ExportError::ComponentUnavailable { component, reason: why }
                    if *component == reference("b/x264") && why == reason),
                "{error:?}"
            );
        }

        // A default skips an unavailable candidate for the next one.
        let registry = all();
        let resolved = resolve_output(
            &OutputChoice::default(),
            Path::new("film.mp4"),
            &registry,
            &mut describer(&registry, &[("b/x264", reason)]),
        )
        .unwrap();
        assert_eq!(resolved.video.reference, reference("u/h264"));

        // The muxer is checked too.
        let error = resolve_output(
            &OutputChoice::default(),
            Path::new("film.mp4"),
            &registry,
            &mut describer(&registry, &[("b/mp4", "broken")]),
        )
        .unwrap_err();
        assert!(
            matches!(&error, ExportError::ComponentUnavailable { component, reason }
                if *component == reference("b/mp4") && reason == "broken"),
            "{error:?}"
        );
    }

    #[test]
    fn each_plugin_is_described_once() {
        let registry = all();
        let mut inner = describer(&registry, &[]);
        let mut asked = Vec::new();
        resolve_output(
            &OutputChoice::default(),
            Path::new("film.mp4"),
            &registry,
            &mut |plugin| {
                asked.push(plugin);
                inner(plugin)
            },
        )
        .unwrap();
        assert_eq!(asked, [0]);
    }

    #[test]
    fn options_are_parsed_and_validated() {
        let registry = all();
        let resolved = resolve(
            &registry,
            &with_video_options(&[("crf", text("20"))]),
            "film.mp4",
        )
        .unwrap();
        assert_eq!(resolved.video.settings["crf"], SettingValue::Number(20.0));
        // Defaults fill the rest; the hidden bitrate is left out.
        assert_eq!(
            resolved.video.settings["preset"],
            SettingValue::String("medium".into())
        );
        assert!(!resolved.video.settings.contains_key("bitrate"));

        let invalid = |options: &[(&str, SettingInput)]| match resolve(
            &registry,
            &with_video_options(options),
            "film.mp4",
        ) {
            Err(ExportError::InvalidOption {
                component,
                key,
                message,
            }) => (component, key, message),
            other => panic!("{other:?}"),
        };
        assert_eq!(
            invalid(&[("crf", text("x"))]),
            (reference("b/x264"), "crf".into(), "must be a number".into())
        );
        assert_eq!(
            invalid(&[(
                "bitrate",
                SettingInput::Value(SettingValue::Number(4_000_000.0))
            )]),
            (
                reference("b/x264"),
                "bitrate".into(),
                "applies only when rateControl is vbr".into()
            )
        );
        assert_eq!(invalid(&[("quality", text("1"))]).2, "unknown setting");
        let error = resolve(
            &registry,
            &with_video_options(&[("crf", text("x"))]),
            "film.mp4",
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "b/x264: crf must be a number");
    }

    #[test]
    fn an_x264_preset_name_gets_the_video_option_hint() {
        let registry = all();
        let preset = |name: &str| OutputChoice {
            preset: Some(name.into()),
            ..OutputChoice::default()
        };
        let error = resolve(&registry, &preset("ultrafast"), "film.mp4").unwrap_err();
        assert!(
            matches!(&error, ExportError::UnknownPreset { name } if name == "ultrafast"),
            "{error:?}"
        );
        assert_eq!(error.to_string(), "unknown preset `ultrafast`");
        let mut issues = Vec::new();
        assert_eq!(
            report::classify(&error, HintStyle::Cli, &mut issues),
            (
                "unknown_preset",
                Some("use --video-option preset=ultrafast".into())
            )
        );
        assert_eq!(
            report::classify(&error, HintStyle::Mcp, &mut issues),
            (
                "unknown_preset",
                Some("set videoEncoder.options.preset to ultrafast".into())
            )
        );

        let error = resolve(&registry, &preset("YouTube"), "film.mp4").unwrap_err();
        assert_eq!(
            report::classify(&error, HintStyle::Cli, &mut issues),
            ("unknown_preset", None)
        );
    }

    #[test]
    fn the_plan_names_components_and_changed_settings() {
        let registry = all();
        let resolved = resolve(
            &registry,
            &with_video_options(&[("crf", text("20")), ("preset", text("slow"))]),
            "film.mp4",
        )
        .unwrap();
        let plan = resolved.plan();
        assert_eq!(plan.muxer, reference("b/mp4"));
        assert_eq!(plan.muxer_name, "MP4");
        assert_eq!(plan.video.component, reference("b/x264"));
        assert_eq!(plan.video.settings, ["Slow", "CRF 20"]);
        assert_eq!(encoding_label(&plan.video), "H.264 (x264) · Slow · CRF 20");
        let audio = plan.audio.unwrap();
        assert_eq!(audio.component, reference("b/aac"));
        assert_eq!(encoding_label(&audio), "AAC");
    }

    #[test]
    fn plan_labels_follow_the_setting_type() {
        let registry = all();
        let resolved = resolve(
            &registry,
            &with_video_options(&[
                ("rateControl", text("vbr")),
                ("bitrate", text("4000000")),
                ("tune", text("film")),
                ("faststart", text("false")),
            ]),
            "film.mp4",
        )
        .unwrap();
        assert_eq!(
            resolved.plan().video.settings,
            [
                "Average bitrate",
                "Bitrate 4000000 bit/s",
                "Tune film",
                "No Fast start"
            ]
        );
    }
}
