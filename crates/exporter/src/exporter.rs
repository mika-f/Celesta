use crate::control::{CompositionInfo, ExportCancellation, ExportProgress, ExportedFile};
use crate::error::ExportError;
use crate::options::ExportOptions;
use crate::range::{ExportWindow, resolve_window, shifted_audio_graph};
use crate::render::{
    ensure_encoders, ensure_not_cancelled, frame_count, validate_dimensions, validate_output,
};
use crate::subtitles;
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
        validate_output(
            output_path,
            self.options.video_format,
            self.options.overwrite,
        )?;
        ensure_encoders(self.options.video_format)?;
        subtitles::check_outputs(&self.options.subtitles, self.options.overwrite)?;
        let window = self.project_window(project, true, &mut progress)?;
        let subtitle_files = self.project_subtitle_files(project, window)?;

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
        let video_path = temporary
            .path()
            .join(format!("video.{}", self.options.video_format.as_str()));
        let final_file = tempfile::Builder::new()
            .prefix(".celesta-export-")
            .suffix(&format!(".{}", self.options.video_format.as_str()))
            .tempfile_in(parent)
            .map_err(|source| ExportError::Io {
                operation: "create temporary output",
                source,
            })?;

        let evaluator = Evaluator::new(project).map_err(ExportError::Evaluation)?;
        let graph = evaluator.audio_graph().map_err(ExportError::Evaluation)?;
        let graph = if self.options.range.is_some() {
            shifted_audio_graph(&graph, window.start).map_err(ExportError::Time)?
        } else {
            graph
        };
        let audio_duration = window.duration;

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
        progress(ExportProgress::Wrote(ExportedFile::Video {
            path: output_path.to_owned(),
            first_frame: u64::try_from(window.start_frame)
                .map_err(|_| ExportError::TimelineTooLong)?,
            frames: window.frames,
            audio: true,
            format: self.options.video_format,
        }));
        self.write_subtitles(&subtitle_files, &mut progress)
    }

    /// Writes a project's `ExportOptions::subtitles` without rendering
    /// anything: its dialogue items, timed as a video export of the same
    /// span would time them.
    pub fn export_project_subtitles(
        &self,
        project: &Project,
        mut progress: impl FnMut(ExportProgress),
    ) -> Result<(), ExportError> {
        if self.options.subtitles.is_empty() {
            return Err(ExportError::NothingToExport);
        }
        subtitles::check_outputs(&self.options.subtitles, self.options.overwrite)?;
        let window = self.project_window(project, false, &mut progress)?;
        let files = self.project_subtitle_files(project, window)?;
        self.write_subtitles(&files, &mut progress)
    }

    /// Reports the project's composition and resolves the exported span,
    /// checking its dimensions only for a `video`.
    fn project_window(
        &self,
        project: &Project,
        video: bool,
        progress: &mut impl FnMut(ExportProgress),
    ) -> Result<ExportWindow, ExportError> {
        let duration = project.effective_duration().map_err(ExportError::Time)?;
        let full_frame_count = frame_count(duration, project.settings.frame_rate)?;
        progress(ExportProgress::Composition(CompositionInfo {
            width: project.settings.width,
            height: project.settings.height,
            frame_rate: project.settings.frame_rate,
            frames: full_frame_count,
        }));
        if video {
            validate_dimensions(project.settings.width, project.settings.height)?;
        }
        if full_frame_count == 0 {
            return Err(ExportError::EmptyTimeline);
        }
        // With no range this is the whole composition; the audio mixdown then
        // keeps covering the exact `effective_duration` (not its frame-snapped
        // rounding), so that path stays byte-for-byte unchanged.
        Ok(match self.options.range {
            Some(range) => resolve_window(&range, duration, project.settings.frame_rate)?,
            None => ExportWindow {
                start: Time::ZERO,
                start_frame: 0,
                frames: full_frame_count,
                duration,
            },
        })
    }

    /// The subtitle files of a project exported over `window`.
    fn project_subtitle_files(
        &self,
        project: &Project,
        window: ExportWindow,
    ) -> Result<Vec<subtitles::SubtitleFile>, ExportError> {
        if self.options.subtitles.is_empty() {
            return Ok(Vec::new());
        }
        let window = subtitles::Window::new(window.start, window.duration)?;
        let cues = subtitles::project_cues(project, window).map_err(ExportError::Time)?;
        subtitles::files(&self.options.subtitles, &cues, self.options.overwrite)
    }

    /// Writes an export's subtitle files and reports each.
    pub(crate) fn write_subtitles(
        &self,
        files: &[subtitles::SubtitleFile],
        progress: &mut impl FnMut(ExportProgress),
    ) -> Result<(), ExportError> {
        for file in files {
            subtitles::write(file, self.options.overwrite)?;
            progress(ExportProgress::Wrote(ExportedFile::Subtitles {
                path: file.path.clone(),
                format: file.format,
                lang: file.lang.clone(),
                cues: file.cues.len(),
            }));
        }
        Ok(())
    }
}

impl Default for Exporter {
    fn default() -> Self {
        Self::new(ExportOptions::default())
    }
}
