use celesta_exporter::{
    ExportCancellation, ExportError, ExportOptions, ExportProgress, ExportRange, Exporter,
    ReactRuntimeOptions,
};
use celesta_project::Project;
use std::error::Error;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

pub(crate) struct ExportRequest {
    pub(crate) source: ExportSource,
    pub(crate) asset_root: PathBuf,
    pub(crate) output: PathBuf,
    pub(crate) range: Option<ExportRange>,
    pub(crate) cancellation: ExportCancellation,
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
    },
}

pub(crate) enum ExportEvent {
    Progress(ExportProgress),
    Finished {
        output: PathBuf,
        result: Result<(), String>,
        cancelled: bool,
    },
}

pub(crate) struct ExportWorker {
    pub(crate) requests: mpsc::Sender<ExportRequest>,
    pub(crate) events: mpsc::Receiver<ExportEvent>,
}

impl ExportWorker {
    pub(crate) fn spawn() -> Result<Self, Box<dyn Error>> {
        let (request_tx, request_rx) = mpsc::channel::<ExportRequest>();
        let (event_tx, event_rx) = mpsc::channel::<ExportEvent>();
        thread::Builder::new()
            .name("celesta-export".to_owned())
            .spawn(move || {
                while let Ok(request) = request_rx.recv() {
                    let exporter = Exporter::new(ExportOptions {
                        overwrite: true,
                        range: request.range,
                        ..ExportOptions::default()
                    });
                    let progress = |progress| {
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
                        } => exporter.export_react_entry_cancellable(
                            entry,
                            &ReactRuntimeOptions::new(node.clone(), cli_script.clone()),
                            &request.output,
                            &request.cancellation,
                            progress,
                        ),
                    };
                    let cancelled = matches!(result, Err(ExportError::Cancelled));
                    if event_tx
                        .send(ExportEvent::Finished {
                            output: request.output,
                            result: result.map_err(|error| error.to_string()),
                            cancelled,
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
