//! Media for Celesta: probing, video frames, audio decoding, and encoding
//! and muxing exports through a codec plugin (see [`set_codec`]), and the
//! audio mix, ducking, time stretching, and loudness Celesta does itself. No
//! codec library is linked.

mod backend;
pub mod codec;
mod color;
mod decoder;
mod duck;
mod error;
mod loudness;
mod mix;
mod stretch;
#[cfg(test)]
mod tests;
mod types;

pub use backend::MediaBackend;
pub use codec::{
    CodecFactory, CodecProcess, CodecProvider, Container, EncoderId, EncoderSettings, MuxRequest,
    PixelFormat, Sequential, VideoRequest, set_codec, timestamps_match,
};
pub use color::{DynamicRange, VideoColor};
pub use decoder::{AudioDecoder, VideoFrameDecoder};
pub use duck::{DuckCurves, audible_end};
pub use error::{AudioMixError, MediaError};
pub use loudness::{
    Loudness, LoudnessTarget, PeakHandling, TARGET_TOLERANCE_LU, measure_loudness,
    normalize_loudness,
};
pub use mix::{
    frame_byte_len, mix_audio_graph, mix_audio_graph_cancellable,
    mix_audio_graph_unclipped_cancellable, mix_audio_graph_with_ducks_cancellable, video_seconds,
};
pub use types::{AudioBuffer, AudioStream, MediaProbe, VideoFrame, VideoStream};
