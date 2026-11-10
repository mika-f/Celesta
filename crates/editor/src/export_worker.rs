use crate::problems::error_chain;
use crate::worker::Worker;
use celesta_exporter::{
    AudioLoudness, ExportCancellation, ExportError, ExportOptions, ExportProgress, ExportRange,
    ExportedFile, Exporter, GpuDriver, LoudnessTarget, OutputChoice, ReactRuntimeOptions,
};
use celesta_plugin_host::InstalledPlugins;
use celesta_project::Project;
use celesta_react_bridge::PropertyInputs;
use std::path::PathBuf;

pub(crate) struct ExportRequest {
    pub(crate) source: ExportSource,
    pub(crate) asset_root: PathBuf,
    /// The video to write.
    pub(crate) output: PathBuf,
    pub(crate) range: Option<ExportRange>,
    /// The loudness to bring the mix to; `None` leaves it as it is.
    pub(crate) loudness: Option<LoudnessTarget>,
    /// The muxer and encoders the export page chose, with their options.
    pub(crate) output_choice: OutputChoice,
    pub(crate) cancellation: ExportCancellation,
    pub(crate) driver: GpuDriver,
    /// The plugins the preview draws with, so the export draws the same.
    pub(crate) plugins: InstalledPlugins,
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

#[expect(
    clippy::large_enum_variant,
    reason = "a few events per frame at most, matched by value where they arrive"
)]
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
/// file the save panel already confirmed, and writes it with the components
/// the export page chose.
pub(crate) fn export_options(request: &ExportRequest) -> ExportOptions {
    ExportOptions {
        overwrite: true,
        range: request.range,
        output: request.output_choice.clone(),
        driver: request.driver,
        loudness: request.loudness,
        plugins: Some(request.plugins.clone()),
        ..ExportOptions::default()
    }
}
