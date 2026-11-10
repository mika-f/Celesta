//! The bitstream between Media Foundation's Annex B and a track's samples:
//! splitting an access unit into NAL units, taking the parameter sets out
//! of it, and building and reading `avcC` and `hvcC` records
//! (ISO/IEC 14496-15 §5.3.3 and §8.3.3).

use celesta_media::MediaError;

use crate::params::{avc_sps_format, hevc_sps_info, malformed};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Codec {
    H264,
    Hevc,
}

impl Codec {
    /// What a NAL unit is, from its header: H.264's type is the low 5 bits
    /// of the first byte, HEVC's the 6 after the first bit.
    fn unit_type(self, unit: &[u8]) -> u8 {
        match self {
            Self::H264 => unit[0] & 0x1F,
            Self::Hevc => unit[0] >> 1 & 0x3F,
        }
    }

    /// The types dropped from a sample: access unit delimiters, and VPS
    /// (HEVC), SPS, and PPS.
    fn is_dropped(self, kind: u8) -> bool {
        match self {
            Self::H264 => matches!(kind, 7..=9),
            Self::Hevc => matches!(kind, 32..=35),
        }
    }
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "for a muxer, which Task 1's ruling B leaves unbuilt; only its tests use it"
    )
)]
const START_CODE: [u8; 4] = [0, 0, 0, 1];

/// The NAL units of an Annex B stream, without their start codes (`00 00
/// 01` or `00 00 00 01`) or the zero bytes before the next one. Bytes
/// before the first start code are not a unit.
pub(crate) fn nal_units(annexb: &[u8]) -> Vec<&[u8]> {
    // The offsets just past each `00 00 01`.
    let mut starts = Vec::new();
    let mut at = 0;
    while at + 3 <= annexb.len() {
        if annexb[at..at + 3] == [0, 0, 1] {
            starts.push(at + 3);
            at += 3;
        } else {
            at += 1;
        }
    }
    let mut units = Vec::with_capacity(starts.len());
    for (index, &start) in starts.iter().enumerate() {
        let mut end = match starts.get(index + 1) {
            Some(&next) => next - 3,
            None => annexb.len(),
        };
        // A NAL unit never ends in zero: the zeros belong to the next start
        // code (or are trailing_zero_8bits).
        while end > start && annexb[end - 1] == 0 {
            end -= 1;
        }
        if end > start {
            units.push(&annexb[start..end]);
        }
    }
    units
}

/// The parameter sets of a stream, each a NAL unit with its header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ParameterSets {
    pub vps: Option<Vec<u8>>,
    pub sps: Vec<u8>,
    pub pps: Vec<u8>,
}

/// The first parameter sets in an Annex B access unit (None unless it has
/// both an SPS and a PPS), and the access unit as a sample: each remaining
/// NAL unit behind its 4-byte big-endian length, without delimiters or
/// parameter sets.
pub(crate) fn split_sample(codec: Codec, annexb: &[u8]) -> (Option<ParameterSets>, Vec<u8>) {
    let (sps_type, pps_type, vps_type) = match codec {
        Codec::H264 => (7, 8, None),
        Codec::Hevc => (33, 34, Some(32)),
    };
    let (mut vps, mut sps, mut pps) = (None, None, None);
    let mut sample = Vec::with_capacity(annexb.len());
    for unit in nal_units(annexb) {
        let kind = codec.unit_type(unit);
        if Some(kind) == vps_type {
            vps.get_or_insert_with(|| unit.to_vec());
        } else if kind == sps_type {
            sps.get_or_insert_with(|| unit.to_vec());
        } else if kind == pps_type {
            pps.get_or_insert_with(|| unit.to_vec());
        }
        if !codec.is_dropped(kind) {
            sample.extend_from_slice(&(unit.len() as u32).to_be_bytes());
            sample.extend_from_slice(unit);
        }
    }
    let sets = match (sps, pps) {
        (Some(sps), Some(pps)) => Some(ParameterSets { vps, sps, pps }),
        _ => None,
    };
    (sets, sample)
}

/// `u16` length, then `unit`.
fn push_unit(record: &mut Vec<u8>, unit: &[u8]) -> Result<(), MediaError> {
    let length = u16::try_from(unit.len()).map_err(|_| malformed("parameter set"))?;
    record.extend_from_slice(&length.to_be_bytes());
    record.extend_from_slice(unit);
    Ok(())
}

/// An `avcC` record: version 1, the profile, compatibility, and level from
/// the SPS's bytes 1 to 3, 4-byte lengths, and one SPS and one PPS, then,
/// for the high profiles, the chroma format and bit depths the standard
/// repeats there and no SPS extensions.
pub(crate) fn avcc_from(sets: &ParameterSets) -> Result<Vec<u8>, MediaError> {
    if sets.sps.len() < 4 || sets.pps.is_empty() {
        return Err(malformed("H.264 parameter set"));
    }
    let mut record = vec![
        1,
        sets.sps[1],
        sets.sps[2],
        sets.sps[3],
        0xFC | 3, // reserved, lengthSizeMinusOne
        0xE0 | 1, // reserved, numOfSequenceParameterSets
    ];
    push_unit(&mut record, &sets.sps)?;
    record.push(1); // numOfPictureParameterSets
    push_unit(&mut record, &sets.pps)?;
    if matches!(sets.sps[1], 100 | 110 | 122 | 144) {
        let (chroma_format, luma, chroma) =
            avc_sps_format(&sets.sps).ok_or_else(|| malformed("H.264 parameter set"))?;
        record.extend_from_slice(&[
            0xFC | chroma_format & 3,
            0xF8 | luma & 7,
            0xF8 | chroma & 7,
            0,
        ]);
    }
    Ok(record)
}

/// An `hvcC` record: the profile, tier, level, chroma format, bit depths,
/// and temporal layers the SPS gives, 4-byte lengths, and complete arrays
/// of the VPS, SPS, and PPS. What the SPS does not give (frame rate,
/// spatial segmentation, parallelism) is the unspecified 0.
pub(crate) fn hvcc_from(sets: &ParameterSets) -> Result<Vec<u8>, MediaError> {
    let bad = || malformed("HEVC parameter set");
    let vps = sets.vps.as_ref().ok_or_else(bad)?;
    let info = hevc_sps_info(&sets.sps).ok_or_else(bad)?;
    let mut record = vec![1, info.profile];
    record.extend_from_slice(&info.compatibility);
    record.extend_from_slice(&info.constraints);
    record.push(info.level);
    record.extend_from_slice(&[0xF0, 0x00]); // min_spatial_segmentation_idc
    record.push(0xFC); // parallelismType
    record.push(0xFC | info.chroma_format & 3);
    record.push(0xF8 | info.bit_depth_luma_minus8 & 7);
    record.push(0xF8 | info.bit_depth_chroma_minus8 & 7);
    record.extend_from_slice(&[0, 0]); // avgFrameRate
    // constantFrameRate, numTemporalLayers, temporalIdNested, lengthSizeMinusOne.
    record.push((info.temporal_layers & 7) << 3 | u8::from(info.temporal_id_nesting) << 2 | 3);
    record.push(3); // numOfArrays
    for (kind, unit) in [(32u8, vps), (33, &sets.sps), (34, &sets.pps)] {
        record.push(0x80 | kind); // array_completeness
        record.extend_from_slice(&1u16.to_be_bytes());
        push_unit(&mut record, unit)?;
    }
    Ok(record)
}

/// Every parameter set in an `avcC` or `hvcC` record, in order, each a NAL
/// unit with its header.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "for a muxer, which Task 1's ruling B leaves unbuilt; only its tests use it"
    )
)]
pub(crate) fn record_units(codec: Codec, record: &[u8]) -> Result<Vec<&[u8]>, MediaError> {
    let what = match codec {
        Codec::H264 => "avcC",
        Codec::Hevc => "hvcC",
    };
    let bad = || malformed(what);
    // The length-prefixed unit at `at`, and where the next thing starts.
    let take = |at: usize| -> Result<(&[u8], usize), MediaError> {
        let length = usize::from(u16::from_be_bytes([
            *record.get(at).ok_or_else(bad)?,
            *record.get(at + 1).ok_or_else(bad)?,
        ]));
        let unit = record.get(at + 2..at + 2 + length).ok_or_else(bad)?;
        if unit.is_empty() {
            return Err(bad());
        }
        Ok((unit, at + 2 + length))
    };
    let mut units = Vec::new();
    match codec {
        Codec::H264 => {
            let sps_count = usize::from(*record.get(5).ok_or_else(bad)? & 0x1F);
            let mut at = 6;
            for _ in 0..sps_count {
                let (unit, next) = take(at)?;
                units.push(unit);
                at = next;
            }
            let pps_count = usize::from(*record.get(at).ok_or_else(bad)?);
            at += 1;
            for _ in 0..pps_count {
                let (unit, next) = take(at)?;
                units.push(unit);
                at = next;
            }
        }
        Codec::Hevc => {
            let arrays = *record.get(22).ok_or_else(bad)?;
            let mut at = 23;
            for _ in 0..arrays {
                // array_completeness and NAL_unit_type, then numNalus.
                let count = usize::from(u16::from_be_bytes([
                    *record.get(at + 1).ok_or_else(bad)?,
                    *record.get(at + 2).ok_or_else(bad)?,
                ]));
                at += 3;
                for _ in 0..count {
                    let (unit, next) = take(at)?;
                    units.push(unit);
                    at = next;
                }
            }
        }
    }
    Ok(units)
}

/// The parameter sets of an `avcC` or `hvcC` record as Annex B, each behind
/// a 4-byte start code: the encoder's sequence header.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "for a muxer, which Task 1's ruling B leaves unbuilt; only its tests use it"
    )
)]
pub(crate) fn sequence_header(codec: Codec, record: &[u8]) -> Result<Vec<u8>, MediaError> {
    let mut annexb = Vec::new();
    for unit in record_units(codec, record)? {
        annexb.extend_from_slice(&START_CODE);
        annexb.extend_from_slice(unit);
    }
    Ok(annexb)
}

/// A sample of 4-byte length-prefixed NAL units as Annex B, each behind a
/// 4-byte start code.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "for a muxer, which Task 1's ruling B leaves unbuilt; only its tests use it"
    )
)]
pub(crate) fn to_annexb(sample: &[u8]) -> Result<Vec<u8>, MediaError> {
    let bad = || malformed("sample");
    let mut annexb = Vec::with_capacity(sample.len());
    let mut rest = sample;
    while !rest.is_empty() {
        let (prefix, after) = rest.split_at_checked(4).ok_or_else(bad)?;
        let length = u32::from_be_bytes([prefix[0], prefix[1], prefix[2], prefix[3]]) as usize;
        if length == 0 {
            return Err(bad());
        }
        let (unit, after) = after.split_at_checked(length).ok_or_else(bad)?;
        annexb.extend_from_slice(&START_CODE);
        annexb.extend_from_slice(unit);
        rest = after;
    }
    Ok(annexb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::{
        avc_codec_string, avc_reorder_depth, hevc_codec_string, hevc_reorder_depth,
    };

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
            .collect()
    }

    // P5's vectors (VideoToolbox's records).
    const AVCC: &str = "0164000dffe1000f2764000dac56505067e79a8080808101000428ee3cb0fdf8f800";
    const HVCC: &str = "010220000000b000000000003cf000fcfdfafa00000b03a00001001840010c01ffff022000000300b0000003000003003c15c090a100010023420101022000000300b0000003000003003ca00a080c1f3b62057b9165535010101008a2000100084401c02835182992";

    fn avc_sets() -> ParameterSets {
        let avcc = hex(AVCC);
        let units = record_units(Codec::H264, &avcc).unwrap();
        ParameterSets {
            vps: None,
            sps: units[0].to_vec(),
            pps: units[1].to_vec(),
        }
    }

    fn hevc_sets() -> ParameterSets {
        let hvcc = hex(HVCC);
        let units = record_units(Codec::Hevc, &hvcc).unwrap();
        assert_eq!(units.len(), 3);
        ParameterSets {
            vps: Some(units[0].to_vec()),
            sps: units[1].to_vec(),
            pps: units[2].to_vec(),
        }
    }

    #[test]
    fn splits_and_strips() {
        let sets = avc_sets();
        let idr = [0x65, 0x88, 0x84, 0x21, 0xA0, 0x00, 0x00, 0x03, 0x01];
        let mut annexb = vec![0, 0, 0, 1, 0x09, 0xF0]; // AUD, 4-byte code
        annexb.extend([0, 0, 1]); // SPS, 3-byte
        annexb.extend(&sets.sps);
        annexb.extend([0, 0, 0, 1]); // PPS, 4-byte
        annexb.extend(&sets.pps);
        annexb.extend([0, 0, 1]); // IDR, 3-byte
        annexb.extend(idr);
        let (found, sample) = split_sample(Codec::H264, &annexb);
        assert_eq!(found, Some(sets));
        let mut expected = (idr.len() as u32).to_be_bytes().to_vec();
        expected.extend(idr);
        assert_eq!(sample, expected);
    }

    #[test]
    fn nal_units_find_both_start_codes() {
        let annexb = [0, 0, 0, 1, 0x41, 1, 0, 0, 1, 0x42, 2, 0, 0, 0, 0, 1, 0x43];
        let units: [&[u8]; 3] = [&[0x41, 1], &[0x42, 2], &[0x43]];
        assert_eq!(nal_units(&annexb), units);
        assert!(nal_units(&[]).is_empty());
        assert!(nal_units(&[1, 2, 3]).is_empty());
    }

    #[test]
    fn a_sample_without_parameter_sets_gives_none() {
        let annexb = [0, 0, 0, 1, 0x41, 0x9A, 0x01];
        let (found, sample) = split_sample(Codec::H264, &annexb);
        assert_eq!(found, None);
        assert_eq!(sample, [0, 0, 0, 3, 0x41, 0x9A, 0x01]);
    }

    #[test]
    fn hevc_drops_its_parameter_sets_and_delimiter() {
        let sets = hevc_sets();
        let slice = [0x26, 0x01, 0xAF, 0x08];
        let mut annexb = vec![0, 0, 0, 1, 0x46, 0x01, 0x50]; // AUD (35)
        for unit in [sets.vps.as_ref().unwrap(), &sets.sps, &sets.pps] {
            annexb.extend([0, 0, 0, 1]);
            annexb.extend(unit);
        }
        annexb.extend([0, 0, 1, 0x4E, 0x01, 0x05]); // prefix SEI (39), kept
        annexb.extend([0, 0, 1]);
        annexb.extend(slice);
        let (found, sample) = split_sample(Codec::Hevc, &annexb);
        assert_eq!(found, Some(sets));
        assert_eq!(
            sample,
            [
                0, 0, 0, 3, 0x4E, 0x01, 0x05, 0, 0, 0, 4, 0x26, 0x01, 0xAF, 0x08
            ]
        );
    }

    #[test]
    fn avcc_round_trips() {
        let avcc = hex(AVCC);
        let sets = avc_sets();
        assert_eq!(avcc_from(&sets).unwrap(), avcc);
        let mut expected = START_CODE.to_vec();
        expected.extend(&sets.sps);
        expected.extend(START_CODE);
        expected.extend(&sets.pps);
        assert_eq!(sequence_header(Codec::H264, &avcc).unwrap(), expected);
        assert_eq!(avc_codec_string(&avcc).unwrap(), "avc1.64000D");
        assert_eq!(avc_reorder_depth(&avcc).unwrap(), None);
    }

    #[test]
    fn hvcc_from_parameter_sets() {
        let sets = hevc_sets();
        let made = hvcc_from(&sets).unwrap();
        assert_eq!(hevc_codec_string(&made).unwrap(), "hvc1.2.4.L60.B0");
        assert_eq!(hevc_reorder_depth(&made).unwrap(), Some(2));
        // The 10-bit 4:2:0 stream with one temporal layer.
        assert_eq!(made[16..19], [0xFD, 0xFA, 0xFA]);
        // temporalIdNested follows the SPS's flag, which VideoToolbox's record leaves 0.
        assert_eq!(made[21], 0x0F);
        // And the parameter sets come back out of it.
        let units = record_units(Codec::Hevc, &made).unwrap();
        assert_eq!(units[0], sets.vps.as_deref().unwrap());
        assert_eq!(units[1], sets.sps);
        assert_eq!(units[2], sets.pps);
    }

    #[test]
    fn hevc_sequence_header_is_annexb() {
        let sets = hevc_sets();
        let header = sequence_header(Codec::Hevc, &hex(HVCC)).unwrap();
        let (found, sample) = split_sample(Codec::Hevc, &header);
        assert_eq!(found, Some(sets));
        assert!(sample.is_empty());
    }

    #[test]
    fn to_annexb_inverts_split() {
        let units: [&[u8]; 3] = [
            &[0x06, 0x05, 0x01],
            &[0x41, 0x9A, 0x00, 0x00, 0x03, 0x01],
            &[0x65, 0x88],
        ];
        let mut annexb = Vec::new();
        for unit in units {
            annexb.extend(START_CODE);
            annexb.extend(unit);
        }
        let (_, sample) = split_sample(Codec::H264, &annexb);
        assert_eq!(to_annexb(&sample).unwrap(), annexb);
        assert_eq!(to_annexb(&[]).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn malformed_is_refused() {
        // A cut SPS.
        let sets = avc_sets();
        let cut = ParameterSets {
            sps: sets.sps[..3].to_vec(),
            ..sets
        };
        assert!(matches!(avcc_from(&cut), Err(MediaError::Codec(_))));
        let hevc = hevc_sets();
        let cut = ParameterSets {
            sps: hevc.sps[..10].to_vec(),
            ..hevc.clone()
        };
        assert!(matches!(hvcc_from(&cut), Err(MediaError::Codec(_))));
        let no_vps = ParameterSets { vps: None, ..hevc };
        assert!(hvcc_from(&no_vps).is_err());
        // Cut records.
        let avcc = hex(AVCC);
        let hvcc = hex(HVCC);
        assert!(sequence_header(Codec::H264, &avcc[..20]).is_err());
        assert!(sequence_header(Codec::Hevc, &hvcc[..40]).is_err());
        assert!(sequence_header(Codec::H264, &[]).is_err());
        // A length prefix past the end, and a cut prefix.
        assert!(matches!(
            to_annexb(&[0, 0, 0, 9, 0x65, 0x88]),
            Err(MediaError::Codec(_))
        ));
        assert!(to_annexb(&[0, 0, 0]).is_err());
        assert!(to_annexb(&[0, 0, 0, 0]).is_err());
    }
}
