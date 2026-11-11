//! The built `celesta-codec-avfoundation` program, driven through
//! `celesta_media::MediaProcess` as Celesta drives it for an export: frames
//! reach VideoToolbox through the ring of shared memory, pipelined, and
//! come out as the encoder in this process gives them; errors keep their
//! kind across the protocol.
#![cfg(target_os = "macos")]

use celesta_codec_avfoundation::AvfCodec;
use celesta_codec_ffmpeg::FfmpegCodec;
use celesta_composition::Rational;
use celesta_media::{
    Answered, AudioEncoderRequest, Color, Encoded, FrameTransport, MediaError, MediaProcess,
    MediaProvider, MuxerRequest, Packet, PixelFormat, ProviderUse, SettingValue, Settings, Track,
    TrackConfig, TrackPacket, VideoEncoderOpened, VideoEncoderRequest, VideoRequest, ring_layout,
};
use std::path::{Path, PathBuf};

const WIDTH: u32 = 64;
const HEIGHT: u32 = 36;
/// 29.97 fps: a frame lasts 1001 of the 30000 ticks a second.
const RATE: Rational = Rational {
    numerator: 30_000,
    denominator: 1_001,
};
const TIMESCALE: u32 = 30_000;
const FRAME: i64 = 1_001;
/// The frames each test encodes.
const FRAMES: usize = 10;

/// The program started with `transport` for `purpose`, as Celesta starts a
/// media plugin.
fn program_with(transport: FrameTransport, purpose: ProviderUse) -> MediaProcess {
    let executable = PathBuf::from(env!("CARGO_BIN_EXE_celesta-codec-avfoundation"));
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

fn settings(pairs: &[(&str, SettingValue)]) -> Settings {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect()
}

fn text(value: &str) -> SettingValue {
    SettingValue::String(value.to_owned())
}

/// Opens `component` on `provider` at 64x36 and 29.97 fps, BT.709 limited,
/// offered `offered`.
fn open(
    provider: &mut dyn MediaProvider,
    component: &str,
    given: &Settings,
    offered: &[PixelFormat],
) -> Result<VideoEncoderOpened, MediaError> {
    provider
        .video_encoder()
        .expect("the plugin has video encoders")
        .open_video(&VideoEncoderRequest {
            component,
            settings: given,
            width: WIDTH,
            height: HEIGHT,
            frame_rate: RATE,
            timescale: TIMESCALE,
            offered,
            color: &Color::bt709_limited(),
        })
}

/// The luma of frame `index`: a grey a step lighter each frame, so a frame
/// out of order decodes to the wrong grey.
fn grey(index: usize) -> u8 {
    40 + 15 * index as u8
}

/// Frame `index` in NV12: a flat grey (neutral chroma).
fn nv12_grey(index: usize) -> Vec<u8> {
    let luma = (WIDTH * HEIGHT) as usize;
    let mut frame = vec![grey(index); luma];
    frame.extend(vec![128; luma / 2]);
    frame
}

/// Every packet of `answers`, and the config the first of them gave.
fn gathered(answers: Vec<Encoded>) -> (Vec<Packet>, TrackConfig) {
    let mut packets = Vec::new();
    let mut config = None;
    for answer in answers {
        packets.extend(answer.packets);
        config = config.or(answer.config);
    }
    (packets, config.expect("the encoder gave its config"))
}

/// Writes `packets` of a 64x36 video track of `config` to `output` with the
/// FFmpeg plugin's `mp4`.
fn mux(output: &Path, config: &TrackConfig, packets: &[Packet]) {
    let mut ffmpeg = FfmpegCodec::new();
    let muxer = ffmpeg.muxer().unwrap();
    let track = Track::Video {
        config: config.clone(),
        timescale: TIMESCALE,
        width: WIDTH,
        height: HEIGHT,
        frame_rate: RATE,
        color: Color::bt709_limited(),
        alpha: false,
    };
    let session = muxer
        .open_muxer(&MuxerRequest {
            component: "mp4",
            settings: &Settings::new(),
            output,
            tracks: &[track],
            leading_audio: 0,
        })
        .unwrap();
    let packets: Vec<TrackPacket> = packets
        .iter()
        .map(|packet| TrackPacket {
            track: 0,
            packet: packet.clone(),
        })
        .collect();
    muxer.write_packets(session, &packets).unwrap();
    muxer.finish_muxer(session).unwrap();
}

/// Every frame of the MP4 at `path`, as the FFmpeg plugin decodes it: RGBA
/// pixels, read at each frame's start (the decoder gives the frame at or
/// after the time asked).
fn decoded_frames(path: &Path) -> Vec<Vec<u8>> {
    let mut ffmpeg = FfmpegCodec::new();
    let decoder = ffmpeg.decoder().unwrap();
    (0..FRAMES)
        .map(|index| {
            let time = index as f64 * FRAME as f64 / f64::from(TIMESCALE);
            let frame = decoder
                .decode_video(
                    celesta_codec_ffmpeg::DECODER_ID,
                    &VideoRequest {
                        path,
                        time,
                        sequential: None,
                    },
                )
                .unwrap();
            assert_eq!((frame.width, frame.height), (WIDTH, HEIGHT));
            frame.pixels.to_vec()
        })
        .collect()
}

#[test]
fn encodes_through_the_ring() {
    let ring = ring_layout(&[PixelFormat::Nv12], WIDTH, HEIGHT);
    assert!(ring.is_some(), "a 64x36 frame fits a ring");
    let mut program = program_with(FrameTransport::SharedMemory, ProviderUse::Export { ring });
    assert!(program.shares_frames(), "the program declined the ring");

    let opened = open(&mut program, "h264", &Settings::new(), &[PixelFormat::Nv12]).unwrap();
    assert_eq!(opened.input, PixelFormat::Nv12);
    let pipeline = program.pipeline().expect("a pipeline");
    assert!(pipeline.frames_in_ring(), "h264 declared no ring");
    assert_eq!(pipeline.depth(), 3);
    let mut answers = Vec::new();
    let mut receive = |pipeline: &mut dyn celesta_media::Pipeline| match pipeline.receive().unwrap()
    {
        Answered::VideoFrame(encoded) => answers.push(encoded),
        other => panic!("expected a video frame answer, got {other:?}"),
    };
    for index in 0..FRAMES {
        while pipeline.in_flight() >= pipeline.depth() {
            receive(pipeline);
        }
        pipeline
            .send_video_frame(opened.session, index as i64 * FRAME, nv12_grey(index))
            .unwrap();
    }
    while pipeline.in_flight() > 0 {
        receive(pipeline);
    }
    answers.push(
        program
            .video_encoder()
            .unwrap()
            .finish_video(opened.session)
            .unwrap(),
    );
    drop(program);
    let (through_ring, config) = gathered(answers);

    // Every frame came out once, in decode order, each decoded before it
    // is shown.
    assert_eq!(through_ring.len(), FRAMES);
    let mut shown: Vec<i64> = through_ring.iter().map(|packet| packet.pts).collect();
    shown.sort_unstable();
    assert_eq!(
        shown,
        (0..FRAMES as i64)
            .map(|index| index * FRAME)
            .collect::<Vec<_>>()
    );
    for pair in through_ring.windows(2) {
        assert!(pair[0].dts < pair[1].dts, "{pair:?}");
    }
    assert!(through_ring.iter().all(|packet| packet.dts <= packet.pts));

    // The same frames encoded in this process.
    let mut codec = AvfCodec::default();
    let local = open(&mut codec, "h264", &Settings::new(), &[PixelFormat::Nv12]).unwrap();
    let encoder = codec.video_encoder().unwrap();
    let mut answers: Vec<Encoded> = (0..FRAMES)
        .map(|index| {
            encoder
                .encode_video(local.session, index as i64 * FRAME, nv12_grey(index))
                .unwrap()
        })
        .collect();
    answers.push(encoder.finish_video(local.session).unwrap());
    let (in_process, local_config) = gathered(answers);
    assert_eq!(config.codec, local_config.codec);

    let directory = tempfile::tempdir().unwrap();
    let (ring_file, local_file) = (
        directory.path().join("ring.mp4"),
        directory.path().join("local.mp4"),
    );
    mux(&ring_file, &config, &through_ring);
    mux(&local_file, &local_config, &in_process);
    let (ring_frames, local_frames) = (decoded_frames(&ring_file), decoded_frames(&local_file));
    for (index, (ring, local)) in ring_frames.iter().zip(&local_frames).enumerate() {
        for (ring, local) in ring.as_chunks::<4>().0.iter().zip(local.as_chunks::<4>().0) {
            assert!(
                ring.iter().zip(local).all(|(a, b)| a.abs_diff(*b) <= 2),
                "frame {index}: {ring:?} through the ring, {local:?} in process"
            );
        }
    }
    // Each frame decodes to its own grey, lighter than the one before: the
    // ring's slots were read in the order they were sent.
    let levels: Vec<u8> = ring_frames.iter().map(|frame| frame[0]).collect();
    for pair in levels.windows(2) {
        assert!(pair[0] + 5 < pair[1], "{levels:?}");
    }
}

/// The key of a [`MediaError::InvalidSettings`].
fn refused<T: std::fmt::Debug>(result: Result<T, MediaError>) -> String {
    match result {
        Err(MediaError::InvalidSettings { key, .. }) => key,
        other => panic!("expected invalid settings, got {other:?}"),
    }
}

#[test]
fn errors_keep_their_kind() {
    let mut program = program_with(
        FrameTransport::from_environment(),
        ProviderUse::Export { ring: None },
    );
    let as_text = settings(&[("rateControl", text("bitrate")), ("bitrate", text("fast"))]);
    assert_eq!(
        refused(open(&mut program, "h264", &as_text, &[PixelFormat::Nv12])),
        "bitrate"
    );
    let aac = program
        .audio_encoder()
        .expect("the plugin has audio encoders")
        .open_audio(&AudioEncoderRequest {
            component: "aac",
            settings: &settings(&[("bitrate", text("fast"))]),
            sample_rate: 48_000,
            channels: 2,
        });
    assert_eq!(refused(aac), "bitrate");
    // The program still serves after refusing.
    let opened = open(&mut program, "h264", &Settings::new(), &[PixelFormat::Nv12]).unwrap();
    program.video_encoder().unwrap().abort_video(opened.session);
}

/// A second of Opus in WebM, which only the FFmpeg plugin writes and
/// reads, at `output`.
fn webm(output: &Path) {
    let mut ffmpeg = FfmpegCodec::new();
    let encoder = ffmpeg.audio_encoder().unwrap();
    let opened = encoder
        .open_audio(&AudioEncoderRequest {
            component: "opus",
            settings: &Settings::new(),
            sample_rate: 48_000,
            channels: 2,
        })
        .unwrap();
    let mut answers = vec![
        encoder
            .encode_audio(opened.session, 0, &vec![0.0; 48_000 * 2])
            .unwrap(),
    ];
    answers.push(encoder.finish_audio(opened.session).unwrap());
    let (packets, config) = gathered(answers);
    let track = Track::Audio {
        config,
        timescale: opened.sample_rate,
        sample_rate: opened.sample_rate,
        channels: opened.channels,
        delay: opened.delay,
    };
    let muxer = ffmpeg.muxer().unwrap();
    let session = muxer
        .open_muxer(&MuxerRequest {
            component: "webm",
            settings: &Settings::new(),
            output,
            tracks: &[track],
            leading_audio: 0,
        })
        .unwrap();
    let packets: Vec<TrackPacket> = packets
        .into_iter()
        .map(|packet| TrackPacket { track: 0, packet })
        .collect();
    muxer.write_packets(session, &packets).unwrap();
    muxer.finish_muxer(session).unwrap();
}

#[test]
fn the_program_refuses_what_it_cannot_read() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("sound.webm");
    webm(&output);
    let mut program = program_with(FrameTransport::from_environment(), ProviderUse::Decode);
    match program
        .decoder()
        .expect("the plugin has a decoder")
        .probe(celesta_codec_avfoundation::DECODER_ID, &output)
    {
        Err(MediaError::Unsupported { path, .. }) => assert_eq!(path, output),
        other => panic!("expected unsupported, got {other:?}"),
    }
}
