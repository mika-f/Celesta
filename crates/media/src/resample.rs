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
    fn the_same_rate_is_returned_as_is() {
        let audio = sine(48_000, 440.0);
        assert_eq!(resample_audio(&audio, 48_000).unwrap(), audio);
    }
}
