use crate::decoder::{AudioDecoder, VideoFrameDecoder};
use crate::error::{AudioMixError, MediaError};
use crate::ffmpeg::FfmpegBackend;
use crate::mix::{frame_byte_len, mix_audio_graph, mix_audio_graph_cancellable};
use crate::sequential::path_to_url;
use crate::types::AudioBuffer;
use celesta_composition::{AudioGraph, Rational, Time};
use ez_ffmpeg::Input;
use std::path::{Path, PathBuf};

use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use celesta_composition::{Animatable, AssetLocation, AudioClip, ResolvedAsset, TimeRange};
use ez_ffmpeg::{FfmpegContext, Output};

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
        fn decode_audio(&mut self, _: &Path, _: u32, _: u16) -> Result<AudioBuffer, MediaError> {
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
