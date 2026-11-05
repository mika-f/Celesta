//! Reading and writing media files through a codec plugin.
//!
//! Celesta links no codec library. Probing, decoding, encoding, and muxing
//! go through a [`CodecProvider`]: a codec plugin's process
//! ([`CodecProcess`], speaking [`protocol`]), or, in tests, a provider in
//! the same process. The application chooses the codec once with
//! [`set_codec`]; each [`MediaBackend`](crate::MediaBackend) opens its own
//! instance of the codec chosen when it first needs one. See
//! `docs/handoff/2026-11-05-codec-plugin.md`.

mod process;
pub mod protocol;

pub use process::CodecProcess;

use crate::error::MediaError;
use crate::types::{AudioBuffer, MediaProbe, VideoFrame};
use celesta_composition::Rational;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

/// What a codec plugin does, asked one request at a time. Every path is
/// absolute and local.
pub trait CodecProvider: Send {
    fn probe(&mut self, path: &Path) -> Result<MediaProbe, MediaError>;

    /// The video frame shown at `request.time` seconds into the file, as
    /// tightly packed straight-alpha RGBA8 at the probed (displayed) size.
    fn decode_video(&mut self, request: &VideoRequest<'_>) -> Result<VideoFrame, MediaError>;

    /// The file's first audio stream, whole, as interleaved `f32` samples
    /// at `sample_rate` with `channels` channels (1 or 2); no samples when
    /// it has no audio.
    fn decode_audio(
        &mut self,
        path: &Path,
        sample_rate: u32,
        channels: u16,
    ) -> Result<AudioBuffer, MediaError>;

    /// Fails with [`MediaError::EncoderUnavailable`] when the codec cannot
    /// write `container`'s video or audio.
    fn check_encoder(&mut self, container: Container) -> Result<(), MediaError>;

    /// Starts writing a video-only file that frames are then pushed to.
    fn open_encoder(&mut self, settings: &EncoderSettings) -> Result<EncoderId, MediaError>;

    /// Encodes the next frame: one frame of the encoder's input format.
    fn encode_frame(&mut self, encoder: EncoderId, pixels: Vec<u8>) -> Result<(), MediaError>;

    /// Flushes the encoder and finishes its file.
    fn finish_encoder(&mut self, encoder: EncoderId) -> Result<(), MediaError>;

    /// Stops the encoder, leaving whatever it wrote for the caller to
    /// remove.
    fn abort_encoder(&mut self, encoder: EncoderId);

    /// Writes `request.output`: the video of `request.video`, copied, with
    /// the PCM of `request.audio` encoded in the container's audio codec.
    fn mux_audio(&mut self, request: &MuxRequest) -> Result<(), MediaError>;
}

/// A frame request. With `sequential`, requests of the same session are
/// expected to move forward at about its frame rate, and the codec may keep
/// a decode run open between them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VideoRequest<'a> {
    pub path: &'a Path,
    pub time: f64,
    pub sequential: Option<Sequential<'a>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sequential<'a> {
    /// Which run of requests this is: a renderer asks for each layer
    /// showing the file on its own.
    pub session: &'a str,
    /// The spacing expected between requests until the session has shown
    /// its own.
    pub frame_rate: Rational,
}

/// An encoder the codec opened, by the codec's own number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EncoderId(pub u64);

/// The container an export writes, and with it the codecs: H.264 and AAC
/// in MP4, ProRes 4444 and 24-bit PCM in MOV, VP9 with alpha and Opus in
/// WebM.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Container {
    Mp4,
    Mov,
    Webm,
}

/// The layout of the frames pushed to an encoder.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PixelFormat {
    /// RGBA, 8 bits a channel.
    Rgba8,
    /// RGBA, 16-bit little-endian channels.
    Rgba16,
    /// Planar BT.601 limited-range Y, U, V with chroma halved both ways.
    Yuv420p,
    /// [`PixelFormat::Yuv420p`] with 10 bits in 16-bit little-endian samples.
    Yuv420p10,
}

/// What an encoder writes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncoderSettings {
    pub output: PathBuf,
    pub width: u32,
    pub height: u32,
    pub frame_rate: Rational,
    pub input: PixelFormat,
    pub container: Container,
    /// libx264's `-preset` name; MP4 only.
    pub preset: String,
    /// H.264's constant rate factor; MP4 only.
    pub crf: u8,
    /// 8, or 10 for High 10; MP4 only.
    pub bit_depth: u8,
}

/// What [`CodecProvider::mux_audio`] writes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MuxRequest {
    /// The encoded, audio-less video.
    pub video: PathBuf,
    /// Raw interleaved `f32` little-endian PCM.
    pub audio: PathBuf,
    pub sample_rate: u32,
    pub channels: u16,
    pub output: PathBuf,
    pub container: Container,
    /// Sample frames at the start of `audio` that play before the video's
    /// start, as a looping MP4's lead-in does: the muxer hides them with an
    /// edit list.
    pub lead_in_frames: usize,
}

/// Makes a codec instance, each time a [`crate::MediaBackend`] needs one.
pub type CodecFactory = Arc<dyn Fn() -> Result<Box<dyn CodecProvider>, MediaError> + Send + Sync>;

static CODEC: RwLock<Option<CodecFactory>> = RwLock::new(None);

/// Chooses the codec [`crate::MediaBackend::new`]'s backends open: each opens the
/// one chosen when it first needs one, so a backend made earlier that has
/// not opened one yet gets this choice, and one already open keeps its own.
/// `None` leaves none, so reading or writing media fails with
/// [`MediaError::NoCodec`].
pub fn set_codec(factory: Option<CodecFactory>) {
    *CODEC
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = factory;
}

/// The codec [`set_codec`] chose.
pub fn codec() -> Option<CodecFactory> {
    CODEC
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}
