use crate::decoder::AudioDecoder;
use crate::duck::{DuckCurves, audible_end, curves_with};
use crate::error::{AudioMixError, MediaError};
use crate::stretch::{mix_stretched_clip, needs_stretch};
use crate::types::{AudioBuffer, VideoStream};
use celesta_composition::{
    Animatable, AudioClip, AudioFade, AudioGraph, KeyframeCursor, Time, TimeError, evaluate_f64,
    integrate_f64,
};
use celesta_remote::resolve_asset_path;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
    is_cancelled: impl FnMut() -> bool,
) -> Result<AudioBuffer, AudioMixError> {
    mix_audio_graph_with_ducks_cancellable(graph, asset_root, duration, decoder, is_cancelled)
        .map(|(buffer, _)| buffer)
}

/// [`mix_audio_graph_cancellable`], also returning the ducking curves the
/// mix applied.
pub fn mix_audio_graph_with_ducks_cancellable(
    graph: &AudioGraph,
    asset_root: &Path,
    duration: Time,
    decoder: &mut impl AudioDecoder,
    is_cancelled: impl FnMut() -> bool,
) -> Result<(AudioBuffer, DuckCurves), AudioMixError> {
    let (mut buffer, ducks) = mix_unclipped(graph, asset_root, duration, decoder, is_cancelled)?;
    for sample in &mut buffer.samples {
        *sample = sample.clamp(-1.0, 1.0);
    }
    Ok((buffer, ducks))
}

/// [`mix_audio_graph_cancellable`] without clipping the mix to ±1, for a
/// caller that changes its level first (loudness normalization).
pub fn mix_audio_graph_unclipped_cancellable(
    graph: &AudioGraph,
    asset_root: &Path,
    duration: Time,
    decoder: &mut impl AudioDecoder,
    is_cancelled: impl FnMut() -> bool,
) -> Result<AudioBuffer, AudioMixError> {
    mix_unclipped(graph, asset_root, duration, decoder, is_cancelled).map(|(buffer, _)| buffer)
}

fn mix_unclipped(
    graph: &AudioGraph,
    asset_root: &Path,
    duration: Time,
    decoder: &mut impl AudioDecoder,
    mut is_cancelled: impl FnMut() -> bool,
) -> Result<(AudioBuffer, DuckCurves), AudioMixError> {
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
    let mut paths = Vec::with_capacity(graph.clips.len());

    // Every source is decoded first: ducking needs to know where each marked
    // clip's source runs out before any clip under it is mixed.
    for clip in &graph.clips {
        if is_cancelled() {
            return Err(AudioMixError::Cancelled);
        }
        if clip.muted {
            paths.push(None);
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
        paths.push(Some(path));
    }
    // `curves_with` only asks about clips that are not muted, whose sources
    // were decoded above.
    let ducks = curves_with(graph, |index, clip| {
        audible_end(clip, &decoded[paths[index].as_ref().expect("not muted")])
    })?;

    for ((clip, path), duck) in graph.clips.iter().zip(&paths).zip(&ducks) {
        if is_cancelled() {
            return Err(AudioMixError::Cancelled);
        }
        let Some(path) = path else {
            continue;
        };
        let source = &decoded[path];
        if source.frame_count() == 0 {
            continue;
        }
        let duck = duck.as_ref();
        let source_start_seconds = clip.source_start.as_seconds()?;
        let source_end_seconds = clip
            .source_duration
            .map(|duration| duration.as_seconds())
            .transpose()?
            .map(|duration| source_start_seconds + duration);
        let start = sample_ceil(clip.range.start, graph.sample_rate)?.clamp(0, frame_count as i64);
        let end = sample_ceil(clip.range.end()?, graph.sample_rate)?.clamp(0, frame_count as i64);
        if needs_stretch(clip) {
            mix_stretched_clip(
                clip,
                source,
                graph.sample_rate,
                start..end,
                duck,
                &mut output,
                &mut is_cancelled,
            )?;
            continue;
        }
        let mut gain = ClipGain::new(clip, duck)?;
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
            let mut source_position = source_seconds * f64::from(graph.sample_rate);
            if clip.r#loop {
                // Past the end the source starts over, and its last sample
                // runs into its first.
                source_position = source_position.rem_euclid(source.frame_count() as f64);
            }
            if !source_position.is_finite() || source_position < 0.0 {
                continue;
            }
            let source_frame = source_position.floor() as usize;
            if source_frame >= source.frame_count() {
                continue;
            }
            let next_frame = if clip.r#loop {
                (source_frame + 1) % source.frame_count()
            } else {
                (source_frame + 1).min(source.frame_count() - 1)
            };
            let fraction = (source_position - source_frame as f64) as f32;
            let volume = gain.at(local_time, project_time)?;
            for channel in 0..usize::from(CHANNELS) {
                let from = source.samples[source_frame * usize::from(CHANNELS) + channel];
                let to = source.samples[next_frame * usize::from(CHANNELS) + channel];
                let sample = (from + (to - from) * fraction) * volume;
                output[output_frame as usize * usize::from(CHANNELS) + channel] += sample;
            }
        }
    }
    for sample in &mut output {
        *sample *= graph.master_volume as f32;
    }
    let buffer = AudioBuffer {
        sample_rate: graph.sample_rate,
        channels: CHANNELS,
        samples: output,
    };
    Ok((buffer, ducks))
}

/// A clip's gain along the timeline, read at increasing times: its volume
/// (clamped to 0–16) times its fade and its ducking curve's gain (1 without
/// one). A ducking curve has keys for every run of speech, so it is followed
/// with a cursor instead of searched from its start each sample.
pub(crate) struct ClipGain<'a> {
    volume: &'a Animatable<f64>,
    fade: AudioFade,
    ducking: Option<KeyframeCursor<'a>>,
}

impl<'a> ClipGain<'a> {
    pub(crate) fn new(
        clip: &'a AudioClip,
        duck: Option<&'a Animatable<f64>>,
    ) -> Result<Self, AudioMixError> {
        Ok(Self {
            volume: &clip.volume,
            fade: clip.fade()?,
            ducking: duck.map(KeyframeCursor::new),
        })
    }

    /// The gain at clip-local `local_time`, `project_time` on the timeline.
    pub(crate) fn at(
        &mut self,
        local_time: Time,
        project_time: Time,
    ) -> Result<f32, AudioMixError> {
        let ducking = match &mut self.ducking {
            Some(cursor) => cursor.evaluate(project_time)?,
            None => 1.0,
        };
        Ok((evaluate_f64(self.volume, local_time)?.clamp(0.0, 16.0)
            * self.fade.gain(local_time.as_seconds()?)
            * ducking) as f32)
    }
}

pub(crate) fn sample_ceil(time: Time, sample_rate: u32) -> Result<i64, TimeError> {
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

pub(crate) fn frame_byte_len(width: u32, height: u32) -> Result<usize, MediaError> {
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
pub(crate) fn clamp_to_source_end(
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
    let Some(seconds) = video_seconds(video, format_duration) else {
        return source_time_seconds;
    };
    let frame_step = f64::from(frame_rate.denominator) / f64::from(frame_rate.numerator);
    source_time_seconds.min((seconds - frame_step).max(0.0))
}

/// How long the video stream lasts: the shorter of the stream's and the
/// container's durations, whichever are known.
pub(crate) fn video_seconds(video: &VideoStream, format_duration: Option<Time>) -> Option<f64> {
    match (format_duration, video.duration) {
        (Some(format), Some(stream)) => format
            .as_seconds()
            .ok()
            .zip(stream.as_seconds().ok())
            .map(|(format, stream)| format.min(stream)),
        (available, None) | (None, available) => available.and_then(|time| time.as_seconds().ok()),
    }
    .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
}
