//! The built `celesta-codec-ffmpeg` program, driven through
//! `celesta_media::MediaProcess` as Celesta drives it: it exposes LGPL
//! components only (everything but libx264, with VP9 and Opus through the
//! BSD-licensed libvpx and libopus), its answers equal the in-process
//! codec's, and its errors keep their kind.

#[path = "support/encoding.rs"]
mod encoding;
#[path = "support/files.rs"]
mod files;

use celesta_codec_ffmpeg::{DECODER_ID, FfmpegCodec, registered};
use celesta_composition::Rational;
use celesta_media::{
    AudioDecoder, Color, Decoder, MediaBackend, MediaError, MediaProcess, MediaProvider,
    MediaRegistry, PixelFormat, ProviderFactory, ProviderUse, RegisteredPlugin, Settings,
    VideoEncoder, VideoEncoderRequest, VideoFrameDecoder,
};
use encoding::{settings, text};
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
