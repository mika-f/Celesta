//! Frame-exact project export through the shared evaluator and renderers.

use std::collections::BTreeMap;
use std::error::Error;
use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;

use ez_ffmpeg::{FfmpegContext, Input, Output, VideoWriter};
use celesta_composition::{
    AssetLocation, AudioClip, AudioGraph, Layer, LayerContent, Rational, ResolvedAsset, Time,
    TimeError, TimeRange,
};
use celesta_evaluator::{EvaluationError, Evaluator};
use celesta_gpu_renderer::{GpuRenderError, GpuRenderOptions, GpuRenderer, ReadbackFormat};
use celesta_media::{AudioMixError, FfmpegBackend, mix_audio_graph_cancellable};
use celesta_project::{LoadError, Project, TimelineContent};
use celesta_react_bridge::{
    ProjectFrame, ReactAudioClipDescriptor, ReactBridge, ReactBridgeError, ReactCompositionMetadata,
};

/// Sample rate used to mix a React export's audio when no companion project
/// supplies its own `AudioGraph.sample_rate` (the project format has no
/// default of its own; every checked-in example project's `sampleRate` is
/// 48000, so this matches that convention).
const DEFAULT_REACT_AUDIO_SAMPLE_RATE: u32 = 48_000;

/// Timescale `<Audio>`'s `startFrom` (seconds) is converted at when built
/// into a `Time`, matching `packages/react/src/render.ts`'s
/// `SECONDS_TIMESCALE` for `<Video>`'s `startFrom`/`sourceTimeSeconds`.
const AUDIO_SECONDS_TIMESCALE: u32 = 1_000_000;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExportOptions {
    pub overwrite: bool,
    /// When set, only this composition-time span is rendered; the encoded
    /// output starts at its own `t = 0` (both video and the mixed audio are
    /// shifted so the span's start becomes the file's start). `None` exports
    /// the whole composition, byte-for-byte as before.
    pub range: Option<ExportRange>,
    /// H.264 encoder settings; the default matches the historical output.
    pub video: VideoEncoding,
}

/// Settings for the exported H.264 video stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VideoEncoding {
    /// Speed/compression trade-off. Faster presets encode much faster at the
    /// cost of a larger file for the same `crf`; they do not lower quality.
    pub preset: EncoderPreset,
    /// Constant rate factor, `0..=51`; lower is higher quality and larger.
    pub crf: u8,
    /// Where the rendered RGBA frames become the encoder's yuv420p.
    pub color_conversion: ColorConversion,
}

impl VideoEncoding {
    /// Highest CRF libx264 accepts for 8-bit output.
    pub const MAX_CRF: u8 = 51;
}

impl Default for VideoEncoding {
    fn default() -> Self {
        Self {
            preset: EncoderPreset::Medium,
            crf: 18,
            color_conversion: ColorConversion::Auto,
        }
    }
}

/// libx264's `-preset` values, fastest first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EncoderPreset {
    Ultrafast,
    Superfast,
    Veryfast,
    Faster,
    Fast,
    #[default]
    Medium,
    Slow,
    Slower,
    Veryslow,
}

impl EncoderPreset {
    pub const ALL: [Self; 9] = [
        Self::Ultrafast,
        Self::Superfast,
        Self::Veryfast,
        Self::Faster,
        Self::Fast,
        Self::Medium,
        Self::Slow,
        Self::Slower,
        Self::Veryslow,
    ];

    /// The name libx264 (and `ffmpeg -preset`) uses.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ultrafast => "ultrafast",
            Self::Superfast => "superfast",
            Self::Veryfast => "veryfast",
            Self::Faster => "faster",
            Self::Fast => "fast",
            Self::Medium => "medium",
            Self::Slow => "slow",
            Self::Slower => "slower",
            Self::Veryslow => "veryslow",
        }
    }
}

impl fmt::Display for EncoderPreset {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for EncoderPreset {
    type Err = UnknownEncoderPreset;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|preset| preset.as_str() == name)
            .ok_or_else(|| UnknownEncoderPreset(name.to_owned()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownEncoderPreset(pub String);

impl fmt::Display for UnknownEncoderPreset {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "unknown encoder preset '{}' (expected one of",
            self.0
        )?;
        for preset in EncoderPreset::ALL {
            write!(formatter, " {preset}")?;
        }
        formatter.write_str(")")
    }
}

impl Error for UnknownEncoderPreset {}

/// Where rendered RGBA frames are converted to yuv420p for the encoder.
///
/// On the GPU, only 1.5 instead of 4 bytes per pixel are read back and the
/// encoder skips its own conversion. Both use the BT.601 limited-range
/// matrix; the GPU averages each 2x2 block for chroma where libswscale
/// filters bicubically, a difference of a few code values at hard color
/// edges.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorConversion {
    /// On the GPU when it is a hardware GPU that supports it, otherwise in
    /// the encoder: a software renderer (lavapipe, WARP) runs the
    /// conversion passes slower than libswscale converts.
    #[default]
    Auto,
    /// Always on the GPU; the export fails if the GPU cannot.
    Gpu,
    /// Always in the encoder (libswscale), reading RGBA back.
    Encoder,
}

impl ColorConversion {
    pub const ALL: [Self; 3] = [Self::Auto, Self::Gpu, Self::Encoder];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Gpu => "gpu",
            Self::Encoder => "encoder",
        }
    }
}

impl fmt::Display for ColorConversion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ColorConversion {
    type Err = UnknownColorConversion;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|conversion| conversion.as_str() == name)
            .ok_or_else(|| UnknownColorConversion(name.to_owned()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownColorConversion(pub String);

impl fmt::Display for UnknownColorConversion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "unknown color conversion '{}' (expected auto, gpu, or encoder)",
            self.0
        )
    }
}

impl Error for UnknownColorConversion {}

/// A composition-time span to export. `start` is inclusive, `end` exclusive;
/// both are clamped into `[0, composition duration]` and `start` is snapped
/// down to a frame boundary. `end: None` means "to the end of the
/// composition".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExportRange {
    pub start: Time,
    pub end: Option<Time>,
}

impl ExportRange {
    /// `[start, end]` with an explicit end.
    pub fn new(start: Time, end: Time) -> Self {
        Self {
            start,
            end: Some(end),
        }
    }

    /// `[start, composition end)`.
    pub fn from(start: Time) -> Self {
        Self { start, end: None }
    }

    /// `[0, end)`.
    pub fn until(end: Time) -> Self {
        Self {
            start: Time::ZERO,
            end: Some(end),
        }
    }
}

/// The frame-snapped result of resolving an [`ExportRange`] against a concrete
/// composition duration and frame rate.
#[derive(Clone, Copy, Debug)]
struct ExportWindow {
    /// Snapped composition-time start (the amount video/audio are shifted by).
    start: Time,
    /// First composition frame index to render.
    start_frame: i64,
    /// Number of frames to render.
    frames: u64,
    /// `frames` expressed as a `Time` at the composition frame rate — the
    /// span the audio mixdown covers.
    duration: Time,
}

/// Resolves `range` against the composition's full `duration`/`frame_rate`:
/// clamps both ends into `[0, duration]`, snaps `start` down to a frame
/// boundary, and clamps the frame count so it never runs past the
/// composition. An empty span is [`ExportError::EmptyRange`].
fn resolve_window(
    range: &ExportRange,
    duration: Time,
    frame_rate: Rational,
) -> Result<ExportWindow, ExportError> {
    let total_frames = frame_count(duration, frame_rate)?;
    let clamp = |time: Time| -> Result<Time, ExportError> {
        if time
            .cmp_exact(Time::ZERO)
            .map_err(ExportError::Time)?
            .is_lt()
        {
            return Ok(Time::ZERO);
        }
        if time.cmp_exact(duration).map_err(ExportError::Time)?.is_gt() {
            return Ok(duration);
        }
        Ok(time)
    };

    let start = clamp(range.start)?;
    let end = clamp(range.end.unwrap_or(duration))?;

    let start_frame = frame_floor(start, frame_rate)?.min(total_frames as i64);
    let snapped_start = Time::frames(start_frame, frame_rate).map_err(ExportError::Time)?;
    let span = end.checked_sub(snapped_start).map_err(ExportError::Time)?;
    if span
        .cmp_exact(Time::ZERO)
        .map_err(ExportError::Time)?
        .is_le()
    {
        return Err(ExportError::EmptyRange);
    }
    let frames = frame_count(span, frame_rate)?.min(total_frames - start_frame as u64);
    if frames == 0 {
        return Err(ExportError::EmptyRange);
    }

    Ok(ExportWindow {
        start: snapped_start,
        start_frame,
        frames,
        duration: Time::frames(frames as i64, frame_rate).map_err(ExportError::Time)?,
    })
}

/// Clones `graph`, shifting every clip's `range.start` earlier by `offset` so
/// a mix over `[0, window]` renders the composition span `[offset, offset +
/// window]`. Clips that began before the window get a negative `range.start`;
/// the mixer's `local_time = project_time - clip.range.start` then keeps
/// advancing them from the correct source position.
fn shifted_audio_graph(graph: &AudioGraph, offset: Time) -> Result<AudioGraph, TimeError> {
    let mut shifted = graph.clone();
    for clip in &mut shifted.clips {
        clip.range.start = clip.range.start.checked_sub(offset)?;
    }
    Ok(shifted)
}

/// Locates the `@celesta/react` Node.js runtime used to evaluate a React entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReactRuntimeOptions {
    pub node: PathBuf,
    pub cli_script: PathBuf,
}

impl ReactRuntimeOptions {
    pub fn new(node: impl Into<PathBuf>, cli_script: impl Into<PathBuf>) -> Self {
        Self {
            node: node.into(),
            cli_script: cli_script.into(),
        }
    }
}

/// A GUI-editor-owned project a React export evaluates alongside its entry,
/// for the entry's `<ProjectTimeline />` to embed.
#[derive(Clone, Copy, Debug)]
pub struct CompanionProject<'a> {
    pub project: &'a Project,
    pub project_asset_root: &'a Path,
}

struct ReactVideoRequest<'a> {
    bridge: &'a mut ReactBridge,
    metadata: &'a ReactCompositionMetadata,
    /// First composition frame to render (0 unless an export range is set).
    start_frame: u64,
    /// Number of frames to render (the whole composition unless a range is set).
    frame_count: u64,
    asset_root: &'a Path,
    project: Option<(&'a Project, &'a Path)>,
    /// Every frame's reported `<Audio>` clips accumulate here (one entry per
    /// clip per frame; duplicates are merged afterwards — see
    /// `merge_react_audio_clips`).
    audio: &'a mut Vec<ReactAudioClipDescriptor>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportProgress {
    Rendering { frame: u64, total: u64 },
    MixingAudio,
    Muxing,
}

#[derive(Clone, Debug, Default)]
pub struct ExportCancellation {
    cancelled: Arc<AtomicBool>,
}

impl ExportCancellation {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

pub struct Exporter {
    options: ExportOptions,
}

impl Exporter {
    pub const fn new(options: ExportOptions) -> Self {
        Self { options }
    }

    pub fn export_file(
        &self,
        project_path: impl AsRef<Path>,
        output_path: impl AsRef<Path>,
    ) -> Result<(), ExportError> {
        self.export_file_with_progress(project_path, output_path, |_| {})
    }

    pub fn export_file_with_progress(
        &self,
        project_path: impl AsRef<Path>,
        output_path: impl AsRef<Path>,
        progress: impl FnMut(ExportProgress),
    ) -> Result<(), ExportError> {
        let project_path = project_path.as_ref();
        let project = Project::load(project_path).map_err(ExportError::Project)?;
        let asset_root = project_path.parent().unwrap_or_else(|| Path::new("."));
        self.export_project_with_progress(&project, asset_root, output_path, progress)
    }

    pub fn export_project(
        &self,
        project: &Project,
        asset_root: impl AsRef<Path>,
        output_path: impl AsRef<Path>,
    ) -> Result<(), ExportError> {
        self.export_project_with_progress(project, asset_root, output_path, |_| {})
    }

    pub fn export_project_with_progress(
        &self,
        project: &Project,
        asset_root: impl AsRef<Path>,
        output_path: impl AsRef<Path>,
        progress: impl FnMut(ExportProgress),
    ) -> Result<(), ExportError> {
        self.export_project_cancellable(
            project,
            asset_root,
            output_path,
            &ExportCancellation::default(),
            progress,
        )
    }

    pub fn export_project_cancellable(
        &self,
        project: &Project,
        asset_root: impl AsRef<Path>,
        output_path: impl AsRef<Path>,
        cancellation: &ExportCancellation,
        mut progress: impl FnMut(ExportProgress),
    ) -> Result<(), ExportError> {
        let asset_root = asset_root.as_ref();
        let output_path = output_path.as_ref();
        ensure_not_cancelled(cancellation)?;
        validate_output(output_path, self.options.overwrite)?;
        validate_dimensions(project.settings.width, project.settings.height)?;

        let duration = project.effective_duration().map_err(ExportError::Time)?;
        let full_frame_count = frame_count(duration, project.settings.frame_rate)?;
        if full_frame_count == 0 {
            return Err(ExportError::EmptyTimeline);
        }
        // With no range this is the whole composition; the audio mixdown then
        // keeps covering the exact `effective_duration` (not its frame-snapped
        // rounding), so that path stays byte-for-byte unchanged.
        let window = match self.options.range {
            Some(range) => resolve_window(&range, duration, project.settings.frame_rate)?,
            None => ExportWindow {
                start: Time::ZERO,
                start_frame: 0,
                frames: full_frame_count,
                duration,
            },
        };

        let parent = output_path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|source| ExportError::Io {
            operation: "create output directory",
            source,
        })?;
        let temporary = tempfile::Builder::new()
            .prefix(".celesta-export-")
            .tempdir_in(parent)
            .map_err(|source| ExportError::Io {
                operation: "create export workspace",
                source,
            })?;
        let video_path = temporary.path().join("video.mp4");
        let final_file = tempfile::Builder::new()
            .prefix(".celesta-export-")
            .suffix(".mp4")
            .tempfile_in(parent)
            .map_err(|source| ExportError::Io {
                operation: "create temporary output",
                source,
            })?;

        let evaluator = Evaluator::new(project).map_err(ExportError::Evaluation)?;
        let graph = evaluator.audio_graph().map_err(ExportError::Evaluation)?;
        let (graph, audio_duration) = if self.options.range.is_some() {
            (
                shifted_audio_graph(&graph, window.start).map_err(ExportError::Time)?,
                window.duration,
            )
        } else {
            (graph, duration)
        };

        // The audio mixdown does not depend on the rendered video, so it runs
        // on its own thread while the frames render instead of after them.
        // `abandon_audio` stops it early when rendering fails.
        let abandon_audio = AtomicBool::new(false);
        let (video, audio) = thread::scope(|scope| {
            let audio = scope.spawn(|| {
                let mut audio_decoder = FfmpegBackend::new();
                mix_audio_graph_cancellable(
                    &graph,
                    asset_root,
                    audio_duration,
                    &mut audio_decoder,
                    || cancellation.is_cancelled() || abandon_audio.load(Ordering::Acquire),
                )
            });
            let video = self.render_video(
                project,
                asset_root,
                window,
                &video_path,
                cancellation,
                &mut progress,
            );
            if video.is_err() {
                abandon_audio.store(true, Ordering::Release);
            }
            (video, audio.join())
        });
        video?;
        ensure_not_cancelled(cancellation)?;
        progress(ExportProgress::MixingAudio);
        let audio = audio
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
            .map_err(|error| match error {
                AudioMixError::Cancelled => ExportError::Cancelled,
                error => ExportError::Audio(error),
            })?;

        ensure_not_cancelled(cancellation)?;
        progress(ExportProgress::Muxing);
        self.mux_audio(&video_path, final_file.path(), &audio, cancellation)?;
        ensure_not_cancelled(cancellation)?;
        if self.options.overwrite {
            final_file.persist(output_path)
        } else {
            final_file.persist_noclobber(output_path)
        }
        .map_err(|error| {
            if error.error.kind() == io::ErrorKind::AlreadyExists {
                ExportError::OutputExists(output_path.to_owned())
            } else {
                ExportError::Io {
                    operation: "publish exported video",
                    source: error.error,
                }
            }
        })?;
        Ok(())
    }

    /// Exports a React composition entry to MP4. When the composition (and
    /// any companion project) declares no audio, the encoded video is
    /// published directly instead of going through a separate mux stage.
    pub fn export_react_entry(
        &self,
        entry: impl AsRef<Path>,
        react_runtime: &ReactRuntimeOptions,
        output_path: impl AsRef<Path>,
    ) -> Result<(), ExportError> {
        self.export_react_entry_with_progress(entry, react_runtime, output_path, |_| {})
    }

    pub fn export_react_entry_with_progress(
        &self,
        entry: impl AsRef<Path>,
        react_runtime: &ReactRuntimeOptions,
        output_path: impl AsRef<Path>,
        progress: impl FnMut(ExportProgress),
    ) -> Result<(), ExportError> {
        self.export_react_entry_cancellable(
            entry,
            react_runtime,
            output_path,
            &ExportCancellation::default(),
            progress,
        )
    }

    pub fn export_react_entry_cancellable(
        &self,
        entry: impl AsRef<Path>,
        react_runtime: &ReactRuntimeOptions,
        output_path: impl AsRef<Path>,
        cancellation: &ExportCancellation,
        progress: impl FnMut(ExportProgress),
    ) -> Result<(), ExportError> {
        self.export_react_entry_impl(
            entry,
            react_runtime,
            None,
            output_path,
            cancellation,
            progress,
        )
    }

    /// Same as [`Self::export_react_entry`], but also evaluates a companion
    /// project once per frame and gives the entry's `<ProjectTimeline />`
    /// the resulting layers. `companion.project_asset_root` resolves the
    /// project's own relative asset paths and may differ from the entry's
    /// directory; every timeline content kind but `audio` is evaluated (see
    /// `visual_only_project`).
    pub fn export_react_entry_with_project(
        &self,
        entry: impl AsRef<Path>,
        react_runtime: &ReactRuntimeOptions,
        companion: CompanionProject<'_>,
        output_path: impl AsRef<Path>,
    ) -> Result<(), ExportError> {
        self.export_react_entry_with_project_and_progress(
            entry,
            react_runtime,
            companion,
            output_path,
            |_| {},
        )
    }

    pub fn export_react_entry_with_project_and_progress(
        &self,
        entry: impl AsRef<Path>,
        react_runtime: &ReactRuntimeOptions,
        companion: CompanionProject<'_>,
        output_path: impl AsRef<Path>,
        progress: impl FnMut(ExportProgress),
    ) -> Result<(), ExportError> {
        self.export_react_entry_with_project_cancellable(
            entry,
            react_runtime,
            companion,
            output_path,
            &ExportCancellation::default(),
            progress,
        )
    }

    pub fn export_react_entry_with_project_cancellable(
        &self,
        entry: impl AsRef<Path>,
        react_runtime: &ReactRuntimeOptions,
        companion: CompanionProject<'_>,
        output_path: impl AsRef<Path>,
        cancellation: &ExportCancellation,
        progress: impl FnMut(ExportProgress),
    ) -> Result<(), ExportError> {
        self.export_react_entry_impl(
            entry,
            react_runtime,
            Some((companion.project, companion.project_asset_root)),
            output_path,
            cancellation,
            progress,
        )
    }

    fn export_react_entry_impl(
        &self,
        entry: impl AsRef<Path>,
        react_runtime: &ReactRuntimeOptions,
        project: Option<(&Project, &Path)>,
        output_path: impl AsRef<Path>,
        cancellation: &ExportCancellation,
        mut progress: impl FnMut(ExportProgress),
    ) -> Result<(), ExportError> {
        let entry = entry.as_ref();
        let output_path = output_path.as_ref();
        ensure_not_cancelled(cancellation)?;
        validate_output(output_path, self.options.overwrite)?;

        // Absolutize the asset roots up front: `absolutize_layers` and
        // `build_audio_graph` rewrite relative asset paths into absolute ones
        // by joining them with these roots, and a relative root (an entry
        // path passed relative to the current directory) would produce a
        // still-relative "absolute" path that the audio mixer then joins
        // again.
        let asset_root = entry.parent().unwrap_or_else(|| Path::new("."));
        let asset_root = &fs::canonicalize(asset_root).unwrap_or_else(|_| asset_root.to_owned());
        let project = project.map(|(project, project_asset_root)| {
            let project_asset_root = fs::canonicalize(project_asset_root)
                .unwrap_or_else(|_| project_asset_root.to_owned());
            (project, project_asset_root)
        });
        let parent = output_path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|source| ExportError::Io {
            operation: "create output directory",
            source,
        })?;

        let mut bridge = ReactBridge::spawn(&react_runtime.node, &react_runtime.cli_script, entry)
            .map_err(ExportError::React)?;
        let metadata = bridge.metadata().clone();
        validate_dimensions(metadata.width, metadata.height)?;
        if metadata.duration_in_frames == 0 {
            return Err(ExportError::EmptyTimeline);
        }

        let (start_frame, frame_count, window_start) = match self.options.range {
            None => (0, metadata.duration_in_frames, Time::ZERO),
            Some(range) => {
                let full = Time::frames(
                    i64::try_from(metadata.duration_in_frames)
                        .map_err(|_| ExportError::TimelineTooLong)?,
                    metadata.frame_rate,
                )
                .map_err(ExportError::Time)?;
                let window = resolve_window(&range, full, metadata.frame_rate)?;
                let start_frame =
                    u64::try_from(window.start_frame).map_err(|_| ExportError::TimelineTooLong)?;
                (start_frame, window.frames, window.start)
            }
        };

        // Video frames stream into a staged file first; every frame's
        // response reports the `<Audio>` clips its tree declares (see
        // ReactAudioClipDescriptor), which accumulate into the export's
        // AudioGraph afterwards. An empty result (no `<Audio>` anywhere it
        // renders, no companion project audio) keeps the silent-export
        // behavior: the staged video is published directly, no mix/mux
        // stage.
        let final_file = tempfile::Builder::new()
            .prefix(".celesta-export-")
            .suffix(".mp4")
            .tempfile_in(parent)
            .map_err(|source| ExportError::Io {
                operation: "create temporary output",
                source,
            })?;
        let mut react_audio = Vec::new();
        let project_refs = project
            .as_ref()
            .map(|(project, root)| (*project, root.as_path()));
        self.render_react_video(
            ReactVideoRequest {
                bridge: &mut bridge,
                metadata: &metadata,
                start_frame,
                frame_count,
                asset_root,
                project: project_refs,
                audio: &mut react_audio,
            },
            final_file.path(),
            cancellation,
            &mut progress,
        )?;
        ensure_not_cancelled(cancellation)?;

        let graph = build_audio_graph(&react_audio, asset_root, project_refs)?;
        let output_file = if graph.clips.is_empty() {
            final_file
        } else {
            progress(ExportProgress::MixingAudio);
            let duration = Time::frames(
                i64::try_from(frame_count).map_err(|_| ExportError::TimelineTooLong)?,
                metadata.frame_rate,
            )
            .map_err(ExportError::Time)?;
            let graph = shifted_audio_graph(&graph, window_start).map_err(ExportError::Time)?;
            let mut audio_decoder = FfmpegBackend::new();
            let audio = mix_audio_graph_cancellable(
                &graph,
                asset_root,
                duration,
                &mut audio_decoder,
                || cancellation.is_cancelled(),
            )
            .map_err(|error| match error {
                AudioMixError::Cancelled => ExportError::Cancelled,
                error => ExportError::Audio(error),
            })?;
            ensure_not_cancelled(cancellation)?;
            progress(ExportProgress::Muxing);
            let muxed_file = tempfile::Builder::new()
                .prefix(".celesta-export-")
                .suffix(".mp4")
                .tempfile_in(parent)
                .map_err(|source| ExportError::Io {
                    operation: "create temporary output",
                    source,
                })?;
            self.mux_audio(final_file.path(), muxed_file.path(), &audio, cancellation)?;
            muxed_file
        };

        ensure_not_cancelled(cancellation)?;
        if self.options.overwrite {
            output_file.persist(output_path)
        } else {
            output_file.persist_noclobber(output_path)
        }
        .map_err(|error| {
            if error.error.kind() == io::ErrorKind::AlreadyExists {
                ExportError::OutputExists(output_path.to_owned())
            } else {
                ExportError::Io {
                    operation: "publish exported video",
                    source: error.error,
                }
            }
        })?;
        Ok(())
    }

    fn render_video(
        &self,
        project: &Project,
        asset_root: &Path,
        window: ExportWindow,
        output: &Path,
        cancellation: &ExportCancellation,
        progress: &mut impl FnMut(ExportProgress),
    ) -> Result<(), ExportError> {
        let ExportWindow {
            start_frame,
            frames: frame_count,
            ..
        } = window;
        let frame_rate = project.settings.frame_rate;
        let mut renderer =
            export_renderer(asset_root, frame_rate, self.options.video.color_conversion)?;
        let mut writer = open_video_writer(
            project.settings.width,
            project.settings.height,
            frame_rate,
            self.options.video,
            renderer.readback_format(),
            output,
        )?;

        let result = (|| {
            let evaluator = Evaluator::new(project).map_err(ExportError::Evaluation)?;
            for frame_index in 0..frame_count {
                ensure_not_cancelled(cancellation)?;
                progress(ExportProgress::Rendering {
                    frame: frame_index + 1,
                    total: frame_count,
                });
                let frame_index = i64::try_from(frame_index)
                    .ok()
                    .and_then(|offset: i64| offset.checked_add(start_frame))
                    .ok_or(ExportError::TimelineTooLong)?;
                let time = Time::frames(frame_index, frame_rate).map_err(ExportError::Time)?;
                let scene = evaluator.scene_at(time).map_err(ExportError::Evaluation)?;
                // `submit` keeps a few frames in flight on the GPU rather
                // than blocking on this frame's readback immediately, so the
                // wait (when there is one) overlaps with evaluating and
                // encoding other frames instead of stalling every frame.
                if let Some(frame) = renderer.submit(&scene).map_err(ExportError::Render)? {
                    write_frame(&mut writer, frame)?;
                }
            }
            for frame in renderer.drain().map_err(ExportError::Render)? {
                write_frame(&mut writer, frame)?;
            }
            Ok(())
        })();
        finish_encode(writer, result)
    }

    fn render_react_video(
        &self,
        request: ReactVideoRequest<'_>,
        output: &Path,
        cancellation: &ExportCancellation,
        progress: &mut impl FnMut(ExportProgress),
    ) -> Result<(), ExportError> {
        let ReactVideoRequest {
            bridge,
            metadata,
            start_frame,
            frame_count,
            asset_root,
            project,
            audio: react_audio,
        } = request;

        let filtered_project = project.map(|(project, _)| visual_only_project(project));
        let project_evaluator = filtered_project
            .as_ref()
            .map(Evaluator::new)
            .transpose()
            .map_err(ExportError::Evaluation)?;
        let project_asset_root = project.map(|(_, asset_root)| asset_root);
        let project_fonts = project_evaluator
            .as_ref()
            .map(|evaluator| -> Result<_, EvaluationError> {
                let mut fonts = evaluator.scene_at(Time::ZERO)?.fonts;
                if let Some(asset_root) = project_asset_root {
                    absolutize_fonts(&mut fonts, asset_root);
                }
                Ok(fonts)
            })
            .transpose()
            .map_err(ExportError::Evaluation)?
            .unwrap_or_default();

        // The video decoder is attached unconditionally: the React entry's
        // own <Video> elements need decoding just as much as a companion
        // project's Video content does, and the React entry's asset_root
        // stays the renderer's single asset_root either way (see
        // absolutize_layers/absolutize_fonts below for how a project's own,
        // differently-rooted assets still resolve).
        let mut renderer = export_renderer(
            asset_root,
            metadata.frame_rate,
            self.options.video.color_conversion,
        )?;
        let mut writer = open_video_writer(
            metadata.width,
            metadata.height,
            metadata.frame_rate,
            self.options.video,
            renderer.readback_format(),
            output,
        )?;

        let result = (|| {
            for offset in 0..frame_count {
                ensure_not_cancelled(cancellation)?;
                progress(ExportProgress::Rendering {
                    frame: offset + 1,
                    total: frame_count,
                });
                let frame_index = i64::try_from(start_frame + offset)
                    .map_err(|_| ExportError::TimelineTooLong)?;
                let time =
                    Time::frames(frame_index, metadata.frame_rate).map_err(ExportError::Time)?;
                let project_frame = project_evaluator
                    .as_ref()
                    .zip(filtered_project.as_ref())
                    .map(
                        |(evaluator, filtered_project)| -> Result<_, EvaluationError> {
                            let mut scene = evaluator.scene_at(time)?;
                            let mut tracks = BTreeMap::new();
                            for track in &filtered_project.tracks {
                                let mut layers = evaluator.layers_for_track(&track.id, time)?;
                                if let Some(asset_root) = project_asset_root {
                                    absolutize_layers(&mut layers, asset_root);
                                }
                                tracks.insert(track.id.clone(), layers);
                            }
                            if let Some(asset_root) = project_asset_root {
                                absolutize_layers(&mut scene.layers, asset_root);
                            }
                            Ok((scene.layers, tracks))
                        },
                    )
                    .transpose()
                    .map_err(ExportError::Evaluation)?;
                let evaluation = bridge
                    .evaluate_at(
                        time,
                        project_frame.as_ref().map(|(layers, tracks)| ProjectFrame {
                            layers: layers.as_slice(),
                            tracks,
                        }),
                    )
                    .map_err(ExportError::React)?;
                react_audio.extend(evaluation.audio);
                let mut scene = evaluation.scene;
                scene.fonts.extend(project_fonts.iter().cloned());
                // See render_video's matching comment: submit overlaps this
                // frame's GPU work with the *next* frame's Node IPC round
                // trip and project evaluation instead of blocking here.
                if let Some(frame) = renderer.submit(&scene).map_err(ExportError::Render)? {
                    write_frame(&mut writer, frame)?;
                }
            }
            for frame in renderer.drain().map_err(ExportError::Render)? {
                write_frame(&mut writer, frame)?;
            }
            Ok(())
        })();
        finish_encode(writer, result)
    }

    /// Muxes the encoded, audio-less `video` with the mixed `audio` (staged as
    /// a raw `f32le` PCM sidecar file) into `output`, stream-copying the video
    /// and encoding AAC — the library-linked equivalent of a second
    /// `ffmpeg -i video -f f32le -i pcm -c:v copy -c:a aac` pass.
    fn mux_audio(
        &self,
        video: &Path,
        output: &Path,
        audio: &celesta_media::AudioBuffer,
        cancellation: &ExportCancellation,
    ) -> Result<(), ExportError> {
        ensure_not_cancelled(cancellation)?;
        let pcm_path = video.with_extension("pcm");
        let mut pcm =
            io::BufWriter::new(
                fs::File::create(&pcm_path).map_err(|source| ExportError::Io {
                    operation: "create mixed audio sidecar",
                    source,
                })?,
            );
        for sample in &audio.samples {
            pcm.write_all(&sample.to_le_bytes())
                .map_err(|source| ExportError::Io {
                    operation: "stage mixed audio",
                    source,
                })?;
        }
        pcm.into_inner()
            .map_err(|error| ExportError::Io {
                operation: "flush mixed audio",
                source: error.into_error(),
            })?
            .sync_all()
            .map_err(|source| ExportError::Io {
                operation: "flush mixed audio",
                source,
            })?;
        ensure_not_cancelled(cancellation)?;

        let context = FfmpegContext::builder()
            .input(Input::from(path_to_url(video)))
            .input(
                Input::from(path_to_url(&pcm_path))
                    .set_format("f32le")
                    .set_format_opt("sample_rate", audio.sample_rate.to_string())
                    .set_format_opt("ch_layout", format!("{}c", audio.channels)),
            )
            .output(
                Output::from(path_to_url(output))
                    .add_stream_map_with_copy("0:v:0")
                    .add_stream_map("1:a:0")
                    .set_audio_codec("aac")
                    .set_audio_codec_opt("b", "192k")
                    .set_shortest(true)
                    .set_format_opt("movflags", "+faststart"),
            )
            .build()
            .map_err(|source| ExportError::Ffmpeg {
                stage: "audio muxing",
                source,
            })?;
        let result = context
            .start()
            .and_then(|running| running.wait())
            .map_err(|source| ExportError::Ffmpeg {
                stage: "audio muxing",
                source,
            });
        let _ = fs::remove_file(&pcm_path);
        result
    }
}

impl Default for Exporter {
    fn default() -> Self {
        Self::new(ExportOptions::default())
    }
}

/// Drops `audio` timeline items before evaluating a companion project's
/// *visual* content for a React export. `Evaluator::visual_layer` already
/// evaluates an `audio` item to no layer (`TimelineContent::Audio { .. } =>
/// return Ok(None)`), so this filter is a cheap, explicit skip rather than a
/// behavior change; an `audio` item has nothing to contribute to
/// `<ProjectTimeline />`/`<ProjectTrack />` either way. This filtering is
/// specific to the visual path: the companion project's *audio* mixdown
/// (`build_audio_graph` below) deliberately uses the project unfiltered, via
/// `Evaluator::audio_graph()`, so its audio timeline items do play. Every
/// other visual kind reaches
/// `<ProjectTimeline />`/`<ProjectTrack />`: `component` items evaluate to
/// `LayerContent::MissingComponent`, which `@celesta/react` resolves against
/// its own `registerComponent()` registry (falling back to leaving
/// `missingComponent` layers as-is, which `GpuRenderer` then errors on);
/// `dialogue` items evaluate to a `LayerContent::Group` of the character's
/// portrait image and subtitle text (`Evaluator::dialogue`) — there is no
/// dedicated `LayerContent::Dialogue`, so it needs no special handling here
/// or on the TypeScript side, the same generic `Group`/`Image`/`Text`
/// rendering every other layer already gets.
fn visual_only_project(project: &Project) -> Project {
    let mut filtered = project.clone();
    for track in &mut filtered.tracks {
        track
            .items
            .retain(|item| !matches!(item.content, TimelineContent::Audio { .. }));
    }
    filtered
}

/// Builds the complete `AudioGraph` a React export mixes down: every
/// `<Audio>` clip the entry's frames reported (`react_audio`, accumulated by
/// `render_react_video` — one raw entry per clip per frame) plus, when a
/// companion project is given, that project's own complete, *unfiltered*
/// `Evaluator::audio_graph()` (unlike the visual path's
/// `visual_only_project()`, which drops `TimelineContent::Audio` items
/// because they contribute nothing visually — the audio mixdown needs them
/// to actually play). The companion project supplies the graph's
/// `sample_rate`/`master_volume` when present, since it already carries
/// authoritative values for those; otherwise `DEFAULT_REACT_AUDIO_SAMPLE_RATE`
/// is used.
fn build_audio_graph(
    react_audio: &[ReactAudioClipDescriptor],
    react_asset_root: &Path,
    project: Option<(&Project, &Path)>,
) -> Result<AudioGraph, ExportError> {
    let mut graph = match project {
        Some((project, project_asset_root)) => {
            let evaluator = Evaluator::new(project).map_err(ExportError::Evaluation)?;
            let mut graph = evaluator.audio_graph().map_err(ExportError::Evaluation)?;
            for clip in &mut graph.clips {
                absolutize_asset(&mut clip.asset, project_asset_root);
            }
            graph
        }
        None => AudioGraph {
            sample_rate: DEFAULT_REACT_AUDIO_SAMPLE_RATE,
            master_volume: 1.0,
            clips: Vec::new(),
        },
    };

    // The same declaration reports once per rendered frame; merge identical
    // reports (bit-identical because every field is recomputed from the same
    // inputs each frame) while preserving multiplicity — two distinct clips
    // with identical parameters at the same instant really do play twice.
    let merged = merge_react_audio_clips(react_audio);

    for (index, (clip, _)) in merged.iter().enumerate() {
        let mut asset = ResolvedAsset {
            id: clip.src.clone(),
            location: AssetLocation::File {
                path: clip.src.clone(),
            },
        };
        absolutize_asset(&mut asset, react_asset_root);
        graph.clips.push(AudioClip {
            id: format!("react-audio:{index}"),
            asset,
            range: TimeRange {
                start: seconds_to_time(clip.start),
                duration: seconds_to_time(clip.duration),
            },
            source_start: seconds_to_time(clip.source_start),
            source_duration: None,
            playback_rate: clip.playback_rate.clone(),
            volume: clip.volume.clone(),
            muted: clip.muted,
        });
    }

    Ok(graph)
}

/// Collapses per-frame audio reports into one entry per distinct clip,
/// keeping first-seen order and how many frames reported it (its
/// multiplicity — two identical clips playing at once must stay two clips).
fn merge_react_audio_clips(
    clips: &[ReactAudioClipDescriptor],
) -> Vec<(ReactAudioClipDescriptor, usize)> {
    let mut merged: Vec<(ReactAudioClipDescriptor, usize)> = Vec::new();
    for clip in clips {
        match merged.iter_mut().find(|(known, _)| known == clip) {
            Some((_, count)) => *count += 1,
            None => merged.push((clip.clone(), 1)),
        }
    }
    merged
}

fn seconds_to_time(seconds: f64) -> Time {
    Time::new(
        (seconds * f64::from(AUDIO_SECONDS_TIMESCALE)).round() as i64,
        AUDIO_SECONDS_TIMESCALE,
    )
}

/// Rewrites a project-evaluated layer tree's relative asset paths into
/// absolute ones. A React export's `GpuRenderer` has a single `asset_root`
/// (the entry's own directory); a companion project's assets may live
/// elsewhere, so its evaluated layers carry absolute paths instead of
/// relying on that shared root.
fn absolutize_layers(layers: &mut [Layer], asset_root: &Path) {
    for layer in layers {
        absolutize_layer_content(&mut layer.content, asset_root);
    }
}

fn absolutize_layer_content(content: &mut LayerContent, asset_root: &Path) {
    match content {
        LayerContent::Video { asset, .. }
        | LayerContent::Image { asset }
        | LayerContent::Psd { asset, .. } => {
            absolutize_asset(asset, asset_root);
        }
        LayerContent::Group { layers } => absolutize_layers(layers, asset_root),
        LayerContent::Text { .. }
        | LayerContent::Rect { .. }
        | LayerContent::MissingComponent { .. } => {}
    }
}

fn absolutize_fonts(fonts: &mut [ResolvedAsset], asset_root: &Path) {
    for font in fonts {
        absolutize_asset(font, asset_root);
    }
}

fn absolutize_asset(asset: &mut ResolvedAsset, asset_root: &Path) {
    if let AssetLocation::File { path } = &mut asset.location {
        let candidate = Path::new(path.as_str());
        if candidate.is_relative() {
            *path = asset_root.join(candidate).to_string_lossy().into_owned();
        }
    }
}

fn validate_output(output: &Path, overwrite: bool) -> Result<(), ExportError> {
    if output.extension() != Some(OsStr::new("mp4")) {
        return Err(ExportError::UnsupportedOutput(output.to_owned()));
    }
    if !overwrite && output.exists() {
        return Err(ExportError::OutputExists(output.to_owned()));
    }
    Ok(())
}

fn path_to_url(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// The renderer an export draws its frames with, reading them back as
/// yuv420p when `color_conversion` puts that conversion on the GPU.
fn export_renderer(
    asset_root: &Path,
    frame_rate: Rational,
    color_conversion: ColorConversion,
) -> Result<GpuRenderer, ExportError> {
    let mut renderer = GpuRenderer::new(GpuRenderOptions::default())
        .map_err(ExportError::Render)?
        .with_asset_root(asset_root)
        .with_video_decoder(FfmpegBackend::new().with_sequential_video(frame_rate));
    let on_gpu = match color_conversion {
        ColorConversion::Auto => renderer.supports_yuv420p_readback() && !renderer.is_software(),
        ColorConversion::Gpu => true,
        ColorConversion::Encoder => false,
    };
    if on_gpu {
        renderer
            .set_readback_format(ReadbackFormat::Yuv420p)
            .map_err(ExportError::Render)?;
    }
    Ok(renderer)
}

/// Opens a constant-frame-rate H.264 `VideoWriter` for pushed frames in
/// `input` layout — the library-linked equivalent of piping `rawvideo` into
/// `ffmpeg -c:v libx264 -preset <preset> -crf <crf> -pix_fmt yuv420p -movflags +faststart`
/// (`-preset medium -crf 18` by default). yuv420p input reaches the encoder
/// without a conversion.
fn open_video_writer(
    width: u32,
    height: u32,
    frame_rate: Rational,
    encoding: VideoEncoding,
    input: ReadbackFormat,
    output: &Path,
) -> Result<VideoWriter, ExportError> {
    if encoding.crf > VideoEncoding::MAX_CRF {
        return Err(ExportError::InvalidCrf(encoding.crf));
    }
    let fps_num = i32::try_from(frame_rate.numerator).map_err(|_| ExportError::TimelineTooLong)?;
    let fps_den =
        i32::try_from(frame_rate.denominator).map_err(|_| ExportError::TimelineTooLong)?;
    VideoWriter::builder(width, height)
        .pixel_format(match input {
            ReadbackFormat::Rgba8 => "rgba",
            ReadbackFormat::Yuv420p => "yuv420p",
        })
        .fps(fps_num, fps_den)
        .open(
            Output::from(path_to_url(output))
                .set_video_codec("libx264")
                .set_video_codec_opt("preset", encoding.preset.as_str())
                .set_video_codec_opt("crf", encoding.crf.to_string())
                .set_pix_fmt("yuv420p")
                .set_format_opt("movflags", "+faststart"),
        )
        .map_err(|source| ExportError::Ffmpeg {
            stage: "video encoding",
            source,
        })
}

/// Hands the frame's pixel buffer to the encoder as is (`write_owned`), rather
/// than having `write` copy all of it first.
fn write_frame(
    writer: &mut VideoWriter,
    frame: celesta_gpu_renderer::GpuFrame,
) -> Result<(), ExportError> {
    writer
        .write_owned(frame.into_pixels())
        .map_err(|error| ExportError::Ffmpeg {
            stage: "video encoding",
            source: error.into_parts().1.into(),
        })
}

/// Finalizes an encode: `finish()` on success (writes the container trailer),
/// `abort()` on any earlier failure (dropping the writer would also abort, but
/// this is explicit and drains the worker).
fn finish_encode(writer: VideoWriter, result: Result<(), ExportError>) -> Result<(), ExportError> {
    match result {
        Ok(()) => writer.finish().map_err(|source| ExportError::Ffmpeg {
            stage: "video encoding",
            source,
        }),
        Err(error) => {
            writer.abort();
            Err(error)
        }
    }
}

fn ensure_not_cancelled(cancellation: &ExportCancellation) -> Result<(), ExportError> {
    if cancellation.is_cancelled() {
        return Err(ExportError::Cancelled);
    }
    Ok(())
}

fn validate_dimensions(width: u32, height: u32) -> Result<(), ExportError> {
    if width == 0 || height == 0 || !width.is_multiple_of(2) || !height.is_multiple_of(2) {
        return Err(ExportError::UnsupportedDimensions { width, height });
    }
    Ok(())
}

/// Largest frame index whose start time is `<= time` (i.e. `time` snapped
/// down to a frame boundary). Negative times snap to frame 0.
fn frame_floor(time: Time, frame_rate: Rational) -> Result<i64, ExportError> {
    if !time.is_valid() {
        return Err(ExportError::Time(TimeError::ZeroTimescale));
    }
    if !frame_rate.is_valid() {
        return Err(ExportError::Time(TimeError::InvalidFrameRate));
    }
    let numerator = i128::from(time.value.max(0)) * i128::from(frame_rate.numerator);
    let denominator = i128::from(time.timescale) * i128::from(frame_rate.denominator);
    i64::try_from(numerator / denominator).map_err(|_| ExportError::TimelineTooLong)
}

fn frame_count(duration: Time, frame_rate: Rational) -> Result<u64, ExportError> {
    if !duration.is_valid() {
        return Err(ExportError::Time(TimeError::ZeroTimescale));
    }
    if !frame_rate.is_valid() {
        return Err(ExportError::Time(TimeError::InvalidFrameRate));
    }
    let numerator = i128::from(duration.value.max(0)) * i128::from(frame_rate.numerator);
    let denominator = i128::from(duration.timescale) * i128::from(frame_rate.denominator);
    let frames = numerator
        .checked_add(denominator - 1)
        .map(|value| value / denominator)
        .and_then(|value| u64::try_from(value).ok())
        .ok_or(ExportError::TimelineTooLong)?;
    Ok(frames)
}

#[derive(Debug)]
pub enum ExportError {
    Project(LoadError),
    Evaluation(EvaluationError),
    Render(GpuRenderError),
    Audio(AudioMixError),
    React(ReactBridgeError),
    Time(TimeError),
    Io {
        operation: &'static str,
        source: io::Error,
    },
    Ffmpeg {
        stage: &'static str,
        source: ez_ffmpeg::error::Error,
    },
    OutputExists(PathBuf),
    UnsupportedOutput(PathBuf),
    UnsupportedDimensions {
        width: u32,
        height: u32,
    },
    /// [`VideoEncoding::crf`] is above [`VideoEncoding::MAX_CRF`].
    InvalidCrf(u8),
    EmptyTimeline,
    /// The requested export range, once clamped to the composition and
    /// snapped to frames, covers zero frames.
    EmptyRange,
    Cancelled,
    TimelineTooLong,
}

impl fmt::Display for ExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Project(error) => write!(formatter, "could not load export project: {error}"),
            Self::Evaluation(error) => write!(formatter, "could not evaluate export: {error}"),
            Self::Render(error) => write!(formatter, "could not render export: {error}"),
            Self::Audio(error) => write!(formatter, "could not mix export audio: {error}"),
            Self::React(error) => write!(formatter, "could not evaluate React export: {error}"),
            Self::Time(error) => write!(formatter, "could not calculate export time: {error}"),
            Self::Io { operation, source } => write!(formatter, "could not {operation}: {source}"),
            Self::Ffmpeg { stage, source } => {
                write!(formatter, "FFmpeg {stage} failed: {source}")
            }
            Self::OutputExists(path) => {
                write!(formatter, "output already exists: {}", path.display())
            }
            Self::UnsupportedOutput(path) => write!(
                formatter,
                "export output must use the .mp4 extension: {}",
                path.display()
            ),
            Self::UnsupportedDimensions { width, height } => write!(
                formatter,
                "H.264 MP4 export requires non-zero even dimensions, got {width}x{height}"
            ),
            Self::InvalidCrf(crf) => write!(
                formatter,
                "H.264 CRF must be between 0 and {}, got {crf}",
                VideoEncoding::MAX_CRF
            ),
            Self::EmptyTimeline => formatter.write_str("cannot export an empty timeline"),
            Self::EmptyRange => {
                formatter.write_str("the requested export range does not cover any frames")
            }
            Self::Cancelled => formatter.write_str("export was cancelled"),
            Self::TimelineTooLong => formatter.write_str("export timeline is too long"),
        }
    }
}

impl Error for ExportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Project(error) => Some(error),
            Self::Evaluation(error) => Some(error),
            Self::Render(error) => Some(error),
            Self::Audio(error) => Some(error),
            Self::React(error) => Some(error),
            Self::Time(error) => Some(error),
            Self::Io { source, .. } => Some(source),
            Self::Ffmpeg { source, .. } => Some(source),
            Self::OutputExists(_)
            | Self::UnsupportedOutput(_)
            | Self::UnsupportedDimensions { .. }
            | Self::InvalidCrf(_)
            | Self::EmptyTimeline
            | Self::EmptyRange
            | Self::Cancelled
            | Self::TimelineTooLong => None,
        }
    }
}

/// Parses a `HH:MM:SS(.mmm)` / `MM:SS(.mmm)` / `SS(.mmm)` timecode into an
/// exact [`Time`] (millisecond timescale). Every component must be a
/// non-negative integer; the fractional part is at most three digits.
pub fn parse_timecode(input: &str) -> Result<Time, TimecodeError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(TimecodeError::Empty);
    }
    let malformed = || TimecodeError::Malformed(trimmed.to_owned());

    let components: Vec<&str> = trimmed.split(':').collect();
    if components.len() > 3 {
        return Err(malformed());
    }
    let (seconds_component, leading) = components.split_last().expect("split is never empty");

    let (whole_seconds, millis) = match seconds_component.split_once('.') {
        Some((whole, fraction)) => {
            if fraction.is_empty() || fraction.len() > 3 || !is_ascii_digits(fraction) {
                return Err(malformed());
            }
            let mut padded = fraction.to_owned();
            while padded.len() < 3 {
                padded.push('0');
            }
            (whole, padded.parse::<i64>().map_err(|_| malformed())?)
        }
        None => (*seconds_component, 0),
    };

    let mut total_ms = parse_component(whole_seconds, &malformed)?
        .checked_mul(1_000)
        .and_then(|value| value.checked_add(millis))
        .ok_or_else(|| TimecodeError::Overflow(trimmed.to_owned()))?;

    let mut unit_ms: i64 = 60_000;
    for component in leading.iter().rev() {
        let value = parse_component(component, &malformed)?;
        total_ms = value
            .checked_mul(unit_ms)
            .and_then(|scaled| total_ms.checked_add(scaled))
            .ok_or_else(|| TimecodeError::Overflow(trimmed.to_owned()))?;
        unit_ms = unit_ms
            .checked_mul(60)
            .ok_or_else(|| TimecodeError::Overflow(trimmed.to_owned()))?;
    }

    Ok(Time::new(total_ms, 1_000))
}

fn is_ascii_digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

fn parse_component(
    text: &str,
    malformed: &impl Fn() -> TimecodeError,
) -> Result<i64, TimecodeError> {
    if !is_ascii_digits(text) {
        return Err(malformed());
    }
    text.parse::<i64>().map_err(|_| malformed())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TimecodeError {
    Empty,
    Malformed(String),
    Overflow(String),
}

impl fmt::Display for TimecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("timecode is empty"),
            Self::Malformed(input) => write!(
                formatter,
                "'{input}' is not a HH:MM:SS(.mmm), MM:SS(.mmm) or SS(.mmm) timecode"
            ),
            Self::Overflow(input) => write!(formatter, "timecode '{input}' is out of range"),
        }
    }
}

impl Error for TimecodeError {}

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_composition::{Animatable, Keyframe, KeyframeAnimation, KeyframeAnimationType};

    fn static_clip(src: &str, start: f64, duration: f64, volume: f64) -> ReactAudioClipDescriptor {
        ReactAudioClipDescriptor {
            src: src.to_owned(),
            source_start: 0.0,
            playback_rate: Animatable::Static(1.0),
            volume: Animatable::Static(volume),
            muted: false,
            start,
            duration,
        }
    }

    #[test]
    fn merges_identical_frame_reports_while_preserving_multiplicity() {
        let clips = vec![
            static_clip("./a.wav", 0.0, 5.0, 1.0),
            static_clip("./b.wav", 1.0, 2.0, 0.5),
            static_clip("./a.wav", 0.0, 5.0, 1.0),
            static_clip("./a.wav", 1.0, 4.0, 1.0),
        ];

        let merged = merge_react_audio_clips(&clips);
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].1, 2, "the same clip reported twice merges");
        assert_eq!(merged[1].1, 1);
        assert_eq!(merged[2].1, 1);
        // A different window or volume is a different clip even for the
        // same source file.
        assert_eq!(merged[0].0.src, "./a.wav");
        assert_eq!(merged[1].0.volume, Animatable::Static(0.5));
        assert_eq!(merged[2].0.start, 1.0);
    }

    #[test]
    fn builds_a_react_audio_graph_with_ranges_and_keyframed_volume() {
        let clips = vec![ReactAudioClipDescriptor {
            src: "./voice.wav".to_owned(),
            source_start: 1.5,
            playback_rate: Animatable::Static(2.0),
            volume: Animatable::Keyframes(KeyframeAnimation {
                kind: KeyframeAnimationType::Keyframes,
                keyframes: vec![Keyframe {
                    time: Time::new(500_000, 1_000_000),
                    value: 0.25,
                    easing: None,
                }],
            }),
            muted: false,
            start: 1.0,
            duration: 2.0,
        }];

        let graph = build_audio_graph(&clips, Path::new("/entry/root"), None).unwrap();
        assert_eq!(graph.sample_rate, DEFAULT_REACT_AUDIO_SAMPLE_RATE);
        assert_eq!(graph.clips.len(), 1);
        let clip = &graph.clips[0];
        assert_eq!(
            clip.range.start.as_seconds().unwrap(),
            1.0,
            "range.start is the composition-space audible start"
        );
        assert_eq!(clip.range.duration.as_seconds().unwrap(), 2.0);
        assert_eq!(
            clip.source_start.as_seconds().unwrap(),
            1.5,
            "source_start carries the head-clipping adjustment"
        );
        assert_eq!(clip.playback_rate, Animatable::Static(2.0));
        assert!(matches!(&clip.volume, Animatable::Keyframes(_)));
        let AssetLocation::File { path } = &clip.asset.location else {
            panic!("expected a file asset");
        };
        assert_eq!(
            path,
            Path::new("/entry/root/./voice.wav"),
            "relative sources resolve against the entry's own directory"
        );
    }

    #[test]
    fn parses_timecodes_of_every_length() {
        assert_eq!(parse_timecode("5").unwrap(), Time::new(5_000, 1_000));
        assert_eq!(parse_timecode("1.5").unwrap(), Time::new(1_500, 1_000));
        assert_eq!(parse_timecode("0:02").unwrap(), Time::new(2_000, 1_000));
        assert_eq!(parse_timecode("01:30").unwrap(), Time::new(90_000, 1_000));
        assert_eq!(
            parse_timecode("01:02:03.250").unwrap(),
            Time::new(3_723_250, 1_000)
        );
        assert_eq!(
            parse_timecode(" 00:00:01 ").unwrap(),
            Time::new(1_000, 1_000)
        );
        assert_eq!(parse_timecode(""), Err(TimecodeError::Empty));
        assert!(matches!(
            parse_timecode("1:2:3:4"),
            Err(TimecodeError::Malformed(_))
        ));
        assert!(matches!(
            parse_timecode("-5"),
            Err(TimecodeError::Malformed(_))
        ));
        assert!(matches!(
            parse_timecode("00:00:01.2500"),
            Err(TimecodeError::Malformed(_))
        ));
    }

    #[test]
    fn encoder_presets_and_color_conversions_round_trip_through_their_names() {
        for preset in EncoderPreset::ALL {
            assert_eq!(preset.as_str().parse::<EncoderPreset>(), Ok(preset));
        }
        assert_eq!(VideoEncoding::default().preset, EncoderPreset::Medium);
        assert_eq!(VideoEncoding::default().crf, 18);
        for conversion in ColorConversion::ALL {
            assert_eq!(
                conversion.as_str().parse::<ColorConversion>(),
                Ok(conversion)
            );
        }
        assert_eq!(
            VideoEncoding::default().color_conversion,
            ColorConversion::Auto
        );
        assert_eq!(
            "Medium".parse::<EncoderPreset>(),
            Err(UnknownEncoderPreset("Medium".to_owned()))
        );
    }

    #[test]
    fn rejects_an_out_of_range_crf_before_encoding() {
        let directory = tempfile::tempdir().unwrap();
        let result = open_video_writer(
            64,
            64,
            Rational::new(30, 1),
            VideoEncoding {
                crf: VideoEncoding::MAX_CRF + 1,
                ..VideoEncoding::default()
            },
            ReadbackFormat::Rgba8,
            &directory.path().join("video.mp4"),
        );
        assert!(matches!(result, Err(ExportError::InvalidCrf(52))));
    }

    #[test]
    fn resolve_window_clamps_and_snaps_to_frames() {
        let rate = Rational::new(30, 1);
        let duration = Time::new(10, 1);

        // A mid-composition span, start snapped down to the frame boundary.
        let window = resolve_window(
            &ExportRange::new(Time::new(1001, 1000), Time::new(3, 1)),
            duration,
            rate,
        )
        .unwrap();
        assert_eq!(window.start_frame, 30);
        assert_eq!(window.start, Time::frames(30, rate).unwrap());
        assert_eq!(window.frames, 60);

        // `end` past the composition is clamped; `None` means "to the end".
        let full_tail =
            resolve_window(&ExportRange::from(Time::new(9, 1)), duration, rate).unwrap();
        assert_eq!(full_tail.start_frame, 270);
        assert_eq!(full_tail.frames, 30);

        // A zero-length span is rejected.
        assert!(matches!(
            resolve_window(
                &ExportRange::new(Time::new(5, 1), Time::new(5, 1)),
                duration,
                rate
            ),
            Err(ExportError::EmptyRange)
        ));
    }

    #[test]
    fn shifted_audio_graph_moves_clip_starts_earlier() {
        let graph = build_audio_graph(
            &[static_clip("./a.wav", 5.0, 2.0, 1.0)],
            Path::new("/root"),
            None,
        )
        .unwrap();
        let shifted = shifted_audio_graph(&graph, Time::new(3, 1)).unwrap();
        assert_eq!(shifted.clips[0].range.start.as_seconds().unwrap(), 2.0);

        // A clip that began before the window gets a negative start so the
        // mixer keeps advancing it from the right source position.
        let before = shifted_audio_graph(&graph, Time::new(7, 1)).unwrap();
        assert_eq!(before.clips[0].range.start.as_seconds().unwrap(), -2.0);
    }

    #[test]
    fn rounds_partial_project_frames_up_exactly() {
        assert_eq!(
            frame_count(Time::new(1001, 1000), Rational::new(30_000, 1001)).unwrap(),
            30
        );
        assert_eq!(
            frame_count(Time::new(1002, 1000), Rational::new(30_000, 1001)).unwrap(),
            31
        );
    }

    #[test]
    fn visual_only_project_keeps_dialogue_and_component_but_drops_audio() {
        const WITH_DIALOGUE: &str = r#"{
            "version": 0,
            "settings": {
                "width": 640,
                "height": 360,
                "frameRate": {"numerator": 30, "denominator": 1},
                "sampleRate": 48000
            },
            "assets": {
                "akane-default": {"type": "image", "name": "Akane", "source": {"type": "file", "path": "./portrait.png"}},
                "voice-001": {"type": "audio", "source": {"type": "file", "path": "./voice.wav"}}
            },
            "characters": {
                "akane": {
                    "name": "Akane",
                    "portrait": {
                        "defaultExpression": "default",
                        "expressions": {"default": "akane-default"}
                    },
                    "subtitle": {}
                }
            },
            "tracks": [
                {
                    "id": "dialogue",
                    "name": "Dialogue",
                    "kind": "dialogue",
                    "items": [
                        {
                            "id": "line-1",
                            "range": {"start": {"value": 0, "timescale": 1}, "duration": {"value": 1, "timescale": 1}},
                            "content": {"type": "dialogue", "character": "akane", "text": "Hello", "audio": "voice-001"}
                        },
                        {
                            "id": "voice-only",
                            "range": {"start": {"value": 0, "timescale": 1}, "duration": {"value": 1, "timescale": 1}},
                            "content": {"type": "audio", "asset": "voice-001"}
                        }
                    ]
                }
            ],
            "properties": {}
        }"#;
        let project = Project::from_json(WITH_DIALOGUE).unwrap();

        let filtered = visual_only_project(&project);
        let item_ids: Vec<&str> = filtered.tracks[0]
            .items
            .iter()
            .map(|item| item.id.as_str())
            .collect();
        assert_eq!(item_ids, vec!["line-1"]);

        let evaluator = Evaluator::new(&filtered).unwrap();
        let scene = evaluator.scene_at(Time::ZERO).unwrap();
        assert_eq!(scene.layers.len(), 1);
        let LayerContent::Group { layers } = &scene.layers[0].content else {
            panic!("dialogue should evaluate to a group of portrait + subtitle layers");
        };
        assert!(layers.iter().any(|layer| matches!(
            &layer.content,
            LayerContent::Image { asset } if asset.id == "akane-default"
        )));
        assert!(layers.iter().any(|layer| matches!(
            &layer.content,
            LayerContent::Text { text, .. } if text == "Hello"
        )));
    }

    #[test]
    fn refuses_non_mp4_and_existing_outputs() {
        let directory = tempfile::tempdir().unwrap();
        let existing = directory.path().join("existing.mp4");
        fs::write(&existing, []).unwrap();
        assert!(matches!(
            validate_output(&existing, false),
            Err(ExportError::OutputExists(_))
        ));
        assert!(matches!(
            validate_output(&directory.path().join("video.mov"), false),
            Err(ExportError::UnsupportedOutput(_))
        ));
        assert!(matches!(
            validate_dimensions(63, 64),
            Err(ExportError::UnsupportedDimensions {
                width: 63,
                height: 64
            })
        ));
    }
}
