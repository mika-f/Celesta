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
use crate::encode::{MUXER_STAGE, VIDEO_STAGE, check_opened};
use crate::error::ExportError;
use crate::options::{AudioChoice, OutputChoice, SettingInput};
use crate::output;
use crate::resolve::{ResolvedOutput, resolve_preset_output};
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
use std::collections::HashMap;
use std::collections::hash_map::Entry;
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
    /// `names` are the final names by composition frame.
    Sequence {
        /// Kept until the export ends, then removed with what is left in it.
        _folder: tempfile::TempDir,
        pattern: PathBuf,
        names: HashMap<u64, PathBuf>,
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
    /// ring; the built-ins in this process), stages `files` beside
    /// `output`, and opens the encoder for `size` frames at `frame_rate`,
    /// offering `rgba16` and `rgba8`, or only `rgba8` for a contact
    /// `sheet`. All before anything renders. Returns the resolution's
    /// warnings too.
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
        let warnings = std::mem::take(&mut resolved.warnings);
        let staging = stage(files, output)?;
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
                stage: VIDEO_STAGE,
                source,
            })
        })?;
        let still = Self {
            registry,
            cancellation: cancellation.clone(),
            providers,
            resolved,
            opened,
            encoder: Some(opened.session),
            muxer: None,
            config: None,
            size,
            frame_rate,
            overwrite,
            staging,
        };
        if let Err(source) = check_opened(offered, &opened) {
            let error = still.error(VIDEO_STAGE, source);
            still.abort();
            return Err(error);
        }
        Ok((still, warnings))
    }

    /// What the encoder answered: its input format, and whether it keeps
    /// alpha.
    pub(crate) fn opened(&self) -> &VideoEncoderOpened {
        &self.opened
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
    /// writes it. A sequence's file is published now and returned; one file
    /// stays staged until [`Self::finish`].
    pub(crate) fn write(
        &mut self,
        frame: u64,
        pixels: Vec<u8>,
    ) -> Result<Option<PathBuf>, ExportError> {
        let pts = i64::try_from(frame)
            .ok()
            .and_then(|frame| frame.checked_mul(i64::from(self.frame_rate.denominator)))
            .ok_or(ExportError::TimelineTooLong)?;
        let session = self.encoder.ok_or_else(|| {
            self.error(
                VIDEO_STAGE,
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
            .map_err(|source| self.error(VIDEO_STAGE, source))?;
        self.mux(encoded)?;
        match &self.staging {
            Staging::One { .. } => Ok(None),
            Staging::Sequence { .. } => self.publish_frame(frame).map(Some),
        }
    }

    /// Writes `encoded`'s packets, opening the muxer with the first config.
    fn mux(&mut self, encoded: Encoded) -> Result<(), ExportError> {
        if self.config.is_none() {
            self.config = encoded.config;
        }
        if encoded.packets.is_empty() {
            return Ok(());
        }
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
            .map_err(|source| self.error(MUXER_STAGE, source))
    }

    fn open_muxer(&mut self) -> Result<SessionId, ExportError> {
        let Some(config) = self.config.clone() else {
            return Err(self.error(
                VIDEO_STAGE,
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
        publish_path(&staged, name, self.overwrite).map_err(|source| {
            if source.kind() == io::ErrorKind::AlreadyExists {
                ExportError::OutputExists(name.clone())
            } else {
                ExportError::Io {
                    operation: "publish image",
                    source,
                }
            }
        })?;
        Ok(name.clone())
    }

    /// Flushes the encoder, finishes the muxer, and publishes one file,
    /// which it returns; a sequence's files are all published already. A
    /// failure aborts what is still open.
    pub(crate) fn finish(mut self) -> Result<Option<PathBuf>, ExportError> {
        match self.try_finish() {
            Ok(()) => {}
            Err(error) => {
                self.abort();
                return Err(error);
            }
        }
        match self.staging {
            Staging::One { file, path } => {
                output::publish(file, &path, self.overwrite, "publish image")?;
                Ok(Some(path))
            }
            Staging::Sequence { .. } => Ok(None),
        }
    }

    fn try_finish(&mut self) -> Result<(), ExportError> {
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
                .map_err(|source| self.error(VIDEO_STAGE, source))?;
            let frames: Vec<u64> = encoded
                .packets
                .iter()
                .filter_map(|packet| u64::try_from(packet.pts).ok())
                .map(|pts| pts / u64::from(self.frame_rate.denominator.max(1)))
                .collect();
            self.mux(encoded)?;
            if matches!(self.staging, Staging::Sequence { .. }) {
                for frame in frames {
                    self.publish_frame(frame)?;
                }
            }
        }
        let Some(session) = self.muxer.take() else {
            return Err(self.error(
                MUXER_STAGE,
                MediaError::Codec("no frame was written".to_owned()),
            ));
        };
        let plugin = self.resolved.muxer.plugin;
        let registry = self.registry.clone();
        self.provider(plugin)
            .and_then(|provider| {
                provider
                    .muxer()
                    .ok_or_else(|| missing(&registry, plugin, "muxer"))?
                    .finish_muxer(session)
            })
            .map(drop)
            .map_err(|source| self.error(MUXER_STAGE, source))
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
        assert_eq!(still.write(3, opaque(16, 8, [255, 0, 0])).unwrap(), None);
        assert!(!output.exists());
        assert_eq!(still.finish().unwrap(), Some(output.clone()));
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
            still.write(0, pixels.clone()).unwrap();
            still.finish().unwrap();
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
            still.write(0, opaque(8, 8, [0, 255, 0])).unwrap(),
            Some(first.clone())
        );
        let bytes = fs::read(&first).unwrap();
        assert_eq!(&bytes[8..12], b"WEBP");
        assert_eq!(
            still.write(1, opaque(8, 8, [0, 0, 255])).unwrap(),
            Some(second)
        );
        assert_eq!(still.finish().unwrap(), None);
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
        still.write(0, opaque(8, 8, [0, 255, 0])).unwrap();
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
        still.write(0, opaque(8, 8, [0, 255, 0])).unwrap();
        let error = still.write(1, opaque(8, 8, [0, 255, 0])).unwrap_err();
        assert!(
            matches!(&error, ExportError::OutputExists(path) if *path == taken),
            "{error}"
        );
        still.abort();
        assert_eq!(fs::read(&taken).unwrap(), b"keep");
        assert_eq!(names(dir.path()), ["c-000000.png", "c-000001.png"]);

        let mut still = open_with(true);
        still.write(0, opaque(8, 8, [0, 255, 0])).unwrap();
        still.write(1, opaque(8, 8, [0, 255, 0])).unwrap();
        still.finish().unwrap();
        assert_eq!(&fs::read(&taken).unwrap()[1..4], b"PNG");
        assert_eq!(names(dir.path()), ["c-000000.png", "c-000001.png"]);
    }
}
