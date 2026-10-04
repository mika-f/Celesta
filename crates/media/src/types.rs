use celesta_composition::{Rational, Time};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaProbe {
    pub duration: Option<Time>,
    pub video: Option<VideoStream>,
    pub audio: Vec<AudioStream>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoStream {
    pub index: u32,
    pub codec: Option<String>,
    pub width: u32,
    pub height: u32,
    pub frame_rate: Option<Rational>,
    pub duration: Option<Time>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioStream {
    pub index: u32,
    pub codec: Option<String>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
    pub duration: Option<Time>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    /// Shared so a decoder can keep the frame it last served (to answer a
    /// repeat request or freeze past the source end) without copying a full
    /// RGBA frame — 8MB at 1080p — for every frame it hands out.
    pub pixels: Arc<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AudioBuffer {
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}

impl AudioBuffer {
    pub fn frame_count(&self) -> usize {
        self.samples.len() / usize::from(self.channels)
    }
}
