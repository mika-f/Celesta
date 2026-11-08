//! Celesta's FFmpeg media plugin: probing, decoding, encoding, and muxing
//! through the linked FFmpeg libraries (`ez-ffmpeg`).
//!
//! The `celesta-codec-ffmpeg` program serves [`FfmpegCodec`] over the media
//! role of the plugin protocol (`celesta_media::codec::protocol`), so
//! Celesta itself links no FFmpeg: an FFmpeg build with GPL components (such
//! as libx264) makes only this program GPL licensed. Its one decoder,
//! [`DECODER_ID`], reads whatever FFmpeg does. Tests and tools may also use
//! [`FfmpegCodec`] in their own process, through [`install`].

mod color;
mod converted;
mod decoder;
mod encoder;
mod probe;
mod sequential;
#[cfg(test)]
mod tests;

pub use decoder::FfmpegBackend;

use celesta_media::{
    AudioBuffer, AudioDecoder, Container, EncoderId, EncoderSettings, MediaError, MediaProbe,
    MediaProvider, MediaRegistry, MuxRequest, ProviderFactory, RegisteredPlugin, VideoFrame,
    VideoRequest,
};
use celesta_plugin_host::{MediaDecoderEntry, MediaOrigin};
use ez_ffmpeg::VideoWriter;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// The id the plugin's manifest and `hello` give.
pub const PLUGIN_ID: &str = "ffmpeg";

/// The id of the plugin's one decoder, which reads whatever FFmpeg does.
pub const DECODER_ID: &str = "ffmpeg";

/// The plugin's manifest source, which declares its decoder.
const PLUGIN_JSON: &str = include_str!("../plugin.json");

/// A [`MediaProvider`] on the linked FFmpeg libraries.
#[derive(Default)]
pub struct FfmpegCodec {
    decoder: FfmpegBackend,
    encoders: HashMap<EncoderId, VideoWriter>,
    next: u64,
}

impl FfmpegCodec {
    pub fn new() -> Self {
        Self::default()
    }

    /// A [`ProviderFactory`] making an [`FfmpegCodec`] in this process.
    pub fn factory() -> ProviderFactory {
        Arc::new(|_| Ok(Box::new(Self::new()) as Box<dyn MediaProvider>))
    }

    fn encoder(&mut self, encoder: EncoderId) -> Result<&mut VideoWriter, MediaError> {
        self.encoders
            .get_mut(&encoder)
            .ok_or_else(|| MediaError::Codec(format!("no open encoder {}", encoder.0)))
    }
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

/// The decoders `plugin.json` declares.
fn decoders() -> Vec<MediaDecoderEntry> {
    serde_json::from_value(plugin_json()["media"]["decoders"].clone())
        .expect("plugin.json declares its decoders")
}

/// The plugin as a media plugin made in this process: bundled, with the
/// decoders its manifest declares.
pub fn registered() -> RegisteredPlugin {
    RegisteredPlugin {
        id: PLUGIN_ID.to_owned(),
        origin: MediaOrigin::Bundled,
        decoders: decoders(),
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

    fn check_encoder(&mut self, container: Container) -> Result<(), MediaError> {
        encoder::check_encoder(container)
    }

    fn open_encoder(&mut self, settings: &EncoderSettings) -> Result<EncoderId, MediaError> {
        let writer = encoder::open_video_writer(settings)?;
        self.next += 1;
        let id = EncoderId(self.next);
        self.encoders.insert(id, writer);
        Ok(id)
    }

    fn encode_frame(&mut self, encoder: EncoderId, pixels: Vec<u8>) -> Result<(), MediaError> {
        encoder::write_frame(self.encoder(encoder)?, pixels)
    }

    fn finish_encoder(&mut self, encoder: EncoderId) -> Result<(), MediaError> {
        let writer = self
            .encoders
            .remove(&encoder)
            .ok_or_else(|| MediaError::Codec(format!("no open encoder {}", encoder.0)))?;
        encoder::finish(writer)
    }

    fn abort_encoder(&mut self, encoder: EncoderId) {
        if let Some(writer) = self.encoders.remove(&encoder) {
            writer.abort();
        }
    }

    fn mux_audio(&mut self, request: &MuxRequest) -> Result<(), MediaError> {
        encoder::mux_audio(request)
    }
}

pub(crate) fn ffmpeg_error(error: ez_ffmpeg::error::Error) -> MediaError {
    MediaError::Codec(format!("FFmpeg operation failed: {error}"))
}
