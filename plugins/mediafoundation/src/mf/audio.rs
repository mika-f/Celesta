//! The AAC encoder, on Microsoft's AAC encoder transform: interleaved `f32`
//! samples in (converted to 16-bit PCM), raw AAC-LC out, 1024 sample frames
//! a packet.
//!
//! Times (the Decisions' AAC rule), counted by the plugin rather than taken
//! from the transform's sample times:
//! - The transform primes nothing, but it loses up to the first ~700 frames
//!   it is given (Task 1). So [`open`] feeds it [`AAC_LEAD_IN`] frames of
//!   silence before the mix, and answers that as the delay readers hide
//!   ([`AAC_DELAY`]): the mix's frame 0 is the transform's frame 1024.
//! - The `k`-th packet starts `1024 k` frames after the first sample given
//!   less the delay, and lasts 1024 frames; the last lasts only the frames
//!   it covers, so the muxer trims the end.

use crate::aac_user_data::{aac_codec_string, asc_from_user_data};
use crate::components::{AudioCodec, aac_bitrate, checked_audio_settings};
use crate::hns::to_hns;
use crate::mf::media_type::{aac_output, pcm_input};
use crate::mf::mft::{
    Transform, blob_attribute, candidates, check, first_accepting, friendly_name, make_sample,
    named, sample_bytes,
};
use celesta_media::{AudioEncoderRequest, Encoded, MediaError, Packet, TrackConfig};
use windows::Win32::Media::MediaFoundation::{
    IMFAttributes, MF_MT_USER_DATA, MFAudioFormat_AAC, MFAudioFormat_PCM,
    MFT_CATEGORY_AUDIO_ENCODER,
};
use windows_core::Interface;

/// The frames of silence fed before the mix.
pub(crate) const AAC_LEAD_IN: u64 = 1024;

/// The delay `aac` answers: the lead-in, which the transform itself does
/// not prime (Task 1).
pub(crate) const AAC_DELAY: u64 = AAC_LEAD_IN;

/// The sample frames in each AAC-LC packet.
const FRAMES_PER_PACKET: i64 = 1024;

/// The most frames one input sample holds.
const MAX_FRAMES: usize = 1024 * 16;

/// The rate the encoder takes when the mix's is not one it does.
const FALLBACK_RATE: u32 = 48_000;

/// An open AAC encoder.
pub(crate) struct AudioSession {
    transform: Transform,
    rate: u32,
    channels: u16,
    /// The `pts` of the first mix frame given; none before any.
    first: Option<i64>,
    /// The mix's frames given.
    frames: i64,
    /// Every frame fed to the transform, the lead-in's included.
    fed: i64,
    /// The packets made.
    packets: i64,
    /// Packets the lead-in made, kept until the first frame's `pts` is
    /// known.
    held: Vec<Vec<u8>>,
    /// The track's config, until the first packets (or the finish) carry it.
    config: Option<TrackConfig>,
}

// SAFETY: as `VideoSession`: the transform is a Media Foundation object of
// the multithreaded apartment `MfCodec::new` joins, a session is only
// reached through `&mut self`, and the rest is plain owned data.
unsafe impl Send for AudioSession {}

/// The rate `aac` takes for a mix at `asked`: the mix's when it is 44100 or
/// 48000, else 48000.
fn answered_rate(asked: u32) -> u32 {
    if matches!(asked, 44_100 | 48_000) {
        asked
    } else {
        FALLBACK_RATE
    }
}

/// `x` as a 16-bit sample: `round(clamp(x, -1, 1) * 32767)`.
fn to_i16(x: f32) -> i16 {
    (x.clamp(-1.0, 1.0) * 32767.0).round() as i16
}

/// Opens the AAC encoder for `request`, at the mix's rate when it is 44100
/// or 48000, else 48 kHz, and the mix's channel count (mono or stereo): the
/// session, its rate, channels, and delay.
pub(crate) fn open(
    request: &AudioEncoderRequest<'_>,
) -> Result<(AudioSession, u32, u16, u64), MediaError> {
    let settings = checked_audio_settings(AudioCodec::Aac, request.settings)?;
    let bitrate = aac_bitrate(&settings);
    let rate = answered_rate(request.sample_rate);
    let channels: u16 = if request.channels == 1 { 1 } else { 2 };
    // The AAC encoder is Microsoft's software one (Task 1: the hardware
    // flags list the same transform).
    let listed = candidates(
        MFT_CATEGORY_AUDIO_ENCODER,
        MFAudioFormat_PCM,
        MFAudioFormat_AAC,
        false,
    )?;
    if listed.is_empty() {
        return Err(MediaError::ComponentUnavailable {
            component: AudioCodec::Aac.id().to_owned(),
            reason: AudioCodec::Aac.missing_reason().to_owned(),
        });
    }
    let transform = first_accepting(AudioCodec::Aac.id(), &listed, |activate| {
        let try_one = || {
            let mut transform = Transform::activate(activate)?;
            let output = aac_output(rate, u32::from(channels), bitrate)?;
            let input = pcm_input(rate, u32::from(channels))?;
            transform.set_types(&output, &input)?;
            Ok(transform)
        };
        try_one().map_err(|error| named(&friendly_name(activate), error))
    })?;
    let config = config_of(&transform)?;
    let mut session = AudioSession {
        transform,
        rate,
        channels,
        first: None,
        frames: 0,
        fed: 0,
        packets: 0,
        held: Vec::new(),
        config: Some(config),
    };
    let silence = vec![0; AAC_LEAD_IN as usize * usize::from(channels)];
    session.held = session.feed(&silence)?;
    Ok((session, rate, channels, AAC_DELAY))
}

/// The track's config from `transform`'s output type: `mp4a.40.2` and the
/// AudioSpecificConfig at the end of its `MF_MT_USER_DATA`.
fn config_of(transform: &Transform) -> Result<TrackConfig, MediaError> {
    let output_type = transform.output_type()?;
    let attributes: IMFAttributes = check("QueryInterface(IMFAttributes)", output_type.cast())?;
    let user_data = blob_attribute(&attributes, &MF_MT_USER_DATA)
        .ok_or_else(|| MediaError::Codec(format!("{} gave no AAC user data", transform.name())))?;
    let asc = asc_from_user_data(&user_data)?;
    Ok(TrackConfig {
        codec: aac_codec_string(&asc)?,
        description: Some(asc),
    })
}

impl AudioSession {
    /// Feeds the transform interleaved 16-bit `samples` (whole frames, at
    /// most [`MAX_FRAMES`]): the bytes of the packets it made.
    fn feed(&mut self, samples: &[i16]) -> Result<Vec<Vec<u8>>, MediaError> {
        let frames = (samples.len() / usize::from(self.channels)) as i64;
        let bytes: Vec<u8> = samples
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect();
        let sample = make_sample(
            &bytes,
            to_hns(self.fed, self.rate),
            to_hns(frames, self.rate),
        )?;
        self.fed += frames;
        let outputs = self.transform.push(&sample)?;
        outputs.iter().map(sample_bytes).collect()
    }

    /// Encodes interleaved `samples`, the first at `pts` in sample frames:
    /// the packets that are ready, and the config with the first.
    pub(crate) fn encode(&mut self, pts: i64, samples: &[f32]) -> Result<Encoded, MediaError> {
        let channels = usize::from(self.channels);
        if !samples.len().is_multiple_of(channels) {
            return Err(MediaError::Codec(format!(
                "{} samples are not whole frames of {channels} channels",
                samples.len()
            )));
        }
        let next = self.first.map_or(pts, |first| first + self.frames);
        if pts != next {
            return Err(MediaError::Codec(format!(
                "audio at {pts} does not follow the audio before it, which ends at {next}"
            )));
        }
        self.first.get_or_insert(pts);
        let mut made = std::mem::take(&mut self.held);
        for chunk in samples.chunks(MAX_FRAMES * channels) {
            let pcm: Vec<i16> = chunk.iter().copied().map(to_i16).collect();
            made.extend(self.feed(&pcm)?);
            self.frames += (chunk.len() / channels) as i64;
        }
        let packets = self.packets_of(made)?;
        let config = if packets.is_empty() {
            None
        } else {
            self.config.take()
        };
        Ok(Encoded { packets, config })
    }

    /// Drains the transform: the last packets, the one covering the last
    /// frame lasting only to it (none after it), and the config if no
    /// packets carried it yet. The session ends.
    pub(crate) fn finish(mut self) -> Result<Encoded, MediaError> {
        let mut made = std::mem::take(&mut self.held);
        for output in self.transform.drain()? {
            made.push(sample_bytes(&output)?);
        }
        let mut packets = self.packets_of(made)?;
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

    /// `made`, the bytes of packets in order, as packets.
    fn packets_of(&mut self, made: Vec<Vec<u8>>) -> Result<Vec<Packet>, MediaError> {
        let first = self.first.unwrap_or(0);
        made.into_iter()
            .map(|bytes| {
                if bytes.is_empty() {
                    return Err(MediaError::Codec(format!(
                        "{} gave an empty packet",
                        self.transform.name()
                    )));
                }
                let pts = first + FRAMES_PER_PACKET * self.packets - AAC_DELAY as i64;
                self.packets += 1;
                Ok(Packet {
                    pts,
                    dts: pts,
                    duration: FRAMES_PER_PACKET,
                    key: true,
                    bytes,
                    additional: Vec::new(),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rate_answered_is_the_mixs_or_48_khz() {
        assert_eq!(answered_rate(44_100), 44_100);
        assert_eq!(answered_rate(48_000), 48_000);
        for asked in [0, 8_000, 32_000, 88_200, 96_000] {
            assert_eq!(answered_rate(asked), 48_000);
        }
    }

    #[test]
    fn samples_round_and_saturate() {
        assert_eq!(to_i16(0.0), 0);
        assert_eq!(to_i16(1.0), 32767);
        assert_eq!(to_i16(-1.0), -32767);
        assert_eq!(to_i16(2.0), 32767);
        assert_eq!(to_i16(-2.0), -32767);
        assert_eq!(to_i16(f32::INFINITY), 32767);
        assert_eq!(to_i16(f32::NAN), 0);
        // Rounded, not truncated: 0.5 / 32767 is half a code.
        assert_eq!(to_i16(1.4 / 32767.0), 1);
        assert_eq!(to_i16(-1.6 / 32767.0), -2);
        assert_eq!(AAC_DELAY, AAC_LEAD_IN);
    }
}
