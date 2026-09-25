//! Media probing and frame decoding through the linked FFmpeg libraries
//! (`ez-ffmpeg`).

use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

use ez_ffmpeg::frame_export::{
    Channels, FrameExtractor, FrameIter, PixelLayout, SampleExtractor, VideoFrame as EzVideoFrame,
};
use ez_ffmpeg::stream_info::{StreamInfo, find_all_stream_infos};
use ez_ffmpeg::{AVRational, Input, container_info};

use celesta_composition::{
    AnimationError, AudioGraph, Rational, Time, TimeError, evaluate_f64, integrate_f64,
};
use celesta_remote::{RemoteAssetError, resolve_asset_path};

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
    pub pixels: Vec<u8>,
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

pub struct FfmpegBackend {
    probes: HashMap<PathBuf, MediaProbe>,
    sequential_frame_rate: Option<Rational>,
    video_sessions: HashMap<(String, PathBuf), SequentialVideoSession>,
    sequential_video_processes_started: u64,
}

/// How far ahead of the last served frame a request may be while still being
/// answered by walking the open decode run forward. Beyond this a fresh
/// container seek is cheaper than decoding every frame in between.
const MAX_FORWARD_WALK_SECONDS: f64 = 0.5;

/// One long-lived `ez-ffmpeg` decode run held open across a monotonic
/// sequence of frame requests. Frames are pulled from `frames` at the
/// source's native cadence; each request walks the iterator forward to the
/// first frame at or after the requested time (a container seek re-zeroed
/// `frames`' timeline at `start_seconds`).
struct SequentialVideoSession {
    frames: FrameIter,
    width: u32,
    height: u32,
    /// The source time the decode window was seeked to; frame presentation
    /// times are reported relative to this.
    start_seconds: f64,
    /// The expected spacing between consecutive requests, used only to decide
    /// whether the next request stays within this session's cadence.
    step_seconds: f64,
    /// The last (clamped) request time this session served — the cadence the
    /// `decode_sequential` fast paths reason about.
    last_timestamp: Option<f64>,
    /// The presentation time of `last_frame`, used to tell whether the frame
    /// already in hand answers a near-repeat request.
    last_frame_seconds: Option<f64>,
    last_frame: Option<VideoFrame>,
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

    fn decode(&mut self, path: &Path, source_time_seconds: f64) -> Result<VideoFrame, MediaError> {
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

    fn decode_sequential(
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

impl SequentialVideoSession {
    fn spawn(
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
    fn read_frame(&mut self, target_seconds: f64) -> Result<VideoFrame, MediaError> {
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

fn timestamps_match(left: f64, right: f64) -> bool {
    (left - right).abs() <= 1e-7_f64.max(left.abs().max(right.abs()) * 1e-9)
}

/// Whether a decoded frame at `frame_seconds` satisfies a request for
/// `target_seconds` — it is at or after the request (with the same tolerance
/// [`timestamps_match`] uses for an exact landing).
fn reached(frame_seconds: f64, target_seconds: f64) -> bool {
    frame_seconds > target_seconds || timestamps_match(frame_seconds, target_seconds)
}

fn path_to_url(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn seconds_to_us(seconds: f64) -> i64 {
    (seconds * 1_000_000.0).round() as i64
}

/// Converts one `ez-ffmpeg` RGBA frame into a [`VideoFrame`], rejecting any
/// frame whose dimensions or packed length disagree with the probed stream.
fn convert_frame(
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
        pixels,
    })
}

fn probe_path(path: &Path) -> Result<MediaProbe, MediaError> {
    let url = path_to_url(path);
    let streams = find_all_stream_infos(url.as_str()).map_err(MediaError::Ffmpeg)?;
    let duration = container_info::get_duration_us(url.as_str())
        .ok()
        .filter(|micros| *micros > 0)
        .map(|micros| Time::new(micros, 1_000_000).reduced());

    let mut video = None;
    let mut audio = Vec::new();
    for stream in streams {
        match stream {
            StreamInfo::Video {
                index,
                codec_name,
                width,
                height,
                avg_frame_rate,
                r_frame_rate,
                duration,
                time_base,
                ..
            } if video.is_none() => {
                video = Some(VideoStream {
                    index: index.max(0) as u32,
                    codec: normalize_codec(codec_name),
                    width: width.max(0) as u32,
                    height: height.max(0) as u32,
                    frame_rate: video_frame_rate(avg_frame_rate, r_frame_rate),
                    duration: stream_duration(duration, time_base),
                });
            }
            StreamInfo::Audio {
                index,
                codec_name,
                sample_rate,
                nb_channels,
                duration,
                time_base,
                ..
            } => {
                audio.push(AudioStream {
                    index: index.max(0) as u32,
                    codec: normalize_codec(codec_name),
                    sample_rate: (sample_rate > 0).then_some(sample_rate as u32),
                    channels: (nb_channels > 0).then_some(nb_channels as u16),
                    duration: stream_duration(duration, time_base),
                });
            }
            _ => {}
        }
    }
    Ok(MediaProbe {
        duration,
        video,
        audio,
    })
}

fn normalize_codec(name: String) -> Option<String> {
    (!name.is_empty() && name != "Unknown codec").then_some(name)
}

fn rational_from_av(rate: AVRational) -> Option<Rational> {
    (rate.num > 0 && rate.den > 0).then(|| Rational::new(rate.num as u32, rate.den as u32))
}

/// A video stream's frame rate: `avg_frame_rate` when the demuxer computed
/// one, otherwise `r_frame_rate` — but only when the latter is a plausible
/// capture/render rate. FFmpeg reports the container time base as
/// `r_frame_rate` (Matroska's `1000/1`, MPEG-TS's `90000/1`, …) when a stream
/// carries no real frame-duration hint, and that artifact must not be
/// mistaken for a real rate (`clamp_to_source_end` reads it).
fn video_frame_rate(avg: AVRational, raw: AVRational) -> Option<Rational> {
    rational_from_av(avg).or_else(|| rational_from_av(raw).filter(is_plausible_frame_rate))
}

fn is_plausible_frame_rate(rate: &Rational) -> bool {
    f64::from(rate.numerator) / f64::from(rate.denominator) <= 480.0
}

/// A stream's own `duration` (in `time_base` units) as exact [`Time`], or
/// `None` when the demuxer reported no usable value.
fn stream_duration(duration: i64, time_base: AVRational) -> Option<Time> {
    if duration <= 0 || time_base.num <= 0 || time_base.den <= 0 {
        return None;
    }
    let micros =
        i128::from(duration) * i128::from(time_base.num) * 1_000_000 / i128::from(time_base.den);
    i64::try_from(micros)
        .ok()
        .map(|micros| Time::new(micros, 1_000_000).reduced())
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

/// Decodes and mixes a complete audio graph into interleaved stereo PCM.
pub fn mix_audio_graph(
    graph: &AudioGraph,
    asset_root: &Path,
    duration: Time,
    decoder: &mut impl AudioDecoder,
) -> Result<AudioBuffer, AudioMixError> {
    mix_audio_graph_cancellable(graph, asset_root, duration, decoder, || false)
}

/// Decodes and mixes an audio graph, stopping promptly when `is_cancelled`
/// reports that the caller has superseded this work.
pub fn mix_audio_graph_cancellable(
    graph: &AudioGraph,
    asset_root: &Path,
    duration: Time,
    decoder: &mut impl AudioDecoder,
    mut is_cancelled: impl FnMut() -> bool,
) -> Result<AudioBuffer, AudioMixError> {
    const CHANNELS: u16 = 2;
    if is_cancelled() {
        return Err(AudioMixError::Cancelled);
    }
    if graph.sample_rate == 0 {
        return Err(AudioMixError::InvalidSampleRate);
    }
    if !graph.master_volume.is_finite() || graph.master_volume < 0.0 {
        return Err(AudioMixError::InvalidMasterVolume(graph.master_volume));
    }
    let frame_count = sample_ceil(duration, graph.sample_rate)?.max(0) as usize;
    let sample_count = frame_count
        .checked_mul(usize::from(CHANNELS))
        .ok_or(AudioMixError::TimelineTooLong)?;
    let mut output = vec![0.0_f32; sample_count];
    let mut decoded = HashMap::<PathBuf, AudioBuffer>::new();

    for clip in &graph.clips {
        if is_cancelled() {
            return Err(AudioMixError::Cancelled);
        }
        if clip.muted {
            continue;
        }
        let path = resolve_asset_path(asset_root, &clip.asset.location).map_err(|source| {
            AudioMixError::RemoteAsset {
                asset: clip.asset.id.clone(),
                source,
            }
        })?;
        if !decoded.contains_key(&path) {
            let audio = decoder.decode_audio(&path, graph.sample_rate, CHANNELS)?;
            if is_cancelled() {
                return Err(AudioMixError::Cancelled);
            }
            if audio.sample_rate != graph.sample_rate || audio.channels != CHANNELS {
                return Err(AudioMixError::UnexpectedDecodedFormat {
                    sample_rate: audio.sample_rate,
                    channels: audio.channels,
                });
            }
            decoded.insert(path.clone(), audio);
        }
        let source = decoded.get(&path).expect("decoded audio was cached");
        if source.frame_count() == 0 {
            continue;
        }
        let source_start_seconds = clip.source_start.as_seconds()?;
        let source_end_seconds = clip
            .source_duration
            .map(|duration| duration.as_seconds())
            .transpose()?
            .map(|duration| source_start_seconds + duration);
        let start = sample_ceil(clip.range.start, graph.sample_rate)?.clamp(0, frame_count as i64);
        let end = sample_ceil(clip.range.end()?, graph.sample_rate)?.clamp(0, frame_count as i64);
        for output_frame in start..end {
            if output_frame & 2_047 == 0 && is_cancelled() {
                return Err(AudioMixError::Cancelled);
            }
            let project_time = Time::samples(output_frame, graph.sample_rate);
            let local_time = project_time.checked_sub(clip.range.start)?;
            let source_seconds =
                source_start_seconds + integrate_f64(&clip.playback_rate, local_time)?;
            if source_end_seconds.is_some_and(|end| source_seconds >= end) {
                continue;
            }
            let source_position = source_seconds * f64::from(graph.sample_rate);
            if !source_position.is_finite() || source_position < 0.0 {
                continue;
            }
            let source_frame = source_position.floor() as usize;
            if source_frame >= source.frame_count() {
                continue;
            }
            let next_frame = (source_frame + 1).min(source.frame_count() - 1);
            let fraction = (source_position - source_frame as f64) as f32;
            let volume = evaluate_f64(&clip.volume, local_time)?.clamp(0.0, 16.0) as f32;
            for channel in 0..usize::from(CHANNELS) {
                let from = source.samples[source_frame * usize::from(CHANNELS) + channel];
                let to = source.samples[next_frame * usize::from(CHANNELS) + channel];
                let sample = (from + (to - from) * fraction) * volume;
                output[output_frame as usize * usize::from(CHANNELS) + channel] += sample;
            }
        }
    }
    for sample in &mut output {
        *sample = (*sample * graph.master_volume as f32).clamp(-1.0, 1.0);
    }
    Ok(AudioBuffer {
        sample_rate: graph.sample_rate,
        channels: CHANNELS,
        samples: output,
    })
}

fn sample_ceil(time: Time, sample_rate: u32) -> Result<i64, TimeError> {
    if !time.is_valid() {
        return Err(TimeError::ZeroTimescale);
    }
    let numerator = i128::from(time.value) * i128::from(sample_rate);
    let denominator = i128::from(time.timescale);
    let value = if numerator >= 0 {
        (numerator + denominator - 1) / denominator
    } else {
        numerator / denominator
    };
    i64::try_from(value).map_err(|_| TimeError::Overflow)
}

fn frame_byte_len(width: u32, height: u32) -> Result<usize, MediaError> {
    u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(4))
        .and_then(|bytes| usize::try_from(bytes).ok())
        .ok_or(MediaError::FrameTooLarge { width, height })
}

/// Clamps a requested source time to just inside the source's final frame.
/// FFmpeg's input seek only outputs frames at or after the target
/// timestamp, so landing anywhere past the last frame's presentation time
/// (container duration minus one nominal frame period) yields zero output;
/// clamping there instead makes a clip whose playback outruns its file
/// render the source's last frame. Sources without a usable probed duration
/// or frame rate pass through untouched and keep the old error behavior.
fn clamp_to_source_end(
    source_time_seconds: f64,
    video: &VideoStream,
    format_duration: Option<Time>,
) -> f64 {
    let Some(frame_rate) = video
        .frame_rate
        .filter(|rate| rate.is_valid() && rate.numerator > 0)
    else {
        return source_time_seconds;
    };
    let seconds = match (format_duration, video.duration) {
        (Some(format), Some(stream)) => format
            .as_seconds()
            .ok()
            .zip(stream.as_seconds().ok())
            .map(|(format, stream)| format.min(stream)),
        (available, None) | (None, available) => available.and_then(|time| time.as_seconds().ok()),
    }
    .filter(|seconds| seconds.is_finite() && *seconds > 0.0);
    let Some(seconds) = seconds else {
        return source_time_seconds;
    };
    let frame_step = f64::from(frame_rate.denominator) / f64::from(frame_rate.numerator);
    source_time_seconds.min((seconds - frame_step).max(0.0))
}

#[derive(Debug)]
pub enum MediaError {
    Ffmpeg(ez_ffmpeg::error::Error),
    NoVideoStream(PathBuf),
    InvalidTimestamp(f64),
    FrameTooLarge {
        width: u32,
        height: u32,
    },
    UnexpectedFrameSize {
        width: u32,
        height: u32,
        expected: usize,
        actual: usize,
    },
    InvalidAudioFormat {
        sample_rate: u32,
        channels: u16,
    },
    InvalidAudioSampleCount {
        samples: usize,
        channels: u16,
    },
}

impl fmt::Display for MediaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ffmpeg(error) => write!(formatter, "FFmpeg operation failed: {error}"),
            Self::NoVideoStream(path) => {
                write!(formatter, "{} has no video stream", path.display())
            }
            Self::InvalidTimestamp(time) => write!(formatter, "invalid video timestamp {time}"),
            Self::FrameTooLarge { width, height } => {
                write!(formatter, "video frame {width}x{height} is too large")
            }
            Self::UnexpectedFrameSize {
                width,
                height,
                expected,
                actual,
            } => write!(
                formatter,
                "decoded {width}x{height} RGBA frame has {actual} bytes, expected {expected}"
            ),
            Self::InvalidAudioFormat {
                sample_rate,
                channels,
            } => write!(
                formatter,
                "invalid audio output format {sample_rate} Hz, {channels} channels"
            ),
            Self::InvalidAudioSampleCount { samples, channels } => write!(
                formatter,
                "decoded audio has {samples} samples, not divisible by {channels} channels"
            ),
        }
    }
}

impl Error for MediaError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Ffmpeg(error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum AudioMixError {
    Cancelled,
    InvalidSampleRate,
    InvalidMasterVolume(f64),
    TimelineTooLong,
    RemoteAsset {
        asset: String,
        source: RemoteAssetError,
    },
    UnexpectedDecodedFormat {
        sample_rate: u32,
        channels: u16,
    },
    Media(MediaError),
    Animation(AnimationError),
    Time(TimeError),
}

impl fmt::Display for AudioMixError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("audio mix was cancelled"),
            Self::InvalidSampleRate => formatter.write_str("audio graph has a zero sample rate"),
            Self::InvalidMasterVolume(volume) => {
                write!(formatter, "audio graph has invalid master volume {volume}")
            }
            Self::TimelineTooLong => formatter.write_str("audio timeline is too long to mix"),
            Self::RemoteAsset { asset, source } => {
                write!(formatter, "could not load audio asset `{asset}`: {source}")
            }
            Self::UnexpectedDecodedFormat {
                sample_rate,
                channels,
            } => write!(
                formatter,
                "decoder returned {sample_rate} Hz, {channels} channels"
            ),
            Self::Media(error) => write!(formatter, "could not decode audio: {error}"),
            Self::Animation(error) => write!(formatter, "could not evaluate audio: {error}"),
            Self::Time(error) => write!(formatter, "could not calculate audio time: {error}"),
        }
    }
}

impl Error for AudioMixError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Media(error) => Some(error),
            Self::Animation(error) => Some(error),
            Self::Time(error) => Some(error),
            Self::RemoteAsset { source, .. } => Some(source),
            Self::Cancelled
            | Self::InvalidSampleRate
            | Self::InvalidMasterVolume(_)
            | Self::TimelineTooLong
            | Self::UnexpectedDecodedFormat { .. } => None,
        }
    }
}

impl From<MediaError> for AudioMixError {
    fn from(error: MediaError) -> Self {
        Self::Media(error)
    }
}

impl From<AnimationError> for AudioMixError {
    fn from(error: AnimationError) -> Self {
        Self::Animation(error)
    }
}

impl From<TimeError> for AudioMixError {
    fn from(error: TimeError) -> Self {
        Self::Time(error)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use celesta_composition::{Animatable, AssetLocation, AudioClip, ResolvedAsset, TimeRange};
    use ez_ffmpeg::{FfmpegContext, Output};

    use super::*;

    /// Renders a synthetic `lavfi` source to a lossless MKV fixture through
    /// the linked FFmpeg libraries, forcing the output frame rate so the
    /// container records a real frame-duration hint.
    fn generate_clip(path: &Path, lavfi: &str, fps: (i32, i32)) {
        FfmpegContext::builder()
            .input(Input::from(lavfi).set_format("lavfi"))
            .output(
                Output::from(path_to_url(path))
                    .set_video_codec("ffv1")
                    .set_framerate(fps.0, fps.1),
            )
            .build()
            .unwrap()
            .start()
            .unwrap()
            .wait()
            .unwrap();
    }

    fn fixture_dir(label: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "celesta-media-{label}-{}-{suffix}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn probes_stream_metadata_as_exact_rational_time() {
        let directory = fixture_dir("probe");
        let clip = directory.join("clip.mkv");
        generate_clip(&clip, "testsrc2=size=320x240:rate=25:duration=1", (25, 1));

        let mut backend = FfmpegBackend::new();
        let probe = backend.probe(&clip).unwrap();
        let video = probe.video.clone().unwrap();
        assert_eq!(video.frame_rate, Some(Rational::new(25, 1)));
        assert_eq!((video.width, video.height), (320, 240));
        let seconds = probe.duration.unwrap().as_seconds().unwrap();
        assert!((0.9..=1.1).contains(&seconds), "probed duration {seconds}");

        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn reports_a_missing_source_as_an_ffmpeg_error() {
        let mut backend = FfmpegBackend::new();
        assert!(matches!(
            backend.probe("/definitely-not-a-real/celesta-media.mkv"),
            Err(MediaError::Ffmpeg(_))
        ));
    }

    #[test]
    fn validates_rgba_frame_sizes() {
        assert_eq!(frame_byte_len(1920, 1080).unwrap(), 8_294_400);
    }

    #[test]
    fn freezes_on_the_last_frame_past_the_source_end() {
        // A one-second, 64x64 testsrc clip whose content visibly changes
        // over time (so "the last frame" is distinguishable from the first).
        let directory = fixture_dir("eof");
        let clip = directory.join("clip.mkv");
        generate_clip(&clip, "testsrc=size=64x64:rate=10:duration=1", (10, 1));

        // One-shot decoding far past the end clamps to the source's final
        // frame instead of failing with an unexpected frame size, and that
        // frozen frame is a real decoded frame (different from the first).
        let mut backend = FfmpegBackend::new();
        let first = backend.decode_frame(&clip, 0.0).unwrap();
        let overrun = backend.decode_frame(&clip, 60.0).unwrap();
        assert_eq!((overrun.width, overrun.height), (64, 64));
        assert_ne!(overrun.pixels, first.pixels);

        // Sequential sessions freeze the same way while playback walks past
        // the end, and repeated tail requests keep returning identical
        // pixels.
        let mut sequential = FfmpegBackend::new().with_sequential_video(Rational::new(10, 1));
        let mut previous = sequential.decode_frame_for("clip", &clip, 0.0).unwrap();
        for step in 1..40 {
            previous = sequential
                .decode_frame_for("clip", &clip, f64::from(step) * 0.1)
                .unwrap_or_else(|error| panic!("frame {step} should decode: {error}"));
        }
        let tail_a = sequential.decode_frame_for("clip", &clip, 9.9).unwrap();
        let tail_b = sequential.decode_frame_for("clip", &clip, 30.3).unwrap();
        assert_eq!(tail_a.pixels, tail_b.pixels);
        assert_eq!(tail_a.pixels, previous.pixels);

        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn mixes_audio_at_exact_timeline_and_source_offsets() {
        struct Decoder;

        impl AudioDecoder for Decoder {
            fn decode_audio(
                &mut self,
                path: &Path,
                sample_rate: u32,
                channels: u16,
            ) -> Result<AudioBuffer, MediaError> {
                assert_eq!(path, Path::new("assets/voice.wav"));
                assert_eq!((sample_rate, channels), (4, 2));
                Ok(AudioBuffer {
                    sample_rate,
                    channels,
                    samples: [0.0_f32, 0.25, 0.5, 0.75, 1.0]
                        .into_iter()
                        .flat_map(|sample| [sample, sample])
                        .collect(),
                })
            }
        }

        let mut graph = AudioGraph {
            sample_rate: 4,
            master_volume: 1.0,
            clips: vec![AudioClip {
                id: "voice".to_owned(),
                asset: ResolvedAsset {
                    id: "voice".to_owned(),
                    location: AssetLocation::File {
                        path: "voice.wav".to_owned(),
                    },
                },
                range: TimeRange {
                    start: Time::new(1, 2),
                    duration: Time::new(1, 1),
                },
                source_start: Time::new(1, 4),
                source_duration: None,
                playback_rate: Animatable::Static(1.0),
                volume: Animatable::Static(0.5),
                muted: false,
            }],
        };

        let mixed =
            mix_audio_graph(&graph, Path::new("assets"), Time::new(2, 1), &mut Decoder).unwrap();

        assert_eq!((mixed.sample_rate, mixed.channels), (4, 2));
        let left = mixed
            .samples
            .chunks_exact(2)
            .map(|frame| frame[0])
            .collect::<Vec<_>>();
        assert_eq!(left, vec![0.0, 0.0, 0.125, 0.25, 0.375, 0.5, 0.0, 0.0]);

        graph.master_volume = 0.5;
        let quieter =
            mix_audio_graph(&graph, Path::new("assets"), Time::new(2, 1), &mut Decoder).unwrap();
        assert_eq!(quieter.samples[4], 0.0625);

        graph.master_volume = 1.0;
        graph.clips[0].source_duration = Some(Time::new(1, 2));
        let source_trimmed =
            mix_audio_graph(&graph, Path::new("assets"), Time::new(2, 1), &mut Decoder).unwrap();
        let left = source_trimmed
            .samples
            .chunks_exact(2)
            .map(|frame| frame[0])
            .collect::<Vec<_>>();
        assert_eq!(left, vec![0.0, 0.0, 0.125, 0.25, 0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn cancels_audio_mix_before_decoding() {
        struct Decoder;

        impl AudioDecoder for Decoder {
            fn decode_audio(
                &mut self,
                _: &Path,
                _: u32,
                _: u16,
            ) -> Result<AudioBuffer, MediaError> {
                panic!("cancelled mixes must not decode audio");
            }
        }

        let error = mix_audio_graph_cancellable(
            &AudioGraph {
                sample_rate: 48_000,
                master_volume: 1.0,
                clips: Vec::new(),
            },
            Path::new("assets"),
            Time::new(1, 1),
            &mut Decoder,
            || true,
        )
        .unwrap_err();

        assert!(matches!(error, AudioMixError::Cancelled));
    }
}
