use crate::stretch::MIN_STRETCH_SAMPLE_RATE;
use celesta_composition::{AnimationError, TimeError};
use celesta_remote::RemoteAssetError;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    /// What the media plugin said went wrong (such as an FFmpeg error), or
    /// what it answered that does not fit the request.
    #[error("{0}")]
    Codec(String),
    /// No media plugin is installed to encode with (see [`crate::set_registry`]).
    #[error("no codec plugin is installed, so video and audio files cannot be read or written")]
    NoCodec,
    /// No media decoder is installed (see [`crate::set_registry`]).
    #[error("no media decoder is installed, so video and audio files cannot be read")]
    NoDecoder,
    /// No decoder reads `path`: the decoders `tried` (by id, in order) each
    /// answered that it is not a file they read.
    #[error("no decoder could read {path} (tried {tried})", path = path.display(), tried = tried.join(", "))]
    Unsupported { path: PathBuf, tried: Vec<String> },
    /// The media plugin's process could not be started, went away, or fell
    /// out of step with its requests.
    #[error("the codec plugin is unavailable: {0}")]
    CodecUnavailable(String),
    /// The media plugin's process was stopped on purpose: it missed a
    /// request's deadline, or its kill switch was killed (an export's
    /// Cancel). Unlike [`Self::CodecUnavailable`], the request is not sent
    /// again to a new process; a later request starts one.
    #[error("the codec plugin was stopped: {0}")]
    CodecStopped(String),
    /// The codec plugin cannot write a format, as an FFmpeg build without
    /// one of its encoders cannot.
    #[error("the codec plugin has no {encoder} encoder")]
    EncoderUnavailable { encoder: String },
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
    /// A component's settings that it refused: `key` and why, for a
    /// combination only the plugin can judge, or one that failed Celesta's
    /// validation against the component's schema.
    #[error("{key}: {message}")]
    InvalidSettings { key: String, message: String },
    /// A component that turned out unusable, at `open` or when it was
    /// described.
    #[error("{component} is unavailable: {reason}")]
    ComponentUnavailable { component: String, reason: String },
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
