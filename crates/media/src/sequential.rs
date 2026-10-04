use crate::error::MediaError;
use crate::ffmpeg::SequentialVideoSession;
use crate::mix::frame_byte_len;
use crate::types::VideoFrame;
use ez_ffmpeg::Input;
use ez_ffmpeg::frame_export::{FrameExtractor, PixelLayout, VideoFrame as EzVideoFrame};
use std::path::Path;
use std::sync::Arc;

impl SequentialVideoSession {
    pub(crate) fn spawn(
        path: &Path,
        source_time_seconds: f64,
        step_seconds: f64,
        width: u32,
        height: u32,
    ) -> Result<Self, MediaError> {
        let frames = FrameExtractor::new(Input::from(path_to_url(path)))
            .start_time_us(seconds_to_us(source_time_seconds))
            .pixel(PixelLayout::Rgba32)
            .frames()
            .map_err(MediaError::Ffmpeg)?;
        Ok(Self {
            frames,
            width,
            height,
            start_seconds: source_time_seconds,
            step_seconds,
            last_timestamp: None,
            last_frame_seconds: None,
            last_frame: None,
        })
    }

    /// Walks the open decode run forward to the first frame at or after
    /// `target_seconds`. A clean end of stream freezes on the last decoded
    /// frame (a clip whose window outruns its file); only a run that never
    /// produced any frame stays an error.
    pub(crate) fn read_frame(&mut self, target_seconds: f64) -> Result<VideoFrame, MediaError> {
        loop {
            // The frame already in hand is at or past the request (a
            // near-repeat that slipped the exact-match fast path): reuse it.
            if let (Some(seconds), Some(frame)) = (self.last_frame_seconds, &self.last_frame)
                && reached(seconds, target_seconds)
            {
                let frame = frame.clone();
                self.last_timestamp = Some(target_seconds);
                return Ok(frame);
            }
            match self.frames.next() {
                None => {
                    if let Some(frame) = self.last_frame.clone() {
                        self.last_timestamp = Some(target_seconds);
                        return Ok(frame);
                    }
                    return Err(MediaError::UnexpectedFrameSize {
                        width: self.width,
                        height: self.height,
                        expected: frame_byte_len(self.width, self.height)?,
                        actual: 0,
                    });
                }
                Some(Err(error)) => return Err(MediaError::Ffmpeg(error)),
                Some(Ok(raw)) => {
                    let frame_seconds =
                        self.start_seconds + raw.pts_us().unwrap_or(0) as f64 / 1_000_000.0;
                    let frame = convert_frame(raw, self.width, self.height)?;
                    self.last_frame_seconds = Some(frame_seconds);
                    self.last_frame = Some(frame.clone());
                    if reached(frame_seconds, target_seconds) {
                        self.last_timestamp = Some(target_seconds);
                        return Ok(frame);
                    }
                }
            }
        }
    }
}

pub(crate) fn timestamps_match(left: f64, right: f64) -> bool {
    (left - right).abs() <= 1e-7_f64.max(left.abs().max(right.abs()) * 1e-9)
}

/// Whether a decoded frame at `frame_seconds` satisfies a request for
/// `target_seconds` — it is at or after the request (with the same tolerance
/// [`timestamps_match`] uses for an exact landing).
pub(crate) fn reached(frame_seconds: f64, target_seconds: f64) -> bool {
    frame_seconds > target_seconds || timestamps_match(frame_seconds, target_seconds)
}

pub(crate) fn path_to_url(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

pub(crate) fn seconds_to_us(seconds: f64) -> i64 {
    (seconds * 1_000_000.0).round() as i64
}

/// Converts one `ez-ffmpeg` RGBA frame into a [`VideoFrame`], rejecting any
/// frame whose dimensions or packed length disagree with the probed stream.
pub(crate) fn convert_frame(
    frame: EzVideoFrame,
    expected_width: u32,
    expected_height: u32,
) -> Result<VideoFrame, MediaError> {
    let (width, height) = (frame.width(), frame.height());
    let pixels = frame.into_vec();
    let expected = frame_byte_len(expected_width, expected_height)?;
    if width != expected_width || height != expected_height || pixels.len() != expected {
        return Err(MediaError::UnexpectedFrameSize {
            width: expected_width,
            height: expected_height,
            expected,
            actual: pixels.len(),
        });
    }
    Ok(VideoFrame {
        width,
        height,
        pixels: Arc::new(pixels),
    })
}
