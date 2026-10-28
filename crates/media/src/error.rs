use crate::stretch::MIN_STRETCH_SAMPLE_RATE;
use celesta_composition::{AnimationError, TimeError};
use celesta_remote::RemoteAssetError;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("FFmpeg operation failed: {0}")]
    Ffmpeg(#[source] ez_ffmpeg::error::Error),
    #[error("media file {0} was not found")]
    NotFound(PathBuf),
    #[error("{0} has no video stream")]
    NoVideoStream(PathBuf),
    #[error("invalid video timestamp {0}")]
    InvalidTimestamp(f64),
    #[error("video frame {width}x{height} is too large")]
    FrameTooLarge { width: u32, height: u32 },
    /// A decoded frame (`width`x`height`, `actual` bytes) that is not the
    /// probed stream's size; `0x0` with no bytes when none was decoded.
    #[error(
        "decoded {width}x{height} RGBA frame has {actual} bytes, \
         expected {expected_width}x{expected_height} ({expected} bytes)"
    )]
    UnexpectedFrameSize {
        width: u32,
        height: u32,
        actual: usize,
        expected_width: u32,
        expected_height: u32,
        expected: usize,
    },
    #[error("invalid audio output format {sample_rate} Hz, {channels} channels")]
    InvalidAudioFormat { sample_rate: u32, channels: u16 },
    #[error("decoded audio has {samples} samples, not divisible by {channels} channels")]
    InvalidAudioSampleCount { samples: usize, channels: u16 },
}

#[derive(Debug, thiserror::Error)]
pub enum AudioMixError {
    #[error("audio mix was cancelled")]
    Cancelled,
    #[error("audio graph has a zero sample rate")]
    InvalidSampleRate,
    #[error("audio graph has invalid master volume {0}")]
    InvalidMasterVolume(f64),
    #[error("audio timeline is too long to mix")]
    TimelineTooLong,
    /// A pitched or pitch-preserving clip at a sample rate Signalsmith
    /// Stretch cannot run at.
    #[error(
        "pitch and preservePitch need a sample rate of at least {minimum} Hz, got {sample_rate}",
        minimum = MIN_STRETCH_SAMPLE_RATE
    )]
    PitchNeedsSampleRate { sample_rate: u32 },
    #[error("could not load audio asset `{asset}`: {source}")]
    RemoteAsset {
        asset: String,
        source: RemoteAssetError,
    },
    #[error("decoder returned {sample_rate} Hz, {channels} channels")]
    UnexpectedDecodedFormat { sample_rate: u32, channels: u16 },
    #[error("could not decode audio: {0}")]
    Media(#[from] MediaError),
    #[error("could not evaluate audio: {0}")]
    Animation(#[from] AnimationError),
    #[error("could not calculate audio time: {0}")]
    Time(#[from] TimeError),
}
