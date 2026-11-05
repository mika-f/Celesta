use super::fixture_dir;
use crate::decoder::FfmpegBackend;
use crate::sequential::ffmpeg_path;
use celesta_media::{AudioDecoder, measure_loudness};
use ez_ffmpeg::{FfmpegContext, Input, Output};
use std::fs;
use std::path::Path;

fn db_of(amplitude: f64) -> f64 {
    20.0 * amplitude.log10()
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
