//! The audio encoders: AAC (FFmpeg's `aac`) and Opus (libopus) on
//! libavcodec, fed through an `AVAudioFifo` in frames of the encoder's frame
//! size, and PCM, which needs no codec and is packed here.
//!
//! The parameters are those of `main`'s `mux_audio`, so the decoded audio is
//! the same: AAC and Opus at 192 kbit/s by default, AAC at the mix's rate
//! when it takes it, Opus at 48 kHz, and PCM converted from `f32` exactly as
//! libswresample and `pcm_s24le` did.

use crate::components::AudioCodec;
use crate::ffi::{AudioFifo, CodecContext, Frame, PacketBuf, av_error, extradata, quiet_logs};
use celesta_media::{
    AudioEncoderRequest, Encoded, InvalidSetting, MediaError, Packet, SampleRates, SettingValue,
    Settings, TrackConfig, settings::validate,
};
use ffmpeg_sys_next::{
    AV_CODEC_FLAG_GLOBAL_HEADER, AV_PKT_FLAG_KEY, AVERROR, AVERROR_EOF, AVRational, AVSampleFormat,
    EAGAIN, av_audio_fifo_read, av_audio_fifo_size, av_audio_fifo_write, av_channel_layout_default,
    av_packet_unref, avcodec_find_encoder_by_name, avcodec_open2, avcodec_receive_packet,
    avcodec_send_frame,
};
use std::ffi::{CString, c_int, c_void};
use std::ptr::{null, null_mut};

/// The rate AAC falls back to when it does not take the mix's, and the only
/// one Opus takes.
const FALLBACK_RATE: u32 = 48_000;

/// An open audio encoder.
pub(crate) struct AudioSession {
    channels: u16,
    /// The `pts` the next samples must have: none before the first.
    next: Option<i64>,
    encoder: Encoder,
    /// The track's config, until the first packets (or the finish) carry it.
    config: Option<TrackConfig>,
}

/// What [`open`] chose: the session, its rate, channels, and delay.
pub(crate) struct Opened {
    pub(crate) session: AudioSession,
    pub(crate) sample_rate: u32,
    pub(crate) channels: u16,
    pub(crate) delay: u64,
}

enum Encoder {
    Codec(CodecEncoder),
    Pcm(PcmFormat),
}

/// A libavcodec encoder and the FIFO that cuts its input into frames.
struct CodecEncoder {
    context: CodecContext,
    fifo: AudioFifo,
    packet: PacketBuf,
    format: AVSampleFormat,
    sample_rate: c_int,
    /// The samples in each frame but the last.
    frame_size: c_int,
    /// The `pts` of the FIFO's first sample frame.
    fifo_pts: i64,
}

/// The layout PCM is packed in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PcmFormat {
    S16,
    S24,
    F32,
}

impl PcmFormat {
    fn from_setting(value: &str) -> Self {
        match value {
            "s16le" => Self::S16,
            "f32le" => Self::F32,
            _ => Self::S24,
        }
    }

    /// Its codec string (deviation 13).
    fn codec(self) -> &'static str {
        match self {
            Self::S16 => "lpcm.s16le",
            Self::S24 => "lpcm.s24le",
            Self::F32 => "lpcm.f32le",
        }
    }

    /// `samples` packed little-endian, converted as libswresample converts
    /// `flt` to `s16` and `s32` (scaled, rounded to the nearest even, and
    /// clipped), and `s24` as `pcm_s24le` keeps the high 24 bits of `s32`.
    pub(crate) fn pack(self, samples: &[f32]) -> Vec<u8> {
        match self {
            Self::S16 => samples
                .iter()
                .flat_map(|&sample| {
                    let scaled = (f64::from(sample) * 32_768.0).round_ties_even();
                    (scaled.clamp(-32_768.0, 32_767.0) as i16).to_le_bytes()
                })
                .collect(),
            Self::S24 => samples
                .iter()
                .flat_map(|&sample| {
                    let scaled = (f64::from(sample) * 2_147_483_648.0).round_ties_even();
                    let wide = scaled.clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32;
                    let [low, middle, high, _] = (wide >> 8).to_le_bytes();
                    [low, middle, high]
                })
                .collect(),
            Self::F32 => samples
                .iter()
                .flat_map(|sample| sample.to_le_bytes())
                .collect(),
        }
    }
}

fn text<'a>(settings: &'a Settings, key: &str) -> &'a str {
    match settings.get(key) {
        Some(SettingValue::String(value)) => value,
        _ => "",
    }
}

/// Opens `codec` for `request`.
pub(crate) fn open(
    codec: AudioCodec,
    request: &AudioEncoderRequest<'_>,
) -> Result<Opened, MediaError> {
    quiet_logs();
    codec
        .availability()
        .map_err(|reason| MediaError::ComponentUnavailable {
            component: codec.id().to_owned(),
            reason,
        })?;
    let settings = validate(&codec.settings(), request.settings)
        .map_err(|InvalidSetting { key, message }| MediaError::InvalidSettings { key, message })?;
    if request.sample_rate == 0 {
        return Err(MediaError::Codec(
            "audio at a sample rate of 0 cannot be encoded".to_owned(),
        ));
    }
    let channels = if request.channels == 1 { 1 } else { 2 };
    let sample_rate = match codec.sample_rates() {
        SampleRates::Only(rates) if !rates.contains(&request.sample_rate) => FALLBACK_RATE,
        _ => request.sample_rate,
    };
    let (encoder, config, delay) = match codec.encoder() {
        None => {
            let format = PcmFormat::from_setting(text(&settings, "format"));
            let config = TrackConfig {
                codec: format.codec().to_owned(),
                description: None,
            };
            (Encoder::Pcm(format), config, 0)
        }
        Some(name) => {
            let bit_rate = match settings.get("bitrate") {
                Some(SettingValue::Number(value)) => *value as i64,
                _ => 192_000,
            };
            let (encoder, delay, description) =
                CodecEncoder::open(name, sample_rate, channels, bit_rate)?;
            let config = TrackConfig {
                codec: match codec {
                    AudioCodec::Aac => "mp4a.40.2",
                    _ => "opus",
                }
                .to_owned(),
                description: Some(description),
            };
            (Encoder::Codec(encoder), config, delay)
        }
    };
    Ok(Opened {
        session: AudioSession {
            channels,
            next: None,
            encoder,
            config: Some(config),
        },
        sample_rate,
        channels,
        delay,
    })
}

impl CodecEncoder {
    /// Opens libavcodec's `name`: the encoder, its delay, and its extradata.
    fn open(
        name: &str,
        sample_rate: u32,
        channels: u16,
        bit_rate: i64,
    ) -> Result<(Self, u64, Vec<u8>), MediaError> {
        let rate = c_int::try_from(sample_rate).map_err(|_| {
            MediaError::Codec(format!("a sample rate of {sample_rate} is too large"))
        })?;
        // AAC takes planar samples; libopus interleaved ones, as `main`'s
        // filter graph gave it.
        let format = if name == "aac" {
            AVSampleFormat::AV_SAMPLE_FMT_FLTP
        } else {
            AVSampleFormat::AV_SAMPLE_FMT_FLT
        };
        let encoder_name = CString::new(name).expect("encoder names have no NUL");
        // SAFETY: `encoder_name` is NUL-terminated and outlives the call.
        let encoder = unsafe { avcodec_find_encoder_by_name(encoder_name.as_ptr()) };
        let context = CodecContext::new(encoder)?;
        // SAFETY: `context` is a fresh context of `encoder`, configured
        // before `avcodec_open2`.
        let (frame_size, delay, description) = unsafe {
            let raw = context.as_ptr();
            (*raw).sample_fmt = format;
            (*raw).sample_rate = rate;
            av_channel_layout_default(&mut (*raw).ch_layout, c_int::from(channels));
            (*raw).time_base = AVRational { num: 1, den: rate };
            (*raw).bit_rate = bit_rate;
            (*raw).flags |= AV_CODEC_FLAG_GLOBAL_HEADER as c_int;
            let result = avcodec_open2(raw, encoder, null_mut());
            if result < 0 {
                return Err(av_error(result, &format!("open {name}")));
            }
            ((*raw).frame_size, (*raw).initial_padding, extradata(raw))
        };
        if frame_size <= 0 {
            return Err(MediaError::Codec(format!(
                "{name} gave no frame size to cut its input to"
            )));
        }
        Ok((
            Self {
                context,
                fifo: AudioFifo::new(format, channels)?,
                packet: PacketBuf::new()?,
                format,
                sample_rate: rate,
                frame_size,
                fifo_pts: 0,
            },
            u64::try_from(delay).unwrap_or(0),
            description,
        ))
    }

    fn channels(&self) -> u16 {
        // SAFETY: the context is open, with the channel count given.
        let count = unsafe { (*self.context.as_ptr()).ch_layout.nb_channels };
        u16::try_from(count).unwrap_or(1)
    }

    /// Queues interleaved `samples`, `frames` sample frames of them.
    fn write(&mut self, samples: &[f32], frames: usize) -> Result<(), MediaError> {
        let count = c_int::try_from(frames)
            .map_err(|_| MediaError::Codec(format!("{frames} sample frames are too many")))?;
        let channels = usize::from(self.channels());
        let planes: Vec<Vec<f32>> = if self.format == AVSampleFormat::AV_SAMPLE_FMT_FLTP {
            (0..channels)
                .map(|channel| {
                    samples
                        .iter()
                        .skip(channel)
                        .step_by(channels)
                        .copied()
                        .collect()
                })
                .collect()
        } else {
            Vec::new()
        };
        let pointers: Vec<*mut c_void> = if planes.is_empty() {
            vec![samples.as_ptr().cast_mut().cast()]
        } else {
            planes
                .iter()
                .map(|plane| plane.as_ptr().cast_mut().cast())
                .collect()
        };
        // SAFETY: each pointer holds `count` samples of its plane (or all
        // channels, interleaved), which the FIFO only reads.
        let written = unsafe { av_audio_fifo_write(self.fifo.as_ptr(), pointers.as_ptr(), count) };
        if written < 0 {
            return Err(av_error(written, "queue audio"));
        }
        if written != count {
            return Err(MediaError::Codec(format!(
                "FFmpeg queued {written} of {count} sample frames"
            )));
        }
        Ok(())
    }

    fn queued(&self) -> c_int {
        // SAFETY: the FIFO is this encoder's.
        unsafe { av_audio_fifo_size(self.fifo.as_ptr()) }
    }

    /// Sends `count` queued sample frames as one frame, then takes the
    /// packets the encoder has ready.
    fn send(&mut self, count: c_int, packets: &mut Vec<Packet>) -> Result<(), MediaError> {
        let channels = self.channels();
        let frame = Frame::audio(self.format, channels, self.sample_rate, count)?;
        let raw = frame.as_ptr();
        // SAFETY: the frame has buffers for `count` sample frames of the
        // FIFO's format; the encoder takes its own reference.
        unsafe {
            let read = av_audio_fifo_read(
                self.fifo.as_ptr(),
                (*raw).extended_data.cast::<*mut c_void>().cast_const(),
                count,
            );
            if read < 0 {
                return Err(av_error(read, "read queued audio"));
            }
            if read != count {
                return Err(MediaError::Codec(format!(
                    "FFmpeg read {read} of {count} queued sample frames"
                )));
            }
            (*raw).pts = self.fifo_pts;
            let result = avcodec_send_frame(self.context.as_ptr(), raw);
            if result < 0 {
                return Err(av_error(result, "encode audio"));
            }
        }
        self.fifo_pts += i64::from(count);
        self.receive(packets)
    }

    /// Sends every whole frame queued.
    fn send_whole(&mut self, packets: &mut Vec<Packet>) -> Result<(), MediaError> {
        while self.queued() >= self.frame_size {
            self.send(self.frame_size, packets)?;
        }
        Ok(())
    }

    /// Sends what is left as a short last frame and drains the encoder.
    fn finish(&mut self, packets: &mut Vec<Packet>) -> Result<(), MediaError> {
        self.send_whole(packets)?;
        let rest = self.queued();
        if rest > 0 {
            self.send(rest, packets)?;
        }
        // SAFETY: a null frame starts the encoder's flush.
        let result = unsafe { avcodec_send_frame(self.context.as_ptr(), null()) };
        if result < 0 {
            return Err(av_error(result, "finish encoding audio"));
        }
        self.receive(packets)
    }

    /// Every packet the encoder has ready.
    fn receive(&mut self, packets: &mut Vec<Packet>) -> Result<(), MediaError> {
        let packet = self.packet.as_ptr();
        loop {
            // SAFETY: `packet` is this encoder's; it is unreferenced once
            // copied out, and holds `size` bytes until then.
            unsafe {
                let result = avcodec_receive_packet(self.context.as_ptr(), packet);
                if result == AVERROR(EAGAIN) || result == AVERROR_EOF {
                    return Ok(());
                }
                if result < 0 {
                    return Err(av_error(result, "encode audio"));
                }
                let bytes = match usize::try_from((*packet).size) {
                    Ok(size) if size > 0 => std::slice::from_raw_parts((*packet).data, size),
                    _ => &[],
                };
                packets.push(Packet {
                    pts: (*packet).pts,
                    dts: (*packet).dts,
                    duration: (*packet).duration,
                    key: (*packet).flags & AV_PKT_FLAG_KEY != 0,
                    bytes: bytes.to_vec(),
                    additional: Vec::new(),
                });
                av_packet_unref(packet);
            }
        }
    }
}

impl AudioSession {
    /// Encodes interleaved `samples`, the first at `pts` in sample frames.
    pub(crate) fn encode(&mut self, pts: i64, samples: &[f32]) -> Result<Encoded, MediaError> {
        let channels = usize::from(self.channels);
        if !samples.len().is_multiple_of(channels) {
            return Err(MediaError::Codec(format!(
                "{} samples are not whole frames of {channels} channels",
                samples.len()
            )));
        }
        let frames = samples.len() / channels;
        match self.next {
            Some(next) if next != pts => {
                return Err(MediaError::Codec(format!(
                    "audio at {pts} does not follow the audio before it, which ends at {next}"
                )));
            }
            Some(_) => {}
            None => {
                if let Encoder::Codec(encoder) = &mut self.encoder {
                    encoder.fifo_pts = pts;
                }
            }
        }
        self.next = Some(pts + frames as i64);
        let mut packets = Vec::new();
        match &mut self.encoder {
            Encoder::Pcm(_) if frames == 0 => {}
            Encoder::Pcm(format) => packets.push(Packet {
                pts,
                dts: pts,
                duration: frames as i64,
                key: true,
                bytes: format.pack(samples),
                additional: Vec::new(),
            }),
            Encoder::Codec(encoder) => {
                encoder.write(samples, frames)?;
                encoder.send_whole(&mut packets)?;
            }
        }
        let config = if packets.is_empty() {
            None
        } else {
            self.config.take()
        };
        Ok(Encoded { packets, config })
    }

    /// Flushes the encoder: its last packets, and the config if no packets
    /// carried it yet.
    pub(crate) fn finish(&mut self) -> Result<Encoded, MediaError> {
        let mut packets = Vec::new();
        if let Encoder::Codec(encoder) = &mut self.encoder {
            encoder.finish(&mut packets)?;
        }
        Ok(Encoded {
            packets,
            config: self.config.take(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unpack_s24(bytes: &[u8]) -> Vec<i32> {
        bytes
            .as_chunks::<3>()
            .0
            .iter()
            .map(|&[low, middle, high]| i32::from_le_bytes([0, low, middle, high]) >> 8)
            .collect()
    }

    #[test]
    fn s24_keeps_the_high_bits_of_s32() {
        let samples = [0.0, 0.5, -0.5, 1.0, -1.0, 2.0, -2.0, 1.0 / 8_388_608.0];
        assert_eq!(
            unpack_s24(&PcmFormat::S24.pack(&samples)),
            [
                0, 4_194_304, -4_194_304, 8_388_607, -8_388_608, 8_388_607, -8_388_608, 1
            ]
        );
        // x × 2³¹ rounds to the nearest even before the shift: 1.5 × 2⁻³¹
        // is 2, and 2.5 × 2⁻³¹ is 2 too; both shift to 0.
        let tiny = [1.5 / 2_147_483_648.0, -(255.5 / 2_147_483_648.0)];
        assert_eq!(unpack_s24(&PcmFormat::S24.pack(&tiny)), [0, -1]);
    }

    #[test]
    fn s16_rounds_and_clips() {
        let packed = PcmFormat::S16.pack(&[0.5, -1.0, 1.0, 0.5 / 32_768.0, 1.5 / 32_768.0]);
        let values: Vec<i16> = packed
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&bytes| i16::from_le_bytes(bytes))
            .collect();
        assert_eq!(values, [16_384, -32_768, 32_767, 0, 2]);
    }

    #[test]
    fn f32_is_raw() {
        assert_eq!(
            PcmFormat::F32.pack(&[0.25, -3.0]),
            [0.25f32.to_le_bytes(), (-3.0f32).to_le_bytes()].concat()
        );
    }
}
