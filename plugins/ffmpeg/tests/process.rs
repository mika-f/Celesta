//! The built `celesta-codec-ffmpeg` program, driven through
//! `celesta_media::MediaProcess` as Celesta drives it: it exposes LGPL
//! components only (everything but libx264, with VP9 and Opus through the
//! BSD-licensed libvpx and libopus), its answers equal the in-process
//! codec's, its errors keep their kind, and it encodes and muxes exports.

#[path = "support/encoding.rs"]
mod encoding;
#[path = "support/files.rs"]
mod files;

use celesta_codec_ffmpeg::{DECODER_ID, FfmpegCodec, registered};
use celesta_composition::Rational;
use celesta_media::{
    AudioDecoder, AudioEncoderRequest, Color, DecodedPixelFormat, Decoder, FrameTransport,
    MediaBackend, MediaError, MediaProcess, MediaProvider, MediaRegistry, MuxerRequest,
    PixelFormat, ProviderFactory, ProviderUse, RegisteredPlugin, Settings, Track, TrackPacket,
    VideoEncoder, VideoEncoderRequest, VideoFrameDecoder, ring_layout,
};
use encoding::{FRAMES, HEIGHT, WIDTH, encode_frames, little_endian, open, settings, text};
use ez_ffmpeg::{FfmpegContext, Input, Output};
use files::generate;
use std::path::PathBuf;
use std::sync::Arc;

fn executable() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_celesta-codec-ffmpeg"))
}

/// The program, started as Celesta starts a media plugin for `purpose`.
fn started(purpose: ProviderUse) -> Box<dyn MediaProvider> {
    let executable = executable();
    let directory = executable.parent().unwrap().to_path_buf();
    MediaProcess::factory(executable, directory)(&Default::default(), purpose).unwrap()
}

/// The program, started to decode.
fn provider() -> Box<dyn MediaProvider> {
    started(ProviderUse::Decode)
}

/// A backend on the FFmpeg plugin made by `factory`.
fn backend(factory: ProviderFactory) -> MediaBackend {
    MediaBackend::with_registry(Arc::new(MediaRegistry::new(vec![RegisteredPlugin {
        factory,
        ..registered()
    }])))
}

/// A backend on the program.
fn program_backend() -> MediaBackend {
    let executable = executable();
    let directory = executable.parent().unwrap().to_path_buf();
    backend(MediaProcess::factory(executable, directory))
}

fn assert_ring_export(muxer: &str, video: &str, with_opus: bool) {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("clip").with_extension(muxer);
    let executable = executable();
    let format = PixelFormat::Rgba16;
    let mut program = MediaProcess::start_with(
        &executable,
        executable.parent().unwrap(),
        &Default::default(),
        FrameTransport::SharedMemory,
        ProviderUse::Export {
            ring: ring_layout(&[format], WIDTH, HEIGHT),
        },
    )
    .unwrap();
    let opened = open(&mut program, video, &Settings::new(), &[format]).unwrap();
    assert_eq!(opened.input, format);
    assert_eq!(
        program.pipeline().unwrap().frames_in_ring(),
        cfg!(any(unix, windows)),
        "{video} RGBA16 ring"
    );
    let alpha = opened.alpha;
    let frames = (0..FRAMES).map(|index| {
        let step = index as u8 * 20;
        let pixel = [step, 64, 255 - step, 128].map(|high| u16::from_be_bytes([high, 85]));
        little_endian(pixel).repeat((WIDTH * HEIGHT) as usize)
    });
    let (video_packets, config) = encode_frames(&mut program, opened.session, frames);
    let mut tracks = vec![Track::Video {
        config,
        timescale: 10,
        width: WIDTH,
        height: HEIGHT,
        frame_rate: Rational::new(10, 1),
        color: Color::bt709_limited(),
        alpha,
    }];
    let mut packets: Vec<_> = video_packets
        .into_iter()
        .map(|packet| TrackPacket { track: 0, packet })
        .collect();
    if with_opus {
        let encoder = program.audio_encoder().unwrap();
        let opened = encoder
            .open_audio(&AudioEncoderRequest {
                component: "opus",
                settings: &Settings::new(),
                sample_rate: 48_000,
                channels: 1,
            })
            .unwrap();
        let samples: Vec<_> = (0..48_000)
            .map(|index| (index as f32 * 0.05).sin() * 0.5)
            .collect();
        let encoded = encoder.encode_audio(opened.session, 0, &samples).unwrap();
        let finished = encoder.finish_audio(opened.session).unwrap();
        tracks.push(Track::Audio {
            config: encoded
                .config
                .or(finished.config)
                .expect("Opus track config"),
            timescale: opened.sample_rate,
            sample_rate: opened.sample_rate,
            channels: opened.channels,
            delay: opened.delay,
        });
        packets.extend(
            encoded
                .packets
                .into_iter()
                .chain(finished.packets)
                .map(|packet| TrackPacket { track: 1, packet }),
        );
    }
    let program_muxer = program.muxer().unwrap();
    let session = program_muxer
        .open_muxer(&MuxerRequest {
            component: muxer,
            settings: &Settings::new(),
            output: &output,
            tracks: &tracks,
            leading_audio: 0,
        })
        .unwrap();
    program_muxer.write_packets(session, &packets).unwrap();
    assert_eq!(
        program_muxer.finish_muxer(session).unwrap(),
        std::slice::from_ref(&output)
    );
    drop(program);

    let mut reader = program_backend();
    let probe = reader.probe(&output).unwrap();
    let stream = probe.video.as_ref().unwrap();
    assert_eq!(stream.codec.as_deref(), Some(video));
    assert_eq!((stream.width, stream.height), (WIDTH, HEIGHT));
    if with_opus {
        assert_eq!(probe.audio.len(), 1);
        assert_eq!(probe.audio[0].codec.as_deref(), Some("opus"));
        assert_eq!(probe.audio[0].sample_rate, Some(48_000));
        let audio = reader.decode_audio(&output, 48_000, 1).unwrap();
        assert!(audio.frame_count().abs_diff(48_000) < 960);
    } else {
        assert!(probe.audio.is_empty());
    }
    let first = reader.decode_frame(&output, 0.0).unwrap();
    let last = reader.decode_frame(&output, 0.9).unwrap();
    assert_eq!(first.pixel_format, DecodedPixelFormat::Rgba8);
    assert_eq!(last.pixel_format, DecodedPixelFormat::Rgba8);
    assert!(
        first.pixels[0] < 60 && first.pixels[2] > 190,
        "{video}: first pixel {:?}",
        &first.pixels[..4]
    );
    assert!(
        last.pixels[0] > 130 && last.pixels[2] < 120,
        "{video}: last pixel {:?}",
        &last.pixels[..4]
    );
}

#[test]
fn the_standard_program_encodes_prores_and_muxes_mov_through_the_ring() {
    assert_ring_export("mov", "prores", false);
}

#[test]
fn the_standard_program_encodes_vp9_and_opus_and_muxes_webm_through_the_ring() {
    assert_ring_export("webm", "vp9", true);
}

#[test]
fn manifest_and_describe_exclude_x264() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_celesta-codec-ffmpeg"))
        .arg(celesta_plugin_host::MANIFEST_FLAG)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let manifest: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(manifest["id"], "ffmpeg");
    assert_eq!(manifest["name"], "FFmpeg");
    let mut codec = FfmpegCodec::new();
    let components = codec.describe().unwrap();
    assert_eq!(components.len(), 9);
    assert!(components.iter().all(|component| component.id != "libx264"));
    for id in ["vp9", "opus"] {
        assert!(
            components.iter().any(|component| component.id == id),
            "{id} is served by the standard build"
        );
    }
    let declared = registered();
    assert_eq!(declared.media.video_encoders.len(), 2);
    assert_eq!(declared.media.audio_encoders.len(), 3);
    let refused = codec.open_video(&VideoEncoderRequest {
        component: "libx264",
        settings: &Settings::new(),
        width: 2,
        height: 2,
        frame_rate: Rational::new(1, 1),
        timescale: 1,
        offered: &[PixelFormat::Rgba8],
        color: &Color::bt709_limited(),
    });
    assert!(
        matches!(&refused, Err(MediaError::Codec(message))
            if message == "no video encoder \"libx264\" in ffmpeg"),
        "{refused:?}"
    );
}

#[test]
fn the_x264_provider_exposes_only_its_encoder() {
    let mut provider = FfmpegCodec::x264();
    let described = provider.describe().unwrap();
    assert_eq!(described.len(), 1);
    assert_eq!(described[0].id, "libx264");
    assert!(provider.decoder().is_none());
    assert!(provider.audio_encoder().is_none());
    assert!(provider.muxer().is_none());
    for component in ["vp9", "prores"] {
        let refused = provider.open_video(&VideoEncoderRequest {
            component,
            settings: &Settings::new(),
            width: 2,
            height: 2,
            frame_rate: Rational::new(1, 1),
            timescale: 1,
            offered: &[PixelFormat::Rgba16],
            color: &Color::bt709_limited(),
        });
        // The refusal names the program that refused, not the standard one.
        match refused {
            Err(MediaError::Codec(message)) => assert_eq!(
                message,
                format!("no video encoder {component:?} in ffmpeg-x264"),
                "{component} is not the x264 build's"
            ),
            other => panic!("{component} is not the x264 build's: {other:?}"),
        }
    }
}

#[test]
fn the_program_answers_as_the_codec_in_process_does() {
    let directory = tempfile::tempdir().unwrap();
    let video = directory.path().join("clip.mkv");
    generate(&video, "testsrc2=size=16x8:rate=10:duration=1", "ffv1");
    let audio = directory.path().join("tone.wav");
    FfmpegContext::builder()
        .input(Input::from("sine=frequency=440:sample_rate=48000:duration=0.5").set_format("lavfi"))
        .output(Output::from(audio.to_string_lossy().into_owned()))
        .build()
        .unwrap()
        .start()
        .unwrap()
        .wait()
        .unwrap();

    let mut remote = program_backend().with_sequential_video(Rational::new(10, 1));
    let mut local = backend(FfmpegCodec::factory()).with_sequential_video(Rational::new(10, 1));
    assert_eq!(remote.probe(&video).unwrap(), local.probe(&video).unwrap());
    assert_eq!(remote.probe(&audio).unwrap(), local.probe(&audio).unwrap());
    // Out of order, and a sequential run moving forward.
    for time in [0.5, 0.1, 0.2, 0.3, 0.9] {
        assert_eq!(
            remote.decode_frame(&video, time).unwrap(),
            local.decode_frame(&video, time).unwrap(),
            "{time}"
        );
        assert_eq!(
            remote.decode_frame_for("layer", &video, time).unwrap(),
            local.decode_frame_for("layer", &video, time).unwrap(),
            "{time}"
        );
    }
    assert_eq!(
        remote.decode_audio(&audio, 48_000, 2).unwrap(),
        local.decode_audio(&audio, 48_000, 2).unwrap()
    );
    assert_eq!(remote.video_duration(&video).unwrap(), Some(1.0));

    let missing = directory.path().join("missing.mp4");
    assert!(matches!(
        remote.probe(&missing),
        Err(MediaError::NotFound(path)) if path == missing
    ));
    assert!(matches!(
        remote.decode_frame(&audio, 0.0),
        Err(MediaError::NoVideoStream(path)) if path == audio
    ));
}

#[test]
fn the_bit_depth_is_not_a_prores_setting() {
    // The bit depth is libx264's alone (the x264 plugin's test).
    let mut program = started(ProviderUse::Export { ring: None });
    let prores = program
        .video_encoder()
        .unwrap()
        .open_video(&VideoEncoderRequest {
            component: "prores",
            settings: &settings(&[("bitDepth", text("12"))]),
            width: 2,
            height: 2,
            frame_rate: Rational::new(1, 1),
            timescale: 1,
            offered: &[PixelFormat::Rgba16],
            color: &Color::bt709_limited(),
        });
    match prores {
        Err(MediaError::InvalidSettings { key, .. }) => assert_eq!(key, "bitDepth"),
        other => panic!("expected invalid settings, got {other:?}"),
    }
}

#[test]
fn the_program_answers_the_decoder_it_names() {
    let directory = tempfile::tempdir().unwrap();
    let video = directory.path().join("clip.mkv");
    generate(&video, "color=c=red:s=4x2:r=5:d=1", "ffv1");
    let mut process = provider();
    let process = process.decoder().unwrap();
    let mut local = FfmpegCodec::new();
    assert_eq!(
        process.probe(DECODER_ID, &video).unwrap(),
        local.probe(DECODER_ID, &video).unwrap()
    );
    let message = match process.probe("other", &video) {
        Err(MediaError::Codec(message)) => message,
        other => panic!("expected a codec error, got {other:?}"),
    };
    assert!(message.contains("no decoder"), "{message}");
}

#[test]
fn a_text_file_is_unsupported() {
    let directory = tempfile::tempdir().unwrap();
    let text = directory.path().join("hello.mp4");
    std::fs::write(&text, "hello").unwrap();

    // The probe answers that FFmpeg does not read it, in the program as in
    // this process.
    assert!(matches!(
        provider().decoder().unwrap().probe(DECODER_ID, &text),
        Err(MediaError::Unsupported { path, .. }) if path == text
    ));
    let mut local = FfmpegCodec::new();
    assert!(matches!(
        local.probe(DECODER_ID, &text),
        Err(MediaError::Unsupported { path, .. }) if path == text
    ));
    // A decode does not: `unsupported` answers only a probe.
    let request = celesta_media::VideoRequest {
        accepted_pixel_formats: &[],
        path: &text,
        time: 0.0,
        sequential: None,
    };
    assert!(matches!(
        local.decode_video(DECODER_ID, &request),
        Err(MediaError::Codec(_))
    ));
    // No other decoder is installed, so Celesta says none could read it.
    assert!(matches!(
        program_backend().probe(&text),
        Err(MediaError::Unsupported { path, tried }) if path == text && tried == ["ffmpeg/ffmpeg"]
    ));
}
