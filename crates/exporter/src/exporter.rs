use crate::control::{ExportCancellation, ExportProgress};
use crate::error::ExportError;
use crate::options::ExportOptions;
use crate::range::{ExportWindow, resolve_window, shifted_audio_graph};
use crate::render::{ensure_not_cancelled, frame_count, validate_dimensions, validate_output};
use celesta_composition::Time;
use celesta_evaluator::Evaluator;
use celesta_media::{AudioMixError, FfmpegBackend, mix_audio_graph_cancellable};
use celesta_project::Project;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::{fs, io, thread};

pub struct Exporter {
    pub(crate) options: ExportOptions,
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
}

impl Default for Exporter {
    fn default() -> Self {
        Self::new(ExportOptions::default())
    }
}
