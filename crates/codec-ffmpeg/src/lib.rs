//! Celesta's FFmpeg codec plugin: probing, decoding, encoding, and muxing
//! through the linked FFmpeg libraries (`ez-ffmpeg`).
//!
//! The `celesta-codec-ffmpeg` program serves [`FfmpegCodec`] over the codec
//! protocol (`celesta_media::codec::protocol`), so Celesta itself links no
//! FFmpeg: an FFmpeg build with GPL components (such as libx264) makes only
//! this program GPL licensed. Tests and tools may also use [`FfmpegCodec`]
//! in their own process, through [`install`].

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
    AudioBuffer, AudioDecoder, CodecFactory, CodecProvider, Container, EncoderId, EncoderSettings,
    MediaError, MediaProbe, MuxRequest, VideoFrame, VideoRequest,
};
use ez_ffmpeg::VideoWriter;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// The id the plugin's manifest and `hello` give.
pub const PLUGIN_ID: &str = "ffmpeg";

/// A [`CodecProvider`] on the linked FFmpeg libraries.
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

    /// A [`CodecFactory`] making an [`FfmpegCodec`] in this process.
    pub fn factory() -> CodecFactory {
        Arc::new(|| Ok(Box::new(Self::new()) as Box<dyn CodecProvider>))
    }

    fn encoder(&mut self, encoder: EncoderId) -> Result<&mut VideoWriter, MediaError> {
        self.encoders
            .get_mut(&encoder)
            .ok_or_else(|| MediaError::Codec(format!("no open encoder {}", encoder.0)))
    }
}

/// Makes [`FfmpegCodec`], in this process, the codec every
/// `celesta_media::MediaBackend` uses: for tests and tools, which need no
/// plugin installed.
pub fn install() {
    celesta_media::set_codec(Some(FfmpegCodec::factory()));
}

impl CodecProvider for FfmpegCodec {
    fn probe(&mut self, path: &Path) -> Result<MediaProbe, MediaError> {
        self.decoder.probe(path).cloned()
    }

    fn decode_video(&mut self, request: &VideoRequest<'_>) -> Result<VideoFrame, MediaError> {
        match request.sequential {
            Some(sequential) => self.decoder.decode_sequential(
                sequential.session,
                request.path,
                request.time,
                sequential.frame_rate,
            ),
            None => self.decoder.decode(request.path, request.time),
        }
    }

    fn decode_audio(
        &mut self,
        path: &Path,
        sample_rate: u32,
        channels: u16,
    ) -> Result<AudioBuffer, MediaError> {
        self.decoder.decode_audio(path, sample_rate, channels)
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
