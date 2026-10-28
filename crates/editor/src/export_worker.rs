use crate::problems::error_chain;
use celesta_exporter::{
    AudioLoudness, ExportCancellation, ExportError, ExportOptions, ExportProgress, ExportRange,
    ExportedFile, Exporter, GpuDriver, LoudnessTarget, ReactRuntimeOptions, VideoFormat,
};
use celesta_project::Project;
use celesta_react_bridge::PropertyInputs;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

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

pub(crate) struct ExportWorker {
    pub(crate) requests: mpsc::Sender<ExportRequest>,
    pub(crate) events: mpsc::Receiver<ExportEvent>,
}

impl ExportWorker {
    pub(crate) fn spawn() -> anyhow::Result<Self> {
        let (request_tx, request_rx) = mpsc::channel::<ExportRequest>();
        let (event_tx, event_rx) = mpsc::channel::<ExportEvent>();
        thread::Builder::new()
            .name("celesta-export".to_owned())
            .spawn(move || {
                while let Ok(request) = request_rx.recv() {
                    let exporter = Exporter::new(export_options(&request));
                    let mut loudness = None;
                    let progress = |progress| {
                        if let ExportProgress::Wrote(ExportedFile::Video {
                            loudness: measured,
                            ..
                        }) = &progress
                        {
                            loudness = *measured;
                        }
                        let _ = event_tx.send(ExportEvent::Progress(progress));
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
                    if event_tx
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
            })?;
        Ok(Self {
            requests: request_tx,
            events: event_rx,
        })
    }

    pub(crate) fn request(&self, request: ExportRequest) -> Result<(), String> {
        self.requests
            .send(request)
            .map_err(|_| "export worker stopped unexpectedly".to_owned())
    }
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
