//! The FFmpeg plugin's encoders and muxers, in this process: the input each
//! video encoder chooses of those offered, the rate and delay each audio
//! encoder answers, the packets and track config they give, and what
//! `describe` says of every component.

use celesta_codec_ffmpeg::FfmpegCodec;
use celesta_composition::Rational;
use celesta_media::{
    AudioEncoderOpened, AudioEncoderRequest, Color, ComponentKind, Encoded, MediaError,
    MediaProvider, MuxerRequest, Packet, PixelFormat, SampleRates, SettingValue, Settings, Track,
    TrackConfig, VideoEncoderOpened, VideoEncoderRequest,
};
use ffmpeg_sys_next::{
    AVCodecID, AVERROR, AVERROR_EOF, EAGAIN, av_frame_alloc, av_frame_free, av_malloc,
    av_packet_alloc, av_packet_free, avcodec_alloc_context3, avcodec_find_decoder,
    avcodec_free_context, avcodec_open2, avcodec_receive_frame, avcodec_send_packet,
};
use std::collections::BTreeSet;
use std::ptr::null_mut;

const WIDTH: u32 = 32;
const HEIGHT: u32 = 16;
const FRAMES: i64 = 10;

fn settings(pairs: &[(&str, SettingValue)]) -> Settings {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect()
}

fn text(value: &str) -> SettingValue {
    SettingValue::String(value.to_owned())
}

/// Opens `component` at 32x16 and 10 fps.
fn open(
    codec: &mut FfmpegCodec,
    component: &str,
    given: &Settings,
    offered: &[PixelFormat],
) -> Result<VideoEncoderOpened, MediaError> {
    let color = Color::bt709_limited();
    codec
        .video_encoder()
        .expect("the plugin has video encoders")
        .open_video(&VideoEncoderRequest {
            component,
            settings: given,
            width: WIDTH,
            height: HEIGHT,
            frame_rate: Rational::new(10, 1),
            timescale: 10,
            offered,
            color: &color,
        })
}

/// One frame of `format` at 32x16 whose colors move with `index`.
fn frame(format: PixelFormat, index: i64) -> Vec<u8> {
    let (width, height) = (WIDTH as usize, HEIGHT as usize);
    let step = index as u8 * 20;
    match format {
        PixelFormat::Rgba8 => [step, 64, 255 - step, 255].repeat(width * height),
        PixelFormat::Rgba16 => [step, 64, 255 - step, 128]
            .map(|channel| u16::from(channel) * 257)
            .iter()
            .flat_map(|channel| channel.to_le_bytes())
            .collect::<Vec<u8>>()
            .repeat(width * height),
        PixelFormat::Yuv420p => {
            let mut planes = vec![16 + step / 2; width * height];
            planes.extend(vec![128 - step / 4; width * height / 2]);
            planes
        }
        PixelFormat::Yuv420p10 => {
            let mut samples = vec![64 + u16::from(step) * 2; width * height];
            samples.extend(vec![512 - u16::from(step); width * height / 2]);
            samples
                .iter()
                .flat_map(|sample| sample.to_le_bytes())
                .collect()
        }
        other => panic!("no test frames of {other}"),
    }
}

/// Ten frames through `opened`'s session and its finish: every packet, and
/// the config of the first answer with packets.
fn encode_all(codec: &mut FfmpegCodec, opened: VideoEncoderOpened) -> (Vec<Packet>, TrackConfig) {
    let encoder = codec.video_encoder().unwrap();
    let mut packets = Vec::new();
    let mut config = None;
    let mut take = |answer: Encoded, packets: &mut Vec<Packet>| {
        if config.is_none() && !answer.packets.is_empty() {
            config = Some(
                answer
                    .config
                    .expect("the first packets come with the config"),
            );
        }
        packets.extend(answer.packets);
    };
    for index in 0..FRAMES {
        let answer = encoder
            .encode_video(opened.session, index, frame(opened.input, index))
            .unwrap();
        take(answer, &mut packets);
    }
    let answer = encoder.finish_video(opened.session).unwrap();
    take(answer, &mut packets);
    (packets, config.expect("the encoder made packets"))
}

#[test]
fn libx264_packets_carry_an_avcc_and_a_codec_string() {
    let mut codec = FfmpegCodec::new();
    let opened = open(
        &mut codec,
        "libx264",
        &settings(&[("preset", text("ultrafast"))]),
        &[
            PixelFormat::Yuv420p,
            PixelFormat::Rgba16,
            PixelFormat::Rgba8,
        ],
    )
    .unwrap();
    assert_eq!(opened.input, PixelFormat::Yuv420p);
    assert!(!opened.alpha);
    let (packets, config) = encode_all(&mut codec, opened);
    assert!(config.codec.starts_with("avc1."), "{}", config.codec);
    let description = config.description.expect("H.264 has an avcC");
    assert_eq!(description[0], 1);
    assert_eq!(description[5] & 0x1F, 1, "one SPS");
    assert_eq!(packets.len(), FRAMES as usize);
    assert!(packets[0].key);
    for packet in &packets {
        let length = u32::from_be_bytes(packet.bytes[..4].try_into().unwrap()) as usize;
        assert!(
            length + 4 <= packet.bytes.len(),
            "{length} of {}",
            packet.bytes.len()
        );
        assert_eq!(packet.duration, 1);
    }
}

#[test]
fn ten_bit_h264_picks_yuv420p10_when_offered() {
    let mut codec = FfmpegCodec::new();
    let opened = open(
        &mut codec,
        "libx264",
        &settings(&[("bitDepth", text("10")), ("preset", text("ultrafast"))]),
        &[
            PixelFormat::Yuv420p,
            PixelFormat::Yuv420p10,
            PixelFormat::Rgba16,
        ],
    )
    .unwrap();
    assert_eq!(opened.input, PixelFormat::Yuv420p10);
    let (_, config) = encode_all(&mut codec, opened);
    // High 10 is profile 110.
    assert!(config.codec.starts_with("avc1.6E"), "{}", config.codec);
}

/// Decodes `packets` with libavcodec's H.264 decoder given `avcc`: the
/// sizes of the frames it gives.
fn decode_h264(avcc: &[u8], packets: &[Packet]) -> Vec<(i32, i32)> {
    let mut sizes = Vec::new();
    // SAFETY: every pointer comes from the allocator it is freed with; the
    // extradata carries libavcodec's input padding, and libavcodec copies
    // the packets' data, which is not reference counted, into padded
    // buffers of its own.
    unsafe {
        let decoder = avcodec_find_decoder(AVCodecID::AV_CODEC_ID_H264);
        assert!(!decoder.is_null());
        let mut context = avcodec_alloc_context3(decoder);
        let padding = 64;
        let extradata = av_malloc(avcc.len() + padding).cast::<u8>();
        std::ptr::write_bytes(extradata, 0, avcc.len() + padding);
        std::ptr::copy_nonoverlapping(avcc.as_ptr(), extradata, avcc.len());
        (*context).extradata = extradata;
        (*context).extradata_size = avcc.len() as i32;
        assert_eq!(avcodec_open2(context, decoder, null_mut()), 0);
        let mut packet = av_packet_alloc();
        let mut frame = av_frame_alloc();
        let receive = |sizes: &mut Vec<(i32, i32)>| loop {
            let result = avcodec_receive_frame(context, frame);
            if result == AVERROR(EAGAIN) || result == AVERROR_EOF {
                break;
            }
            assert_eq!(result, 0, "the decoder failed");
            sizes.push(((*frame).width, (*frame).height));
        };
        for encoded in packets {
            let copy = encoded.bytes.clone();
            (*packet).data = copy.as_ptr().cast_mut();
            (*packet).size = copy.len() as i32;
            (*packet).pts = encoded.pts;
            (*packet).dts = encoded.dts;
            assert_eq!(avcodec_send_packet(context, packet), 0);
            (*packet).data = null_mut();
            (*packet).size = 0;
            receive(&mut sizes);
        }
        assert_eq!(avcodec_send_packet(context, null_mut()), 0);
        receive(&mut sizes);
        av_frame_free(&mut frame);
        av_packet_free(&mut packet);
        avcodec_free_context(&mut context);
    }
    sizes
}

#[test]
fn rgba_input_is_converted() {
    let mut codec = FfmpegCodec::new();
    let opened = open(
        &mut codec,
        "libx264",
        &settings(&[("preset", text("ultrafast"))]),
        &[PixelFormat::Rgba8],
    )
    .unwrap();
    assert_eq!(opened.input, PixelFormat::Rgba8);
    assert!(!opened.alpha);
    let (packets, config) = encode_all(&mut codec, opened);
    let sizes = decode_h264(&config.description.unwrap(), &packets);
    assert_eq!(sizes, vec![(WIDTH as i32, HEIGHT as i32); FRAMES as usize]);
}

#[test]
fn software_offers_pick_rgba_by_bit_depth() {
    let offered = [PixelFormat::Rgba16, PixelFormat::Rgba8];
    let mut codec = FfmpegCodec::new();
    let eight = open(
        &mut codec,
        "libx264",
        &settings(&[("bitDepth", text("8"))]),
        &offered,
    )
    .unwrap();
    assert_eq!(eight.input, PixelFormat::Rgba8);
    let ten = open(
        &mut codec,
        "libx264",
        &settings(&[("bitDepth", text("10"))]),
        &offered,
    )
    .unwrap();
    assert_eq!(ten.input, PixelFormat::Rgba16);
    let (_, config) = encode_all(&mut codec, ten);
    assert!(config.codec.starts_with("avc1.6E"), "{}", config.codec);

    let refused = |error: MediaError| match error {
        MediaError::InvalidSettings { key, message } => (key, message),
        other => panic!("expected invalid settings, got {other:?}"),
    };
    let error = open(
        &mut codec,
        "libx264",
        &Settings::new(),
        &[PixelFormat::Rgba16],
    )
    .unwrap_err();
    assert_eq!(
        refused(error),
        ("input".to_owned(), "needs yuv420p or rgba8".to_owned())
    );
    let error = open(
        &mut codec,
        "libx264",
        &settings(&[("bitDepth", text("10"))]),
        &[PixelFormat::Yuv420p, PixelFormat::Rgba8],
    )
    .unwrap_err();
    assert_eq!(
        refused(error),
        ("input".to_owned(), "needs yuv420p10 or rgba16".to_owned())
    );
    let error = open(
        &mut codec,
        "prores",
        &Settings::new(),
        &[PixelFormat::Rgba8],
    )
    .unwrap_err();
    assert_eq!(
        refused(error),
        ("input".to_owned(), "needs rgba16".to_owned())
    );
}

#[test]
fn sessions_are_unique_across_kinds() {
    let mut codec = FfmpegCodec::new();
    let offered = [PixelFormat::Rgba16];
    let first = open(
        &mut codec,
        "libx264",
        &Settings::new(),
        &[PixelFormat::Rgba8],
    )
    .unwrap();
    let second = open(&mut codec, "prores", &Settings::new(), &offered).unwrap();
    let audio = open_audio(&mut codec, "aac", &Settings::new(), 48_000, 2).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("film.mp4");
    let muxer = codec
        .muxer()
        .unwrap()
        .open_muxer(&MuxerRequest {
            component: "mp4",
            settings: &Settings::new(),
            output: &output,
            tracks: &[aac_track()],
            leading_audio: 0,
        })
        .unwrap();
    let sessions: BTreeSet<u64> = [first.session, second.session, audio.session, muxer]
        .iter()
        .map(|session| session.0)
        .collect();
    assert_eq!(sessions.len(), 4, "{sessions:?}");
    let encoder = codec.video_encoder().unwrap();
    encoder.abort_video(first.session);
    encoder.abort_video(second.session);
    // An aborted session is gone.
    assert!(
        encoder
            .encode_video(first.session, 0, frame(PixelFormat::Rgba8, 0))
            .is_err()
    );
    let audio_encoder = codec.audio_encoder().unwrap();
    audio_encoder.abort_audio(audio.session);
    assert!(
        audio_encoder
            .encode_audio(audio.session, 0, &[0.0; 2])
            .is_err()
    );
    // An aborted muxer leaves its file for Celesta to remove.
    assert_eq!(codec.muxer().unwrap().abort_muxer(muxer), [output]);
    assert!(codec.muxer().unwrap().write_packets(muxer, &[]).is_err());
}

/// The key and message of the refusal `result` holds.
fn refused<T: std::fmt::Debug>(result: Result<T, MediaError>) -> (String, String) {
    match result {
        Err(MediaError::InvalidSettings { key, message }) => (key, message),
        other => panic!("expected invalid settings, got {other:?}"),
    }
}

/// Opens audio encoder `component` for a mix at `sample_rate`.
fn open_audio(
    codec: &mut FfmpegCodec,
    component: &str,
    given: &Settings,
    sample_rate: u32,
    channels: u16,
) -> Result<AudioEncoderOpened, MediaError> {
    codec
        .audio_encoder()
        .expect("the plugin has audio encoders")
        .open_audio(&AudioEncoderRequest {
            component,
            settings: given,
            sample_rate,
            channels,
        })
}

/// A stereo 48 kHz AAC track, as `aac` describes it.
fn aac_track() -> Track {
    Track::Audio {
        config: TrackConfig {
            codec: "mp4a.40.2".to_owned(),
            description: Some(vec![0x11, 0x90]),
        },
        timescale: 48_000,
        sample_rate: 48_000,
        channels: 2,
        delay: 1024,
    }
}

/// `frames` sample frames of a quiet stereo tone through `opened`, in the
/// exporter's chunks, then the finish: every packet and the config.
fn encode_audio(
    codec: &mut FfmpegCodec,
    opened: &AudioEncoderOpened,
    frames: usize,
) -> (Vec<Packet>, TrackConfig) {
    let channels = usize::from(opened.channels);
    let samples: Vec<f32> = (0..frames * channels)
        .map(|index| ((index / channels) as f32 * 0.05).sin() * 0.25)
        .collect();
    let encoder = codec.audio_encoder().unwrap();
    let mut packets = Vec::new();
    let mut config = None;
    let mut pts = 0;
    for chunk in samples.chunks(1024 * 16 * channels) {
        let answer = encoder.encode_audio(opened.session, pts, chunk).unwrap();
        pts += (chunk.len() / channels) as i64;
        if config.is_none() && !answer.packets.is_empty() {
            config = Some(
                answer
                    .config
                    .expect("the first packets come with the config"),
            );
        }
        packets.extend(answer.packets);
    }
    let answer = encoder.finish_audio(opened.session).unwrap();
    config = config.or(answer.config);
    packets.extend(answer.packets);
    (packets, config.expect("the encoder gave its config"))
}

#[test]
fn aac_takes_the_mixs_rate_when_it_can() {
    let mut codec = FfmpegCodec::new();
    for (asked, answered) in [(8_000, 8_000), (44_100, 44_100), (12_345, 48_000)] {
        let opened = open_audio(&mut codec, "aac", &Settings::new(), asked, 1).unwrap();
        assert_eq!(
            (opened.sample_rate, opened.channels, opened.delay),
            (answered, 1, 1024),
            "{asked} Hz"
        );
        codec.audio_encoder().unwrap().abort_audio(opened.session);
    }
    let opened = open_audio(&mut codec, "aac", &Settings::new(), 48_000, 2).unwrap();
    let (packets, config) = encode_audio(&mut codec, &opened, 48_000);
    assert_eq!(config.codec, "mp4a.40.2");
    // AudioSpecificConfig: AAC-LC (2), 48 kHz (index 3), two channels.
    let description = config.description.unwrap();
    assert_eq!(description[..2], [0x11, 0x90]);
    // The first packet starts at the delay before 0; each lasts a frame,
    // the last only the samples left.
    assert_eq!(packets[0].pts, -1024);
    assert!(packets.iter().all(|packet| packet.key));
    let total: i64 = packets.iter().map(|packet| packet.duration).sum();
    assert_eq!(total, 48_000 + 1024);
}

#[test]
fn opus_always_takes_48_khz() {
    let mut codec = FfmpegCodec::new();
    let opened = open_audio(&mut codec, "opus", &Settings::new(), 44_100, 2).unwrap();
    assert_eq!((opened.sample_rate, opened.channels), (48_000, 2));
    assert!(opened.delay > 0, "libopus primes its output");
    let (packets, config) = encode_audio(&mut codec, &opened, 48_000);
    assert_eq!(config.codec, "opus");
    assert_eq!(&config.description.unwrap()[..8], b"OpusHead");
    assert_eq!(packets[0].pts, -(opened.delay as i64));
    let total: i64 = packets.iter().map(|packet| packet.duration).sum();
    assert_eq!(total, 48_000 + opened.delay as i64);
}

#[test]
fn pcm_packs_each_chunk_as_one_packet() {
    let mut codec = FfmpegCodec::new();
    for (format, bytes) in [("s16le", 2), ("s24le", 3), ("f32le", 4)] {
        let opened = open_audio(
            &mut codec,
            "pcm",
            &settings(&[("format", text(format))]),
            22_050,
            2,
        )
        .unwrap();
        assert_eq!(
            (opened.sample_rate, opened.channels, opened.delay),
            (22_050, 2, 0)
        );
        let (packets, config) = encode_audio(&mut codec, &opened, 20_000);
        assert_eq!(config.codec, format!("lpcm.{format}"));
        assert_eq!(config.description, None);
        let times: Vec<(i64, i64)> = packets
            .iter()
            .map(|packet| (packet.pts, packet.duration))
            .collect();
        assert_eq!(times, [(0, 16_384), (16_384, 3_616)]);
        assert_eq!(packets[1].bytes.len(), 3_616 * 2 * bytes);
    }
}

#[test]
fn audio_and_muxer_settings_are_checked() {
    let mut codec = FfmpegCodec::new();
    let low = settings(&[("bitrate", SettingValue::Number(1_000.0))]);
    assert_eq!(
        refused(open_audio(&mut codec, "aac", &low, 48_000, 2)).0,
        "bitrate"
    );
    let unknown = settings(&[("format", text("s32le"))]);
    assert_eq!(
        refused(open_audio(&mut codec, "pcm", &unknown, 48_000, 2)).0,
        "format"
    );
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("clip.webm");
    let request = |given| MuxerRequest {
        component: "webm",
        settings: given,
        output: &output,
        tracks: &[],
        leading_audio: 0,
    };
    let faststart = settings(&[("faststart", SettingValue::Boolean(true))]);
    assert_eq!(
        refused(codec.muxer().unwrap().open_muxer(&request(&faststart))).0,
        "faststart"
    );
    // A codec the muxer has no mapping for is refused before any file is
    // made.
    let theora = Track::Video {
        config: TrackConfig {
            codec: "theora".to_owned(),
            description: None,
        },
        timescale: 10,
        width: 2,
        height: 2,
        frame_rate: Rational::new(10, 1),
        color: Color::bt709_limited(),
        alpha: false,
    };
    let error = codec
        .muxer()
        .unwrap()
        .open_muxer(&MuxerRequest {
            tracks: &[theora],
            ..request(&Settings::new())
        })
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        MediaError::Codec("webm does not take theora".into()).to_string()
    );
    assert!(!output.exists());
}

#[test]
fn prores_4444_keeps_alpha_and_hq_does_not() {
    let mut codec = FfmpegCodec::new();
    let offered = [
        PixelFormat::Yuv420p,
        PixelFormat::Rgba16,
        PixelFormat::Rgba8,
    ];
    for (profile, alpha, family) in [("4444", true, "ap4h"), ("hq", false, "apch")] {
        let opened = open(
            &mut codec,
            "prores",
            &settings(&[("profile", text(profile))]),
            &offered,
        )
        .unwrap();
        assert_eq!(opened.input, PixelFormat::Rgba16);
        assert_eq!(opened.alpha, alpha, "{profile}");
        let (packets, config) = encode_all(&mut codec, opened);
        assert_eq!(config.codec, family);
        assert_eq!(config.description, None);
        assert_eq!(packets.len(), FRAMES as usize);
        assert!(packets.iter().all(|packet| packet.key));
    }
}

#[test]
fn vp9_alpha_rides_as_block_additions() {
    let mut codec = FfmpegCodec::new();
    let offered = [PixelFormat::Rgba16, PixelFormat::Rgba8];
    for alpha in [true, false] {
        let opened = open(
            &mut codec,
            "vp9",
            &settings(&[("alpha", SettingValue::Boolean(alpha))]),
            &offered,
        )
        .unwrap();
        assert_eq!(opened.input, PixelFormat::Rgba16);
        assert_eq!(opened.alpha, alpha);
        let (packets, config) = encode_all(&mut codec, opened);
        assert_eq!(config.codec, "vp09.00.10.08");
        assert_eq!(config.description, None);
        assert!(!packets.is_empty());
        for packet in &packets {
            if alpha {
                assert_eq!(packet.additional.len(), 1, "{packet:?}");
                assert_eq!(packet.additional[0].id, 1);
                assert!(!packet.additional[0].bytes.is_empty());
            } else {
                assert!(packet.additional.is_empty());
            }
        }
    }
}

/// The ids `plugin.json` declares under `key`.
fn manifest_ids(key: &str) -> BTreeSet<String> {
    let manifest: serde_json::Value = serde_json::from_str(include_str!("../plugin.json")).unwrap();
    manifest["media"][key]
        .as_array()
        .unwrap_or_else(|| panic!("plugin.json declares {key}"))
        .iter()
        .map(|entry| entry["id"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn describe_names_every_manifest_component() {
    let described = FfmpegCodec::new().describe().unwrap();
    for (kind, key) in [
        (ComponentKind::Decoder, "decoders"),
        (ComponentKind::VideoEncoder, "videoEncoders"),
        (ComponentKind::AudioEncoder, "audioEncoders"),
        (ComponentKind::Muxer, "muxers"),
    ] {
        let ids: BTreeSet<String> = described
            .iter()
            .filter(|component| component.kind == kind)
            .map(|component| component.id.clone())
            .collect();
        assert_eq!(ids, manifest_ids(key), "{key}");
    }
    let libx264 = described
        .iter()
        .find(|component| {
            component.kind == ComponentKind::VideoEncoder && component.id == "libx264"
        })
        .unwrap();
    assert!(libx264.available, "{:?}", libx264.reason);
    assert_eq!(libx264.transports, ["pipe"]);
    assert_eq!(
        libx264.input_formats,
        [
            PixelFormat::Yuv420p,
            PixelFormat::Yuv420p10,
            PixelFormat::Rgba16,
            PixelFormat::Rgba8
        ]
    );
    let keys: Vec<&str> = libx264
        .settings
        .iter()
        .map(|field| field.key.as_str())
        .collect();
    assert_eq!(
        keys,
        [
            "preset",
            "rateControl",
            "crf",
            "bitrate",
            "bitDepth",
            "tune"
        ]
    );
    let audio = |id: &str| {
        described
            .iter()
            .find(|component| component.kind == ComponentKind::AudioEncoder && component.id == id)
            .unwrap()
    };
    for id in ["aac", "opus", "pcm"] {
        assert!(audio(id).available, "{id}: {:?}", audio(id).reason);
        assert_eq!(audio(id).channels, [1, 2], "{id}");
    }
    assert_eq!(
        audio("opus").sample_rates,
        Some(SampleRates::Only(vec![48_000]))
    );
    assert_eq!(audio("pcm").sample_rates, Some(SampleRates::Any));
    let Some(SampleRates::Only(rates)) = &audio("aac").sample_rates else {
        panic!("aac lists its rates");
    };
    assert!(
        rates.contains(&44_100) && rates.contains(&8_000),
        "{rates:?}"
    );
    let muxer = |id: &str| {
        described
            .iter()
            .find(|component| component.kind == ComponentKind::Muxer && component.id == id)
            .unwrap()
    };
    for id in ["mp4", "mov"] {
        assert!(muxer(id).available, "{id}");
        let keys: Vec<&str> = muxer(id)
            .settings
            .iter()
            .map(|field| field.key.as_str())
            .collect();
        assert_eq!(keys, ["faststart"], "{id}");
    }
    assert!(muxer("webm").available);
    assert!(muxer("webm").settings.is_empty());
}
