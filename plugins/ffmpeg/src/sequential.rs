use crate::color::conversion;
use crate::converted::ConvertedFrames;
use crate::decoder::SequentialVideoSession;
use crate::ffmpeg_error;
use celesta_composition::Time;
use celesta_media::{MediaError, VideoFrame, VideoStream, frame_byte_len, timestamps_match};
use ez_ffmpeg::Input;
use ez_ffmpeg::frame_export::{FrameExtractor, FrameIter, PixelLayout};
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

/// One decode run from a seek point: FFmpeg's packed RGBA export, or, for
/// the HDR and BT.2020 streams that export refuses, a run that converts
/// their color to sRGB.
pub(crate) enum FrameRun {
    Packed(FrameIter),
    Converted(ConvertedFrames),
    Native(Box<NativeRun>),
}

pub(crate) struct NativeRun {
    frames: crate::native::NativeFrames,
    path: PathBuf,
    video: VideoStream,
    start: f64,
    max_frames: Option<u64>,
    delivered: usize,
}

impl FrameRun {
    pub(crate) fn spawn(
        path: &Path,
        video: &VideoStream,
        source_time_seconds: f64,
        max_frames: Option<u64>,
        accepted: &[celesta_media::DecodedPixelFormat],
    ) -> Result<Self, MediaError> {
        if video.width.is_multiple_of(2)
            && video.height.is_multiple_of(2)
            && conversion(&video.color).is_none()
            && accepted
                .iter()
                .any(|format| *format != celesta_media::DecodedPixelFormat::Rgba8)
            && crate::native::supports_source(path, video.index)
        {
            return Ok(Self::Native(Box::new(NativeRun {
                frames: crate::native::NativeFrames::spawn(
                    path,
                    video.index,
                    source_time_seconds,
                    max_frames,
                    accepted,
                )?,
                path: path.to_owned(),
                video: video.clone(),
                start: source_time_seconds,
                max_frames,
                delivered: 0,
            })));
        }
        Self::legacy(path, video, source_time_seconds, max_frames)
    }

    fn legacy(
        path: &Path,
        video: &VideoStream,
        source_time_seconds: f64,
        max_frames: Option<u64>,
    ) -> Result<Self, MediaError> {
        match conversion(&video.color) {
            Some(conversion) => ConvertedFrames::spawn(
                path,
                video.index,
                source_time_seconds,
                max_frames,
                conversion,
            )
            .map(Self::Converted),
            None => {
                let mut extractor = FrameExtractor::new(Input::from(ffmpeg_path(path)))
                    .start_time_us(seconds_to_us(source_time_seconds))
                    .pixel(PixelLayout::Rgba32);
                if let Some(max) = max_frames {
                    extractor = extractor.max_frames(max);
                }
                extractor.frames().map(Self::Packed)
            }
        }
        .map_err(ffmpeg_error)
    }

    /// The next frame, checked against the probed size, with its
    /// presentation time in microseconds relative to the run's start.
    pub(crate) fn next_frame(
        &mut self,
        width: u32,
        height: u32,
    ) -> Option<Result<(Option<i64>, VideoFrame), MediaError>> {
        if let Self::Native(native) = self {
            let frame = match native.frames.next()? {
                Ok(frame) => frame,
                Err(error) => return Some(Err(error)),
            };
            if let Some(frame_value) = frame.frame {
                native.delivered += 1;
                if (frame_value.width, frame_value.height) != (width, height) {
                    return Some(Err(MediaError::Codec(
                        "native decode dimensions changed".to_owned(),
                    )));
                }
                return Some(Ok((frame.pts_us, frame_value)));
            }
            let replacement =
                Self::legacy(&native.path, &native.video, native.start, native.max_frames);
            let delivered = native.delivered;
            *self = match replacement {
                Ok(run) => run,
                Err(error) => return Some(Err(error)),
            };
            for _ in 0..delivered {
                match self.next_frame(width, height)? {
                    Ok(_) => {}
                    Err(error) => return Some(Err(error)),
                }
            }
            return self.next_frame(width, height);
        }
        let (pts_us, frame_width, frame_height, pixels) = match self {
            Self::Packed(frames) => match frames.next()? {
                Ok(frame) => (
                    frame.pts_us(),
                    frame.width(),
                    frame.height(),
                    frame.into_vec(),
                ),
                Err(error) => return Some(Err(ffmpeg_error(error))),
            },
            Self::Converted(frames) => match frames.next()? {
                Ok(frame) => (frame.pts_us, frame.width, frame.height, frame.pixels),
                Err(error) => return Some(Err(ffmpeg_error(error))),
            },
            Self::Native(_) => unreachable!(),
        };
        Some(
            checked_frame(frame_width, frame_height, pixels, width, height)
                .map(|frame| (pts_us, frame)),
        )
    }
}

impl SequentialVideoSession {
    pub(crate) fn spawn(
        path: &Path,
        video: &VideoStream,
        source_time_seconds: f64,
        step_seconds: f64,
        accepted: &[celesta_media::DecodedPixelFormat],
    ) -> Result<Self, MediaError> {
        let frames = FrameRun::spawn(path, video, source_time_seconds, None, accepted)?;
        Ok(Self {
            frames,
            width: video.width,
            height: video.height,
            start_seconds: source_time_seconds,
            step_seconds,
            last_timestamp: None,
            last_frame_seconds: None,
            last_frame: None,
            last_used: 0,
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
            match self.frames.next_frame(self.width, self.height) {
                None => {
                    if let Some(frame) = self.last_frame.clone() {
                        self.last_timestamp = Some(target_seconds);
                        return Ok(frame);
                    }
                    return Err(MediaError::UnexpectedFrameSize {
                        width: 0,
                        height: 0,
                        actual: 0,
                        expected_width: self.width,
                        expected_height: self.height,
                        expected: frame_byte_len(self.width, self.height)?,
                    });
                }
                Some(Err(error)) => return Err(error),
                Some(Ok((pts_us, frame))) => {
                    let frame_seconds =
                        self.start_seconds + pts_us.unwrap_or(0) as f64 / 1_000_000.0;
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

/// Whether a decoded frame at `frame_seconds` satisfies a request for
/// `target_seconds` — it is at or after the request (with the same tolerance
/// [`timestamps_match`] uses for an exact landing).
pub(crate) fn reached(frame_seconds: f64, target_seconds: f64) -> bool {
    frame_seconds > target_seconds || timestamps_match(frame_seconds, target_seconds)
}

pub(crate) fn ffmpeg_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// `seconds` in the microseconds FFmpeg seeks by.
pub(crate) fn seconds_to_us(seconds: f64) -> i64 {
    Time::from_seconds(seconds, Time::MICROSECONDS).value
}

/// Wraps decoded RGBA pixels in a [`VideoFrame`], rejecting any frame
/// whose dimensions or packed length disagree with the probed stream.
fn checked_frame(
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    expected_width: u32,
    expected_height: u32,
) -> Result<VideoFrame, MediaError> {
    let expected = frame_byte_len(expected_width, expected_height)?;
    if width != expected_width || height != expected_height || pixels.len() != expected {
        return Err(MediaError::UnexpectedFrameSize {
            width,
            height,
            actual: pixels.len(),
            expected_width,
            expected_height,
            expected,
        });
    }
    Ok(VideoFrame {
        pixel_format: celesta_media::DecodedPixelFormat::Rgba8,
        color: None,
        width,
        height,
        pixels: Arc::new(pixels),
    })
}
