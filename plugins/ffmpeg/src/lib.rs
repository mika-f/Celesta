//! Celesta's FFmpeg media plugin: probing, decoding, encoding, and muxing
//! through the linked FFmpeg libraries (`ez-ffmpeg` for decoding,
//! libavcodec through `ffmpeg-sys-next` for encoding).
//!
//! The `celesta-codec-ffmpeg` program serves [`FfmpegCodec`] over the media
//! role of the plugin protocol (`celesta_media::protocol`), so Celesta
//! itself links no FFmpeg: an FFmpeg build with GPL components (such as
//! libx264) makes only this program GPL licensed. Its one decoder,
//! [`DECODER_ID`], reads whatever FFmpeg does; its video encoders are
//! `libx264`, `prores`, and `vp9`. Tests and tools may also use
//! [`FfmpegCodec`] in their own process, through [`install`].

mod color;
mod components;
mod converted;
mod decoder;
mod ffi;
mod h264;
mod probe;
mod sequential;
#[cfg(test)]
mod tests;
mod video_encoder;

pub use decoder::FfmpegBackend;

use celesta_media::{
    AudioBuffer, AudioDecoder, ComponentDescription, Decoder, Encoded, MediaError, MediaProbe,
    MediaProvider, MediaRegistry, ProviderFactory, RegisteredPlugin, SessionId, VideoEncoder,
    VideoEncoderOpened, VideoEncoderRequest, VideoFrame, VideoRequest,
};
use celesta_plugin_host::{MediaManifest, MediaOrigin};
use components::VideoCodec;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use video_encoder::VideoSession;

/// The id the plugin's manifest and `hello` give.
pub const PLUGIN_ID: &str = "ffmpeg";

/// The id of the plugin's one decoder, which reads whatever FFmpeg does.
pub const DECODER_ID: &str = "ffmpeg";

/// The plugin's manifest source, which declares its components.
const PLUGIN_JSON: &str = include_str!("../plugin.json");

/// A [`MediaProvider`] on the linked FFmpeg libraries.
#[derive(Default)]
pub struct FfmpegCodec {
    decoder: FfmpegBackend,
    video: HashMap<SessionId, VideoSession>,
    /// The last session number given, one count for every kind.
    last: u64,
}

impl FfmpegCodec {
    pub fn new() -> Self {
        Self::default()
    }

    /// A [`ProviderFactory`] making an [`FfmpegCodec`] in this process,
    /// whatever it is for.
    pub fn factory() -> ProviderFactory {
        Arc::new(|_kill, _use| Ok(Box::new(Self::new()) as Box<dyn MediaProvider>))
    }

    fn next_session(&mut self) -> SessionId {
        self.last += 1;
        SessionId(self.last)
    }

    fn video_session(&mut self, session: SessionId) -> Result<&mut VideoSession, MediaError> {
        self.video
            .get_mut(&session)
            .ok_or_else(|| no_session(session))
    }
}

fn no_session(session: SessionId) -> MediaError {
    MediaError::Codec(format!("no open video encoder {}", session.0))
}

/// `plugin.json`, parsed.
fn plugin_json() -> Value {
    serde_json::from_str(PLUGIN_JSON).expect("plugin.json is JSON")
}

/// What `celesta-codec-ffmpeg --manifest` prints, as one line: the
/// manifest's `id`, `version`, `roles`, and `media`, for a `cargo install`
/// of the program, which has no manifest file beside it.
pub fn printed_manifest() -> String {
    let plugin = plugin_json();
    json!({
        "id": plugin["id"],
        "version": env!("CARGO_PKG_VERSION"),
        "roles": plugin["roles"],
        "media": plugin["media"],
    })
    .to_string()
}

/// The components `plugin.json` declares.
fn manifest() -> MediaManifest {
    let (manifest, problems) = MediaManifest::from_json(plugin_json()["media"].clone());
    assert!(problems.is_empty(), "plugin.json is valid: {problems:?}");
    manifest
}

/// The plugin as a media plugin made in this process: bundled, with the
/// components its manifest declares.
pub fn registered() -> RegisteredPlugin {
    RegisteredPlugin {
        id: PLUGIN_ID.to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        origin: MediaOrigin::Bundled,
        directory: PathBuf::new(),
        executable: None,
        media: manifest(),
        factory: FfmpegCodec::factory(),
    }
}

/// Makes [`FfmpegCodec`], in this process, the media plugin every
/// `celesta_media::MediaBackend` uses: for tests and tools, which need no
/// plugin installed.
pub fn install() {
    celesta_media::set_registry(Some(Arc::new(MediaRegistry::new(vec![registered()]))));
}

/// The decoder the request names, which only [`DECODER_ID`] is.
fn check_decoder(decoder: &str) -> Result<(), MediaError> {
    if decoder == DECODER_ID {
        Ok(())
    } else {
        Err(MediaError::Codec(format!(
            "no decoder {decoder:?} in {PLUGIN_ID}"
        )))
    }
}

/// A decode's `result`: [`MediaError::Unsupported`] answers only a probe,
/// so a file FFmpeg cannot open is a plain error here.
fn decoded<T>(result: Result<T, MediaError>) -> Result<T, MediaError> {
    result.map_err(|error| match error {
        MediaError::Unsupported { path, .. } => MediaError::Codec(format!(
            "FFmpeg cannot read {}: it is not a media file FFmpeg knows",
            path.display()
        )),
        error => error,
    })
}

impl MediaProvider for FfmpegCodec {
    fn describe(&mut self) -> Result<Vec<ComponentDescription>, MediaError> {
        Ok(components::describe())
    }

    fn decoder(&mut self) -> Option<&mut dyn Decoder> {
        Some(self)
    }

    fn video_encoder(&mut self) -> Option<&mut dyn VideoEncoder> {
        Some(self)
    }
}

impl Decoder for FfmpegCodec {
    fn probe(&mut self, decoder: &str, path: &Path) -> Result<MediaProbe, MediaError> {
        check_decoder(decoder)?;
        self.decoder.probe(path).cloned()
    }

    fn decode_video(
        &mut self,
        decoder: &str,
        request: &VideoRequest<'_>,
    ) -> Result<VideoFrame, MediaError> {
        check_decoder(decoder)?;
        decoded(match request.sequential {
            Some(sequential) => self.decoder.decode_sequential(
                sequential.session,
                request.path,
                request.time,
                sequential.frame_rate,
            ),
            None => self.decoder.decode(request.path, request.time),
        })
    }

    fn decode_audio(
        &mut self,
        decoder: &str,
        path: &Path,
        sample_rate: u32,
        channels: u16,
    ) -> Result<AudioBuffer, MediaError> {
        check_decoder(decoder)?;
        decoded(self.decoder.decode_audio(path, sample_rate, channels))
    }
}

impl VideoEncoder for FfmpegCodec {
    fn open_video(
        &mut self,
        request: &VideoEncoderRequest<'_>,
    ) -> Result<VideoEncoderOpened, MediaError> {
        let codec = VideoCodec::from_id(request.component).ok_or_else(|| {
            MediaError::Codec(format!(
                "no video encoder {:?} in {PLUGIN_ID}",
                request.component
            ))
        })?;
        let opened = video_encoder::open(codec, request)?;
        let session = self.next_session();
        self.video.insert(session, opened.session);
        Ok(VideoEncoderOpened {
            session,
            input: opened.input,
            alpha: opened.alpha,
        })
    }

    fn encode_video(
        &mut self,
        session: SessionId,
        pts: i64,
        frame: Vec<u8>,
    ) -> Result<Encoded, MediaError> {
        self.video_session(session)?.encode(pts, &frame)
    }

    fn finish_video(&mut self, session: SessionId) -> Result<Encoded, MediaError> {
        let mut state = self
            .video
            .remove(&session)
            .ok_or_else(|| no_session(session))?;
        state.finish()
    }

    fn abort_video(&mut self, session: SessionId) {
        self.video.remove(&session);
    }
}

pub(crate) fn ffmpeg_error(error: ez_ffmpeg::error::Error) -> MediaError {
    MediaError::Codec(format!("FFmpeg operation failed: {error}"))
}
