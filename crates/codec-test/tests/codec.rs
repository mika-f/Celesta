//! The test codec in this process: what its encoders and muxer write the
//! decoder reads back exactly, and it refuses what the FFmpeg plugin would.

use celesta_codec_test::{
    AUDIO_CODEC, AUDIO_ENCODER_ID, DECODER_ID, MUXER_ID, StoredAudio, StoredVideo, TestCodec,
    TestMedia, VIDEO_CODEC, VIDEO_ENCODER_ID, read, write_audio, write_rgba_video,
};
use celesta_composition::{Rational, Time};
use celesta_media::{
    AudioEncoder, AudioEncoderRequest, Color, ComponentKind, Decoder, Encoded, MediaError,
    MediaProvider, Muxer, MuxerRequest, Packet, PixelFormat, SampleRates, Sequential, SessionId,
    SettingValue, Settings, Track, TrackConfig, TrackPacket, VideoEncoder, VideoEncoderOpened,
    VideoEncoderRequest, VideoFrame, VideoRequest,
};
use std::path::{Path, PathBuf};

const FRAME_RATE: Rational = Rational::new(25, 1);

const ALL_FORMATS: [PixelFormat; 6] = [
    PixelFormat::Rgba8,
    PixelFormat::Rgba16,
    PixelFormat::Yuv420p,
    PixelFormat::Yuv420p10,
    PixelFormat::Nv12,
    PixelFormat::P010,
];

fn settings(pairs: &[(&str, SettingValue)]) -> Settings {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect()
}

fn text(value: &str) -> SettingValue {
    SettingValue::String(value.to_owned())
}

/// `len` bytes that differ from frame to frame by `seed`.
fn frame(len: usize, seed: u8) -> Vec<u8> {
    (0..len)
        .map(|index| (index as u8).wrapping_mul(31).wrapping_add(seed))
        .collect()
}

fn open_video(
    codec: &mut TestCodec,
    settings: &Settings,
    width: u32,
    height: u32,
    offered: &[PixelFormat],
) -> Result<VideoEncoderOpened, MediaError> {
    codec.open_video(&VideoEncoderRequest {
        component: VIDEO_ENCODER_ID,
        settings,
        width,
        height,
        frame_rate: FRAME_RATE,
        timescale: FRAME_RATE.numerator,
        offered,
        color: &Color::bt709_limited(),
    })
}

fn open_audio(
    codec: &mut TestCodec,
    settings: &Settings,
    sample_rate: u32,
    channels: u16,
) -> Result<celesta_media::AudioEncoderOpened, MediaError> {
    codec.open_audio(&AudioEncoderRequest {
        component: AUDIO_ENCODER_ID,
        settings,
        sample_rate,
        channels,
    })
}

fn open_muxer(
    codec: &mut TestCodec,
    output: &Path,
    tracks: &[Track],
    leading_audio: u64,
) -> Result<SessionId, MediaError> {
    codec.open_muxer(&MuxerRequest {
        component: MUXER_ID,
        settings: &Settings::new(),
        output,
        tracks,
        leading_audio,
    })
}

fn video_track(config: TrackConfig, width: u32, height: u32) -> Track {
    Track::Video {
        config,
        timescale: FRAME_RATE.numerator,
        width,
        height,
        frame_rate: FRAME_RATE,
        color: Color::bt709_limited(),
        alpha: false,
    }
}

/// Encodes `frames` (one packet each, at their index) in `session`, and
/// finishes it: every packet and the track's config, which came once.
fn encode_frames(
    codec: &mut TestCodec,
    session: SessionId,
    frames: &[Vec<u8>],
) -> (Vec<Packet>, TrackConfig) {
    let mut packets = Vec::new();
    let mut configs = Vec::new();
    for (index, frame) in frames.iter().enumerate() {
        let Encoded {
            packets: ready,
            config,
        } = codec
            .encode_video(session, index as i64, frame.clone())
            .unwrap();
        packets.extend(ready);
        configs.extend(config);
    }
    let finished = codec.finish_video(session).unwrap();
    packets.extend(finished.packets);
    configs.extend(finished.config);
    assert_eq!(configs.len(), 1, "the config is answered once");
    (packets, configs.remove(0))
}

fn packets_for(track: usize, packets: Vec<Packet>) -> Vec<TrackPacket> {
    packets
        .into_iter()
        .map(|packet| TrackPacket { track, packet })
        .collect()
}

/// Writes `frames` as a video-only file: encoder, then muxer.
fn write_video_file(
    codec: &mut TestCodec,
    output: &Path,
    width: u32,
    height: u32,
    format: PixelFormat,
    frames: &[Vec<u8>],
) {
    let opened = open_video(
        codec,
        &settings(&[("input", text(&format.to_string()))]),
        width,
        height,
        &ALL_FORMATS,
    )
    .unwrap();
    let (packets, config) = encode_frames(codec, opened.session, frames);
    let muxer = open_muxer(codec, output, &[video_track(config, width, height)], 0).unwrap();
    codec
        .write_packets(muxer, &packets_for(0, packets))
        .unwrap();
    assert_eq!(codec.finish_muxer(muxer).unwrap(), [output.to_path_buf()]);
}

fn codec_message<T: std::fmt::Debug>(result: Result<T, MediaError>) -> String {
    match result {
        Err(MediaError::Codec(message)) => message,
        other => panic!("expected a codec error, got {other:?}"),
    }
}

fn invalid_key<T: std::fmt::Debug>(result: Result<T, MediaError>) -> String {
    match result {
        Err(MediaError::InvalidSettings { key, .. }) => key,
        other => panic!("expected invalid settings, got {other:?}"),
    }
}

fn frame_at(codec: &mut TestCodec, path: &Path, time: f64) -> Result<VideoFrame, MediaError> {
    codec.decode_video(
        DECODER_ID,
        &VideoRequest {
            path,
            time,
            sequential: None,
        },
    )
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn stores_every_pixel_format_byte_for_byte() {
    let directory = tempfile::tempdir().unwrap();
    for (format, len) in [
        (PixelFormat::Rgba8, 96),
        (PixelFormat::Rgba16, 192),
        (PixelFormat::Yuv420p, 36),
        (PixelFormat::Yuv420p10, 72),
        (PixelFormat::Nv12, 36),
        (PixelFormat::P010, 72),
    ] {
        let output = directory.path().join(format!("{format}.ctst"));
        let frames: Vec<_> = (0..3).map(|index| frame(len, index * 7 + 1)).collect();
        let mut codec = TestCodec::new();
        write_video_file(&mut codec, &output, 6, 4, format, &frames);

        assert_eq!(
            read(&output).unwrap(),
            TestMedia {
                extension: Some("ctst".to_owned()),
                video: Some(StoredVideo {
                    width: 6,
                    height: 4,
                    frame_rate: FRAME_RATE,
                    format,
                    frames,
                }),
                audio: None,
            },
            "{format:?}"
        );
        let probe = codec.probe(DECODER_ID, &output).unwrap();
        let video = probe.video.expect("a video stream");
        assert_eq!((video.width, video.height), (6, 4));
        assert_eq!(video.frame_rate, Some(FRAME_RATE));
        assert_eq!(video.codec.as_deref(), Some(VIDEO_CODEC));
        assert_eq!(video.duration, Some(Time::new(3, 25)));
        assert_eq!(probe.duration, Some(Time::new(3, 25)));
        assert!(probe.audio.is_empty());
    }
}

#[test]
fn video_packets_are_lz4_key_frames_and_the_config_comes_first() {
    let mut codec = TestCodec::new();
    let opened = open_video(
        &mut codec,
        &settings(&[("input", text("yuv420p"))]),
        2,
        2,
        &ALL_FORMATS,
    )
    .unwrap();
    assert_eq!(opened.input, PixelFormat::Yuv420p);
    assert!(!opened.alpha);

    let first = frame(6, 3);
    let answer = codec
        .encode_video(opened.session, 40, first.clone())
        .unwrap();
    assert_eq!(
        answer.config,
        Some(TrackConfig {
            codec: VIDEO_CODEC.to_owned(),
            description: Some(b"yuv420p".to_vec()),
        })
    );
    let [packet] = answer.packets.as_slice() else {
        panic!("one packet per frame");
    };
    assert_eq!(
        (packet.pts, packet.dts, packet.duration, packet.key),
        (40, 40, 1, true)
    );
    assert_eq!(
        lz4_flex::block::decompress_size_prepended(&packet.bytes).unwrap(),
        first
    );
    assert!(packet.additional.is_empty());

    let second = codec.encode_video(opened.session, 41, frame(6, 4)).unwrap();
    assert_eq!(second.config, None);
    assert!(
        codec
            .finish_video(opened.session)
            .unwrap()
            .packets
            .is_empty()
    );
    // The session ended with its finish.
    assert!(codec_message(codec.finish_video(opened.session)).contains("no open session"));
}

#[test]
fn yuv_frame_sizes_round_chroma_up() {
    let mut codec = TestCodec::new();
    let opened = open_video(&mut codec, &Settings::new(), 5, 3, &[PixelFormat::Yuv420p]).unwrap();
    // 5x3 luma, and two 3x2 chroma planes.
    codec.encode_video(opened.session, 0, vec![0; 27]).unwrap();
    let message = codec_message(codec.encode_video(opened.session, 1, vec![0; 26]));
    assert!(message.contains("27"), "{message}");
}

#[test]
fn auto_takes_the_first_offered() {
    let mut codec = TestCodec::new();
    let opened = open_video(
        &mut codec,
        &Settings::new(),
        2,
        2,
        &[PixelFormat::Yuv420p, PixelFormat::Rgba8],
    )
    .unwrap();
    assert_eq!(opened.input, PixelFormat::Yuv420p);

    // An explicit choice must be one of those offered.
    let rgba16 = settings(&[("input", text("rgba16"))]);
    assert_eq!(
        invalid_key(open_video(
            &mut codec,
            &rgba16,
            2,
            2,
            &[PixelFormat::Yuv420p]
        )),
        "input"
    );
    assert_eq!(
        invalid_key(open_video(&mut codec, &Settings::new(), 2, 2, &[])),
        "input"
    );
}

#[test]
fn alpha_needs_rgba16() {
    let mut codec = TestCodec::new();
    let alpha = settings(&[("alpha", SettingValue::Boolean(true))]);
    assert_eq!(
        invalid_key(open_video(
            &mut codec,
            &alpha,
            2,
            2,
            &[PixelFormat::Yuv420p]
        )),
        "alpha"
    );
    let opened = open_video(&mut codec, &alpha, 2, 2, &[PixelFormat::Rgba16]).unwrap();
    assert_eq!(opened.input, PixelFormat::Rgba16);
    assert!(opened.alpha);
}

#[test]
fn refuses_what_it_cannot_open() {
    let directory = tempfile::tempdir().unwrap();
    let mut codec = TestCodec::new();
    let offered = [PixelFormat::Rgba8];

    for (width, height) in [(0, 2), (2, 0)] {
        assert!(open_video(&mut codec, &Settings::new(), width, height, &offered).is_err());
    }
    let unknown = settings(&[("crf", SettingValue::Number(18.0))]);
    assert_eq!(
        invalid_key(open_video(&mut codec, &unknown, 2, 2, &offered)),
        "crf"
    );
    let too_long = settings(&[("delay", SettingValue::Number(65_537.0))]);
    assert_eq!(
        invalid_key(open_audio(&mut codec, &too_long, 48_000, 2)),
        "delay"
    );
    let output = directory.path().join("refused.ctst");
    assert!(
        codec
            .open_muxer(&MuxerRequest {
                component: MUXER_ID,
                settings: &settings(&[("x", text("y"))]),
                output: &output,
                tracks: &[],
                leading_audio: 0,
            })
            .is_err()
    );
    assert!(!output.exists());

    let unknown = SessionId(99);
    for message in [
        codec_message(codec.encode_video(unknown, 0, vec![0; 4])),
        codec_message(codec.finish_video(unknown)),
        codec_message(codec.encode_audio(unknown, 0, &[0.0])),
        codec_message(codec.finish_audio(unknown)),
        codec_message(codec.write_packets(unknown, &[])),
        codec_message(codec.finish_muxer(unknown)),
    ] {
        assert!(message.contains("no open session"), "{message}");
    }
    // Another plugin's component is none of this one's.
    let message = codec_message(codec.open_video(&VideoEncoderRequest {
        component: "x264",
        settings: &Settings::new(),
        width: 2,
        height: 2,
        frame_rate: FRAME_RATE,
        timescale: 25,
        offered: &offered,
        color: &Color::bt709_limited(),
    }));
    assert!(message.contains("x264"), "{message}");
}

#[test]
fn the_muxer_refuses_what_does_not_fit_its_tracks() {
    let directory = tempfile::tempdir().unwrap();
    let mut codec = TestCodec::new();
    let opened = open_video(&mut codec, &Settings::new(), 2, 2, &[PixelFormat::Rgba8]).unwrap();
    let (packets, config) = encode_frames(&mut codec, opened.session, &[frame(16, 1)]);

    // A video track that is not ctst.
    let h264 = TrackConfig {
        codec: "avc1.640028".to_owned(),
        description: None,
    };
    let output = directory.path().join("h264.ctst");
    let message = codec_message(open_muxer(
        &mut codec,
        &output,
        &[video_track(h264, 2, 2)],
        0,
    ));
    assert!(message.contains("ctst"), "{message}");

    // A packet for a track the muxer was not opened with.
    let output = directory.path().join("tracks.ctst");
    let muxer = open_muxer(&mut codec, &output, &[video_track(config.clone(), 2, 2)], 0).unwrap();
    let message = codec_message(codec.write_packets(muxer, &packets_for(1, packets.clone())));
    assert!(message.contains("no track 1"), "{message}");

    // A frame that is not the size of the track's frames: this one is for a
    // 4x2 track.
    let wrong = Packet {
        bytes: lz4_flex::block::compress_prepend_size(&[0; 32]),
        ..packets[0].clone()
    };
    let message = codec_message(codec.write_packets(muxer, &packets_for(0, vec![wrong])));
    assert!(message.contains("16"), "{message}");
    // Bytes that are not LZ4 data.
    let garbage = Packet {
        bytes: vec![0xff; 5],
        ..packets[0].clone()
    };
    codec_message(codec.write_packets(muxer, &packets_for(0, vec![garbage])));
    // The right frame still goes in.
    codec
        .write_packets(muxer, &packets_for(0, packets))
        .unwrap();
    codec.finish_muxer(muxer).unwrap();
    assert_eq!(read(&output).unwrap().video.unwrap().frames.len(), 1);
}

#[test]
fn pcm_delay_and_pre_roll_are_hidden() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("audio.mp4");
    let mut codec = TestCodec::new();
    let opened = open_audio(
        &mut codec,
        &settings(&[("delay", SettingValue::Number(1024.0))]),
        48_000,
        2,
    )
    .unwrap();
    assert_eq!(
        (opened.sample_rate, opened.channels, opened.delay),
        (48_000, 2, 1024)
    );

    let sample = |index: usize| (index as f32 * 0.37).sin();
    let pre_roll: Vec<f32> = (0..2048 * 2).map(sample).collect();
    let body: Vec<f32> = (0..8000 * 2).map(|index| sample(index + 5000)).collect();
    let mut packets = Vec::new();
    let mut config = None;
    for (pts, samples) in [(-2048, &pre_roll), (0, &body)] {
        let answer = codec.encode_audio(opened.session, pts, samples).unwrap();
        packets.extend(answer.packets);
        config = config.or(answer.config);
    }
    let finished = codec.finish_audio(opened.session).unwrap();
    assert!(finished.packets.is_empty());
    let config = config.expect("the config came with the first packets");
    assert_eq!(
        config,
        TrackConfig {
            codec: AUDIO_CODEC.to_owned(),
            description: None,
        }
    );

    // The silence the encoder primes with comes first, ahead of the first
    // samples given.
    assert_eq!(packets.len(), 3);
    assert_eq!(packets[0].pts, -3072);
    assert_eq!(packets[0].bytes, vec![0; 1024 * 2 * 4]);
    assert_eq!((packets[1].pts, packets[2].pts), (-3072, -1024));
    assert!(packets.iter().all(|packet| packet.dts == packet.pts));

    let track = Track::Audio {
        config,
        timescale: 48_000,
        sample_rate: 48_000,
        channels: 2,
        delay: 1024,
    };
    let muxer = open_muxer(&mut codec, &output, &[track], 2048).unwrap();
    // Out of order, as long as the times say where each goes.
    let mut shuffled = packets_for(0, packets);
    shuffled.reverse();
    shuffled.swap(1, 2);
    codec.write_packets(muxer, &shuffled[..1]).unwrap();
    codec.write_packets(muxer, &shuffled[1..]).unwrap();
    codec.finish_muxer(muxer).unwrap();

    let audio = read(&output).unwrap().audio.expect("audio");
    assert_eq!(audio.lead_in_frames, 3072);
    assert_eq!(audio.played(), body);
    let mut all = vec![0.0; 1024 * 2];
    all.extend(&pre_roll);
    all.extend(&body);
    assert_eq!(audio.samples, all);

    let decoded = codec.decode_audio(DECODER_ID, &output, 48_000, 2).unwrap();
    assert_eq!(decoded.samples, body);
    let probe = codec.probe(DECODER_ID, &output).unwrap();
    assert_eq!(probe.audio[0].codec.as_deref(), Some(AUDIO_CODEC));
    assert_eq!(probe.audio[0].duration, Some(Time::new(8000, 48_000)));
}

#[test]
fn a_video_and_audio_track_share_a_file() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("both.mov");
    let mut codec = TestCodec::new();
    let video = open_video(&mut codec, &Settings::new(), 2, 2, &[PixelFormat::Rgba8]).unwrap();
    let frames: Vec<_> = (0..3).map(|index| frame(16, index)).collect();
    let (video_packets, video_config) = encode_frames(&mut codec, video.session, &frames);
    let audio = open_audio(&mut codec, &Settings::new(), 44_100, 1).unwrap();
    assert_eq!(audio.sample_rate, 44_100, "any echoes the request");
    let samples: Vec<f32> = (0..30).map(|index| index as f32 / 30.0).collect();
    let answer = codec.encode_audio(audio.session, 0, &samples).unwrap();
    let audio_config = answer.config.unwrap();
    assert_eq!(answer.packets.len(), 1, "no delay, no silence");

    let tracks = [
        video_track(video_config, 2, 2),
        Track::Audio {
            config: audio_config,
            timescale: 44_100,
            sample_rate: 44_100,
            channels: 1,
            delay: 0,
        },
    ];
    let muxer = open_muxer(&mut codec, &output, &tracks, 0).unwrap();
    let mut packets = packets_for(0, video_packets);
    packets.extend(packets_for(1, answer.packets));
    codec.write_packets(muxer, &packets).unwrap();
    codec.finish_muxer(muxer).unwrap();

    let media = read(&output).unwrap();
    assert_eq!(media.extension.as_deref(), Some("mov"));
    assert_eq!(media.video.unwrap().frames, frames);
    assert_eq!(
        media.audio.unwrap(),
        StoredAudio {
            sample_rate: 44_100,
            channels: 1,
            lead_in_frames: 0,
            samples,
        }
    );
}

#[test]
fn a_fixed_sample_rate_is_answered() {
    let mut codec = TestCodec::new();
    let fixed = settings(&[("sampleRate", text("48000"))]);
    let opened = open_audio(&mut codec, &fixed, 44_100, 2).unwrap();
    assert_eq!(opened.sample_rate, 48_000);
    assert_eq!((opened.channels, opened.delay), (2, 0));
}

#[test]
fn an_aborted_mux_leaves_no_file() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("aborted.ctst");
    let mut codec = TestCodec::new();
    let opened = open_video(&mut codec, &Settings::new(), 2, 2, &[PixelFormat::Rgba8]).unwrap();
    let (packets, config) = encode_frames(&mut codec, opened.session, &[frame(16, 7)]);
    let muxer = open_muxer(&mut codec, &output, &[video_track(config, 2, 2)], 0).unwrap();
    codec
        .write_packets(muxer, &packets_for(0, packets))
        .unwrap();
    assert!(output.exists());

    assert!(codec.abort_muxer(muxer).is_empty());
    assert!(!output.exists());
    // Aborting again, or a session that is not a muxer's, is harmless.
    assert!(codec.abort_muxer(muxer).is_empty());
    assert!(codec.abort_muxer(SessionId(99)).is_empty());
}

#[test]
fn sessions_are_unique_across_kinds() {
    let directory = tempfile::tempdir().unwrap();
    let mut codec = TestCodec::new();
    let video = open_video(&mut codec, &Settings::new(), 2, 2, &[PixelFormat::Rgba8]).unwrap();
    let audio = open_audio(&mut codec, &Settings::new(), 48_000, 2).unwrap();
    let output = directory.path().join("any.ctst");
    let video_track = video_track(
        TrackConfig {
            codec: VIDEO_CODEC.to_owned(),
            description: Some(b"rgba8".to_vec()),
        },
        2,
        2,
    );
    let muxer = open_muxer(&mut codec, &output, &[video_track], 0).unwrap();
    let ids = [video.session, audio.session, muxer];
    assert!(
        ids[0] != ids[1] && ids[1] != ids[2] && ids[0] != ids[2],
        "{ids:?}"
    );

    // `abortEncoder` names only a session, and reaches both encoder kinds:
    // each ends its own session and ignores the other's.
    codec.abort_video(audio.session);
    codec.abort_audio(video.session);
    codec.encode_audio(audio.session, 0, &[0.0, 0.0]).unwrap();
    codec.encode_video(video.session, 0, vec![0; 16]).unwrap();
    codec.abort_video(video.session);
    codec.abort_audio(audio.session);
    codec_message(codec.encode_video(video.session, 1, vec![0; 16]));
    codec_message(codec.encode_audio(audio.session, 1, &[0.0, 0.0]));
}

#[test]
fn written_inputs_read_back() {
    let directory = tempfile::tempdir().unwrap();
    let mut codec = TestCodec::new();

    let video = directory.path().join("video.ctc");
    let frames = vec![frame(8, 1), frame(8, 2)];
    write_rgba_video(&video, 2, 1, Rational::new(2, 1), &frames).unwrap();
    assert_eq!(
        read(&video).unwrap(),
        TestMedia {
            extension: None,
            video: Some(StoredVideo {
                width: 2,
                height: 1,
                frame_rate: Rational::new(2, 1),
                format: PixelFormat::Rgba8,
                frames,
            }),
            audio: None,
        }
    );

    let sound = directory.path().join("sound.ctc");
    let samples = vec![0.5, -0.5, 0.25, -0.25];
    write_audio(&sound, 48_000, 2, &samples).unwrap();
    assert_eq!(
        read(&sound).unwrap(),
        TestMedia {
            extension: None,
            video: None,
            audio: Some(StoredAudio {
                sample_rate: 48_000,
                channels: 2,
                lead_in_frames: 0,
                samples: samples.clone(),
            }),
        }
    );
    assert_eq!(
        codec
            .decode_audio(DECODER_ID, &sound, 48_000, 2)
            .unwrap()
            .samples,
        samples
    );
    // Both are version 2 of the format.
    for path in [&video, &sound] {
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(&bytes[..8], b"CLSTTEST");
        assert_eq!(bytes[8..12], 2u32.to_le_bytes());
    }
}

#[test]
fn version_1_files_are_unsupported() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("old.ctc");
    write_audio(&path, 48_000, 1, &[0.0]).unwrap();
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[8..12].copy_from_slice(&1u32.to_le_bytes());
    std::fs::write(&path, bytes).unwrap();

    let mut codec = TestCodec::new();
    assert!(matches!(
        read(&path),
        Err(MediaError::Unsupported { path: named, .. }) if named == path
    ));
    assert!(matches!(
        codec.probe(DECODER_ID, &path),
        Err(MediaError::Unsupported { path: named, .. }) if named == path
    ));
}

#[test]
fn describe_lists_four_available_components() {
    let described = TestCodec::new().describe().unwrap();
    let listed: Vec<_> = described
        .iter()
        .map(|component| (component.kind, component.id.as_str(), component.available))
        .collect();
    assert_eq!(
        listed,
        [
            (ComponentKind::Decoder, DECODER_ID, true),
            (ComponentKind::VideoEncoder, VIDEO_ENCODER_ID, true),
            (ComponentKind::AudioEncoder, AUDIO_ENCODER_ID, true),
            (ComponentKind::Muxer, MUXER_ID, true),
        ]
    );
    let video = &described[1];
    assert_eq!(video.transports, ["pipe"]);
    assert_eq!(video.input_formats.len(), 6);
    let keys: Vec<_> = video.settings.iter().map(|field| &field.key[..]).collect();
    assert_eq!(keys, ["input", "alpha"]);
    let audio = &described[2];
    assert_eq!(audio.sample_rates, Some(SampleRates::Any));
    assert_eq!(audio.channels, [1, 2]);
    let keys: Vec<_> = audio.settings.iter().map(|field| &field.key[..]).collect();
    assert_eq!(keys, ["delay", "sampleRate"]);
    assert!(described[3].settings.is_empty());

    // The accessors say what is served.
    let mut codec = TestCodec::new();
    assert!(codec.decoder().is_some());
    assert!(codec.video_encoder().is_some());
    assert!(codec.audio_encoder().is_some());
    assert!(codec.muxer().is_some());
}

#[test]
fn every_cut_of_a_file_and_an_extra_byte_are_refused() {
    let directory = tempfile::tempdir().unwrap();
    let muxed = directory.path().join("muxed.ctst");
    let mut codec = TestCodec::new();
    let video = open_video(&mut codec, &Settings::new(), 2, 2, &[PixelFormat::Rgba8]).unwrap();
    let frames: Vec<_> = (0..2).map(|index| frame(16, index)).collect();
    let (video_packets, video_config) = encode_frames(&mut codec, video.session, &frames);
    let audio = open_audio(&mut codec, &Settings::new(), 48_000, 2).unwrap();
    let answer = codec
        .encode_audio(audio.session, 0, &[0.25, -0.5, 0.75, -1.0])
        .unwrap();
    let tracks = [
        video_track(video_config, 2, 2),
        Track::Audio {
            config: answer.config.unwrap(),
            timescale: 48_000,
            sample_rate: 48_000,
            channels: 2,
            delay: 0,
        },
    ];
    let muxer = open_muxer(&mut codec, &muxed, &tracks, 1).unwrap();
    let mut packets = packets_for(0, video_packets);
    packets.extend(packets_for(1, answer.packets));
    codec.write_packets(muxer, &packets).unwrap();
    codec.finish_muxer(muxer).unwrap();
    read(&muxed).unwrap();
    codec.probe(DECODER_ID, &muxed).unwrap();
    let bytes = std::fs::read(&muxed).unwrap();

    // The magic, `CLSTTEST`: a file cut within it is not a test codec file
    // at all; one cut anywhere after it is unfinished.
    let magic = b"CLSTTEST".len();
    for len in 0..bytes.len() {
        let cut = directory.path().join(format!("cut-{len}.ctst"));
        std::fs::write(&cut, &bytes[..len]).unwrap();
        let read = read(&cut).unwrap_err().to_string();
        let probed = codec.probe(DECODER_ID, &cut).unwrap_err().to_string();
        if len >= magic {
            assert!(read.contains("unfinished"), "{len} bytes: {read}");
            assert!(probed.contains("unfinished"), "{len} bytes: {probed}");
        }
    }

    let longer = directory.path().join("longer.ctst");
    std::fs::write(&longer, [&bytes[..], &[0]].concat()).unwrap();
    for message in [
        read(&longer).unwrap_err().to_string(),
        codec.probe(DECODER_ID, &longer).unwrap_err().to_string(),
    ] {
        assert!(message.contains("after its end chunk"), "{message}");
    }
}

#[test]
fn decodes_rgba_frames_at_their_times() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("two.ctc");
    let a = frame(8, 1);
    let b = frame(8, 2);
    write_rgba_video(&path, 2, 1, Rational::new(2, 1), &[a.clone(), b.clone()]).unwrap();
    let mut codec = TestCodec::new();

    // The first frame at or after the time, as FFmpeg's decoder serves it: a
    // time within 0.1 µs past a frame still lands on it, and an overrun holds
    // the last frame.
    for (time, expected) in [
        (0.0, &a),
        (5e-8, &a),
        (1e-6, &b),
        (0.49, &b),
        (0.5, &b),
        (60.0, &b),
    ] {
        let decoded = frame_at(&mut codec, &path, time).unwrap();
        assert_eq!((decoded.width, decoded.height), (2, 1));
        assert_eq!(*decoded.pixels, *expected, "at {time}");
    }
    let sequential = codec
        .decode_video(
            DECODER_ID,
            &VideoRequest {
                path: &path,
                time: 0.5,
                sequential: Some(Sequential {
                    session: "layer",
                    frame_rate: Rational::new(2, 1),
                }),
            },
        )
        .unwrap();
    assert_eq!(*sequential.pixels, b);
    assert!(matches!(
        frame_at(&mut codec, &path, -0.1),
        Err(MediaError::InvalidTimestamp(time)) if time == -0.1
    ));
    assert!(matches!(
        frame_at(&mut codec, &path, f64::NAN),
        Err(MediaError::InvalidTimestamp(time)) if time.is_nan()
    ));

    let deep = directory.path().join("deep.ctst");
    let pixel: Vec<u8> = [65535u16, 32896, 0, 257]
        .iter()
        .flat_map(|channel| channel.to_le_bytes())
        .collect();
    write_video_file(&mut codec, &deep, 1, 1, PixelFormat::Rgba16, &[pixel]);
    assert_eq!(
        *frame_at(&mut codec, &deep, 0.0).unwrap().pixels,
        [255, 128, 0, 1]
    );

    let yuv = directory.path().join("yuv.ctst");
    write_video_file(&mut codec, &yuv, 2, 2, PixelFormat::Yuv420p, &[vec![16; 6]]);
    codec_message(frame_at(&mut codec, &yuv, 0.0));

    let sound = directory.path().join("sound.ctc");
    write_audio(&sound, 48_000, 1, &[0.0; 4]).unwrap();
    assert!(matches!(
        frame_at(&mut codec, &sound, 0.0),
        Err(MediaError::NoVideoStream(path)) if path == sound
    ));
}

#[test]
fn converts_channels_but_never_resamples() {
    let directory = tempfile::tempdir().unwrap();
    let mut codec = TestCodec::new();

    let mono = directory.path().join("mono.ctc");
    write_audio(&mono, 48_000, 1, &[0.5, -0.25]).unwrap();
    let stereo = codec.decode_audio(DECODER_ID, &mono, 48_000, 2).unwrap();
    assert_eq!((stereo.sample_rate, stereo.channels), (48_000, 2));
    assert_eq!(stereo.samples, [0.5, 0.5, -0.25, -0.25]);

    let both = directory.path().join("stereo.ctc");
    write_audio(&both, 48_000, 2, &[1.0, 0.0]).unwrap();
    let down = codec.decode_audio(DECODER_ID, &both, 48_000, 1).unwrap();
    assert_eq!(down.channels, 1);
    assert_eq!(down.samples, [0.5]);

    let message = codec_message(codec.decode_audio(DECODER_ID, &mono, 44_100, 1));
    assert!(
        message.contains("48000") && message.contains("44100"),
        "{message}"
    );

    let silent = directory.path().join("silent.ctc");
    write_rgba_video(&silent, 1, 1, Rational::new(30, 1), &[vec![0; 4]]).unwrap();
    let none = codec.decode_audio(DECODER_ID, &silent, 48_000, 2).unwrap();
    assert_eq!((none.sample_rate, none.channels), (48_000, 2));
    assert!(none.samples.is_empty());
}

#[test]
fn reads_the_checked_in_voices() {
    let assets = workspace_root().join("examples/assets");
    let voice = assets.join("voices/001.wav");
    let mut codec = TestCodec::new();

    let probe = codec.probe(DECODER_ID, &voice).unwrap();
    assert_eq!(probe.video, None);
    assert_eq!(probe.audio.len(), 1);
    let stream = &probe.audio[0];
    assert_eq!(stream.codec.as_deref(), Some("pcm_s16le"));
    assert_eq!(stream.sample_rate, Some(44_100));
    assert_eq!(stream.channels, Some(1));
    assert_eq!(stream.duration, Some(Time::new(84_022, 44_100)));
    assert_eq!(probe.duration, Some(Time::new(84_022, 44_100)));

    let decoded = codec.decode_audio(DECODER_ID, &voice, 44_100, 1).unwrap();
    assert_eq!(decoded.samples.len(), 84_022);
    assert_eq!(decoded.samples[..2], [-11.0 / 32768.0, -15.0 / 32768.0]);

    // Its fmt chunk is followed by an FLLR chunk before the data.
    let padded = codec
        .probe(DECODER_ID, &assets.join("voice-folder/shizuku/voice-1.wav"))
        .unwrap();
    assert_eq!(padded.audio.len(), 1);
    assert_eq!(padded.audio[0].sample_rate, Some(16_000));
    assert_eq!(padded.audio[0].channels, Some(1));
}

#[test]
fn names_files_it_cannot_read() {
    let directory = tempfile::tempdir().unwrap();
    let mut codec = TestCodec::new();

    let missing = directory.path().join("missing.wav");
    assert!(
        matches!(codec.probe(DECODER_ID, &missing), Err(MediaError::NotFound(path)) if path == missing)
    );
    assert!(matches!(
        frame_at(&mut codec, &missing, 0.0),
        Err(MediaError::NotFound(path)) if path == missing
    ));
    assert!(matches!(
        codec.decode_audio(DECODER_ID, &missing, 48_000, 2),
        Err(MediaError::NotFound(path)) if path == missing
    ));

    let random = directory.path().join("random.mp4");
    let mut state = 0x2545_f491_u32;
    let noise: Vec<u8> = (0..64)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8
        })
        .collect();
    std::fs::write(&random, noise).unwrap();

    // A 32-bit float WAV: format tag 3.
    let float = directory.path().join("float.wav");
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&40u32.to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&3u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&48_000u32.to_le_bytes());
    wav.extend_from_slice(&192_000u32.to_le_bytes());
    wav.extend_from_slice(&4u16.to_le_bytes());
    wav.extend_from_slice(&32u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&4u32.to_le_bytes());
    wav.extend_from_slice(&0.5f32.to_le_bytes());
    std::fs::write(&float, wav).unwrap();

    for path in [&random, &float] {
        let name = path.display().to_string();
        for message in [
            codec.probe(DECODER_ID, path).unwrap_err().to_string(),
            frame_at(&mut codec, path, 0.0).unwrap_err().to_string(),
            codec
                .decode_audio(DECODER_ID, path, 48_000, 1)
                .unwrap_err()
                .to_string(),
        ] {
            assert!(message.contains(&name), "{message}");
        }
    }
}
