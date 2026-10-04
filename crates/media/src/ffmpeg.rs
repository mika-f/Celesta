use crate::decoder::{AudioDecoder, VideoFrameDecoder};
use crate::error::MediaError;
use crate::mix::clamp_to_source_end;
use crate::probe::probe_path;
use crate::sequential::{convert_frame, path_to_url, seconds_to_us, timestamps_match};
use crate::types::{AudioBuffer, MediaProbe, VideoFrame};
use celesta_composition::Rational;
use ez_ffmpeg::Input;
use ez_ffmpeg::frame_export::{Channels, FrameExtractor, FrameIter, PixelLayout, SampleExtractor};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct FfmpegBackend {
    pub(crate) probes: HashMap<PathBuf, MediaProbe>,
    pub(crate) sequential_frame_rate: Option<Rational>,
    pub(crate) video_sessions: HashMap<(String, PathBuf), SequentialVideoSession>,
    pub(crate) sequential_video_processes_started: u64,
}

/// How far ahead of the last served frame a request may be while still being
/// answered by walking the open decode run forward. Beyond this a fresh
/// container seek is cheaper than decoding every frame in between.
pub(crate) const MAX_FORWARD_WALK_SECONDS: f64 = 0.5;

/// One long-lived `ez-ffmpeg` decode run held open across a monotonic
/// sequence of frame requests. Frames are pulled from `frames` at the
/// source's native cadence; each request walks the iterator forward to the
/// first frame at or after the requested time (a container seek re-zeroed
/// `frames`' timeline at `start_seconds`).
pub(crate) struct SequentialVideoSession {
    pub(crate) frames: FrameIter,
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// The source time the decode window was seeked to; frame presentation
    /// times are reported relative to this.
    pub(crate) start_seconds: f64,
    /// The expected spacing between consecutive requests, used only to decide
    /// whether the next request stays within this session's cadence.
    pub(crate) step_seconds: f64,
    /// The last (clamped) request time this session served — the cadence the
    /// `decode_sequential` fast paths reason about.
    pub(crate) last_timestamp: Option<f64>,
    /// The presentation time of `last_frame`, used to tell whether the frame
    /// already in hand answers a near-repeat request.
    pub(crate) last_frame_seconds: Option<f64>,
    pub(crate) last_frame: Option<VideoFrame>,
}

impl FfmpegBackend {
    pub fn new() -> Self {
        Self {
            probes: HashMap::new(),
            sequential_frame_rate: None,
            video_sessions: HashMap::new(),
            sequential_video_processes_started: 0,
        }
    }

    pub fn with_sequential_video(mut self, frame_rate: Rational) -> Self {
        if frame_rate.is_valid() {
            self.sequential_frame_rate = Some(frame_rate);
        }
        self
    }

    pub const fn sequential_video_processes_started(&self) -> u64 {
        self.sequential_video_processes_started
    }

    pub fn probe(&mut self, path: impl AsRef<Path>) -> Result<&MediaProbe, MediaError> {
        let path = path.as_ref();
        if !self.probes.contains_key(path) {
            let probe = probe_path(path)?;
            self.probes.insert(path.to_owned(), probe);
        }
        Ok(self.probes.get(path).expect("probe was cached"))
    }

    pub(crate) fn decode(
        &mut self,
        path: &Path,
        source_time_seconds: f64,
    ) -> Result<VideoFrame, MediaError> {
        if !source_time_seconds.is_finite() || source_time_seconds < 0.0 {
            return Err(MediaError::InvalidTimestamp(source_time_seconds));
        }
        let probe = self.probe(path)?;
        let video = probe
            .video
            .clone()
            .ok_or_else(|| MediaError::NoVideoStream(path.to_owned()))?;
        let source_time_seconds = clamp_to_source_end(source_time_seconds, &video, probe.duration);

        // A container seek to the keyframe at or before the request re-zeroes
        // the timeline; the in-graph trim then drops the negative-pts lead-in,
        // so the first delivered frame is the one at or after the request —
        // the same frame `ffmpeg -ss <t> -i … -frames:v 1` produced.
        let mut frames = FrameExtractor::new(Input::from(path_to_url(path)))
            .start_time_us(seconds_to_us(source_time_seconds))
            .pixel(PixelLayout::Rgba32)
            .max_frames(1)
            .frames()
            .map_err(MediaError::Ffmpeg)?;
        let frame = frames
            .next()
            .transpose()
            .map_err(MediaError::Ffmpeg)?
            .ok_or_else(|| MediaError::NoVideoStream(path.to_owned()))?;
        convert_frame(frame, video.width, video.height)
    }

    pub(crate) fn decode_sequential(
        &mut self,
        request_id: &str,
        path: &Path,
        source_time_seconds: f64,
        default_frame_rate: Rational,
    ) -> Result<VideoFrame, MediaError> {
        if !source_time_seconds.is_finite() || source_time_seconds < 0.0 {
            return Err(MediaError::InvalidTimestamp(source_time_seconds));
        }
        let probe = self.probe(path)?;
        let video = probe
            .video
            .clone()
            .ok_or_else(|| MediaError::NoVideoStream(path.to_owned()))?;
        let source_time_seconds = clamp_to_source_end(source_time_seconds, &video, probe.duration);
        let key = (request_id.to_owned(), path.to_owned());
        if let Some(session) = self.video_sessions.get_mut(&key) {
            if session
                .last_timestamp
                .is_some_and(|last| timestamps_match(last, source_time_seconds))
            {
                return session
                    .last_frame
                    .clone()
                    .ok_or_else(|| MediaError::NoVideoStream(path.to_owned()));
            }
            // Forward progress the open run can reach by decoding a short way:
            // keep pulling from it instead of re-seeking. The window is wider
            // than one step because only a batch export asks for an exact +1
            // cadence — an editor preview drops frames to stay with the
            // playhead, so it asks for +2, +3, … steps, and re-seeking those
            // would spawn a fresh decode run for nearly every frame.
            if session.last_timestamp.is_some_and(|last| {
                source_time_seconds > last
                    && source_time_seconds - last
                        <= MAX_FORWARD_WALK_SECONDS.max(session.step_seconds)
            }) {
                return session.read_frame(source_time_seconds);
            }
        }

        // A discontinuity (a seek backwards, or a jump past one step): learn
        // the real request spacing from the last delivered frame, then open a
        // fresh decode run seeked to the request.
        let learned_step = self.video_sessions.remove(&key).and_then(|session| {
            session
                .last_timestamp
                .map(|last| source_time_seconds - last)
                .filter(|step| step.is_finite() && *step > 0.0)
        });
        let step_seconds = learned_step.unwrap_or_else(|| {
            f64::from(default_frame_rate.denominator) / f64::from(default_frame_rate.numerator)
        });
        let mut session = SequentialVideoSession::spawn(
            path,
            source_time_seconds,
            step_seconds,
            video.width,
            video.height,
        )?;
        self.sequential_video_processes_started =
            self.sequential_video_processes_started.saturating_add(1);
        let frame = session.read_frame(source_time_seconds)?;
        self.video_sessions.insert(key, session);
        Ok(frame)
    }
}

impl Default for FfmpegBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl VideoFrameDecoder for FfmpegBackend {
    fn decode_frame(
        &mut self,
        path: &Path,
        source_time_seconds: f64,
    ) -> Result<VideoFrame, MediaError> {
        self.decode(path, source_time_seconds)
    }

    fn decode_frame_for(
        &mut self,
        request_id: &str,
        path: &Path,
        source_time_seconds: f64,
    ) -> Result<VideoFrame, MediaError> {
        match self.sequential_frame_rate {
            Some(frame_rate) => {
                self.decode_sequential(request_id, path, source_time_seconds, frame_rate)
            }
            None => self.decode(path, source_time_seconds),
        }
    }
}

impl AudioDecoder for FfmpegBackend {
    fn decode_audio(
        &mut self,
        path: &Path,
        sample_rate: u32,
        channels: u16,
    ) -> Result<AudioBuffer, MediaError> {
        if sample_rate == 0 || channels == 0 {
            return Err(MediaError::InvalidAudioFormat {
                sample_rate,
                channels,
            });
        }
        if self.probe(path).is_ok_and(|probe| probe.audio.is_empty()) {
            return Ok(AudioBuffer {
                sample_rate,
                channels,
                samples: Vec::new(),
            });
        }
        let layout = if channels == 1 {
            Channels::Mono
        } else {
            Channels::Stereo
        };
        let samples = SampleExtractor::new(Input::from(path_to_url(path)))
            .sample_rate(sample_rate)
            .channels(layout)
            .collect_samples()
            .map_err(MediaError::Ffmpeg)?;
        if samples.len() % usize::from(channels) != 0 {
            return Err(MediaError::InvalidAudioSampleCount {
                samples: samples.len(),
                channels,
            });
        }
        Ok(AudioBuffer {
            sample_rate,
            channels,
            samples,
        })
    }
}
