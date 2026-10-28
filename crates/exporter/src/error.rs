use crate::options::{VideoEncoding, VideoFormat};
use celesta_composition::TimeError;
use celesta_evaluator::EvaluationError;
use celesta_gpu_renderer::GpuRenderError;
use celesta_media::AudioMixError;
use celesta_project::LoadError;
use celesta_react_bridge::{PropertyInputError, ReactBridgeError};
use std::io;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("could not load export project: {0}")]
    Project(#[source] LoadError),
    #[error("could not evaluate export: {0}")]
    Evaluation(#[source] EvaluationError),
    #[error("could not render export: {0}")]
    Render(#[source] GpuRenderError),
    #[error("could not mix export audio: {0}")]
    Audio(#[source] AudioMixError),
    #[error("could not evaluate React export: {0}")]
    React(#[source] ReactBridgeError),
    /// `--props` or `--props-file` is not a readable JSON object.
    #[error("could not read project properties: {0}")]
    Properties(#[source] PropertyInputError),
    #[error("could not calculate export time: {0}")]
    Time(#[source] TimeError),
    #[error("could not {operation}: {source}")]
    Io {
        operation: &'static str,
        source: io::Error,
    },
    #[error("FFmpeg {stage} failed: {source}")]
    Ffmpeg {
        stage: &'static str,
        source: ez_ffmpeg::error::Error,
    },
    #[error("output already exists: {0}")]
    OutputExists(PathBuf),
    /// The still frame selection, its output name or quality, or its
    /// contact sheet layout cannot be exported.
    #[error("could not export still frames: {0}")]
    InvalidSelection(String),
    /// The output path's extension is not the one `format` writes.
    #[error("export output must use the .{format} extension: {path}")]
    UnsupportedOutput { path: PathBuf, format: VideoFormat },
    /// The subtitle file's extension is neither `.srt` nor `.vtt`.
    #[error("subtitles must use the .srt or .vtt extension: {0}")]
    UnsupportedSubtitles(PathBuf),
    /// Subtitles in several languages would go to one file, a path without
    /// `{lang}`.
    #[error(
        "subtitles in several languages ({}) cannot share one file: {path}",
        .languages.join(", ")
    )]
    MixedSubtitleLanguages {
        path: PathBuf,
        /// Each language, `(none)` for subtitles without one.
        languages: Vec<String>,
    },
    /// An export with neither a video nor a subtitle file to write.
    #[error("an export needs a video output or a subtitle file to write")]
    NoOutput,
    /// The linked FFmpeg build lacks an encoder `format` needs.
    #[error("this FFmpeg build has no {encoder} encoder, which {format} export needs")]
    EncoderUnavailable {
        format: VideoFormat,
        encoder: &'static str,
    },
    #[error("video export requires non-zero even dimensions, got {width}x{height}")]
    UnsupportedDimensions { width: u32, height: u32 },
    /// [`VideoEncoding::crf`] is above [`VideoEncoding::MAX_CRF`].
    #[error("H.264 CRF must be between 0 and {max}, got {0}", max = VideoEncoding::MAX_CRF)]
    InvalidCrf(u8),
    #[error("cannot export an empty timeline")]
    EmptyTimeline,
    /// The requested export range, once clamped to the composition and
    /// snapped to frames, covers zero frames.
    #[error("the requested export range does not cover any frames")]
    EmptyRange,
    #[error("export was cancelled")]
    Cancelled,
    #[error("export timeline is too long")]
    TimelineTooLong,
    /// One composition frame could not be evaluated, rendered or written.
    #[error(transparent)]
    Frame(Box<FrameFailure>),
    /// A contact sheet was written with failure tiles in place of the frames
    /// in `failures`, which could not be evaluated or rendered; the others
    /// rendered.
    #[error(
        "{} of {frames} contact sheet frames failed; {path} shows them as failed tiles:{}",
        .failures.len(),
        failure_lines(.failures)
    )]
    FramesFailed {
        path: PathBuf,
        /// Tiles on the sheet, failed ones included.
        frames: usize,
        failures: Vec<FrameFailure>,
    },
}

/// Why zero-based composition frame `frame` failed.
#[derive(Debug, thiserror::Error)]
#[error("frame {frame}: {error}")]
pub struct FrameFailure {
    pub frame: u64,
    #[source]
    pub error: ExportError,
}

fn failure_lines(failures: &[FrameFailure]) -> String {
    failures
        .iter()
        .map(|failure| format!("\n  {failure}"))
        .collect()
}

impl ExportError {
    /// `self` as the failure of composition frame `frame`.
    pub(crate) fn at_frame(self, frame: u64) -> Self {
        match self {
            Self::Cancelled | Self::Frame(_) => self,
            error => Self::Frame(Box::new(FrameFailure { frame, error })),
        }
    }

    /// Whether the error is about one frame's content, so the frames after
    /// it can still be evaluated and rendered: a scene that failed to
    /// evaluate or render, not a React runtime that stopped answering, a GPU
    /// or readback failure, or a file that could not be written.
    pub(crate) fn is_frame_scoped(&self) -> bool {
        match self {
            Self::Evaluation(_)
            | Self::React(ReactBridgeError::Render(_) | ReactBridgeError::InvalidDuck { .. }) => {
                true
            }
            Self::Render(error) => error.is_scene_error(),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_failure_is_named_once() {
        let error = ExportError::EmptyTimeline.at_frame(4).at_frame(9);
        assert!(matches!(&error, ExportError::Frame(failure) if failure.frame == 4));
        assert_eq!(
            error.to_string(),
            "frame 4: cannot export an empty timeline"
        );
        assert!(matches!(
            ExportError::Cancelled.at_frame(4),
            ExportError::Cancelled
        ));
    }

    #[test]
    fn only_a_frames_own_content_lets_the_next_frame_render() {
        assert!(ExportError::React(ReactBridgeError::Render("bad fill".into())).is_frame_scoped());
        assert!(
            !ExportError::React(ReactBridgeError::UnexpectedExit {
                stderr: String::new()
            })
            .is_frame_scoped()
        );
        assert!(
            !ExportError::Io {
                operation: "publish image",
                source: io::Error::other("disk full"),
            }
            .is_frame_scoped()
        );
        assert!(!ExportError::Cancelled.is_frame_scoped());
    }

    #[test]
    fn a_gpu_failure_stops_a_contact_sheet_but_bad_content_does_not() {
        let shader = GpuRenderError::Shader {
            shader: "tint".to_owned(),
            message: "does not compile".to_owned(),
        };
        assert!(ExportError::Render(shader).is_frame_scoped());
        assert!(ExportError::Render(GpuRenderError::TooManyLayers(1 << 20)).is_frame_scoped());
        for error in [
            GpuRenderError::MapCallbackDropped,
            GpuRenderError::ReadbackThread(io::Error::other("no threads")),
            GpuRenderError::SurfaceValidation,
        ] {
            assert!(!ExportError::Render(error).is_frame_scoped());
        }
    }
}
