//! The video encoders on libavcodec: libx264, `prores_ks`, and libvpx's
//! VP9, each fed frames in the pixel format it chose, converting RGBA with
//! libswscale, and answering packets as MP4, MOV, and WebM store them.
//!
//! The conversions and options are those of `main`'s `ffmpeg` pipelines,
//! so the encoded video is the same (deviation 8): libx264 takes YUV as it
//! is and converts RGBA with libswscale's default BT.601 matrix, untagged;
//! ProRes and VP9 convert with BT.709 in limited range and are tagged so.

use crate::components::{VideoCodec, availability};
use crate::ffi::{
    CodecContext, Frame, PacketBuf, Scaler, av_error, dimension, extradata, quiet_logs,
};
use crate::h264::{annexb_to_avc, avcc_codec_string, avcc_from_annexb};
use celesta_media::{
    Encoded, InvalidSetting, MediaError, Packet, PixelFormat, SettingValue, Settings, SideData,
    TrackConfig, VideoEncoderRequest, settings::validate,
};
use ffmpeg_sys_next::{
    AV_CODEC_FLAG_GLOBAL_HEADER, AV_NOPTS_VALUE, AV_PKT_FLAG_KEY, AVColorPrimaries, AVColorRange,
    AVColorSpace, AVColorTransferCharacteristic, AVDictionary, AVERROR, AVERROR_EOF, AVFrame,
    AVPacketSideDataType, AVPixelFormat, AVRational, EAGAIN, SwsFlags, av_dict_count, av_dict_free,
    av_dict_set, av_frame_is_writable, av_image_copy, av_image_fill_arrays,
    av_image_get_buffer_size, av_packet_get_side_data, av_packet_unref, av_rescale_q,
    avcodec_find_encoder_by_name, avcodec_open2, avcodec_receive_packet, avcodec_send_frame,
};
use std::ffi::{CString, c_int};
use std::ptr::{null, null_mut};

/// An open video encoder.
pub(crate) struct VideoSession {
    context: CodecContext,
    /// The frames handed to the encoder, in the encoder's pixel format: one
    /// more whenever the encoder still holds every one (a frame-threaded
    /// encoder such as `prores_ks` keeps those it has not finished), each
    /// reused once it lets go, as libavfilter's pool reused them for
    /// `main`'s encoders.
    frames: Vec<Frame>,
    format: AVPixelFormat,
    packet: PacketBuf,
    /// Converts RGBA input to the encoder's format; none for YUV input,
    /// which is the encoder's format already.
    scaler: Option<Scaler>,
    input: AVPixelFormat,
    input_bytes: usize,
    width: c_int,
    height: c_int,
    /// The encoder's time base, one frame (the frame rate inverted), as
    /// `main`'s encoders had it: libx264 writes it into the SPS's timing.
    frame_base: AVRational,
    /// The track's time base, one over its timescale, which frames come in
    /// and packets go out in.
    track_base: AVRational,
    /// Whether frames are tagged BT.709 limited.
    bt709: bool,
    /// Whether packets are H.264 Annex B, which is answered length-prefixed.
    annexb: bool,
    /// The track's config, until the first packets carry it.
    config: Option<TrackConfig>,
}

/// What [`open`] chose: the session, the input format, and whether the
/// video keeps alpha.
pub(crate) struct Opened {
    pub(crate) session: VideoSession,
    pub(crate) input: PixelFormat,
    pub(crate) alpha: bool,
}

/// How a component encodes for its settings.
struct Plan {
    input: PixelFormat,
    /// The encoder's pixel format.
    format: AVPixelFormat,
    alpha: bool,
    /// Options for `avcodec_open2`.
    options: Vec<(&'static str, String)>,
    bit_rate: Option<i64>,
    bt709: bool,
    /// The codec string; none for H.264, whose string comes from its SPS.
    codec: Option<&'static str>,
}

fn invalid(key: &str, message: &str) -> MediaError {
    MediaError::InvalidSettings {
        key: key.to_owned(),
        message: message.to_owned(),
    }
}

fn text<'a>(settings: &'a Settings, key: &str) -> &'a str {
    match settings.get(key) {
        Some(SettingValue::String(value)) => value,
        _ => "",
    }
}

/// A whole-number setting; none when its field is hidden.
fn whole(settings: &Settings, key: &str) -> Option<i64> {
    match settings.get(key) {
        Some(SettingValue::Number(value)) => Some(*value as i64),
        _ => None,
    }
}

/// The first of `wanted` that is offered, else `message` as the refusal.
fn choose(
    offered: &[PixelFormat],
    wanted: &[PixelFormat],
    message: &str,
) -> Result<PixelFormat, MediaError> {
    wanted
        .iter()
        .copied()
        .find(|format| offered.contains(format))
        .ok_or_else(|| invalid("input", message))
}

fn plan(
    codec: VideoCodec,
    settings: &Settings,
    offered: &[PixelFormat],
) -> Result<Plan, MediaError> {
    let rgba16 = || choose(offered, &[PixelFormat::Rgba16], "needs rgba16");
    Ok(match codec {
        VideoCodec::X264 => {
            // With no YUV offered (a software renderer), 8 bits take RGBA8
            // and 10 bits RGBA16, as `main` read them back.
            let (input, format) = if text(settings, "bitDepth") == "10" {
                (
                    choose(
                        offered,
                        &[PixelFormat::Yuv420p10, PixelFormat::Rgba16],
                        "needs yuv420p10 or rgba16",
                    )?,
                    AVPixelFormat::AV_PIX_FMT_YUV420P10LE,
                )
            } else {
                (
                    choose(
                        offered,
                        &[PixelFormat::Yuv420p, PixelFormat::Rgba8],
                        "needs yuv420p or rgba8",
                    )?,
                    AVPixelFormat::AV_PIX_FMT_YUV420P,
                )
            };
            let mut options = vec![("preset", text(settings, "preset").to_owned())];
            let mut bit_rate = None;
            if text(settings, "rateControl") == "vbr" {
                bit_rate = whole(settings, "bitrate");
            } else if let Some(crf) = whole(settings, "crf") {
                options.push(("crf", crf.to_string()));
            }
            match text(settings, "tune") {
                "" => {}
                tune => options.push(("tune", tune.to_owned())),
            }
            Plan {
                input,
                format,
                alpha: false,
                options,
                bit_rate,
                bt709: false,
                codec: None,
            }
        }
        VideoCodec::ProRes => {
            let input = rgba16()?;
            if text(settings, "profile") == "hq" {
                Plan {
                    input,
                    format: AVPixelFormat::AV_PIX_FMT_YUV422P10LE,
                    alpha: false,
                    options: vec![("profile", "hq".to_owned())],
                    bit_rate: None,
                    bt709: true,
                    codec: Some("apch"),
                }
            } else {
                Plan {
                    input,
                    format: AVPixelFormat::AV_PIX_FMT_YUVA444P10LE,
                    alpha: true,
                    options: vec![
                        ("profile", "4444".to_owned()),
                        ("alpha_bits", "16".to_owned()),
                        ("vendor", "apl0".to_owned()),
                    ],
                    bit_rate: None,
                    bt709: true,
                    codec: Some("ap4h"),
                }
            }
        }
        VideoCodec::Vp9 => {
            let input = rgba16()?;
            let alpha = settings.get("alpha") == Some(&SettingValue::Boolean(true));
            let crf = whole(settings, "crf").unwrap_or(20);
            Plan {
                input,
                format: if alpha {
                    AVPixelFormat::AV_PIX_FMT_YUVA420P
                } else {
                    AVPixelFormat::AV_PIX_FMT_YUV420P
                },
                alpha,
                options: vec![
                    ("crf", crf.to_string()),
                    ("b", "0".to_owned()),
                    ("row-mt", "1".to_owned()),
                ],
                bit_rate: None,
                bt709: true,
                // libvpx reports no level; the WebM muxer derives vpcC itself.
                codec: Some("vp09.00.10.08"),
            }
        }
    })
}

/// The libav pixel format of an input.
fn av_format(format: PixelFormat) -> AVPixelFormat {
    match format {
        PixelFormat::Rgba8 => AVPixelFormat::AV_PIX_FMT_RGBA,
        PixelFormat::Rgba16 => AVPixelFormat::AV_PIX_FMT_RGBA64LE,
        PixelFormat::Yuv420p => AVPixelFormat::AV_PIX_FMT_YUV420P,
        PixelFormat::Yuv420p10 => AVPixelFormat::AV_PIX_FMT_YUV420P10LE,
        PixelFormat::Nv12 => AVPixelFormat::AV_PIX_FMT_NV12,
        PixelFormat::P010 => AVPixelFormat::AV_PIX_FMT_P010LE,
    }
}

fn is_rgba(format: PixelFormat) -> bool {
    matches!(format, PixelFormat::Rgba8 | PixelFormat::Rgba16)
}

/// Opens `codec` for `request`.
pub(crate) fn open(
    codec: VideoCodec,
    request: &VideoEncoderRequest<'_>,
) -> Result<Opened, MediaError> {
    quiet_logs();
    availability(codec.encoder()).map_err(|reason| MediaError::ComponentUnavailable {
        component: codec.id().to_owned(),
        reason,
    })?;
    let settings = validate(&codec.settings(), request.settings)
        .map_err(|InvalidSetting { key, message }| MediaError::InvalidSettings { key, message })?;
    let plan = plan(codec, &settings, request.offered)?;
    let (width, height) = (dimension(request.width)?, dimension(request.height)?);
    let rate = request.frame_rate;
    let (Ok(timescale), Ok(numerator), Ok(denominator)) = (
        c_int::try_from(request.timescale),
        c_int::try_from(rate.numerator),
        c_int::try_from(rate.denominator),
    ) else {
        return Err(MediaError::Codec(format!(
            "frame rate {}/{} in timescale {} is too large",
            rate.numerator, rate.denominator, request.timescale
        )));
    };
    if timescale <= 0 || numerator <= 0 || denominator <= 0 {
        return Err(MediaError::Codec(format!(
            "frame rate {}/{} in timescale {} is not positive",
            rate.numerator, rate.denominator, request.timescale
        )));
    }
    let frame_base = AVRational {
        num: denominator,
        den: numerator,
    };
    let name = CString::new(codec.encoder()).expect("encoder names have no NUL");
    // SAFETY: `name` is NUL-terminated and outlives the call.
    let encoder = unsafe { avcodec_find_encoder_by_name(name.as_ptr()) };
    let context = CodecContext::new(encoder)?;
    let input = av_format(plan.input);
    let options = plan
        .options
        .iter()
        .map(|(key, value)| {
            let text =
                CString::new(value.as_str()).map_err(|_| invalid(key, "must not hold a NUL"))?;
            Ok((CString::new(*key).expect("option names have no NUL"), text))
        })
        .collect::<Result<Vec<_>, MediaError>>()?;
    // SAFETY: `context` is a fresh context of `encoder`, configured before
    // `avcodec_open2`; the dictionary is freed whatever it returns.
    unsafe {
        let raw = context.as_ptr();
        (*raw).width = width;
        (*raw).height = height;
        (*raw).pix_fmt = plan.format;
        (*raw).time_base = frame_base;
        (*raw).framerate = AVRational {
            num: numerator,
            den: denominator,
        };
        (*raw).flags |= AV_CODEC_FLAG_GLOBAL_HEADER as c_int;
        // Automatic threading, as the `ffmpeg` command line has it.
        (*raw).thread_count = 0;
        if let Some(bit_rate) = plan.bit_rate {
            (*raw).bit_rate = bit_rate;
        }
        if plan.bt709 {
            (*raw).color_primaries = AVColorPrimaries::AVCOL_PRI_BT709;
            (*raw).color_trc = AVColorTransferCharacteristic::AVCOL_TRC_BT709;
            (*raw).colorspace = AVColorSpace::AVCOL_SPC_BT709;
            (*raw).color_range = AVColorRange::AVCOL_RANGE_MPEG;
        }
        let mut dictionary: *mut AVDictionary = null_mut();
        for (key, value) in &options {
            av_dict_set(&mut dictionary, key.as_ptr(), value.as_ptr(), 0);
        }
        let result = avcodec_open2(raw, encoder, &mut dictionary);
        let unused = av_dict_count(dictionary);
        av_dict_free(&mut dictionary);
        if result < 0 {
            return Err(av_error(result, &format!("open {}", codec.encoder())));
        }
        if unused > 0 {
            return Err(MediaError::Codec(format!(
                "{} did not take every option given",
                codec.encoder()
            )));
        }
    }
    let config = match plan.codec {
        Some(codec) => TrackConfig {
            codec: codec.to_owned(),
            description: None,
        },
        None => {
            // SAFETY: the context was opened above.
            let avcc = avcc_from_annexb(&unsafe { extradata(context.as_ptr()) })?;
            TrackConfig {
                codec: avcc_codec_string(&avcc),
                description: Some(avcc),
            }
        }
    };
    let scaler = if is_rgba(plan.input) {
        let scaler = Scaler::new(
            request.width,
            request.height,
            input,
            plan.format,
            SwsFlags::SWS_BICUBIC as c_int,
        )?;
        if plan.bt709 {
            scaler.bt709_limited()?;
        }
        Some(scaler)
    } else {
        None
    };
    // SAFETY: a size query on a valid format and dimensions.
    let input_bytes = unsafe { av_image_get_buffer_size(input, width, height, 1) };
    let input_bytes =
        usize::try_from(input_bytes).map_err(|_| av_error(input_bytes, "size the input frames"))?;
    Ok(Opened {
        session: VideoSession {
            context,
            frames: vec![Frame::video(plan.format, request.width, request.height)?],
            format: plan.format,
            packet: PacketBuf::new()?,
            scaler,
            input,
            input_bytes,
            width,
            height,
            frame_base,
            track_base: AVRational {
                num: 1,
                den: timescale,
            },
            bt709: plan.bt709,
            annexb: codec == VideoCodec::X264,
            config: Some(config),
        },
        input: plan.input,
        alpha: plan.alpha,
    })
}

impl VideoSession {
    /// Encodes `pixels`, a tightly packed frame of the session's input, at
    /// `pts`.
    pub(crate) fn encode(&mut self, pts: i64, pixels: &[u8]) -> Result<Encoded, MediaError> {
        if pixels.len() != self.input_bytes {
            return Err(MediaError::Codec(format!(
                "a {}x{} {:?} frame is {} bytes, not {}",
                self.width,
                self.height,
                self.input,
                self.input_bytes,
                pixels.len()
            )));
        }
        let frame = self.writable_frame()?;
        // SAFETY: `pixels` holds a whole frame of `self.input` (checked
        // above), and nothing else references the frame's buffers; the
        // encoder takes its own reference to what it keeps.
        unsafe {
            match &self.scaler {
                Some(scaler) => scaler.convert(frame, pixels)?,
                None => {
                    let mut source = [null_mut::<u8>(); 4];
                    let mut strides = [0 as c_int; 4];
                    let result = av_image_fill_arrays(
                        source.as_mut_ptr(),
                        strides.as_mut_ptr(),
                        pixels.as_ptr(),
                        self.input,
                        self.width,
                        self.height,
                        1,
                    );
                    if result < 0 {
                        return Err(av_error(result, "read the frame"));
                    }
                    av_image_copy(
                        (*frame).data.as_mut_ptr(),
                        (*frame).linesize.as_ptr(),
                        source.as_ptr().cast(),
                        strides.as_ptr(),
                        self.input,
                        self.width,
                        self.height,
                    );
                }
            }
            (*frame).pts = av_rescale_q(pts, self.track_base, self.frame_base);
            (*frame).duration = 1;
            if self.bt709 {
                (*frame).color_primaries = AVColorPrimaries::AVCOL_PRI_BT709;
                (*frame).color_trc = AVColorTransferCharacteristic::AVCOL_TRC_BT709;
                (*frame).colorspace = AVColorSpace::AVCOL_SPC_BT709;
                (*frame).color_range = AVColorRange::AVCOL_RANGE_MPEG;
            }
            let result = avcodec_send_frame(self.context.as_ptr(), frame);
            if result < 0 {
                return Err(av_error(result, "encode a frame"));
            }
        }
        self.receive()
    }

    /// A frame of the session's whose buffers the encoder no longer holds,
    /// a new one if it holds them all.
    fn writable_frame(&mut self) -> Result<*mut AVFrame, MediaError> {
        // SAFETY: each frame has its buffers, allocated by `Frame::video`.
        let free = self
            .frames
            .iter()
            .find(|frame| unsafe { av_frame_is_writable(frame.as_ptr()) } != 0);
        if let Some(frame) = free {
            return Ok(frame.as_ptr());
        }
        let frame = Frame::video(self.format, self.width as u32, self.height as u32)?;
        let raw = frame.as_ptr();
        self.frames.push(frame);
        Ok(raw)
    }

    /// Drains the encoder: its last packets.
    pub(crate) fn finish(&mut self) -> Result<Encoded, MediaError> {
        // SAFETY: a null frame starts the encoder's flush.
        let result = unsafe { avcodec_send_frame(self.context.as_ptr(), null()) };
        if result < 0 {
            return Err(av_error(result, "finish encoding"));
        }
        self.receive()
    }

    /// Every packet the encoder has ready, with the config if it is due.
    fn receive(&mut self) -> Result<Encoded, MediaError> {
        let mut packets = Vec::new();
        let packet = self.packet.as_ptr();
        loop {
            // SAFETY: `packet` is this session's; it is unreferenced once
            // copied out.
            let result = unsafe { avcodec_receive_packet(self.context.as_ptr(), packet) };
            if result == AVERROR(EAGAIN) || result == AVERROR_EOF {
                break;
            }
            if result < 0 {
                return Err(av_error(result, "encode a frame"));
            }
            // SAFETY: as above; the packet holds `size` bytes of data.
            let copied = unsafe { self.copy_packet() };
            unsafe { av_packet_unref(packet) };
            packets.push(copied?);
        }
        let config = if packets.is_empty() {
            None
        } else {
            self.config.take()
        };
        Ok(Encoded { packets, config })
    }

    /// The received packet as a [`Packet`].
    ///
    /// # Safety
    ///
    /// The session's packet holds one received from its encoder.
    unsafe fn copy_packet(&self) -> Result<Packet, MediaError> {
        let packet = self.packet.as_ptr();
        // SAFETY: the caller's packet holds `size` bytes, and its side data
        // `side_size` bytes.
        unsafe {
            let bytes = match usize::try_from((*packet).size) {
                Ok(size) if size > 0 => std::slice::from_raw_parts((*packet).data, size),
                _ => &[],
            };
            let bytes = if self.annexb {
                annexb_to_avc(bytes)
            } else {
                bytes.to_vec()
            };
            let mut side_size = 0;
            let side = av_packet_get_side_data(
                packet,
                AVPacketSideDataType::AV_PKT_DATA_MATROSKA_BLOCKADDITIONAL,
                &mut side_size,
            );
            let mut additional = Vec::new();
            if !side.is_null() {
                let side = std::slice::from_raw_parts(side, side_size);
                let (id, rest) = side.split_at_checked(8).ok_or_else(|| {
                    MediaError::Codec("the encoder gave a BlockAddition with no id".to_owned())
                })?;
                let id = u64::from_be_bytes(id.try_into().expect("eight bytes"));
                let id = u32::try_from(id).map_err(|_| {
                    MediaError::Codec(format!("the encoder gave a BlockAddition id {id}"))
                })?;
                additional.push(SideData {
                    id,
                    bytes: rest.to_vec(),
                });
            }
            let track_time = |time: i64| match time {
                AV_NOPTS_VALUE => time,
                time => av_rescale_q(time, self.frame_base, self.track_base),
            };
            Ok(Packet {
                pts: track_time((*packet).pts),
                dts: track_time((*packet).dts),
                // libx264 and libvpx give none; each packet is a frame.
                duration: track_time(match (*packet).duration {
                    0 => 1,
                    duration => duration,
                }),
                key: (*packet).flags & AV_PKT_FLAG_KEY != 0,
                bytes,
                additional,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_composition::Rational;
    use celesta_media::Color;

    #[test]
    fn ntsc_packets_are_timed_in_the_frame_rates_numerator() {
        let settings: Settings = [("preset".to_owned(), SettingValue::String("medium".into()))]
            .into_iter()
            .collect();
        let color = Color::bt709_limited();
        let mut opened = open(
            VideoCodec::X264,
            &VideoEncoderRequest {
                component: "libx264",
                settings: &settings,
                width: 32,
                height: 16,
                frame_rate: Rational::new(30_000, 1001),
                timescale: 30_000,
                offered: &[PixelFormat::Yuv420p],
                color: &color,
            },
        )
        .unwrap();
        let frame = vec![128; 32 * 16 * 3 / 2];
        let mut packets = Vec::new();
        for index in 0..10 {
            let encoded = opened.session.encode(index * 1001, &frame).unwrap();
            packets.extend(encoded.packets);
        }
        packets.extend(opened.session.finish().unwrap().packets);
        assert_eq!(packets.len(), 10);
        let mut times: Vec<i64> = packets.iter().map(|packet| packet.pts).collect();
        times.sort_unstable();
        assert_eq!(times, (0..10).map(|index| index * 1001).collect::<Vec<_>>());
        for packet in &packets {
            assert_eq!(packet.duration, 1001);
            assert_eq!(packet.dts % 1001, 0, "{packet:?}");
        }
        // libx264 copies each frame in, so one frame serves them all.
        assert_eq!(opened.session.frames.len(), 1);
    }

    #[test]
    fn a_frame_the_encoder_still_holds_is_not_reused() {
        let settings = Settings::new();
        let color = Color::bt709_limited();
        let mut opened = open(
            VideoCodec::ProRes,
            &VideoEncoderRequest {
                component: "prores",
                settings: &settings,
                width: 64,
                height: 32,
                frame_rate: Rational::new(30, 1),
                timescale: 30,
                offered: &[PixelFormat::Rgba16],
                color: &color,
            },
        )
        .unwrap();
        let session = &mut opened.session;
        let first = session.frames[0].as_ptr();
        // A reference such as an encoder's worker takes to a frame it has
        // not finished.
        let held = Frame::video(session.format, 64, 32).unwrap();
        // SAFETY: both frames are valid; `held` is unreferenced before it
        // is dropped.
        unsafe {
            ffmpeg_sys_next::av_frame_unref(held.as_ptr());
            assert_eq!(ffmpeg_sys_next::av_frame_ref(held.as_ptr(), first), 0);
        }
        let other = session.writable_frame().unwrap();
        assert_ne!(other, first);
        assert_eq!(session.frames.len(), 2);
        // SAFETY: as above.
        unsafe { ffmpeg_sys_next::av_frame_unref(held.as_ptr()) };
        assert_eq!(session.writable_frame().unwrap(), first);
        assert_eq!(session.frames.len(), 2);
    }

    #[test]
    fn a_frame_threaded_encoders_frames_are_reused() {
        let settings = Settings::new();
        let color = Color::bt709_limited();
        let mut opened = open(
            VideoCodec::ProRes,
            &VideoEncoderRequest {
                component: "prores",
                settings: &settings,
                width: 64,
                height: 32,
                frame_rate: Rational::new(30, 1),
                timescale: 30,
                offered: &[PixelFormat::Rgba16],
                color: &color,
            },
        )
        .unwrap();
        let frame = vec![0x80; 64 * 32 * 8];
        let mut packets = 0;
        for index in 0..60 {
            packets += opened.session.encode(index, &frame).unwrap().packets.len();
        }
        packets += opened.session.finish().unwrap().packets.len();
        assert_eq!(packets, 60);
        // prores_ks holds the frames its threads have not finished, so the
        // session keeps a few, but reuses them rather than adding one per
        // frame.
        let kept = opened.session.frames.len();
        assert!((1..60).contains(&kept), "{kept} frames for 60");
    }
}
