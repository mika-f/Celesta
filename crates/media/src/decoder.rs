use crate::error::MediaError;
use crate::types::{AudioBuffer, VideoFrame};
use std::path::Path;

pub trait VideoFrameDecoder: Send {
    fn decode_frame(
        &mut self,
        path: &Path,
        source_time_seconds: f64,
    ) -> Result<VideoFrame, MediaError>;

    fn decode_frame_for(
        &mut self,
        request_id: &str,
        path: &Path,
        source_time_seconds: f64,
    ) -> Result<VideoFrame, MediaError> {
        let _ = request_id;
        self.decode_frame(path, source_time_seconds)
    }
}

pub trait AudioDecoder {
    fn decode_audio(
        &mut self,
        path: &Path,
        sample_rate: u32,
        channels: u16,
    ) -> Result<AudioBuffer, MediaError>;
}
