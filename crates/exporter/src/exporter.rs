use crate::control::{CompositionInfo, ExportCancellation, ExportProgress, ExportedFile};
use crate::error::ExportError;
use crate::loudness::finish_mix;
use crate::options::ExportOptions;
use crate::output;
use crate::range::{ExportWindow, resolve_window, shifted_audio_graph};
use crate::render::{
    ensure_encoders, ensure_not_cancelled, frame_count, validate_dimensions, validate_output,
};
use crate::subtitles;
use celesta_composition::Time;
use celesta_evaluator::Evaluator;
use celesta_media::{AudioMixError, FfmpegBackend, mix_audio_graph_unclipped_cancellable};
use celesta_project::Project;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::{fs, thread};

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
        let duration = project.effective_duration().map_err(ExportError::Time)?;
        let full_frame_count = frame_count(duration, project.settings.frame_rate)?;
        progress(ExportProgress::Composition(CompositionInfo {
            width: project.settings.width,
            height: project.settings.height,
            frame_rate: project.settings.frame_rate,
            frames: full_frame_count,
        }));
        validate_dimensions(project.settings.width, project.settings.height)?;
        if full_frame_count == 0 {
            return Err(ExportError::EmptyTimeline);
        }
        let window = self.project_window(project, duration, full_frame_count)?;

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
        let final_file = output::stage(
            output_path,
            &format!(".{}", self.options.video_format.as_str()),
            "create temporary output",
        )?;

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
                let abandoned =
                    || cancellation.is_cancelled() || abandon_audio.load(Ordering::Acquire);
                let mut audio_decoder = FfmpegBackend::new();
                let mut mix = mix_audio_graph_unclipped_cancellable(
                    &graph,
                    asset_root,
                    audio_duration,
                    &mut audio_decoder,
                    abandoned,
                )?;
                let (loudness, warnings) =
                    finish_mix(&mut mix, self.options.loudness.as_ref(), abandoned)?;
                Ok((mix, loudness, warnings))
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
        let (audio, loudness, warnings) = audio
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
            .map_err(|error| match error {
                AudioMixError::Cancelled => ExportError::Cancelled,
                error => ExportError::Audio(error),
            })?;
        for warning in warnings {
            progress(ExportProgress::Warning(warning));
        }

        ensure_not_cancelled(cancellation)?;
        progress(ExportProgress::Muxing);
        self.mux_audio(&video_path, final_file.path(), &audio, cancellation)?;
        ensure_not_cancelled(cancellation)?;
        output::publish(
            final_file,
            output_path,
            self.options.overwrite,
            "publish exported video",
        )?;
        progress(ExportProgress::Wrote(ExportedFile::Video {
            path: output_path.to_owned(),
            first_frame: u64::try_from(window.start_frame)
                .map_err(|_| ExportError::TimelineTooLong)?,
            frames: window.frames,
            audio: true,
            loudness: Some(loudness),
            format: self.options.video_format,
        }));
        if !self.options.subtitles.is_empty() {
            let window = subtitles::Window::new(window.start, window.duration)?;
            let cues = subtitles::project_cues(project, window).map_err(ExportError::Time)?;
            self.write_subtitles(&cues, &mut progress)?;
        }
        Ok(())
    }

    /// Reports the project's size, rate, and length, as an export does
    /// before it renders.
    pub fn describe_project(
        &self,
        project: &Project,
        mut progress: impl FnMut(ExportProgress),
    ) -> Result<(), ExportError> {
        let duration = project.effective_duration().map_err(ExportError::Time)?;
        progress(ExportProgress::Composition(CompositionInfo {
            width: project.settings.width,
            height: project.settings.height,
            frame_rate: project.settings.frame_rate,
            frames: frame_count(duration, project.settings.frame_rate)?,
        }));
        Ok(())
    }

    /// Writes only the project's subtitle files (`ExportOptions::subtitles`),
    /// timed as a video export of the same range would time them, without
    /// rendering anything.
    pub fn export_project_subtitles(
        &self,
        project: &Project,
        mut progress: impl FnMut(ExportProgress),
    ) -> Result<(), ExportError> {
        if self.options.subtitles.is_empty() {
            return Err(ExportError::NoOutput);
        }
        subtitles::check_outputs(&self.options.subtitles, self.options.overwrite)?;
        let duration = project.effective_duration().map_err(ExportError::Time)?;
        let full_frame_count = frame_count(duration, project.settings.frame_rate)?;
        progress(ExportProgress::Composition(CompositionInfo {
            width: project.settings.width,
            height: project.settings.height,
            frame_rate: project.settings.frame_rate,
            frames: full_frame_count,
        }));
        if full_frame_count == 0 {
            return Err(ExportError::EmptyTimeline);
        }
        let window = self.project_window(project, duration, full_frame_count)?;
        let window = subtitles::Window::new(window.start, window.duration)?;
        let cues = subtitles::project_cues(project, window).map_err(ExportError::Time)?;
        self.write_subtitles(&cues, &mut progress)
    }

    /// The span of `project` an export covers. With no range this is the
    /// whole composition; the audio mixdown then keeps covering the exact
    /// `effective_duration` (not its frame-snapped rounding), so that path
    /// stays byte-for-byte unchanged.
    fn project_window(
        &self,
        project: &Project,
        duration: Time,
        full_frame_count: u64,
    ) -> Result<ExportWindow, ExportError> {
        match self.options.range {
            Some(range) => resolve_window(&range, duration, project.settings.frame_rate),
            None => Ok(ExportWindow {
                start: Time::ZERO,
                start_frame: 0,
                frames: full_frame_count,
                duration,
            }),
        }
    }

    /// Writes `cues` to every subtitle path and reports each file. Every
    /// path's files are worked out before any is written, so a path that
    /// cannot take the cues' languages writes nothing.
    pub(crate) fn write_subtitles(
        &self,
        cues: &[subtitles::Cue],
        progress: &mut impl FnMut(ExportProgress),
    ) -> Result<(), ExportError> {
        let planned = self
            .options
            .subtitles
            .iter()
            .map(|path| subtitles::files(path, cues).map(|files| (path, files)))
            .collect::<Result<Vec<_>, _>>()?;
        for (path, files) in planned {
            if files.is_empty() {
                progress(ExportProgress::Warning(format!(
                    "no subtitles to write to {}",
                    path.display()
                )));
            }
            for file in files {
                let format = subtitles::write(&file.path, &file.cues, self.options.overwrite)?;
                progress(ExportProgress::Wrote(ExportedFile::Subtitles {
                    path: file.path,
                    format,
                    cues: file.cues.len(),
                    lang: file.lang,
                }));
            }
        }
        Ok(())
    }
}

impl Default for Exporter {
    fn default() -> Self {
        Self::new(ExportOptions::default())
    }
}
