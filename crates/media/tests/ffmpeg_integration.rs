use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use ez_ffmpeg::{FfmpegContext, Input, Output};
use celesta_composition::Rational;
use celesta_media::{AudioDecoder, FfmpegBackend, VideoFrameDecoder};

/// Renders a synthetic `lavfi` source to a lossless fixture through the linked
/// FFmpeg libraries (`ffv1` in MKV for video, WAV for audio). `fps`, when set,
/// forces the output stream's frame rate so the container records a real
/// frame-duration hint (short synthetic clips otherwise leave the demuxer with
/// nothing but the container time base).
fn generate(path: &Path, lavfi: &str, video_codec: Option<&str>, fps: Option<(i32, i32)>) {
    let mut output = Output::from(path.to_string_lossy().into_owned());
    if let Some(codec) = video_codec {
        output = output.set_video_codec(codec);
    }
    if let Some((num, den)) = fps {
        output = output.set_framerate(num, den);
    }
    FfmpegContext::builder()
        .input(Input::from(lavfi).set_format("lavfi"))
        .output(output)
        .build()
        .unwrap()
        .start()
        .unwrap()
        .wait()
        .unwrap();
}

fn fixture_dir(label: &str) -> std::path::PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "celesta-media-{label}-{}-{suffix}",
        std::process::id()
    ));
    std::fs::create_dir(&directory).unwrap();
    directory
}

#[test]
fn probes_and_decodes_a_generated_video() {
    let directory = fixture_dir("decode");
    let video_path = directory.join("fixture.mkv");
    generate(
        &video_path,
        "color=c=red:s=4x2:r=2:d=1",
        Some("ffv1"),
        Some((2, 1)),
    );

    let mut backend = FfmpegBackend::new();
    let probe = backend.probe(&video_path).unwrap();
    let video = probe.video.as_ref().unwrap();
    assert_eq!((video.width, video.height), (4, 2));
    assert_eq!(video.frame_rate, Some(Rational::new(2, 1)));

    let frame = backend.decode_frame(&video_path, 0.5).unwrap();
    assert_eq!((frame.width, frame.height), (4, 2));
    assert_eq!(frame.pixels.len(), 32);
    assert!(
        frame
            .pixels
            .chunks_exact(4)
            .all(|pixel| { pixel[0] > 200 && pixel[1] < 20 && pixel[2] < 20 && pixel[3] == 255 })
    );

    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn sequential_video_decode_reuses_one_process_and_matches_exact_time_decode() {
    let directory = fixture_dir("sequence");
    let video_path = directory.join("fixture.mkv");
    generate(
        &video_path,
        "testsrc2=size=8x4:rate=4:duration=2",
        Some("ffv1"),
        Some((4, 1)),
    );

    let mut exact = FfmpegBackend::new();
    let mut sequential = FfmpegBackend::new().with_sequential_video(Rational::new(4, 1));
    for timestamp in [0.0, 0.25, 0.5, 0.75] {
        let expected = exact.decode_frame(&video_path, timestamp).unwrap();
        let actual = sequential
            .decode_frame_for("video-layer", &video_path, timestamp)
            .unwrap();
        assert_eq!(actual, expected, "frame differed at {timestamp}");
    }
    let repeated = sequential
        .decode_frame_for("video-layer", &video_path, 0.75)
        .unwrap();
    assert_eq!(repeated, exact.decode_frame(&video_path, 0.75).unwrap());
    assert_eq!(sequential.sequential_video_processes_started(), 1);

    let mut offset_sequence = FfmpegBackend::new().with_sequential_video(Rational::new(4, 1));
    for timestamp in [0.25, 0.5, 0.75] {
        assert_eq!(
            offset_sequence
                .decode_frame_for("offset-layer", &video_path, timestamp)
                .unwrap(),
            exact.decode_frame(&video_path, timestamp).unwrap()
        );
    }
    assert_eq!(offset_sequence.sequential_video_processes_started(), 1);

    // A preview that drops frames to keep up with the playhead asks for more
    // than one step at a time. As long as the request stays inside the forward
    // walk window the open run answers it, so the irregular cadence does not
    // cost a decode run per frame.
    let mut double_speed = FfmpegBackend::new().with_sequential_video(Rational::new(4, 1));
    for timestamp in [0.0, 0.5, 0.75, 1.25] {
        assert_eq!(
            double_speed
                .decode_frame_for("fast-layer", &video_path, timestamp)
                .unwrap(),
            exact.decode_frame(&video_path, timestamp).unwrap()
        );
    }
    assert_eq!(double_speed.sequential_video_processes_started(), 1);

    // Past the walk window, seeking beats decoding everything in between.
    let mut long_jump = FfmpegBackend::new().with_sequential_video(Rational::new(4, 1));
    for timestamp in [0.0, 1.25] {
        assert_eq!(
            long_jump
                .decode_frame_for("jump-layer", &video_path, timestamp)
                .unwrap(),
            exact.decode_frame(&video_path, timestamp).unwrap()
        );
    }
    assert_eq!(long_jump.sequential_video_processes_started(), 2);

    // An open run can only move forward, so scrubbing back always re-seeks.
    let mut backwards = FfmpegBackend::new().with_sequential_video(Rational::new(4, 1));
    for timestamp in [0.75, 0.25] {
        assert_eq!(
            backwards
                .decode_frame_for("back-layer", &video_path, timestamp)
                .unwrap(),
            exact.decode_frame(&video_path, timestamp).unwrap()
        );
    }
    assert_eq!(backwards.sequential_video_processes_started(), 2);

    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn decodes_generated_audio_to_project_pcm() {
    let directory = fixture_dir("audio");
    let audio_path = directory.join("fixture.wav");
    generate(
        &audio_path,
        "sine=frequency=440:sample_rate=8000:duration=0.05",
        None,
        None,
    );

    let mut backend = FfmpegBackend::new();
    let audio = backend.decode_audio(&audio_path, 48_000, 2).unwrap();

    assert_eq!((audio.sample_rate, audio.channels), (48_000, 2));
    assert_eq!(audio.samples.len() % 2, 0);
    assert!(audio.frame_count() >= 2_300 && audio.frame_count() <= 2_500);
    assert!(audio.samples.iter().any(|sample| sample.abs() > 0.01));
    std::fs::remove_dir_all(directory).unwrap();
}
