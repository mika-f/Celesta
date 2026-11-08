//! The built `celesta-codec-ffmpeg` program, driven through
//! `celesta_media::MediaProcess` as Celesta drives it: its answers equal the
//! in-process codec's, its errors keep their kind, an export written through
//! it reads back, and `--manifest` gives its manifest.

use celesta_codec_ffmpeg::{DECODER_ID, FfmpegCodec, registered};
use celesta_composition::Rational;
use celesta_media::{
    AudioDecoder, Decoder, MediaBackend, MediaError, MediaProcess, MediaProvider, MediaRegistry,
    ProviderFactory, ProviderUse, RegisteredPlugin, VideoFrameDecoder,
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
    MediaProcess::factory(executable, directory)(&Default::default(), ProviderUse::Decode).unwrap()
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
#[ignore = "ported in Task 13"]
fn writes_an_export_through_the_program() {
    // Writes an MP4 through the program: its encoders, then its muxer.
    unimplemented!("ported in Task 13");
}

#[test]
#[ignore = "ported in Task 13"]
fn settings_libx264_cannot_write_are_refused() {
    // The program refuses settings libx264 cannot write.
    unimplemented!("ported in Task 13");
}

#[test]
#[ignore = "ported in Task 13"]
fn reads_and_writes_paths_relative_to_celestas_working_directory() {
    // Paths relative to Celesta's working directory, read and written.
    unimplemented!("ported in Task 13");
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
