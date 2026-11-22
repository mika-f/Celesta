use celesta_codec_ffmpeg::{DECODER_ID, FfmpegBackend};
use celesta_composition::Rational;
use celesta_media::{
    DecodedPixelFormat as F, Decoder, FrameTransport, MediaProcess, Sequential, VideoFrame,
    VideoFrameDecoder, VideoRequest,
};
use std::path::{Path, PathBuf};

#[path = "../../../crates/media/src/tests/yuv_reference.rs"]
mod yuv_reference;

#[test]
fn native_yuv_matches_independent_nearest_reference() {
    let mut exact = true;
    for name in [
        "yuv-709-limited.mp4",
        "yuv-untagged.mp4",
        "yuv-709-full.mp4",
        "yuv-odd-chroma.mp4",
        "yuv-native-nv12.nut",
        "yuv-sequence.mp4",
    ] {
        let mut backend = FfmpegBackend::new();
        backend.set_accepted_pixel_formats(&[F::Yuv420p, F::Nv12]);
        let frame = backend.decode_frame(&fixture(name), 0.0).unwrap();
        assert_ne!(frame.pixel_format, F::Rgba8);
        let reference = yuv_reference::rgba(&frame).unwrap();
        exact &= yuv_reference::exact(
            &format!("{name} CPU"),
            &frame.to_rgba().unwrap().pixels,
            &reference.pixels,
        );
    }
    assert!(
        exact,
        "CPU conversion differs from the nearest-rounded reference"
    );
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn diff(name: &str, actual: &VideoFrame, reference: &VideoFrame) {
    let rgba = actual.to_rgba().unwrap();
    assert_eq!(
        (rgba.width, rgba.height),
        (reference.width, reference.height)
    );
    let (mut maximum, mut total, mut differing) = (0, 0_u64, 0);
    for (a, b) in rgba
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .zip(reference.pixels.as_chunks::<4>().0.iter())
    {
        assert_eq!(a[3], b[3]);
        differing += usize::from(a != b);
        for c in 0..3 {
            let d = a[c].abs_diff(b[c]);
            maximum = maximum.max(d);
            total += u64::from(d);
        }
    }
    println!(
        "{name}: format={:?} max={maximum} mean={:.6} differing={differing}",
        actual.pixel_format,
        total as f64 / (rgba.width as f64 * rgba.height as f64 * 3.)
    );
    assert!(maximum <= 4, "{name}: max RGB difference {maximum}");
}

#[test]
fn native_yuv_meets_the_rgba_pixel_gate_over_both_transports() {
    for (name, format) in [
        ("yuv-709-limited.mp4", F::Yuv420p),
        ("yuv-untagged.mp4", F::Yuv420p),
        ("yuv-709-full.mp4", F::Yuv420p),
        ("yuv-odd-chroma.mp4", F::Yuv420p),
        ("yuv-native-nv12.nut", F::Nv12),
    ] {
        let path = fixture(name);
        let reference = FfmpegBackend::new().decode_frame(&path, 0.).unwrap();
        for transport in [FrameTransport::SharedMemory, FrameTransport::Pipe] {
            let exe = PathBuf::from(env!("CARGO_BIN_EXE_celesta-codec-ffmpeg"));
            let mut process = MediaProcess::start_with(
                &exe,
                exe.parent().unwrap(),
                &Default::default(),
                transport,
                celesta_media::ProviderUse::Decode,
            )
            .unwrap();
            assert_eq!(
                process.shares_frames(),
                transport == FrameTransport::SharedMemory
            );
            let frame = process
                .decode_video(
                    DECODER_ID,
                    &VideoRequest {
                        path: &path,
                        time: 0.,
                        sequential: None,
                        accepted_pixel_formats: &[F::Yuv420p, F::Nv12],
                    },
                )
                .unwrap();
            assert_eq!(frame.pixel_format, format, "{name}");
            // Unconfirmed: FFmpeg aarch64 NEON NV12→RGBA appears to skip vertical
            // chroma reconstruction (max 76 in CI job 137199105972).
            #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
            if name == "yuv-native-nv12.nut" {
                continue;
            }
            diff(name, &frame, &reference);
        }
    }
}

#[test]
fn unsupported_sources_and_offers_use_identical_rgba() {
    for name in ["yuv-odd-444.mp4", "yuv-odd-i420.mkv", "yuv-709-limited.mp4"] {
        let path = fixture(name);
        let reference = FfmpegBackend::new().decode_frame(&path, 0.).unwrap();
        let mut backend = FfmpegBackend::new();
        let accepted = if name == "yuv-709-limited.mp4" {
            vec![F::Nv12]
        } else {
            vec![F::Yuv420p, F::Nv12]
        };
        backend.set_accepted_pixel_formats(&accepted);
        let frame = backend.decode_frame(&path, 0.).unwrap();
        assert_eq!(frame.pixel_format, F::Rgba8);
        assert_eq!(frame, reference);
    }
}

#[test]
fn sequential_format_changes_seeks_and_end_of_stream_keep_the_right_frames() {
    let path = fixture("yuv-709-limited.mp4");
    let mut backend = FfmpegBackend::new().with_sequential_video(Rational::new(24, 1));
    for (time, accepted) in [
        (0., vec![F::Yuv420p]),
        (0., vec![]),
        (0., vec![F::Yuv420p]),
        (0.1, vec![F::Yuv420p]),
        (0.2, vec![F::Yuv420p]),
        (30., vec![F::Yuv420p]),
        (0., vec![F::Yuv420p]),
    ] {
        backend.set_accepted_pixel_formats(&accepted);
        let frame = backend.decode_frame_for("layer", &path, time).unwrap();
        let reference = FfmpegBackend::new().decode_frame(&path, time).unwrap();
        assert_eq!(
            frame.pixel_format,
            if accepted.is_empty() {
                F::Rgba8
            } else {
                F::Yuv420p
            }
        );
        diff("sequential", &frame, &reference);
    }
}

#[test]
fn missing_offer_keeps_legacy_rgba() {
    let path = fixture("yuv-709-limited.mp4");
    let mut codec = celesta_codec_ffmpeg::FfmpegCodec::new();
    let frame = codec
        .decode_video(
            DECODER_ID,
            &VideoRequest {
                path: &path,
                time: 0.,
                sequential: Some(Sequential {
                    session: "old",
                    frame_rate: Rational::new(24, 1),
                }),
                accepted_pixel_formats: &[],
            },
        )
        .unwrap();
    assert_eq!(frame.pixel_format, F::Rgba8);
}

#[test]
fn real_process_yuv_prefetch_selects_timestamps_and_invalidates_changed_offers() {
    let path = fixture("yuv-sequence.mp4");
    for transport in [FrameTransport::SharedMemory, FrameTransport::Pipe] {
        let exe = PathBuf::from(env!("CARGO_BIN_EXE_celesta-codec-ffmpeg"));
        let mut process = MediaProcess::start_with(
            &exe,
            exe.parent().unwrap(),
            &Default::default(),
            transport,
            celesta_media::ProviderUse::Decode,
        )
        .unwrap();
        process.set_decode_prefetch(true);
        for (index, time) in [0., 0.25, 0.5, 0.75, 1., 1.25, 1.5, 1.75, 2., 5., 0., 0.25]
            .into_iter()
            .enumerate()
        {
            let accepted = if index == 3 {
                &[][..]
            } else {
                &[F::Yuv420p][..]
            };
            let frame = process
                .decode_video(
                    DECODER_ID,
                    &VideoRequest {
                        path: &path,
                        time,
                        sequential: Some(Sequential {
                            session: "clip",
                            frame_rate: Rational::new(4, 1),
                        }),
                        accepted_pixel_formats: accepted,
                    },
                )
                .unwrap();
            assert_eq!(
                frame.pixel_format,
                if index == 3 { F::Rgba8 } else { F::Yuv420p }
            );
            let reference = FfmpegBackend::new().decode_frame(&path, time).unwrap();
            diff("prefetch", &frame, &reference);
        }
    }
}
