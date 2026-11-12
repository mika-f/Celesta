//! The x264 plugin's libx264 encoder, in this process and linked against
//! the GPL build: the input it chooses of those offered, the packets and
//! track config it gives, its colors, and what `describe` says of it. The
//! standard plugin's encoders and muxers are tested in `plugins/ffmpeg`,
//! against the standard build.

#[path = "../../ffmpeg/tests/support/encoding.rs"]
mod encoding;
use celesta_codec_ffmpeg::FfmpegCodec;
use celesta_composition::Rational;
use celesta_media::{
    Color, ComponentKind, MediaError, MediaProvider, Packet, PixelFormat, SettingValue, Settings,
    TrackConfig, VideoEncoderRequest,
};
use encoding::{
    Decoded, FRAMES, HEIGHT, WIDTH, decode, encode_all, encode_frames, frame, open, open_colored,
    settings, solid, text,
};
use ffmpeg_sys_next::{
    AVColorPrimaries, AVColorRange, AVColorSpace, AVColorTransferCharacteristic,
};
use std::collections::BTreeSet;

#[test]
fn libx264_packets_carry_an_avcc_and_a_codec_string() {
    let mut codec = FfmpegCodec::x264();
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
    let mut codec = FfmpegCodec::x264();
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

/// [`decode`] with libavcodec's H.264 decoder, given the track's avcC.
fn decode_h264(config: &TrackConfig, packets: &[Packet]) -> Vec<Decoded> {
    decode("h264", config.description.as_deref(), packets)
}

#[test]
fn rgba_input_is_converted() {
    let mut codec = FfmpegCodec::x264();
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
    let sizes: Vec<(i32, i32)> = decode_h264(&config, &packets)
        .iter()
        .map(|frame| frame.size)
        .collect();
    assert_eq!(sizes, vec![(WIDTH as i32, HEIGHT as i32); FRAMES as usize]);
}

#[test]
fn software_offers_pick_rgba_by_bit_depth() {
    let offered = [PixelFormat::Rgba16, PixelFormat::Rgba8];
    let mut codec = FfmpegCodec::x264();
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
    // Either RGBA serves either bit depth when the other is not offered.
    for (depth, offered) in [("8", PixelFormat::Rgba16), ("10", PixelFormat::Rgba8)] {
        let opened = open(
            &mut codec,
            "libx264",
            &settings(&[("bitDepth", text(depth))]),
            &[offered],
        )
        .unwrap();
        assert_eq!(opened.input, offered, "{depth}-bit");
    }

    let refused = |error: MediaError| match error {
        MediaError::InvalidSettings { key, message } => (key, message),
        other => panic!("expected invalid settings, got {other:?}"),
    };
    let error = open(
        &mut codec,
        "libx264",
        &Settings::new(),
        &[PixelFormat::Yuv420p10, PixelFormat::P010],
    )
    .unwrap_err();
    assert_eq!(
        refused(error),
        (
            "input".to_owned(),
            "needs yuv420p, nv12, rgba8, or rgba16".to_owned()
        )
    );
    let error = open(
        &mut codec,
        "libx264",
        &settings(&[("bitDepth", text("10"))]),
        &[PixelFormat::Yuv420p, PixelFormat::Nv12],
    )
    .unwrap_err();
    assert_eq!(
        refused(error),
        (
            "input".to_owned(),
            "needs yuv420p10, p010, rgba16, or rgba8".to_owned()
        )
    );
}

const RED: [u8; 3] = [255, 0, 0];
const GREEN: [u8; 3] = [0, 255, 0];
const BLUE: [u8; 3] = [0, 0, 255];

#[test]
fn libx264_writes_bt709_vui() {
    let mut codec = FfmpegCodec::x264();
    let opened = open(
        &mut codec,
        "libx264",
        &settings(&[("preset", text("ultrafast"))]),
        &[PixelFormat::Yuv420p],
    )
    .unwrap();
    let (packets, config) = encode_all(&mut codec, opened);
    let frames = decode_h264(&config, &packets);
    assert_eq!(frames.len(), FRAMES as usize);
    for frame in &frames {
        assert_eq!(
            (
                frame.colorspace,
                frame.primaries,
                frame.transfer,
                frame.range
            ),
            (
                AVColorSpace::AVCOL_SPC_BT709,
                AVColorPrimaries::AVCOL_PRI_BT709,
                AVColorTransferCharacteristic::AVCOL_TRC_BT709,
                AVColorRange::AVCOL_RANGE_MPEG
            )
        );
    }
}

#[test]
fn rgba_input_is_converted_with_bt709() {
    let mut codec = FfmpegCodec::x264();
    // libswscale's BT.709 limited-range codes of red, green, and blue, as
    // measured on arm64 (Apple M4); every decoded sample is within 1 of
    // them, since x86_64's SIMD paths may round differently. At 10 bits,
    // from 16-bit RGB, its fixed point gives these: within 2 of the
    // formula's (250, 409, 960), (691, 167, 105), (127, 960, 471), and far
    // from BT.601's (red's Y would be about 326).
    for (format, depth, expected) in [
        (
            PixelFormat::Rgba8,
            "8",
            [[63, 102, 240], [173, 42, 26], [32, 240, 118]],
        ),
        (
            PixelFormat::Rgba16,
            "10",
            [[251, 409, 962], [693, 165, 103], [128, 962, 471]],
        ),
    ] {
        let opened = open(
            &mut codec,
            "libx264",
            &settings(&[
                ("preset", text("ultrafast")),
                ("crf", SettingValue::Number(0.0)),
                ("bitDepth", text(depth)),
            ]),
            &[format],
        )
        .unwrap();
        assert_eq!(opened.input, format);
        let frames = [RED, GREEN, BLUE].map(|rgb| solid(format, rgb));
        let (packets, config) = encode_frames(&mut codec, opened.session, frames);
        let decoded = decode_h264(&config, &packets);
        assert_eq!(decoded.len(), 3, "{format}");
        for (frame, codes) in decoded.iter().zip(expected) {
            let planes = frame.plane_values();
            assert_eq!(planes.len(), codes.len(), "{format}");
            for (values, code) in planes.iter().zip(codes) {
                assert!(
                    values.iter().all(|value| value.abs_diff(code) <= 1),
                    "{format}: {values:?}, expected {code}"
                );
            }
        }
    }
}

#[test]
fn libx264_takes_nv12_and_p010() {
    let mut codec = FfmpegCodec::x264();
    for (depth, planar, interleaved) in [
        ("8", PixelFormat::Yuv420p, PixelFormat::Nv12),
        ("10", PixelFormat::Yuv420p10, PixelFormat::P010),
    ] {
        let given = settings(&[("preset", text("ultrafast")), ("bitDepth", text(depth))]);
        let mut run = |offered: PixelFormat| {
            let opened = open(&mut codec, "libx264", &given, &[offered]).unwrap();
            assert_eq!(opened.input, offered);
            let (packets, config) = encode_all(&mut codec, opened);
            (decode_h264(&config, &packets), config.codec)
        };
        let (expected, _) = run(planar);
        let (decoded, family) = run(interleaved);
        assert_eq!(decoded.len(), FRAMES as usize, "{interleaved}");
        assert!(decoded == expected, "{interleaved} decodes unlike {planar}");
        if depth == "10" {
            assert!(family.starts_with("avc1.6E"), "{family}");
        }
    }
}

#[test]
fn an_unknown_color_name_is_refused() {
    let mut codec = FfmpegCodec::x264();
    for (color, expected) in [
        (
            Color {
                matrix: "nonsense".into(),
                ..Color::bt709_limited()
            },
            "unknown color matrix `nonsense`",
        ),
        (
            Color {
                range: "wide".into(),
                ..Color::bt709_limited()
            },
            "unknown color range `wide`",
        ),
    ] {
        let result = open_colored(
            &mut codec,
            "libx264",
            &Settings::new(),
            &[PixelFormat::Yuv420p],
            &color,
        );
        match result {
            Err(MediaError::Codec(message)) => {
                assert!(message.contains(expected), "libx264: {message}");
            }
            other => panic!("libx264 took {color:?}: {:?}", other.map(|_| ())),
        }
    }
}

#[test]
fn an_rgb_matrix_libswscale_cannot_convert_to_is_refused() {
    let mut codec = FfmpegCodec::x264();
    let ycgco = Color {
        matrix: "ycgco".into(),
        ..Color::bt709_limited()
    };
    let result = open_colored(
        &mut codec,
        "libx264",
        &Settings::new(),
        &[PixelFormat::Rgba8],
        &ycgco,
    );
    match result {
        Err(MediaError::Codec(message)) => assert_eq!(
            message,
            "libswscale cannot convert RGB to the `ycgco` matrix"
        ),
        other => panic!("libx264 took ycgco: {:?}", other.map(|_| ())),
    }
    // YUV input needs no conversion, so the matrix is only a tag.
    let opened = open_colored(
        &mut codec,
        "libx264",
        &Settings::new(),
        &[PixelFormat::Yuv420p],
        &ycgco,
    )
    .unwrap();
    codec.video_encoder().unwrap().abort_video(opened.session);
}

#[test]
fn describe_lists_six_inputs_for_libx264() {
    let described = FfmpegCodec::x264().describe().unwrap();
    let libx264 = described
        .iter()
        .find(|component| {
            component.kind == ComponentKind::VideoEncoder && component.id == "libx264"
        })
        .unwrap();
    let six = [
        PixelFormat::Yuv420p,
        PixelFormat::Yuv420p10,
        PixelFormat::Nv12,
        PixelFormat::P010,
        PixelFormat::Rgba8,
        PixelFormat::Rgba16,
    ];
    assert_eq!(libx264.input_formats, six);
    let manifest: serde_json::Value = serde_json::from_str(include_str!("../plugin.json")).unwrap();
    let entry = manifest["media"]["videoEncoders"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == "libx264")
        .unwrap();
    let listed: Vec<PixelFormat> = serde_json::from_value(entry["inputFormats"].clone()).unwrap();
    assert_eq!(listed, six);
}

/// The ids `plugin.json` declares under `key`; the x264 plugin declares
/// only video encoders.
fn manifest_ids(key: &str) -> BTreeSet<String> {
    let manifest: serde_json::Value = serde_json::from_str(include_str!("../plugin.json")).unwrap();
    manifest["media"][key]
        .as_array()
        .into_iter()
        .flatten()
        .map(|entry| entry["id"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn describe_names_every_manifest_component() {
    let described = FfmpegCodec::x264().describe().unwrap();
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
    for encoder in described
        .iter()
        .filter(|component| component.kind == ComponentKind::VideoEncoder && component.available)
    {
        assert_eq!(
            encoder.transports,
            ["sharedMemory", "pipe"],
            "{}",
            encoder.id
        );
    }
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
}

#[test]
fn x264_ntsc_packets_use_the_requested_timescale() {
    let mut codec = FfmpegCodec::x264();
    let opened = codec
        .video_encoder()
        .unwrap()
        .open_video(&VideoEncoderRequest {
            component: "libx264",
            settings: &Settings::new(),
            width: WIDTH,
            height: HEIGHT,
            frame_rate: Rational::new(30_000, 1001),
            timescale: 30_000,
            offered: &[PixelFormat::Yuv420p],
            color: &Color::bt709_limited(),
        })
        .unwrap();
    let encoder = codec.video_encoder().unwrap();
    let mut packets = Vec::new();
    for index in 0..10 {
        packets.extend(
            encoder
                .encode_video(
                    opened.session,
                    index * 1001,
                    frame(PixelFormat::Yuv420p, index),
                )
                .unwrap()
                .packets,
        );
    }
    packets.extend(encoder.finish_video(opened.session).unwrap().packets);
    let mut times: Vec<_> = packets.iter().map(|packet| packet.pts).collect();
    times.sort_unstable();
    assert_eq!(times, (0..10).map(|index| index * 1001).collect::<Vec<_>>());
    assert!(
        packets
            .iter()
            .all(|packet| packet.duration == 1001 && packet.dts % 1001 == 0)
    );
}

#[test]
fn an_unknown_x264_tune_is_refused_before_encoding() {
    let mut codec = FfmpegCodec::x264();
    let result = open(
        &mut codec,
        "libx264",
        &settings(&[("tune", text("cinema"))]),
        &[PixelFormat::Rgba8],
    );
    assert!(matches!(result, Err(MediaError::InvalidSettings { key, .. }) if key == "tune"));
}

#[test]
fn the_gpl_build_fails_the_standard_distribution_check() {
    let error = celesta_codec_ffmpeg::check_lgpl().unwrap_err();
    assert!(
        error.contains("ffmpeg distribution requires LGPL-only"),
        "{error}"
    );
    assert!(error.contains("avcodec: GPL version"), "{error}");
}
