//! Media probing and frame decoding behind an FFmpeg process boundary.

use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Output, Stdio};

use mikan_composition::{
    AnimationError, AssetLocation, AudioGraph, Rational, Time, TimeError, evaluate_f64,
    integrate_f64,
};
use serde::Deserialize;

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
    ffmpeg: PathBuf,
    ffprobe: PathBuf,
    probes: HashMap<PathBuf, MediaProbe>,
    sequential_frame_rate: Option<Rational>,
    video_sessions: HashMap<(String, PathBuf), SequentialVideoSession>,
    sequential_video_processes_started: u64,
}

struct SequentialVideoSession {
    child: Child,
    stdout: ChildStdout,
    width: u32,
    height: u32,
    step_seconds: f64,
    last_timestamp: Option<f64>,
    last_frame: Option<VideoFrame>,
}

impl Drop for SequentialVideoSession {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl FfmpegBackend {
    pub fn new() -> Self {
        Self::with_executables("ffmpeg", "ffprobe")
    }

    pub fn with_executables(ffmpeg: impl Into<PathBuf>, ffprobe: impl Into<PathBuf>) -> Self {
        Self {
            ffmpeg: ffmpeg.into(),
            ffprobe: ffprobe.into(),
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
            let output = Command::new(&self.ffprobe)
                .args([
                    "-v",
                    "error",
                    "-of",
                    "json",
                    "-show_streams",
                    "-show_format",
                ])
                .arg(path)
                .output()
                .map_err(|source| MediaError::Executable {
                    executable: self.ffprobe.clone(),
                    source,
                })?;
            ensure_success("ffprobe", &output)?;
            let probe = parse_probe(&output.stdout)?;
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
        let timestamp = format!("{source_time_seconds:.9}");
        let output = Command::new(&self.ffmpeg)
            .args(["-v", "error", "-ss"])
            .arg(timestamp)
            .arg("-i")
            .arg(path)
            .args([
                "-map",
                "0:v:0",
                "-frames:v",
                "1",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgba",
                "pipe:1",
            ])
            .output()
            .map_err(|source| MediaError::Executable {
                executable: self.ffmpeg.clone(),
                source,
            })?;
        ensure_success("ffmpeg", &output)?;

        let expected = frame_byte_len(video.width, video.height)?;
        if output.stdout.len() != expected {
            return Err(MediaError::UnexpectedFrameSize {
                width: video.width,
                height: video.height,
                expected,
                actual: output.stdout.len(),
            });
        }
        Ok(VideoFrame {
            width: video.width,
            height: video.height,
            pixels: output.stdout,
        })
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
            let expected = session
                .last_timestamp
                .map(|last| last + session.step_seconds);
            if expected.is_some_and(|expected| timestamps_match(expected, source_time_seconds)) {
                return session.read_frame(source_time_seconds);
            }
        }

        let learned_step = self.video_sessions.remove(&key).and_then(|session| {
            session
                .last_timestamp
                .map(|last| source_time_seconds - last)
                .filter(|step| step.is_finite() && *step > 0.0)
        });
        let (step_seconds, rate) = learned_step.map_or_else(
            || {
                (
                    f64::from(default_frame_rate.denominator)
                        / f64::from(default_frame_rate.numerator),
                    format!(
                        "{}/{}",
                        default_frame_rate.numerator, default_frame_rate.denominator
                    ),
                )
            },
            |step| (step, format!("{:.12}", 1.0 / step)),
        );
        let mut session = SequentialVideoSession::spawn(
            &self.ffmpeg,
            path,
            source_time_seconds,
            step_seconds,
            &rate,
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
        ffmpeg: &Path,
        path: &Path,
        source_time_seconds: f64,
        step_seconds: f64,
        rate: &str,
        width: u32,
        height: u32,
    ) -> Result<Self, MediaError> {
        let timestamp = format!("{source_time_seconds:.9}");
        let mut child = Command::new(ffmpeg)
            .args(["-v", "error", "-ss"])
            .arg(timestamp)
            .arg("-i")
            .arg(path)
            .args(["-map", "0:v:0", "-vf"])
            .arg(format!("fps=fps={rate}:start_time=0:round=near"))
            .args(["-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|source| MediaError::Executable {
                executable: ffmpeg.to_owned(),
                source,
            })?;
        let stdout = child.stdout.take().ok_or(MediaError::MissingVideoPipe)?;
        Ok(Self {
            child,
            stdout,
            width,
            height,
            step_seconds,
            last_timestamp: None,
            last_frame: None,
        })
    }

    fn read_frame(&mut self, source_time_seconds: f64) -> Result<VideoFrame, MediaError> {
        let expected = frame_byte_len(self.width, self.height)?;
        let mut pixels = vec![0; expected];
        let mut actual = 0;
        while actual < expected {
            match self.stdout.read(&mut pixels[actual..]) {
                Ok(0) => {
                    // Clean EOF: the source ran out while playback continued
                    // into it (a clip whose window or rate outruns its
                    // file). Freeze on the last decoded frame rather than
                    // failing the caller's whole frame render; only a
                    // session that never produced any frame stays an error.
                    if let Some(frame) = self.last_frame.clone() {
                        return Ok(frame);
                    }
                    return Err(MediaError::UnexpectedFrameSize {
                        width: self.width,
                        height: self.height,
                        expected,
                        actual,
                    });
                }
                Ok(read) => actual += read,
                Err(source) => return Err(MediaError::VideoPipe(source)),
            }
        }
        let frame = VideoFrame {
            width: self.width,
            height: self.height,
            pixels,
        };
        self.last_timestamp = Some(source_time_seconds);
        self.last_frame = Some(frame.clone());
        Ok(frame)
    }
}

fn timestamps_match(left: f64, right: f64) -> bool {
    (left - right).abs() <= 1e-7_f64.max(left.abs().max(right.abs()) * 1e-9)
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
        let output = Command::new(&self.ffmpeg)
            .args(["-v", "error", "-i"])
            .arg(path)
            .args([
                "-map",
                "0:a:0?",
                "-vn",
                "-f",
                "f32le",
                "-acodec",
                "pcm_f32le",
            ])
            .arg("-ac")
            .arg(channels.to_string())
            .arg("-ar")
            .arg(sample_rate.to_string())
            .arg("pipe:1")
            .output()
            .map_err(|source| MediaError::Executable {
                executable: self.ffmpeg.clone(),
                source,
            })?;
        ensure_success("ffmpeg", &output)?;
        if output.stdout.len() % 4 != 0 {
            return Err(MediaError::InvalidAudioByteLength(output.stdout.len()));
        }
        let samples = output
            .stdout
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().expect("chunk has four bytes")))
            .collect::<Vec<_>>();
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
        let path = match &clip.asset.location {
            AssetLocation::File { path } => {
                let path = Path::new(path);
                if path.is_absolute() {
                    path.to_owned()
                } else {
                    asset_root.join(path)
                }
            }
            AssetLocation::Url { url } => {
                return Err(AudioMixError::UnsupportedAssetUrl {
                    asset: clip.asset.id.clone(),
                    url: url.clone(),
                });
            }
        };
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

fn ensure_success(program: &'static str, output: &Output) -> Result<(), MediaError> {
    if output.status.success() {
        return Ok(());
    }
    Err(MediaError::Process {
        program,
        status: output.status.code(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
    })
}

#[derive(Deserialize)]
struct ProbeOutput {
    #[serde(default)]
    streams: Vec<ProbeStream>,
    format: Option<ProbeFormat>,
}

#[derive(Deserialize)]
struct ProbeFormat {
    duration: Option<String>,
}

#[derive(Deserialize)]
struct ProbeStream {
    index: u32,
    codec_type: String,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    avg_frame_rate: Option<String>,
    sample_rate: Option<String>,
    channels: Option<u16>,
    duration: Option<String>,
}

fn parse_probe(input: &[u8]) -> Result<MediaProbe, MediaError> {
    let output: ProbeOutput = serde_json::from_slice(input).map_err(MediaError::ProbeJson)?;
    let duration = output
        .format
        .and_then(|format| format.duration)
        .map(|duration| parse_decimal_time(&duration))
        .transpose()?;
    let mut video = None;
    let mut audio = Vec::new();
    for stream in output.streams {
        let stream_duration = stream
            .duration
            .as_deref()
            .map(parse_decimal_time)
            .transpose()?;
        match stream.codec_type.as_str() {
            "video" if video.is_none() => {
                let width = stream.width.ok_or(MediaError::MissingProbeField("width"))?;
                let height = stream
                    .height
                    .ok_or(MediaError::MissingProbeField("height"))?;
                video = Some(VideoStream {
                    index: stream.index,
                    codec: stream.codec_name,
                    width,
                    height,
                    frame_rate: stream
                        .avg_frame_rate
                        .as_deref()
                        .filter(|rate| *rate != "0/0")
                        .map(parse_rational)
                        .transpose()?,
                    duration: stream_duration,
                });
            }
            "audio" => audio.push(AudioStream {
                index: stream.index,
                codec: stream.codec_name,
                sample_rate: stream
                    .sample_rate
                    .as_deref()
                    .map(str::parse)
                    .transpose()
                    .map_err(|_| MediaError::InvalidProbeValue("sample_rate"))?,
                channels: stream.channels,
                duration: stream_duration,
            }),
            _ => {}
        }
    }
    Ok(MediaProbe {
        duration,
        video,
        audio,
    })
}

fn parse_rational(input: &str) -> Result<Rational, MediaError> {
    let (numerator, denominator) = input
        .split_once('/')
        .ok_or(MediaError::InvalidProbeValue("frame_rate"))?;
    let rational = Rational::new(
        numerator
            .parse()
            .map_err(|_| MediaError::InvalidProbeValue("frame_rate"))?,
        denominator
            .parse()
            .map_err(|_| MediaError::InvalidProbeValue("frame_rate"))?,
    );
    if !rational.is_valid() {
        return Err(MediaError::InvalidProbeValue("frame_rate"));
    }
    Ok(rational)
}

fn parse_decimal_time(input: &str) -> Result<Time, MediaError> {
    let input = input.trim();
    let (negative, unsigned) = input
        .strip_prefix('-')
        .map_or((false, input), |value| (true, value));
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > 9
    {
        return Err(MediaError::InvalidProbeValue("duration"));
    }
    let timescale = 10_u32.pow(fraction.len() as u32);
    let whole: i128 = whole
        .parse()
        .map_err(|_| MediaError::InvalidProbeValue("duration"))?;
    let fraction: i128 = if fraction.is_empty() {
        0
    } else {
        fraction
            .parse()
            .map_err(|_| MediaError::InvalidProbeValue("duration"))?
    };
    let value = whole
        .checked_mul(i128::from(timescale))
        .and_then(|value| value.checked_add(fraction))
        .and_then(|value| {
            if negative {
                value.checked_neg()
            } else {
                Some(value)
            }
        })
        .and_then(|value| i64::try_from(value).ok())
        .ok_or(MediaError::InvalidProbeValue("duration"))?;
    Ok(Time::new(value, timescale).reduced())
}

#[derive(Debug)]
pub enum MediaError {
    Executable {
        executable: PathBuf,
        source: io::Error,
    },
    Process {
        program: &'static str,
        status: Option<i32>,
        stderr: String,
    },
    ProbeJson(serde_json::Error),
    MissingProbeField(&'static str),
    InvalidProbeValue(&'static str),
    NoVideoStream(PathBuf),
    InvalidTimestamp(f64),
    MissingVideoPipe,
    VideoPipe(io::Error),
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
    InvalidAudioByteLength(usize),
    InvalidAudioSampleCount {
        samples: usize,
        channels: u16,
    },
}

impl fmt::Display for MediaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Executable { executable, source } => {
                write!(
                    formatter,
                    "could not run {}: {source}",
                    executable.display()
                )
            }
            Self::Process {
                program,
                status,
                stderr,
            } => {
                write!(
                    formatter,
                    "{program} failed with status {status:?}: {stderr}"
                )
            }
            Self::ProbeJson(error) => write!(formatter, "invalid ffprobe JSON: {error}"),
            Self::MissingProbeField(field) => write!(formatter, "ffprobe omitted `{field}`"),
            Self::InvalidProbeValue(field) => {
                write!(formatter, "ffprobe returned invalid `{field}`")
            }
            Self::NoVideoStream(path) => {
                write!(formatter, "{} has no video stream", path.display())
            }
            Self::InvalidTimestamp(time) => write!(formatter, "invalid video timestamp {time}"),
            Self::MissingVideoPipe => formatter.write_str("FFmpeg did not provide video output"),
            Self::VideoPipe(error) => {
                write!(formatter, "could not read FFmpeg video output: {error}")
            }
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
            Self::InvalidAudioByteLength(bytes) => {
                write!(
                    formatter,
                    "decoded audio has an invalid byte length of {bytes}"
                )
            }
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
            Self::Executable { source, .. } | Self::VideoPipe(source) => Some(source),
            Self::ProbeJson(error) => Some(error),
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
    UnsupportedAssetUrl { asset: String, url: String },
    UnexpectedDecodedFormat { sample_rate: u32, channels: u16 },
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
            Self::UnsupportedAssetUrl { asset, url } => {
                write!(
                    formatter,
                    "audio asset `{asset}` uses unsupported URL `{url}`"
                )
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
            Self::Cancelled
            | Self::InvalidSampleRate
            | Self::InvalidMasterVolume(_)
            | Self::TimelineTooLong
            | Self::UnsupportedAssetUrl { .. }
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

    use mikan_composition::{Animatable, AudioClip, ResolvedAsset, TimeRange};

    use super::*;

    const PROBE: &[u8] = br#"{
      "streams": [
        {
          "index": 0,
          "codec_name": "h264",
          "codec_type": "video",
          "width": 1920,
          "height": 1080,
          "avg_frame_rate": "30000/1001",
          "duration": "12.512500"
        },
        {
          "index": 1,
          "codec_name": "aac",
          "codec_type": "audio",
          "sample_rate": "48000",
          "channels": 2,
          "duration": "12.500000"
        }
      ],
      "format": { "duration": "12.512500" }
    }"#;

    #[test]
    fn parses_probe_metadata_without_floating_point_time() {
        let probe = parse_probe(PROBE).unwrap();
        assert_eq!(probe.duration, Some(Time::new(1001, 80)));
        let video = probe.video.unwrap();
        assert_eq!(video.frame_rate, Some(Rational::new(30_000, 1001)));
        assert_eq!((video.width, video.height), (1920, 1080));
        assert_eq!(probe.audio[0].sample_rate, Some(48_000));
    }

    #[test]
    fn reports_a_missing_ffprobe_executable() {
        let mut backend = FfmpegBackend::with_executables(
            "/definitely-not-installed/mikan-ffmpeg",
            "/definitely-not-installed/mikan-ffprobe",
        );
        assert!(matches!(
            backend.probe("video.mp4"),
            Err(MediaError::Executable { .. })
        ));
    }

    #[test]
    fn validates_rgba_frame_sizes() {
        assert_eq!(frame_byte_len(1920, 1080).unwrap(), 8_294_400);
    }

    #[test]
    fn freezes_on_the_last_frame_past_the_source_end_when_ffmpeg_is_available() {
        let Some((ffmpeg, ffprobe)) = find_ffmpeg_binaries() else {
            eprintln!("skipping live FFmpeg test: ffmpeg/ffprobe were not found");
            return;
        };

        // A one-second, 64x64 testsrc clip whose content visibly changes
        // over time (so "the last frame" is distinguishable from the first).
        let directory =
            std::env::temp_dir().join(format!("mikan-media-eof-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let clip = directory.join("clip.mp4");
        let generated = Command::new(&ffmpeg)
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=64x64:rate=10",
            ])
            .args(["-t", "1", "-y"])
            .arg(&clip)
            .output()
            .unwrap();
        assert!(
            generated.status.success(),
            "{}",
            String::from_utf8_lossy(&generated.stderr)
        );

        // One-shot decoding far past the end clamps to the source's final
        // frame instead of failing with an unexpected frame size, and that
        // frozen frame is a real decoded frame (different from the first).
        let mut backend = FfmpegBackend::with_executables(&ffmpeg, &ffprobe);
        let first = backend.decode_frame(&clip, 0.0).unwrap();
        let overrun = backend.decode_frame(&clip, 60.0).unwrap();
        assert_eq!((overrun.width, overrun.height), (64, 64));
        assert_ne!(overrun.pixels, first.pixels);

        // Sequential sessions freeze the same way while playback walks past
        // the end, and repeated tail requests keep returning identical
        // pixels.
        let mut sequential = FfmpegBackend::with_executables(&ffmpeg, &ffprobe)
            .with_sequential_video(Rational::new(10, 1));
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

    fn find_ffmpeg_binaries() -> Option<(PathBuf, PathBuf)> {
        if let Some(directory) = std::env::var_os("MIKAN_FFMPEG_DIR").map(PathBuf::from) {
            let ffmpeg = directory.join("ffmpeg");
            let ffprobe = directory.join("ffprobe");
            if ffmpeg.is_file() && ffprobe.is_file() {
                return Some((ffmpeg, ffprobe));
            }
        }
        let which = |program: &str| {
            Command::new("which")
                .arg(program)
                .output()
                .ok()
                .filter(|output| output.status.success())
                .map(|output| PathBuf::from(String::from_utf8_lossy(&output.stdout).trim()))
        };
        Some((which("ffmpeg")?, which("ffprobe")?))
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
