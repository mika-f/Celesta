use celesta_composition::{AnimationError, TimeError};
use celesta_remote::RemoteAssetError;
use std::error::Error;
use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
pub enum MediaError {
    Ffmpeg(ez_ffmpeg::error::Error),
    NoVideoStream(PathBuf),
    InvalidTimestamp(f64),
    FrameTooLarge {
        width: u32,
        height: u32,
    },
    UnexpectedFrameSize {
        width: u32,
        height: u32,
        expected: usize,
        actual: usize,
    },
    InvalidAudioFormat {
        sample_rate: u32,
        channels: u16,
    },
    InvalidAudioSampleCount {
        samples: usize,
        channels: u16,
    },
}

impl fmt::Display for MediaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ffmpeg(error) => write!(formatter, "FFmpeg operation failed: {error}"),
            Self::NoVideoStream(path) => {
                write!(formatter, "{} has no video stream", path.display())
            }
            Self::InvalidTimestamp(time) => write!(formatter, "invalid video timestamp {time}"),
            Self::FrameTooLarge { width, height } => {
                write!(formatter, "video frame {width}x{height} is too large")
            }
            Self::UnexpectedFrameSize {
                width,
                height,
                expected,
                actual,
            } => write!(
                formatter,
                "decoded {width}x{height} RGBA frame has {actual} bytes, expected {expected}"
            ),
            Self::InvalidAudioFormat {
                sample_rate,
                channels,
            } => write!(
                formatter,
                "invalid audio output format {sample_rate} Hz, {channels} channels"
            ),
            Self::InvalidAudioSampleCount { samples, channels } => write!(
                formatter,
                "decoded audio has {samples} samples, not divisible by {channels} channels"
            ),
        }
    }
}

impl Error for MediaError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Ffmpeg(error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum AudioMixError {
    Cancelled,
    InvalidSampleRate,
    InvalidMasterVolume(f64),
    TimelineTooLong,
    RemoteAsset {
        asset: String,
        source: RemoteAssetError,
    },
    UnexpectedDecodedFormat {
        sample_rate: u32,
        channels: u16,
    },
    Media(MediaError),
    Animation(AnimationError),
    Time(TimeError),
}

impl fmt::Display for AudioMixError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("audio mix was cancelled"),
            Self::InvalidSampleRate => formatter.write_str("audio graph has a zero sample rate"),
            Self::InvalidMasterVolume(volume) => {
                write!(formatter, "audio graph has invalid master volume {volume}")
            }
            Self::TimelineTooLong => formatter.write_str("audio timeline is too long to mix"),
            Self::RemoteAsset { asset, source } => {
                write!(formatter, "could not load audio asset `{asset}`: {source}")
            }
            Self::UnexpectedDecodedFormat {
                sample_rate,
                channels,
            } => write!(
                formatter,
                "decoder returned {sample_rate} Hz, {channels} channels"
            ),
            Self::Media(error) => write!(formatter, "could not decode audio: {error}"),
            Self::Animation(error) => write!(formatter, "could not evaluate audio: {error}"),
            Self::Time(error) => write!(formatter, "could not calculate audio time: {error}"),
        }
    }
}

impl Error for AudioMixError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Media(error) => Some(error),
            Self::Animation(error) => Some(error),
            Self::Time(error) => Some(error),
            Self::RemoteAsset { source, .. } => Some(source),
            Self::Cancelled
            | Self::InvalidSampleRate
            | Self::InvalidMasterVolume(_)
            | Self::TimelineTooLong
            | Self::UnexpectedDecodedFormat { .. } => None,
        }
    }
}

impl From<MediaError> for AudioMixError {
    fn from(error: MediaError) -> Self {
        Self::Media(error)
    }
}

impl From<AnimationError> for AudioMixError {
    fn from(error: AnimationError) -> Self {
        Self::Animation(error)
    }
}

impl From<TimeError> for AudioMixError {
    fn from(error: TimeError) -> Self {
        Self::Time(error)
    }
}
