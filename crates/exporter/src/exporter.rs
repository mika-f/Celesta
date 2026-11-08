use crate::control::{
    AudioLoudness, CompositionInfo, ExportCancellation, ExportProgress, ExportedFile,
};
use crate::encode::{ExportSession, ExportedTracks, published_files};
use crate::error::ExportError;
use crate::loudness::finish_mix;
use crate::options::ExportOptions;
use crate::output;
use crate::range::{ExportWindow, resolve_window, shifted_audio_graph};
use crate::render::{
    ensure_not_cancelled, frame_count, open_video_output, validate_dimensions, validate_output,
};
use crate::subtitles;
use celesta_composition::Time;
use celesta_evaluator::Evaluator;
use celesta_media::{
    AudioBuffer, AudioMixError, MediaBackend, mix_audio_graph_unclipped_cancellable,
};
use celesta_plugin_host::InstalledPlugins;
use celesta_project::Project;
use celesta_remote::StagedFile;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, TryRecvError};
use std::thread;

/// A mix and its loudness, with the warnings finishing it gave, or why it
/// could not be made.
type Mixed = Result<(AudioBuffer, AudioLoudness, Vec<String>), AudioMixError>;

pub struct Exporter {
    pub(crate) options: ExportOptions,
}

impl Exporter {
    /// An exporter with `options`. Without `options.plugins`, the plugins
    /// are discovered here, once, in every location Celesta searches. When
    /// no media registry has been chosen yet ([`crate::use_media`]), theirs is.
    pub fn new(mut options: ExportOptions) -> Self {
        let plugins = options
            .plugins
            .get_or_insert_with(|| InstalledPlugins::find_reporting(&[], &[]));
        crate::media::ensure_media(plugins);
        Self { options }
    }

    /// The plugins this export's renderer and React runtime share.
    pub(crate) fn plugins(&self) -> &InstalledPlugins {
        self.options
            .plugins
            .as_ref()
            .expect("Exporter::new discovers the plugins")
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
        let mut session = ExportSession::start(crate::media::current(), cancellation);
        let mut staged = None;
        let (window, loudness) = match self.encode_project(
            project,
            asset_root,
            output_path,
            &mut session,
            &mut staged,
            cancellation,
            &mut progress,
        ) {
            Ok(encoded) => encoded,
            Err(error) => {
                // The staged output goes after the muxer stopped writing it.
                session.abort();
                return Err(cancellation.settle(error));
            }
        };
        progress(ExportProgress::Muxing);
        let tracks = session
            .finish()
            .map_err(|error| cancellation.settle(error))?;
        let staged = staged.expect("an encoded export has its staged output");
        let files = self.publish_video(staged, output_path, tracks.files.clone(), cancellation)?;
        progress(ExportProgress::Wrote(video_file(
            output_path,
            u64::try_from(window.start_frame).map_err(|_| ExportError::TimelineTooLong)?,
            window.frames,
            tracks,
            files,
            loudness,
        )));
        if !self.options.subtitles.is_empty() {
            let window = subtitles::Window::new(window.start, window.duration)?;
            let cues = subtitles::project_cues(project, window).map_err(ExportError::Time)?;
            self.write_subtitles(&cues, &mut progress)?;
        }
        Ok(())
    }

    /// Chooses the components, renders and encodes `project` into a file
    /// staged beside `output_path` (left in `staged`), and encodes its mix,
    /// which runs on its own thread beside the render: the span exported
    /// and the mix's loudness, when there is a mix.
    #[allow(clippy::too_many_arguments)]
    fn encode_project(
        &self,
        project: &Project,
        asset_root: &Path,
        output_path: &Path,
        session: &mut ExportSession,
        staged: &mut Option<StagedFile>,
        cancellation: &ExportCancellation,
        progress: &mut impl FnMut(ExportProgress),
    ) -> Result<(ExportWindow, Option<AudioLoudness>), ExportError> {
        progress(ExportProgress::Output(
            session.resolve(&self.options.output, output_path)?,
        ));
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

        let frame_rate = project.settings.frame_rate;
        let mut renderer = self.export_renderer(asset_root, frame_rate, cancellation)?;
        let file = staged.insert(stage_beside(output_path)?);
        open_video_output(
            session,
            &mut renderer,
            project.settings.width,
            project.settings.height,
            frame_rate,
            file.path(),
        )?;

        if !session.encodes_audio() {
            if self.options.loudness.is_some() {
                progress(ExportProgress::Warning(NO_AUDIO_TRACK_LOUDNESS.to_owned()));
            }
            session.no_audio()?;
            self.render_video(
                project,
                window,
                &mut renderer,
                session,
                cancellation,
                progress,
                &mut |_| Ok(()),
            )?;
            ensure_not_cancelled(cancellation)?;
            return Ok((window, None));
        }

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
        // Its mix is encoded after the first frame that ends once it is
        // done, so the muxer opens then and the frames stop waiting for it.
        // `abandon_audio` stops it early when rendering fails.
        let abandon_audio = AtomicBool::new(false);
        let looping = self.options.looping;
        thread::scope(|scope| {
            let (sender, mixed) = mpsc::channel::<Mixed>();
            let (graph, abandon_audio) = (&graph, &abandon_audio);
            // The sender moves into the thread, so a panic there disconnects
            // the channel rather than leaving the wait below hanging.
            let mixing = scope.spawn(move || {
                let abandoned =
                    || cancellation.is_cancelled() || abandon_audio.load(Ordering::Acquire);
                let mut audio_decoder =
                    MediaBackend::new().with_kill_switch(cancellation.plugins());
                let result = mix_audio_graph_unclipped_cancellable(
                    graph,
                    asset_root,
                    audio_duration,
                    &mut audio_decoder,
                    abandoned,
                )
                .and_then(|mut mix| {
                    let (loudness, warnings) =
                        finish_mix(&mut mix, self.options.loudness.as_ref(), abandoned)?;
                    Ok((mix, loudness, warnings))
                });
                let _ = sender.send(result);
            });
            let mut encoded = None;
            let rendered = self.render_video(
                project,
                window,
                &mut renderer,
                session,
                cancellation,
                progress,
                &mut |session| match mixed.try_recv() {
                    Ok(result) => {
                        encoded = Some(encode_mix(session, result, looping, cancellation)?);
                        Ok(())
                    }
                    Err(TryRecvError::Empty | TryRecvError::Disconnected) => Ok(()),
                },
            );
            if rendered.is_err() {
                abandon_audio.store(true, Ordering::Release);
            }
            rendered?;
            ensure_not_cancelled(cancellation)?;
            progress(ExportProgress::MixingAudio);
            let (loudness, warnings) = match encoded {
                Some(encoded) => encoded,
                None => match mixed.recv() {
                    Ok(result) => encode_mix(session, result, looping, cancellation)?,
                    // The mix thread panicked: the scope re-raises it.
                    Err(_) => match mixing.join() {
                        Err(panic) => std::panic::resume_unwind(panic),
                        Ok(()) => unreachable!("the mix thread always sends its mix"),
                    },
                },
            };
            for warning in warnings {
                progress(ExportProgress::Warning(warning));
            }
            ensure_not_cancelled(cancellation)?;
            Ok((window, Some(loudness)))
        })
    }

    /// Publishes the staged video at `output_path`: `files`, the muxer's,
    /// with the staged output named by its published path.
    pub(crate) fn publish_video(
        &self,
        staged: StagedFile,
        output_path: &Path,
        files: Vec<PathBuf>,
        cancellation: &ExportCancellation,
    ) -> Result<Vec<PathBuf>, ExportError> {
        ensure_not_cancelled(cancellation)?;
        let staged_path = staged.path().to_owned();
        output::publish(
            staged,
            output_path,
            self.options.overwrite,
            "publish exported video",
        )?;
        Ok(published_files(files, &staged_path, output_path))
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

/// The warning for a loudness target on an output without an audio track.
pub(crate) const NO_AUDIO_TRACK_LOUDNESS: &str =
    "the output has no audio track, so there is no loudness to bring to the target";

/// Stages the file a muxer writes beside `output`, with its extension.
pub(crate) fn stage_beside(output: &Path) -> Result<StagedFile, ExportError> {
    let suffix = output
        .extension()
        .map(|extension| format!(".{}", extension.to_string_lossy()))
        .unwrap_or_default();
    output::stage(output, &suffix, "create temporary output")
}

/// Encodes the finished mix, or fails with why it could not be made: its
/// loudness and the warnings finishing it gave.
fn encode_mix(
    session: &mut ExportSession,
    mixed: Mixed,
    looping: bool,
    cancellation: &ExportCancellation,
) -> Result<(AudioLoudness, Vec<String>), ExportError> {
    let (mix, loudness, warnings) = mixed.map_err(|error| match error {
        AudioMixError::Cancelled => ExportError::Cancelled,
        error => cancellation.settle(ExportError::Audio(error)),
    })?;
    session.encode_audio(&mix, looping)?;
    Ok((loudness, warnings))
}

/// The video an export wrote at `path`, as its components answered.
pub(crate) fn video_file(
    path: &Path,
    first_frame: u64,
    frames: u64,
    tracks: ExportedTracks,
    files: Vec<PathBuf>,
    loudness: Option<AudioLoudness>,
) -> ExportedFile {
    ExportedFile::Video {
        path: path.to_owned(),
        first_frame,
        frames,
        muxer: tracks.muxer,
        video: tracks.video,
        audio: tracks.audio,
        alpha: tracks.alpha,
        files,
        loudness,
    }
}

impl Default for Exporter {
    fn default() -> Self {
        Self::new(ExportOptions::default())
    }
}
