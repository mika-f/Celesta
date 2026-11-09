//! The VideoToolbox encoders, in this process: the input each chooses, the
//! packets and track config they give, their decode timestamps, and what
//! FFmpeg decodes of them (libavcodec for the planes, the FFmpeg plugin's
//! muxers and decoder for files).
#![cfg(target_os = "macos")]

use celesta_codec_avfoundation::AvfCodec;
use celesta_codec_ffmpeg::FfmpegCodec;
use celesta_composition::Rational;
use celesta_media::{
    Color, ComponentKind, Encoded, MediaError, MediaProvider, MuxerRequest, Packet, PixelFormat,
    SessionId, SettingValue, Settings, Track, TrackConfig, TrackPacket, VideoEncoderOpened,
    VideoEncoderRequest, VideoRequest,
};
use ffmpeg_sys_next::{
    AVColorPrimaries, AVColorRange, AVColorSpace, AVColorTransferCharacteristic, AVERROR,
    AVERROR_EOF, AVPixelFormat, EAGAIN, FF_COMPLIANCE_STRICT, av_frame_alloc, av_frame_free,
    av_malloc, av_packet_alloc, av_packet_free, av_pix_fmt_desc_get, avcodec_alloc_context3,
    avcodec_find_decoder_by_name, avcodec_free_context, avcodec_open2, avcodec_receive_frame,
    avcodec_send_packet,
};
use std::ffi::CString;
use std::path::Path;
use std::ptr::null_mut;

const WIDTH: u32 = 64;
const HEIGHT: u32 = 36;
/// 29.97 fps: a frame lasts 1001 of the 30000 ticks a second.
const RATE: Rational = Rational {
    numerator: 30_000,
    denominator: 1_001,
};
const TIMESCALE: u32 = 30_000;
const FRAME: i64 = 1_001;

/// BT.709 limited-range red in 8 and 10 bits.
const RED_8: [u16; 3] = [63, 102, 240];
const RED_10: [u16; 3] = [250, 409, 960];

fn settings(pairs: &[(&str, SettingValue)]) -> Settings {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect()
}

fn text(value: &str) -> SettingValue {
    SettingValue::String(value.to_owned())
}

/// Whether this Mac's VideoToolbox has `component`; prints why `test` skips
/// when it has not. H.264 has a software encoder on every macOS, so its
/// absence fails.
fn available(codec: &mut AvfCodec, component: &str, test: &str) -> bool {
    let described = codec.describe().unwrap();
    let description = described
        .iter()
        .find(|description| {
            description.kind == ComponentKind::VideoEncoder && description.id == component
        })
        .unwrap_or_else(|| panic!("no video encoder {component}"));
    if description.available {
        return true;
    }
    let reason = description.reason.as_deref().unwrap_or("no reason");
    assert_ne!(component, "h264", "{reason}");
    println!("skipping {test}: {reason}");
    false
}

/// Opens `component` at 64x36 and 29.97 fps, BT.709 limited.
fn open(
    codec: &mut AvfCodec,
    component: &str,
    given: &Settings,
    offered: &[PixelFormat],
) -> Result<VideoEncoderOpened, MediaError> {
    codec
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

/// `samples` as 16-bit little-endian bytes.
fn little_endian(samples: impl IntoIterator<Item = u16>) -> Vec<u8> {
    samples
        .into_iter()
        .flat_map(|sample| sample.to_le_bytes())
        .collect()
}

/// A Y plane of `y` and an interleaved chroma plane of `cb`, `cr`: NV12's
/// samples, or P010's before they are shifted and widened.
fn semi_planar(y: impl Fn(u32, u32) -> u16, cb: u16, cr: u16) -> Vec<u16> {
    let luma = (0..HEIGHT).flat_map(|row| (0..WIDTH).map(move |column| (column, row)));
    let chroma = [cb, cr].repeat((WIDTH / 2 * HEIGHT / 2) as usize);
    luma.map(|(column, row)| y(column, row))
        .chain(chroma)
        .collect()
}

/// An NV12 frame of one colour, in 8-bit codes.
fn nv12_solid([y, cb, cr]: [u16; 3]) -> Vec<u8> {
    semi_planar(|_, _| y, cb, cr)
        .into_iter()
        .map(|code| code as u8)
        .collect()
}

/// An NV12 frame whose luma moves with `index`, so the encoder predicts
/// across frames.
fn nv12_moving(index: i64) -> Vec<u8> {
    let step = index as u32 * 5;
    semi_planar(
        move |column, row| (16 + (column * 3 + row * 2 + step) % 200) as u16,
        96 + (index as u16 % 16),
        160,
    )
    .into_iter()
    .map(|code| code as u8)
    .collect()
}

/// A P010 frame of one colour, in 10-bit codes.
fn p010_solid([y, cb, cr]: [u16; 3]) -> Vec<u8> {
    little_endian(
        semi_planar(|_, _| y, cb, cr)
            .into_iter()
            .map(|code| code << 6),
    )
}

/// An RGBA16 frame: `left` in the left half and `right` in the right, each
/// in 16-bit channels.
fn rgba16_halves(left: [u16; 4], right: [u16; 4]) -> Vec<u8> {
    little_endian((0..HEIGHT).flat_map(|_| {
        (0..WIDTH).flat_map(move |column| if column < WIDTH / 2 { left } else { right })
    }))
}

/// `frames` through `session`, shown one frame apart from 0, then its
/// finish: every answer, the finish's last.
fn encode_frames(
    codec: &mut AvfCodec,
    session: SessionId,
    frames: impl IntoIterator<Item = Vec<u8>>,
) -> Vec<Encoded> {
    let encoder = codec.video_encoder().unwrap();
    let mut answers: Vec<Encoded> = frames
        .into_iter()
        .enumerate()
        .map(|(index, pixels)| {
            encoder
                .encode_video(session, index as i64 * FRAME, pixels)
                .unwrap()
        })
        .collect();
    answers.push(encoder.finish_video(session).unwrap());
    answers
}

/// Every packet of `answers`, and the config of the first answer with
/// packets.
fn packets_and_config(answers: Vec<Encoded>) -> (Vec<Packet>, TrackConfig) {
    let config = answers
        .iter()
        .find(|answer| !answer.packets.is_empty())
        .expect("the encoder made packets")
        .config
        .clone()
        .expect("the first packets come with the config");
    let packets = answers
        .into_iter()
        .flat_map(|answer| answer.packets)
        .collect();
    (packets, config)
}

/// A frame libavcodec decoded: its colour tags, and each plane's samples,
/// tightly packed, 16 bits or fewer each.
#[derive(Debug)]
struct Decoded {
    size: (i32, i32),
    colorspace: AVColorSpace,
    primaries: AVColorPrimaries,
    transfer: AVColorTransferCharacteristic,
    range: AVColorRange,
    planes: Vec<Vec<u16>>,
}

impl Decoded {
    /// The first sample of plane `plane` (0 Y, 1 Cb, 2 Cr) more than
    /// `tolerance` from `code`, if any.
    fn off(&self, plane: usize, code: u16, tolerance: u16) -> Option<u16> {
        self.planes[plane]
            .iter()
            .copied()
            .find(|sample| sample.abs_diff(code) > tolerance)
    }

    /// Panics unless every Y, Cb, and Cr sample is within `tolerance` of
    /// `codes`.
    fn assert_codes(&self, codes: [u16; 3], tolerance: u16) {
        for (plane, code) in codes.into_iter().enumerate() {
            assert_eq!(
                self.off(plane, code, tolerance),
                None,
                "plane {plane} is not within {tolerance} of {code}"
            );
        }
    }

    fn assert_bt709_limited(&self) {
        assert_eq!(
            (self.colorspace, self.primaries, self.transfer, self.range),
            (
                AVColorSpace::AVCOL_SPC_BT709,
                AVColorPrimaries::AVCOL_PRI_BT709,
                AVColorTransferCharacteristic::AVCOL_TRC_BT709,
                AVColorRange::AVCOL_RANGE_MPEG
            )
        );
    }
}

/// Decodes `packets` with libavcodec's decoder `name`, given the track's
/// `extradata` (its avcC or hvcC): every frame it gives, in presentation
/// order. Planar YUV is split into its 4:2:0 planes; a semi-planar frame is
/// not expected.
///
/// VideoToolbox's H.264 SPS has no `bitstream_restriction`, so libavcodec
/// would guess how many frames it reorders, and drop one when it finds out;
/// strict compliance has it assume the most the level allows instead (as a
/// demuxer tells it from an MP4's composition offsets).
fn decode(name: &str, extradata: Option<&[u8]>, packets: &[Packet]) -> Vec<Decoded> {
    let mut frames = Vec::new();
    let name = CString::new(name).unwrap();
    // SAFETY: every pointer comes from the allocator it is freed with; the
    // extradata carries libavcodec's input padding, and libavcodec copies
    // the packets' data, which is not reference counted, into padded
    // buffers of its own. A frame's planes hold `linesize` bytes a row for
    // the rows its format's descriptor gives.
    unsafe {
        let decoder = avcodec_find_decoder_by_name(name.as_ptr());
        assert!(!decoder.is_null(), "no decoder {name:?}");
        let mut context = avcodec_alloc_context3(decoder);
        (*context).strict_std_compliance = FF_COMPLIANCE_STRICT;
        if let Some(bytes) = extradata {
            let padding = 64;
            let copy = av_malloc(bytes.len() + padding).cast::<u8>();
            std::ptr::write_bytes(copy, 0, bytes.len() + padding);
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), copy, bytes.len());
            (*context).extradata = copy;
            (*context).extradata_size = bytes.len() as i32;
        }
        assert_eq!(avcodec_open2(context, decoder, null_mut()), 0);
        let mut packet = av_packet_alloc();
        let mut frame = av_frame_alloc();
        let receive = |frames: &mut Vec<Decoded>| loop {
            let result = avcodec_receive_frame(context, frame);
            if result == AVERROR(EAGAIN) || result == AVERROR_EOF {
                break;
            }
            assert_eq!(result, 0, "the decoder failed");
            let (width, height) = ((*frame).width, (*frame).height);
            let format: AVPixelFormat = std::mem::transmute((*frame).format);
            let descriptor = av_pix_fmt_desc_get(format);
            let wide = (*descriptor).comp[0].depth > 8;
            let mut planes = Vec::new();
            for plane in 0..4 {
                let data = (*frame).data[plane];
                if data.is_null() {
                    break;
                }
                let (columns, rows) = if plane == 1 || plane == 2 {
                    (
                        -((-width) >> (*descriptor).log2_chroma_w),
                        -((-height) >> (*descriptor).log2_chroma_h),
                    )
                } else {
                    (width, height)
                };
                let stride = (*frame).linesize[plane] as isize;
                let mut samples = Vec::new();
                for row in 0..rows as isize {
                    let start = data.offset(row * stride);
                    for column in 0..columns as usize {
                        samples.push(if wide {
                            u16::from_le_bytes([*start.add(column * 2), *start.add(column * 2 + 1)])
                        } else {
                            u16::from(*start.add(column))
                        });
                    }
                }
                planes.push(samples);
            }
            frames.push(Decoded {
                size: (width, height),
                colorspace: (*frame).colorspace,
                primaries: (*frame).color_primaries,
                transfer: (*frame).color_trc,
                range: (*frame).color_range,
                planes,
            });
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
            receive(&mut frames);
        }
        assert_eq!(avcodec_send_packet(context, null_mut()), 0);
        receive(&mut frames);
        av_frame_free(&mut frame);
        av_packet_free(&mut packet);
        avcodec_free_context(&mut context);
    }
    frames
}

/// Writes `packets` of a 64x36 video track of `config` to `output` with the
/// FFmpeg plugin's muxer `component`.
fn mux(component: &str, output: &Path, config: &TrackConfig, alpha: bool, packets: &[Packet]) {
    let mut ffmpeg = FfmpegCodec::new();
    let muxer = ffmpeg.muxer().unwrap();
    let track = Track::Video {
        config: config.clone(),
        timescale: TIMESCALE,
        width: WIDTH,
        height: HEIGHT,
        frame_rate: RATE,
        color: Color::bt709_limited(),
        alpha,
    };
    let session = muxer
        .open_muxer(&MuxerRequest {
            component,
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

/// The four bytes after the first `colr` in `path` (an ISO BMFF colour
/// box's type: `nclx` in MP4).
fn colr_atom(path: &Path) -> Option<[u8; 4]> {
    let bytes = std::fs::read(path).unwrap();
    let at = bytes.windows(4).position(|window| window == b"colr")?;
    bytes.get(at + 4..at + 8)?.try_into().ok()
}

#[test]
fn h264_from_nv12_keeps_bt709_codes() {
    let mut codec = AvfCodec::default();
    let opened = open(
        &mut codec,
        "h264",
        &Settings::new(),
        &PixelFormat::OFFERABLE,
    )
    .unwrap();
    assert_eq!(opened.input, PixelFormat::Nv12);
    assert!(!opened.alpha);
    let frames = (0..10).map(|_| nv12_solid(RED_8));
    let (packets, config) = packets_and_config(encode_frames(&mut codec, opened.session, frames));
    assert!(config.codec.starts_with("avc1.64"), "{}", config.codec);
    let avcc = config.description.as_deref().expect("an avcC");
    assert_eq!(avcc[0], 1, "an avcC's configurationVersion");
    let decoded = decode("h264", Some(avcc), &packets);
    assert_eq!(decoded.len(), 10);
    for frame in &decoded {
        assert_eq!(frame.size, (WIDTH as i32, HEIGHT as i32));
        frame.assert_codes(RED_8, 2);
        frame.assert_bt709_limited();
    }

    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("clip.mp4");
    mux("mp4", &output, &config, false, &packets);
    assert_eq!(colr_atom(&output), Some(*b"nclx"));
}

#[test]
fn hevc_10_bit_from_p010_is_main10() {
    let mut codec = AvfCodec::default();
    if !available(&mut codec, "hevc", "hevc_10_bit_from_p010_is_main10") {
        return;
    }
    let ten = settings(&[("bitDepth", text("10"))]);
    let opened = open(&mut codec, "hevc", &ten, &PixelFormat::OFFERABLE).unwrap();
    assert_eq!(opened.input, PixelFormat::P010);
    assert!(!opened.alpha);
    let frames = (0..10).map(|_| p010_solid(RED_10));
    let (packets, config) = packets_and_config(encode_frames(&mut codec, opened.session, frames));
    assert!(config.codec.starts_with("hvc1.2."), "{}", config.codec);
    let hvcc = config.description.as_deref().expect("an hvcC");
    let decoded = decode("hevc", Some(hvcc), &packets);
    assert_eq!(decoded.len(), 10);
    for frame in &decoded {
        frame.assert_codes(RED_10, 4);
        frame.assert_bt709_limited();
    }
}

#[test]
fn prores_takes_rgba16_little_endian() {
    let mut codec = AvfCodec::default();
    if !available(&mut codec, "prores", "prores_takes_rgba16_little_endian") {
        return;
    }
    let profile = |name| settings(&[("profile", text(name))]);
    let opened = open(
        &mut codec,
        "prores",
        &profile("4444"),
        &PixelFormat::OFFERABLE,
    )
    .unwrap();
    assert_eq!(opened.input, PixelFormat::Rgba16);
    assert!(opened.alpha);
    let red = [65_535, 0, 0, 65_535];
    let half_green = [0, 65_535, 0, 32_768];
    let frames = (0..10).map(|_| rgba16_halves(red, half_green));
    let (packets, config) = packets_and_config(encode_frames(&mut codec, opened.session, frames));
    assert_eq!(
        config,
        TrackConfig {
            codec: "ap4h".to_owned(),
            description: None
        }
    );
    assert_eq!(packets.len(), 10);
    assert!(packets.iter().all(|packet| packet.key));

    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("clip.mov");
    mux("mov", &output, &config, true, &packets);
    let mut ffmpeg = FfmpegCodec::new();
    let frame = ffmpeg
        .decoder()
        .unwrap()
        .decode_video(
            celesta_codec_ffmpeg::DECODER_ID,
            &VideoRequest {
                path: &output,
                time: 0.0,
                sequential: None,
            },
        )
        .unwrap();
    assert_eq!((frame.width, frame.height), (WIDTH, HEIGHT));
    let pixel = |column: u32, row: u32| -> [u8; 4] {
        let start = ((row * WIDTH + column) * 4) as usize;
        frame.pixels[start..start + 4].try_into().unwrap()
    };
    let near = |actual: [u8; 4], expected: [u8; 4]| {
        actual
            .iter()
            .zip(expected)
            .all(|(actual, expected)| actual.abs_diff(expected) <= 2)
    };
    for row in [0, HEIGHT / 2, HEIGHT - 1] {
        for (column, expected) in [(4, [255, 0, 0, 255]), (WIDTH - 5, [0, 255, 0, 128])] {
            let actual = pixel(column, row);
            assert!(near(actual, expected), "({column}, {row}): {actual:?}");
        }
    }

    let opened = open(
        &mut codec,
        "prores",
        &profile("hq"),
        &PixelFormat::OFFERABLE,
    )
    .unwrap();
    assert_eq!(opened.input, PixelFormat::Rgba16);
    assert!(!opened.alpha);
    let frames = (0..3).map(|_| rgba16_halves(red, half_green));
    let (packets, config) = packets_and_config(encode_frames(&mut codec, opened.session, frames));
    assert_eq!(
        config,
        TrackConfig {
            codec: "apch".to_owned(),
            description: None
        }
    );
    assert_eq!(packets.len(), 3);
}

/// Panics unless `packets` are `count` frames shown one frame apart from 0,
/// the first a keyframe, decoded in strictly increasing order and never
/// after they are shown.
fn assert_decodable(component: &str, packets: &[Packet], count: i64) {
    assert_eq!(packets.len(), count as usize, "{component}");
    assert!(
        packets[0].key,
        "{component}: the first packet is a keyframe"
    );
    for packet in packets {
        assert!(packet.dts <= packet.pts, "{component}: {packet:?}");
        assert_eq!(packet.duration, FRAME, "{component}");
    }
    assert!(
        packets.windows(2).all(|pair| pair[0].dts < pair[1].dts),
        "{component}: {:?}",
        packets.iter().map(|packet| packet.dts).collect::<Vec<_>>()
    );
    let mut shown: Vec<i64> = packets.iter().map(|packet| packet.pts).collect();
    shown.sort_unstable();
    assert_eq!(
        shown,
        (0..count).map(|index| index * FRAME).collect::<Vec<_>>(),
        "{component}"
    );
}

#[test]
fn h264_and_hevc_decode_times_precede_presentation() {
    let mut codec = AvfCodec::default();
    for component in ["h264", "hevc"] {
        if !available(
            &mut codec,
            component,
            "h264_and_hevc_decode_times_precede_presentation",
        ) {
            continue;
        }
        let opened = open(
            &mut codec,
            component,
            &Settings::new(),
            &[PixelFormat::Nv12],
        )
        .unwrap();
        let answers = encode_frames(&mut codec, opened.session, (0..30).map(nv12_moving));
        let (packets, _) = packets_and_config(answers);
        assert_decodable(component, &packets, 30);
        assert!(packets[0].dts < 0, "{component}: {:?}", packets[0]);
        assert!(
            packets.iter().any(|packet| packet.dts != packet.pts),
            "{component} reorders by default"
        );

        let no_b_frames = settings(&[("bFrames", SettingValue::Boolean(false))]);
        let opened = open(&mut codec, component, &no_b_frames, &[PixelFormat::Nv12]).unwrap();
        let answers = encode_frames(&mut codec, opened.session, (0..30).map(nv12_moving));
        let (packets, _) = packets_and_config(answers);
        assert_decodable(component, &packets, 30);
        for packet in &packets {
            assert_eq!(packet.dts, packet.pts, "{component} without B-frames");
        }
    }
}

#[test]
fn packets_come_with_the_config() {
    let mut codec = AvfCodec::default();
    for (component, input) in [
        ("h264", PixelFormat::Nv12),
        ("hevc", PixelFormat::Nv12),
        ("prores", PixelFormat::Rgba16),
    ] {
        if !available(&mut codec, component, "packets_come_with_the_config") {
            continue;
        }
        let opened = open(&mut codec, component, &Settings::new(), &[input]).unwrap();
        let frames = (0..12).map(|index| match input {
            PixelFormat::Nv12 => nv12_moving(index),
            _ => rgba16_halves([65_535, 0, 0, 65_535], [0, 0, 65_535, 65_535]),
        });
        let answers = encode_frames(&mut codec, opened.session, frames);
        let first = answers
            .iter()
            .position(|answer| !answer.packets.is_empty())
            .expect("packets");
        for (index, answer) in answers.iter().enumerate() {
            assert_eq!(
                answer.config.is_some(),
                index == first,
                "{component}: answer {index} of {}",
                answers.len()
            );
        }
    }
}

#[test]
fn rgba16_input_is_converted_with_bt709() {
    let mut codec = AvfCodec::default();
    let opened = open(
        &mut codec,
        "h264",
        &Settings::new(),
        &[PixelFormat::Rgba16, PixelFormat::Rgba8],
    )
    .unwrap();
    assert_eq!(opened.input, PixelFormat::Rgba16);
    let red = [65_535, 0, 0, 65_535];
    let frames = (0..5).map(|_| rgba16_halves(red, red));
    let (packets, config) = packets_and_config(encode_frames(&mut codec, opened.session, frames));
    let decoded = decode("h264", config.description.as_deref(), &packets);
    assert_eq!(decoded.len(), 5);
    for frame in &decoded {
        frame.assert_codes(RED_8, 2);
    }
}

#[test]
fn a_frame_of_the_wrong_size_is_refused() {
    let mut codec = AvfCodec::default();
    let opened = open(&mut codec, "h264", &Settings::new(), &[PixelFormat::Nv12]).unwrap();
    let encoder = codec.video_encoder().unwrap();
    let short = PixelFormat::Nv12.frame_bytes(WIDTH, HEIGHT - 2);
    assert!(matches!(
        encoder.encode_video(opened.session, 0, vec![16; short]),
        Err(MediaError::Codec(_))
    ));
    let answers = [
        encoder
            .encode_video(opened.session, 0, nv12_solid(RED_8))
            .unwrap(),
        encoder.finish_video(opened.session).unwrap(),
    ];
    let (packets, _) = packets_and_config(answers.to_vec());
    assert_eq!(packets.len(), 1);
    assert!(packets[0].key);
    assert!(encoder.finish_video(opened.session).is_err());
}
