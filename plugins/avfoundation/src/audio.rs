//! The AAC encoder, on AudioToolbox's `AudioConverter`: interleaved `f32`
//! LPCM in, AAC-LC out, 1024 sample frames a packet.
//!
//! A session keeps the samples it was given that the converter has not
//! taken yet. `AudioConverterFillComplexBuffer` pulls them through an input
//! callback, which hands over what there is; when nothing is left before
//! [`AudioSession::finish`] it answers a private status, [`NO_DATA_YET`],
//! which stops the fill without ending the stream (the pattern Apple
//! documents for an input that has run dry for now). Only `finish` answers
//! the end of the stream, after which the converter flushes its last
//! packets. The `k`-th packet starts `1024 k` frames after the first sample
//! less the encoder's priming (`kAudioConverterPrimeInfo.leadingFrames`),
//! which [`open`] answers as the delay; the last lasts only the frames it
//! covers, so the muxer trims the end.

use crate::components::{self, aac_bitrate};
use crate::{apple, esds};
use celesta_media::{AudioEncoderRequest, Encoded, MediaError, Packet, TrackConfig};
use objc2_audio_toolbox::{
    AudioConverterDispose, AudioConverterFillComplexBuffer, AudioConverterGetProperty,
    AudioConverterGetPropertyInfo, AudioConverterNew, AudioConverterPrimeInfo,
    AudioConverterPropertyID, AudioConverterRef, AudioConverterSetProperty,
    kAudioCodecBitRateControlMode_Constant, kAudioCodecPropertyBitRateControlMode,
    kAudioConverterApplicableEncodeBitRates, kAudioConverterCompressionMagicCookie,
    kAudioConverterEncodeBitRate, kAudioConverterPrimeInfo,
    kAudioConverterPropertyMaximumOutputPacketSize,
};
use objc2_core_audio_types::{
    AudioBuffer, AudioBufferList, AudioStreamBasicDescription, AudioStreamPacketDescription,
    AudioValueRange, MPEG4ObjectID, kAudioFormatFlagsNativeFloatPacked, kAudioFormatLinearPCM,
    kAudioFormatMPEG4AAC,
};
use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::{NonNull, null_mut};

/// The sample frames in each AAC-LC packet.
const FRAMES_PER_PACKET: i64 = 1024;

/// The rate the encoder takes when AudioToolbox does not list the mix's.
const FALLBACK_RATE: u32 = 48_000;

/// What the input callback answers when it has no samples left before the
/// session finishes: not `noErr`, which with no data would end the stream,
/// and none of AudioToolbox's own codes.
const NO_DATA_YET: i32 = i32::from_be_bytes(*b"nodt");

/// The packets each `AudioConverterFillComplexBuffer` asks for.
const PACKETS_PER_FILL: u32 = 16;

/// The error for an AudioToolbox call that answered `status`.
fn failed(call: &str, status: i32) -> MediaError {
    MediaError::Codec(format!("{call} failed: {status}"))
}

/// An `AudioConverter`, disposed when dropped.
struct Converter(NonNull<c_void>);

impl Converter {
    fn raw(&self) -> AudioConverterRef {
        self.0.as_ptr().cast()
    }

    /// The value of `property`, which is one `T`.
    fn value<T: Copy>(
        &self,
        property: AudioConverterPropertyID,
        name: &str,
        mut value: T,
    ) -> Result<T, MediaError> {
        let mut size = size_of::<T>() as u32;
        // SAFETY: the converter is valid, `value` has room for `size` bytes,
        // and each caller's `T` is the property's documented plain-data type.
        let status = unsafe {
            AudioConverterGetProperty(
                self.raw(),
                property,
                NonNull::from(&mut size),
                NonNull::from(&mut value).cast(),
            )
        };
        if status != 0 {
            return Err(failed(
                &format!("AudioConverterGetProperty({name})"),
                status,
            ));
        }
        Ok(value)
    }

    /// Sets `property`, which is one `UInt32`, to `value`.
    fn set(
        &self,
        property: AudioConverterPropertyID,
        name: &str,
        value: u32,
    ) -> Result<(), MediaError> {
        // SAFETY: the converter is valid, and `value` is the `UInt32` the
        // property takes.
        let status = unsafe {
            AudioConverterSetProperty(
                self.raw(),
                property,
                size_of::<u32>() as u32,
                NonNull::from(&value).cast(),
            )
        };
        if status != 0 {
            return Err(failed(
                &format!("AudioConverterSetProperty({name})"),
                status,
            ));
        }
        Ok(())
    }

    /// The value of `property`, a list of `T` of the size it says.
    fn list<T: Copy>(
        &self,
        property: AudioConverterPropertyID,
        name: &str,
        empty: T,
    ) -> Result<Vec<T>, MediaError> {
        let call = format!("AudioConverterGetProperty({name})");
        let mut size: u32 = 0;
        // SAFETY: the converter is valid and `size` is a valid place for the
        // answer; the writable flag is not asked for.
        let status =
            unsafe { AudioConverterGetPropertyInfo(self.raw(), property, &mut size, null_mut()) };
        if status != 0 {
            return Err(failed(&call, status));
        }
        let mut values = vec![empty; size as usize / size_of::<T>()];
        if values.is_empty() {
            return Ok(values);
        }
        let mut size = (values.len() * size_of::<T>()) as u32;
        // SAFETY: `values` has room for `size` bytes, and each caller's `T`
        // is the plain-data element type of the property's list.
        let status = unsafe {
            AudioConverterGetProperty(
                self.raw(),
                property,
                NonNull::from(&mut size),
                NonNull::from(values.as_mut_slice()).cast(),
            )
        };
        if status != 0 {
            return Err(failed(&call, status));
        }
        values.truncate(size as usize / size_of::<T>());
        Ok(values)
    }
}

impl Drop for Converter {
    fn drop(&mut self) {
        // SAFETY: the converter is valid and disposed of only here.
        unsafe { AudioConverterDispose(self.raw()) };
    }
}

/// The samples a session holds for the converter's input callback.
struct Input {
    channels: usize,
    /// Interleaved samples given and not yet handed to the converter.
    pending: Vec<f32>,
    /// The samples last handed to the converter, which it may read until it
    /// calls back again.
    lent: Vec<f32>,
    /// Whether the stream has ended (the session is finishing).
    ended: bool,
}

/// An open AAC encoder.
pub(crate) struct AudioSession {
    // Declared first, so it is disposed of before the samples it may read.
    converter: Converter,
    input: Input,
    channels: u16,
    /// The largest packet the converter writes, in bytes.
    max_packet: usize,
    /// The encoder's priming, in sample frames.
    delay: i64,
    /// The `pts` of the first sample frame given; none before any.
    first: Option<i64>,
    /// The sample frames given.
    frames: i64,
    /// The packets made.
    packets: i64,
    /// The track's config, until the first packets (or the finish) carry it.
    config: Option<TrackConfig>,
}

// SAFETY: an `AudioConverter` is not tied to the thread that made it; the
// session is used from one thread at a time (`&mut self`), and the input
// callback runs only within `AudioConverterFillComplexBuffer` on the
// caller's thread. The rest is plain owned data.
unsafe impl Send for AudioSession {}

/// The input callback: hands the converter as many of the pending samples
/// as it asks for, or none, answering [`NO_DATA_YET`] before the end of the
/// stream and `noErr` at it.
unsafe extern "C-unwind" fn supply(
    _converter: AudioConverterRef,
    packets: NonNull<u32>,
    data: NonNull<AudioBufferList>,
    _descriptions: *mut *mut AudioStreamPacketDescription,
    user: *mut c_void,
) -> i32 {
    // SAFETY: `user` is the session's `Input`, which `fill` lends for the
    // length of the `AudioConverterFillComplexBuffer` that calls back and
    // touches nowhere else meanwhile; `packets` and `data` are the
    // converter's, valid for this call, and an interleaved input's list has
    // one buffer.
    let (input, wanted, buffer) = unsafe {
        (
            &mut *user.cast::<Input>(),
            *packets.as_ptr() as usize,
            &mut (*data.as_ptr()).mBuffers[0],
        )
    };
    let given = wanted.min(input.pending.len() / input.channels);
    if given == 0 {
        // SAFETY: as above.
        unsafe { *packets.as_ptr() = 0 };
        buffer.mDataByteSize = 0;
        buffer.mData = null_mut();
        return if input.ended { 0 } else { NO_DATA_YET };
    }
    let samples = given * input.channels;
    input.lent = if samples == input.pending.len() {
        std::mem::take(&mut input.pending)
    } else {
        input.pending.drain(..samples).collect()
    };
    buffer.mNumberChannels = input.channels as u32;
    buffer.mDataByteSize = (samples * size_of::<f32>()) as u32;
    buffer.mData = input.lent.as_mut_ptr().cast();
    // SAFETY: as above.
    unsafe { *packets.as_ptr() = given as u32 };
    0
}

/// The rate `aac` takes for a mix at `asked`: the mix's when it is one of
/// `rates` (those AudioToolbox lists), else [`FALLBACK_RATE`].
fn sample_rate(asked: u32, rates: &[u32]) -> u32 {
    if rates.contains(&asked) {
        asked
    } else {
        FALLBACK_RATE
    }
}

/// Opens the AAC encoder for `request`, at the mix's rate when it is one of
/// the rates the encoder takes, else 48 kHz, mono for a mono mix and stereo
/// for any other: the session, its rate, channels, and delay.
pub(crate) fn open(
    request: &AudioEncoderRequest<'_>,
) -> Result<(AudioSession, u32, u16, u64), MediaError> {
    let bitrate = aac_bitrate(request.settings)?;
    let encoders = apple::audio_encoder_formats();
    let rates = apple::aac_sample_rates();
    let rates = components::aac_rates(
        encoders.as_deref().map_err(String::as_str),
        rates.as_deref().map_err(String::as_str),
    )
    .map_err(|reason| MediaError::ComponentUnavailable {
        component: components::AudioCodec::Aac.id().to_owned(),
        reason,
    })?;
    let rate = sample_rate(request.sample_rate, rates);
    let channels: u16 = if request.channels == 1 { 1 } else { 2 };
    let frame_bytes = u32::from(channels) * size_of::<f32>() as u32;
    let source = AudioStreamBasicDescription {
        mSampleRate: f64::from(rate),
        mFormatID: kAudioFormatLinearPCM,
        mFormatFlags: kAudioFormatFlagsNativeFloatPacked,
        mBytesPerPacket: frame_bytes,
        mFramesPerPacket: 1,
        mBytesPerFrame: frame_bytes,
        mChannelsPerFrame: u32::from(channels),
        mBitsPerChannel: 32,
        mReserved: 0,
    };
    let destination = AudioStreamBasicDescription {
        mSampleRate: f64::from(rate),
        mFormatID: kAudioFormatMPEG4AAC,
        mFormatFlags: MPEG4ObjectID::AAC_LC.0 as u32,
        mBytesPerPacket: 0,
        mFramesPerPacket: FRAMES_PER_PACKET as u32,
        mBytesPerFrame: 0,
        mChannelsPerFrame: u32::from(channels),
        mBitsPerChannel: 0,
        mReserved: 0,
    };
    let mut raw: AudioConverterRef = null_mut();
    // SAFETY: both descriptions and `raw` are valid for the call.
    let status = unsafe {
        AudioConverterNew(
            NonNull::from(&source),
            NonNull::from(&destination),
            NonNull::from(&mut raw),
        )
    };
    if status != 0 {
        return Err(failed("AudioConverterNew", status));
    }
    let converter =
        Converter(NonNull::new(raw.cast()).ok_or_else(|| failed("AudioConverterNew", status))?);
    // AudioToolbox's default, a long-term average, starts low and settles
    // over the first second, so a video's first hits (and a loop's start,
    // whose lead-in is shorter) come out worse than the rest; a constant
    // bitrate codes every packet alike, as FFmpeg's `aac_at` asks for a
    // bitrate.
    converter.set(
        kAudioCodecPropertyBitRateControlMode,
        "BitRateControlMode",
        kAudioCodecBitRateControlMode_Constant,
    )?;

    let applicable = converter.list(
        kAudioConverterApplicableEncodeBitRates,
        "ApplicableEncodeBitRates",
        AudioValueRange {
            mMinimum: 0.0,
            mMaximum: 0.0,
        },
    )?;
    let lowest = applicable
        .iter()
        .map(|range| range.mMinimum)
        .reduce(f64::min);
    let highest = applicable
        .iter()
        .map(|range| range.mMaximum)
        .reduce(f64::max);
    if let (Some(lowest), Some(highest)) = (lowest, highest)
        && !(lowest..=highest).contains(&f64::from(bitrate))
    {
        let layout = if channels == 1 { "mono" } else { "stereo" };
        return Err(MediaError::InvalidSettings {
            key: "bitrate".to_owned(),
            message: format!(
                "{layout} AAC at {rate} Hz takes {lowest} to {highest} bit/s, not {bitrate}"
            ),
        });
    }
    converter.set(kAudioConverterEncodeBitRate, "EncodeBitRate", bitrate)?;

    let prime = converter.value(
        kAudioConverterPrimeInfo,
        "PrimeInfo",
        AudioConverterPrimeInfo {
            leadingFrames: 0,
            trailingFrames: 0,
        },
    )?;
    let max_packet = converter.value(
        kAudioConverterPropertyMaximumOutputPacketSize,
        "MaximumOutputPacketSize",
        0_u32,
    )?;
    if max_packet == 0 {
        return Err(MediaError::Codec(
            "AudioToolbox gave no AAC packet size".to_owned(),
        ));
    }
    let cookie = converter.list(
        kAudioConverterCompressionMagicCookie,
        "CompressionMagicCookie",
        0_u8,
    )?;
    let asc = esds::asc_from_cookie(&cookie)?;
    let config = TrackConfig {
        codec: esds::aac_codec_string(&asc)?,
        description: Some(asc),
    };
    let delay = prime.leadingFrames;
    let session = AudioSession {
        converter,
        input: Input {
            channels: usize::from(channels),
            pending: Vec::new(),
            lent: Vec::new(),
            ended: false,
        },
        channels,
        max_packet: max_packet as usize,
        delay: i64::from(delay),
        first: None,
        frames: 0,
        packets: 0,
        config: Some(config),
    };
    Ok((session, rate, channels, u64::from(delay)))
}

impl AudioSession {
    /// Encodes interleaved `samples`, the first at `pts` in sample frames:
    /// the packets the converter makes of them (it keeps what does not fill
    /// a packet yet), and the config with the first.
    pub(crate) fn encode(&mut self, pts: i64, samples: &[f32]) -> Result<Encoded, MediaError> {
        let channels = usize::from(self.channels);
        if !samples.len().is_multiple_of(channels) {
            return Err(MediaError::Codec(format!(
                "{} samples are not whole frames of {channels} channels",
                samples.len()
            )));
        }
        let next = *self.first.get_or_insert(pts) + self.frames;
        if pts != next {
            return Err(MediaError::Codec(format!(
                "audio at {pts} does not follow the audio before it, which ends at {next}"
            )));
        }
        self.input.pending.extend_from_slice(samples);
        self.frames += (samples.len() / channels) as i64;
        let packets = self.fill()?;
        let config = if packets.is_empty() {
            None
        } else {
            self.config.take()
        };
        Ok(Encoded { packets, config })
    }

    /// Ends the stream: the converter's last packets, the one covering the
    /// last sample lasting only to it (none after it), and the config if no
    /// packets carried it yet.
    pub(crate) fn finish(mut self) -> Result<Encoded, MediaError> {
        self.input.ended = true;
        let mut packets = self.fill()?;
        let end = self.first.unwrap_or(0) + self.frames;
        packets.retain(|packet| packet.pts < end);
        if let Some(last) = packets.last_mut() {
            last.duration = last.duration.min(end - last.pts);
        }
        Ok(Encoded {
            packets,
            config: self.config.take(),
        })
    }

    /// Every packet the converter makes of the pending samples: until the
    /// input callback runs dry, or, once the stream has ended, until the
    /// converter has flushed.
    fn fill(&mut self) -> Result<Vec<Packet>, MediaError> {
        let mut packets = Vec::new();
        let mut buffer = vec![0_u8; self.max_packet * PACKETS_PER_FILL as usize];
        let empty = AudioStreamPacketDescription {
            mStartOffset: 0,
            mVariableFramesInPacket: 0,
            mDataByteSize: 0,
        };
        let mut descriptions = vec![empty; PACKETS_PER_FILL as usize];
        loop {
            let mut count = PACKETS_PER_FILL;
            let mut list = AudioBufferList {
                mNumberBuffers: 1,
                mBuffers: [AudioBuffer {
                    mNumberChannels: u32::from(self.channels),
                    mDataByteSize: buffer.len() as u32,
                    mData: buffer.as_mut_ptr().cast(),
                }],
            };
            // SAFETY: the converter is valid; `supply` is the callback for
            // `&mut self.input`, which nothing else touches during the call;
            // `list` points at `buffer`, which holds `PACKETS_PER_FILL`
            // packets of the largest size, and `descriptions` has room for
            // that many descriptions.
            let status = unsafe {
                AudioConverterFillComplexBuffer(
                    self.converter.raw(),
                    Some(supply),
                    (&raw mut self.input).cast(),
                    NonNull::from(&mut count),
                    NonNull::from(&mut list),
                    descriptions.as_mut_ptr(),
                )
            };
            if status != 0 && status != NO_DATA_YET {
                return Err(failed("AudioConverterFillComplexBuffer", status));
            }
            let made = descriptions.get(..count as usize).ok_or_else(|| {
                MediaError::Codec(format!(
                    "AudioToolbox made {count} packets of {PACKETS_PER_FILL}"
                ))
            })?;
            for description in made {
                let start = usize::try_from(description.mStartOffset).unwrap_or(usize::MAX);
                let bytes = start
                    .checked_add(description.mDataByteSize as usize)
                    .and_then(|end| buffer.get(start..end))
                    .ok_or_else(|| {
                        MediaError::Codec(
                            "AudioToolbox described a packet outside its buffer".to_owned(),
                        )
                    })?;
                let pts = self.first.unwrap_or(0) + FRAMES_PER_PACKET * self.packets - self.delay;
                self.packets += 1;
                packets.push(Packet {
                    pts,
                    dts: pts,
                    duration: FRAMES_PER_PACKET,
                    key: true,
                    bytes: bytes.to_vec(),
                    additional: Vec::new(),
                });
            }
            if status == NO_DATA_YET || count == 0 {
                return Ok(packets);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rate_audiotoolbox_does_not_list_falls_back_to_48_khz() {
        let rates = [44_100, 48_000];
        assert_eq!(sample_rate(44_100, &rates), 44_100);
        assert_eq!(sample_rate(96_000, &rates), 48_000);
        assert_eq!(sample_rate(0, &rates), 48_000);
        assert_eq!(sample_rate(44_100, &[]), 48_000);
    }
}
