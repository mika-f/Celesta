//! The `celesta-codec-test` program serves the test codec over the codec
//! protocol, answering as [`TestCodec`] does in this process.

use celesta_codec_test::{
    DECODER_ID, StoredAudio, StoredVideo, TestCodec, TestMedia, read, write_rgba_video,
};
use celesta_composition::Rational;
use celesta_media::{
    Container, EncoderSettings, FrameTransport, MediaError, MediaProcess, MediaProvider,
    MuxRequest, PixelFormat, Sequential, VideoRequest,
};
use std::path::{Path, PathBuf};

fn program() -> Box<dyn MediaProvider> {
    let executable = PathBuf::from(env!("CARGO_BIN_EXE_celesta-codec-test"));
    let folder = executable.parent().unwrap().to_path_buf();
    MediaProcess::factory(executable, folder)(&Default::default()).unwrap()
}

/// A codec program that never answers `hello`, as one that hangs while it
/// loads its libraries; it touches `started` first.
#[cfg(unix)]
fn hanging_program(directory: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let program = directory.join("codec.sh");
    std::fs::write(
        &program,
        format!(
            "#!/bin/sh\ntouch '{}'\nexec sleep 60\n",
            directory.join("started").display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    program
}

#[cfg(unix)]
#[test]
fn killing_the_switch_ends_a_codec_start_waiting_on_hello() {
    use celesta_plugin_host::PluginKillSwitch;
    use std::time::{Duration, Instant};

    let directory = tempfile::tempdir().unwrap();
    let program = hanging_program(directory.path());
    let kill = PluginKillSwitch::new();
    let killer = {
        let kill = kill.clone();
        let started = directory.path().join("started");
        std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(30);
            while !started.exists() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            std::thread::sleep(Duration::from_millis(200));
            kill.kill();
        })
    };
    let begun = Instant::now();
    let result = MediaProcess::start(&program, directory.path(), &kill);
    killer.join().unwrap();
    assert!(
        matches!(result, Err(MediaError::CodecStopped(_))),
        "{:?}",
        result.err()
    );
    assert!(
        begun.elapsed() < Duration::from_secs(20),
        "it returned before hello's 30 s deadline"
    );
}

#[cfg(unix)]
#[test]
fn a_killed_switch_starts_no_codec_process() {
    let directory = tempfile::tempdir().unwrap();
    let program = hanging_program(directory.path());
    let kill = celesta_plugin_host::PluginKillSwitch::new();
    kill.kill();
    assert!(matches!(
        MediaProcess::start(&program, directory.path(), &kill),
        Err(MediaError::CodecStopped(_))
    ));
    assert!(!directory.path().join("started").exists(), "nothing ran");
}

/// The program started with `transport`.
fn program_with(transport: FrameTransport) -> MediaProcess {
    let executable = PathBuf::from(env!("CARGO_BIN_EXE_celesta-codec-test"));
    let folder = executable.parent().unwrap().to_path_buf();
    MediaProcess::start_with(&executable, &folder, &Default::default(), transport).unwrap()
}

#[test]
fn frames_through_shared_memory_are_the_frames_through_the_pipe() {
    let directory = tempfile::tempdir().unwrap();
    let video = directory.path().join("video.ctc");
    // 64x36, 24 frames whose every byte differs from frame to frame.
    let (width, height) = (64u32, 36u32);
    let frames: Vec<Vec<u8>> = (0..24u32)
        .map(|frame| {
            (0..width * height * 4)
                .map(|index| (index.wrapping_mul(31) ^ frame.wrapping_mul(97)) as u8)
                .collect()
        })
        .collect();
    write_rgba_video(&video, width, height, Rational::new(24, 1), &frames).unwrap();

    let mut shared = program_with(FrameTransport::SharedMemory);
    let mut piped = program_with(FrameTransport::Pipe);
    let mut local = TestCodec::new();
    if cfg!(any(unix, windows)) {
        assert!(shared.shares_frames(), "the program declined shared memory");
    }
    assert!(!piped.shares_frames());
    // Exact requests out of order, then a sequential session forward,
    // repeating a frame and holding past the end.
    let mut times: Vec<(f64, Option<&str>)> = [0.5, 0.0, 0.75, 0.25, 0.25]
        .into_iter()
        .map(|time| (time, None))
        .collect();
    times.extend(
        [0.0, 1.0 / 24.0, 2.0 / 24.0, 2.0 / 24.0, 0.5, 2.0]
            .into_iter()
            .map(|time| (time, Some("layer"))),
    );
    for (time, session) in times {
        let request = VideoRequest {
            path: &video,
            time,
            sequential: session.map(|session| Sequential {
                session,
                frame_rate: Rational::new(24, 1),
            }),
        };
        let expected = local.decode_video(DECODER_ID, &request).unwrap();
        assert_eq!(
            shared.decode_video(DECODER_ID, &request).unwrap(),
            expected,
            "shared, at {time}"
        );
        assert_eq!(
            piped.decode_video(DECODER_ID, &request).unwrap(),
            expected,
            "piped, at {time}"
        );
    }
}

#[test]
fn the_program_answers_as_the_codec_in_process_does() {
    let directory = tempfile::tempdir().unwrap();
    let mut process = program();
    let mut local = TestCodec::new();

    let video = directory.path().join("video.ctc");
    let frames = vec![
        vec![1, 2, 3, 4, 5, 6, 7, 8],
        vec![9, 10, 11, 12, 13, 14, 15, 16],
    ];
    write_rgba_video(&video, 2, 1, Rational::new(2, 1), &frames).unwrap();
    assert_eq!(
        process.probe(DECODER_ID, &video).unwrap(),
        local.probe(DECODER_ID, &video).unwrap()
    );
    for time in [0.0, 0.5] {
        let request = VideoRequest {
            path: &video,
            time,
            sequential: None,
        };
        assert_eq!(
            process.decode_video(DECODER_ID, &request).unwrap(),
            local.decode_video(DECODER_ID, &request).unwrap(),
            "at {time}"
        );
    }
    let voice = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/assets/voices/001.wav");
    assert_eq!(
        process.decode_audio(DECODER_ID, &voice, 44_100, 1).unwrap(),
        local.decode_audio(DECODER_ID, &voice, 44_100, 1).unwrap()
    );

    let encoded = directory.path().join("encoded.ctc");
    let encoder = process
        .open_encoder(&EncoderSettings {
            output: encoded.clone(),
            width: 2,
            height: 1,
            frame_rate: Rational::new(30, 1),
            input: PixelFormat::Rgba8,
            container: Container::Mp4,
            preset: "medium".into(),
            crf: 18,
            bit_depth: 8,
        })
        .unwrap();
    for frame in &frames {
        process.encode_frame(encoder, frame.clone()).unwrap();
    }
    process.finish_encoder(encoder).unwrap();
    let pcm = directory.path().join("audio.pcm");
    let samples = [0.25f32, -0.5, 0.75, -1.0];
    std::fs::write(
        &pcm,
        samples
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let output = directory.path().join("muxed.ctc");
    process
        .mux_audio(&MuxRequest {
            video: encoded,
            audio: pcm,
            sample_rate: 48_000,
            channels: 2,
            output: output.clone(),
            container: Container::Mp4,
            lead_in_frames: 1,
        })
        .unwrap();
    assert_eq!(
        read(&output).unwrap(),
        TestMedia {
            container: Some(Container::Mp4),
            video: Some(StoredVideo {
                width: 2,
                height: 1,
                frame_rate: Rational::new(30, 1),
                format: PixelFormat::Rgba8,
                frames,
            }),
            audio: Some(StoredAudio {
                sample_rate: 48_000,
                channels: 2,
                lead_in_frames: 1,
                samples: samples.to_vec(),
            }),
        }
    );

    let missing = directory.path().join("missing.mp4");
    assert!(matches!(
        process.probe(DECODER_ID, &missing),
        Err(MediaError::NotFound(path)) if path == missing
    ));
}

#[test]
fn a_foreign_file_is_unsupported() {
    let directory = tempfile::tempdir().unwrap();
    // A PNG's first bytes: neither a test codec file nor a WAV file.
    let png = directory.path().join("image.png");
    std::fs::write(
        &png,
        [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0],
    )
    .unwrap();

    // The wire code "unsupported" comes back as `Unsupported` naming the
    // file, as the codec in this process answers.
    let mut process = program();
    assert!(matches!(
        process.probe(DECODER_ID, &png),
        Err(MediaError::Unsupported { path, .. }) if path == png
    ));
    assert!(matches!(
        TestCodec::new().probe(DECODER_ID, &png),
        Err(MediaError::Unsupported { path, .. }) if path == png
    ));
}

#[test]
fn another_decoder_is_not_the_test_codec() {
    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("any.ctc");
    let message = match program().probe("ffmpeg", &missing) {
        Err(MediaError::Codec(message)) => message,
        other => panic!("expected a codec error, got {other:?}"),
    };
    assert!(message.contains("no decoder"), "{message}");
}
