//! Media probing and frame decoding through the linked FFmpeg libraries
//! (`ez-ffmpeg`).

mod decoder;
mod error;
mod ffmpeg;
mod mix;
mod probe;
mod sequential;
#[cfg(test)]
mod tests;
mod types;

pub use decoder::{AudioDecoder, VideoFrameDecoder};
pub use error::{AudioMixError, MediaError};
pub use ffmpeg::FfmpegBackend;
pub use mix::{mix_audio_graph, mix_audio_graph_cancellable};
pub use types::{AudioBuffer, AudioStream, MediaProbe, VideoFrame, VideoStream};
