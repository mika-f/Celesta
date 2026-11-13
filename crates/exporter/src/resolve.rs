//! Turning an [`OutputChoice`] into the components that write an export:
//! the spec's "Resolving an output".

use crate::error::{ExportError, UnavailableReason};
use crate::options::{AudioChoice, EncoderChoice, OutputChoice, SettingInput};
use crate::presets::{FoundPreset, PresetAudio, PresetEncoder, PresetLookup, PresetOutput};
use crate::resolve_warning::ResolveWarning;
use celesta_media::settings::{differences, layer, parse_setting, sanitize, validate};
use celesta_media::{
    ComponentDescription, ComponentEntry, ComponentKind, ComponentRef, InvalidSetting,
    MediaComponentInfo, MediaRegistry, PixelFormat, PluginDescription, SettingField, SettingKind,
    SettingValue, Settings, default_rank,
};
use celesta_plugin_host::{MediaMuxerEntry, MediaOrigin, RESERVED_PLUGIN_ID, codec_family};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A chosen video or audio encoder: its plugin's index in the registry,
/// the codec families it writes, its validated settings (defaults filled
/// in, hidden fields left out), the schema they were checked against, and
/// the formats and transports it described taking (a video encoder's: the
/// formats size its process's ring of frame slots, which is used only when
/// the transports list `sharedMemory`).
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedEncoder {
    pub plugin: usize,
    pub reference: ComponentRef,
    pub name: String,
    pub codecs: Vec<String>,
    pub settings: Settings,
    pub schema: Vec<SettingField>,
    pub input_formats: Vec<PixelFormat>,
    pub transports: Vec<String>,
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
    /// What the export is told before it starts: a preset's stale values
    /// and components, left out or replaced by defaults, and choices that
    /// work but may surprise (a muxer named for another extension).
    pub warnings: Vec<ResolveWarning>,
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
/// 1. the base is the preset `choice` names, looked up in `presets`, or,
///    when `choice` names no preset and no component (an
///    [`AudioChoice::None`] is one), the project file's `default`; a name
///    found in no file is [`ExportError::UnknownPreset`];
/// 2. the muxer is the named one, which may be any installed, or else the
///    base's, or else the first bundled, then user, muxer writing
///    `output`'s extension;
/// 3. the video encoder is the named one, which must write a codec the
///    muxer takes, or else the base's, or else the first available one
///    writing the muxer's default video codec, from the muxer's plugin,
///    then bundled, then user plugins;
/// 4. the audio encoder likewise, against the default audio codec, or none
///    for [`AudioChoice::None`], a base leaving the audio out, or a muxer
///    that takes no audio;
/// 5. each chosen component must be described as available;
/// 6. options are read by each field's type and validated against the
///    schema the description brought, over the base's options for its own
///    encoder; the muxer takes its defaults.
///
/// A base's component that cannot be used falls back to the default, and
/// a base's value the schema does not take is left out, each with a
/// warning in [`ResolvedOutput::warnings`]; the choice's own values stay
/// strict. The preset's lookup warnings come first, and a project
/// `default` naming a project plugin's component warns that it does.
///
/// `describe` answers what plugin `plugin` (an index into `registry`) says
/// of its components; it is asked at most once per plugin. A project
/// plugin's components are chosen only when named (a preset names them),
/// or as the encoders of a named project muxer.
pub fn resolve_output(
    choice: &OutputChoice,
    output: &Path,
    registry: &MediaRegistry,
    presets: &PresetLookup,
    describe: &mut dyn FnMut(usize) -> Arc<PluginDescription>,
) -> Result<ResolvedOutput, ExportError> {
    let find = |name: &str, default_in: Option<PathBuf>| {
        presets
            .find(name)?
            .ok_or_else(|| ExportError::UnknownPreset {
                name: name.to_owned(),
                default_in,
            })
    };
    let (found, by_default) = match &choice.preset {
        Some(name) => (Some(find(name, None)?), false),
        None if names_no_component(choice) => match presets.project_default()? {
            Some(name) => (Some(find(&name, presets.project.clone())?), true),
            None => (None, false),
        },
        None => (None, false),
    };
    let Some(FoundPreset {
        preset, warnings, ..
    }) = found
    else {
        return resolve_preset_output(choice, None, output, registry, describe);
    };
    let mut warnings: Vec<ResolveWarning> =
        warnings.into_iter().map(ResolveWarning::Lookup).collect();
    let label = format!("preset `{}`", preset.name);
    let mut resolved = resolve_preset_output(
        choice,
        Some((&label, &preset.output)),
        output,
        registry,
        describe,
    )?;
    warnings.append(&mut resolved.warnings);
    if by_default {
        let used = [
            Some((resolved.muxer.plugin, &resolved.muxer.reference)),
            Some((resolved.video.plugin, &resolved.video.reference)),
            resolved
                .audio
                .as_ref()
                .map(|audio| (audio.plugin, &audio.reference)),
        ];
        for (plugin, reference) in used.into_iter().flatten() {
            if registry.plugin(plugin).origin == MediaOrigin::Project {
                warnings.push(ResolveWarning::Lookup(format!(
                    "the project's default preset `{}` uses {reference}, from the project's plugin {}",
                    preset.name, reference.plugin
                )));
            }
        }
    }
    resolved.warnings = warnings;
    Ok(resolved)
}

/// Whether `choice` leaves every component to the defaults, so a project's
/// `default` preset applies.
fn names_no_component(choice: &OutputChoice) -> bool {
    choice.muxer.is_none()
        && choice.video.component.is_none()
        && matches!(&choice.audio, AudioChoice::Encode(audio) if audio.component.is_none())
}

/// [`resolve_output`] with the base already found: `base` is (what its
/// warnings call it, as ``preset `web` ``, and the preset), and
/// `choice.preset` is not read. Steps 2–6 are [`resolve_output`]'s.
pub fn resolve_preset_output(
    choice: &OutputChoice,
    base: Option<(&str, &PresetOutput)>,
    output: &Path,
    registry: &MediaRegistry,
    describe: &mut dyn FnMut(usize) -> Arc<PluginDescription>,
) -> Result<ResolvedOutput, ExportError> {
    let (name, base) = match base {
        Some((name, base)) => (name, Some(base)),
        None => ("", None),
    };
    let components = registry.components();
    let mut descriptions = Descriptions {
        describe,
        asked: HashMap::new(),
    };
    let mut warnings = Vec::new();

    let muxer = match (&choice.muxer, base.and_then(|base| base.muxer.as_deref())) {
        (Some(reference), _) => Some(named(
            registry,
            &components,
            reference,
            ComponentKind::Muxer,
        )?),
        (None, Some(text)) => fallback(
            &mut warnings,
            name,
            ComponentKind::Muxer,
            text,
            preset_component(
                registry,
                &components,
                text,
                ComponentKind::Muxer,
                None,
                &mut descriptions,
            ),
        ),
        (None, None) => None,
    };
    let named_muxer = muxer.is_some();
    let muxer = match muxer {
        Some(muxer) => muxer,
        None => default_muxer(registry, &components, output)?,
    };
    let ComponentEntry::Muxer(entry) = &muxer.entry else {
        unreachable!("a muxer's entry is a muxer's");
    };
    if named_muxer {
        warnings.extend(extension_warning(muxer, entry, output));
    }

    let base_video = base.and_then(|base| base.video.as_ref());
    let video = match (
        &choice.video.component,
        base_video.and_then(|video| video.component.as_deref()),
    ) {
        (Some(reference), _) => {
            let video = named(
                registry,
                &components,
                reference,
                ComponentKind::VideoEncoder,
            )?;
            check_writes(video, muxer, &entry.video)?;
            Some(video)
        }
        (None, Some(text)) => fallback(
            &mut warnings,
            name,
            ComponentKind::VideoEncoder,
            text,
            preset_component(
                registry,
                &components,
                text,
                ComponentKind::VideoEncoder,
                Some((muxer, &entry.video)),
                &mut descriptions,
            ),
        ),
        (None, None) => None,
    };
    let video = match video {
        Some(video) => video,
        None => {
            let codec = entry.default_video.as_ref().or(entry.video.first());
            default_encoder(
                registry,
                &components,
                ComponentKind::VideoEncoder,
                muxer,
                codec,
                &mut descriptions,
            )?
        }
    };

    let accepted_audio = entry.audio.as_ref().filter(|audio| !audio.is_empty());
    let base_audio = base.and_then(|base| base.audio.as_ref());
    let audio = match &choice.audio {
        AudioChoice::None => None,
        AudioChoice::Encode(encoder)
            if encoder.component.is_none() && matches!(base_audio, Some(PresetAudio::None(_))) =>
        {
            if !encoder.options.is_empty() {
                warnings.push(ResolveWarning::AudioOptionsIgnored {
                    base: name.to_owned(),
                });
            }
            None
        }
        AudioChoice::Encode(encoder) => {
            let preset_text = match base_audio {
                Some(PresetAudio::Encode(audio)) => audio.component.as_deref(),
                _ => None,
            };
            let chosen = match (&encoder.component, preset_text) {
                (Some(reference), _) => {
                    let audio = named(
                        registry,
                        &components,
                        reference,
                        ComponentKind::AudioEncoder,
                    )?;
                    let Some(accepted) = accepted_audio else {
                        return Err(ExportError::IncompatibleComponents {
                            component: audio.reference.clone(),
                            muxer: muxer.reference.clone(),
                            reason: format!("{} takes no audio", muxer.reference),
                        });
                    };
                    check_writes(audio, muxer, accepted)?;
                    Some(audio)
                }
                (None, Some(text)) => match accepted_audio {
                    None => {
                        warnings.push(ResolveWarning::NoAudio {
                            base: name.to_owned(),
                            text: text.to_owned(),
                            muxer: muxer.reference.clone(),
                        });
                        None
                    }
                    Some(accepted) => {
                        let found = preset_component(
                            registry,
                            &components,
                            text,
                            ComponentKind::AudioEncoder,
                            Some((muxer, accepted)),
                            &mut descriptions,
                        );
                        fallback(
                            &mut warnings,
                            name,
                            ComponentKind::AudioEncoder,
                            text,
                            found,
                        )
                    }
                },
                (None, None) => None,
            };
            let audio = match (chosen, accepted_audio) {
                (Some(audio), _) => Some(audio),
                // Options for an encoder the output does not have would be
                // dropped unchecked, typos and all.
                (None, None) => match encoder.options.keys().next() {
                    Some(key) => {
                        return Err(ExportError::InvalidOption {
                            component: muxer.reference.clone(),
                            key: key.clone(),
                            message: format!(
                                "needs an audio encoder, and {} takes no audio",
                                muxer.reference
                            ),
                        });
                    }
                    None => None,
                },
                (None, Some(accepted)) => {
                    let codec = entry.default_audio.as_ref().or(accepted.first());
                    Some(default_encoder(
                        registry,
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
        settings: settings(
            muxer,
            &muxer_description,
            &BTreeMap::new(),
            None,
            &mut warnings,
        )?,
        schema: muxer_description.settings,
    };
    let video_base = preset_options(base_video, &choice.video, video).map(|base| (name, base));
    let video = resolved_encoder(
        video,
        video_description,
        &choice.video,
        video_base,
        &mut warnings,
    )?;
    let base_audio = match base_audio {
        Some(PresetAudio::Encode(audio)) => Some(audio),
        _ => None,
    };
    let audio = audio
        .map(|(audio, encoder, description)| {
            let base = preset_options(base_audio, encoder, audio).map(|base| (name, base));
            resolved_encoder(audio, description, encoder, base, &mut warnings)
        })
        .transpose()?;
    Ok(ResolvedOutput {
        muxer,
        video,
        audio,
        warnings,
    })
}

/// The `kind` component a preset names as `text`, or why it cannot be
/// used: not a reference, not installed, not of that kind, not writing a
/// codec `accepted`'s muxer takes, or described unavailable.
fn preset_component<'a>(
    registry: &MediaRegistry,
    components: &'a [MediaComponentInfo],
    text: &str,
    kind: ComponentKind,
    accepted: Option<(&MediaComponentInfo, &[String])>,
    descriptions: &mut Descriptions<'_>,
) -> Result<&'a MediaComponentInfo, String> {
    let reason = |error: ExportError| match error {
        ExportError::ComponentUnavailable { reason, .. } => reason.to_string(),
        ExportError::IncompatibleComponents { reason, .. } => reason,
        other => other.to_string(),
    };
    let reference: ComponentRef = text
        .parse()
        .map_err(|_| "not a component reference".to_owned())?;
    let component = named(registry, components, &reference, kind).map_err(reason)?;
    if let Some((muxer, accepted)) = accepted {
        check_writes(component, muxer, accepted).map_err(reason)?;
    }
    descriptions.available(component).map_err(reason)?;
    Ok(component)
}

/// `found`, or `None` with the warning that the base `name` (as warnings
/// call it)'s `text` falls back to the default.
fn fallback<'a>(
    warnings: &mut Vec<ResolveWarning>,
    name: &str,
    kind: ComponentKind,
    text: &str,
    found: Result<&'a MediaComponentInfo, String>,
) -> Option<&'a MediaComponentInfo> {
    match found {
        Ok(component) => Some(component),
        Err(reason) => {
            warnings.push(ResolveWarning::Unavailable {
                base: name.to_owned(),
                kind,
                text: text.to_owned(),
                reason,
            });
            None
        }
    }
}

/// The warning for a named or preset `muxer` that does not list `output`'s
/// extension; `None` when it does or `output` has none.
fn extension_warning(
    muxer: &MediaComponentInfo,
    entry: &MediaMuxerEntry,
    output: &Path,
) -> Option<ResolveWarning> {
    let extension = output.extension()?.to_str()?;
    let writes = entry
        .extensions
        .iter()
        .any(|written| written.eq_ignore_ascii_case(extension));
    (!writes).then(|| ResolveWarning::Extension {
        muxer: muxer.reference.clone(),
        muxer_name: muxer.name.clone(),
        extension: extension.to_owned(),
    })
}

/// The base's options that apply to `component`: its own encoder's, when
/// `component` is the one the base names, or when neither the base nor the
/// choice names one.
fn preset_options<'a>(
    base: Option<&'a PresetEncoder>,
    choice: &EncoderChoice,
    component: &MediaComponentInfo,
) -> Option<&'a Settings> {
    let base = base?;
    let applies = match &base.component {
        Some(text) => text
            .parse::<ComponentRef>()
            .is_ok_and(|reference| reference == component.reference),
        None => choice.component.is_none(),
    };
    applies.then_some(&base.options)
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
            reason: reason.into(),
        })
    }
}

/// Why a component of the plugin `plugin` is unavailable when no registered
/// plugin has it: why discovery left out the program that would have
/// served it, or [`UnavailableReason::NotInstalled`].
pub(crate) fn not_installed(registry: &MediaRegistry, plugin: &str) -> UnavailableReason {
    registry
        .skipped(plugin)
        .map_or(UnavailableReason::NotInstalled, |skipped| {
            UnavailableReason::Other(skipped.unavailable_reason())
        })
}

/// The `kind` component `reference` names: [`ExportError::ComponentUnavailable`]
/// when there is none, "not installed" or, when `reference` names another
/// kind of component, "not a video encoder" and so on.
fn named<'a>(
    registry: &MediaRegistry,
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
        not_installed(registry, &reference.plugin)
    } else {
        match kind {
            ComponentKind::Decoder => "not a decoder",
            ComponentKind::VideoEncoder => "not a video encoder",
            ComponentKind::AudioEncoder => "not an audio encoder",
            ComponentKind::Muxer => "not a muxer",
        }
        .into()
    };
    Err(ExportError::ComponentUnavailable {
        component: reference.clone(),
        reason,
    })
}

/// Whether a video export may write with `muxer`: not the built-in
/// plugin's, not a sequence muxer (deviation 50 of the media components
/// spec): those write still images. For a component that is not a muxer
/// this is `true`: only muxers are ever refused.
pub fn serves_video(muxer: &MediaComponentInfo) -> bool {
    match &muxer.entry {
        ComponentEntry::Muxer(entry) => muxer_serves_video(&muxer.reference.plugin, entry),
        _ => true,
    }
}

pub(crate) fn muxer_serves_video(plugin: &str, entry: &MediaMuxerEntry) -> bool {
    plugin != RESERVED_PLUGIN_ID && !entry.sequence
}

/// The reason a video export refuses a muxer that is not [`serves_video`].
pub(crate) const STILL_MUXER_REASON: &str =
    "it writes still images; select frames to export stills";

/// The first muxer writing `output`'s extension (ignoring case): by
/// [`default_rank`], then in search order; never a project's, and only one
/// that [`serves_video`] (no built-in or sequence muxer).
fn default_muxer<'a>(
    registry: &MediaRegistry,
    components: &'a [MediaComponentInfo],
    output: &Path,
) -> Result<&'a MediaComponentInfo, ExportError> {
    let rank = |info: &MediaComponentInfo| default_rank(registry.plugin(info.plugin));
    let mut candidates: Vec<(&MediaComponentInfo, &MediaMuxerEntry)> = components
        .iter()
        .filter(|info| rank(info).is_some() && serves_video(info))
        .filter_map(|info| match &info.entry {
            ComponentEntry::Muxer(entry) => Some((info, entry)),
            _ => None,
        })
        .collect();
    // Stable: search order, then manifest order, within a rank.
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

/// Whether `component` (a video or audio encoder) writes a codec of the
/// same family (`codec_family`) as one of `accepted`, the codecs a muxer
/// takes.
pub fn writes_any_codec(component: &MediaComponentInfo, accepted: &[String]) -> bool {
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
    if writes_any_codec(encoder, accepted) {
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
/// plugin's first, then by [`default_rank`] and search order; a project's
/// only when it is the muxer's. When none is
/// available, the first candidate's reason; with none at all, the muxer
/// is unavailable.
fn default_encoder<'a>(
    registry: &MediaRegistry,
    components: &'a [MediaComponentInfo],
    kind: ComponentKind,
    muxer: &MediaComponentInfo,
    codec: Option<&String>,
    descriptions: &mut Descriptions<'_>,
) -> Result<&'a MediaComponentInfo, ExportError> {
    let rank = |info: &MediaComponentInfo| {
        if info.plugin == muxer.plugin {
            return Some((false, None));
        }
        default_rank(registry.plugin(info.plugin)).map(|rank| (true, Some(rank)))
    };
    let accepted: Vec<String> = codec.into_iter().cloned().collect();
    let mut candidates: Vec<&MediaComponentInfo> = components
        .iter()
        .filter(|info| {
            info.kind == kind && rank(info).is_some() && writes_any_codec(info, &accepted)
        })
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
    Err(
        first_reason.unwrap_or_else(|| ExportError::ComponentUnavailable {
            component: muxer.reference.clone(),
            reason: match codec {
                Some(codec) => UnavailableReason::NoDefaultEncoder {
                    kind,
                    codec: codec.clone(),
                },
                None => format!(
                    "it names no default {} codec",
                    match kind {
                        ComponentKind::AudioEncoder => "audio encoder",
                        _ => "video encoder",
                    }
                )
                .into(),
            },
        }),
    )
}

fn resolved_encoder(
    component: &MediaComponentInfo,
    description: ComponentDescription,
    choice: &EncoderChoice,
    base: Option<(&str, &Settings)>,
    warnings: &mut Vec<ResolveWarning>,
) -> Result<ResolvedEncoder, ExportError> {
    Ok(ResolvedEncoder {
        plugin: component.plugin,
        reference: component.reference.clone(),
        name: component.name.clone(),
        codecs: codecs(component).to_vec(),
        settings: settings(component, &description, &choice.options, base, warnings)?,
        schema: description.settings,
        input_formats: description.input_formats,
        transports: description.transports,
    })
}

/// `options` read by each field's type (text) or as given (values), then
/// validated against `description`'s schema: alone, or layered over
/// `base`'s options (what warnings call the preset, its options) once those the schema
/// does not take are left out, each with a warning.
fn settings(
    component: &MediaComponentInfo,
    description: &ComponentDescription,
    options: &BTreeMap<String, SettingInput>,
    base: Option<(&str, &Settings)>,
    warnings: &mut Vec<ResolveWarning>,
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
    let Some((name, base)) = base else {
        return validate(schema, &given).map_err(invalid);
    };
    let (kept, dropped) = sanitize(schema, base);
    let reference = &component.reference;
    for InvalidSetting { key, message } in dropped {
        let base = name.to_owned();
        let component = reference.clone();
        warnings.push(match schema.iter().find(|field| field.key == key) {
            Some(field) => ResolveWarning::StaleValue {
                base,
                component,
                label: field.label.clone(),
                key,
                message,
            },
            None => ResolveWarning::UnknownSetting {
                base,
                component,
                key,
            },
        });
    }
    layer(schema, &kept, &given).map_err(invalid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::{PresetLookup, PresetsFile};
    use crate::report::{self, HintStyle};
    use celesta_media::{MediaError, RegisteredPlugin, SelectOption};
    use celesta_plugin_host::{MediaAudioEncoderEntry, MediaManifest, MediaVideoEncoderEntry};
    use serde_json::json;
    use std::path::PathBuf;

    fn reference(text: &str) -> ComponentRef {
        text.parse().unwrap()
    }

    fn texts(warnings: &[ResolveWarning]) -> Vec<String> {
        warnings.iter().map(ToString::to_string).collect()
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
            name: id.into(),
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
                priority: 0,
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
                priority: 0,
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

    /// `u`, `b`, `p`, in the order discovery finds them: the user's
    /// plugins first, so bundled-before-user ranking has to sort.
    fn all() -> MediaRegistry {
        registry(vec![user(), bundled(), project()])
    }

    fn resolve(
        registry: &MediaRegistry,
        choice: &OutputChoice,
        output: &str,
    ) -> Result<ResolvedOutput, ExportError> {
        resolve_with(registry, choice, output, &PresetLookup::default())
    }

    fn resolve_with(
        registry: &MediaRegistry,
        choice: &OutputChoice,
        output: &str,
        presets: &PresetLookup,
    ) -> Result<ResolvedOutput, ExportError> {
        resolve_output(
            choice,
            Path::new(output),
            registry,
            presets,
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
    fn a_video_never_defaults_to_an_image_muxer() {
        let mut sequence = user();
        sequence.id = "seq".into();
        sequence.media.muxers = vec![MediaMuxerEntry {
            id: "tif".into(),
            name: "TIFF sequence".into(),
            extensions: vec!["tif".into()],
            video: vec!["avc1".into()],
            audio: None,
            default_video: None,
            default_audio: None,
            trims_leading_audio: false,
            sequence: true,
        }];
        let mut plugins = vec![sequence, bundled()];
        let registry = crate::media::with_builtin(std::mem::take(&mut plugins));
        for output in ["out.png", "out.tif"] {
            let error = resolve(&registry, &OutputChoice::default(), output).unwrap_err();
            assert!(
                matches!(&error, ExportError::UnsupportedOutput { extensions, .. }
                    if extensions == &["ivf", "mp4"]),
                "{output}: {error:?}"
            );
        }
        let image = registry
            .components()
            .into_iter()
            .find(|info| info.reference == reference("builtin/image"))
            .unwrap();
        assert!(!serves_video(&image));
        let mp4 = registry
            .components()
            .into_iter()
            .find(|info| info.reference == reference("b/mp4"))
            .unwrap();
        assert!(serves_video(&mp4));
    }

    #[test]
    fn defaults_follow_the_extension_and_origin() {
        let registry = all();
        let resolved = resolve(&registry, &OutputChoice::default(), "film.MP4").unwrap();
        assert_eq!(resolved.muxer.reference, reference("b/mp4"));
        assert_eq!(resolved.muxer.plugin, 1);
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

    /// A plugin `id` whose muxer `m` writes `.ext` files taking `codec`,
    /// with video encoder `enc` writing it.
    fn writer(
        id: &str,
        origin: MediaOrigin,
        muxer: &str,
        ext: &str,
        codec: &str,
    ) -> RegisteredPlugin {
        plugin(
            id,
            origin,
            MediaManifest {
                priority: 0,
                decoders: Vec::new(),
                video_encoders: vec![video_encoder("enc", "Encoder", codec)],
                audio_encoders: Vec::new(),
                muxers: vec![MediaMuxerEntry {
                    id: muxer.into(),
                    name: muxer.into(),
                    extensions: vec![ext.into()],
                    video: vec![codec.into()],
                    audio: None,
                    default_video: Some(codec.into()),
                    default_audio: None,
                    trims_leading_audio: false,
                    sequence: false,
                }],
            },
        )
    }

    #[test]
    fn the_bundled_plugin_with_the_highest_priority_is_the_default() {
        let preferred = RegisteredPlugin {
            media: MediaManifest {
                priority: 1,
                ..writer("second", MediaOrigin::Bundled, "mp4", "mp4", "avc1").media
            },
            ..writer("second", MediaOrigin::Bundled, "mp4", "mp4", "avc1")
        };
        let registry = registry(vec![
            writer("first", MediaOrigin::Bundled, "mp4", "mp4", "avc1"),
            preferred,
        ]);
        let resolved = resolve(&registry, &OutputChoice::default(), "film.mp4").unwrap();
        assert_eq!(resolved.muxer.reference, reference("second/mp4"));
        assert_eq!(resolved.video.reference, reference("second/enc"));

        let choice = OutputChoice {
            muxer: Some(reference("first/mp4")),
            ..OutputChoice::default()
        };
        let resolved = resolve(&registry, &choice, "film.mp4").unwrap();
        assert_eq!(resolved.muxer.reference, reference("first/mp4"));
        assert_eq!(resolved.video.reference, reference("first/enc"));

        // Without priorities, search order decides.
        let registry = registry_of_writers(&["first", "second"]);
        let resolved = resolve(&registry, &OutputChoice::default(), "film.mp4").unwrap();
        assert_eq!(resolved.muxer.reference, reference("first/mp4"));
    }

    fn registry_of_writers(ids: &[&str]) -> MediaRegistry {
        registry(
            ids.iter()
                .map(|id| writer(id, MediaOrigin::Bundled, "mp4", "mp4", "avc1"))
                .collect(),
        )
    }

    #[test]
    fn built_in_encoders_rank_before_bundled() {
        let user_muxer = RegisteredPlugin {
            media: MediaManifest {
                video_encoders: Vec::new(),
                ..writer("u", MediaOrigin::User, "img", "png", "png").media
            },
            ..writer("u", MediaOrigin::User, "img", "png", "png")
        };
        let registry = registry(vec![
            user_muxer,
            writer("b", MediaOrigin::Bundled, "other", "bmp", "png"),
            writer("builtin", MediaOrigin::BuiltIn, "image", "jpg", "png"),
        ]);
        let resolved = resolve(&registry, &OutputChoice::default(), "pic.png").unwrap();
        assert_eq!(resolved.muxer.reference, reference("u/img"));
        assert_eq!(resolved.video.reference, reference("builtin/enc"));
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
        assert_eq!(resolved.muxer.plugin, 0);
        assert_eq!(resolved.video.reference, reference("u/h264"));
        // `u` has no audio encoder, so a bundled one is next.
        assert_eq!(
            resolved.audio.map(|audio| audio.reference),
            Some(reference("b/aac"))
        );
    }

    #[test]
    fn a_named_project_muxer_brings_its_own_encoders() {
        let registry = all();
        let choice = OutputChoice {
            muxer: Some(reference("p/mp4")),
            ..OutputChoice::default()
        };
        let resolved = resolve(&registry, &choice, "film.mp4").unwrap();
        assert_eq!(resolved.muxer.reference, reference("p/mp4"));
        assert_eq!(resolved.muxer.plugin, 2);
        assert_eq!(resolved.video.reference, reference("p/h264"));
        assert_eq!(
            resolved.audio.map(|audio| audio.reference),
            Some(reference("p/aac"))
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
                if *component == reference("b/aac") && reason.to_string() == "not a video encoder"),
            "{error:?}"
        );
        let error = resolve(&registry, &with_audio("b/x264"), "film.mp4").unwrap_err();
        assert!(
            matches!(&error, ExportError::ComponentUnavailable { reason, .. }
                if reason.to_string() == "not an audio encoder"),
            "{error:?}"
        );
        let choice = OutputChoice {
            muxer: Some(reference("b/x264")),
            ..OutputChoice::default()
        };
        let error = resolve(&registry, &choice, "film.mp4").unwrap_err();
        assert!(
            matches!(&error, ExportError::ComponentUnavailable { reason, .. }
                if reason.to_string() == "not a muxer"),
            "{error:?}"
        );
        for missing in ["x/x264", "b/nothing"] {
            let error = resolve(&registry, &with_video(missing), "film.mp4").unwrap_err();
            assert!(
                matches!(&error, ExportError::ComponentUnavailable { component, reason }
                    if *component == reference(missing) && *reason == UnavailableReason::NotInstalled),
                "{error:?}"
            );
        }
    }

    #[test]
    fn a_component_of_a_program_skipped_at_discovery_says_why() {
        let skipped = |id: &str| celesta_plugin_host::SkippedMedia {
            id: id.to_owned(),
            program: "/bin/celesta-codec-ffmpeg-x264".into(),
            reason: "--manifest failed (exit status: 1): no libx264".to_owned(),
        };
        // "b" is registered, so a program skipped for it is ignored.
        let registry = all().with_skipped(vec![skipped("x"), skipped("b")]);
        let error = resolve(&registry, &with_video("x/x264"), "film.mp4").unwrap_err();
        assert!(
            matches!(&error, ExportError::ComponentUnavailable { component, reason }
                if *component == reference("x/x264")
                    && reason.to_string() == "skipped at discovery (/bin/celesta-codec-ffmpeg-x264): \
                                  --manifest failed (exit status: 1): no libx264"),
            "{error:?}"
        );
        let error = resolve(&registry, &with_video("b/nothing"), "film.mp4").unwrap_err();
        assert!(
            matches!(&error, ExportError::ComponentUnavailable { reason, .. }
                if *reason == UnavailableReason::NotInstalled),
            "{error:?}"
        );
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

        let choice = OutputChoice {
            audio: AudioChoice::Encode(EncoderChoice {
                component: None,
                options: BTreeMap::from([("bitrate".to_owned(), text("96000"))]),
            }),
            ..OutputChoice::default()
        };
        let error = resolve(&registry, &choice, "film.ivf").unwrap_err();
        assert!(
            matches!(&error, ExportError::InvalidOption { component, key, .. }
                if *component == reference("b/ivf") && key == "bitrate"),
            "{error:?}"
        );
        assert_eq!(
            error.to_string(),
            "b/ivf: bitrate needs an audio encoder, and b/ivf takes no audio"
        );

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
                &PresetLookup::default(),
                &mut describer(&bundled_only, &[("b/x264", reason)]),
            )
            .unwrap_err();
            assert!(
                matches!(&error, ExportError::ComponentUnavailable { component, reason: why }
                    if *component == reference("b/x264") && why.to_string() == *reason),
                "{error:?}"
            );
        }

        // A default skips an unavailable candidate for the next one.
        let registry = all();
        let resolved = resolve_output(
            &OutputChoice::default(),
            Path::new("film.mp4"),
            &registry,
            &PresetLookup::default(),
            &mut describer(&registry, &[("b/x264", reason)]),
        )
        .unwrap();
        assert_eq!(resolved.video.reference, reference("u/h264"));

        // The muxer is checked too.
        let error = resolve_output(
            &OutputChoice::default(),
            Path::new("film.mp4"),
            &registry,
            &PresetLookup::default(),
            &mut describer(&registry, &[("b/mp4", "broken")]),
        )
        .unwrap_err();
        assert!(
            matches!(&error, ExportError::ComponentUnavailable { component, reason }
                if *component == reference("b/mp4") && reason.to_string() == "broken"),
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
            &PresetLookup::default(),
            &mut |plugin| {
                asked.push(plugin);
                inner(plugin)
            },
        )
        .unwrap();
        assert_eq!(asked, [1]);
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
    fn an_unknown_preset_has_no_hint() {
        let registry = all();
        let mut issues = Vec::new();
        for name in ["ultrafast", "YouTube"] {
            let choice = OutputChoice {
                preset: Some(name.into()),
                ..OutputChoice::default()
            };
            let error = resolve(&registry, &choice, "film.mp4").unwrap_err();
            assert!(
                matches!(&error, ExportError::UnknownPreset { name: unknown, default_in: None } if unknown == name),
                "{error:?}"
            );
            assert_eq!(error.to_string(), format!("unknown preset `{name}`"));
            for style in [HintStyle::Cli, HintStyle::Mcp] {
                assert_eq!(
                    report::classify(&error, style, &mut issues),
                    ("unknown_preset", None),
                    "{name}"
                );
            }
        }
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

    /// A presets file holding `presets` (JSON), and `default` when given.
    fn presets_file(presets: serde_json::Value, default: Option<&str>) -> PresetsFile {
        let mut file: PresetsFile =
            serde_json::from_value(json!({ "version": 1, "presets": presets })).unwrap();
        file.default = default.map(str::to_owned);
        file
    }

    /// The project and user files, written into a new folder, as a lookup.
    fn lookup(
        project: Option<PresetsFile>,
        user: Option<PresetsFile>,
    ) -> (tempfile::TempDir, PresetLookup) {
        let dir = tempfile::tempdir().unwrap();
        let mut lookup = PresetLookup::default();
        for (file, name, slot) in [
            (project, "project.json", &mut lookup.project),
            (user, "user.json", &mut lookup.user),
        ] {
            let path = dir.path().join(name);
            if let Some(file) = file {
                file.save(&path).unwrap();
            }
            *slot = Some(path);
        }
        (dir, lookup)
    }

    /// A user file holding `presets`.
    fn user_presets(presets: serde_json::Value) -> (tempfile::TempDir, PresetLookup) {
        lookup(None, Some(presets_file(presets, None)))
    }

    /// `web`: `b/mp4`, `b/x264` at CRF 20 and `slow`, no audio.
    fn web() -> serde_json::Value {
        json!({ "name": "web", "muxer": "b/mp4",
            "video": { "component": "b/x264", "options": { "crf": 20, "preset": "slow" } },
            "audio": "none" })
    }

    fn named_preset(name: &str) -> OutputChoice {
        OutputChoice {
            preset: Some(name.into()),
            ..OutputChoice::default()
        }
    }

    fn num(value: f64) -> SettingValue {
        SettingValue::Number(value)
    }

    fn string(value: &str) -> SettingValue {
        SettingValue::String(value.into())
    }

    #[test]
    fn a_named_preset_is_the_base() {
        let registry = all();
        let (_dir, presets) = user_presets(json!([web()]));
        let resolved = resolve_with(&registry, &named_preset("web"), "film.mov", &presets).unwrap();
        assert_eq!(resolved.muxer.reference, reference("b/mp4"));
        assert_eq!(resolved.video.reference, reference("b/x264"));
        assert_eq!(resolved.video.settings["crf"], num(20.0));
        assert_eq!(resolved.video.settings["preset"], string("slow"));
        assert_eq!(resolved.audio, None);
    }

    #[test]
    fn explicit_choices_layer_over_the_preset() {
        let registry = all();
        let (_dir, presets) = user_presets(json!([web()]));
        let mut choice = with_video_options(&[("crf", text("22"))]);
        choice.preset = Some("web".into());
        let resolved = resolve_with(&registry, &choice, "film.mp4", &presets).unwrap();
        assert_eq!(resolved.video.settings["crf"], num(22.0));
        assert_eq!(resolved.video.settings["preset"], string("slow"));
        assert!(resolved.warnings.is_empty(), "{:?}", resolved.warnings);

        // Another encoder drops the preset's options without a warning.
        let mut choice = with_video("u/h264");
        choice.preset = Some("web".into());
        let resolved = resolve_with(&registry, &choice, "film.mp4", &presets).unwrap();
        assert_eq!(resolved.video.reference, reference("u/h264"));
        assert!(resolved.video.settings.is_empty());
        assert!(resolved.warnings.is_empty(), "{:?}", resolved.warnings);

        // An explicit audio encoder replaces the preset's "none".
        let mut choice = with_audio("b/aac");
        choice.preset = Some("web".into());
        let resolved = resolve_with(&registry, &choice, "film.mp4", &presets).unwrap();
        assert_eq!(
            resolved.audio.map(|audio| audio.reference),
            Some(reference("b/aac"))
        );
    }

    #[test]
    fn a_presets_options_without_a_component_go_to_the_default_encoder() {
        let registry = all();
        let (_dir, presets) =
            user_presets(json!([{ "name": "crf", "video": { "options": { "crf": 20 } } }]));
        let resolved = resolve_with(&registry, &named_preset("crf"), "film.mp4", &presets).unwrap();
        assert_eq!(resolved.video.reference, reference("b/x264"));
        assert_eq!(resolved.video.settings["crf"], num(20.0));

        // Not to an encoder the choice names.
        let mut choice = with_video("u/h264");
        choice.preset = Some("crf".into());
        let resolved = resolve_with(&registry, &choice, "film.mp4", &presets).unwrap();
        assert!(resolved.video.settings.is_empty());
        assert!(resolved.warnings.is_empty(), "{:?}", resolved.warnings);
    }

    #[test]
    fn stale_preset_values_warn_and_reset() {
        let registry = all();
        let (_dir, presets) = user_presets(json!([{ "name": "old",
            "video": { "component": "b/x264", "options": { "crf": 99, "speed": 1 } } }]));
        let resolved = resolve_with(&registry, &named_preset("old"), "film.mp4", &presets).unwrap();
        assert_eq!(resolved.video.settings["crf"], num(18.0));
        assert!(
            matches!(&resolved.warnings[0], ResolveWarning::StaleValue { label, .. } if label == "CRF"),
            "{:?}",
            resolved.warnings
        );
        assert_eq!(
            texts(&resolved.warnings),
            [
                "preset `old`: b/x264 crf must be at most 51; using its default",
                "preset `old`: b/x264 has no setting `speed`; it is left out",
            ]
        );
    }

    #[test]
    fn explicit_values_stay_strict_over_a_preset() {
        let registry = all();
        let (_dir, presets) = user_presets(json!([web()]));
        let mut choice = with_video_options(&[("crf", text("99"))]);
        choice.preset = Some("web".into());
        let error = resolve_with(&registry, &choice, "film.mp4", &presets).unwrap_err();
        assert!(
            matches!(&error, ExportError::InvalidOption { component, key, .. }
                if *component == reference("b/x264") && key == "crf"),
            "{error:?}"
        );
    }

    #[test]
    fn a_gone_preset_component_falls_back() {
        let registry = all();
        let stale = |video: &str| {
            json!({ "name": "stale",
                "video": { "component": video, "options": { "crf": 20 } } })
        };
        let (_dir, presets) = user_presets(json!([stale("b/gone")]));
        let resolved =
            resolve_with(&registry, &named_preset("stale"), "film.mp4", &presets).unwrap();
        assert_eq!(resolved.video.reference, reference("b/x264"));
        // The preset's options went with its encoder.
        assert_eq!(resolved.video.settings["crf"], num(18.0));
        assert_eq!(
            texts(&resolved.warnings),
            ["preset `stale`: b/gone is unavailable (not installed); using the default instead"]
        );

        let (_dir, presets) = user_presets(json!([stale("nonsense")]));
        let resolved =
            resolve_with(&registry, &named_preset("stale"), "film.mp4", &presets).unwrap();
        assert_eq!(resolved.video.reference, reference("b/x264"));
        assert_eq!(
            texts(&resolved.warnings),
            [
                "preset `stale`: nonsense is unavailable (not a component reference); using the default instead"
            ]
        );

        let (_dir, presets) = user_presets(json!([stale("b/vp9")]));
        let resolved =
            resolve_with(&registry, &named_preset("stale"), "film.mp4", &presets).unwrap();
        assert_eq!(resolved.video.reference, reference("b/x264"));
        assert_eq!(
            texts(&resolved.warnings),
            [
                "preset `stale`: b/vp9 is unavailable (it writes vp09, and b/mp4 takes avc1); using the default instead"
            ]
        );

        let (_dir, presets) = user_presets(json!([stale("b/x264")]));
        let reason = "this FFmpeg build has no libx264";
        let resolved = resolve_output(
            &named_preset("stale"),
            Path::new("film.mp4"),
            &registry,
            &presets,
            &mut describer(&registry, &[("b/x264", reason)]),
        )
        .unwrap();
        assert_eq!(resolved.video.reference, reference("u/h264"));
        assert!(resolved.video.settings.is_empty());
        assert_eq!(
            texts(&resolved.warnings),
            [format!(
                "preset `stale`: b/x264 is unavailable ({reason}); using the default instead"
            )]
        );

        // A muxer too.
        let (_dir, presets) = user_presets(json!([{ "name": "stale", "muxer": "b/x264" }]));
        let resolved =
            resolve_with(&registry, &named_preset("stale"), "film.mp4", &presets).unwrap();
        assert_eq!(resolved.muxer.reference, reference("b/mp4"));
        assert!(
            matches!(
                &resolved.warnings[..],
                [ResolveWarning::Unavailable {
                    kind: ComponentKind::Muxer,
                    ..
                }]
            ),
            "{:?}",
            resolved.warnings
        );
        assert_eq!(
            texts(&resolved.warnings),
            ["preset `stale`: b/x264 is unavailable (not a muxer); using the default instead"]
        );

        // Audio against a muxer that takes none is left out, not defaulted.
        let (_dir, presets) = user_presets(json!([{ "name": "stale", "muxer": "b/ivf",
            "audio": { "component": "b/aac" } }]));
        let resolved =
            resolve_with(&registry, &named_preset("stale"), "film.ivf", &presets).unwrap();
        assert_eq!(resolved.audio, None);
        assert_eq!(
            texts(&resolved.warnings),
            ["preset `stale`: b/aac is unavailable (b/ivf takes no audio); the audio is left out"]
        );
    }

    #[test]
    fn a_preset_muxer_against_another_extension_warns() {
        let registry = all();
        let (_dir, presets) = user_presets(json!([web()]));
        let resolved = resolve_with(&registry, &named_preset("web"), "film.mov", &presets).unwrap();
        assert_eq!(
            texts(&resolved.warnings),
            ["b/mp4 does not write .mov files; the file is written as MP4 anyway"]
        );
        let resolved = resolve_with(&registry, &named_preset("web"), "film.mp4", &presets).unwrap();
        assert!(resolved.warnings.is_empty(), "{:?}", resolved.warnings);

        // A named muxer warns the same; one chosen by the extension never does.
        let choice = OutputChoice {
            muxer: Some(reference("b/mp4")),
            ..OutputChoice::default()
        };
        let resolved = resolve(&registry, &choice, "film.MOV").unwrap();
        assert_eq!(
            texts(&resolved.warnings),
            ["b/mp4 does not write .MOV files; the file is written as MP4 anyway"]
        );
        let resolved = resolve(&registry, &OutputChoice::default(), "film.mp4").unwrap();
        assert!(resolved.warnings.is_empty(), "{:?}", resolved.warnings);
    }

    #[test]
    fn the_project_default_applies_without_components() {
        let registry = all();
        let (_dir, presets) = lookup(Some(presets_file(json!([web()]), Some("web"))), None);
        let resolved =
            resolve_with(&registry, &OutputChoice::default(), "film.mp4", &presets).unwrap();
        assert_eq!(resolved.video.settings["crf"], num(20.0));
        assert_eq!(resolved.video.settings["preset"], string("slow"));
        assert_eq!(resolved.audio, None);
        assert!(resolved.warnings.is_empty(), "{:?}", resolved.warnings);

        let resolved = resolve_with(
            &registry,
            &with_video_options(&[("crf", text("22"))]),
            "film.mp4",
            &presets,
        )
        .unwrap();
        assert_eq!(resolved.video.settings["crf"], num(22.0));
        assert_eq!(resolved.video.settings["preset"], string("slow"));
        assert_eq!(resolved.audio, None);

        // `--audio-encoder none` names a component: no default preset.
        let choice = OutputChoice {
            audio: AudioChoice::None,
            ..OutputChoice::default()
        };
        let resolved = resolve_with(&registry, &choice, "film.mp4", &presets).unwrap();
        assert_eq!(resolved.video.settings["crf"], num(18.0));
        assert_eq!(resolved.video.settings["preset"], string("medium"));
        for choice in [
            with_video("b/x264"),
            with_audio("b/aac"),
            OutputChoice {
                muxer: Some(reference("b/mp4")),
                ..OutputChoice::default()
            },
        ] {
            let resolved = resolve_with(&registry, &choice, "film.mp4", &presets).unwrap();
            assert_eq!(resolved.video.settings["crf"], num(18.0), "{choice:?}");
        }
    }

    #[test]
    fn a_project_default_naming_a_project_component_warns() {
        let registry = all();
        let local = json!([{ "name": "local", "muxer": "p/mp4",
            "video": { "component": "p/h264" }, "audio": { "component": "p/aac" } }]);
        let (_dir, presets) = lookup(Some(presets_file(local, Some("local"))), None);
        let components = |resolved: &ResolvedOutput| {
            (
                resolved.muxer.reference.clone(),
                resolved.video.reference.clone(),
                resolved.audio.as_ref().map(|audio| audio.reference.clone()),
            )
        };
        let expected = (
            reference("p/mp4"),
            reference("p/h264"),
            Some(reference("p/aac")),
        );
        let resolved =
            resolve_with(&registry, &OutputChoice::default(), "film.mp4", &presets).unwrap();
        assert_eq!(components(&resolved), expected);
        assert_eq!(
            texts(&resolved.warnings),
            [
                "the project's default preset `local` uses p/mp4, from the project's plugin p",
                "the project's default preset `local` uses p/h264, from the project's plugin p",
                "the project's default preset `local` uses p/aac, from the project's plugin p",
            ]
        );

        let resolved =
            resolve_with(&registry, &named_preset("local"), "film.mp4", &presets).unwrap();
        assert_eq!(components(&resolved), expected);
        assert!(resolved.warnings.is_empty(), "{:?}", resolved.warnings);
    }

    #[test]
    fn a_missing_default_is_unknown_preset() {
        let registry = all();
        let (dir, presets) = lookup(Some(presets_file(json!([]), Some("gone"))), None);
        let error =
            resolve_with(&registry, &OutputChoice::default(), "film.mp4", &presets).unwrap_err();
        let project = dir.path().join("project.json");
        assert!(
            matches!(&error, ExportError::UnknownPreset { name, default_in: Some(path) }
                if name == "gone" && *path == project),
            "{error:?}"
        );
    }

    #[test]
    fn an_unknown_name_is_still_unknown_preset() {
        let registry = all();
        let (_dir, with_files) = user_presets(json!([web()]));
        for presets in [PresetLookup::default(), with_files] {
            let error = resolve_with(&registry, &named_preset("ultrafast"), "film.mp4", &presets)
                .unwrap_err();
            assert!(
                matches!(&error, ExportError::UnknownPreset { name, default_in: None }
                    if name == "ultrafast"),
                "{error:?}"
            );
        }
    }

    #[test]
    fn the_shadow_warning_reaches_the_resolved_output() {
        let registry = all();
        let x = || json!([{ "name": "x", "muxer": "b/mp4" }]);
        let (_dir, presets) = lookup(Some(presets_file(x(), None)), Some(presets_file(x(), None)));
        let resolved = resolve_with(&registry, &named_preset("x"), "film.mov", &presets).unwrap();
        assert_eq!(
            texts(&resolved.warnings),
            [
                "the project's preset `x` hides the user preset of the same name",
                "b/mp4 does not write .mov files; the file is written as MP4 anyway",
            ]
        );
    }

    #[test]
    fn a_broken_user_file_does_not_fail_a_project_preset() {
        let registry = all();
        let (dir, presets) = lookup(Some(presets_file(json!([web()]), Some("web"))), None);
        let user = dir.path().join("user.json");
        std::fs::write(&user, "{").unwrap();
        let expected = format!(
            "your presets file {} could not be read, so it was not checked for a preset of the same name: is not valid JSON: EOF while parsing an object at line 1 column 1",
            user.display()
        );
        for choice in [named_preset("web"), OutputChoice::default()] {
            let resolved = resolve_with(&registry, &choice, "film.mp4", &presets).unwrap();
            assert_eq!(resolved.video.settings["crf"], num(20.0), "{choice:?}");
            assert_eq!(texts(&resolved.warnings), [expected.as_str()], "{choice:?}");
        }

        // A name only the user file could hold still fails on it.
        let error =
            resolve_with(&registry, &named_preset("other"), "film.mp4", &presets).unwrap_err();
        assert!(
            matches!(&error, ExportError::InvalidPresets { path, .. } if *path == user),
            "{error:?}"
        );
    }

    #[test]
    fn naming_the_presets_own_encoder_keeps_its_options() {
        let registry = all();
        let (_dir, presets) = user_presets(json!([web()]));
        let mut choice = with_video("b/x264");
        choice.preset = Some("web".into());
        let resolved = resolve_with(&registry, &choice, "film.mp4", &presets).unwrap();
        assert_eq!(resolved.video.reference, reference("b/x264"));
        assert_eq!(resolved.video.settings["crf"], num(20.0));
        assert_eq!(resolved.video.settings["preset"], string("slow"));
        assert!(resolved.warnings.is_empty(), "{:?}", resolved.warnings);
    }

    #[test]
    fn a_project_default_is_found_in_the_user_file() {
        let registry = all();
        let (_dir, presets) = lookup(
            Some(presets_file(json!([]), Some("web"))),
            Some(presets_file(json!([web()]), None)),
        );
        let resolved =
            resolve_with(&registry, &OutputChoice::default(), "film.mp4", &presets).unwrap();
        assert_eq!(resolved.video.settings["crf"], num(20.0));
        assert_eq!(resolved.video.settings["preset"], string("slow"));
        assert_eq!(resolved.audio, None);
        assert!(resolved.warnings.is_empty(), "{:?}", resolved.warnings);
    }

    #[test]
    fn audio_options_without_audio_are_ignored() {
        let registry = all();
        let (_dir, presets) = user_presets(json!([web()]));
        let choice = OutputChoice {
            preset: Some("web".into()),
            audio: AudioChoice::Encode(EncoderChoice {
                component: None,
                options: [("bitrate".to_owned(), text("1"))].into(),
            }),
            ..OutputChoice::default()
        };
        let resolved = resolve_with(&registry, &choice, "film.mp4", &presets).unwrap();
        assert_eq!(resolved.audio, None);
        assert_eq!(
            texts(&resolved.warnings),
            ["audio options are ignored: preset `web` leaves the audio out"]
        );
    }
}
