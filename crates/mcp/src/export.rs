//! Runs an export job off the async runtime, one at a time, and reports it
//! as `Celesta-export --json` does, with MCP progress and cancellation.

use std::sync::Arc;
use std::time::{Duration, Instant};

use celesta_exporter::report::{self, HintStyle};
use celesta_exporter::{ExportCancellation, ExportJob, ExportProgress, ExportedFile};
use celesta_react_bridge::resolve_props_file;
use rmcp::model::{ProgressNotificationParam, ProgressToken};
use rmcp::service::RequestContext;
use rmcp::{Peer, RoleServer};
use serde_json::Value;
use tokio::sync::{Mutex, mpsc};

use crate::server::SharedMedia;

/// Rendering progress is sent at most this often, plus the last frame.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

/// A finished export: its report, whether it succeeded, and the files it
/// wrote.
pub struct Run {
    pub report: Value,
    pub ok: bool,
    pub files: Vec<ExportedFile>,
}

/// Runs `job` while holding `render_lock`, so exports never compete for
/// the GPU, with the plugins `media` finds for its project
/// ([`SharedMedia::for_project`]). Progress goes to the client when the
/// request asked for it, and cancelling the request stops the export; the
/// lock is held until it has.
pub async fn run(
    job: ExportJob,
    render_lock: &Mutex<()>,
    media: &Arc<SharedMedia>,
    context: &RequestContext<RoleServer>,
) -> Run {
    let _guard = render_lock.lock().await;
    let cancellation = ExportCancellation::default();
    let watcher = {
        let cancellation = cancellation.clone();
        let cancelled = context.ct.clone();
        tokio::spawn(async move {
            cancelled.cancelled().await;
            cancellation.cancel();
        })
    };
    let (events, received) = mpsc::unbounded_channel();
    let forwarder = context
        .meta
        .get_progress_token()
        .map(|token| tokio::spawn(forward_progress(context.peer.clone(), token, received)));

    let media = media.clone();
    let joined = tokio::task::spawn_blocking(move || {
        let mut job = job;
        let found = media
            .for_project(&job.source)
            .map(|plugins| job.options.plugins = Some(plugins));
        let props_file = job
            .react
            .then(|| resolve_props_file(&job.source, job.props_file.as_deref()))
            .flatten();
        let mut files = Vec::new();
        let (report, ok) = report::build(&job.source, props_file, HintStyle::Mcp, |collect| {
            found?;
            job.run(&cancellation, &mut |event: ExportProgress| {
                if let ExportProgress::Wrote(file) = &event {
                    files.push(file.clone());
                }
                // Nobody listens without a progress token.
                let _ = events.send(event.clone());
                collect(event);
            })
        });
        Run {
            report: serde_json::to_value(report).expect("reports serialize"),
            ok,
            files,
        }
    })
    .await;
    watcher.abort();
    if let Some(forwarder) = forwarder {
        // The sender went away with the export, so this ends.
        let _ = forwarder.await;
    }
    joined.unwrap_or_else(|error| Run {
        report: serde_json::json!({
            "status": "error",
            "error": { "code": "internal", "message": format!("the export failed: {error}") },
        }),
        ok: false,
        files: Vec::new(),
    })
}

/// Sends rendering progress as frames of the total, then audio mixing and
/// muxing as two more steps, so the progress value always increases.
async fn forward_progress(
    peer: Peer<RoleServer>,
    token: ProgressToken,
    mut events: mpsc::UnboundedReceiver<ExportProgress>,
) {
    let mut last_sent: Option<Instant> = None;
    let mut frames = 0;
    while let Some(event) = events.recv().await {
        let notification = match event {
            ExportProgress::Rendering { frame, total } => {
                frames = total;
                let due = last_sent.is_none_or(|sent| sent.elapsed() >= PROGRESS_INTERVAL);
                if frame != total && !due {
                    continue;
                }
                ProgressNotificationParam::new(token.clone(), frame as f64).with_total(total as f64)
            }
            ExportProgress::MixingAudio => {
                ProgressNotificationParam::new(token.clone(), (frames + 1) as f64)
                    .with_total((frames + 2) as f64)
                    .with_message("mixing audio")
            }
            ExportProgress::Muxing => {
                ProgressNotificationParam::new(token.clone(), (frames + 2) as f64)
                    .with_total((frames + 2) as f64)
                    .with_message("writing the video")
            }
            _ => continue,
        };
        last_sent = Some(Instant::now());
        if peer.notify_progress(notification).await.is_err() {
            break;
        }
    }
}
