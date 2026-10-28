use crate::control::{CompositionInfo, ExportCancellation, ExportProgress, ExportedFile};
use crate::error::ExportError;
use crate::exporter::Exporter;
use crate::loudness::finish_mix;
use crate::output;
use crate::project::build_audio_graph;
use crate::range::{resolve_window, shifted_audio_graph};
use crate::react::{CompanionProject, ReactRuntimeOptions, ReactVideoRequest};
use crate::render::{ensure_encoders, ensure_not_cancelled, validate_dimensions, validate_output};
use crate::subtitles;
use celesta_composition::Time;
use celesta_media::{AudioMixError, FfmpegBackend, mix_audio_graph_unclipped_cancellable};
use celesta_project::Project;
use celesta_react_bridge::ReactCompositionMetadata;
use std::fs;
use std::path::{Path, PathBuf};

impl Exporter {
    /// Exports a React composition entry to a video in
    /// `ExportOptions::video_format`. When the composition (and any companion
    /// project) declares no audio, the encoded video is published directly
    /// instead of going through a separate mux stage.
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
        validate_output(
            output_path,
            self.options.video_format,
            self.options.overwrite,
        )?;
        ensure_encoders(self.options.video_format)?;
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
        let parent = output_path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|source| ExportError::Io {
            operation: "create output directory",
            source,
        })?;

        let mut bridge = react_runtime.spawn(
            entry,
            project
                .as_ref()
                .map(|(project, root)| (*project, root.as_path())),
        )?;
        let metadata = bridge.metadata().clone();
        report_composition(&metadata, &mut progress);
        validate_dimensions(metadata.width, metadata.height)?;
        if metadata.duration_in_frames == 0 {
            return Err(ExportError::EmptyTimeline);
        }

        let (start_frame, frame_count, window_start) = self.react_window(&metadata)?;

        // Video frames stream into a staged file first; every frame's
        // response reports the `<Audio>` clips its tree declares (see
        // ReactAudioClipDescriptor), which accumulate into the export's
        // AudioGraph afterwards. An empty result (no `<Audio>` anywhere it
        // renders, no companion project audio) keeps the silent-export
        // behavior: the staged video is published directly, no mix/mux
        // stage.
        let suffix = format!(".{}", self.options.video_format.as_str());
        let final_file = output::stage(output_path, &suffix, "create temporary output")?;
        let mut react_audio = Vec::new();
        let mut react_subtitles = Vec::new();
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
                subtitles: &mut react_subtitles,
            },
            final_file.path(),
            cancellation,
            &mut progress,
        )?;
        ensure_not_cancelled(cancellation)?;

        let duration = Time::frames(
            i64::try_from(frame_count).map_err(|_| ExportError::TimelineTooLong)?,
            metadata.frame_rate,
        )
        .map_err(ExportError::Time)?;
        let graph = build_audio_graph(&react_audio, asset_root, project_refs)?;
        let audio = !graph.clips.is_empty();
        let mut loudness = None;
        let output_file = if !audio {
            if self.options.loudness.is_some() {
                progress(ExportProgress::Warning(
                    "the composition has no sound, so there is no loudness to bring to the target"
                        .to_owned(),
                ));
            }
            final_file
        } else {
            progress(ExportProgress::MixingAudio);
            let graph = shifted_audio_graph(&graph, window_start).map_err(ExportError::Time)?;
            let mut audio_decoder = FfmpegBackend::new();
            let audio_error = |error| match error {
                AudioMixError::Cancelled => ExportError::Cancelled,
                error => ExportError::Audio(error),
            };
            let mut audio = mix_audio_graph_unclipped_cancellable(
                &graph,
                asset_root,
                duration,
                &mut audio_decoder,
                || cancellation.is_cancelled(),
            )
            .map_err(audio_error)?;
            let (measured, warnings) =
                finish_mix(&mut audio, self.options.loudness.as_ref(), || {
                    cancellation.is_cancelled()
                })
                .map_err(audio_error)?;
            loudness = Some(measured);
            for warning in warnings {
                progress(ExportProgress::Warning(warning));
            }
            ensure_not_cancelled(cancellation)?;
            progress(ExportProgress::Muxing);
            let muxed_file = output::stage(output_path, &suffix, "create temporary output")?;
            self.mux_audio(final_file.path(), muxed_file.path(), &audio, cancellation)?;
            muxed_file
        };

        ensure_not_cancelled(cancellation)?;
        output::publish(
            output_file,
            output_path,
            self.options.overwrite,
            "publish exported video",
        )?;
        progress(ExportProgress::Wrote(ExportedFile::Video {
            path: output_path.to_owned(),
            first_frame: start_frame,
            frames: frame_count,
            audio,
            loudness,
            format: self.options.video_format,
        }));
        if !self.options.subtitles.is_empty() {
            let window = subtitles::Window::new(window_start, duration)?;
            self.write_subtitles(
                &subtitles::react_cues(&react_subtitles, window),
                &mut progress,
            )?;
        }
        Ok(())
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
}
