//! A still export's encoder and muxer: the built-in image components, or a
//! plugin's named ones, writing one image file or a sequence of them (the
//! media components spec's Stills and sequences).
//!
//! One file is staged beside the output and published whole once the
//! muxer finishes. A sequence is staged in a hidden folder beside the
//! output, whose pattern only the muxer reads; each frame's file is
//! published to its final name as soon as the muxer has written it, so a
//! failed or cancelled export keeps the frames written before it.

use crate::control::ExportCancellation;
use crate::encode::{IMAGE_STAGE, MUXER_STAGE, absolute, check_opened};
use crate::error::ExportError;
use crate::options::{AudioChoice, OutputChoice, SettingInput};
use crate::output;
use crate::render::ensure_not_cancelled;
use crate::resolve::{OutputPlan, ResolvedOutput, resolve_preset_output};
use celesta_composition::Rational;
use celesta_media::{
    Color as MediaColor, ComponentDescription, ComponentEntry, ComponentKind, ComponentRef,
    Encoded, MediaError, MediaProvider, MediaRegistry, MuxerRequest, PixelFormat, ProviderUse,
    SessionId, SettingValue, Track, TrackConfig, TrackPacket, VideoEncoderOpened,
    VideoEncoderRequest,
};
use celesta_media_image::{IMAGE, IMAGE_SEQUENCE, codec_for_extension, sequence_path};
use celesta_plugin_host::{MediaOrigin, PluginKillSwitch, RESERVED_PLUGIN_ID, codec_family};
use celesta_remote::{StagedFile, publish_path};
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The files a still export writes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StillFiles {
    /// One image: one frame, or a contact sheet.
    One(PathBuf),
    /// One image per frame: (composition frame, final name), in selection
    /// order.
    Sequence(Vec<(u64, PathBuf)>),
}

/// The template the staged sequence's pattern holds in its file name.
const SEQUENCE_NAME: &str = "frame-%06d";

/// Where the muxer writes before Celesta publishes.
enum Staging {
    /// One file beside the output, published at `path` by `finish`.
    One { file: StagedFile, path: PathBuf },
    /// A hidden folder beside the output holding the muxer's `pattern`;
    /// `names` are the final names by composition frame, and `order` the
    /// frames in selection order.
    Sequence {
        /// Kept until the export ends, then removed with what is left in it.
        _folder: tempfile::TempDir,
        pattern: PathBuf,
        names: HashMap<u64, PathBuf>,
        order: Vec<u64>,
    },
}

impl Staging {
    /// The path the muxer is given: the staged file, or the pattern.
    fn muxer_output(&self) -> &Path {
        match self {
            Self::One { file, .. } => file.path(),
            Self::Sequence { pattern, .. } => pattern,
        }
    }
}

/// A still export's open encoder and, once its first frame is encoded, its
/// muxer, through the providers started for them: one per plugin, the
/// built-in one in this process. Dropping it unfinished leaves the staged
/// files to go with it; [`Self::abort`] also ends the sessions.
pub(crate) struct StillOutput {
    registry: Arc<MediaRegistry>,
    cancellation: ExportCancellation,
    providers: HashMap<usize, Box<dyn MediaProvider>>,
    resolved: ResolvedOutput,
    opened: VideoEncoderOpened,
    encoder: Option<SessionId>,
    muxer: Option<SessionId>,
    config: Option<TrackConfig>,
    /// The last composition frame written to one file.
    written: Option<u64>,
    /// The sequence's frames published so far.
    published: HashSet<u64>,
    size: (u32, u32),
    frame_rate: Rational,
    overwrite: bool,
    staging: Staging,
}

fn invalid(message: impl Into<String>) -> ExportError {
    ExportError::InvalidSelection(message.into())
}

/// Today's message for an extension no built-in muxer writes, built from
/// the extensions they list.
fn extension_message() -> String {
    let extensions: Vec<String> = celesta_media_image::EXTENSIONS
        .iter()
        .map(|extension| format!(".{extension}"))
        .collect();
    let (last, rest) = extensions.split_last().expect("the built-ins write files");
    format!(
        "still output must use the {} or {last} extension",
        rest.join(", ")
    )
}

fn builtin(component: &str) -> ComponentRef {
    ComponentRef {
        plugin: RESERVED_PLUGIN_ID.to_owned(),
        component: component.to_owned(),
    }
}

/// The components a still export writes `files` with, for `output`: the
/// choice's own, with no preset and no audio, and the built-in image muxer
/// and the built-in encoder of `output`'s extension for those it leaves
/// out (deviation 51 of the media components spec). `quality` becomes the
/// encoder's `quality` option.
pub(crate) fn still_choice(
    choice: &OutputChoice,
    quality: Option<u8>,
    files: &StillFiles,
    output: &Path,
    registry: &MediaRegistry,
) -> Result<OutputChoice, ExportError> {
    if choice.preset.is_some() {
        return Err(invalid("presets apply to video output, not still frames"));
    }
    if let AudioChoice::Encode(audio) = &choice.audio
        && (audio.component.is_some() || !audio.options.is_empty())
    {
        return Err(invalid("audio applies to video output, not still frames"));
    }
    let mut video = choice.video.clone();
    if let Some(quality) = quality {
        if video.options.contains_key("quality") {
            return Err(invalid("quality is given twice"));
        }
        video.options.insert(
            "quality".to_owned(),
            SettingInput::Value(SettingValue::Number(f64::from(quality))),
        );
    }
    let several = matches!(files, StillFiles::Sequence(_));
    let muxer = match &choice.muxer {
        Some(reference) => {
            let sequence = registry
                .component(reference, ComponentKind::Muxer)
                .and_then(|info| match info.entry {
                    ComponentEntry::Muxer(entry) => Some(entry.sequence),
                    _ => None,
                });
            match sequence {
                Some(false) if several => {
                    return Err(invalid(format!(
                        "{reference} writes one image; several frames need a sequence muxer \
                         such as builtin/{IMAGE_SEQUENCE}"
                    )));
                }
                Some(true) if !several => {
                    return Err(invalid(format!(
                        "{reference} writes image sequences; one image needs a muxer such as \
                         builtin/{IMAGE}"
                    )));
                }
                // One that is not installed is the resolution's to refuse.
                _ => reference.clone(),
            }
        }
        None => builtin(if several { IMAGE_SEQUENCE } else { IMAGE }),
    };
    if muxer.plugin == RESERVED_PLUGIN_ID {
        let extension = output.extension().and_then(OsStr::to_str);
        let Some(codec) = extension.and_then(codec_for_extension) else {
            return Err(invalid(extension_message()));
        };
        match &video.component {
            None => video.component = Some(builtin(codec)),
            Some(reference) => {
                if let Some(info) = registry.component(reference, ComponentKind::VideoEncoder)
                    && let ComponentEntry::VideoEncoder(entry) = &info.entry
                    && !entry
                        .codecs
                        .iter()
                        .any(|written| codec_family(written) == codec)
                {
                    return Err(ExportError::IncompatibleComponents {
                        component: reference.clone(),
                        muxer: muxer.clone(),
                        reason: format!(
                            "{muxer} writes .{} files as {codec}, and {reference} writes {}",
                            extension.unwrap_or_default().to_ascii_lowercase(),
                            entry.codecs.join(", ")
                        ),
                    });
                }
            }
        }
    }
    Ok(OutputChoice {
        preset: None,
        muxer: Some(muxer),
        video,
        audio: AudioChoice::None,
    })
}

/// The provider of plugin `plugin`, started for the export when it is first
/// needed.
fn provider<'a>(
    providers: &'a mut HashMap<usize, Box<dyn MediaProvider>>,
    registry: &MediaRegistry,
    kill: &PluginKillSwitch,
    plugin: usize,
) -> Result<&'a mut Box<dyn MediaProvider>, MediaError> {
    Ok(match providers.entry(plugin) {
        Entry::Occupied(entry) => entry.into_mut(),
        Entry::Vacant(entry) => entry.insert((registry.plugin(plugin).factory)(
            kill,
            ProviderUse::Export { ring: None },
        )?),
    })
}

/// A plugin's provider that starts only when it is asked to describe, so a
/// cached description starts nothing.
struct Starting<'a> {
    providers: &'a mut HashMap<usize, Box<dyn MediaProvider>>,
    registry: &'a MediaRegistry,
    kill: &'a PluginKillSwitch,
    plugin: usize,
}

impl MediaProvider for Starting<'_> {
    fn describe(&mut self) -> Result<Vec<ComponentDescription>, MediaError> {
        provider(self.providers, self.registry, self.kill, self.plugin)?.describe()
    }
}

fn missing(registry: &MediaRegistry, plugin: usize, kind: &str) -> MediaError {
    MediaError::Codec(format!("{} serves no {kind}", registry.plugin(plugin).id))
}

impl StillOutput {
    /// Names the components ([`still_choice`]), resolves them through
    /// `registry`, starts their providers ([`ProviderUse::Export`] with no
    /// ring; the built-ins in this process), opens the encoder for `size`
    /// frames at `frame_rate`, offering `rgba16` and `rgba8`, or only
    /// `rgba8` for a contact `sheet`, and only once it accepted stages
    /// `files` beside `output`, so a refusal leaves no folder behind. All
    /// before anything renders. Returns the resolution's warnings too.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn open(
        registry: Arc<MediaRegistry>,
        choice: &OutputChoice,
        quality: Option<u8>,
        files: StillFiles,
        output: &Path,
        sheet: bool,
        size: (u32, u32),
        frame_rate: Rational,
        overwrite: bool,
        cancellation: &ExportCancellation,
    ) -> Result<(Self, Vec<String>), ExportError> {
        let choice = still_choice(choice, quality, &files, output, &registry)?;
        let kill = cancellation.plugins();
        let mut providers = HashMap::new();
        let mut resolved = resolve_preset_output(&choice, None, output, &registry, &mut |plugin| {
            if registry.plugin(plugin).origin == MediaOrigin::BuiltIn {
                registry.describe(plugin, &kill)
            } else {
                registry.describe_with(
                    plugin,
                    &mut Starting {
                        providers: &mut providers,
                        registry: &registry,
                        kill: &kill,
                        plugin,
                    },
                )
            }
        })
        .map_err(|error| cancellation.settle(error))?;
        let used = [resolved.muxer.plugin, resolved.video.plugin];
        providers.retain(|plugin, _| used.contains(plugin));
        let warnings = std::mem::take(&mut resolved.warnings)
            .iter()
            .map(ToString::to_string)
            .collect();
        let offered: &[PixelFormat] = if sheet {
            &[PixelFormat::Rgba8]
        } else {
            &[PixelFormat::Rgba16, PixelFormat::Rgba8]
        };
        let (width, height) = size;
        let color = MediaColor::bt709_limited();
        let video = &resolved.video;
        let request = VideoEncoderRequest {
            component: &video.reference.component,
            settings: &video.settings,
            width,
            height,
            frame_rate,
            timescale: frame_rate.numerator,
            offered,
            color: &color,
        };
        let opened =
            provider(&mut providers, &registry, &kill, video.plugin).and_then(|provider| {
                provider
                    .video_encoder()
                    .ok_or_else(|| missing(&registry, video.plugin, "video encoder"))?
                    .open_video(&request)
            });
        let opened = opened.map_err(|source| {
            cancellation.settle(ExportError::Codec {
                stage: IMAGE_STAGE,
                source,
            })
        })?;
        let staged = check_opened(offered, &opened)
            .map_err(|source| {
                cancellation.settle(ExportError::Codec {
                    stage: IMAGE_STAGE,
                    source,
                })
            })
            .and_then(|()| stage(files, output));
        let staging = match staged {
            Ok(staging) => staging,
            Err(error) => {
                if let Some(encoder) = providers
                    .get_mut(&video.plugin)
                    .and_then(|provider| provider.video_encoder())
                {
                    encoder.abort_video(opened.session);
                }
                return Err(error);
            }
        };
        let still = Self {
            registry,
            cancellation: cancellation.clone(),
            providers,
            resolved,
            opened,
            encoder: Some(opened.session),
            muxer: None,
            config: None,
            written: None,
            published: HashSet::new(),
            size,
            frame_rate,
            overwrite,
            staging,
        };
        Ok((still, warnings))
    }

    /// What the encoder answered: its input format, and whether it keeps
    /// alpha.
    pub(crate) fn opened(&self) -> &VideoEncoderOpened {
        &self.opened
    }

    /// The components it writes with, as the progress display names them:
    /// the muxer and the encoder with its changed settings, and no audio.
    pub(crate) fn plan(&self) -> OutputPlan {
        self.resolved.plan()
    }

    fn error(&self, stage: &'static str, source: MediaError) -> ExportError {
        self.cancellation
            .settle(ExportError::Codec { stage, source })
    }

    fn provider(&mut self, plugin: usize) -> Result<&mut Box<dyn MediaProvider>, MediaError> {
        let kill = self.cancellation.plugins();
        provider(&mut self.providers, &self.registry, &kill, plugin)
    }

    /// Encodes composition frame `frame` (shown at `frame` × the frame
    /// rate's denominator) from `pixels` in the encoder's input format and
    /// writes the packets the encoder gave for it. A sequence publishes the
    /// file of each frame those packets carry, which an encoder that holds
    /// frames back may make an earlier frame's, or none, adding each to
    /// `published` (by composition frame, in packet order) as it goes, so
    /// the files published before a failure are there too. One file stays
    /// staged until [`Self::finish`].
    pub(crate) fn write(
        &mut self,
        frame: u64,
        pixels: Vec<u8>,
        published: &mut Vec<(u64, PathBuf)>,
    ) -> Result<(), ExportError> {
        let pts = i64::try_from(frame)
            .ok()
            .and_then(|frame| frame.checked_mul(i64::from(self.frame_rate.denominator)))
            .ok_or(ExportError::TimelineTooLong)?;
        let session = self.encoder.ok_or_else(|| {
            self.error(
                IMAGE_STAGE,
                MediaError::Codec("the encoder is finished".to_owned()),
            )
        })?;
        let plugin = self.resolved.video.plugin;
        let registry = self.registry.clone();
        let encoded = self
            .provider(plugin)
            .and_then(|provider| {
                provider
                    .video_encoder()
                    .ok_or_else(|| missing(&registry, plugin, "video encoder"))?
                    .encode_video(session, pts, pixels)
            })
            .map_err(|source| self.error(IMAGE_STAGE, source))?;
        let frames = self.mux(encoded)?;
        self.publish(frames, published)
    }

    /// Writes `encoded`'s packets, opening the muxer with the first config:
    /// the composition frames they carry, in packet order.
    fn mux(&mut self, encoded: Encoded) -> Result<Vec<u64>, ExportError> {
        if self.config.is_none() {
            self.config = encoded.config;
        }
        if encoded.packets.is_empty() {
            return Ok(Vec::new());
        }
        let denominator = u64::from(self.frame_rate.denominator.max(1));
        let frames: Vec<u64> = encoded
            .packets
            .iter()
            .map(|packet| u64::try_from(packet.pts).unwrap_or(0) / denominator)
            .collect();
        let session = match self.muxer {
            Some(session) => session,
            None => self.open_muxer()?,
        };
        let packets: Vec<TrackPacket> = encoded
            .packets
            .into_iter()
            .map(|packet| TrackPacket { track: 0, packet })
            .collect();
        let plugin = self.resolved.muxer.plugin;
        let registry = self.registry.clone();
        self.provider(plugin)
            .and_then(|provider| {
                provider
                    .muxer()
                    .ok_or_else(|| missing(&registry, plugin, "muxer"))?
                    .write_packets(session, &packets)
            })
            .map_err(|source| self.error(MUXER_STAGE, source))?;
        Ok(frames)
    }

    /// Publishes the sequence's files of `frames`, just written, in order,
    /// adding each to `published` once it is, so a failure partway keeps
    /// the ones before it. One file only notes the last frame written.
    fn publish(
        &mut self,
        frames: Vec<u64>,
        published: &mut Vec<(u64, PathBuf)>,
    ) -> Result<(), ExportError> {
        if matches!(self.staging, Staging::One { .. }) {
            if let Some(&last) = frames.last() {
                self.written = Some(last);
            }
            return Ok(());
        }
        for frame in frames {
            published.push((frame, self.publish_frame(frame)?));
            self.published.insert(frame);
        }
        Ok(())
    }

    fn open_muxer(&mut self) -> Result<SessionId, ExportError> {
        let Some(config) = self.config.clone() else {
            return Err(self.error(
                IMAGE_STAGE,
                MediaError::Codec("the encoder produced no configuration".to_owned()),
            ));
        };
        let (width, height) = self.size;
        let tracks = [Track::Video {
            config,
            timescale: self.frame_rate.numerator,
            width,
            height,
            frame_rate: self.frame_rate,
            color: MediaColor::bt709_limited(),
            alpha: self.opened.alpha,
        }];
        let muxer = &self.resolved.muxer;
        let (plugin, component, settings) = (
            muxer.plugin,
            muxer.reference.component.clone(),
            muxer.settings.clone(),
        );
        let output = self.staging.muxer_output().to_owned();
        let request = MuxerRequest {
            component: &component,
            settings: &settings,
            output: &output,
            tracks: &tracks,
            leading_audio: 0,
        };
        let registry = self.registry.clone();
        let session = self
            .provider(plugin)
            .and_then(|provider| {
                provider
                    .muxer()
                    .ok_or_else(|| missing(&registry, plugin, "muxer"))?
                    .open_muxer(&request)
            })
            .map_err(|source| self.error(MUXER_STAGE, source))?;
        self.muxer = Some(session);
        Ok(session)
    }

    /// Publishes a sequence's file for `frame` at its final name, once the
    /// muxer has written it.
    fn publish_frame(&self, frame: u64) -> Result<PathBuf, ExportError> {
        let Staging::Sequence { pattern, names, .. } = &self.staging else {
            unreachable!("only a sequence publishes as it goes");
        };
        let muxer = &self.resolved.muxer.reference;
        let not_written = || {
            self.error(
                MUXER_STAGE,
                MediaError::Codec(format!("{muxer} wrote no file for frame {frame}")),
            )
        };
        let name = names.get(&frame).ok_or_else(|| {
            self.error(
                MUXER_STAGE,
                MediaError::Codec(format!("frame {frame} is not one this export writes")),
            )
        })?;
        let staged =
            sequence_path(pattern, frame).map_err(|source| self.error(MUXER_STAGE, source))?;
        if !staged.is_file() {
            return Err(not_written());
        }
        publish_path(&staged, name, self.overwrite)
            .map_err(|source| publish_failed(source, name))?;
        Ok(name.clone())
    }

    /// Flushes the encoder, finishes the muxer, and publishes what is left,
    /// adding it to `published`: one file, under the frame written to it,
    /// or the sequence's files of the frames the flush gave, in packet
    /// order, each as it is published. A failure aborts what is still open.
    /// The muxer must have written one file's staged file, or a file for
    /// every frame of a sequence. One file is not published once the export
    /// is cancelled.
    pub(crate) fn finish(mut self, published: &mut Vec<(u64, PathBuf)>) -> Result<(), ExportError> {
        if let Err(error) = self.try_finish(published) {
            self.abort();
            return Err(error);
        }
        if let Staging::One { file, path } = self.staging {
            // The staged file goes with `file` when this returns early.
            ensure_not_cancelled(&self.cancellation)?;
            file.publish(&path, self.overwrite)
                .map_err(|source| publish_failed(source, &path))?;
            published.push((self.written.unwrap_or(0), path));
        }
        Ok(())
    }

    fn try_finish(&mut self, published: &mut Vec<(u64, PathBuf)>) -> Result<(), ExportError> {
        if let Some(session) = self.encoder.take() {
            let plugin = self.resolved.video.plugin;
            let registry = self.registry.clone();
            let encoded = self
                .provider(plugin)
                .and_then(|provider| {
                    provider
                        .video_encoder()
                        .ok_or_else(|| missing(&registry, plugin, "video encoder"))?
                        .finish_video(session)
                })
                .map_err(|source| self.error(IMAGE_STAGE, source))?;
            let frames = self.mux(encoded)?;
            self.publish(frames, published)?;
        }
        let Some(session) = self.muxer.take() else {
            return Err(self.error(
                MUXER_STAGE,
                MediaError::Codec("no frame was written".to_owned()),
            ));
        };
        let plugin = self.resolved.muxer.plugin;
        let registry = self.registry.clone();
        let files = self
            .provider(plugin)
            .and_then(|provider| {
                provider
                    .muxer()
                    .ok_or_else(|| missing(&registry, plugin, "muxer"))?
                    .finish_muxer(session)
            })
            .map_err(|source| self.error(MUXER_STAGE, source))?;
        let muxer = &self.resolved.muxer.reference;
        let unwritten = match &self.staging {
            Staging::One { file, .. } => {
                // A plugin is handed the path made absolute, which may differ
                // from the staged one in form (`..` on Windows), as in
                // `published_files`.
                let staged = absolute(file.path());
                (!files.iter().any(|written| absolute(written) == staged))
                    .then(|| format!("{muxer} wrote no image file"))
            }
            Staging::Sequence { order, .. } => order
                .iter()
                .find(|frame| !self.published.contains(frame))
                .map(|frame| format!("{muxer} wrote no file for frame {frame}")),
        };
        match unwritten {
            Some(message) => Err(self.error(MUXER_STAGE, MediaError::Codec(message))),
            None => Ok(()),
        }
    }

    /// Ends the sessions still open; staged files go, published ones stay.
    /// Nothing is started to abort.
    pub(crate) fn abort(mut self) {
        if let Some(session) = self.encoder.take()
            && let Some(encoder) = self
                .providers
                .get_mut(&self.resolved.video.plugin)
                .and_then(|provider| provider.video_encoder())
        {
            encoder.abort_video(session);
        }
        if let Some(session) = self.muxer.take()
            && let Some(muxer) = self
                .providers
                .get_mut(&self.resolved.muxer.plugin)
                .and_then(|provider| provider.muxer())
        {
            muxer.abort_muxer(session);
        }
        // The staged file or folder goes with `self`.
    }
}

/// A failure to publish an image at `name`, naming it.
fn publish_failed(source: io::Error, name: &Path) -> ExportError {
    if source.kind() == io::ErrorKind::AlreadyExists {
        ExportError::OutputExists(name.to_owned())
    } else {
        ExportError::Io {
            operation: "publish image",
            source: io::Error::new(source.kind(), format!("{}: {source}", name.display())),
        }
    }
}

/// Stages `files` beside `output`: one file named like it (the muxer may
/// read its extension), or a hidden folder whose pattern keeps `output`'s
/// extension as given.
fn stage(files: StillFiles, output: &Path) -> Result<Staging, ExportError> {
    let suffix = output.extension().map(|extension| {
        let mut suffix = std::ffi::OsString::from(".");
        suffix.push(extension);
        suffix
    });
    match files {
        StillFiles::One(path) => {
            let suffix = suffix.as_deref().and_then(OsStr::to_str).unwrap_or(".tmp");
            let file = output::stage(&path, suffix, "create temporary image")?;
            Ok(Staging::One { file, path })
        }
        StillFiles::Sequence(named) => {
            let parent = match output.parent() {
                Some(parent) if !parent.as_os_str().is_empty() => parent,
                _ => Path::new("."),
            };
            let failed = |source| ExportError::Io {
                operation: "create the staging folder",
                source,
            };
            std::fs::create_dir_all(parent).map_err(failed)?;
            let folder = tempfile::Builder::new()
                .prefix(".celesta-")
                .tempdir_in(parent)
                .map_err(failed)?;
            let mut name = std::ffi::OsString::from(SEQUENCE_NAME);
            name.extend(suffix);
            let pattern = folder.path().join(name);
            Ok(Staging::Sequence {
                _folder: folder,
                pattern,
                order: named.iter().map(|(frame, _)| *frame).collect(),
                names: named.into_iter().collect(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::with_builtin;
    use crate::options::{AudioChoice, EncoderChoice, SettingInput};
    use crate::report::{HintStyle, classify};
    use crate::stills::StillExport;
    use celesta_media::{Packet, VideoEncoder};
    use std::fs;

    fn registry() -> Arc<MediaRegistry> {
        with_builtin(Vec::new())
    }

    fn reference(text: &str) -> ComponentRef {
        text.parse().unwrap()
    }

    fn one(path: &Path) -> StillFiles {
        StillFiles::One(path.to_owned())
    }

    /// Frames 0 and 1 of `path`, named as a still export names them.
    fn two(path: &Path) -> StillFiles {
        let named = |frame: u64| {
            let mut name = path.file_stem().unwrap().to_os_string();
            name.push(format!("-{frame:06}."));
            name.push(path.extension().unwrap_or_default());
            (frame, path.with_file_name(name))
        };
        StillFiles::Sequence(vec![named(0), named(1)])
    }

    fn with_video_options(options: &[(&str, &str)]) -> OutputChoice {
        OutputChoice {
            video: EncoderChoice {
                component: None,
                options: options
                    .iter()
                    .map(|(key, value)| {
                        ((*key).to_owned(), SettingInput::Text((*value).to_owned()))
                    })
                    .collect(),
            },
            ..OutputChoice::default()
        }
    }

    fn naming(muxer: &str) -> OutputChoice {
        OutputChoice {
            muxer: Some(reference(muxer)),
            ..OutputChoice::default()
        }
    }

    fn code(error: &ExportError) -> &'static str {
        classify(error, HintStyle::Cli, &mut Vec::new()).0
    }

    /// Opens a still output for `files` at 4 fps, `size` pixels.
    fn open(
        output: &Path,
        choice: &OutputChoice,
        quality: Option<u8>,
        files: StillFiles,
        sheet: bool,
        size: (u32, u32),
    ) -> Result<(StillOutput, Vec<String>), ExportError> {
        StillOutput::open(
            registry(),
            choice,
            quality,
            files,
            output,
            sheet,
            size,
            Rational::new(4, 1),
            false,
            &ExportCancellation::default(),
        )
    }

    /// An opaque `width`x`height` RGBA8 frame of `color`.
    fn opaque(width: u32, height: u32, color: [u8; 3]) -> Vec<u8> {
        [color[0], color[1], color[2], 255].repeat((width * height) as usize)
    }

    /// [`StillOutput::write`]: the files it published.
    fn write(
        still: &mut StillOutput,
        frame: u64,
        pixels: Vec<u8>,
    ) -> Result<Vec<(u64, PathBuf)>, ExportError> {
        let mut published = Vec::new();
        still
            .write(frame, pixels, &mut published)
            .map(|()| published)
    }

    /// [`StillOutput::finish`]: the files it published.
    fn finish(still: StillOutput) -> Result<Vec<(u64, PathBuf)>, ExportError> {
        let mut published = Vec::new();
        still.finish(&mut published).map(|()| published)
    }

    fn names(directory: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn names_the_built_ins_by_extension() {
        let registry = registry();
        let choose = |choice: &OutputChoice, files: &StillFiles, output: &str| {
            still_choice(choice, None, files, Path::new(output), &registry)
        };
        let chosen = choose(&OutputChoice::default(), &one(Path::new("a.JPG")), "a.JPG").unwrap();
        assert_eq!(chosen.muxer, Some(reference("builtin/image")));
        assert_eq!(chosen.video.component, Some(reference("builtin/jpeg")));
        assert_eq!(chosen.audio, AudioChoice::None);
        assert_eq!(chosen.preset, None);
        let chosen = choose(&OutputChoice::default(), &two(Path::new("a.png")), "a.png").unwrap();
        assert_eq!(chosen.muxer, Some(reference("builtin/imageSequence")));
        assert_eq!(chosen.video.component, Some(reference("builtin/png")));
        for (output, encoder) in [
            ("a.jpeg", "builtin/jpeg"),
            ("a.WebP", "builtin/webp"),
            ("a.png", "builtin/png"),
        ] {
            let chosen = choose(&OutputChoice::default(), &one(Path::new(output)), output).unwrap();
            assert_eq!(chosen.video.component, Some(reference(encoder)), "{output}");
        }
        // An extension no built-in muxer writes keeps today's message.
        for output in ["check.gif", "check.mp4", "check"] {
            let error =
                choose(&OutputChoice::default(), &one(Path::new(output)), output).unwrap_err();
            assert!(
                matches!(&error, ExportError::InvalidSelection(message)
                    if message == "still output must use the .png, .jpg, .jpeg or .webp extension"),
                "{output}: {error}"
            );
        }
    }

    #[test]
    fn quality_becomes_the_encoders_option() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("a.webp");
        let (still, _) = open(
            &output,
            &OutputChoice::default(),
            Some(40),
            one(&output),
            false,
            (8, 8),
        )
        .unwrap();
        assert_eq!(
            still.resolved.video.settings.get("quality"),
            Some(&SettingValue::Number(40.0))
        );
        still.abort();
        let jpeg = dir.path().join("a.jpg");
        let (still, _) = open(
            &jpeg,
            &OutputChoice::default(),
            None,
            one(&jpeg),
            false,
            (8, 8),
        )
        .unwrap();
        assert_eq!(
            still.resolved.video.settings.get("quality"),
            Some(&SettingValue::Number(f64::from(
                StillExport::DEFAULT_QUALITY
            )))
        );
        still.abort();

        let error = open(
            &output,
            &with_video_options(&[("quality", "40")]),
            Some(40),
            one(&output),
            false,
            (8, 8),
        )
        .err()
        .unwrap();
        assert!(
            matches!(&error, ExportError::InvalidSelection(message) if message == "quality is given twice"),
            "{error}"
        );
        for quality in [0, 101] {
            let error = open(
                &jpeg,
                &OutputChoice::default(),
                Some(quality),
                one(&jpeg),
                false,
                (8, 8),
            )
            .err()
            .unwrap();
            assert_eq!(code(&error), "invalid_option", "{quality}: {error}");
        }
        assert!(names(dir.path()).is_empty(), "{:?}", names(dir.path()));
    }

    #[test]
    fn presets_and_audio_do_not_apply() {
        let registry = registry();
        let output = Path::new("a.png");
        let refused = |choice: &OutputChoice| match still_choice(
            choice,
            None,
            &one(output),
            output,
            &registry,
        ) {
            Err(ExportError::InvalidSelection(message)) => message,
            other => panic!("{other:?}"),
        };
        assert_eq!(
            refused(&OutputChoice {
                preset: Some("web".to_owned()),
                ..OutputChoice::default()
            }),
            "presets apply to video output, not still frames"
        );
        for audio in [
            EncoderChoice {
                component: Some(reference("ffmpeg/aac")),
                options: Default::default(),
            },
            EncoderChoice {
                component: None,
                options: [("bitrate".to_owned(), SettingInput::Text("1".to_owned()))].into(),
            },
        ] {
            assert_eq!(
                refused(&OutputChoice {
                    audio: AudioChoice::Encode(audio),
                    ..OutputChoice::default()
                }),
                "audio applies to video output, not still frames"
            );
        }
        // No audio, or the default audio, is no audio for a still.
        for audio in [AudioChoice::None, AudioChoice::default()] {
            let choice = OutputChoice {
                audio,
                ..OutputChoice::default()
            };
            let chosen = still_choice(&choice, None, &one(output), output, &registry).unwrap();
            assert_eq!(chosen.audio, AudioChoice::None);
        }
    }

    #[test]
    fn a_named_muxer_must_match_the_file_count() {
        let registry = registry();
        let output = Path::new("a.png");
        let choose = |muxer: &str, files: &StillFiles| {
            still_choice(&naming(muxer), None, files, output, &registry)
        };
        let error = choose("builtin/imageSequence", &one(output)).unwrap_err();
        assert!(
            matches!(&error, ExportError::InvalidSelection(message) if message
                == "builtin/imageSequence writes image sequences; one image needs a muxer such as builtin/image"),
            "{error}"
        );
        let error = choose("builtin/image", &two(output)).unwrap_err();
        assert!(
            matches!(&error, ExportError::InvalidSelection(message) if message
                == "builtin/image writes one image; several frames need a sequence muxer such as builtin/imageSequence"),
            "{error}"
        );
        assert!(choose("builtin/image", &one(output)).is_ok());
        let chosen = choose("builtin/imageSequence", &two(output)).unwrap();
        assert_eq!(chosen.video.component, Some(reference("builtin/png")));
        // A muxer that is not installed is the resolution's to refuse.
        let chosen = choose("other/tiff", &one(output)).unwrap();
        assert_eq!(chosen.muxer, Some(reference("other/tiff")));
        assert_eq!(chosen.video.component, None);
    }

    #[test]
    fn a_mismatched_built_in_codec_is_incompatible() {
        let registry = registry();
        let output = Path::new("a.png");
        let choice = OutputChoice {
            video: EncoderChoice {
                component: Some(reference("builtin/webp")),
                options: Default::default(),
            },
            ..OutputChoice::default()
        };
        let error = still_choice(&choice, None, &one(output), output, &registry).unwrap_err();
        match &error {
            ExportError::IncompatibleComponents {
                component,
                muxer,
                reason,
            } => {
                assert_eq!(*component, reference("builtin/webp"));
                assert_eq!(*muxer, reference("builtin/image"));
                assert_eq!(
                    reason,
                    "builtin/image writes .png files as png, and builtin/webp writes webp"
                );
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(code(&error), "incompatible_components");
        let matching = OutputChoice {
            video: EncoderChoice {
                component: Some(reference("builtin/png")),
                options: Default::default(),
            },
            ..OutputChoice::default()
        };
        assert!(still_choice(&matching, None, &one(output), output, &registry).is_ok());
    }

    #[test]
    fn png_quality_is_an_invalid_option() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("a.png");
        let error = open(
            &output,
            &OutputChoice::default(),
            Some(80),
            one(&output),
            false,
            (8, 8),
        )
        .err()
        .unwrap();
        assert_eq!(code(&error), "invalid_option", "{error}");
        assert!(
            matches!(&error, ExportError::InvalidOption { key, .. } if key == "quality"),
            "{error}"
        );
        assert!(names(dir.path()).is_empty());
    }

    #[test]
    fn webp_too_large_fails_before_rendering() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("a.webp");
        let error = open(
            &output,
            &OutputChoice::default(),
            None,
            one(&output),
            true,
            (16_384, 9),
        )
        .err()
        .unwrap();
        assert_eq!(code(&error), "invalid_option", "{error}");
        assert!(
            error.to_string().contains(
                "a 16384x9 image is too large for WebP, which holds at most 16383 pixels on each \
                 side (write PNG or JPEG, or make the contact sheet smaller)"
            ),
            "{error}"
        );
        assert!(names(dir.path()).is_empty());
        let (still, _) = open(
            &output,
            &OutputChoice::default(),
            None,
            one(&output),
            true,
            (16_383, 16_383),
        )
        .unwrap();
        still.abort();
        for other in ["a.jpg", "a.png"] {
            let output = dir.path().join(other);
            let (still, _) = open(
                &output,
                &OutputChoice::default(),
                None,
                one(&output),
                true,
                (16_384, 9),
            )
            .unwrap();
            still.abort();
        }
        assert!(names(dir.path()).is_empty());
    }

    #[test]
    fn sixteen_bit_png_is_not_for_contact_sheets() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("a.png");
        let choice = with_video_options(&[("depth", "16")]);
        let error = open(&output, &choice, None, one(&output), true, (8, 8))
            .err()
            .unwrap();
        assert_eq!(code(&error), "invalid_option", "{error}");
        assert!(
            matches!(&error, ExportError::Codec {
                source: MediaError::InvalidSettings { key, .. }, ..
            } if key == "depth"),
            "{error}"
        );
        let (still, _) = open(&output, &choice, None, one(&output), false, (8, 8)).unwrap();
        assert_eq!(still.opened().input, PixelFormat::Rgba16);
        still.abort();
        let (still, _) = open(
            &output,
            &OutputChoice::default(),
            None,
            one(&output),
            true,
            (8, 8),
        )
        .unwrap();
        assert_eq!(still.opened().input, PixelFormat::Rgba8);
        still.abort();
        assert!(names(dir.path()).is_empty());
    }

    #[test]
    fn a_refusal_at_open_creates_no_folder() {
        let dir = tempfile::tempdir().unwrap();
        // A contact sheet refusing 16 bits, and WebP refusing a size, each
        // into a folder that does not exist yet.
        let sheet = dir.path().join("new1/s.png");
        let choice = with_video_options(&[("depth", "16")]);
        let error = open(&sheet, &choice, None, one(&sheet), true, (8, 8))
            .err()
            .unwrap();
        assert_eq!(code(&error), "invalid_option", "{error}");
        let webp = dir.path().join("new2/a.webp");
        let error = open(
            &webp,
            &OutputChoice::default(),
            None,
            one(&webp),
            true,
            (16_384, 9),
        )
        .err()
        .unwrap();
        assert_eq!(code(&error), "invalid_option", "{error}");
        let sequence = dir.path().join("new3/c.webp");
        let error = open(
            &sequence,
            &OutputChoice::default(),
            None,
            two(&sequence),
            false,
            (16_384, 9),
        )
        .err()
        .unwrap();
        assert_eq!(code(&error), "invalid_option", "{error}");
        assert!(names(dir.path()).is_empty(), "{:?}", names(dir.path()));
    }

    #[test]
    fn a_still_encoder_error_names_image_encoding() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("a.png");
        let choice = with_video_options(&[("depth", "16")]);
        let error = open(&output, &choice, None, one(&output), true, (8, 8))
            .err()
            .unwrap();
        assert!(
            matches!(&error, ExportError::Codec { stage, .. } if *stage == IMAGE_STAGE),
            "{error:?}"
        );
        assert!(error.to_string().starts_with("image encoding"), "{error}");
    }

    #[test]
    fn jpeg_stills_are_opaque() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("a.jpg");
        let (mut still, _) = open(
            &output,
            &OutputChoice::default(),
            Some(100),
            one(&output),
            false,
            (16, 8),
        )
        .unwrap();
        // JPEG has no alpha: the renderer clears to the opaque background.
        assert!(!still.opened().alpha);
        assert_eq!(still.opened().input, PixelFormat::Rgba8);
        assert_eq!(
            write(&mut still, 3, opaque(16, 8, [255, 0, 0])).unwrap(),
            []
        );
        assert!(!output.exists());
        assert_eq!(finish(still).unwrap(), [(3, output.clone())]);
        let decoded = image::load_from_memory(&fs::read(&output).unwrap()).unwrap();
        assert_eq!(decoded.color(), image::ColorType::Rgb8);
        let pixel = decoded.to_rgb8().get_pixel(4, 4).0;
        assert!(
            pixel
                .iter()
                .zip([255, 0, 0])
                .all(|(&a, b)| a.abs_diff(b) <= 4),
            "{pixel:?}"
        );
        assert_eq!(names(dir.path()), ["a.jpg"]);
        // And it takes no transparency.
        let other = dir.path().join("b.jpg");
        let error = open(
            &other,
            &with_video_options(&[("transparent", "true")]),
            None,
            one(&other),
            false,
            (16, 8),
        )
        .err()
        .unwrap();
        assert_eq!(code(&error), "invalid_option", "{error}");
    }

    #[test]
    fn webp_is_lossy_at_the_requested_quality() {
        let dir = tempfile::tempdir().unwrap();
        // A noisy frame, so lower quality visibly saves bytes.
        let pixels: Vec<u8> = (0u32..64 * 64)
            .flat_map(|index| {
                let value = (index.wrapping_mul(2_654_435_761) >> 24) as u8;
                [value, value.wrapping_mul(3), 255 - value, 255]
            })
            .collect();
        let write = |quality| {
            let path = dir.path().join(format!("q{quality}.webp"));
            let (mut still, _) = open(
                &path,
                &OutputChoice::default(),
                Some(quality),
                one(&path),
                false,
                (64, 64),
            )
            .unwrap();
            write(&mut still, 0, pixels.clone()).unwrap();
            finish(still).unwrap();
            fs::read(path).unwrap()
        };
        let (low, high) = (write(10), write(100));
        for bytes in [&low, &high] {
            assert_eq!(&bytes[..4], b"RIFF");
            assert_eq!(&bytes[8..12], b"WEBP");
            let image = webp::Decoder::new(bytes).decode().unwrap();
            assert_eq!((image.width(), image.height()), (64, 64));
        }
        // VP8 (lossy), not VP8L (lossless).
        assert_eq!(&high[12..16], b"VP8 ");
        assert!(low.len() < high.len(), "{} >= {}", low.len(), high.len());
    }

    #[test]
    fn a_sequence_publishes_each_frame_as_it_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("nested/c%06d.WEBP");
        let files = two(&output);
        let StillFiles::Sequence(named) = &files else {
            unreachable!()
        };
        let (first, second) = (named[0].1.clone(), named[1].1.clone());
        let (mut still, warnings) = open(
            &output,
            &OutputChoice::default(),
            None,
            files,
            false,
            (8, 8),
        )
        .unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        let nested = dir.path().join("nested");
        // The staging folder is hidden beside the output.
        assert!(
            names(&nested)
                .iter()
                .all(|name| name.starts_with(".celesta-")),
            "{:?}",
            names(&nested)
        );
        assert_eq!(
            write(&mut still, 0, opaque(8, 8, [0, 255, 0])).unwrap(),
            [(0, first.clone())]
        );
        let bytes = fs::read(&first).unwrap();
        assert_eq!(&bytes[8..12], b"WEBP");
        assert_eq!(
            write(&mut still, 1, opaque(8, 8, [0, 0, 255])).unwrap(),
            [(1, second)]
        );
        assert_eq!(finish(still).unwrap(), []);
        assert_eq!(names(&nested), ["c%06d-000000.WEBP", "c%06d-000001.WEBP"]);
    }

    #[test]
    fn an_aborted_sequence_keeps_published_frames_and_no_staging() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("c.png");
        let files = two(&output);
        let (mut still, _) = open(
            &output,
            &OutputChoice::default(),
            None,
            files,
            false,
            (8, 8),
        )
        .unwrap();
        write(&mut still, 0, opaque(8, 8, [0, 255, 0])).unwrap();
        still.abort();
        assert_eq!(names(dir.path()), ["c-000000.png"]);
    }

    #[test]
    fn a_sequence_frame_never_clobbers_unless_asked() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("c.png");
        let taken = dir.path().join("c-000001.png");
        let open_with = |overwrite| {
            StillOutput::open(
                registry(),
                &OutputChoice::default(),
                None,
                two(&output),
                &output,
                false,
                (8, 8),
                Rational::new(4, 1),
                overwrite,
                &ExportCancellation::default(),
            )
            .unwrap()
            .0
        };
        // Taken after the checks before rendering.
        let mut still = open_with(false);
        fs::write(&taken, b"keep").unwrap();
        write(&mut still, 0, opaque(8, 8, [0, 255, 0])).unwrap();
        let error = write(&mut still, 1, opaque(8, 8, [0, 255, 0])).unwrap_err();
        assert!(
            matches!(&error, ExportError::OutputExists(path) if *path == taken),
            "{error}"
        );
        still.abort();
        assert_eq!(fs::read(&taken).unwrap(), b"keep");
        assert_eq!(names(dir.path()), ["c-000000.png", "c-000001.png"]);

        let mut still = open_with(true);
        write(&mut still, 0, opaque(8, 8, [0, 255, 0])).unwrap();
        write(&mut still, 1, opaque(8, 8, [0, 255, 0])).unwrap();
        finish(still).unwrap();
        assert_eq!(&fs::read(&taken).unwrap()[1..4], b"PNG");
        assert_eq!(names(dir.path()), ["c-000000.png", "c-000001.png"]);
    }

    /// A video encoder writing `png` that holds each frame back by one: a
    /// frame's packet comes with the next frame's call, the last with
    /// `finish`. Each packet's bytes are the frame's pixels.
    #[derive(Default)]
    struct Delayed {
        held: Option<Packet>,
        config_due: bool,
        /// Holds every frame back until `finish`, which gives them all.
        hold_all: bool,
        all: Vec<Packet>,
    }

    impl MediaProvider for Delayed {
        fn describe(&mut self) -> Result<Vec<ComponentDescription>, MediaError> {
            Ok(vec![ComponentDescription {
                kind: ComponentKind::VideoEncoder,
                id: "delayed".to_owned(),
                available: true,
                reason: None,
                settings: Vec::new(),
                input_formats: vec![PixelFormat::Rgba8],
                transports: Vec::new(),
                sample_rates: None,
                channels: Vec::new(),
            }])
        }

        fn video_encoder(&mut self) -> Option<&mut dyn VideoEncoder> {
            Some(self)
        }
    }

    impl VideoEncoder for Delayed {
        fn open_video(
            &mut self,
            _: &VideoEncoderRequest<'_>,
        ) -> Result<VideoEncoderOpened, MediaError> {
            self.config_due = true;
            Ok(VideoEncoderOpened {
                session: SessionId(1),
                input: PixelFormat::Rgba8,
                alpha: false,
            })
        }

        fn encode_video(
            &mut self,
            _: SessionId,
            pts: i64,
            frame: Vec<u8>,
        ) -> Result<Encoded, MediaError> {
            let packet = Packet {
                pts,
                dts: pts,
                duration: 1,
                key: true,
                bytes: frame,
                additional: Vec::new(),
            };
            let packets = if self.hold_all {
                self.all.push(packet);
                Vec::new()
            } else {
                self.held.replace(packet).into_iter().collect()
            };
            Ok(Encoded {
                packets,
                config: std::mem::take(&mut self.config_due).then(|| TrackConfig {
                    codec: "png".to_owned(),
                    description: None,
                }),
            })
        }

        fn finish_video(&mut self, _: SessionId) -> Result<Encoded, MediaError> {
            let mut packets = std::mem::take(&mut self.all);
            packets.extend(self.held.take());
            Ok(Encoded {
                packets,
                config: None,
            })
        }

        fn abort_video(&mut self, _: SessionId) {}
    }

    fn delayed_plugin() -> celesta_media::RegisteredPlugin {
        delayed_plugin_holding(false)
    }

    fn delayed_plugin_holding(hold_all: bool) -> celesta_media::RegisteredPlugin {
        celesta_media::RegisteredPlugin {
            id: "fake".to_owned(),
            name: "fake".to_owned(),
            version: "1.0.0".to_owned(),
            origin: MediaOrigin::User,
            directory: PathBuf::new(),
            executable: None,
            media: celesta_plugin_host::MediaManifest {
                priority: 0,
                decoders: Vec::new(),
                video_encoders: vec![celesta_plugin_host::MediaVideoEncoderEntry {
                    id: "delayed".to_owned(),
                    name: "Delayed".to_owned(),
                    codecs: vec!["png".to_owned()],
                    input_formats: Some(vec!["rgba8".to_owned()]),
                }],
                audio_encoders: Vec::new(),
                muxers: Vec::new(),
            },
            factory: Arc::new(move |_, _| {
                Ok(Box::new(Delayed {
                    hold_all,
                    ..Delayed::default()
                }) as Box<dyn MediaProvider>)
            }),
        }
    }

    #[test]
    fn a_held_back_frame_is_published_under_its_own_name() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("c.png");
        let named = |frame: u64| dir.path().join(format!("c-{frame:06}.png"));
        let files = StillFiles::Sequence([4, 0, 9].map(|frame| (frame, named(frame))).to_vec());
        let choice = OutputChoice {
            video: EncoderChoice {
                component: Some(reference("fake/delayed")),
                options: Default::default(),
            },
            ..OutputChoice::default()
        };
        let (mut still, _) = StillOutput::open(
            with_builtin(vec![delayed_plugin()]),
            &choice,
            None,
            files,
            &output,
            false,
            (1, 1),
            Rational::new(30_000, 1_001),
            false,
            &ExportCancellation::default(),
        )
        .unwrap();
        assert_eq!(
            still.resolved.muxer.reference,
            reference("builtin/imageSequence")
        );
        let pixel = |frame: u8| vec![frame, frame, frame, 255];
        assert_eq!(write(&mut still, 4, pixel(4)).unwrap(), []);
        assert_eq!(
            names(dir.path())
                .iter()
                .filter(|n| n.starts_with('c'))
                .count(),
            0
        );
        assert_eq!(write(&mut still, 0, pixel(0)).unwrap(), [(4, named(4))]);
        assert_eq!(write(&mut still, 9, pixel(9)).unwrap(), [(0, named(0))]);
        assert_eq!(finish(still).unwrap(), [(9, named(9))]);
        for frame in [4, 0, 9] {
            assert_eq!(
                fs::read(named(frame)).unwrap(),
                pixel(frame as u8),
                "{frame}"
            );
        }
        assert_eq!(
            names(dir.path()),
            ["c-000000.png", "c-000004.png", "c-000009.png"]
        );
    }

    #[test]
    fn a_flush_failing_partway_keeps_the_frames_it_published() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("c.png");
        let named = |frame: u64| dir.path().join(format!("c-{frame:06}.png"));
        let files = StillFiles::Sequence([0, 1, 2].map(|frame| (frame, named(frame))).to_vec());
        let choice = OutputChoice {
            video: EncoderChoice {
                component: Some(reference("fake/delayed")),
                options: Default::default(),
            },
            ..OutputChoice::default()
        };
        let (mut still, _) = StillOutput::open(
            with_builtin(vec![delayed_plugin_holding(true)]),
            &choice,
            None,
            files,
            &output,
            false,
            (1, 1),
            Rational::new(4, 1),
            false,
            &ExportCancellation::default(),
        )
        .unwrap();
        let pixel = |frame: u8| vec![frame, frame, frame, 255];
        for frame in 0..3 {
            assert_eq!(write(&mut still, frame, pixel(frame as u8)).unwrap(), []);
        }
        // Frame 1's name is taken once rendering started: the flush gives all
        // three frames and publishes frame 0 before it fails on frame 1.
        fs::write(named(1), b"keep").unwrap();
        let mut published = Vec::new();
        let error = still.finish(&mut published).unwrap_err();
        assert!(
            matches!(&error, ExportError::OutputExists(path) if *path == named(1)),
            "{error}"
        );
        assert_eq!(published, [(0, named(0))]);
        assert_eq!(fs::read(named(0)).unwrap(), pixel(0));
        assert_eq!(fs::read(named(1)).unwrap(), b"keep");
        assert_eq!(names(dir.path()), ["c-000000.png", "c-000001.png"]);
    }

    /// How [`FakeImage`] misbehaves.
    #[derive(Clone, Copy, Default)]
    struct Mode {
        /// Added to each packet's timestamp.
        shift: i64,
        /// A composition frame the encoder gives no packet for.
        drop: Option<u64>,
        /// A composition frame the sequence muxer writes no file for.
        skip: Option<u64>,
        /// Whether `finish_muxer` leaves the files out of its answer.
        unlisted: bool,
    }

    /// A plugin's image components: `raw`, an encoder writing `png` whose
    /// packets are the frame's pixels, and two muxers writing `.png`:
    /// `frames` (a sequence) and `single` (one file). Each writes a
    /// packet's bytes as they are.
    #[derive(Default)]
    struct FakeImage {
        mode: Mode,
        config_due: bool,
        output: PathBuf,
        sequence: bool,
        files: Vec<PathBuf>,
    }

    impl MediaProvider for FakeImage {
        fn describe(&mut self) -> Result<Vec<ComponentDescription>, MediaError> {
            let describe = |kind, id: &str| ComponentDescription {
                kind,
                id: id.to_owned(),
                available: true,
                reason: None,
                settings: Vec::new(),
                input_formats: if kind == ComponentKind::VideoEncoder {
                    vec![PixelFormat::Rgba8]
                } else {
                    Vec::new()
                },
                transports: Vec::new(),
                sample_rates: None,
                channels: Vec::new(),
            };
            Ok(vec![
                describe(ComponentKind::VideoEncoder, "raw"),
                describe(ComponentKind::Muxer, "frames"),
                describe(ComponentKind::Muxer, "single"),
            ])
        }

        fn video_encoder(&mut self) -> Option<&mut dyn VideoEncoder> {
            Some(self)
        }

        fn muxer(&mut self) -> Option<&mut dyn celesta_media::Muxer> {
            Some(self)
        }
    }

    impl VideoEncoder for FakeImage {
        fn open_video(
            &mut self,
            _: &VideoEncoderRequest<'_>,
        ) -> Result<VideoEncoderOpened, MediaError> {
            self.config_due = true;
            Ok(VideoEncoderOpened {
                session: SessionId(1),
                input: PixelFormat::Rgba8,
                alpha: false,
            })
        }

        fn encode_video(
            &mut self,
            _: SessionId,
            pts: i64,
            frame: Vec<u8>,
        ) -> Result<Encoded, MediaError> {
            let config = std::mem::take(&mut self.config_due).then(|| TrackConfig {
                codec: "png".to_owned(),
                description: None,
            });
            let packets = if self.mode.drop == u64::try_from(pts).ok() {
                Vec::new()
            } else {
                let pts = pts + self.mode.shift;
                vec![Packet {
                    pts,
                    dts: pts,
                    duration: 1,
                    key: true,
                    bytes: frame,
                    additional: Vec::new(),
                }]
            };
            Ok(Encoded { packets, config })
        }

        fn finish_video(&mut self, _: SessionId) -> Result<Encoded, MediaError> {
            Ok(Encoded::default())
        }

        fn abort_video(&mut self, _: SessionId) {}
    }

    impl celesta_media::Muxer for FakeImage {
        fn open_muxer(&mut self, request: &MuxerRequest<'_>) -> Result<SessionId, MediaError> {
            self.output = request.output.to_owned();
            self.sequence = request.component == "frames";
            Ok(SessionId(2))
        }

        fn write_packets(
            &mut self,
            _: SessionId,
            packets: &[TrackPacket],
        ) -> Result<(), MediaError> {
            for packet in packets {
                // These tests run at 4 fps: a timestamp is its frame.
                let frame = packet.packet.pts as u64;
                let path = if self.sequence {
                    if self.mode.skip == Some(frame) {
                        continue;
                    }
                    sequence_path(&self.output, frame)?
                } else {
                    self.output.clone()
                };
                fs::write(&path, &packet.packet.bytes)
                    .map_err(|error| MediaError::Codec(error.to_string()))?;
                if !self.files.contains(&path) {
                    self.files.push(path);
                }
            }
            Ok(())
        }

        fn finish_muxer(&mut self, _: SessionId) -> Result<Vec<PathBuf>, MediaError> {
            let files = std::mem::take(&mut self.files);
            Ok(if self.mode.unlisted {
                Vec::new()
            } else {
                files
            })
        }

        fn abort_muxer(&mut self, _: SessionId) -> Vec<PathBuf> {
            std::mem::take(&mut self.files)
        }
    }

    fn fake_image_plugin(mode: Mode) -> celesta_media::RegisteredPlugin {
        let muxer = |id: &str, sequence| celesta_plugin_host::MediaMuxerEntry {
            id: id.to_owned(),
            name: id.to_owned(),
            extensions: vec!["png".to_owned()],
            video: vec!["png".to_owned()],
            audio: None,
            default_video: Some("png".to_owned()),
            default_audio: None,
            trims_leading_audio: false,
            sequence,
        };
        celesta_media::RegisteredPlugin {
            id: "fake".to_owned(),
            name: "fake".to_owned(),
            version: "1.0.0".to_owned(),
            origin: MediaOrigin::User,
            directory: PathBuf::new(),
            executable: None,
            media: celesta_plugin_host::MediaManifest {
                priority: 0,
                decoders: Vec::new(),
                video_encoders: vec![celesta_plugin_host::MediaVideoEncoderEntry {
                    id: "raw".to_owned(),
                    name: "Raw".to_owned(),
                    codecs: vec!["png".to_owned()],
                    input_formats: Some(vec!["rgba8".to_owned()]),
                }],
                audio_encoders: Vec::new(),
                muxers: vec![muxer("frames", true), muxer("single", false)],
            },
            factory: Arc::new(move |_, _| {
                Ok(Box::new(FakeImage {
                    mode,
                    ..FakeImage::default()
                }) as Box<dyn MediaProvider>)
            }),
        }
    }

    /// Opens `muxer` of [`FakeImage`] for `files` of `output`, 1x1 at 4 fps.
    fn open_fake(
        mode: Mode,
        muxer: &str,
        files: StillFiles,
        output: &Path,
        cancellation: &ExportCancellation,
    ) -> StillOutput {
        StillOutput::open(
            with_builtin(vec![fake_image_plugin(mode)]),
            &naming(muxer),
            None,
            files,
            output,
            false,
            (1, 1),
            Rational::new(4, 1),
            false,
            cancellation,
        )
        .unwrap()
        .0
    }

    fn sequence_of(dir: &Path, frames: &[u64]) -> (PathBuf, StillFiles) {
        let named = |frame: u64| (frame, dir.join(format!("c-{frame:06}.png")));
        (
            dir.join("c.png"),
            StillFiles::Sequence(frames.iter().map(|&frame| named(frame)).collect()),
        )
    }

    fn pixel(value: u8) -> Vec<u8> {
        vec![value, value, value, 255]
    }

    fn muxer_message(error: &ExportError) -> String {
        match error {
            ExportError::Codec {
                stage,
                source: MediaError::Codec(message),
            } if *stage == MUXER_STAGE => message.clone(),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_plugins_sequence_muxer_writes_each_frame_before_it_answers() {
        let dir = tempfile::tempdir().unwrap();
        let (output, files) = sequence_of(dir.path(), &[2, 0]);
        let mut still = open_fake(
            Mode::default(),
            "fake/frames",
            files,
            &output,
            &ExportCancellation::default(),
        );
        // The muxer's own plugin brings the encoder of its default codec.
        assert_eq!(still.resolved.video.reference, reference("fake/raw"));
        let named = |frame: u64| dir.path().join(format!("c-{frame:06}.png"));
        assert_eq!(write(&mut still, 2, pixel(2)).unwrap(), [(2, named(2))]);
        assert_eq!(fs::read(named(2)).unwrap(), pixel(2));
        assert_eq!(write(&mut still, 0, pixel(0)).unwrap(), [(0, named(0))]);
        assert_eq!(finish(still).unwrap(), []);
        assert_eq!(fs::read(named(0)).unwrap(), pixel(0));
        assert_eq!(names(dir.path()), ["c-000000.png", "c-000002.png"]);
    }

    #[test]
    fn a_frame_the_sequence_muxer_did_not_write_fails() {
        let dir = tempfile::tempdir().unwrap();
        let (output, files) = sequence_of(dir.path(), &[0, 1]);
        let mode = Mode {
            skip: Some(1),
            ..Mode::default()
        };
        let mut still = open_fake(
            mode,
            "fake/frames",
            files,
            &output,
            &ExportCancellation::default(),
        );
        write(&mut still, 0, pixel(0)).unwrap();
        let error = write(&mut still, 1, pixel(1)).unwrap_err();
        assert_eq!(
            muxer_message(&error),
            "fake/frames wrote no file for frame 1"
        );
        still.abort();
        assert_eq!(names(dir.path()), ["c-000000.png"]);
    }

    #[test]
    fn a_packet_for_a_frame_not_selected_fails() {
        let dir = tempfile::tempdir().unwrap();
        let (output, files) = sequence_of(dir.path(), &[0, 1]);
        let mode = Mode {
            shift: 10,
            ..Mode::default()
        };
        let mut still = open_fake(
            mode,
            "fake/frames",
            files,
            &output,
            &ExportCancellation::default(),
        );
        let error = write(&mut still, 0, pixel(0)).unwrap_err();
        assert_eq!(
            muxer_message(&error),
            "frame 10 is not one this export writes"
        );
        still.abort();
        assert!(names(dir.path()).is_empty(), "{:?}", names(dir.path()));
    }

    #[test]
    fn a_sequence_missing_a_frame_fails_at_finish() {
        let dir = tempfile::tempdir().unwrap();
        let (output, files) = sequence_of(dir.path(), &[0, 1, 2]);
        let mode = Mode {
            drop: Some(1),
            ..Mode::default()
        };
        let mut still = open_fake(
            mode,
            "fake/frames",
            files,
            &output,
            &ExportCancellation::default(),
        );
        for frame in 0..3 {
            write(&mut still, frame, pixel(frame as u8)).unwrap();
        }
        let mut published = Vec::new();
        let error = still.finish(&mut published).unwrap_err();
        assert_eq!(
            muxer_message(&error),
            "fake/frames wrote no file for frame 1"
        );
        assert!(published.is_empty());
        // The frames written stay, and the staging folder goes.
        assert_eq!(names(dir.path()), ["c-000000.png", "c-000002.png"]);
    }

    #[test]
    fn a_plugins_one_file_muxer_publishes_its_image() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("a.png");
        let mut still = open_fake(
            Mode::default(),
            "fake/single",
            one(&output),
            &output,
            &ExportCancellation::default(),
        );
        assert_eq!(write(&mut still, 3, pixel(3)).unwrap(), []);
        assert!(!output.exists());
        assert_eq!(finish(still).unwrap(), [(3, output.clone())]);
        assert_eq!(fs::read(&output).unwrap(), pixel(3));
        assert_eq!(names(dir.path()), ["a.png"]);
    }

    #[test]
    fn a_one_file_muxer_must_answer_with_the_staged_file() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("a.png");
        let mode = Mode {
            unlisted: true,
            ..Mode::default()
        };
        let mut still = open_fake(
            mode,
            "fake/single",
            one(&output),
            &output,
            &ExportCancellation::default(),
        );
        write(&mut still, 0, pixel(0)).unwrap();
        let error = finish(still).unwrap_err();
        assert_eq!(muxer_message(&error), "fake/single wrote no image file");
        assert!(names(dir.path()).is_empty(), "{:?}", names(dir.path()));
    }

    #[test]
    fn a_cancel_before_finish_publishes_no_image() {
        for sheet in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let output = dir.path().join("a.png");
            let cancellation = ExportCancellation::default();
            let (mut still, _) = StillOutput::open(
                registry(),
                &OutputChoice::default(),
                None,
                one(&output),
                &output,
                sheet,
                (8, 8),
                Rational::new(4, 1),
                false,
                &cancellation,
            )
            .unwrap();
            write(&mut still, 0, opaque(8, 8, [0, 255, 0])).unwrap();
            cancellation.cancel();
            let error = finish(still).unwrap_err();
            assert!(matches!(error, ExportError::Cancelled), "{sheet}: {error}");
            assert!(
                names(dir.path()).is_empty(),
                "{sheet}: {:?}",
                names(dir.path())
            );
        }
    }

    #[test]
    fn a_failed_publish_names_the_image_and_leaves_no_staging() {
        let dir = tempfile::tempdir().unwrap();
        // A folder where the image would go: the publish cannot replace it.
        let output = dir.path().join("a.png");
        let (mut still, _) = StillOutput::open(
            registry(),
            &OutputChoice::default(),
            None,
            one(&output),
            &output,
            false,
            (8, 8),
            Rational::new(4, 1),
            true,
            &ExportCancellation::default(),
        )
        .unwrap();
        write(&mut still, 0, opaque(8, 8, [0, 255, 0])).unwrap();
        fs::create_dir(&output).unwrap();
        fs::write(output.join("keep"), b"keep").unwrap();
        let error = finish(still).unwrap_err();
        assert!(
            matches!(
                &error,
                ExportError::Io {
                    operation: "publish image",
                    ..
                }
            ),
            "{error:?}"
        );
        assert!(
            error.to_string().contains(&output.display().to_string()),
            "{error}"
        );
        assert_eq!(names(dir.path()), ["a.png"]);
        assert_eq!(names(&output), ["keep"]);
    }
}
