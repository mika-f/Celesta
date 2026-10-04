use crate::audio::take_latest;
use crate::waveform::media_asset_info;
use celesta_composition::Time;
use celesta_media::FfmpegBackend;
use std::error::Error;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

pub(crate) struct MediaProbeRequest {
    pub(crate) generation: u64,
    pub(crate) assets: Vec<ProbeAsset>,
}

pub(crate) struct ProbeAsset {
    pub(crate) id: String,
    pub(crate) path: PathBuf,
}

pub(crate) struct MediaProbeResult {
    pub(crate) generation: u64,
    pub(crate) assets: Vec<(String, Result<MediaAssetInfo, String>)>,
}

#[derive(Clone)]
pub(crate) struct MediaAssetInfo {
    pub(crate) duration: Option<Time>,
    pub(crate) video_size: Option<(u32, u32)>,
    pub(crate) has_audio: bool,
}

pub(crate) struct MediaProbeWorker {
    pub(crate) requests: mpsc::Sender<MediaProbeRequest>,
    pub(crate) results: mpsc::Receiver<MediaProbeResult>,
}

impl MediaProbeWorker {
    pub(crate) fn spawn() -> Result<Self, Box<dyn Error>> {
        let (request_tx, request_rx) = mpsc::channel::<MediaProbeRequest>();
        let (result_tx, result_rx) = mpsc::channel::<MediaProbeResult>();
        thread::Builder::new()
            .name("celesta-media-probe".to_owned())
            .spawn(move || {
                let mut backend = FfmpegBackend::new();
                while let Ok(first) = request_rx.recv() {
                    let request = take_latest(first, &request_rx);
                    let assets = request
                        .assets
                        .into_iter()
                        .map(|asset| {
                            let result = backend
                                .probe(&asset.path)
                                .map(media_asset_info)
                                .map_err(|error| error.to_string());
                            (asset.id, result)
                        })
                        .collect();
                    if result_tx
                        .send(MediaProbeResult {
                            generation: request.generation,
                            assets,
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

    pub(crate) fn request(&self, request: MediaProbeRequest) -> Result<(), String> {
        self.requests
            .send(request)
            .map_err(|_| "media probe worker stopped unexpectedly".to_owned())
    }
}
