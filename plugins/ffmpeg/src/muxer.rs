//! The muxers on libavformat: MP4, QuickTime, and WebM, one stream per
//! track, written from the encoders' packets as they are.
//!
//! Negative audio times pass through, as `main`'s `mux_audio` gave them:
//! libavformat writes MP4 and MOV edit lists from them (hiding the encoder's
//! delay and the loop pre-roll alike, so `leadingAudio` needs nothing more),
//! and WebM's `CodecDelay` from the stream's `initial_padding`; an Opus
//! packet shorter than the samples it holds gets the padding to discard, as
//! libopus marked it on `main`. ProRes and VP9 streams
//! are tagged with the track's color; H.264 and HEVC stay untagged in P2a
//! (deviation 8), as `main` left them.

use crate::components::MuxerFormat;
use crate::ffi::{PacketBuf, av_error, quiet_logs};
use celesta_media::{
    Color, InvalidSetting, MediaError, MuxerRequest, SettingValue, Track, TrackConfig, TrackPacket,
    settings::validate,
};
use celesta_plugin_host::codec_family;
use ffmpeg_sys_next::{
    AV_INPUT_BUFFER_PADDING_SIZE, AV_PKT_FLAG_KEY, AVCodecID, AVCodecParameters, AVColorPrimaries,
    AVColorRange, AVColorSpace, AVColorTransferCharacteristic, AVDictionary, AVFMT_NOFILE,
    AVFieldOrder, AVFormatContext, AVIO_FLAG_WRITE, AVMediaType, AVPacketSideDataType,
    AVPixelFormat, AVRational, av_channel_layout_default, av_dict_count, av_dict_free, av_dict_set,
    av_interleaved_write_frame, av_mallocz, av_new_packet, av_packet_new_side_data,
    av_packet_rescale_ts, av_packet_unref, av_write_trailer, avformat_alloc_output_context2,
    avformat_free_context, avformat_new_stream, avformat_write_header, avio_closep, avio_open,
};
use std::ffi::{CString, c_int};
use std::path::PathBuf;
use std::ptr::{null, null_mut};

/// An open muxer.
pub(crate) struct MuxerSession {
    output: PathBuf,
    context: OutputContext,
    /// Each track's stream, by the track's index.
    streams: Vec<Stream>,
    packet: PacketBuf,
}

/// A track's stream.
struct Stream {
    index: c_int,
    /// The track's time base, which its packets come in.
    track_base: AVRational,
    /// For an Opus track, its timescale: a packet shorter than the samples
    /// it holds ends the stream, and the rest is padding to discard.
    opus: Option<u32>,
}

/// An output `AVFormatContext` and its file, closed and freed when dropped.
struct OutputContext(*mut AVFormatContext);

// SAFETY: the context is owned here alone and used from one thread at a
// time; libavformat keeps no thread-bound state in it.
unsafe impl Send for OutputContext {}

impl OutputContext {
    /// Closes the file, if it is open.
    fn close(&mut self) -> Result<(), MediaError> {
        // SAFETY: the context is a muxing context whose `pb`, when set, was
        // opened by `avio_open`.
        let result = unsafe {
            let raw = self.0;
            if (*(*raw).oformat).flags & AVFMT_NOFILE != 0 || (*raw).pb.is_null() {
                return Ok(());
            }
            avio_closep(&mut (*raw).pb)
        };
        if result < 0 {
            Err(av_error(result, "close the output"))
        } else {
            Ok(())
        }
    }
}

impl Drop for OutputContext {
    fn drop(&mut self) {
        let _ = self.close();
        // SAFETY: the context was allocated by
        // `avformat_alloc_output_context2`.
        unsafe { avformat_free_context(self.0) }
    }
}

/// libavformat's codec id for `config`'s codec, and the codec tag it must
/// be stored with (ProRes's profile), or why `muxer` cannot store it.
fn codec_id(muxer: &str, config: &TrackConfig) -> Result<(AVCodecID, u32), MediaError> {
    let family = codec_family(&config.codec);
    let tag = |name: &[u8; 4]| u32::from_le_bytes(*name);
    Ok(match (family.as_str(), config.codec.as_str()) {
        ("avc1", _) => (AVCodecID::AV_CODEC_ID_H264, 0),
        ("hvc1", _) => (AVCodecID::AV_CODEC_ID_HEVC, 0),
        ("av01", _) => (AVCodecID::AV_CODEC_ID_AV1, 0),
        ("vp09", _) => (AVCodecID::AV_CODEC_ID_VP9, 0),
        ("ap4h", _) => (AVCodecID::AV_CODEC_ID_PRORES, tag(b"ap4h")),
        ("apch", _) => (AVCodecID::AV_CODEC_ID_PRORES, tag(b"apch")),
        ("mp4a", _) => (AVCodecID::AV_CODEC_ID_AAC, 0),
        ("opus", _) => (AVCodecID::AV_CODEC_ID_OPUS, 0),
        (_, "lpcm.s16le") => (AVCodecID::AV_CODEC_ID_PCM_S16LE, 0),
        (_, "lpcm.s24le") => (AVCodecID::AV_CODEC_ID_PCM_S24LE, 0),
        (_, "lpcm.f32le") => (AVCodecID::AV_CODEC_ID_PCM_F32LE, 0),
        _ => {
            return Err(MediaError::Codec(format!(
                "{muxer} does not take {}",
                config.codec
            )));
        }
    })
}

/// The bits a PCM sample takes, by codec id; none for compressed audio.
fn pcm_bits(id: AVCodecID) -> Option<c_int> {
    match id {
        AVCodecID::AV_CODEC_ID_PCM_S16LE => Some(16),
        AVCodecID::AV_CODEC_ID_PCM_S24LE => Some(24),
        AVCodecID::AV_CODEC_ID_PCM_F32LE => Some(32),
        _ => None,
    }
}

/// `value` as a C `int`, for the field `what`.
fn int(value: u32, what: &str) -> Result<c_int, MediaError> {
    c_int::try_from(value)
        .map_err(|_| MediaError::Codec(format!("a {what} of {value} is too large")))
}

/// Tags `parameters` with `color` where libavformat knows its names.
///
/// # Safety
///
/// `parameters` is a stream's codec parameters.
unsafe fn tag_color(parameters: *mut AVCodecParameters, color: &Color) {
    // SAFETY: the caller's parameters are writable.
    unsafe {
        if color.primaries == "bt709" {
            (*parameters).color_primaries = AVColorPrimaries::AVCOL_PRI_BT709;
        }
        if color.transfer == "bt709" {
            (*parameters).color_trc = AVColorTransferCharacteristic::AVCOL_TRC_BT709;
        }
        if color.matrix == "bt709" {
            (*parameters).color_space = AVColorSpace::AVCOL_SPC_BT709;
        }
        (*parameters).color_range = match color.range.as_str() {
            "limited" => AVColorRange::AVCOL_RANGE_MPEG,
            "full" => AVColorRange::AVCOL_RANGE_JPEG,
            _ => AVColorRange::AVCOL_RANGE_UNSPECIFIED,
        };
    }
}

/// Copies `bytes` into `parameters`' extradata, padded as libavformat
/// expects.
///
/// # Safety
///
/// `parameters` is a stream's codec parameters, with no extradata yet.
unsafe fn set_extradata(
    parameters: *mut AVCodecParameters,
    bytes: &[u8],
) -> Result<(), MediaError> {
    if bytes.is_empty() {
        return Ok(());
    }
    let size = c_int::try_from(bytes.len())
        .map_err(|_| MediaError::Codec("a track's description is too large".to_owned()))?;
    // SAFETY: the buffer is `bytes.len()` bytes and its padding, zeroed,
    // and libavformat frees it with the parameters.
    unsafe {
        let buffer = av_mallocz(bytes.len() + AV_INPUT_BUFFER_PADDING_SIZE as usize).cast::<u8>();
        if buffer.is_null() {
            return Err(MediaError::Codec(
                "FFmpeg could not allocate a track's description".to_owned(),
            ));
        }
        buffer.copy_from_nonoverlapping(bytes.as_ptr(), bytes.len());
        (*parameters).extradata = buffer;
        (*parameters).extradata_size = size;
    }
    Ok(())
}

/// Adds a stream for `track` to `context`.
///
/// # Safety
///
/// `context` is a muxing context whose header is not written yet.
unsafe fn add_stream(
    context: *mut AVFormatContext,
    muxer: &str,
    track: &Track,
) -> Result<Stream, MediaError> {
    let (config, timescale) = match track {
        Track::Video {
            config, timescale, ..
        }
        | Track::Audio {
            config, timescale, ..
        } => (config, *timescale),
    };
    let (id, tag) = codec_id(muxer, config)?;
    let track_base = AVRational {
        num: 1,
        den: int(timescale, "timescale")?,
    };
    if track_base.den <= 0 {
        return Err(MediaError::Codec("a track has a timescale of 0".to_owned()));
    }
    // SAFETY: the stream belongs to `context`, which the caller has not
    // started writing; its parameters are set before the header.
    unsafe {
        let stream = avformat_new_stream(context, null());
        if stream.is_null() {
            return Err(MediaError::Codec(
                "FFmpeg could not add a stream".to_owned(),
            ));
        }
        let parameters = (*stream).codecpar;
        (*parameters).codec_id = id;
        (*parameters).codec_tag = tag;
        (*stream).time_base = track_base;
        set_extradata(parameters, config.description.as_deref().unwrap_or(&[]))?;
        let mut opus = None;
        match track {
            Track::Video {
                width,
                height,
                frame_rate,
                color,
                alpha,
                ..
            } => {
                (*parameters).codec_type = AVMediaType::AVMEDIA_TYPE_VIDEO;
                (*parameters).width = int(*width, "width")?;
                (*parameters).height = int(*height, "height")?;
                let rate = AVRational {
                    num: int(frame_rate.numerator, "frame rate")?,
                    den: int(frame_rate.denominator, "frame rate")?,
                };
                (*parameters).framerate = rate;
                (*stream).avg_frame_rate = rate;
                // Celesta renders whole frames; `main`'s encoders said so
                // too (MOV's `fiel`, Matroska's FlagInterlaced).
                (*parameters).field_order = AVFieldOrder::AV_FIELD_PROGRESSIVE;
                let family = codec_family(&config.codec);
                if family == "vp09" && *alpha {
                    // What makes the WebM muxer write AlphaMode.
                    (*parameters).format = AVPixelFormat::AV_PIX_FMT_YUVA420P as c_int;
                }
                if id == AVCodecID::AV_CODEC_ID_PRORES && *alpha {
                    // A depth of 32 is how QuickTime marks ProRes with
                    // alpha, as `prores_ks` declared it on `main`.
                    (*parameters).bits_per_coded_sample = 32;
                }
                if matches!(family.as_str(), "ap4h" | "apch" | "vp09") {
                    tag_color(parameters, color);
                }
            }
            Track::Audio {
                sample_rate,
                channels,
                delay,
                ..
            } => {
                (*parameters).codec_type = AVMediaType::AVMEDIA_TYPE_AUDIO;
                (*parameters).sample_rate = int(*sample_rate, "sample rate")?;
                av_channel_layout_default(&mut (*parameters).ch_layout, c_int::from(*channels));
                (*parameters).initial_padding = c_int::try_from(*delay).map_err(|_| {
                    MediaError::Codec(format!("an audio delay of {delay} is too large"))
                })?;
                match id {
                    AVCodecID::AV_CODEC_ID_AAC => (*parameters).frame_size = 1024,
                    AVCodecID::AV_CODEC_ID_OPUS => {
                        (*parameters).frame_size = 960;
                        opus = Some(timescale);
                    }
                    _ => {}
                }
                if let Some(bits) = pcm_bits(id) {
                    (*parameters).bits_per_coded_sample = bits;
                    (*parameters).block_align = bits / 8 * c_int::from(*channels);
                    (*parameters).bit_rate =
                        i64::from(bits) * i64::from(*channels) * i64::from(*sample_rate);
                }
            }
        }
        Ok(Stream {
            index: (*stream).index,
            track_base,
            opus,
        })
    }
}

/// Opens `format` for `request`: the streams, the file, and its header.
pub(crate) fn open(
    format: MuxerFormat,
    request: &MuxerRequest<'_>,
) -> Result<MuxerSession, MediaError> {
    quiet_logs();
    format
        .availability()
        .map_err(|reason| MediaError::ComponentUnavailable {
            component: format.id().to_owned(),
            reason,
        })?;
    let settings = validate(&format.settings(), request.settings)
        .map_err(|InvalidSetting { key, message }| MediaError::InvalidSettings { key, message })?;
    let faststart = settings.get("faststart") == Some(&SettingValue::Boolean(true));
    let url = request
        .output
        .to_str()
        .and_then(|url| CString::new(url).ok())
        .ok_or_else(|| {
            MediaError::Codec(format!(
                "FFmpeg cannot write {}: its path is not UTF-8",
                request.output.display()
            ))
        })?;
    let name = CString::new(format.id()).expect("muxer names have no NUL");
    let mut raw = null_mut();
    // SAFETY: `name` and `url` are NUL-terminated and outlive the call.
    let result =
        unsafe { avformat_alloc_output_context2(&mut raw, null(), name.as_ptr(), url.as_ptr()) };
    if result < 0 {
        return Err(av_error(result, &format!("start a {} file", format.id())));
    }
    if raw.is_null() {
        return Err(MediaError::Codec(format!(
            "FFmpeg could not start a {} file",
            format.id()
        )));
    }
    let mut context = OutputContext(raw);
    let streams = request
        .tracks
        .iter()
        // SAFETY: the context's header is not written yet.
        .map(|track| unsafe { add_stream(raw, format.id(), track) })
        .collect::<Result<Vec<_>, _>>()?;
    // SAFETY: `pb` is the context's, closed when it is dropped.
    let result = unsafe { avio_open(&mut (*raw).pb, url.as_ptr(), AVIO_FLAG_WRITE) };
    if result < 0 {
        return Err(av_error(
            result,
            &format!("create {}", request.output.display()),
        ));
    }
    if let Err(error) = write_header(raw, faststart) {
        let _ = context.close();
        let _ = std::fs::remove_file(request.output);
        return Err(error);
    }
    Ok(MuxerSession {
        output: request.output.to_owned(),
        context,
        streams,
        packet: PacketBuf::new()?,
    })
}

/// Writes `context`'s header, with `+faststart` when asked.
fn write_header(context: *mut AVFormatContext, faststart: bool) -> Result<(), MediaError> {
    let mut options: *mut AVDictionary = null_mut();
    // SAFETY: the context has its streams and an open file; the dictionary
    // is freed whatever the header's result.
    unsafe {
        if faststart {
            let result = av_dict_set(
                &mut options,
                c"movflags".as_ptr(),
                c"+faststart".as_ptr(),
                0,
            );
            if result < 0 {
                av_dict_free(&mut options);
                return Err(av_error(result, "set the muxer's options"));
            }
        }
        let result = avformat_write_header(context, &mut options);
        let unused = av_dict_count(options);
        av_dict_free(&mut options);
        if result < 0 {
            return Err(av_error(result, "write the header"));
        }
        if unused > 0 {
            return Err(MediaError::Codec(
                "the muxer did not take every option given".to_owned(),
            ));
        }
    }
    Ok(())
}

/// The sample frames an Opus packet holds at 48 kHz, from its TOC byte
/// (RFC 6716, section 3.1).
fn opus_samples(packet: &[u8]) -> Option<i64> {
    let toc = *packet.first()?;
    let config = toc >> 3;
    let frame = match config {
        0..=11 => [480, 960, 1920, 2880][usize::from(config % 4)],
        12..=15 => [480, 960][usize::from(config % 2)],
        _ => [120, 240, 480, 960][usize::from(config % 4)],
    };
    let frames = match toc & 3 {
        0 => 1,
        1 | 2 => 2,
        _ => i64::from(*packet.get(1)? & 0x3F),
    };
    Some(frame * frames)
}

impl MuxerSession {
    /// Writes `packets` in the order given.
    pub(crate) fn write(&mut self, packets: &[TrackPacket]) -> Result<(), MediaError> {
        for packet in packets {
            let result = self.write_one(packet);
            // SAFETY: the session's packet; blank already after a write.
            unsafe { av_packet_unref(self.packet.as_ptr()) };
            result?;
        }
        Ok(())
    }

    fn write_one(&mut self, given: &TrackPacket) -> Result<(), MediaError> {
        let stream = self.streams.get(given.track).ok_or_else(|| {
            MediaError::Codec(format!(
                "a packet for track {} of {}",
                given.track,
                self.streams.len()
            ))
        })?;
        let source = &given.packet;
        let size = c_int::try_from(source.bytes.len())
            .map_err(|_| MediaError::Codec("a packet is too large".to_owned()))?;
        let packet = self.packet.as_ptr();
        let context = self.context.0;
        // SAFETY: `packet` is the session's, blank, and given `size` bytes
        // and its side data before it is written; the stream index is one
        // of the context's.
        unsafe {
            let result = av_new_packet(packet, size);
            if result < 0 {
                return Err(av_error(result, "allocate a packet"));
            }
            if !source.bytes.is_empty() {
                (*packet)
                    .data
                    .copy_from_nonoverlapping(source.bytes.as_ptr(), source.bytes.len());
            }
            (*packet).pts = source.pts;
            (*packet).dts = source.dts;
            (*packet).duration = source.duration;
            (*packet).stream_index = stream.index;
            if source.key {
                (*packet).flags |= AV_PKT_FLAG_KEY;
            }
            for side in &source.additional {
                let data = av_packet_new_side_data(
                    packet,
                    AVPacketSideDataType::AV_PKT_DATA_MATROSKA_BLOCKADDITIONAL,
                    8 + side.bytes.len(),
                );
                if data.is_null() {
                    return Err(MediaError::Codec(
                        "FFmpeg could not allocate a BlockAddition".to_owned(),
                    ));
                }
                let data = std::slice::from_raw_parts_mut(data, 8 + side.bytes.len());
                data[..8].copy_from_slice(&u64::from(side.id).to_be_bytes());
                data[8..].copy_from_slice(&side.bytes);
            }
            // libopus shortens its last packet to the samples it was given;
            // the rest is padding, which Matroska's DiscardPadding hides.
            if let Some(timescale) = stream.opus {
                let held = opus_samples(&source.bytes)
                    .map(|samples| samples * i64::from(timescale) / 48_000);
                if let Some(held) = held.filter(|&held| source.duration < held) {
                    let padding = u32::try_from(held - source.duration).unwrap_or(0);
                    let data = av_packet_new_side_data(
                        packet,
                        AVPacketSideDataType::AV_PKT_DATA_SKIP_SAMPLES,
                        10,
                    );
                    if data.is_null() {
                        return Err(MediaError::Codec(
                            "FFmpeg could not allocate a packet's padding".to_owned(),
                        ));
                    }
                    let data = std::slice::from_raw_parts_mut(data, 10);
                    data.fill(0);
                    data[4..8].copy_from_slice(&padding.to_le_bytes());
                }
            }
            let target = (**(*context).streams.add(stream.index as usize)).time_base;
            av_packet_rescale_ts(packet, stream.track_base, target);
            let result = av_interleaved_write_frame(context, packet);
            if result < 0 {
                return Err(av_error(result, "write a packet"));
            }
        }
        Ok(())
    }

    /// Writes the trailer and closes the file: the files written.
    pub(crate) fn finish(mut self) -> Result<Vec<PathBuf>, MediaError> {
        // SAFETY: the context's header was written.
        let result = unsafe { av_write_trailer(self.context.0) };
        if result < 0 {
            return Err(av_error(result, "finish the file"));
        }
        self.context.close()?;
        Ok(vec![self.output])
    }

    /// Closes the file with no trailer: the files left.
    pub(crate) fn abort(mut self) -> Vec<PathBuf> {
        let _ = self.context.close();
        vec![self.output]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opus_packets_count_their_samples() {
        // CELT 20 ms, one frame; SILK 60 ms, two frames; CELT 2.5 ms, a
        // count of 3 in the second byte.
        assert_eq!(opus_samples(&[31 << 3, 0]), Some(960));
        assert_eq!(opus_samples(&[(3 << 3) | 1]), Some(5760));
        assert_eq!(opus_samples(&[(16 << 3) | 3, 3]), Some(360));
        assert_eq!(opus_samples(&[]), None);
    }

    #[test]
    fn families_map_to_codec_ids() {
        let config = |codec: &str| TrackConfig {
            codec: codec.to_owned(),
            description: None,
        };
        assert_eq!(
            codec_id("mov", &config("ap4h")).unwrap(),
            (AVCodecID::AV_CODEC_ID_PRORES, u32::from_le_bytes(*b"ap4h"))
        );
        assert_eq!(
            codec_id("mp4", &config("avc3.640028")).unwrap().0,
            AVCodecID::AV_CODEC_ID_H264
        );
        assert_eq!(
            codec_id("mov", &config("lpcm.s24le")).unwrap().0,
            AVCodecID::AV_CODEC_ID_PCM_S24LE
        );
        let error = codec_id("webm", &config("theora")).unwrap_err();
        assert_eq!(
            error.to_string(),
            MediaError::Codec("webm does not take theora".into()).to_string()
        );
    }
}
