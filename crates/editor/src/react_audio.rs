use crate::audio::take_latest;
use celesta_composition::AudioGraph;
use celesta_react_bridge::{PropertyInputs, ReactBridge};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;

pub(crate) struct ReactAudioRequest {
    pub(crate) generation: u64,
    pub(crate) node: PathBuf,
    pub(crate) cli_script: PathBuf,
    pub(crate) entry: PathBuf,
    pub(crate) properties: PropertyInputs,
    pub(crate) sample_rate: u32,
    pub(crate) master_volume: f64,
}

pub(crate) struct ReactAudioResult {
    pub(crate) generation: u64,
    pub(crate) graph: Result<AudioGraph, String>,
}

/// Spawns a transient `@celesta/react` process, sweeps every frame of a
/// standalone entry for its `<Audio>` declarations, and returns the assembled
/// [`AudioGraph`] — the standalone-preview counterpart of the audio graph
/// `celesta-exporter` accumulates while rendering. Runs on its own thread
/// because `ReactBridge::spawn` and the per-frame sweep both block.
pub(crate) struct ReactAudioWorker {
    pub(crate) requests: mpsc::Sender<ReactAudioRequest>,
    pub(crate) results: mpsc::Receiver<ReactAudioResult>,
}

impl ReactAudioWorker {
    pub(crate) fn spawn() -> Result<Self, Box<dyn Error>> {
        let (request_tx, request_rx) = mpsc::channel::<ReactAudioRequest>();
        let (result_tx, result_rx) = mpsc::channel::<ReactAudioResult>();
        thread::Builder::new()
            .name("celesta-react-audio".to_owned())
            .spawn(move || {
                while let Ok(first) = request_rx.recv() {
                    let request = take_latest(first, &request_rx);
                    let entry_dir = request
                        .entry
                        .parent()
                        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
                    let graph = ReactBridge::spawn_with_properties(
                        &request.node,
                        &request.cli_script,
                        &request.entry,
                        &request.properties,
                    )
                    .map_err(|error| error.to_string())
                    .and_then(|mut bridge| {
                        bridge
                            .collect_audio_graph(
                                request.sample_rate,
                                request.master_volume,
                                &entry_dir,
                            )
                            .map_err(|error| error.to_string())
                    });
                    if result_tx
                        .send(ReactAudioResult {
                            generation: request.generation,
                            graph,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })?;
        Ok(Self {
            requests: request_tx,
            results: result_rx,
        })
    }

    pub(crate) fn request(&self, request: ReactAudioRequest) -> Result<(), String> {
        self.requests
            .send(request)
            .map_err(|_| "react audio worker stopped unexpectedly".to_owned())
    }
}
