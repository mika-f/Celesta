//! The H.264 and HEVC encoders, on Media Foundation transforms.
//!
//! A session takes frames of the input [`choose_input`] chose (RGBA is
//! converted to NV12 or P010 first, `convert.rs`), each a memory-buffer
//! sample at its time in hns. Each output sample's Annex B access unit
//! becomes a packet of length-prefixed NAL units without delimiters or
//! parameter sets (`annexb.rs`); the first parameter sets give the track's
//! config and the stream's reorder depth.
//!
//! Times (the Decisions' DTS rule):
//! - `pts` is the sample time less the transform's own delay, the first
//!   output's time less the first input's (Task 1: Microsoft's software
//!   H.264 MFT delays every sample by its reorder depth when it reorders).
//! - `dts` is `MFSampleExtension_DecodeTimestamp` when the sample has it,
//!   else its `pts`, shifted back by the reorder depth whenever the stream
//!   may reorder, as P5's VideoToolbox encoders do.

use crate::annexb::{self, ParameterSets, split_sample};
use crate::color::check_color;
use crate::components::{
    CodecSetting, VideoCodec, checked_settings, choose_input, codec_settings, hardware,
};
use crate::convert::{rgba8_to_nv12, rgba16_to_p010};
use crate::hns::{from_hns, to_hns};
use crate::mf::media_type::{encoder_input, encoder_output, input_subtype};
use crate::mf::mft::{
    Transform, blob_attribute, candidates, check, first_accepting, friendly_name, make_sample,
    named, sample_bytes,
};
use crate::params::{
    UNKNOWN_REORDER_DEPTH, avc_codec_string, avc_reorder_depth, hevc_codec_string,
    hevc_reorder_depth,
};
use crate::vui::with_bt709;
use celesta_media::{Encoded, MediaError, Packet, PixelFormat, TrackConfig, VideoEncoderRequest};
use windows::Win32::Foundation::VARIANT_BOOL;
use windows::Win32::Media::MediaFoundation::{
    CODECAPI_AVEncCommonLowLatency, CODECAPI_AVEncCommonMeanBitRate, CODECAPI_AVEncCommonQuality,
    CODECAPI_AVEncCommonRateControlMode, CODECAPI_AVEncMPVDefaultBPictureCount,
    CODECAPI_AVEncMPVGOPSize, CODECAPI_AVEncVideoOutputColorNominalRange,
    CODECAPI_AVEncVideoOutputColorPrimaries, CODECAPI_AVEncVideoOutputColorTransferFunction,
    CODECAPI_AVEncVideoOutputColorTransferMatrix, ICodecAPI, IMFSample, MF_MT_MPEG_SEQUENCE_HEADER,
    MFNominalRange_16_235, MFSampleExtension_CleanPoint, MFSampleExtension_DecodeTimestamp,
    MFT_CATEGORY_VIDEO_ENCODER, MFVideoFormat_H264, MFVideoFormat_HEVC, MFVideoPrimaries_BT709,
    MFVideoTransFunc_709, MFVideoTransferMatrix_BT709, eAVEncCommonRateControlMode_Quality,
    eAVEncCommonRateControlMode_UnconstrainedVBR,
};
use windows::Win32::System::Variant::{VARIANT, VT_BOOL, VT_UI4};
use windows_core::{GUID, Interface};

/// The average bitrate the output type carries in quality mode, which the
/// transforms require.
const QUALITY_MODE_BITRATE: u32 = 8_000_000;

/// An open encoder transform.
pub(crate) struct VideoSession {
    transform: Transform,
    codec: VideoCodec,
    input: PixelFormat,
    width: u32,
    height: u32,
    timescale: u32,
    /// A frame's duration in the timescale.
    frame: i64,
    /// Whether the stream may reorder frames (`bFrames`, never Baseline).
    reorders: bool,
    /// The first parameter sets, once the first output gave them.
    sets: Option<ParameterSets>,
    /// How many frames decode times are shifted back by, once known.
    depth: i64,
    /// The first input's time, in hns.
    first_input: Option<i64>,
    /// The transform's own delay, in hns: the first output's time less the
    /// first input's.
    delay: Option<i64>,
    /// The last packet's decode time.
    last_dts: Option<i64>,
}

// SAFETY: the transform and its interfaces are Media Foundation objects
// of the multithreaded apartment `MfCodec::new` joins, which Media
// Foundation's transforms (free-threaded or "both") may be called from on
// any thread of it. A session is only reached through `&mut self`, so
// never from two threads at once; the program serves on its main thread.
unsafe impl Send for VideoSession {}

/// A `VT_UI4` variant of `value`.
fn variant_u32(value: u32) -> VARIANT {
    let mut variant = VARIANT::default();
    // SAFETY: writes the tag and the matching member of a default variant.
    unsafe {
        let inner = &mut *variant.Anonymous.Anonymous;
        inner.vt = VT_UI4;
        inner.Anonymous.ulVal = value;
    }
    variant
}

/// A `VT_BOOL` variant of `value`.
fn variant_bool(value: bool) -> VARIANT {
    let mut variant = VARIANT::default();
    // SAFETY: as above.
    unsafe {
        let inner = &mut *variant.Anonymous.Anonymous;
        inner.vt = VT_BOOL;
        inner.Anonymous.boolVal = VARIANT_BOOL(if value { -1 } else { 0 });
    }
    variant
}

/// Sets `api` to `value` on `codec`: whether it took it, an `IsSupported`
/// or `SetValue` that fails being a no.
fn set_value(codec: &ICodecAPI, api: &GUID, value: &VARIANT) -> bool {
    // SAFETY: plain COM calls; the GUID and variant outlive them.
    unsafe { codec.IsSupported(api).is_ok() && codec.SetValue(api, value).is_ok() }
}

/// The refusal of a transform `name` that has no `mode` of rate control
/// (the setting `key`): the user can choose the `other` mode, or, when the
/// transform is a GPU's and `hardware` is on, Microsoft's software encoder.
fn no_rate_control(
    key: &str,
    name: &str,
    mode: &str,
    other: &str,
    hint_hardware: bool,
) -> MediaError {
    let hardware = if hint_hardware {
        ", or turn Hardware off"
    } else {
        ""
    };
    MediaError::InvalidSettings {
        key: key.to_owned(),
        message: format!("{name} has no {mode} mode; choose {other}{hardware}"),
    }
}

/// Sets `values` on `transform`'s `ICodecAPI`, `hardware` being the
/// setting. A property it does not take is skipped (Task 1: Microsoft's
/// software MFT refuses `LowLatency`, NVIDIA's `BPictureCount` too), except
/// the rate-control mode, and the bitrate in bitrate mode, which refuse the
/// transform: the next candidate is tried (deviation 78).
fn apply_codec_settings(
    transform: &Transform,
    values: &[CodecSetting],
    hardware: bool,
) -> Result<(), MediaError> {
    let codec = transform.codec_api();
    let name = transform.name();
    let hint_hardware = hardware && transform.is_hardware();
    let takes = |api: &GUID, value: &VARIANT| {
        codec
            .as_ref()
            .is_some_and(|codec| set_value(codec, api, value))
    };
    for value in values {
        match *value {
            CodecSetting::RateControlQuality(quality) => {
                let mode = variant_u32(eAVEncCommonRateControlMode_Quality.0 as u32);
                if !takes(&CODECAPI_AVEncCommonRateControlMode, &mode) {
                    return Err(no_rate_control(
                        "rateControl",
                        name,
                        "constant quality",
                        "Average bitrate",
                        hint_hardware,
                    ));
                }
                takes(&CODECAPI_AVEncCommonQuality, &variant_u32(quality));
            }
            CodecSetting::RateControlMeanBitRate(bitrate) => {
                let mode = variant_u32(eAVEncCommonRateControlMode_UnconstrainedVBR.0 as u32);
                if !(takes(&CODECAPI_AVEncCommonRateControlMode, &mode)
                    && takes(&CODECAPI_AVEncCommonMeanBitRate, &variant_u32(bitrate)))
                {
                    return Err(no_rate_control(
                        "bitrate",
                        name,
                        "average bitrate",
                        "Constant quality",
                        hint_hardware,
                    ));
                }
            }
            CodecSetting::GopSize(frames) => {
                if let Some(codec) = &codec {
                    set_value(codec, &CODECAPI_AVEncMPVGOPSize, &variant_u32(frames));
                }
            }
            CodecSetting::BPictureCount(count) => {
                if let Some(codec) = &codec {
                    set_value(
                        codec,
                        &CODECAPI_AVEncMPVDefaultBPictureCount,
                        &variant_u32(count),
                    );
                }
            }
            CodecSetting::LowLatency(low) => {
                if let Some(codec) = &codec {
                    set_value(codec, &CODECAPI_AVEncCommonLowLatency, &variant_bool(low));
                }
            }
        }
    }
    // The colour the encoder writes into the stream's own headers (the SPS's
    // VUI), so a decoder that reads no container box decodes BT.709 limited
    // too. A transform that does not take them is skipped, as above.
    if let Some(codec) = &codec {
        for (api, value) in [
            (
                &CODECAPI_AVEncVideoOutputColorPrimaries,
                MFVideoPrimaries_BT709.0,
            ),
            (
                &CODECAPI_AVEncVideoOutputColorTransferFunction,
                MFVideoTransFunc_709.0,
            ),
            (
                &CODECAPI_AVEncVideoOutputColorTransferMatrix,
                MFVideoTransferMatrix_BT709.0,
            ),
            (
                &CODECAPI_AVEncVideoOutputColorNominalRange,
                MFNominalRange_16_235.0,
            ),
        ] {
            set_value(codec, api, &variant_u32(value as u32));
        }
    }
    Ok(())
}

/// Opens `codec` for `request`: the session and the input it chose.
pub(crate) fn open(
    codec: VideoCodec,
    request: &VideoEncoderRequest<'_>,
) -> Result<(VideoSession, PixelFormat), MediaError> {
    let settings = checked_settings(codec, request.settings)?;
    check_color(request.color)?;
    let (width, height) = (request.width, request.height);
    // Media Foundation encodes 4:2:0 whatever the input, so an odd size is
    // refused rather than cropped (P5's rule).
    if !(width.is_multiple_of(2) && height.is_multiple_of(2)) {
        return Err(MediaError::Codec(format!(
            "{} needs even dimensions, not {width}x{height}",
            codec.display_name()
        )));
    }
    let rate = request.frame_rate;
    if width == 0
        || height == 0
        || request.timescale == 0
        || rate.numerator == 0
        || rate.denominator == 0
    {
        return Err(MediaError::Codec(format!(
            "a {width}x{height} video at {}/{} fps in a timescale of {} has no frames to encode",
            rate.numerator, rate.denominator, request.timescale
        )));
    }
    let input = choose_input(codec, &settings, request.offered)?;
    let (profile, values) = codec_settings(codec, &settings)?;
    let bitrate = values
        .iter()
        .find_map(|value| match value {
            CodecSetting::RateControlMeanBitRate(bitrate) => Some(*bitrate),
            _ => None,
        })
        .unwrap_or(QUALITY_MODE_BITRATE);
    let reorders = values
        .iter()
        .any(|value| matches!(value, CodecSetting::BPictureCount(count) if *count > 0));
    let hardware = hardware(&settings);
    let output_subtype = match codec {
        VideoCodec::H264 => MFVideoFormat_H264,
        VideoCodec::Hevc => MFVideoFormat_HEVC,
    };
    let listed = candidates(
        MFT_CATEGORY_VIDEO_ENCODER,
        input_subtype(input),
        output_subtype,
        hardware,
    )?;
    if listed.is_empty() {
        let ten_bit = matches!(input, PixelFormat::P010 | PixelFormat::Rgba16);
        // Task 1: 10-bit HEVC is the usual case of the first, as no software
        // MFT takes P010.
        let reason = if !hardware {
            format!(
                "no software {} encoder; turn Hardware on",
                codec.display_name()
            )
        } else if ten_bit {
            "Media Foundation has no 10-bit HEVC encoder on this system; install a GPU driver with one"
                .to_owned()
        } else {
            codec.missing_reason().to_owned()
        };
        return Err(MediaError::ComponentUnavailable {
            component: codec.id().to_owned(),
            reason,
        });
    }
    // Each refusal names its transform, so the one kept says which refused.
    let transform = first_accepting(&listed, |activate| {
        let try_one = || {
            let mut transform = Transform::activate(activate)?;
            apply_codec_settings(&transform, &values, hardware)?;
            let output = encoder_output(codec, profile, width, height, rate, bitrate)?;
            let input_type = encoder_input(input, width, height, rate)?;
            transform.set_types(&output, &input_type)?;
            Ok(transform)
        };
        try_one().map_err(|error| named(&friendly_name(activate), error))
    })?;
    let session = VideoSession {
        transform,
        codec,
        input,
        width,
        height,
        timescale: request.timescale,
        frame: i64::from(rate.denominator),
        reorders,
        sets: None,
        depth: 0,
        first_input: None,
        delay: None,
        last_dts: None,
    };
    Ok((session, input))
}

impl VideoSession {
    /// The transform's friendly name.
    pub(crate) fn encoder_name(&self) -> &str {
        self.transform.name()
    }

    /// Encodes `frame`, a tightly packed frame of the session's input, shown
    /// at `pts`: the packets that are ready.
    pub(crate) fn encode(&mut self, pts: i64, frame: Vec<u8>) -> Result<Encoded, MediaError> {
        let expected = self.input.frame_bytes(self.width, self.height);
        if frame.len() != expected {
            return Err(MediaError::Codec(format!(
                "a {}x{} {} frame is {expected} bytes, not {}",
                self.width,
                self.height,
                self.input,
                frame.len()
            )));
        }
        let bytes = match self.input {
            PixelFormat::Rgba8 => rgba8_to_nv12(&frame, self.width, self.height),
            PixelFormat::Rgba16 => rgba16_to_p010(&frame, self.width, self.height),
            _ => frame,
        };
        let time = to_hns(pts, self.timescale);
        let sample = make_sample(&bytes, time, to_hns(self.frame, self.timescale))?;
        // Before the push: a transform may answer the first frame at once.
        self.first_input.get_or_insert(time);
        let outputs = self.transform.push(&sample)?;
        self.packets(outputs)
    }

    /// Drains the transform: the last packets. The session ends.
    pub(crate) fn finish(mut self) -> Result<Encoded, MediaError> {
        let outputs = self.transform.drain()?;
        self.packets(outputs)
    }

    /// `outputs` as packets, the track's config with the first.
    fn packets(&mut self, outputs: Vec<IMFSample>) -> Result<Encoded, MediaError> {
        let mut encoded = Encoded::default();
        for output in outputs {
            let packet = self.packet(&output, &mut encoded)?;
            encoded.packets.push(packet);
        }
        Ok(encoded)
    }

    /// The parameter sets of the output type's `MF_MT_MPEG_SEQUENCE_HEADER`,
    /// for a first sample that carries none.
    fn sequence_header_sets(&self) -> Result<Option<ParameterSets>, MediaError> {
        let output_type = self.transform.output_type()?;
        let header = blob_attribute(
            &check("QueryInterface(IMFAttributes)", output_type.cast())?,
            &MF_MT_MPEG_SEQUENCE_HEADER,
        );
        Ok(header.and_then(|header| split_sample(self.annexb_codec(), &header).0))
    }

    fn annexb_codec(&self) -> annexb::Codec {
        match self.codec {
            VideoCodec::H264 => annexb::Codec::H264,
            VideoCodec::Hevc => annexb::Codec::Hevc,
        }
    }

    /// The track's config and reorder depth, from the first parameter sets.
    fn configure(&mut self, sets: ParameterSets) -> Result<TrackConfig, MediaError> {
        // The record's SPS says BT.709 limited whatever the MFT wrote; `sets`
        // stays as it came, to compare the later ones with.
        let described = ParameterSets {
            sps: with_bt709(self.annexb_codec(), &sets.sps),
            ..sets.clone()
        };
        let (codec, depth, record) = match self.codec {
            VideoCodec::H264 => {
                let avcc = annexb::avcc_from(&described)?;
                (avc_codec_string(&avcc)?, avc_reorder_depth(&avcc)?, avcc)
            }
            VideoCodec::Hevc => {
                let hvcc = annexb::hvcc_from(&described)?;
                (hevc_codec_string(&hvcc)?, hevc_reorder_depth(&hvcc)?, hvcc)
            }
        };
        self.depth = if self.reorders {
            i64::from(depth.unwrap_or(UNKNOWN_REORDER_DEPTH))
        } else {
            0
        };
        self.sets = Some(sets);
        Ok(TrackConfig {
            codec,
            description: Some(record),
        })
    }

    /// `output` as a packet; the track's config goes into `encoded` with the
    /// first.
    fn packet(&mut self, output: &IMFSample, encoded: &mut Encoded) -> Result<Packet, MediaError> {
        let name = self.transform.name().to_owned();
        // SAFETY (each): plain COM calls on a live sample.
        let time = check("IMFSample::GetSampleTime", unsafe {
            output.GetSampleTime()
        })?;
        let decode_time = unsafe { output.GetUINT64(&MFSampleExtension_DecodeTimestamp) }
            .ok()
            .map(|time| time as i64);
        let key = unsafe { output.GetUINT32(&MFSampleExtension_CleanPoint) }
            .is_ok_and(|clean| clean != 0);
        let (sets, bytes) = split_sample(self.annexb_codec(), &sample_bytes(output)?);
        if bytes.is_empty() {
            return Err(MediaError::Codec(format!("{name} gave an empty frame")));
        }
        match (&self.sets, sets) {
            (None, sets) => {
                let sets = match sets {
                    Some(sets) => sets,
                    None => self.sequence_header_sets()?.ok_or_else(|| {
                        MediaError::Codec(format!(
                            "{name} gave no parameter sets with its first frame"
                        ))
                    })?,
                };
                encoded.config = Some(self.configure(sets)?);
            }
            (Some(first), Some(sets)) if *first != sets => {
                return Err(MediaError::Codec(format!(
                    "{name} changed its parameter sets mid-stream"
                )));
            }
            (Some(_), _) => {}
        }
        let first_input = self.first_input.unwrap_or(0);
        let delay = *self.delay.get_or_insert(time - first_input);
        let shown = time - delay;
        let pts = from_hns(shown, self.timescale);
        let depth = self.depth;
        let dts = from_hns(decode_time.unwrap_or(shown), self.timescale) - depth * self.frame;
        if dts > pts {
            return Err(MediaError::Codec(format!(
                "{name} gave a frame shown at {pts} that decodes at {dts}, after it is shown, \
                 even with a reorder depth of {depth} frames"
            )));
        }
        if let Some(last) = self.last_dts
            && dts <= last
        {
            return Err(MediaError::Codec(format!(
                "{name} gave a frame that decodes at {dts}, not after the one before it at \
                 {last}, with a reorder depth of {depth} frames"
            )));
        }
        self.last_dts = Some(dts);
        Ok(Packet {
            pts,
            dts,
            duration: self.frame,
            key,
            bytes,
            additional: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refusal(error: MediaError) -> (String, String) {
        match error {
            MediaError::InvalidSettings { key, message } => (key, message),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn rate_control_refusals_say_what_to_change() {
        assert_eq!(
            refusal(no_rate_control(
                "rateControl",
                "NVIDIA H.264 Encoder MFT",
                "constant quality",
                "Average bitrate",
                true
            )),
            (
                "rateControl".to_owned(),
                "NVIDIA H.264 Encoder MFT has no constant quality mode; choose Average bitrate, or turn Hardware off".to_owned()
            )
        );
        // A software transform, or Hardware already off: no hardware hint.
        assert_eq!(
            refusal(no_rate_control(
                "bitrate",
                "H264 Encoder MFT",
                "average bitrate",
                "Constant quality",
                false
            )),
            (
                "bitrate".to_owned(),
                "H264 Encoder MFT has no average bitrate mode; choose Constant quality".to_owned()
            )
        );
    }
}
