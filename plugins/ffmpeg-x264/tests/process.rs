//! The built `celesta-codec-ffmpeg-x264` program, driven through
//! `celesta_media::MediaProcess` as Celesta drives it beside the standard
//! `celesta-codec-ffmpeg` program: libx264's errors keep their kind, an
//! export encoded through it and muxed by the standard program reads back,
//! frames reach it through the ring, and `--manifest` gives its manifest.
//! The standard program's own process tests are in `plugins/ffmpeg`.

#[path = "../../ffmpeg/tests/support/encoding.rs"]
mod encoding;
#[path = "../../ffmpeg/tests/support/files.rs"]
mod files;
#[path = "../../ffmpeg/tests/support/program.rs"]
mod program;

use celesta_codec_ffmpeg::registered;
use celesta_composition::Rational;
use celesta_media::{
    Answered, AudioDecoder, AudioEncoderRequest, Color, ComponentKind, Encoded, FrameTransport,
    MediaBackend, MediaError, MediaProcess, MediaProvider, MediaRegistry, MuxerRequest, Packet,
    PixelFormat, ProviderUse, RegisteredPlugin, SettingValue, Settings, Track, TrackConfig,
    TrackPacket, VideoEncoderOpened, VideoEncoderRequest, VideoFrameDecoder, ring_layout,
};
use encoding::{settings, text};
use files::{generate, stream_color};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn executable() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_celesta-codec-ffmpeg-x264"))
}

/// The standard program, built by its own package against the standard
/// build, started to export.
fn base_program() -> Box<dyn MediaProvider> {
    program::factory("ffmpeg")(&Default::default(), ProviderUse::Export { ring: None }).unwrap()
}

// Build/test the LGPL package first; its executable survives the GPL library build.

/// The program, started as Celesta starts a media plugin for `purpose`.
fn started(purpose: ProviderUse) -> Box<dyn MediaProvider> {
    let executable = executable();
    let directory = executable.parent().unwrap().to_path_buf();
    MediaProcess::factory(executable, directory)(&Default::default(), purpose).unwrap()
}

/// The program started with `transport` for `purpose`.
fn program_with(transport: FrameTransport, purpose: ProviderUse) -> MediaProcess {
    let executable = executable();
    let directory = executable.parent().unwrap().to_path_buf();
    MediaProcess::start_with(
        &executable,
        &directory,
        &Default::default(),
        transport,
        purpose,
    )
    .unwrap()
}

/// Opens libx264 on `program` for `width`x`height` RGBA8 frames at 10 fps.
fn open_x264(
    program: &mut dyn MediaProvider,
    given: &Settings,
    (width, height): (u32, u32),
) -> Result<VideoEncoderOpened, MediaError> {
    let color = Color::bt709_limited();
    program
        .video_encoder()
        .expect("the program has video encoders")
        .open_video(&VideoEncoderRequest {
            component: "libx264",
            settings: given,
            width,
            height,
            frame_rate: Rational::new(10, 1),
            timescale: 10,
            offered: &[PixelFormat::Rgba8],
            color: &color,
        })
}

/// The packets of `answers`, and the config the first of them gave.
fn gathered(answers: impl IntoIterator<Item = Encoded>) -> (Vec<Packet>, TrackConfig) {
    let mut packets = Vec::new();
    let mut config = None;
    for answer in answers {
        packets.extend(answer.packets);
        config = config.or(answer.config);
    }
    (packets, config.expect("the encoder gave its config"))
}

/// `frames` RGBA8 frames of `size` from red-less blue to red, encoded
/// with libx264 `ultrafast` on `program`: the video track and its packets.
fn encode_video(
    program: &mut dyn MediaProvider,
    size: (u32, u32),
    frames: u8,
) -> (Track, Vec<Packet>) {
    let opened = open_x264(program, &settings(&[("preset", text("ultrafast"))]), size).unwrap();
    assert_eq!(opened.input, PixelFormat::Rgba8);
    let encoder = program.video_encoder().unwrap();
    let mut answers: Vec<Encoded> = (0..frames)
        .map(|frame| {
            let pixels = [frame * 20, 0, 255 - frame * 20, 255].repeat((size.0 * size.1) as usize);
            encoder
                .encode_video(opened.session, i64::from(frame), pixels)
                .unwrap()
        })
        .collect();
    answers.push(encoder.finish_video(opened.session).unwrap());
    let (packets, config) = gathered(answers);
    let track = Track::Video {
        config,
        timescale: 10,
        width: size.0,
        height: size.1,
        frame_rate: Rational::new(10, 1),
        color: Color::bt709_limited(),
        alpha: false,
    };
    (track, packets)
}

/// Writes `tracks` and their packets, each track's in turn, to `output`
/// with the standard FFmpeg process's MP4 muxer.
fn mux_mp4(output: &Path, tracks: Vec<(Track, Vec<Packet>)>) {
    let (tracks, packets): (Vec<Track>, Vec<Vec<Packet>>) = tracks.into_iter().unzip();
    let mut program = base_program();
    let muxer = program.muxer().expect("the program has muxers");
    let session = muxer
        .open_muxer(&MuxerRequest {
            component: "mp4",
            settings: &Settings::new(),
            output,
            tracks: &tracks,
            leading_audio: 0,
        })
        .unwrap();
    let packets: Vec<TrackPacket> = packets
        .into_iter()
        .enumerate()
        .flat_map(|(track, packets)| {
            packets
                .into_iter()
                .map(move |packet| TrackPacket { track, packet })
        })
        .collect();
    muxer.write_packets(session, &packets).unwrap();
    let written = muxer.finish_muxer(session).unwrap();
    assert_eq!(written.len(), 1, "{written:?}");
}

/// A backend on the standard program, which reads what the tests write.
fn program_backend() -> MediaBackend {
    MediaBackend::with_registry(Arc::new(MediaRegistry::new(vec![RegisteredPlugin {
        factory: program::factory("ffmpeg"),
        ..registered()
    }])))
}

#[test]
fn writes_an_export_through_the_program() {
    let directory = tempfile::tempdir().unwrap();
    let (width, height) = (32, 16);
    let mut program = started(ProviderUse::Export { ring: None });
    let described = program.describe().unwrap();
    let libx264 = described
        .iter()
        .find(|component| {
            component.kind == ComponentKind::VideoEncoder && component.id == "libx264"
        })
        .unwrap();
    assert!(libx264.available, "{:?}", libx264.reason);

    let video = encode_video(program.as_mut(), (width, height), 10);

    let mut program = base_program();
    let encoder = program
        .audio_encoder()
        .expect("the program has audio encoders");
    let opened = encoder
        .open_audio(&AudioEncoderRequest {
            component: "aac",
            settings: &Settings::new(),
            sample_rate: 48_000,
            channels: 2,
        })
        .unwrap();
    assert_eq!(
        (opened.sample_rate, opened.channels, opened.delay),
        (48_000, 2, 1024)
    );
    let samples: Vec<f32> = (0..48_000)
        .flat_map(|frame| {
            let sample = (f64::from(frame) / 48_000.0 * 440.0 * std::f64::consts::TAU).sin() as f32;
            [sample * 0.5, sample * 0.5]
        })
        .collect();
    let mut answers: Vec<Encoded> = samples
        .chunks(1024 * 16 * 2)
        .enumerate()
        .map(|(chunk, samples)| {
            encoder
                .encode_audio(opened.session, chunk as i64 * 1024 * 16, samples)
                .unwrap()
        })
        .collect();
    answers.push(encoder.finish_audio(opened.session).unwrap());
    let (packets, config) = gathered(answers);
    assert_eq!(config.codec, "mp4a.40.2");
    // The first packet holds the priming samples, before 0.
    assert_eq!(packets[0].pts, -1024);
    let audio = Track::Audio {
        config,
        timescale: 48_000,
        sample_rate: 48_000,
        channels: 2,
        delay: opened.delay,
    };

    let output = directory.path().join("film.mp4");
    mux_mp4(&output, vec![video, (audio, packets)]);
    drop(program);

    assert_eq!(stream_color(&output), (1, 1, 1, 1));
    let mut reader = program_backend();
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

/// The key and message of an [`MediaError::InvalidSettings`].
fn refused<T: std::fmt::Debug>(result: Result<T, MediaError>) -> String {
    match result {
        Err(MediaError::InvalidSettings { key, .. }) => key,
        other => panic!("expected invalid settings, got {other:?}"),
    }
}

#[test]
fn settings_libx264_cannot_write_are_refused() {
    let mut program = started(ProviderUse::Export { ring: None });
    let program = program.as_mut();
    let crf = settings(&[("crf", SettingValue::Number(99.0))]);
    assert_eq!(refused(open_x264(program, &crf, (2, 2))), "crf");
    let depth = settings(&[("bitDepth", text("12"))]);
    assert_eq!(refused(open_x264(program, &depth, (2, 2))), "bitDepth");
    // A CRF is not taken at an average bitrate, where its field is hidden.
    let hidden = settings(&[
        ("rateControl", text("vbr")),
        ("crf", SettingValue::Number(18.0)),
    ]);
    assert_eq!(refused(open_x264(program, &hidden, (2, 2))), "crf");
    // ProRes has no bit depth (the standard plugin's test).
}

#[test]
fn reads_and_writes_paths_relative_to_celestas_working_directory() {
    // Tests run in the package's folder; the program runs in its own.
    let directory = tempfile::tempdir_in(".").unwrap();
    let relative = Path::new(directory.path().file_name().unwrap());
    let video = relative.join("clip.mkv");
    generate(&video, "color=c=blue:s=4x4:r=5:d=1", "ffv1");
    let mut codec = program_backend();
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
    let mut program = started(ProviderUse::Export { ring: None });
    let encoded = encode_video(program.as_mut(), (4, 4), 1);
    mux_mp4(&relative.join("out.mp4"), vec![encoded]);
    assert!(directory.path().join("out.mp4").is_file());
}

#[test]
fn manifest_and_describe_expose_only_libx264() {
    let output = std::process::Command::new(executable())
        .arg(celesta_plugin_host::MANIFEST_FLAG)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let manifest: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(manifest["id"], "ffmpeg-x264");
    assert_eq!(manifest["media"]["videoEncoders"][0]["id"], "libx264");
    let mut process = started(ProviderUse::Export { ring: None });
    let components = process.describe().unwrap();
    assert_eq!(components.len(), 1);
    assert_eq!(components[0].id, "libx264");
    assert!(process.decoder().is_some()); // MediaProcess exposes protocol interfaces; provider rejects requests.
    // A standard component is refused by name, naming this program rather
    // than the standard one.
    let refused = process
        .video_encoder()
        .expect("the program has video encoders")
        .open_video(&VideoEncoderRequest {
            component: "vp9",
            settings: &Settings::new(),
            width: 2,
            height: 2,
            frame_rate: Rational::new(10, 1),
            timescale: 10,
            offered: &[PixelFormat::Rgba8],
            color: &Color::bt709_limited(),
        });
    assert!(
        matches!(&refused, Err(MediaError::Codec(message))
            if message == "no video encoder \"vp9\" in ffmpeg-x264"),
        "{refused:?}"
    );
}

/// Encodes `frames` (ten of them at 10 fps) through `program`'s pipeline,
/// receiving before each send so that fewer than `depth` requests are in
/// flight, and muxes them to `output`. The frames go through the ring
/// when `in_ring`, else in the payloads.
fn export_pipelined(
    program: &mut MediaProcess,
    output: &Path,
    size: (u32, u32),
    frames: u8,
    in_ring: bool,
) {
    let opened = open_x264(program, &settings(&[("preset", text("ultrafast"))]), size).unwrap();
    let mut answers: Vec<Encoded> = Vec::new();
    let pipeline = program.pipeline().expect("a pipeline");
    assert_eq!(pipeline.frames_in_ring(), in_ring);
    assert_eq!(pipeline.depth(), 3);
    for frame in 0..frames {
        while pipeline.in_flight() >= pipeline.depth() {
            match pipeline.receive().unwrap() {
                Answered::VideoFrame(encoded) => answers.push(encoded),
                other => panic!("expected a video frame answer, got {other:?}"),
            }
        }
        let pixels =
            [frame * 20, frame * 3, 255 - frame * 20, 255].repeat((size.0 * size.1) as usize);
        pipeline
            .send_video_frame(opened.session, i64::from(frame), pixels)
            .unwrap();
    }
    while pipeline.in_flight() > 0 {
        match pipeline.receive().unwrap() {
            Answered::VideoFrame(encoded) => answers.push(encoded),
            other => panic!("expected a video frame answer, got {other:?}"),
        }
    }
    answers.push(
        program
            .video_encoder()
            .unwrap()
            .finish_video(opened.session)
            .unwrap(),
    );
    let (packets, config) = gathered(answers);
    let track = Track::Video {
        config,
        timescale: 10,
        width: size.0,
        height: size.1,
        frame_rate: Rational::new(10, 1),
        color: Color::bt709_limited(),
        alpha: false,
    };
    mux_mp4(output, vec![(track, packets)]);
}

#[test]
fn frames_reach_the_encoder_through_the_ring() {
    let directory = tempfile::tempdir().unwrap();
    let size = (32, 16);
    let export = ProviderUse::Export {
        ring: ring_layout(&[PixelFormat::Rgba8], size.0, size.1),
    };

    let mut shared = program_with(FrameTransport::SharedMemory, export);
    if cfg!(any(unix, windows)) {
        assert!(shared.shares_frames(), "the program declined the ring");
    }
    let through_ring = directory.path().join("ring.mp4");
    export_pipelined(
        &mut shared,
        &through_ring,
        size,
        10,
        cfg!(any(unix, windows)),
    );
    drop(shared);

    let mut piped = program_with(FrameTransport::Pipe, export);
    assert!(!piped.shares_frames());
    let through_pipe = directory.path().join("pipe.mp4");
    export_pipelined(&mut piped, &through_pipe, size, 10, false);
    drop(piped);

    assert_eq!(
        std::fs::read(&through_ring).unwrap(),
        std::fs::read(&through_pipe).unwrap()
    );
    let mut reader = program_backend().with_sequential_video(Rational::new(10, 1));
    for frame in 0..10 {
        let decoded = reader
            .decode_frame(&through_ring, f64::from(frame) / 10.0)
            .unwrap();
        assert_eq!((decoded.width, decoded.height), size, "{frame}");
    }
}
