//! The `celesta-codec-test` program serves the test codec over the codec
//! protocol, answering as [`TestCodec`] does in this process.

use celesta_codec_test::{
    AUDIO_ENCODER_ID, DECODER_ID, MUXER_ID, StoredAudio, StoredVideo, TestCodec, TestMedia,
    VIDEO_ENCODER_ID, read, write_rgba_video,
};
use celesta_composition::Rational;
use celesta_media::{
    Answered, AudioEncoderRequest, Color, ComponentKind, Decoder, FrameTransport, MediaError,
    MediaProcess, MediaProvider, MuxerRequest, Packet, PixelFormat, ProviderUse, Sequential,
    SessionId, SettingValue, Settings, Track, TrackConfig, TrackPacket, VideoEncoderRequest,
    VideoRequest, ring_layout,
};
use std::path::{Path, PathBuf};

fn program() -> Box<dyn MediaProvider> {
    let executable = PathBuf::from(env!("CARGO_BIN_EXE_celesta-codec-test"));
    let folder = executable.parent().unwrap().to_path_buf();
    MediaProcess::factory(executable, folder)(
        &Default::default(),
        ProviderUse::Export { ring: None },
    )
    .unwrap()
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

/// The program started with `transport` for `purpose`.
fn program_with(transport: FrameTransport, purpose: ProviderUse) -> MediaProcess {
    let executable = PathBuf::from(env!("CARGO_BIN_EXE_celesta-codec-test"));
    let folder = executable.parent().unwrap().to_path_buf();
    MediaProcess::start_with(
        &executable,
        &folder,
        &Default::default(),
        transport,
        purpose,
    )
    .unwrap()
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

    let mut shared = program_with(FrameTransport::SharedMemory, ProviderUse::Decode);
    let mut piped = program_with(FrameTransport::Pipe, ProviderUse::Decode);
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
fn the_program_answers_decoding_as_the_codec_in_process_does() {
    let mut provider = program();
    let process = provider.decoder().expect("a decoder");
    let mut local = TestCodec::new();
    let directory = tempfile::tempdir().unwrap();

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

    let missing = directory.path().join("missing.mp4");
    assert!(matches!(
        process.probe(DECODER_ID, &missing),
        Err(MediaError::NotFound(path)) if path == missing
    ));
}

#[test]
fn the_program_describes_and_exports() {
    let directory = tempfile::tempdir().unwrap();
    let mut provider = program();

    let described = provider.describe().unwrap();
    assert_eq!(described, TestCodec::new().describe().unwrap());
    let kinds: Vec<_> = described.iter().map(|component| component.kind).collect();
    assert_eq!(
        kinds,
        [
            ComponentKind::Decoder,
            ComponentKind::VideoEncoder,
            ComponentKind::AudioEncoder,
            ComponentKind::Muxer,
        ]
    );

    // Video: two frames in the first format offered.
    let frames = vec![
        vec![1, 2, 3, 4, 5, 6, 7, 8],
        vec![9, 10, 11, 12, 13, 14, 15, 16],
    ];
    let color = Color::bt709_limited();
    let encoder = provider.video_encoder().expect("a video encoder");
    let opened = encoder
        .open_video(&VideoEncoderRequest {
            component: VIDEO_ENCODER_ID,
            settings: &Settings::new(),
            width: 2,
            height: 1,
            frame_rate: Rational::new(30, 1),
            timescale: 30,
            offered: &[PixelFormat::Rgba8],
            color: &color,
        })
        .unwrap();
    assert_eq!(opened.input, PixelFormat::Rgba8);
    let mut video_packets = Vec::new();
    let mut video_config = None;
    for (index, frame) in frames.iter().enumerate() {
        let answer = encoder
            .encode_video(opened.session, index as i64, frame.clone())
            .unwrap();
        video_packets.extend(answer.packets);
        video_config = video_config.or(answer.config);
    }
    let finished = encoder.finish_video(opened.session).unwrap();
    video_packets.extend(finished.packets);
    video_config = video_config.or(finished.config);
    assert_eq!(video_packets.len(), 2);

    // Audio: 100 stereo frames, one of them the encoder's delay.
    let samples: Vec<f32> = (0..200).map(|index| index as f32 / 200.0 - 0.5).collect();
    let encoder = provider.audio_encoder().expect("an audio encoder");
    let delay = Settings::from([("delay".to_owned(), SettingValue::Number(1.0))]);
    let opened_audio = encoder
        .open_audio(&AudioEncoderRequest {
            component: AUDIO_ENCODER_ID,
            settings: &delay,
            sample_rate: 48_000,
            channels: 2,
        })
        .unwrap();
    assert_eq!(opened_audio.delay, 1);
    assert_ne!(opened_audio.session, opened.session);
    let answer = encoder
        .encode_audio(opened_audio.session, 0, &samples)
        .unwrap();
    let mut audio_packets = answer.packets;
    let audio_config = answer
        .config
        .expect("the config comes with the first packets");
    audio_packets.extend(encoder.finish_audio(opened_audio.session).unwrap().packets);
    assert_eq!(audio_packets.len(), 2, "the delay's silence and the chunk");

    // The muxer writes both, with the encoder's one frame of delay and one
    // frame of leading audio hidden.
    let output = directory.path().join("exported.ctst");
    let tracks = [
        Track::Video {
            config: video_config.expect("a video config"),
            timescale: 30,
            width: 2,
            height: 1,
            frame_rate: Rational::new(30, 1),
            color,
            alpha: false,
        },
        Track::Audio {
            config: audio_config,
            timescale: 48_000,
            sample_rate: 48_000,
            channels: 2,
            delay: 1,
        },
    ];
    let muxer = provider.muxer().expect("a muxer");
    let session = muxer
        .open_muxer(&MuxerRequest {
            component: MUXER_ID,
            settings: &Settings::new(),
            output: &output,
            tracks: &tracks,
            leading_audio: 1,
        })
        .unwrap();
    let packets: Vec<TrackPacket> = video_packets
        .into_iter()
        .map(|packet| TrackPacket { track: 0, packet })
        .chain(
            audio_packets
                .into_iter()
                .map(|packet| TrackPacket { track: 1, packet }),
        )
        .collect();
    muxer.write_packets(session, &packets).unwrap();
    assert_eq!(
        muxer.finish_muxer(session).unwrap(),
        std::slice::from_ref(&output)
    );

    let mut all = vec![0.0; 2];
    all.extend(&samples);
    assert_eq!(
        read(&output).unwrap(),
        TestMedia {
            extension: Some("ctst".to_owned()),
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
                lead_in_frames: 2,
                samples: all,
            }),
        }
    );

    // An aborted mux removes its file, and the program says there is none
    // left to remove.
    let aborted = directory.path().join("aborted.ctst");
    let session = muxer
        .open_muxer(&MuxerRequest {
            component: MUXER_ID,
            settings: &Settings::new(),
            output: &aborted,
            tracks: &tracks[..1],
            leading_audio: 0,
        })
        .unwrap();
    assert!(aborted.exists());
    assert!(muxer.abort_muxer(session).is_empty());
    assert!(!aborted.exists());
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
    let mut provider = program();
    let process = provider.decoder().unwrap();
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
    let message = match program().decoder().unwrap().probe("ffmpeg", &missing) {
        Err(MediaError::Codec(message)) => message,
        other => panic!("expected a codec error, got {other:?}"),
    };
    assert!(message.contains("no decoder"), "{message}");
}

/// 32x16 `rgba8` frames, whose every byte differs from frame to frame.
const RING_WIDTH: u32 = 32;
const RING_HEIGHT: u32 = 16;

fn rgba_frames(count: u32, width: u32, height: u32) -> Vec<Vec<u8>> {
    (0..count)
        .map(|frame| {
            (0..width * height * 4)
                .map(|index| (index.wrapping_mul(29) ^ frame.wrapping_mul(53)) as u8)
                .collect()
        })
        .collect()
}

/// An export with a ring for the 32x16 `rgba8` frames.
fn rgba_export() -> ProviderUse {
    ProviderUse::Export {
        ring: ring_layout(&[PixelFormat::Rgba8], RING_WIDTH, RING_HEIGHT),
    }
}

fn open_rgba(
    provider: &mut dyn MediaProvider,
    width: u32,
    height: u32,
    color: &Color,
) -> celesta_media::VideoEncoderOpened {
    let opened = provider
        .video_encoder()
        .expect("a video encoder")
        .open_video(&VideoEncoderRequest {
            component: VIDEO_ENCODER_ID,
            settings: &Settings::from([("input".to_owned(), SettingValue::String("rgba8".into()))]),
            width,
            height,
            frame_rate: Rational::new(30, 1),
            timescale: 30,
            offered: &[PixelFormat::Rgba8],
            color,
        })
        .unwrap();
    assert_eq!(opened.input, PixelFormat::Rgba8);
    opened
}

/// Encodes `frames` in `session`, frame `i` at `i * denominator`: through
/// the provider's pipeline, receiving before each send so that fewer than
/// `depth` requests are in flight, when it has one, else one at a time.
/// Drains, and gives the packets in order and the track's first config.
fn encode_pipelined(
    provider: &mut dyn MediaProvider,
    session: SessionId,
    frames: &[Vec<u8>],
    denominator: i64,
) -> (Vec<Packet>, Option<TrackConfig>) {
    let mut packets = Vec::new();
    let mut config = None;
    let mut take = |encoded: celesta_media::Encoded| {
        packets.extend(encoded.packets);
        config = config.take().or(encoded.config);
    };
    if let Some(pipeline) = provider.pipeline() {
        for (index, frame) in frames.iter().enumerate() {
            while pipeline.in_flight() >= pipeline.depth() {
                match pipeline.receive().unwrap() {
                    Answered::VideoFrame(encoded) => take(encoded),
                    other => panic!("expected a video frame answer, got {other:?}"),
                }
            }
            pipeline
                .send_video_frame(session, index as i64 * denominator, frame.clone())
                .unwrap();
        }
        while pipeline.in_flight() > 0 {
            match pipeline.receive().unwrap() {
                Answered::VideoFrame(encoded) => take(encoded),
                other => panic!("expected a video frame answer, got {other:?}"),
            }
        }
    } else {
        let encoder = provider.video_encoder().unwrap();
        for (index, frame) in frames.iter().enumerate() {
            let encoded = encoder
                .encode_video(session, index as i64 * denominator, frame.clone())
                .unwrap();
            take(encoded);
        }
    }
    (packets, config)
}

/// Exports `frames` through `provider` to a file in `directory`, and gives
/// the file's bytes.
fn export_frames(
    provider: &mut dyn MediaProvider,
    directory: &Path,
    name: &str,
    frames: &[Vec<u8>],
    (width, height): (u32, u32),
) -> Vec<u8> {
    let color = Color::bt709_limited();
    let opened = open_rgba(provider, width, height, &color);
    let (mut packets, mut config) = encode_pipelined(provider, opened.session, frames, 1);
    let finished = provider
        .video_encoder()
        .unwrap()
        .finish_video(opened.session)
        .unwrap();
    packets.extend(finished.packets);
    config = config.or(finished.config);
    assert_eq!(packets.len(), frames.len());

    let output = directory.join(name);
    let tracks = [Track::Video {
        config: config.expect("a video config"),
        timescale: 30,
        width,
        height,
        frame_rate: Rational::new(30, 1),
        color,
        alpha: false,
    }];
    let muxer = provider.muxer().expect("a muxer");
    let session = muxer
        .open_muxer(&MuxerRequest {
            component: MUXER_ID,
            settings: &Settings::new(),
            output: &output,
            tracks: &tracks,
            leading_audio: 0,
        })
        .unwrap();
    let packets: Vec<TrackPacket> = packets
        .into_iter()
        .map(|packet| TrackPacket { track: 0, packet })
        .collect();
    muxer.write_packets(session, &packets).unwrap();
    muxer.finish_muxer(session).unwrap();
    let stored = read(&output).unwrap();
    assert_eq!(stored.video.unwrap().frames, frames);
    std::fs::read(&output).unwrap()
}

/// Sends `count` frames to a fresh session without receiving, and checks
/// the window's depth; drains and finishes the session.
fn fill_window(provider: &mut dyn MediaProvider, depth: usize) {
    let color = Color::bt709_limited();
    let opened = open_rgba(provider, RING_WIDTH, RING_HEIGHT, &color);
    let frames = rgba_frames(3, RING_WIDTH, RING_HEIGHT);
    let pipeline = provider.pipeline().expect("a pipeline");
    assert_eq!(pipeline.depth(), depth);
    for frame in &frames {
        pipeline
            .send_video_frame(opened.session, 0, frame.clone())
            .unwrap();
    }
    assert_eq!(pipeline.in_flight(), 3);
    while pipeline.in_flight() > 0 {
        pipeline.receive().unwrap();
    }
    provider
        .video_encoder()
        .unwrap()
        .finish_video(opened.session)
        .unwrap();
}

#[test]
fn pipelined_frames_through_the_ring_match_the_pipe_byte_for_byte() {
    let directory = tempfile::tempdir().unwrap();
    let size = (RING_WIDTH, RING_HEIGHT);
    let frames = rgba_frames(7, size.0, size.1);
    let export = rgba_export();

    let mut shared = program_with(FrameTransport::SharedMemory, export);
    if cfg!(any(unix, windows)) {
        assert!(shared.shares_frames(), "the program declined the ring");
        let ring = shared.ring().expect("a ring");
        assert_eq!((ring.slots, ring.slot_bytes), (3, 2048));
    }
    fill_window(&mut shared, 3);
    let through_ring = export_frames(&mut shared, directory.path(), "ring.ctst", &frames, size);

    let mut piped = program_with(FrameTransport::Pipe, export);
    assert!(!piped.shares_frames());
    fill_window(&mut piped, 3);
    let through_pipe = export_frames(&mut piped, directory.path(), "pipe.ctst", &frames, size);

    let through_codec = export_frames(
        &mut TestCodec::new(),
        directory.path(),
        "local.ctst",
        &frames,
        size,
    );
    assert_eq!(through_ring, through_pipe);
    assert_eq!(through_ring, through_codec);
}

#[test]
fn a_describing_or_ringless_export_process_gets_no_segment() {
    for purpose in [ProviderUse::Describe, ProviderUse::Export { ring: None }] {
        let mut process = program_with(FrameTransport::SharedMemory, purpose);
        assert!(!process.shares_frames(), "{purpose:?}");
        assert_eq!(process.ring(), None);
        assert_eq!(
            process.describe().unwrap(),
            TestCodec::new().describe().unwrap()
        );
    }
}

#[test]
fn frames_too_large_for_the_ring_go_through_the_pipe() {
    let directory = tempfile::tempdir().unwrap();
    let mut shared = program_with(FrameTransport::SharedMemory, rgba_export());
    // 8192-byte frames do not fit the 2048-byte slots.
    let frames = rgba_frames(4, 64, 32);
    export_frames(
        &mut shared,
        directory.path(),
        "large.ctst",
        &frames,
        (64, 32),
    );
}

#[test]
fn an_error_in_the_window_leaves_the_rest_in_flight() {
    let mut process = program_with(FrameTransport::SharedMemory, rgba_export());
    let color = Color::bt709_limited();
    let opened = open_rgba(&mut process, RING_WIDTH, RING_HEIGHT, &color);
    let mut frames = rgba_frames(3, RING_WIDTH, RING_HEIGHT);
    frames[1].truncate(2044);
    let pipeline = process.pipeline().unwrap();
    for (index, frame) in frames.into_iter().enumerate() {
        pipeline
            .send_video_frame(opened.session, index as i64, frame)
            .unwrap();
    }
    assert_eq!(pipeline.in_flight(), 3);
    assert!(matches!(pipeline.receive(), Ok(Answered::VideoFrame(_))));
    match pipeline.receive() {
        Err(MediaError::Codec(message)) => assert!(message.contains("not 2044"), "{message}"),
        other => panic!("expected a codec error, got {other:?}"),
    }
    assert!(matches!(pipeline.receive(), Ok(Answered::VideoFrame(_))));
    assert_eq!(pipeline.in_flight(), 0);
    // The process still answers.
    assert_eq!(
        process.describe().unwrap(),
        TestCodec::new().describe().unwrap()
    );
}

/// A program that runs the real one, having written its process id to
/// `pid` first (`exec` keeps it), so that a test can stop it after `hello`.
#[cfg(unix)]
fn stoppable_program(directory: &Path) -> (PathBuf, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let program = directory.join("codec.sh");
    let pid = directory.join("pid");
    std::fs::write(
        &program,
        format!(
            "#!/bin/sh\necho $$ > '{}'\nexec '{}' \"$@\"\n",
            pid.display(),
            env!("CARGO_BIN_EXE_celesta-codec-test")
        ),
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    (program, pid)
}

/// Starts the stoppable program for the export `purpose`, opens its
/// encoder, stops the process (it answers nothing from here on), and sends
/// `frames` frames; a thread kills the switch 200 ms later.
#[cfg(unix)]
fn stopped_export(
    directory: &Path,
    purpose: ProviderUse,
    frames: usize,
) -> (MediaProcess, std::thread::JoinHandle<()>) {
    let (program, pid) = stoppable_program(directory);
    let kill = celesta_plugin_host::PluginKillSwitch::new();
    let mut process = MediaProcess::start_with(
        &program,
        directory,
        &kill,
        FrameTransport::SharedMemory,
        purpose,
    )
    .unwrap();
    let color = Color::bt709_limited();
    let opened = open_rgba(&mut process, RING_WIDTH, RING_HEIGHT, &color);
    let pid = std::fs::read_to_string(pid).unwrap();
    let stopped = std::process::Command::new("kill")
        .args(["-STOP", pid.trim()])
        .status()
        .unwrap();
    assert!(stopped.success());
    let pipeline = process.pipeline().unwrap();
    for (index, frame) in rgba_frames(frames as u32, RING_WIDTH, RING_HEIGHT)
        .into_iter()
        .enumerate()
    {
        pipeline
            .send_video_frame(opened.session, index as i64, frame)
            .unwrap();
    }
    assert_eq!(pipeline.in_flight(), frames);
    let killer = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(200));
        kill.kill();
    });
    (process, killer)
}

#[cfg(unix)]
#[test]
fn a_killed_switch_fails_the_window() {
    use std::time::{Duration, Instant};

    let directory = tempfile::tempdir().unwrap();
    let (mut process, killer) =
        stopped_export(directory.path(), ProviderUse::Export { ring: None }, 2);
    let begun = Instant::now();
    let pipeline = process.pipeline().unwrap();
    let first = pipeline.receive();
    let second = pipeline.receive();
    killer.join().unwrap();
    assert!(
        matches!(first, Err(MediaError::CodecStopped(_))),
        "{first:?}"
    );
    assert!(
        matches!(second, Err(MediaError::CodecStopped(_))),
        "{second:?}"
    );
    assert!(begun.elapsed() < Duration::from_secs(10));
}

#[cfg(unix)]
#[test]
fn a_stopped_program_frees_its_ring() {
    let directory = tempfile::tempdir().unwrap();
    let (mut process, killer) = stopped_export(directory.path(), rgba_export(), 3);
    assert!(process.shares_frames(), "the program declined the ring");
    let pipeline = process.pipeline().unwrap();
    assert_eq!(pipeline.depth(), 3);
    for _ in 0..3 {
        assert!(matches!(
            pipeline.receive(),
            Err(MediaError::CodecStopped(_))
        ));
    }
    killer.join().unwrap();
    // Nothing is left in the window, and nothing stale is received.
    assert_eq!(pipeline.in_flight(), 0);
    assert!(matches!(pipeline.receive(), Err(MediaError::Codec(_))));
    // A stopped program takes no more frames, and none stays in flight.
    let frame = rgba_frames(1, RING_WIDTH, RING_HEIGHT).remove(0);
    for _ in 0..4 {
        let sent = pipeline.send_video_frame(SessionId(1), 0, frame.clone());
        assert!(matches!(sent, Err(MediaError::CodecStopped(_))), "{sent:?}");
        assert_eq!(pipeline.in_flight(), 0);
    }
}
