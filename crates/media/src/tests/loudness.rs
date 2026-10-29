use super::fixture_dir;
use crate::decoder::AudioDecoder;
use crate::ffmpeg::FfmpegBackend;
use crate::loudness::{
    LoudnessTarget, PeakHandling, TARGET_TOLERANCE_LU, measure_loudness, normalize_loudness,
};
use crate::sequential::ffmpeg_path;
use crate::types::AudioBuffer;
use ez_ffmpeg::{FfmpegContext, Input, Output};
use std::f64::consts::TAU;
use std::fs;
use std::path::Path;

/// Stereo `seconds` of `sample(t)` on both channels at 48 kHz.
fn stereo(seconds: f64, sample: impl Fn(f64) -> f64) -> AudioBuffer {
    let rate = 48_000;
    let frames = (seconds * f64::from(rate)).round() as usize;
    AudioBuffer {
        sample_rate: rate,
        channels: 2,
        samples: (0..frames)
            .flat_map(|frame| {
                let value = sample(frame as f64 / f64::from(rate)) as f32;
                [value, value]
            })
            .collect(),
    }
}

fn db(db: f64) -> f64 {
    10.0_f64.powf(db / 20.0)
}

/// Test material with a high crest factor, as a voice over music has: a
/// 220 Hz tone whose level swells and fades every two seconds, under a
/// 10 ms, 3 kHz hit 18 dB over it every second.
fn program(seconds: f64, level_db: f64) -> AudioBuffer {
    let level = db(level_db);
    stereo(seconds, |t| {
        let swell = 0.55 + 0.45 * (TAU * 0.5 * t).sin();
        let hit = if t % 1.0 < 0.01 {
            8.0 * (TAU * 3000.0 * t).sin()
        } else {
            0.0
        };
        level * (swell * (TAU * 220.0 * t).sin() + hit)
    })
}

#[test]
fn a_sine_reads_its_level_as_lufs() {
    // EBU Tech 3341 cases 1 and 2: a 1 kHz sine at -23 (and -33) dBFS on
    // both channels measures -23 (-33) LUFS.
    for level in [-23.0, -33.0] {
        let loudness = measure_loudness(&stereo(4.0, |t| db(level) * (TAU * 1000.0 * t).sin()));
        let integrated = loudness.integrated_lufs.unwrap();
        assert!((integrated - level).abs() <= 0.1, "{integrated} LUFS");
    }
}

#[test]
fn the_true_peak_counts_peaks_between_samples() {
    // A quarter-rate sine 45° off the sample grid: every sample is at
    // -3 dBFS, the wave between them at 0.
    let buffer = stereo(0.5, |t| 0.5 * (TAU * 12_000.0 * t + TAU / 8.0).sin());
    let peak = measure_loudness(&buffer).true_peak_dbtp.unwrap();
    assert!((peak - db_of(0.5)).abs() <= 0.2, "{peak} dBTP");
}

fn db_of(amplitude: f64) -> f64 {
    20.0 * amplitude.log10()
}

#[test]
fn silence_and_short_sounds_have_no_integrated_loudness() {
    let silent = measure_loudness(&stereo(2.0, |_| 0.0));
    assert_eq!(silent.integrated_lufs, None);
    assert_eq!(silent.true_peak_dbtp, None);

    // Shorter than one 400 ms gating block.
    let short = measure_loudness(&stereo(0.3, |t| 0.5 * (TAU * 1000.0 * t).sin()));
    assert_eq!(short.integrated_lufs, None);
    let peak = short.true_peak_dbtp.unwrap();
    assert!((peak - db_of(0.5)).abs() <= 0.1, "{peak} dBTP");

    let empty = AudioBuffer {
        sample_rate: 48_000,
        channels: 2,
        samples: Vec::new(),
    };
    assert_eq!(measure_loudness(&empty).integrated_lufs, None);
}

/// Escapes `path` for an option value inside a filtergraph description:
/// once for the option parser and once more for the graph parser.
fn filter_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .replace(':', "\\\\\\:")
}

/// The last integrated loudness and running true peak FFmpeg's `ebur128`
/// filter reports for `path`.
fn ffmpeg_ebur128(path: &Path, directory: &Path) -> (f64, f64) {
    let metadata = directory.join("ebur128.txt");
    FfmpegContext::builder()
        .input(Input::from(ffmpeg_path(path)))
        .output(
            Output::from(ffmpeg_path(&directory.join("null.wav")))
                .set_format("null")
                .set_audio_filter(format!(
                    "ebur128=peak=true:metadata=1,ametadata=mode=print:file={}",
                    filter_path(&metadata)
                )),
        )
        .build()
        .unwrap()
        .start()
        .unwrap()
        .wait()
        .unwrap();
    let text = fs::read_to_string(metadata).unwrap();
    let last = |key: &str| -> f64 {
        text.lines()
            .rev()
            .find_map(|line| line.strip_prefix(key))
            .unwrap_or_else(|| panic!("no {key} in ebur128 output"))
            .parse()
            .unwrap()
    };
    (last("lavfi.r128.I="), db_of(last("lavfi.r128.true_peak=")))
}

#[test]
fn measures_as_ffmpeg_ebur128_does() {
    let directory = fixture_dir("loudness");
    let source = directory.join("program.wav");
    // Two channels apart, a quiet stretch for the relative gate, and an
    // off-grid 11 kHz burst whose true peak sits between samples.
    FfmpegContext::builder()
        .input(
            Input::from(
                "aevalsrc=exprs='0.3*sin(2*PI*220*t)*(0.55+0.45*sin(2*PI*0.5*t))+0.4*sin(2*PI*11000*t+PI/4)*between(mod(t,2),1.5,1.55)|0.2*sin(2*PI*330*t)*lt(t,4)+0.01*sin(2*PI*90*t)':sample_rate=48000:duration=6",
            )
            .set_format("lavfi"),
        )
        .output(Output::from(ffmpeg_path(&source)).set_audio_codec("pcm_f32le"))
        .build()
        .unwrap()
        .start()
        .unwrap()
        .wait()
        .unwrap();

    let audio = FfmpegBackend::new()
        .decode_audio(&source, 48_000, 2)
        .unwrap();
    let ours = measure_loudness(&audio);
    let (integrated, true_peak) = ffmpeg_ebur128(&source, &directory);
    let ours_integrated = ours.integrated_lufs.unwrap();
    let ours_peak = ours.true_peak_dbtp.unwrap();
    // FFmpeg prints the loudness to 0.001 LU and the linear peak to 0.001.
    assert!(
        (ours_integrated - integrated).abs() <= 0.05,
        "{ours_integrated} LUFS, FFmpeg {integrated}"
    );
    // FFmpeg reads the peak at 4x (192 kHz), between fewer points than our
    // 8x, so it can read a little lower; EBU Tech 3341 allows +0.2/-0.4 dB.
    assert!(
        (-0.2..=0.4).contains(&(ours_peak - true_peak)),
        "{ours_peak} dBTP, FFmpeg {true_peak}"
    );

    fs::remove_dir_all(&directory).ok();
}

#[test]
fn a_quiet_mix_is_turned_up_to_the_target() {
    let mut buffer = program(4.0, -30.0);
    let before = buffer.clone();
    let input = measure_loudness(&buffer).integrated_lufs.unwrap();
    let (gain, loudness) =
        normalize_loudness(&mut buffer, &LoudnessTarget::BROADCAST, || false).unwrap();

    let integrated = loudness.integrated_lufs.unwrap();
    assert!(
        (integrated + 23.0).abs() <= TARGET_TOLERANCE_LU,
        "{integrated}"
    );
    assert!((gain - (-23.0 - input)).abs() <= 0.01, "{gain} dB");
    assert!(loudness.true_peak_dbtp.unwrap() <= -1.0);
    // Its peaks stay under the ceiling, so it is only made louder.
    let factor = db(gain) as f32;
    for (after, before) in buffer.samples.iter().zip(&before.samples) {
        assert!(
            (after - before * factor).abs() <= 1e-6,
            "{after} vs {before}"
        );
    }
}

#[test]
fn the_limiter_holds_peaks_at_the_ceiling() {
    // -14 LUFS takes the hits well over 0 dBTP.
    let mut buffer = program(4.0, -24.0);
    let (_, loudness) =
        normalize_loudness(&mut buffer, &LoudnessTarget::STREAMING, || false).unwrap();

    let integrated = loudness.integrated_lufs.unwrap();
    assert!(
        (integrated + 14.0).abs() <= TARGET_TOLERANCE_LU,
        "{integrated}"
    );
    let peak = loudness.true_peak_dbtp.unwrap();
    assert!((-1.3..=-1.0).contains(&peak), "{peak} dBTP");
    assert_eq!(measure_loudness(&buffer), loudness);
}

#[test]
fn lowering_keeps_the_peak_at_the_ceiling_instead() {
    let mut buffer = program(4.0, -24.0);
    let input = measure_loudness(&buffer);
    let target = LoudnessTarget {
        peak_handling: PeakHandling::Lower,
        ..LoudnessTarget::STREAMING
    };
    let (gain, loudness) = normalize_loudness(&mut buffer, &target, || false).unwrap();

    let expected_gain = -1.0 - input.true_peak_dbtp.unwrap();
    assert!((gain - expected_gain).abs() <= 0.01, "{gain} dB");
    let integrated = loudness.integrated_lufs.unwrap();
    assert!(
        (integrated - (input.integrated_lufs.unwrap() + expected_gain)).abs() <= 0.01,
        "{integrated}"
    );
    assert!(integrated < -14.5, "{integrated}");
    let peak = loudness.true_peak_dbtp.unwrap();
    assert!((peak + 1.0).abs() <= 0.01, "{peak}");
}

#[test]
fn a_mix_over_full_scale_is_brought_down() {
    let mut buffer = program(3.0, 0.0);
    assert!(buffer.samples.iter().any(|sample| sample.abs() > 1.0));
    let (gain, loudness) =
        normalize_loudness(&mut buffer, &LoudnessTarget::BROADCAST, || false).unwrap();

    assert!(gain < -10.0, "{gain} dB");
    let integrated = loudness.integrated_lufs.unwrap();
    assert!(
        (integrated + 23.0).abs() <= TARGET_TOLERANCE_LU,
        "{integrated}"
    );
    assert!(buffer.samples.iter().all(|sample| sample.abs() < 1.0));
}

#[test]
fn normalizing_is_deterministic() {
    let normalize = || {
        let mut buffer = program(3.0, -26.0);
        normalize_loudness(&mut buffer, &LoudnessTarget::STREAMING, || false).unwrap();
        buffer.samples
    };
    assert!(normalize() == normalize());
}

#[test]
fn silence_and_short_sounds_are_left_alone() {
    for mut buffer in [
        stereo(2.0, |_| 0.0),
        stereo(0.3, |t| 0.1 * (TAU * 1000.0 * t).sin()),
    ] {
        let before = buffer.clone();
        let (gain, loudness) =
            normalize_loudness(&mut buffer, &LoudnessTarget::STREAMING, || false).unwrap();
        assert_eq!(gain, 0.0);
        assert_eq!(loudness.integrated_lufs, None);
        assert_eq!(buffer, before);
    }
}

#[test]
fn normalizing_stops_when_cancelled() {
    let mut buffer = program(1.0, -24.0);
    assert!(matches!(
        normalize_loudness(&mut buffer, &LoudnessTarget::STREAMING, || true),
        Err(crate::error::AudioMixError::Cancelled)
    ));
}
