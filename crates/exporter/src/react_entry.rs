use crate::control::{AudioLoudness, CompositionInfo, ExportCancellation, ExportProgress};
use crate::encode::{ExportSession, ExportTiming, report_timing};
use crate::error::ExportError;
use crate::exporter::{Exporter, NO_AUDIO_TRACK_LOUDNESS, stage_beside, video_file};
use crate::loudness::finish_mix;
use crate::project::build_audio_graph;
use crate::range::{resolve_window, shifted_audio_graph};
use crate::react::{CompanionProject, ReactRuntimeOptions, ReactVideoRequest};
use crate::render::{
    ensure_not_cancelled, open_video_output, validate_dimensions, validate_output,
};
use crate::subtitles;
use celesta_composition::Time;
use celesta_media::{AudioMixError, MediaBackend, mix_audio_graph_unclipped_cancellable};
use celesta_project::Project;
use celesta_react_bridge::{ReactCompositionMetadata, ReactSubtitleDescriptor};
use celesta_remote::StagedFile;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

impl Exporter {
    /// Exports a React composition entry to a video with
    /// `ExportOptions::output`'s components. When the composition (and any
    /// companion project) declares no audio, the video has no audio track.
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
    /// directory. Its `audio` items evaluate to no layer and play only in
    /// the mixdown.
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

    pub(crate) fn export_react_entry_impl(
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
        let mut session = ExportSession::start(crate::media::current(), cancellation);
        let mut staged = None;
        let encoded = match self.encode_react_entry(
            entry,
            react_runtime,
            project,
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
        report_timing(&encoded.timing.with(tracks.timing));
        let staged = staged.expect("an encoded export has its staged output");
        let files = self.publish_video(staged, output_path, tracks.files.clone(), cancellation)?;
        progress(ExportProgress::Wrote(video_file(
            output_path,
            encoded.start_frame,
            encoded.frame_count,
            tracks,
            files,
            encoded.loudness,
        )));
        if !self.options.subtitles.is_empty() {
            let window = subtitles::Window::new(encoded.window_start, encoded.duration)?;
            self.write_subtitles(
                &subtitles::react_cues(&encoded.subtitles, window),
                &mut progress,
            )?;
        }
        Ok(())
    }

    /// Chooses the components once the entry says its frame size, renders
    /// and encodes the entry into a file staged beside `output_path` (left
    /// in `staged`), then mixes and encodes the audio its frames declared,
    /// or decides it has none.
    #[allow(clippy::too_many_arguments)]
    fn encode_react_entry(
        &self,
        entry: &Path,
        react_runtime: &ReactRuntimeOptions,
        project: Option<(&Project, &Path)>,
        output_path: &Path,
        session: &mut ExportSession,
        staged: &mut Option<StagedFile>,
        cancellation: &ExportCancellation,
        progress: &mut impl FnMut(ExportProgress),
    ) -> Result<EncodedReactEntry, ExportError> {
        subtitles::check_outputs(&self.options.subtitles, self.options.overwrite)?;

        // Absolutize the asset roots up front: `absolutize_layers` and
        // `build_audio_graph` rewrite relative asset paths into absolute ones
        // by joining them with these roots, and a relative root (an entry
        // path passed relative to the current directory) would produce a
        // still-relative "absolute" path that the audio mixer then joins
        // again.
        let asset_root = entry.parent().unwrap_or_else(|| Path::new("."));
        let asset_root = &fs::canonicalize(asset_root).unwrap_or_else(|_| asset_root.to_owned());
        let project = canonical_project(project);

        let mut bridge = react_runtime.spawn(
            entry,
            project
                .as_ref()
                .map(|(project, root)| (*project, root.as_path())),
            self.plugins(),
        )?;
        let metadata = bridge.metadata().clone();
        report_composition(&metadata, progress);
        // The video encoder's ring is sized for the entry's frames.
        let (plan, warnings) = session.resolve(
            &self.options.output,
            &self.options.presets,
            output_path,
            (metadata.width, metadata.height),
        )?;
        for warning in warnings {
            progress(ExportProgress::Warning(warning));
        }
        progress(ExportProgress::Output(plan));
        validate_dimensions(metadata.width, metadata.height)?;
        if metadata.duration_in_frames == 0 {
            return Err(ExportError::EmptyTimeline);
        }

        let (start_frame, frame_count, window_start) = self.react_window(&metadata)?;

        // Every frame's response reports the `<Audio>` clips its tree
        // declares (see ReactAudioClipDescriptor), which accumulate into the
        // export's AudioGraph after the render; until then every video
        // packet waits for the muxer. An empty result (no `<Audio>`
        // anywhere it renders, no companion project audio) has no audio
        // track. The video decoder is attached unconditionally: the React
        // entry's own <Video> elements need decoding just as much as a
        // companion project's Video content does, and the React entry's
        // asset_root stays the renderer's single asset_root either way (see
        // absolutize_layers/absolutize_fonts for how a project's own,
        // differently-rooted assets still resolve).
        let mut renderer = self.export_renderer(asset_root, metadata.frame_rate, cancellation)?;
        let file = staged.insert(stage_beside(output_path)?);
        open_video_output(
            session,
            &mut renderer,
            metadata.width,
            metadata.height,
            metadata.frame_rate,
            file.path(),
        )?;
        let mut react_audio = Vec::new();
        let mut react_subtitles = Vec::new();
        let project_refs = project
            .as_ref()
            .map(|(project, root)| (*project, root.as_path()));
        let render_started = Instant::now();
        self.render_react_video(
            ReactVideoRequest {
                bridge: &mut bridge,
                metadata: &metadata,
                start_frame,
                frame_count,
                project: project_refs,
                audio: &mut react_audio,
                subtitles: &mut react_subtitles,
            },
            &mut renderer,
            session,
            cancellation,
            progress,
        )?;
        let mut timing = ExportTiming {
            render: render_started.elapsed(),
            frames: session.frames_sent(),
            ..ExportTiming::default()
        };
        ensure_not_cancelled(cancellation)?;

        let duration = Time::frames(
            i64::try_from(frame_count).map_err(|_| ExportError::TimelineTooLong)?,
            metadata.frame_rate,
        )
        .map_err(ExportError::Time)?;
        let graph = build_audio_graph(&react_audio, asset_root, project_refs)?;
        let mut loudness = None;
        if graph.clips.is_empty() || !session.encodes_audio() {
            if self.options.loudness.is_some() {
                progress(ExportProgress::Warning(if graph.clips.is_empty() {
                    "the composition has no sound, so there is no loudness to bring to the target"
                        .to_owned()
                } else {
                    NO_AUDIO_TRACK_LOUDNESS.to_owned()
                }));
            }
            session.no_audio()?;
        } else {
            progress(ExportProgress::MixingAudio);
            let mix_started = Instant::now();
            let graph = shifted_audio_graph(&graph, window_start).map_err(ExportError::Time)?;
            let mut audio_decoder = MediaBackend::new().with_kill_switch(cancellation.plugins());
            let audio_error = |error| match error {
                AudioMixError::Cancelled => ExportError::Cancelled,
                error => cancellation.settle(ExportError::Audio(error)),
            };
            let mut mix = mix_audio_graph_unclipped_cancellable(
                &graph,
                asset_root,
                duration,
                &mut audio_decoder,
                || cancellation.is_cancelled(),
            )
            .map_err(audio_error)?;
            let (measured, warnings) = finish_mix(&mut mix, self.options.loudness.as_ref(), || {
                cancellation.is_cancelled()
            })
            .map_err(audio_error)?;
            loudness = Some(measured);
            timing.mix = Some(mix_started.elapsed());
            timing.mix_done_at_frame = Some(session.frames_sent());
            for warning in warnings {
                progress(ExportProgress::Warning(warning));
            }
            ensure_not_cancelled(cancellation)?;
            session.encode_audio(&mix, self.options.looping)?;
        }
        ensure_not_cancelled(cancellation)?;
        Ok(EncodedReactEntry {
            start_frame,
            frame_count,
            window_start,
            duration,
            loudness,
            subtitles: react_subtitles,
            timing,
        })
    }

    /// Writes only a React entry's subtitle files (`ExportOptions::subtitles`),
    /// as a video export of the same range would: every frame of the range
    /// is evaluated, but none is rendered or encoded, so it takes a fraction
    /// of the export's time. `companion` supplies project properties as in a
    /// video export. Rendering progress counts the evaluated frames.
    pub fn export_react_subtitles(
        &self,
        entry: impl AsRef<Path>,
        react_runtime: &ReactRuntimeOptions,
        companion: Option<CompanionProject<'_>>,
        cancellation: &ExportCancellation,
        mut progress: impl FnMut(ExportProgress),
    ) -> Result<(), ExportError> {
        if self.options.subtitles.is_empty() {
            return Err(ExportError::NoOutput);
        }
        ensure_not_cancelled(cancellation)?;
        subtitles::check_outputs(&self.options.subtitles, self.options.overwrite)?;
        let project = canonical_project(
            companion.map(|companion| (companion.project, companion.project_asset_root)),
        );
        let mut bridge = react_runtime.spawn(
            entry.as_ref(),
            project
                .as_ref()
                .map(|(project, root)| (*project, root.as_path())),
            self.plugins(),
        )?;
        let metadata = bridge.metadata().clone();
        report_composition(&metadata, &mut progress);
        if metadata.duration_in_frames == 0 {
            return Err(ExportError::EmptyTimeline);
        }
        let (start_frame, frame_count, window_start) = self.react_window(&metadata)?;

        // A second of frames per request: few enough round trips to cost
        // nothing, often enough to report progress and notice cancellation.
        let chunk = metadata
            .frame_rate
            .numerator
            .div_ceil(metadata.frame_rate.denominator.max(1))
            .max(1);
        let mut reports = Vec::new();
        let mut done = 0;
        while done < frame_count {
            ensure_not_cancelled(cancellation)?;
            let frames = u64::from(chunk).min(frame_count - done);
            reports.extend(
                bridge
                    .collect_subtitles(start_frame + done, frames)
                    .map_err(ExportError::React)?,
            );
            done += frames;
            progress(ExportProgress::Rendering {
                frame: done,
                total: frame_count,
            });
        }
        ensure_not_cancelled(cancellation)?;

        let duration = Time::frames(
            i64::try_from(frame_count).map_err(|_| ExportError::TimelineTooLong)?,
            metadata.frame_rate,
        )
        .map_err(ExportError::Time)?;
        let window = subtitles::Window::new(window_start, duration)?;
        self.write_subtitles(&subtitles::react_cues(&reports, window), &mut progress)
    }

    /// Reports a React entry's compositions and the selected one's size and
    /// length, as an export does before it renders, without evaluating a
    /// frame. `companion` supplies project properties as in a video export.
    pub fn describe_react_entry(
        &self,
        entry: impl AsRef<Path>,
        react_runtime: &ReactRuntimeOptions,
        companion: Option<CompanionProject<'_>>,
        mut progress: impl FnMut(ExportProgress),
    ) -> Result<(), ExportError> {
        let project = canonical_project(
            companion.map(|companion| (companion.project, companion.project_asset_root)),
        );
        let bridge = react_runtime.spawn(
            entry.as_ref(),
            project
                .as_ref()
                .map(|(project, root)| (*project, root.as_path())),
            self.plugins(),
        )?;
        report_composition(bridge.metadata(), &mut progress);
        Ok(())
    }

    /// The first frame, the number of frames, and the start time of the
    /// span of the entry an export covers.
    fn react_window(
        &self,
        metadata: &ReactCompositionMetadata,
    ) -> Result<(u64, u64, Time), ExportError> {
        match self.options.range {
            None => Ok((0, metadata.duration_in_frames, Time::ZERO)),
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
                Ok((start_frame, window.frames, window.start))
            }
        }
    }
}

/// What rendering a React entry gave, for its report and subtitles.
struct EncodedReactEntry {
    start_frame: u64,
    frame_count: u64,
    window_start: Time,
    duration: Time,
    loudness: Option<AudioLoudness>,
    subtitles: Vec<ReactSubtitleDescriptor>,
    timing: ExportTiming,
}

/// The companion project with its asset root made absolute (see
/// `export_react_entry_impl`).
fn canonical_project<'a>(project: Option<(&'a Project, &Path)>) -> Option<(&'a Project, PathBuf)> {
    project.map(|(project, project_asset_root)| {
        let project_asset_root =
            fs::canonicalize(project_asset_root).unwrap_or_else(|_| project_asset_root.to_owned());
        (project, project_asset_root)
    })
}

/// Reports the entry's compositions and the selected one's size and length.
fn report_composition(
    metadata: &ReactCompositionMetadata,
    progress: &mut impl FnMut(ExportProgress),
) {
    progress(ExportProgress::ReactCompositions {
        selected: metadata.composition_id.clone(),
        compositions: metadata.compositions.clone(),
    });
    progress(ExportProgress::Composition(CompositionInfo {
        width: metadata.width,
        height: metadata.height,
        frame_rate: metadata.frame_rate,
        frames: metadata.duration_in_frames,
    }));
    for legacy in &metadata.legacy_imports {
        progress(ExportProgress::Warning(legacy.to_string()));
    }
}
