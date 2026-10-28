use crate::problems::error_chain;
use crate::worker::Worker;
use celesta_exporter::{
    AudioLoudness, ExportCancellation, ExportError, ExportOptions, ExportProgress, ExportRange,
    ExportedFile, Exporter, GpuDriver, LoudnessTarget, ReactRuntimeOptions, VideoFormat,
};
use celesta_project::Project;
use celesta_react_bridge::PropertyInputs;
use std::path::PathBuf;

pub(crate) struct ExportRequest {
    pub(crate) source: ExportSource,
    pub(crate) asset_root: PathBuf,
    pub(crate) output: PathBuf,
    pub(crate) format: VideoFormat,
    pub(crate) range: Option<ExportRange>,
    /// The loudness to bring the mix to; `None` leaves it as it is.
    pub(crate) loudness: Option<LoudnessTarget>,
    pub(crate) cancellation: ExportCancellation,
    pub(crate) driver: GpuDriver,
}

/// What the export worker renders: a normal `project.json`, or a standalone
/// React composition entry (`.tsx` preview mode). The React path has no
/// export range (the whole composition is always rendered).
pub(crate) enum ExportSource {
    Project(Box<Project>),
    ReactEntry {
        entry: PathBuf,
        node: PathBuf,
        cli_script: PathBuf,
        /// The `--props`/`--props-file` values the preview uses.
        properties: PropertyInputs,
        composition_id: String,
    },
}

pub(crate) enum ExportEvent {
    Progress(ExportProgress),
    Finished {
        output: PathBuf,
        result: Result<(), String>,
        cancelled: bool,
        /// The written video's mix loudness, when it has sound.
        loudness: Option<AudioLoudness>,
    },
}

/// Runs exports in the order they were asked for, sending each one's
/// progress and then how it finished.
pub(crate) type ExportWorker = Worker<ExportRequest, ExportEvent>;

pub(crate) fn spawn_export() -> anyhow::Result<ExportWorker> {
    Worker::spawn("export", |requests, events| {
        while let Ok(request) = requests.recv() {
            let exporter = Exporter::new(export_options(&request));
            let mut loudness = None;
            let progress = |progress| {
                if let ExportProgress::Wrote(ExportedFile::Video {
                    loudness: measured, ..
                }) = &progress
                {
                    loudness = *measured;
                }
                let _ = events.send(ExportEvent::Progress(progress));
            };
            let result = match &request.source {
                ExportSource::Project(project) => exporter.export_project_cancellable(
                    project,
                    &request.asset_root,
                    &request.output,
                    &request.cancellation,
                    progress,
                ),
                ExportSource::ReactEntry {
                    entry,
                    node,
                    cli_script,
                    properties,
                    composition_id,
                } => exporter.export_react_entry_cancellable(
                    entry,
                    &ReactRuntimeOptions::new(node.clone(), cli_script.clone())
                        .with_properties(properties.clone())
                        .with_composition(Some(composition_id.clone())),
                    &request.output,
                    &request.cancellation,
                    progress,
                ),
            };
            let cancelled = matches!(result, Err(ExportError::Cancelled));
            if events
                .send(ExportEvent::Finished {
                    output: request.output,
                    result: result.map_err(|error| error_chain(&error)),
                    cancelled,
                    loudness,
                })
                .is_err()
            {
                break;
            }
        }
    })
}

/// The exporter settings for one request: the editor always overwrites the
/// file the save panel already confirmed.
pub(crate) fn export_options(request: &ExportRequest) -> ExportOptions {
    ExportOptions {
        overwrite: true,
        range: request.range,
        video_format: request.format,
        driver: request.driver,
        loudness: request.loudness,
        ..ExportOptions::default()
    }
}
