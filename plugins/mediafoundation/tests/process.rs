//! The built `celesta-codec-mediafoundation` program, driven through
//! `celesta_media::MediaProcess` as Celesta drives it for an export: frames
//! reach the encoder through the ring of shared memory, pipelined, and come
//! out as the encoder in this process gives them; errors keep their kind
//! across the protocol.
#![cfg(windows)]

use celesta_codec_ffmpeg::FfmpegCodec;
use celesta_codec_mediafoundation::MfCodec;
use celesta_composition::Rational;
use celesta_media::{
    Answered, AudioEncoderRequest, Color, Encoded, FrameTransport, MediaError, MediaProcess,
    MediaProvider, MuxerRequest, Packet, PixelFormat, ProviderUse, SettingValue, Settings, Track,
    TrackConfig, TrackPacket, VideoEncoderOpened, VideoEncoderRequest, ring_layout,
};
use ffmpeg_sys_next as ff;
use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};

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
    let executable = PathBuf::from(env!("CARGO_BIN_EXE_celesta-codec-mediafoundation"));
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

fn text(value: &str) -> SettingValue {
    SettingValue::String(value.to_owned())
}

fn settings(pairs: &[(&str, SettingValue)]) -> Settings {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect()
}

/// `pairs` with `hardware: false`: Microsoft's software MFT, which takes
/// 64x36 where NVIDIA's refuses it.
fn software(pairs: &[(&str, SettingValue)]) -> Settings {
    let mut given = settings(pairs);
    given.insert("hardware".to_owned(), SettingValue::Boolean(false));
    given
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

/// The mean luma of each frame of `path`'s video stream as libavcodec
/// decodes it, in presentation order.
fn decoded_lumas(path: &Path) -> Vec<f64> {
    let url = CString::new(path.to_string_lossy().as_bytes()).unwrap();
    let mut lumas = Vec::new();
    // SAFETY: every pointer comes from the allocator it is freed with; the
    // decoder is opened from the stream's parameters; the first plane of a
    // decoded 8-bit YUV frame holds `linesize` bytes for each of `height`
    // rows of `width` samples.
    unsafe {
        let mut format = null_mut();
        assert!(ff::avformat_open_input(&mut format, url.as_ptr(), null(), null_mut()) >= 0);
        assert!(ff::avformat_find_stream_info(format, null_mut()) >= 0);
        let streams = std::slice::from_raw_parts((*format).streams, (*format).nb_streams as usize);
        let index = streams
            .iter()
            .position(|&stream| {
                (*(*stream).codecpar).codec_type == ff::AVMediaType::AVMEDIA_TYPE_VIDEO
            })
            .expect("a video stream");
        let parameters = (*streams[index]).codecpar;
        let decoder = ff::avcodec_find_decoder((*parameters).codec_id);
        assert!(!decoder.is_null(), "no decoder");
        let mut context = ff::avcodec_alloc_context3(decoder);
        assert!(ff::avcodec_parameters_to_context(context, parameters) >= 0);
        assert_eq!(ff::avcodec_open2(context, decoder, null_mut()), 0);
        let mut packet = ff::av_packet_alloc();
        let mut frame = ff::av_frame_alloc();
        let receive = |lumas: &mut Vec<f64>| {
            loop {
                let result = ff::avcodec_receive_frame(context, frame);
                if result == ff::AVERROR(ff::EAGAIN) || result == ff::AVERROR_EOF {
                    break;
                }
                assert_eq!(result, 0, "the decoder failed");
                assert_eq!(
                    ((*frame).width, (*frame).height),
                    (WIDTH as i32, HEIGHT as i32)
                );
                let stride = (*frame).linesize[0] as isize;
                let mut sum = 0u64;
                for row in 0..HEIGHT as isize {
                    let start = (*frame).data[0].offset(row * stride);
                    for column in 0..WIDTH as usize {
                        sum += u64::from(*start.add(column));
                    }
                }
                lumas.push(sum as f64 / f64::from(WIDTH * HEIGHT));
            }
        };
        while ff::av_read_frame(format, packet) >= 0 {
            if (*packet).stream_index == index as i32 {
                assert_eq!(ff::avcodec_send_packet(context, packet), 0);
                receive(&mut lumas);
            }
            ff::av_packet_unref(packet);
        }
        assert_eq!(ff::avcodec_send_packet(context, null_mut()), 0);
        receive(&mut lumas);
        ff::av_frame_free(&mut frame);
        ff::av_packet_free(&mut packet);
        ff::avcodec_free_context(&mut context);
        ff::avformat_close_input(&mut format);
    }
    lumas
}

#[test]
fn encodes_through_the_ring() {
    let ring = ring_layout(&[PixelFormat::Nv12], WIDTH, HEIGHT);
    assert!(ring.is_some(), "a 64x36 frame fits a ring");
    let mut program = program_with(FrameTransport::SharedMemory, ProviderUse::Export { ring });
    assert!(program.shares_frames(), "the program declined the ring");

    let opened = open(&mut program, "h264", &software(&[]), &[PixelFormat::Nv12]).unwrap();
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
    // is shown (the software encoder reorders).
    assert_eq!(through_ring.len(), FRAMES);
    assert!(through_ring[0].key, "the first packet is a keyframe");
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
    assert!(config.codec.starts_with("avc1."), "{}", config.codec);

    // The same frames encoded in this process.
    let mut codec = MfCodec::new();
    let local = open(&mut codec, "h264", &software(&[]), &[PixelFormat::Nv12]).unwrap();
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
    let (ring_lumas, local_lumas) = (decoded_lumas(&ring_file), decoded_lumas(&local_file));
    assert_eq!(ring_lumas.len(), FRAMES);
    assert_eq!(local_lumas.len(), FRAMES);
    for (index, (ring, local)) in ring_lumas.iter().zip(&local_lumas).enumerate() {
        assert!(
            (ring - local).abs() <= 2.0,
            "frame {index}: {ring} through the ring, {local} in process"
        );
        assert!(
            (ring - f64::from(grey(index))).abs() <= 2.0,
            "frame {index}: {ring}, not {}",
            grey(index)
        );
    }
    // Each frame decodes to its own grey, lighter than the one before: the
    // ring's slots were read in the order they were sent.
    for pair in ring_lumas.windows(2) {
        assert!(pair[0] + 5.0 < pair[1], "{ring_lumas:?}");
    }
}

/// The key of a [`MediaError::InvalidSettings`].
fn refused<T: std::fmt::Debug>(result: Result<T, MediaError>) -> String {
    match result {
        Err(MediaError::InvalidSettings { key, .. }) => key,
        other => panic!("expected invalid settings, got {other:?}"),
    }
}

/// Whether `error` is what probing a file the decoder cannot read gives.
/// The decoder's refusal of a file Media Foundation's reader cannot open:
/// `Unsupported`, naming the file.
fn is_the_refusal_of_unreadable_files(error: &MediaError, path: &Path) -> bool {
    matches!(error, MediaError::Unsupported { path: refused, .. } if refused == path)
}

#[test]
fn errors_keep_their_kind() {
    let mut program = program_with(
        FrameTransport::from_environment(),
        ProviderUse::Export { ring: None },
    );
    let as_text = software(&[("rateControl", text("bitrate")), ("bitrate", text("fast"))]);
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
    let opened = open(&mut program, "h264", &software(&[]), &[PixelFormat::Nv12]).unwrap();
    program.video_encoder().unwrap().abort_video(opened.session);
    drop(program);

    // A file the decoder cannot read: a WebM, which only the FFmpeg plugin
    // writes and reads, unless this Windows has a WebM source (this
    // machine's Windows 11 reads Opus in WebM).
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("sound.webm");
    webm(&output);
    let mut program = program_with(FrameTransport::from_environment(), ProviderUse::Decode);
    let decoder = program.decoder().expect("the plugin has a decoder");
    match decoder.probe(celesta_codec_mediafoundation::DECODER_ID, &output) {
        Err(error) if is_the_refusal_of_unreadable_files(&error, &output) => {}
        Ok(probe) => println!("this Windows reads WebM: {probe:?}"),
        other => panic!("expected the refusal of an unreadable file, got {other:?}"),
    }
    // And a file no Media Foundation reads, on every Windows.
    let text = directory.path().join("x.mp4");
    std::fs::write(&text, "not a movie\n").unwrap();
    match decoder.probe(celesta_codec_mediafoundation::DECODER_ID, &text) {
        Err(error) if is_the_refusal_of_unreadable_files(&error, &text) => {}
        other => panic!("expected the refusal of an unreadable file, got {other:?}"),
    }
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
