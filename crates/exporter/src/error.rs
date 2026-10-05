use crate::options::VideoEncoding;
use celesta_composition::TimeError;
use celesta_evaluator::EvaluationError;
use celesta_gpu_renderer::GpuRenderError;
use celesta_media::AudioMixError;
use celesta_project::LoadError;
use celesta_react_bridge::ReactBridgeError;
use std::error::Error;
use std::path::PathBuf;
use std::{fmt, io};

#[derive(Debug)]
pub enum ExportError {
    Project(LoadError),
    Evaluation(EvaluationError),
    Render(GpuRenderError),
    Audio(AudioMixError),
    React(ReactBridgeError),
    Time(TimeError),
    Io {
        operation: &'static str,
        source: io::Error,
    },
    Ffmpeg {
        stage: &'static str,
        source: ez_ffmpeg::error::Error,
    },
    OutputExists(PathBuf),
    /// The PNG frame selection, its output name, or its contact sheet
    /// layout cannot be exported.
    InvalidSelection(String),
    UnsupportedOutput(PathBuf),
    UnsupportedDimensions {
        width: u32,
        height: u32,
    },
    /// [`VideoEncoding::crf`] is above [`VideoEncoding::MAX_CRF`].
    InvalidCrf(u8),
    EmptyTimeline,
    /// The requested export range, once clamped to the composition and
    /// snapped to frames, covers zero frames.
    EmptyRange,
    Cancelled,
    TimelineTooLong,
}

impl fmt::Display for ExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Project(error) => write!(formatter, "could not load export project: {error}"),
            Self::Evaluation(error) => write!(formatter, "could not evaluate export: {error}"),
            Self::Render(error) => write!(formatter, "could not render export: {error}"),
            Self::Audio(error) => write!(formatter, "could not mix export audio: {error}"),
            Self::React(error) => write!(formatter, "could not evaluate React export: {error}"),
            Self::Time(error) => write!(formatter, "could not calculate export time: {error}"),
            Self::Io { operation, source } => write!(formatter, "could not {operation}: {source}"),
            Self::Ffmpeg { stage, source } => {
                write!(formatter, "FFmpeg {stage} failed: {source}")
            }
            Self::OutputExists(path) => {
                write!(formatter, "output already exists: {}", path.display())
            }
            Self::InvalidSelection(message) => {
                write!(formatter, "could not export PNG frames: {message}")
            }
            Self::UnsupportedOutput(path) => write!(
                formatter,
                "export output must use the .mp4 extension: {}",
                path.display()
            ),
            Self::UnsupportedDimensions { width, height } => write!(
                formatter,
                "H.264 MP4 export requires non-zero even dimensions, got {width}x{height}"
            ),
            Self::InvalidCrf(crf) => write!(
                formatter,
                "H.264 CRF must be between 0 and {}, got {crf}",
                VideoEncoding::MAX_CRF
            ),
            Self::EmptyTimeline => formatter.write_str("cannot export an empty timeline"),
            Self::EmptyRange => {
                formatter.write_str("the requested export range does not cover any frames")
            }
            Self::Cancelled => formatter.write_str("export was cancelled"),
            Self::TimelineTooLong => formatter.write_str("export timeline is too long"),
        }
    }
}

impl Error for ExportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Project(error) => Some(error),
            Self::Evaluation(error) => Some(error),
            Self::Render(error) => Some(error),
            Self::Audio(error) => Some(error),
            Self::React(error) => Some(error),
            Self::Time(error) => Some(error),
            Self::Io { source, .. } => Some(source),
            Self::Ffmpeg { source, .. } => Some(source),
            Self::OutputExists(_)
            | Self::InvalidSelection(_)
            | Self::UnsupportedOutput(_)
            | Self::UnsupportedDimensions { .. }
            | Self::InvalidCrf(_)
            | Self::EmptyTimeline
            | Self::EmptyRange
            | Self::Cancelled
            | Self::TimelineTooLong => None,
        }
    }
}
