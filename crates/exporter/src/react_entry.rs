use crate::control::{CompositionInfo, ExportCancellation, ExportProgress, ExportedFile};
use crate::error::ExportError;
use crate::exporter::Exporter;
use crate::project::build_audio_graph;
use crate::range::{resolve_window, shifted_audio_graph};
use crate::react::{CompanionProject, ReactRuntimeOptions, ReactVideoRequest};
use crate::render::{ensure_not_cancelled, validate_dimensions, validate_output};
use celesta_composition::Time;
use celesta_media::{AudioMixError, FfmpegBackend, mix_audio_graph_cancellable};
use celesta_project::Project;
use celesta_react_bridge::ReactBridge;
use std::path::Path;
use std::{fs, io};

impl Exporter {
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
        progress(ExportProgress::Composition(CompositionInfo {
            width: metadata.width,
            height: metadata.height,
            frame_rate: metadata.frame_rate,
            frames: metadata.duration_in_frames,
        }));
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
        let audio = !graph.clips.is_empty();
        let output_file = if !audio {
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
        progress(ExportProgress::Wrote(ExportedFile::Video {
            path: output_path.to_owned(),
            first_frame: start_frame,
            frames: frame_count,
            audio,
        }));
        Ok(())
    }
}
