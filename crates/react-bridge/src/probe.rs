use celesta_composition::{Rational, Time};
use celesta_media::{AudioStream, MediaProbe, VideoStream};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub(crate) struct MediaProbeRequest {
    /// A local path, or an `http`/`https` URL.
    pub(crate) path: String,
}

#[derive(Serialize)]
pub(crate) struct MediaProbeResponse<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) media: Option<MediaProbePayload<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MediaProbePayload<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) duration_seconds: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) video: Option<VideoProbePayload<'a>>,
    pub(crate) audio: Vec<AudioProbePayload<'a>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VideoProbePayload<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) codec: Option<&'a str>,
    pub(crate) width: u32,
    pub(crate) height: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) frame_rate: Option<Rational>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) duration_seconds: Option<f64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AudioProbePayload<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) codec: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) sample_rate: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) channels: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) duration_seconds: Option<f64>,
}

pub(crate) fn media_probe_payload(probe: &MediaProbe) -> MediaProbePayload<'_> {
    MediaProbePayload {
        duration_seconds: seconds(probe.duration),
        video: probe.video.as_ref().map(video_probe_payload),
        audio: probe.audio.iter().map(audio_probe_payload).collect(),
    }
}

pub(crate) fn video_probe_payload(video: &VideoStream) -> VideoProbePayload<'_> {
    VideoProbePayload {
        codec: video.codec.as_deref(),
        width: video.width,
        height: video.height,
        frame_rate: video.frame_rate,
        duration_seconds: seconds(video.duration),
    }
}

pub(crate) fn audio_probe_payload(audio: &AudioStream) -> AudioProbePayload<'_> {
    AudioProbePayload {
        codec: audio.codec.as_deref(),
        sample_rate: audio.sample_rate,
        channels: audio.channels,
        duration_seconds: seconds(audio.duration),
    }
}

pub(crate) fn seconds(time: Option<Time>) -> Option<f64> {
    time.and_then(|time| time.as_seconds().ok())
}
