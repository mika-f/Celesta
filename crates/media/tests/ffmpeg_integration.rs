use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use mikan_composition::Rational;
use mikan_media::{AudioDecoder, FfmpegBackend, VideoFrameDecoder};

#[test]
fn probes_and_decodes_a_generated_video_when_ffmpeg_is_available() {
    let Some(ffmpeg) = find_executable(
        "MIKAN_FFMPEG",
        "ffmpeg",
        "/opt/homebrew/opt/ffmpeg/bin/ffmpeg",
    ) else {
        eprintln!("skipping live FFmpeg test: ffmpeg was not found");
        return;
    };
    let Some(ffprobe) = find_executable(
        "MIKAN_FFPROBE",
        "ffprobe",
        "/opt/homebrew/opt/ffmpeg/bin/ffprobe",
    ) else {
        eprintln!("skipping live FFmpeg test: ffprobe was not found");
        return;
    };

    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory =
        std::env::temp_dir().join(format!("mikan-media-{}-{suffix}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let video_path = directory.join("fixture.mkv");

    let generated = Command::new(&ffmpeg)
        .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
        .arg("color=c=red:s=4x2:r=2:d=1")
        .args(["-c:v", "ffv1"])
        .arg(&video_path)
        .status()
        .unwrap();
    assert!(generated.success());

    let mut backend = FfmpegBackend::with_executables(ffmpeg, ffprobe);
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
    let Some(ffmpeg) = find_executable(
        "MIKAN_FFMPEG",
        "ffmpeg",
        "/opt/homebrew/opt/ffmpeg/bin/ffmpeg",
    ) else {
        eprintln!("skipping sequential FFmpeg test: ffmpeg was not found");
        return;
    };
    let Some(ffprobe) = find_executable(
        "MIKAN_FFPROBE",
        "ffprobe",
        "/opt/homebrew/opt/ffmpeg/bin/ffprobe",
    ) else {
        eprintln!("skipping sequential FFmpeg test: ffprobe was not found");
        return;
    };
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory =
        std::env::temp_dir().join(format!("mikan-sequence-{}-{suffix}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let video_path = directory.join("fixture.mkv");
    let generated = Command::new(&ffmpeg)
        .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
        .arg("testsrc2=size=8x4:rate=4:duration=2")
        .args(["-c:v", "ffv1"])
        .arg(&video_path)
        .status()
        .unwrap();
    assert!(generated.success());

    let mut exact = FfmpegBackend::with_executables(&ffmpeg, &ffprobe);
    let mut sequential = FfmpegBackend::with_executables(&ffmpeg, &ffprobe)
        .with_sequential_video(Rational::new(4, 1));
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

    let mut offset_sequence = FfmpegBackend::with_executables(&ffmpeg, &ffprobe)
        .with_sequential_video(Rational::new(4, 1));
    for timestamp in [0.25, 0.5, 0.75] {
        assert_eq!(
            offset_sequence
                .decode_frame_for("offset-layer", &video_path, timestamp)
                .unwrap(),
            exact.decode_frame(&video_path, timestamp).unwrap()
        );
    }
    assert_eq!(offset_sequence.sequential_video_processes_started(), 1);

    let mut double_speed = FfmpegBackend::with_executables(&ffmpeg, &ffprobe)
        .with_sequential_video(Rational::new(4, 1));
    for timestamp in [0.0, 0.5, 1.0] {
        assert_eq!(
            double_speed
                .decode_frame_for("fast-layer", &video_path, timestamp)
                .unwrap(),
            exact.decode_frame(&video_path, timestamp).unwrap()
        );
    }
    assert_eq!(double_speed.sequential_video_processes_started(), 2);

    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn decodes_generated_audio_to_project_pcm_when_ffmpeg_is_available() {
    let Some(ffmpeg) = find_executable(
        "MIKAN_FFMPEG",
        "ffmpeg",
        "/opt/homebrew/opt/ffmpeg/bin/ffmpeg",
    ) else {
        eprintln!("skipping live FFmpeg audio test: ffmpeg was not found");
        return;
    };
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory =
        std::env::temp_dir().join(format!("mikan-audio-{}-{suffix}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let audio_path = directory.join("fixture.wav");
    let generated = Command::new(&ffmpeg)
        .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
        .arg("sine=frequency=440:sample_rate=8000:duration=0.05")
        .arg(&audio_path)
        .status()
        .unwrap();
    assert!(generated.success());

    let mut backend = FfmpegBackend::with_executables(ffmpeg, "unused-ffprobe");
    let audio = backend.decode_audio(&audio_path, 48_000, 2).unwrap();

    assert_eq!((audio.sample_rate, audio.channels), (48_000, 2));
    assert_eq!(audio.samples.len() % 2, 0);
    assert!(audio.frame_count() >= 2_300 && audio.frame_count() <= 2_500);
    assert!(audio.samples.iter().any(|sample| sample.abs() > 0.01));
    std::fs::remove_dir_all(directory).unwrap();
}

fn find_executable(environment: &str, command: &str, homebrew: &str) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(environment).map(PathBuf::from)
        && executable_works(&path)
    {
        return Some(path);
    }
    let command = PathBuf::from(command);
    if executable_works(&command) {
        return Some(command);
    }
    let homebrew = PathBuf::from(homebrew);
    executable_works(&homebrew).then_some(homebrew)
}

fn executable_works(path: &Path) -> bool {
    Command::new(path)
        .arg("-version")
        .output()
        .is_ok_and(|output| output.status.success())
}
