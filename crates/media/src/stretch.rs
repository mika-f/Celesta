//! Pitch shifting and time stretching for audio clips, through Signalsmith
//! Stretch. Clips without pitch work stay on the mixer's per-sample loop.

use crate::error::AudioMixError;
use crate::mix::{ClipGain, sample_ceil};
use crate::types::AudioBuffer;
use celesta_composition::{Animatable, AudioClip, Time, evaluate_f64, integrate_f64};
use signalsmith_stretch::Stretch;
use std::ops::Range;

const CHANNELS: usize = 2;
/// Output frames per `process` call. The transposition factor is set, and
/// cancellation checked, once per block; the stretcher picks the factor up at
/// its own ~30 ms analysis interval.
const BLOCK: i64 = 512;
/// Signalsmith's default preset crashes or never returns at very low sample
/// rates (a segfault at 1 and 8 Hz, an endless loop at 32 Hz).
pub(crate) const MIN_STRETCH_SAMPLE_RATE: u32 = 8000;

/// Whether `clip` needs the stretcher: a pitch other than 0, or a playback
/// rate other than 1 whose pitch change is suppressed.
pub(crate) fn needs_stretch(clip: &AudioClip) -> bool {
    clip.pitch != Animatable::Static(0.0)
        || (clip.preserve_pitch && clip.playback_rate != Animatable::Static(1.0))
}

/// Adds `clip`, pitch-shifted and time-stretched, into the interleaved
/// stereo `output` over timeline frames `frames`.
///
/// The stream always starts at the clip's own first frame so the stretcher
/// state is the same however the timeline cuts the clip. Like Signalsmith's
/// own `exact()`, the stretcher is first centred on the clip's first source
/// frame by seeking with the `input_latency()` frames that follow it, and
/// stream frame `j` is then heard at clip frame `j - output_latency()`.
pub(crate) fn mix_stretched_clip(
    clip: &AudioClip,
    source: &AudioBuffer,
    sample_rate: u32,
    frames: Range<i64>,
    duck: Option<&Animatable<f64>>,
    output: &mut [f32],
    is_cancelled: &mut impl FnMut() -> bool,
) -> Result<(), AudioMixError> {
    if sample_rate < MIN_STRETCH_SAMPLE_RATE {
        return Err(AudioMixError::PitchNeedsSampleRate { sample_rate });
    }
    let rate = f64::from(sample_rate);
    let range_start_frame = sample_ceil(clip.range.start, sample_rate)?;
    let produced = frames.end - range_start_frame;
    let skip = frames.start - range_start_frame;
    if produced <= skip {
        return Ok(());
    }
    let local_time = |frame: i64| {
        Time::samples(range_start_frame + frame, sample_rate).checked_sub(clip.range.start)
    };
    let source_start_seconds = clip.source_start.as_seconds()?;
    // Heard frames only move forward, so the gain is followed with a cursor.
    let mut gain = ClipGain::new(clip, duck)?;
    // The source frame clip frame `frame` reads; rounding positions rather
    // than per-block counts keeps blocks from drifting.
    let source_frame = |frame: i64| -> Result<i64, AudioMixError> {
        let seconds =
            source_start_seconds + integrate_f64(&clip.playback_rate, local_time(frame)?)?;
        Ok((seconds * rate).round() as i64)
    };
    let reader = SourceReader {
        source,
        end: clip
            .source_duration
            .map(|duration| duration.as_seconds())
            .transpose()?
            .map(|duration| (source_start_seconds + duration) * rate),
        r#loop: clip.r#loop,
    };

    let mut stretch = Stretch::preset_default(CHANNELS as u32, sample_rate);
    let input_latency = stretch.input_latency() as i64;
    let output_latency = stretch.output_latency() as i64;
    let mut input = Vec::new();
    let mut block = Vec::new();
    let first = source_frame(0)?;
    reader.read(first, input_latency, &mut input);
    stretch.seek(&input, evaluate_f64(&clip.playback_rate, local_time(0)?)?);

    let stream_end = produced + output_latency;
    let mut block_start = 0;
    let mut block_source = first;
    while block_start < stream_end {
        if is_cancelled() {
            return Err(AudioMixError::Cancelled);
        }
        let block_end = (block_start + BLOCK).min(stream_end);
        let next_source = source_frame(block_end)?;
        let input_frames = next_source - block_source;
        // A rate of 0 or below cannot be stretched; its blocks are silent.
        // They still run through the stretcher, with no input, so its output
        // clock keeps pace and its latency tail is not replayed after the gap.
        let silent = input_frames <= 0;
        // Spectra analysed now are centred on clip frame `block_start`'s
        // source and come out `output_latency` frames later, heard at
        // `block_start` itself.
        if let Some(factor) = transpose_factor(clip, local_time(block_start)?)? {
            stretch.set_transpose_factor(factor, None);
        }
        if silent {
            input.clear();
        } else {
            reader.read(block_source + input_latency, input_frames, &mut input);
        }
        block.clear();
        block.resize((block_end - block_start) as usize * CHANNELS, 0.0);
        stretch.process(&input, &mut block);
        if !silent {
            for (heard, frame) in
                (block_start - output_latency..).zip(block.as_chunks::<CHANNELS>().0)
            {
                if heard < skip || heard >= produced {
                    continue;
                }
                let local_time = local_time(heard)?;
                let project_time = Time::samples(range_start_frame + heard, sample_rate);
                let volume = gain.at(local_time, project_time)?;
                let at = (range_start_frame + heard) as usize * CHANNELS;
                for (mixed, sample) in output[at..at + CHANNELS].iter_mut().zip(frame) {
                    *mixed += sample * volume;
                }
            }
        }
        block_source = next_source;
        block_start = block_end;
    }
    Ok(())
}

/// The heard transposition at `local_time`: the clamped pitch, times the
/// playback rate unless the clip preserves its pitch. `None` when that is
/// not a usable multiplier, which keeps the previous one.
fn transpose_factor(clip: &AudioClip, local_time: Time) -> Result<Option<f32>, AudioMixError> {
    let semitones = evaluate_f64(&clip.pitch, local_time)?;
    let semitones = if semitones.is_nan() {
        0.0
    } else {
        semitones.clamp(-24.0, 24.0)
    };
    let mut factor = (semitones / 12.0).exp2();
    if !clip.preserve_pitch {
        factor *= evaluate_f64(&clip.playback_rate, local_time)?;
    }
    Ok((factor.is_finite() && factor > 0.0).then_some(factor as f32))
}

/// Reads a clip's source by frame with the fast path's rules: silence before
/// the first frame and at or past `end`, then wrapping when `loop`, else
/// silence past the last frame.
struct SourceReader<'a> {
    source: &'a AudioBuffer,
    /// Where the clip's `source_duration` ends, in source frames.
    end: Option<f64>,
    r#loop: bool,
}

impl SourceReader<'_> {
    /// Replaces `into` with the interleaved frames `first..first + count`.
    fn read(&self, first: i64, count: i64, into: &mut Vec<f32>) {
        let length = self.source.frame_count() as i64;
        into.clear();
        for frame in first..first + count {
            let silent = frame < 0 || self.end.is_some_and(|end| frame as f64 >= end);
            let frame = if self.r#loop {
                frame.rem_euclid(length)
            } else {
                frame
            };
            if silent || frame >= length {
                into.extend_from_slice(&[0.0; CHANNELS]);
            } else {
                let at = frame as usize * CHANNELS;
                into.extend_from_slice(&self.source.samples[at..at + CHANNELS]);
            }
        }
    }
}
