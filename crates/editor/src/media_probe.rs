use crate::problems::error_chain;
use crate::waveform::media_asset_info;
use crate::worker::LatestWorker;
use celesta_composition::Time;
use celesta_media::{DynamicRange, FfmpegBackend};
use std::path::PathBuf;

pub(crate) struct MediaProbeRequest {
    pub(crate) assets: Vec<ProbeAsset>,
}

pub(crate) struct ProbeAsset {
    pub(crate) id: String,
    pub(crate) path: PathBuf,
}

/// Each asset's id and what probing it found.
pub(crate) type MediaProbeResult = Vec<(String, Result<MediaAssetInfo, String>)>;

#[derive(Clone)]
pub(crate) struct MediaAssetInfo {
    pub(crate) duration: Option<Time>,
    pub(crate) video_size: Option<(u32, u32)>,
    pub(crate) dynamic_range: DynamicRange,
    pub(crate) has_audio: bool,
}

pub(crate) type MediaProbeWorker = LatestWorker<MediaProbeRequest, MediaProbeResult>;

pub(crate) fn spawn_media_probe() -> anyhow::Result<MediaProbeWorker> {
    let mut backend = FfmpegBackend::new();
    LatestWorker::spawn("media probe", move |request: MediaProbeRequest, _| {
        request
            .assets
            .into_iter()
            .map(|asset| {
                let result = backend
                    .probe(&asset.path)
                    .map(media_asset_info)
                    .map_err(|error| error_chain(&error));
                (asset.id, result)
            })
            .collect()
    })
}
