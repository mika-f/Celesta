use crate::color::conversion;
use crate::decoder::FfmpegBackend;
use crate::probe::displayed_size;
use crate::sequential::ffmpeg_path;
use celesta_composition::Rational;
use celesta_media::{
    AudioDecoder, DynamicRange, MediaError, VideoColor, VideoFrame, VideoFrameDecoder,
    frame_byte_len,
};
use ez_ffmpeg::Input;
use std::path::{Path, PathBuf};

use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use ez_ffmpeg::{FfmpegContext, Output};

mod loudness;

/// Renders a synthetic `lavfi` source to a lossless MKV fixture through
/// the linked FFmpeg libraries, forcing the output frame rate so the
/// container records a real frame-duration hint.
fn generate_clip(path: &Path, lavfi: &str, fps: (i32, i32)) {
    FfmpegContext::builder()
        .input(Input::from(lavfi).set_format("lavfi"))
        .output(
            Output::from(ffmpeg_path(path))
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
        "celesta-codec-ffmpeg-{label}-{}-{suffix}",
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
fn reports_a_missing_source_with_its_path() {
    let path = Path::new("/definitely-not-a-real/celesta-media.mkv");
    let mut backend = FfmpegBackend::new();
    for error in [
        backend.probe(path).err().unwrap(),
        backend.decode_frame(path, 0.0).err().unwrap(),
        backend.decode_audio(path, 48_000, 2).err().unwrap(),
    ] {
        assert!(matches!(&error, MediaError::NotFound(missing) if missing == path));
        assert_eq!(
            error.to_string(),
            "media file /definitely-not-a-real/celesta-media.mkv was not found"
        );
    }
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
fn loops_video_from_its_first_frame_past_the_source_end() {
    let directory = fixture_dir("loop");
    let clip = directory.join("clip.mkv");
    generate_clip(&clip, "testsrc=size=64x64:rate=10:duration=1", (10, 1));

    let mut backend = FfmpegBackend::new();
    let duration = backend.video_duration(&clip).unwrap().unwrap();
    assert!((duration - 1.0).abs() < 1e-9, "video duration {duration}");
    // The last frame still shows just before the end; at the end, and a
    // whole pass later, the first one does again.
    assert!((backend.loop_source_time(&clip, 0.95).unwrap() - 0.95).abs() < 1e-9);
    assert!(backend.loop_source_time(&clip, 1.0).unwrap().abs() < 1e-9);
    assert!((backend.loop_source_time(&clip, 2.3).unwrap() - 0.3).abs() < 1e-9);

    let mut sequential = FfmpegBackend::new().with_sequential_video(Rational::new(10, 1));
    let first = sequential.decode_frame_for("clip", &clip, 0.0).unwrap();
    let third = sequential.decode_frame_for("clip", &clip, 0.2).unwrap();
    let wrapped = sequential.loop_source_time(&clip, 1.0).unwrap();
    assert_eq!(
        sequential
            .decode_frame_for("clip", &clip, wrapped)
            .unwrap()
            .pixels,
        first.pixels
    );
    let wrapped = sequential.loop_source_time(&clip, 2.2).unwrap();
    assert_eq!(
        sequential
            .decode_frame_for("clip", &clip, wrapped)
            .unwrap()
            .pixels,
        third.pixels
    );

    fs::remove_dir_all(&directory).ok();
}

/// Rewrites the identity matrix in a MOV's track header (`tkhd`) to the
/// display matrix a portrait iPhone recording carries: show the coded
/// frame rotated 90° clockwise. This is what `ffmpeg -display_rotation`
/// changes, and the mov muxer has no other way to be told it.
fn tag_portrait_rotation(path: &Path) {
    const IDENTITY: [u32; 9] = [0x1_0000, 0, 0, 0, 0x1_0000, 0, 0, 0, 0x4000_0000];
    const PORTRAIT: [u32; 9] = [0, 0x1_0000, 0, 0xFFFF_0000, 0, 0, 0, 0, 0x4000_0000];
    let encode = |matrix: [u32; 9]| -> Vec<u8> {
        matrix
            .iter()
            .flat_map(|value| value.to_be_bytes())
            .collect()
    };
    let identity = encode(IDENTITY);
    let mut bytes = fs::read(path).unwrap();
    let tkhd = bytes
        .windows(4)
        .position(|window| window == b"tkhd")
        .expect("MOV has a track header");
    let offset = tkhd
        + bytes[tkhd..]
            .windows(identity.len())
            .position(|window| window == identity)
            .expect("track header has an identity matrix");
    bytes[offset..offset + identity.len()].copy_from_slice(&encode(PORTRAIT));
    fs::write(path, bytes).unwrap();
}

/// `frame`'s RGBA pixels turned 90° clockwise.
fn rotate_clockwise(frame: &VideoFrame) -> Vec<u8> {
    let (width, height) = (frame.width as usize, frame.height as usize);
    let mut rotated = Vec::with_capacity(frame.pixels.len());
    for y in 0..width {
        for x in 0..height {
            let source = ((height - 1 - x) * width + y) * 4;
            rotated.extend_from_slice(&frame.pixels[source..source + 4]);
        }
    }
    rotated
}

#[test]
fn probes_and_decodes_a_rotated_video_at_its_displayed_size() {
    // A 64x32 coding tagged like a portrait iPhone recording displays as
    // 32x64, and FFmpeg's autorotation delivers its frames that way.
    let directory = fixture_dir("rotated");
    let plain = directory.join("plain.mov");
    let rotated = directory.join("rotated.mov");
    generate_clip(&plain, "testsrc=size=64x32:rate=10:duration=1", (10, 1));
    fs::copy(&plain, &rotated).unwrap();
    tag_portrait_rotation(&rotated);

    let mut backend = FfmpegBackend::new();
    let video = backend.probe(&rotated).unwrap().video.clone().unwrap();
    assert_eq!((video.width, video.height), (32, 64));

    let upright = rotate_clockwise(&backend.decode_frame(&plain, 0.5).unwrap());
    let one_shot = backend.decode_frame(&rotated, 0.5).unwrap();
    assert_eq!((one_shot.width, one_shot.height), (32, 64));
    assert_eq!(*one_shot.pixels, upright);

    let mut sequential = FfmpegBackend::new().with_sequential_video(Rational::new(10, 1));
    let mut frame = None;
    for step in 0..=5 {
        frame = Some(
            sequential
                .decode_frame_for("clip", &rotated, f64::from(step) * 0.1)
                .unwrap_or_else(|error| panic!("frame {step} should decode: {error}")),
        );
    }
    let frame = frame.unwrap();
    assert_eq!((frame.width, frame.height), (32, 64));
    assert_eq!(*frame.pixels, upright);

    fs::remove_dir_all(&directory).ok();
}

/// A 10-bit limited-range gray clip of luma code `luma`, tagged BT.2020
/// with `transfer`.
fn generate_hdr_clip(path: &Path, luma: u16, transfer: &str) {
    generate_clip(
        path,
        &format!(
            "color=s=32x16:r=10:d=1,format=yuv420p10le,geq=lum={luma}:cb=512:cr=512,\
             setparams=color_primaries=bt2020:color_trc={transfer}:colorspace=bt2020nc:range=tv"
        ),
        (10, 1),
    );
}

fn center_pixel(frame: &crate::VideoFrame) -> [u8; 4] {
    let offset = ((frame.height / 2 * frame.width + frame.width / 2) * 4) as usize;
    frame.pixels[offset..offset + 4].try_into().unwrap()
}

#[test]
fn probes_and_tone_maps_hdr_video_like_chrome() {
    let directory = fixture_dir("hdr");
    // Luma 573 is PQ's 203 nits and 721 HLG's 75% signal: both the HDR
    // reference white, which Chrome draws at half of SDR white's light.
    for (name, luma, transfer, range) in [
        ("pq.mkv", 573, "smpte2084", DynamicRange::Pq),
        ("hlg.mkv", 721, "arib-std-b67", DynamicRange::Hlg),
    ] {
        let clip = directory.join(name);
        generate_hdr_clip(&clip, luma, transfer);

        let mut backend = FfmpegBackend::new();
        let video = backend.probe(&clip).unwrap().video.clone().unwrap();
        assert_eq!(
            video.color,
            VideoColor {
                primaries: Some("bt2020".to_owned()),
                transfer: Some(transfer.to_owned()),
                matrix: Some("bt2020nc".to_owned()),
            }
        );
        assert_eq!(video.color.dynamic_range(), range);

        let frame = backend.decode_frame(&clip, 0.5).unwrap();
        assert_eq!((frame.width, frame.height), (32, 16));
        let [r, g, b, a] = center_pixel(&frame);
        assert!(
            [r, g, b].iter().all(|channel| channel.abs_diff(188) <= 2) && a == 255,
            "{name} reference white decoded as {:?}",
            [r, g, b, a]
        );

        // Sequential playback converts the same way, frame after frame.
        let mut sequential = FfmpegBackend::new().with_sequential_video(Rational::new(10, 1));
        for step in 0..10 {
            let next = sequential
                .decode_frame_for("clip", &clip, f64::from(step) * 0.1)
                .unwrap_or_else(|error| panic!("{name} frame {step} should decode: {error}"));
            assert_eq!(next.pixels, frame.pixels, "{name} frame {step}");
        }
        assert_eq!(sequential.sequential_video_processes_started(), 1);
    }
    fs::remove_dir_all(&directory).ok();
}

#[test]
fn tone_maps_a_rotated_hdr_video_at_its_displayed_size() {
    // The HDR conversion decodes through its own filter graph; autorotation
    // turns its frames too, so they must match the rotated probe.
    let directory = fixture_dir("rotated-hdr");
    let clip = directory.join("pq.mov");
    generate_hdr_clip(&clip, 573, "smpte2084");
    tag_portrait_rotation(&clip);

    let mut backend = FfmpegBackend::new();
    let video = backend.probe(&clip).unwrap().video.clone().unwrap();
    assert_eq!(video.color.dynamic_range(), DynamicRange::Pq);
    assert_eq!((video.width, video.height), (16, 32));

    let frame = backend.decode_frame(&clip, 0.5).unwrap();
    assert_eq!((frame.width, frame.height), (16, 32));
    let [r, g, b, _] = center_pixel(&frame);
    assert!(
        [r, g, b].iter().all(|channel| channel.abs_diff(188) <= 2),
        "reference white decoded as {:?}",
        [r, g, b]
    );

    let mut sequential = FfmpegBackend::new().with_sequential_video(Rational::new(10, 1));
    for step in 0..=5 {
        let next = sequential
            .decode_frame_for("clip", &clip, f64::from(step) * 0.1)
            .unwrap_or_else(|error| panic!("frame {step} should decode: {error}"));
        assert_eq!(next.pixels, frame.pixels, "frame {step}");
    }

    fs::remove_dir_all(&directory).ok();
}

#[test]
fn swaps_the_probed_size_for_quarter_turn_rotations_of_either_sign() {
    for rotate in [90, 270, -90, -270, 450] {
        assert_eq!(displayed_size(64, 32, rotate), (32, 64), "rotate {rotate}");
    }
    for rotate in [0, 180, -180, 360] {
        assert_eq!(displayed_size(64, 32, rotate), (64, 32), "rotate {rotate}");
    }
}

#[test]
fn names_the_decoded_size_in_a_frame_size_mismatch() {
    let error = MediaError::UnexpectedFrameSize {
        width: 360,
        height: 640,
        actual: 921_600,
        expected_width: 640,
        expected_height: 360,
        expected: 921_600,
    };
    assert_eq!(
        error.to_string(),
        "decoded 360x640 RGBA frame has 921600 bytes, expected 640x360 (921600 bytes)"
    );
}

#[test]
fn leaves_sdr_video_on_the_packed_path() {
    let directory = fixture_dir("sdr-color");
    let clip = directory.join("clip.mkv");
    generate_clip(
        &clip,
        "color=c=0x949494:s=32x16:r=10:d=1,format=yuv420p,\
         setparams=color_primaries=bt709:color_trc=bt709:colorspace=bt709:range=tv",
        (10, 1),
    );
    let mut backend = FfmpegBackend::new();
    let video = backend.probe(&clip).unwrap().video.clone().unwrap();
    assert_eq!(video.color.dynamic_range(), DynamicRange::Sdr);
    assert_eq!(conversion(&video.color), None);
    let frame = backend.decode_frame(&clip, 0.0).unwrap();
    let [r, g, b, _] = center_pixel(&frame);
    assert!([r, g, b].iter().all(|channel| channel.abs_diff(148) <= 1));
    fs::remove_dir_all(&directory).ok();
}

#[test]
fn decodes_aac_without_the_encoder_padding_after_its_last_sample() {
    let directory = fixture_dir("aac");
    let tone = directory.join("tone.m4a");
    // One second is 46.875 AAC frames, so the encoder pads the last one.
    FfmpegContext::builder()
        .input(Input::from("sine=frequency=440:sample_rate=48000:duration=1").set_format("lavfi"))
        .output(Output::from(ffmpeg_path(&tone)).set_audio_codec("aac"))
        .build()
        .unwrap()
        .start()
        .unwrap()
        .wait()
        .unwrap();

    let audio = FfmpegBackend::new().decode_audio(&tone, 48_000, 2).unwrap();
    assert_eq!(audio.frame_count(), 48_000);

    fs::remove_dir_all(&directory).ok();
}
