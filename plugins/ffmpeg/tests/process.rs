//! The built `celesta-codec-ffmpeg` program, driven through
//! `celesta_media::MediaProcess` as Celesta drives it: its answers equal the
//! in-process codec's, its errors keep their kind, an export written through
//! it reads back, and `--manifest` gives its manifest.

use celesta_codec_ffmpeg::{DECODER_ID, FfmpegCodec, registered};
use celesta_composition::Rational;
use celesta_media::{
    AudioDecoder, Container, EncoderSettings, MediaBackend, MediaError, MediaProcess,
    MediaProvider, MediaRegistry, MuxRequest, PixelFormat, ProviderFactory, RegisteredPlugin,
    VideoFrameDecoder,
};
use ez_ffmpeg::{FfmpegContext, Input, Output};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn executable() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_celesta-codec-ffmpeg"))
}

/// The program, started as Celesta starts a media plugin.
fn provider() -> Box<dyn MediaProvider> {
    let executable = executable();
    let directory = executable.parent().unwrap().to_path_buf();
    MediaProcess::factory(executable, directory)(&Default::default()).unwrap()
}

/// A backend on the FFmpeg plugin made by `factory`.
fn backend(factory: ProviderFactory) -> MediaBackend {
    MediaBackend::with_registry(Arc::new(MediaRegistry::new(vec![RegisteredPlugin {
        factory,
        ..registered()
    }])))
}

/// A backend on the program.
fn program() -> MediaBackend {
    let executable = executable();
    let directory = executable.parent().unwrap().to_path_buf();
    backend(MediaProcess::factory(executable, directory))
}

fn generate(path: &Path, lavfi: &str, codec: &str) {
    FfmpegContext::builder()
        .input(Input::from(lavfi).set_format("lavfi"))
        .output(Output::from(path.to_string_lossy().into_owned()).set_video_codec(codec))
        .build()
        .unwrap()
        .start()
        .unwrap()
        .wait()
        .unwrap();
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

    let mut remote = program().with_sequential_video(Rational::new(10, 1));
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
fn writes_an_export_through_the_program() {
    let directory = tempfile::tempdir().unwrap();
    let video = directory.path().join("video.mp4");
    let (width, height) = (32, 16);
    let mut codec = program();
    codec.check_encoder(Container::Mp4).unwrap();
    let encoder = codec
        .open_encoder(&EncoderSettings {
            output: video.clone(),
            width,
            height,
            frame_rate: Rational::new(10, 1),
            input: PixelFormat::Rgba8,
            container: Container::Mp4,
            preset: "ultrafast".into(),
            crf: 18,
            bit_depth: 8,
        })
        .unwrap();
    for frame in 0..10u8 {
        let pixels = [frame * 20, 0, 255 - frame * 20, 255].repeat((width * height) as usize);
        codec.encode_frame(encoder, pixels).unwrap();
    }
    codec.finish_encoder(encoder).unwrap();

    let pcm = directory.path().join("mix.pcm");
    let samples: Vec<u8> = (0..48_000)
        .flat_map(|frame| {
            let sample = (f64::from(frame) / 48_000.0 * 440.0 * std::f64::consts::TAU).sin() as f32;
            [sample * 0.5, sample * 0.5]
        })
        .flat_map(f32::to_le_bytes)
        .collect();
    std::fs::write(&pcm, samples).unwrap();
    let output = directory.path().join("film.mp4");
    codec
        .mux_audio(&MuxRequest {
            video,
            audio: pcm,
            sample_rate: 48_000,
            channels: 2,
            output: output.clone(),
            container: Container::Mp4,
            lead_in_frames: 0,
        })
        .unwrap();

    let mut reader = program();
    let probe = reader.probe(&output).unwrap().clone();
    let stream = probe.video.unwrap();
    assert_eq!((stream.width, stream.height), (width, height));
    assert_eq!(stream.codec.as_deref(), Some("h264"));
    assert_eq!(probe.audio[0].codec.as_deref(), Some("aac"));
    let last = reader.decode_frame(&output, 0.9).unwrap();
    let [red, _, blue, alpha] = last.pixels[..4] else {
        unreachable!()
    };
    assert!(red > 160 && blue < 90 && alpha == 255, "{red} {blue}");
    let decoded = reader.decode_audio(&output, 48_000, 2).unwrap();
    assert!((decoded.frame_count() as i64 - 48_000).abs() < 2_048);
}

#[test]
fn settings_libx264_cannot_write_are_refused() {
    let mut codec = program();
    let error = codec
        .open_encoder(&EncoderSettings {
            output: std::env::temp_dir().join("celesta-never-written.mp4"),
            width: 2,
            height: 2,
            frame_rate: Rational::new(1, 1),
            input: PixelFormat::Rgba8,
            container: Container::Mp4,
            preset: "medium".into(),
            crf: 99,
            bit_depth: 8,
        })
        .unwrap_err();
    assert!(error.to_string().contains("CRF"), "{error}");
    let error = codec
        .open_encoder(&EncoderSettings {
            output: std::env::temp_dir().join("celesta-never-written.mp4"),
            width: 2,
            height: 2,
            frame_rate: Rational::new(1, 1),
            input: PixelFormat::Rgba8,
            container: Container::Mp4,
            preset: "medium".into(),
            crf: 18,
            bit_depth: 12,
        })
        .unwrap_err();
    assert!(error.to_string().contains("bit depth"), "{error}");
    // The bit depth is H.264's alone: MOV ignores it.
    let directory = tempfile::tempdir().unwrap();
    let encoder = codec
        .open_encoder(&EncoderSettings {
            output: directory.path().join("alpha.mov"),
            width: 2,
            height: 2,
            frame_rate: Rational::new(1, 1),
            input: PixelFormat::Rgba16,
            container: Container::Mov,
            preset: "medium".into(),
            crf: 18,
            bit_depth: 12,
        })
        .unwrap();
    codec.abort_encoder(encoder);
}

#[test]
fn reads_and_writes_paths_relative_to_celestas_working_directory() {
    // Tests run in the package's folder; the program runs in its own.
    let directory = tempfile::tempdir_in(".").unwrap();
    let relative = Path::new(directory.path().file_name().unwrap());
    let video = relative.join("clip.mkv");
    generate(&video, "color=c=blue:s=4x4:r=5:d=1", "ffv1");
    let mut codec = program();
    assert_eq!(
        codec.probe(&video).unwrap().video.as_ref().unwrap().width,
        4
    );
    assert_eq!(codec.decode_frame(&video, 0.2).unwrap().width, 4);
    let missing = relative.join("missing.mkv");
    assert!(matches!(
        codec.probe(&missing),
        Err(MediaError::NotFound(path)) if path == missing
    ));
    let output = relative.join("out.mp4");
    let encoder = codec
        .open_encoder(&EncoderSettings {
            output: output.clone(),
            width: 4,
            height: 4,
            frame_rate: Rational::new(5, 1),
            input: PixelFormat::Rgba8,
            container: Container::Mp4,
            preset: "ultrafast".into(),
            crf: 18,
            bit_depth: 8,
        })
        .unwrap();
    codec.encode_frame(encoder, vec![255; 64]).unwrap();
    codec.finish_encoder(encoder).unwrap();
    assert!(directory.path().join("out.mp4").is_file());
}

#[test]
fn the_program_answers_the_decoder_it_names() {
    let directory = tempfile::tempdir().unwrap();
    let video = directory.path().join("clip.mkv");
    generate(&video, "color=c=red:s=4x2:r=5:d=1", "ffv1");
    let mut process = provider();
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
        provider().probe(DECODER_ID, &text),
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
        program().probe(&text),
        Err(MediaError::Unsupported { path, tried }) if path == text && tried == ["ffmpeg/ffmpeg"]
    ));
}

#[test]
fn manifest_flag_prints_the_media_manifest() {
    let output = std::process::Command::new(executable())
        .arg(celesta_plugin_host::MANIFEST_FLAG)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let printed = String::from_utf8(output.stdout).unwrap();
    assert_eq!(printed.lines().count(), 1, "{printed}");
    let manifest: serde_json::Value = serde_json::from_str(&printed).unwrap();
    assert_eq!(manifest["id"], "ffmpeg");
    assert_eq!(manifest["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(manifest["roles"], serde_json::json!(["media"]));
    assert_eq!(manifest["media"]["decoders"][0]["id"], DECODER_ID);
    assert_eq!(manifest["media"]["decoders"][0]["name"], "FFmpeg");
    let extensions = manifest["media"]["decoders"][0]["extensions"]
        .as_array()
        .unwrap();
    assert!(extensions.contains(&"mp4".into()), "{extensions:?}");
}
