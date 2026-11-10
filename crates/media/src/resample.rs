//! Audio sample-rate conversion, for the rare mix whose rate an audio
//! encoder refuses (Opus takes 48 kHz only).

use crate::error::MediaError;
use crate::types::AudioBuffer;
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};

/// Input frames the resampler takes at a time.
const CHUNK_FRAMES: usize = 1024;

/// `audio` at `sample_rate`: the same channels, `round(frames × to / from)`
/// frames, lined up with the input. The same rate returns a clone.
pub fn resample_audio(audio: &AudioBuffer, sample_rate: u32) -> Result<AudioBuffer, MediaError> {
    let channels = usize::from(audio.channels);
    if audio.sample_rate == 0 || sample_rate == 0 || channels == 0 {
        return Err(MediaError::InvalidAudioFormat {
            sample_rate: if audio.sample_rate == 0 {
                audio.sample_rate
            } else {
                sample_rate
            },
            channels: audio.channels,
        });
    }
    if audio.sample_rate == sample_rate {
        return Ok(audio.clone());
    }
    if !audio.samples.len().is_multiple_of(channels) {
        return Err(MediaError::InvalidAudioSampleCount {
            samples: audio.samples.len(),
            channels: audio.channels,
        });
    }
    let frames = audio.frame_count();
    if frames == 0 {
        return Ok(AudioBuffer {
            sample_rate,
            channels: audio.channels,
            samples: Vec::new(),
        });
    }
    let wanted =
        (frames as f64 * f64::from(sample_rate) / f64::from(audio.sample_rate)).round() as usize;
    let failed = |error: &dyn std::fmt::Display| {
        MediaError::Codec(format!("cannot resample the audio: {error}"))
    };
    let parameters = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        oversampling_factor: 256,
        interpolation: SincInterpolationType::Linear,
        window: WindowFunction::BlackmanHarris2,
    };
    let mut resampler = SincFixedIn::<f32>::new(
        f64::from(sample_rate) / f64::from(audio.sample_rate),
        1.0,
        parameters,
        CHUNK_FRAMES,
        channels,
    )
    .map_err(|error| failed(&error))?;

    let planar: Vec<Vec<f32>> = (0..channels)
        .map(|channel| {
            audio
                .samples
                .iter()
                .skip(channel)
                .step_by(channels)
                .copied()
                .collect()
        })
        .collect();
    let mut output: Vec<Vec<f32>> = vec![Vec::new(); channels];
    let append = |output: &mut Vec<Vec<f32>>, chunk: Vec<Vec<f32>>| {
        for (all, chunk) in output.iter_mut().zip(chunk) {
            all.extend(chunk);
        }
    };

    let mut start = 0;
    while frames - start >= resampler.input_frames_next() {
        let end = start + resampler.input_frames_next();
        let chunk: Vec<&[f32]> = planar.iter().map(|channel| &channel[start..end]).collect();
        append(
            &mut output,
            resampler
                .process(&chunk, None)
                .map_err(|error| failed(&error))?,
        );
        start = end;
    }
    // The tail, zero-padded, then silence until `wanted` frames have come
    // out. rubato's sinc resampler already lines its output up with its
    // input (an impulse at input frame `n` peaks at output frame
    // `n × to / from`), so no delay is cut from the front.
    let tail: Vec<&[f32]> = planar.iter().map(|channel| &channel[start..]).collect();
    append(
        &mut output,
        resampler
            .process_partial(Some(&tail), None)
            .map_err(|error| failed(&error))?,
    );
    while output[0].len() < wanted {
        append(
            &mut output,
            resampler
                .process_partial(None::<&[&[f32]]>, None)
                .map_err(|error| failed(&error))?,
        );
    }

    let mut samples = Vec::with_capacity(wanted * channels);
    for frame in 0..wanted {
        for channel in &output {
            samples.push(channel[frame]);
        }
    }
    Ok(AudioBuffer {
        sample_rate,
        channels: audio.channels,
        samples,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(sample_rate: u32, hertz: f32) -> AudioBuffer {
        let samples = (0..sample_rate)
            .flat_map(|frame| {
                let value =
                    (std::f32::consts::TAU * hertz * frame as f32 / sample_rate as f32).sin() * 0.5;
                [value, value]
            })
            .collect();
        AudioBuffer {
            sample_rate,
            channels: 2,
            samples,
        }
    }

    #[test]
    fn resampling_keeps_length_and_tone() {
        let resampled = resample_audio(&sine(44_100, 440.0), 48_000).unwrap();
        assert_eq!(resampled.sample_rate, 48_000);
        assert_eq!(resampled.channels, 2);
        assert_eq!(resampled.frame_count(), 48_000);
        assert!(
            resampled
                .samples
                .as_chunks::<2>()
                .0
                .iter()
                .all(|frame| frame[0] == frame[1])
        );
        let left: Vec<f32> = resampled.samples.iter().step_by(2).copied().collect();
        let crossings = left
            .windows(2)
            .filter(|pair| (pair[0] < 0.0) != (pair[1] < 0.0))
            .count();
        assert!(crossings.abs_diff(880) <= 2, "{crossings} zero crossings");
    }

    #[test]
    fn no_frames_resample_to_no_frames() {
        let empty = AudioBuffer {
            sample_rate: 44_100,
            channels: 2,
            samples: Vec::new(),
        };
        let resampled = resample_audio(&empty, 48_000).unwrap();
        assert_eq!(resampled.sample_rate, 48_000);
        assert_eq!(resampled.channels, 2);
        assert!(resampled.samples.is_empty());
    }

    /// Ruling R8: rubato's output is already lined up with its input, so
    /// nothing is cut from the front. An impulse at input frame `n` peaks
    /// within a frame of `n × to / from`: an output frame between 44.1 and
    /// 48 kHz and when downsampling, an input frame when upsampling by more,
    /// which trails by just under one (5 output frames from 8 to 48 kHz,
    /// 0.1 ms).
    #[test]
    fn an_impulse_stays_where_it_was() {
        let rates = [
            (44_100, 48_000),
            (48_000, 44_100),
            (96_000, 48_000),
            (48_000, 8_000),
            (16_000, 48_000),
            (8_000, 48_000),
        ];
        for (from, to) in rates {
            for n in [3_000usize, 5_001] {
                let mut samples = vec![0.0; from as usize];
                samples[n] = 1.0;
                let audio = AudioBuffer {
                    sample_rate: from,
                    channels: 1,
                    samples,
                };
                let resampled = resample_audio(&audio, to).unwrap();
                let peak = resampled
                    .samples
                    .iter()
                    .enumerate()
                    .max_by(|a, b| a.1.total_cmp(b.1))
                    .map(|(frame, _)| frame)
                    .unwrap();
                let ratio = f64::from(to) / f64::from(from);
                let expected = n as f64 * ratio;
                assert!(
                    (peak as f64 - expected).abs() <= ratio.max(1.0),
                    "{from} -> {to}: impulse at {n} peaks at {peak}, expected {expected}"
                );
            }
        }
    }

    #[test]
    fn the_same_rate_is_returned_as_is() {
        let audio = sine(48_000, 440.0);
        assert_eq!(resample_audio(&audio, 48_000).unwrap(), audio);
    }
}
