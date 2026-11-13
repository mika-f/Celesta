//! The video encoder helpers both FFmpeg plugins' encoder tests share:
//! opening a component at 32x16 and 10 fps, frames of every input format,
//! encoding them, and decoding the packets back with libavcodec.
#![allow(dead_code)]

use celesta_composition::Rational;
use celesta_media::{
    Color, Encoded, MediaError, MediaProvider, Packet, PixelFormat, SessionId, SettingValue,
    Settings, TrackConfig, VideoEncoderOpened, VideoEncoderRequest,
};
use ffmpeg_sys_next::{
    AVColorPrimaries, AVColorRange, AVColorSpace, AVColorTransferCharacteristic, AVERROR,
    AVERROR_EOF, AVPixelFormat, EAGAIN, av_frame_alloc, av_frame_free, av_malloc, av_packet_alloc,
    av_packet_free, av_pix_fmt_desc_get, avcodec_alloc_context3, avcodec_find_decoder_by_name,
    avcodec_free_context, avcodec_open2, avcodec_receive_frame, avcodec_send_packet,
};
use std::collections::BTreeSet;
use std::ffi::CString;
use std::ptr::null_mut;

pub const WIDTH: u32 = 32;
pub const HEIGHT: u32 = 16;
pub const FRAMES: i64 = 10;

pub fn settings(pairs: &[(&str, SettingValue)]) -> Settings {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect()
}

pub fn text(value: &str) -> SettingValue {
    SettingValue::String(value.to_owned())
}

/// Opens `component` at 32x16 and 10 fps.
pub fn open(
    codec: &mut dyn MediaProvider,
    component: &str,
    given: &Settings,
    offered: &[PixelFormat],
) -> Result<VideoEncoderOpened, MediaError> {
    open_colored(codec, component, given, offered, &Color::bt709_limited())
}

/// [`open`] with `color`.
pub fn open_colored(
    codec: &mut dyn MediaProvider,
    component: &str,
    given: &Settings,
    offered: &[PixelFormat],
    color: &Color,
) -> Result<VideoEncoderOpened, MediaError> {
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
            color,
        })
}

/// The Y, U, and V planes of frame `index` at 32x16, in 8-bit codes or, with
/// `ten`, 10-bit ones: each moves with `index` and along its rows and
/// columns, and U and V differ, so a mixed-up plane or chroma order shows.
pub fn planes(index: i64, ten: bool) -> [Vec<u16>; 3] {
    let (width, height) = (WIDTH as u16, HEIGHT as u16);
    let step = index as u16 * 20;
    let plane = |width: u16, height: u16, sample: &dyn Fn(u16, u16) -> u16| {
        (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .map(|(x, y)| {
                let code = sample(x, y);
                if ten { code * 4 + (x + y) % 4 } else { code }
            })
            .collect::<Vec<u16>>()
    };
    [
        plane(width, height, &|x, y| 16 + step / 2 + x * 4 + y),
        plane(width / 2, height / 2, &|x, y| 64 + step / 4 + x * 3 + y * 2),
        plane(width / 2, height / 2, &|x, y| 192 - step / 4 - x * 3 - y),
    ]
}

/// `samples` as 16-bit little-endian bytes.
pub fn little_endian(samples: impl IntoIterator<Item = u16>) -> Vec<u8> {
    samples
        .into_iter()
        .flat_map(|sample| sample.to_le_bytes())
        .collect()
}

/// U and V interleaved, as NV12 and P010 store chroma.
pub fn interleaved(u: &[u16], v: &[u16]) -> Vec<u16> {
    u.iter().zip(v).flat_map(|(u, v)| [*u, *v]).collect()
}

/// One frame of `format` at 32x16 whose colors move with `index`; the YUV
/// formats hold the same [`planes`].
pub fn frame(format: PixelFormat, index: i64) -> Vec<u8> {
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
        PixelFormat::Yuv420p => planes(index, false)
            .concat()
            .iter()
            .map(|&code| code as u8)
            .collect(),
        PixelFormat::Nv12 => {
            let [y, u, v] = planes(index, false);
            [y, interleaved(&u, &v)]
                .concat()
                .iter()
                .map(|&code| code as u8)
                .collect()
        }
        PixelFormat::Yuv420p10 => little_endian(planes(index, true).concat()),
        PixelFormat::P010 => {
            let [y, u, v] = planes(index, true);
            little_endian(
                [y, interleaved(&u, &v)]
                    .concat()
                    .iter()
                    .map(|code| code << 6),
            )
        }
    }
}

/// A frame of RGBA `format` of one color, `rgb` in 8 bits.
pub fn solid(format: PixelFormat, rgb: [u8; 3]) -> Vec<u8> {
    let [r, g, b] = rgb;
    let pixels = (WIDTH * HEIGHT) as usize;
    match format {
        PixelFormat::Rgba8 => [r, g, b, 255].repeat(pixels),
        PixelFormat::Rgba16 => {
            little_endian([r, g, b, 255].map(|channel| u16::from(channel) * 257)).repeat(pixels)
        }
        other => panic!("no solid frames of {other}"),
    }
}

/// Ten frames through `opened`'s session and its finish: every packet, and
/// the config of the first answer with packets.
pub fn encode_all(
    codec: &mut dyn MediaProvider,
    opened: VideoEncoderOpened,
) -> (Vec<Packet>, TrackConfig) {
    let frames = (0..FRAMES).map(|index| frame(opened.input, index));
    encode_frames(codec, opened.session, frames)
}

/// `frames` through `session` and its finish: every packet, and the config
/// of the first answer with packets.
pub fn encode_frames(
    codec: &mut dyn MediaProvider,
    session: SessionId,
    frames: impl IntoIterator<Item = Vec<u8>>,
) -> (Vec<Packet>, TrackConfig) {
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
    for (index, pixels) in frames.into_iter().enumerate() {
        let answer = encoder.encode_video(session, index as i64, pixels).unwrap();
        take(answer, &mut packets);
    }
    let answer = encoder.finish_video(session).unwrap();
    take(answer, &mut packets);
    (packets, config.expect("the encoder made packets"))
}

/// A frame a decoder gave: its size, color tags, and samples.
#[derive(Debug, PartialEq)]
pub struct Decoded {
    pub size: (i32, i32),
    pub colorspace: AVColorSpace,
    pub primaries: AVColorPrimaries,
    pub transfer: AVColorTransferCharacteristic,
    pub range: AVColorRange,
    /// Each plane's samples, tightly packed, 16 bits or fewer each.
    pub planes: Vec<Vec<u16>>,
}

impl Decoded {
    /// The distinct samples of each of the first three planes.
    pub fn plane_values(&self) -> Vec<BTreeSet<u16>> {
        self.planes[..3]
            .iter()
            .map(|plane| plane.iter().copied().collect())
            .collect()
    }
}

/// Decodes `packets` with libavcodec's decoder `name`, given `extradata`
/// (an avcC for H.264): every frame it gives.
pub fn decode(name: &str, extradata: Option<&[u8]>, packets: &[Packet]) -> Vec<Decoded> {
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
