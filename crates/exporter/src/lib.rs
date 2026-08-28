//! Frame-exact project export through the shared evaluator and renderers.

use std::collections::BTreeMap;
use std::error::Error;
use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use mikan_composition::{
    AssetLocation, AudioClip, AudioGraph, Layer, LayerContent, Rational, ResolvedAsset, Time,
    TimeError, TimeRange,
};
use mikan_evaluator::{EvaluationError, Evaluator};
use mikan_gpu_renderer::{GpuRenderError, GpuRenderOptions, GpuRenderer};
use mikan_media::{AudioMixError, FfmpegBackend, mix_audio_graph_cancellable};
use mikan_project::{LoadError, Project, TimelineContent};
use mikan_react_bridge::{
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportOptions {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub overwrite: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            ffmpeg: PathBuf::from("ffmpeg"),
            ffprobe: PathBuf::from("ffprobe"),
            overwrite: false,
        }
    }
}

/// Locates the `@mikan/react` Node.js runtime used to evaluate a React entry.
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
        let frame_count = frame_count(duration, project.settings.frame_rate)?;
        if frame_count == 0 {
            return Err(ExportError::EmptyTimeline);
        }

        let parent = output_path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|source| ExportError::Io {
            operation: "create output directory",
            source,
        })?;
        let temporary = tempfile::Builder::new()
            .prefix(".mikan-export-")
            .tempdir_in(parent)
            .map_err(|source| ExportError::Io {
                operation: "create export workspace",
                source,
            })?;
        let video_path = temporary.path().join("video.mp4");
        let final_file = tempfile::Builder::new()
            .prefix(".mikan-export-")
            .suffix(".mp4")
            .tempfile_in(parent)
            .map_err(|source| ExportError::Io {
                operation: "create temporary output",
                source,
            })?;

        self.render_video(
            project,
            asset_root,
            frame_count,
            &video_path,
            cancellation,
            &mut progress,
        )?;
        ensure_not_cancelled(cancellation)?;
        progress(ExportProgress::MixingAudio);
        let evaluator = Evaluator::new(project).map_err(ExportError::Evaluation)?;
        let graph = evaluator.audio_graph().map_err(ExportError::Evaluation)?;
        let mut audio_decoder = FfmpegBackend::with_executables(
            self.options.ffmpeg.clone(),
            self.options.ffprobe.clone(),
        );
        let audio =
            mix_audio_graph_cancellable(&graph, asset_root, duration, &mut audio_decoder, || {
                cancellation.is_cancelled()
            })
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

        // Video frames stream into a staged file first; every frame's
        // response reports the `<Audio>` clips its tree declares (see
        // ReactAudioClipDescriptor), which accumulate into the export's
        // AudioGraph afterwards. An empty result (no `<Audio>` anywhere it
        // renders, no companion project audio) keeps the silent-export
        // behavior: the staged video is published directly, no mix/mux
        // stage.
        let final_file = tempfile::Builder::new()
            .prefix(".mikan-export-")
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
                i64::try_from(metadata.duration_in_frames)
                    .map_err(|_| ExportError::TimelineTooLong)?,
                metadata.frame_rate,
            )
            .map_err(ExportError::Time)?;
            let mut audio_decoder = FfmpegBackend::with_executables(
                self.options.ffmpeg.clone(),
                self.options.ffprobe.clone(),
            );
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
                .prefix(".mikan-export-")
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
        frame_count: u64,
        output: &Path,
        cancellation: &ExportCancellation,
        progress: &mut impl FnMut(ExportProgress),
    ) -> Result<(), ExportError> {
        let frame_rate = project.settings.frame_rate;
        let dimensions = format!("{}x{}", project.settings.width, project.settings.height);
        let rate = format!("{}/{}", frame_rate.numerator, frame_rate.denominator);
        let mut child = Command::new(&self.options.ffmpeg)
            .args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgba"])
            .arg("-video_size")
            .arg(dimensions)
            .arg("-framerate")
            .arg(rate)
            .args(["-i", "pipe:0", "-an", "-frames:v"])
            .arg(frame_count.to_string())
            .args([
                "-c:v",
                "libx264",
                "-preset",
                "medium",
                "-crf",
                "18",
                "-pix_fmt",
                "yuv420p",
                "-threads",
                "0",
                "-movflags",
                "+faststart",
            ])
            .arg(output)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| ExportError::Executable {
                executable: self.options.ffmpeg.clone(),
                source,
            })?;

        let write_result = (|| {
            let stdin = child.stdin.take().ok_or(ExportError::MissingPipe)?;
            let mut stdin = BufWriter::new(stdin);
            let evaluator = Evaluator::new(project).map_err(ExportError::Evaluation)?;
            let video_decoder = FfmpegBackend::with_executables(
                self.options.ffmpeg.clone(),
                self.options.ffprobe.clone(),
            )
            .with_sequential_video(frame_rate);
            let mut renderer = GpuRenderer::new(GpuRenderOptions::default())
                .map_err(ExportError::Render)?
                .with_asset_root(asset_root)
                .with_video_decoder(video_decoder);
            for frame_index in 0..frame_count {
                ensure_not_cancelled(cancellation)?;
                progress(ExportProgress::Rendering {
                    frame: frame_index + 1,
                    total: frame_count,
                });
                let frame_index =
                    i64::try_from(frame_index).map_err(|_| ExportError::TimelineTooLong)?;
                let time = Time::frames(frame_index, frame_rate).map_err(ExportError::Time)?;
                let scene = evaluator.scene_at(time).map_err(ExportError::Evaluation)?;
                let frame = renderer.render(&scene).map_err(ExportError::Render)?;
                stdin
                    .write_all(frame.pixels())
                    .map_err(|source| ExportError::Io {
                        operation: "stream video frame to FFmpeg",
                        source,
                    })?;
            }
            stdin.flush().map_err(|source| ExportError::Io {
                operation: "finish video frame stream",
                source,
            })?;
            Ok(())
        })();
        finish_process(child, write_result, "video encoding")
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

        let dimensions = format!("{}x{}", metadata.width, metadata.height);
        let rate = format!(
            "{}/{}",
            metadata.frame_rate.numerator, metadata.frame_rate.denominator
        );
        let mut child = Command::new(&self.options.ffmpeg)
            .args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgba"])
            .arg("-video_size")
            .arg(dimensions)
            .arg("-framerate")
            .arg(rate)
            .args(["-i", "pipe:0", "-an", "-frames:v"])
            .arg(metadata.duration_in_frames.to_string())
            .args([
                "-c:v",
                "libx264",
                "-preset",
                "medium",
                "-crf",
                "18",
                "-pix_fmt",
                "yuv420p",
                "-threads",
                "0",
                "-movflags",
                "+faststart",
            ])
            .arg(output)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| ExportError::Executable {
                executable: self.options.ffmpeg.clone(),
                source,
            })?;

        let write_result = (|| {
            let stdin = child.stdin.take().ok_or(ExportError::MissingPipe)?;
            let mut stdin = BufWriter::new(stdin);
            let mut renderer =
                GpuRenderer::new(GpuRenderOptions::default()).map_err(ExportError::Render)?;
            renderer = renderer.with_asset_root(asset_root);
            // Attached unconditionally: the React entry's own <Video>
            // elements need decoding just as much as a companion project's
            // Video content does, and the React entry's asset_root stays the
            // renderer's single asset_root either way (see
            // absolutize_layers/absolutize_fonts below for how a project's
            // own, differently-rooted assets still resolve).
            let video_decoder = FfmpegBackend::with_executables(
                self.options.ffmpeg.clone(),
                self.options.ffprobe.clone(),
            )
            .with_sequential_video(metadata.frame_rate);
            renderer = renderer.with_video_decoder(video_decoder);
            for frame_index in 0..metadata.duration_in_frames {
                ensure_not_cancelled(cancellation)?;
                progress(ExportProgress::Rendering {
                    frame: frame_index + 1,
                    total: metadata.duration_in_frames,
                });
                let frame_index =
                    i64::try_from(frame_index).map_err(|_| ExportError::TimelineTooLong)?;
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
                let frame = renderer.render(&scene).map_err(ExportError::Render)?;
                stdin
                    .write_all(frame.pixels())
                    .map_err(|source| ExportError::Io {
                        operation: "stream video frame to FFmpeg",
                        source,
                    })?;
            }
            stdin.flush().map_err(|source| ExportError::Io {
                operation: "finish video frame stream",
                source,
            })?;
            Ok(())
        })();
        finish_process(child, write_result, "video encoding")
    }

    fn mux_audio(
        &self,
        video: &Path,
        output: &Path,
        audio: &mikan_media::AudioBuffer,
        cancellation: &ExportCancellation,
    ) -> Result<(), ExportError> {
        let mut child = Command::new(&self.options.ffmpeg)
            .args(["-v", "error", "-y", "-i"])
            .arg(video)
            .args(["-f", "f32le", "-ar"])
            .arg(audio.sample_rate.to_string())
            .arg("-ac")
            .arg(audio.channels.to_string())
            .args([
                "-i",
                "pipe:0",
                "-map",
                "0:v:0",
                "-map",
                "1:a:0",
                "-c:v",
                "copy",
                "-c:a",
                "aac",
                "-b:a",
                "192k",
                "-threads",
                "0",
                "-shortest",
                "-movflags",
                "+faststart",
            ])
            .arg(output)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| ExportError::Executable {
                executable: self.options.ffmpeg.clone(),
                source,
            })?;
        let write_result = (|| {
            let stdin = child.stdin.take().ok_or(ExportError::MissingPipe)?;
            let mut stdin = BufWriter::new(stdin);
            for samples in audio.samples.chunks(4_096) {
                ensure_not_cancelled(cancellation)?;
                for sample in samples {
                    stdin
                        .write_all(&sample.to_le_bytes())
                        .map_err(|source| ExportError::Io {
                            operation: "stream mixed audio to FFmpeg",
                            source,
                        })?;
                }
            }
            stdin.flush().map_err(|source| ExportError::Io {
                operation: "finish mixed audio stream",
                source,
            })
        })();
        finish_process(child, write_result, "audio muxing")
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
/// `LayerContent::MissingComponent`, which `@mikan/react` resolves against
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
        LayerContent::Video { asset, .. } | LayerContent::Image { asset } => {
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

fn finish_process(
    mut child: Child,
    write_result: Result<(), ExportError>,
    stage: &'static str,
) -> Result<(), ExportError> {
    drop(child.stdin.take());
    if matches!(&write_result, Err(ExportError::Cancelled)) {
        let _ = child.kill();
        let _ = child.wait();
        return write_result;
    }
    let output = child.wait_with_output().map_err(|source| ExportError::Io {
        operation: "wait for FFmpeg",
        source,
    })?;
    if !output.status.success() {
        return Err(ExportError::Process {
            stage,
            status: output.status,
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    write_result
}

#[derive(Debug)]
pub enum ExportError {
    Project(LoadError),
    Evaluation(EvaluationError),
    Render(GpuRenderError),
    Audio(AudioMixError),
    React(ReactBridgeError),
    Time(TimeError),
    Executable {
        executable: PathBuf,
        source: io::Error,
    },
    Io {
        operation: &'static str,
        source: io::Error,
    },
    Process {
        stage: &'static str,
        status: ExitStatus,
        stderr: String,
    },
    OutputExists(PathBuf),
    UnsupportedOutput(PathBuf),
    UnsupportedDimensions {
        width: u32,
        height: u32,
    },
    EmptyTimeline,
    Cancelled,
    TimelineTooLong,
    MissingPipe,
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
            Self::Executable { executable, source } => {
                write!(
                    formatter,
                    "could not run {}: {source}",
                    executable.display()
                )
            }
            Self::Io { operation, source } => write!(formatter, "could not {operation}: {source}"),
            Self::Process {
                stage,
                status,
                stderr,
            } => write!(formatter, "FFmpeg {stage} failed ({status}): {stderr}"),
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
            Self::EmptyTimeline => formatter.write_str("cannot export an empty timeline"),
            Self::Cancelled => formatter.write_str("export was cancelled"),
            Self::TimelineTooLong => formatter.write_str("export timeline is too long"),
            Self::MissingPipe => formatter.write_str("FFmpeg did not provide its input pipe"),
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
            Self::Executable { source, .. } | Self::Io { source, .. } => Some(source),
            Self::Process { .. }
            | Self::OutputExists(_)
            | Self::UnsupportedOutput(_)
            | Self::UnsupportedDimensions { .. }
            | Self::EmptyTimeline
            | Self::Cancelled
            | Self::TimelineTooLong
            | Self::MissingPipe => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mikan_composition::{Animatable, Keyframe, KeyframeAnimation, KeyframeAnimationType};

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
