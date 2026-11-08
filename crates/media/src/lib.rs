//! Media for Celesta: probing, video frames, audio decoding, and encoding
//! and muxing exports through media plugins (see [`set_registry`]), and the
//! audio mix, ducking, time stretching, and loudness Celesta does itself. No
//! codec library is linked.

mod backend;
mod color;
mod decoder;
mod duck;
mod error;
mod loudness;
mod mix;
mod process;
pub mod protocol;
mod provider;
mod registry;
mod resample;
mod ring;
pub mod settings;
mod stretch;
#[cfg(test)]
mod tests;
mod types;

pub use backend::MediaBackend;
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
pub use process::{FrameTransport, MediaProcess, SHARED_FRAME_BYTES, SHARED_MEMORY_VARIABLE};
pub use provider::{
    Answered, AudioEncoder, AudioEncoderOpened, AudioEncoderRequest, Color, ComponentDescription,
    ComponentKind, Decoder, Encoded, MediaProvider, Muxer, MuxerRequest, PIPE_WINDOW, Packet,
    Pipeline, PixelFormat, ProviderFactory, ProviderUse, SampleRates, Sequential, SessionId,
    SideData, Track, TrackConfig, TrackPacket, VideoEncoder, VideoEncoderOpened,
    VideoEncoderRequest, VideoRequest, timestamps_match,
};
pub use registry::{
    ComponentEntry, ComponentRef, DecoderUse, InvalidComponentRef, MediaComponentInfo,
    MediaRegistry, PluginDescription, RegisteredPlugin, registry, set_registry,
};
pub use resample::resample_audio;
pub use ring::{MAX_RING_BYTES, RING_SLOTS, ring_layout};
pub use settings::{
    InvalidSetting, SelectOption, SettingField, SettingKind, SettingValue, Settings,
};
pub use types::{AudioBuffer, AudioStream, MediaProbe, VideoFrame, VideoStream};
