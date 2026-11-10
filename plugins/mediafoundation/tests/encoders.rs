//! The H.264 and HEVC encoders on Media Foundation transforms, in this
//! process: the input each chooses, the packets and track config they give,
//! their timestamps, and what FFmpeg decodes of them (the FFmpeg plugin's
//! `mp4` muxer writes a file, which libavformat and libavcodec read back).
//!
//! Tests that compare decoded values or bitstream structure pin
//! Microsoft's software MFTs (`hardware: false`); the hardware ones check
//! the hard invariants and colour within 16 codes.
#![cfg(windows)]

use celesta_codec_ffmpeg::FfmpegCodec;
use celesta_codec_mediafoundation::MfCodec;
use celesta_composition::Rational;
use celesta_media::{
    Color, ComponentKind, Encoded, MediaError, MediaProvider, MuxerRequest, Packet, PixelFormat,
    SessionId, SettingValue, Settings, Track, TrackConfig, TrackPacket, VideoEncoderOpened,
    VideoEncoderRequest,
};
use ffmpeg_sys_next as ff;
use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};

/// The size every test but the hardware one encodes at.
const SIZE: (u32, u32) = (64, 36);
/// The hardware tests' size: NVIDIA's H.264 MFT refuses 64x36 (Task 1),
/// and its HEVC one too, with the same `MF_E_UNSUPPORTED_D3D_TYPE`.
const HARDWARE_SIZE: (u32, u32) = (160, 90);
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

/// The names of Microsoft's software MFTs, which `hardware: true` falls
/// back to when no hardware one accepts.
const SOFTWARE_MFTS: [&str; 2] = ["H264 Encoder MFT", "HEVCVideoExtensionEncoder"];

fn settings(pairs: &[(&str, SettingValue)]) -> Settings {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect()
}

fn text(value: &str) -> SettingValue {
    SettingValue::String(value.to_owned())
}

/// `pairs` with `hardware: false`, which pins Microsoft's software MFT.
fn software(pairs: &[(&str, SettingValue)]) -> Settings {
    let mut given = settings(pairs);
    given.insert("hardware".to_owned(), SettingValue::Boolean(false));
    given
}

/// Whether this system's Media Foundation describes `component`; prints why
/// `test` skips when it does not. H.264 has a software encoder on every
/// Windows 10 and 11, so its absence fails.
fn available(codec: &mut MfCodec, component: &str, test: &str) -> bool {
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

/// Opens `component` at `(width, height)` and 29.97 fps, BT.709 limited.
fn open_sized(
    codec: &mut MfCodec,
    component: &str,
    given: &Settings,
    offered: &[PixelFormat],
    (width, height): (u32, u32),
) -> Result<VideoEncoderOpened, MediaError> {
    codec
        .video_encoder()
        .expect("the plugin has video encoders")
        .open_video(&VideoEncoderRequest {
            component,
            settings: given,
            width,
            height,
            frame_rate: RATE,
            timescale: TIMESCALE,
            offered,
            color: &Color::bt709_limited(),
        })
}

/// [`open_sized`] at 64x36.
fn open(
    codec: &mut MfCodec,
    component: &str,
    given: &Settings,
    offered: &[PixelFormat],
) -> Result<VideoEncoderOpened, MediaError> {
    open_sized(codec, component, given, offered, SIZE)
}

/// [`open`], or None after printing why `test` skips when the open answers
/// that the component is unavailable (no software HEVC encoder, say).
/// H.264 always opens.
fn open_or_skip(
    codec: &mut MfCodec,
    component: &str,
    given: &Settings,
    offered: &[PixelFormat],
    size: (u32, u32),
    test: &str,
) -> Option<VideoEncoderOpened> {
    match open_sized(codec, component, given, offered, size) {
        Ok(opened) => Some(opened),
        Err(MediaError::ComponentUnavailable { reason, .. }) if component != "h264" => {
            println!("skipping {test}: {reason}");
            None
        }
        Err(error) => panic!("{component} did not open: {error:?}"),
    }
}

/// `samples` as 16-bit little-endian bytes.
fn little_endian(samples: impl IntoIterator<Item = u16>) -> Vec<u8> {
    samples
        .into_iter()
        .flat_map(|sample| sample.to_le_bytes())
        .collect()
}

/// A Y plane of `y` and an interleaved chroma plane of `cb`, `cr` at
/// `(width, height)`: NV12's samples, or P010's before they are shifted and
/// widened.
fn semi_planar(
    (width, height): (u32, u32),
    y: impl Fn(u32, u32) -> u16,
    cb: u16,
    cr: u16,
) -> Vec<u16> {
    let luma = (0..height).flat_map(|row| (0..width).map(move |column| (column, row)));
    let chroma = [cb, cr].repeat((width / 2 * height / 2) as usize);
    luma.map(|(column, row)| y(column, row))
        .chain(chroma)
        .collect()
}

/// An NV12 frame of one colour, in 8-bit codes.
fn nv12_solid(size: (u32, u32), [y, cb, cr]: [u16; 3]) -> Vec<u8> {
    semi_planar(size, |_, _| y, cb, cr)
        .into_iter()
        .map(|code| code as u8)
        .collect()
}

/// An NV12 frame whose luma moves with `index`, so the encoder predicts
/// across frames.
fn nv12_moving(size: (u32, u32), index: i64) -> Vec<u8> {
    let step = index as u32 * 5;
    semi_planar(
        size,
        move |column, row| (16 + (column * 3 + row * 2 + step) % 200) as u16,
        96 + (index as u16 % 16),
        160,
    )
    .into_iter()
    .map(|code| code as u8)
    .collect()
}

/// A P010 frame of one colour, in 10-bit codes.
fn p010_solid(size: (u32, u32), [y, cb, cr]: [u16; 3]) -> Vec<u8> {
    little_endian(
        semi_planar(size, |_, _| y, cb, cr)
            .into_iter()
            .map(|code| code << 6),
    )
}

/// An opaque RGBA8 frame of one colour.
fn rgba8_solid((width, height): (u32, u32), rgba: [u8; 4]) -> Vec<u8> {
    rgba.repeat((width * height) as usize)
}

/// `frames` through `session`, shown one frame apart from 0, then its
/// finish: every answer, the finish's last.
fn encode_frames(
    codec: &mut MfCodec,
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

/// Panics unless `packets` keep the hard invariants for `count` frames
/// shown one frame apart from 0: as many packets, the first a keyframe,
/// decode times never after presentation and strictly increasing, and every
/// input time answered.
fn assert_invariants(what: &str, packets: &[Packet], count: i64) {
    assert_eq!(packets.len(), count as usize, "{what}");
    assert!(packets[0].key, "{what}: the first packet is a keyframe");
    for packet in packets {
        assert!(packet.dts <= packet.pts, "{what}: {packet:?}");
        assert_eq!(packet.duration, FRAME, "{what}");
    }
    assert!(
        packets.windows(2).all(|pair| pair[0].dts < pair[1].dts),
        "{what}: {:?}",
        packets.iter().map(|packet| packet.dts).collect::<Vec<_>>()
    );
    let mut shown: Vec<i64> = packets.iter().map(|packet| packet.pts).collect();
    shown.sort_unstable();
    assert_eq!(
        shown,
        (0..count).map(|index| index * FRAME).collect::<Vec<_>>(),
        "{what}"
    );
}

/// Whether some packet decodes at another time than it is shown.
fn reorders(packets: &[Packet]) -> bool {
    packets.iter().any(|packet| packet.dts != packet.pts)
}

/// Writes `packets` of a video track of `config` at `(width, height)` with
/// the FFmpeg plugin's `mp4` into `directory`: the file.
fn mux_mp4(
    directory: &Path,
    config: &TrackConfig,
    (width, height): (u32, u32),
    packets: &[Packet],
) -> PathBuf {
    let output = directory.join("clip.mp4");
    let mut ffmpeg = FfmpegCodec::new();
    let muxer = ffmpeg.muxer().unwrap();
    let track = Track::Video {
        config: config.clone(),
        timescale: TIMESCALE,
        width,
        height,
        frame_rate: RATE,
        color: Color::bt709_limited(),
        alpha: false,
    };
    let session = muxer
        .open_muxer(&MuxerRequest {
            component: "mp4",
            settings: &Settings::new(),
            output: &output,
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
    output
}

/// The colour an MP4's video stream is tagged with, as libavformat reads it.
#[derive(Debug, PartialEq, Eq)]
struct Tags {
    colorspace: ff::AVColorSpace,
    primaries: ff::AVColorPrimaries,
    transfer: ff::AVColorTransferCharacteristic,
    range: ff::AVColorRange,
}

const BT709_LIMITED: Tags = Tags {
    colorspace: ff::AVColorSpace::AVCOL_SPC_BT709,
    primaries: ff::AVColorPrimaries::AVCOL_PRI_BT709,
    transfer: ff::AVColorTransferCharacteristic::AVCOL_TRC_BT709,
    range: ff::AVColorRange::AVCOL_RANGE_MPEG,
};

/// A frame libavcodec decoded: its size, and each plane's samples, tightly
/// packed, 16 bits or fewer each.
#[derive(Debug)]
struct Decoded {
    size: (u32, u32),
    planes: Vec<Vec<u16>>,
}

impl Decoded {
    /// The largest distance of plane `plane`'s samples (0 Y, 1 Cb, 2 Cr)
    /// from `code`.
    fn farthest(&self, plane: usize, code: u16) -> u16 {
        self.planes[plane]
            .iter()
            .map(|sample| sample.abs_diff(code))
            .max()
            .unwrap_or(0)
    }
}

/// Panics unless every frame of `frames` is of `size`, with every Y, Cb,
/// and Cr sample within `tolerance` of `codes`.
fn assert_codes(what: &str, frames: &[Decoded], size: (u32, u32), codes: [u16; 3], tolerance: u16) {
    for (index, frame) in frames.iter().enumerate() {
        assert_eq!(frame.size, size, "{what}: frame {index}");
        let farthest = [0, 1, 2].map(|plane| frame.farthest(plane, codes[plane]));
        assert!(
            farthest.iter().all(|&distance| distance <= tolerance),
            "{what}: frame {index}'s Y, Cb, Cr are up to {farthest:?} from {codes:?}, more than {tolerance}"
        );
    }
}

/// Reads `path`'s first video stream with libavformat and decodes it with
/// libavcodec: the stream's colour tags, and every frame, in presentation
/// order. Planar YUV is split into its planes.
fn decode_mp4(path: &Path) -> (Tags, Vec<Decoded>) {
    let url = CString::new(path.to_string_lossy().as_bytes()).unwrap();
    let mut frames = Vec::new();
    // SAFETY: every pointer comes from the allocator it is freed with; the
    // decoder is opened from the stream's parameters; a frame's planes hold
    // `linesize` bytes a row for the rows its format's descriptor gives.
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
        let tags = Tags {
            colorspace: (*parameters).color_space,
            primaries: (*parameters).color_primaries,
            transfer: (*parameters).color_trc,
            range: (*parameters).color_range,
        };
        let decoder = ff::avcodec_find_decoder((*parameters).codec_id);
        assert!(!decoder.is_null(), "no decoder");
        let mut context = ff::avcodec_alloc_context3(decoder);
        assert!(ff::avcodec_parameters_to_context(context, parameters) >= 0);
        assert_eq!(ff::avcodec_open2(context, decoder, null_mut()), 0);
        let mut packet = ff::av_packet_alloc();
        let mut frame = ff::av_frame_alloc();
        let receive = |frames: &mut Vec<Decoded>| loop {
            let result = ff::avcodec_receive_frame(context, frame);
            if result == ff::AVERROR(ff::EAGAIN) || result == ff::AVERROR_EOF {
                break;
            }
            assert_eq!(result, 0, "the decoder failed");
            let (width, height) = ((*frame).width, (*frame).height);
            let pixel_format: ff::AVPixelFormat = std::mem::transmute((*frame).format);
            let descriptor = ff::av_pix_fmt_desc_get(pixel_format);
            let wide = (*descriptor).comp[0].depth > 8;
            let mut planes = Vec::new();
            for plane in 0..3 {
                let data = (*frame).data[plane];
                if data.is_null() {
                    break;
                }
                let (columns, rows) = if plane > 0 {
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
                size: (width as u32, height as u32),
                planes,
            });
        };
        while ff::av_read_frame(format, packet) >= 0 {
            if (*packet).stream_index == index as i32 {
                assert_eq!(ff::avcodec_send_packet(context, packet), 0);
                receive(&mut frames);
            }
            ff::av_packet_unref(packet);
        }
        assert_eq!(ff::avcodec_send_packet(context, null_mut()), 0);
        receive(&mut frames);
        ff::av_frame_free(&mut frame);
        ff::av_packet_free(&mut packet);
        ff::avcodec_free_context(&mut context);
        ff::avformat_close_input(&mut format);
        (tags, frames)
    }
}

/// `packets` of `config` at `size` through the FFmpeg plugin's `mp4` and
/// back through libavcodec.
fn through_mp4(config: &TrackConfig, size: (u32, u32), packets: &[Packet]) -> (Tags, Vec<Decoded>) {
    let directory = tempfile::tempdir().unwrap();
    let output = mux_mp4(directory.path(), config, size, packets);
    decode_mp4(&output)
}

#[test]
fn h264_from_nv12_keeps_bt709_codes() {
    let mut codec = MfCodec::new();
    let opened = open(&mut codec, "h264", &software(&[]), &PixelFormat::OFFERABLE).unwrap();
    assert_eq!(opened.input, PixelFormat::Nv12);
    assert!(!opened.alpha);
    let frames = (0..10).map(|_| nv12_solid(SIZE, RED_8));
    let (packets, config) = packets_and_config(encode_frames(&mut codec, opened.session, frames));
    assert!(config.codec.starts_with("avc1.64"), "{}", config.codec);
    let avcc = config.description.as_deref().expect("an avcC");
    assert_eq!(avcc[0], 1, "an avcC's configurationVersion");
    let (tags, decoded) = through_mp4(&config, SIZE, &packets);
    assert_eq!(tags, BT709_LIMITED);
    assert_eq!(decoded.len(), 10);
    assert_codes("h264", &decoded, SIZE, RED_8, 2);
}

#[test]
fn rgba8_input_is_converted_with_bt709() {
    let mut codec = MfCodec::new();
    let opened = open(
        &mut codec,
        "h264",
        &software(&[]),
        &[PixelFormat::Rgba16, PixelFormat::Rgba8],
    )
    .unwrap();
    assert_eq!(opened.input, PixelFormat::Rgba8);
    assert!(!opened.alpha);
    let frames = (0..5).map(|_| rgba8_solid(SIZE, [255, 0, 0, 255]));
    let (packets, config) = packets_and_config(encode_frames(&mut codec, opened.session, frames));
    let (tags, decoded) = through_mp4(&config, SIZE, &packets);
    assert_eq!(tags, BT709_LIMITED);
    assert_eq!(decoded.len(), 5);
    assert_codes("h264 from rgba8", &decoded, SIZE, RED_8, 2);
}

#[test]
fn h264_and_hevc_decode_times_precede_presentation() {
    const TEST: &str = "h264_and_hevc_decode_times_precede_presentation";
    let mut codec = MfCodec::new();
    for component in ["h264", "hevc"] {
        if !available(&mut codec, component, TEST) {
            continue;
        }
        let Some(opened) = open_or_skip(
            &mut codec,
            component,
            &software(&[]),
            &[PixelFormat::Nv12],
            SIZE,
            TEST,
        ) else {
            continue;
        };
        let answers = encode_frames(
            &mut codec,
            opened.session,
            (0..30).map(|index| nv12_moving(SIZE, index)),
        );
        let (packets, config) = packets_and_config(answers);
        assert_invariants(component, &packets, 30);
        if reorders(&packets) {
            // Decode times are shifted back by the reorder depth, so the
            // first frame decodes before it is shown.
            assert!(packets[0].dts < 0, "{component}: {:?}", packets[0]);
            assert_eq!(packets[0].pts, 0, "{component}");
        } else {
            println!(
                "skipping {TEST} ({component}'s reordering checks): the encoder did not reorder"
            );
        }
        let (_, decoded) = through_mp4(&config, SIZE, &packets);
        assert_eq!(decoded.len(), 30, "{component}");

        let no_b_frames = software(&[("bFrames", SettingValue::Boolean(false))]);
        let opened = open(&mut codec, component, &no_b_frames, &[PixelFormat::Nv12]).unwrap();
        let answers = encode_frames(
            &mut codec,
            opened.session,
            (0..30).map(|index| nv12_moving(SIZE, index)),
        );
        let (packets, _) = packets_and_config(answers);
        assert_invariants(component, &packets, 30);
        for packet in &packets {
            assert_eq!(packet.dts, packet.pts, "{component} without B-frames");
        }
    }
}

#[test]
fn every_input_pts_comes_back() {
    let mut codec = MfCodec::new();
    let opened = open(&mut codec, "h264", &software(&[]), &[PixelFormat::Nv12]).unwrap();
    let answers = encode_frames(
        &mut codec,
        opened.session,
        (0..300).map(|index| nv12_moving(SIZE, index)),
    );
    let (packets, _) = packets_and_config(answers);
    assert_eq!(packets.len(), 300);
    let mut shown: Vec<i64> = packets.iter().map(|packet| packet.pts).collect();
    shown.sort_unstable();
    assert_eq!(shown, (0..300).map(|k| k * 1001).collect::<Vec<_>>());
    assert_invariants("h264", &packets, 300);
}

#[test]
fn hevc_10_bit_from_p010_is_main10() {
    const TEST: &str = "hevc_10_bit_from_p010_is_main10";
    let mut codec = MfCodec::new();
    if !available(&mut codec, "hevc", TEST) {
        return;
    }
    // No software MFT takes P010 (Task 1), so this is the GPU's encoder, at
    // the hardware size: NVIDIA's HEVC MFT, like its H.264 one, refuses
    // 64x36.
    let ten = settings(&[
        ("bitDepth", text("10")),
        ("hardware", SettingValue::Boolean(true)),
    ]);
    let opened = match open_sized(
        &mut codec,
        "hevc",
        &ten,
        &PixelFormat::OFFERABLE,
        HARDWARE_SIZE,
    ) {
        Ok(opened) => opened,
        Err(error @ MediaError::ComponentUnavailable { .. }) => {
            println!("skipping {TEST}: {error}");
            return;
        }
        Err(error) => panic!("10-bit HEVC did not open: {error:?}"),
    };
    assert_eq!(opened.input, PixelFormat::P010);
    assert!(!opened.alpha);
    let frames = (0..10).map(|_| p010_solid(HARDWARE_SIZE, RED_10));
    let (packets, config) = packets_and_config(encode_frames(&mut codec, opened.session, frames));
    assert_invariants("hevc 10-bit", &packets, 10);
    assert!(config.codec.starts_with("hvc1.2."), "{}", config.codec);
    let (tags, decoded) = through_mp4(&config, HARDWARE_SIZE, &packets);
    assert_eq!(tags, BT709_LIMITED);
    assert_eq!(decoded.len(), 10);
    // The hardware tolerance: 16 10-bit codes.
    assert_codes("hevc 10-bit", &decoded, HARDWARE_SIZE, RED_10, 16);
}

#[test]
fn packets_come_with_the_config() {
    const TEST: &str = "packets_come_with_the_config";
    let mut codec = MfCodec::new();
    for component in ["h264", "hevc"] {
        if !available(&mut codec, component, TEST) {
            continue;
        }
        let Some(opened) = open_or_skip(
            &mut codec,
            component,
            &software(&[]),
            &[PixelFormat::Nv12],
            SIZE,
            TEST,
        ) else {
            continue;
        };
        let frames = (0..12).map(|index| nv12_moving(SIZE, index));
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
fn odd_sizes_are_refused() {
    let mut codec = MfCodec::new();
    let offers: [&[PixelFormat]; 2] = [
        &PixelFormat::OFFERABLE,
        &[PixelFormat::Rgba16, PixelFormat::Rgba8],
    ];
    for component in ["h264", "hevc"] {
        for offered in offers {
            for given in [software(&[]), Settings::new()] {
                match open_sized(&mut codec, component, &given, offered, (63, 35)) {
                    Err(MediaError::Codec(message)) => {
                        assert!(message.contains("even dimensions"), "{message}");
                    }
                    other => panic!("{component} opened 63x35: {:?}", other.map(|_| ())),
                }
            }
        }
    }
}

#[test]
fn a_frame_of_the_wrong_size_is_refused() {
    let mut codec = MfCodec::new();
    let opened = open(&mut codec, "h264", &software(&[]), &[PixelFormat::Nv12]).unwrap();
    let encoder = codec.video_encoder().unwrap();
    let short = PixelFormat::Nv12.frame_bytes(SIZE.0, SIZE.1 - 2);
    assert!(matches!(
        encoder.encode_video(opened.session, 0, vec![16; short]),
        Err(MediaError::Codec(_))
    ));
    let answers = vec![
        encoder
            .encode_video(opened.session, 0, nv12_solid(SIZE, RED_8))
            .unwrap(),
        encoder.finish_video(opened.session).unwrap(),
    ];
    let (packets, _) = packets_and_config(answers);
    assert_eq!(packets.len(), 1);
    assert!(packets[0].key);
    assert!(encoder.finish_video(opened.session).is_err());

    // An RGBA8 frame is checked before it is converted.
    let opened = open(&mut codec, "h264", &software(&[]), &[PixelFormat::Rgba8]).unwrap();
    let encoder = codec.video_encoder().unwrap();
    assert!(matches!(
        encoder.encode_video(opened.session, 0, vec![0; 64 * 35 * 4]),
        Err(MediaError::Codec(_))
    ));
    encoder
        .encode_video(opened.session, 0, rgba8_solid(SIZE, [255, 0, 0, 255]))
        .unwrap();
    encoder.finish_video(opened.session).unwrap();
}

#[test]
fn hardware_encoders_keep_the_invariants() {
    const TEST: &str = "hardware_encoders_keep_the_invariants";
    let mut codec = MfCodec::new();
    for component in ["h264", "hevc"] {
        if !available(&mut codec, component, TEST) {
            continue;
        }
        let hardware = settings(&[("hardware", SettingValue::Boolean(true))]);
        let opened = open_sized(
            &mut codec,
            component,
            &hardware,
            &[PixelFormat::Nv12],
            HARDWARE_SIZE,
        )
        .unwrap();
        let name = codec
            .encoder_name(opened.session)
            .expect("an open session has an encoder")
            .to_owned();
        if SOFTWARE_MFTS.contains(&name.as_str()) {
            println!(
                "skipping {TEST} ({component}): the encoder that opened is {name}, Microsoft's software MFT"
            );
            codec.video_encoder().unwrap().abort_video(opened.session);
            continue;
        }
        let answers = encode_frames(
            &mut codec,
            opened.session,
            (0..30).map(|index| nv12_moving(HARDWARE_SIZE, index)),
        );
        let (packets, _) = packets_and_config(answers);
        assert_invariants(&name, &packets, 30);
        if reorders(&packets) {
            assert!(packets[0].dts < 0, "{name}: {:?}", packets[0]);
        } else {
            println!("skipping {TEST} ({name}'s reordering checks): the encoder did not reorder");
        }

        let opened = open_sized(
            &mut codec,
            component,
            &hardware,
            &[PixelFormat::Nv12],
            HARDWARE_SIZE,
        )
        .unwrap();
        let frames = (0..10).map(|_| nv12_solid(HARDWARE_SIZE, RED_8));
        let (packets, config) =
            packets_and_config(encode_frames(&mut codec, opened.session, frames));
        assert_invariants(&name, &packets, 10);
        let (tags, decoded) = through_mp4(&config, HARDWARE_SIZE, &packets);
        assert_eq!(tags, BT709_LIMITED);
        assert_eq!(decoded.len(), 10);
        assert_codes(&name, &decoded, HARDWARE_SIZE, RED_8, 16);
    }
}
