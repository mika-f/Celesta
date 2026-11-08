//! H.264 from libx264's Annex B byte stream to the form MP4 and MOV store:
//! an `avcC` decoder configuration and length-prefixed NAL units.

use celesta_media::MediaError;

/// The NAL unit types of a sequence and a picture parameter set.
const SPS: u8 = 7;
const PPS: u8 = 8;

/// The index of the first `00 00 01` in `data` at or after `from`, or
/// `data.len()`.
fn find_start_code(data: &[u8], from: usize) -> usize {
    data.get(from..)
        .and_then(|rest| rest.windows(3).position(|window| window == [0, 0, 1]))
        .map_or(data.len(), |position| from + position)
}

/// The NAL units of an Annex B stream, split as libavformat's
/// `ff_avc_parse_nal_units` splits them for MP4: from after each start code
/// to the next one, less the zero byte of a 4-byte start code.
pub(crate) fn nal_units(data: &[u8]) -> Vec<&[u8]> {
    let mut units = Vec::new();
    let mut start = find_start_code(data, 0);
    // Past the start code's zeros and its 1.
    while let Some(offset) = data[start..].iter().position(|&byte| byte != 0) {
        let begin = start + offset + 1;
        if begin >= data.len() {
            break;
        }
        let mut end = find_start_code(data, begin);
        if end > begin && end < data.len() && data[end - 1] == 0 {
            end -= 1;
        }
        units.push(&data[begin..end]);
        start = end;
    }
    units
}

/// `annexb` with each start code replaced by its NAL unit's length, four
/// bytes big-endian.
pub(crate) fn annexb_to_avc(annexb: &[u8]) -> Vec<u8> {
    let units = nal_units(annexb);
    let mut avc = Vec::with_capacity(annexb.len() + units.len());
    for unit in units {
        avc.extend_from_slice(&(unit.len() as u32).to_be_bytes());
        avc.extend_from_slice(unit);
    }
    avc
}

/// The first NAL unit of `kind` in an Annex B stream.
fn first_unit(annexb: &[u8], kind: u8) -> Option<&[u8]> {
    nal_units(annexb)
        .into_iter()
        .find(|unit| unit.first().is_some_and(|header| header & 0x1F == kind))
}

/// The profiles whose `avcC` has no chroma format and bit depth fields:
/// Baseline, Main, and Extended.
const PROFILES_WITHOUT_EXTENSION: [u8; 3] = [66, 77, 88];

/// The profiles whose SPS gives its chroma format and bit depths.
const PROFILES_WITH_CHROMA_FORMAT: [u8; 13] =
    [100, 110, 122, 244, 44, 83, 86, 118, 128, 138, 139, 134, 135];

/// A NAL unit's payload without its emulation prevention bytes: the 3 of
/// each `00 00 03`.
fn unescaped(unit: &[u8]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(unit.len());
    let mut zeros = 0;
    for &byte in unit {
        if zeros >= 2 && byte == 3 {
            zeros = 0;
            continue;
        }
        zeros = if byte == 0 { zeros + 1 } else { 0 };
        payload.push(byte);
    }
    payload
}

/// Reads bits from the most significant of the first byte on.
struct Bits {
    bytes: Vec<u8>,
    position: usize,
}

impl Bits {
    fn bit(&mut self) -> Option<u32> {
        let byte = *self.bytes.get(self.position / 8)?;
        let bit = u32::from(byte >> (7 - self.position % 8)) & 1;
        self.position += 1;
        Some(bit)
    }

    fn bits(&mut self, count: u32) -> Option<u32> {
        (0..count).try_fold(0, |value, _| Some(value << 1 | self.bit()?))
    }

    /// An unsigned Exp-Golomb code.
    fn ue(&mut self) -> Option<u32> {
        let mut zeros = 0;
        while self.bit()? == 0 {
            zeros += 1;
            if zeros > 31 {
                return None;
            }
        }
        Some((1 << zeros) - 1 + self.bits(zeros)?)
    }
}

/// An SPS's `chroma_format_idc` and luma and chroma bit depths, less 8:
/// 4:2:0 and 8 bits for the profiles that do not give them. None for an
/// SPS it cannot read, or one whose values are out of H.264's ranges
/// (`chroma_format_idc` 0 to 3, depths 8 to 14 bits).
fn chroma_format_and_depths(sps: &[u8]) -> Option<(u8, u8, u8)> {
    // Past the NAL header, the profile, the constraints, and the level.
    let mut bits = Bits {
        bytes: unescaped(sps.get(4..)?),
        position: 0,
    };
    bits.ue()?; // seq_parameter_set_id
    if !PROFILES_WITH_CHROMA_FORMAT.contains(&sps[1]) {
        return Some((1, 0, 0));
    }
    let chroma_format = u8::try_from(bits.ue()?).ok().filter(|idc| *idc <= 3)?;
    if chroma_format == 3 {
        bits.bit()?; // separate_colour_plane_flag
    }
    let mut depth = || u8::try_from(bits.ue()?).ok().filter(|less_8| *less_8 <= 6);
    Some((chroma_format, depth()?, depth()?))
}

/// libx264's Annex B extradata as an `avcC` box's contents, as libavformat
/// writes it for MP4: version 1, the SPS's profile, constraints, and level,
/// 4-byte lengths, then its first SPS and first PPS, and, beyond Baseline,
/// Main, and Extended, the chroma format and bit depths.
pub(crate) fn avcc_from_annexb(extradata: &[u8]) -> Result<Vec<u8>, MediaError> {
    let missing = |what: &str| MediaError::Codec(format!("libx264 gave no {what} in its headers"));
    let sps = first_unit(extradata, SPS)
        .filter(|sps| sps.len() >= 4)
        .ok_or_else(|| missing("sequence parameter set"))?;
    let pps = first_unit(extradata, PPS).ok_or_else(|| missing("picture parameter set"))?;
    let length = |unit: &[u8]| {
        u16::try_from(unit.len())
            .map(u16::to_be_bytes)
            .map_err(|_| MediaError::Codec("libx264's parameter sets are too large".to_owned()))
    };
    let mut avcc = vec![1, sps[1], sps[2], sps[3], 0xFF, 0xE1];
    avcc.extend_from_slice(&length(sps)?);
    avcc.extend_from_slice(sps);
    avcc.push(1);
    avcc.extend_from_slice(&length(pps)?);
    avcc.extend_from_slice(pps);
    if !PROFILES_WITHOUT_EXTENSION.contains(&sps[1]) {
        let (chroma_format, luma, chroma) = chroma_format_and_depths(sps).ok_or_else(|| {
            MediaError::Codec("libx264 gave a sequence parameter set it cannot read".to_owned())
        })?;
        // Reserved bits set, and no SPS extensions.
        avcc.extend_from_slice(&[0xFC | chroma_format, 0xF8 | luma, 0xF8 | chroma, 0]);
    }
    Ok(avcc)
}

/// The SPS's codec string, as WebCodecs names it: `avc1.` and its profile,
/// constraint flags, and level in hexadecimal.
pub(crate) fn codec_string(sps: &[u8]) -> String {
    format!("avc1.{:02X}{:02X}{:02X}", sps[1], sps[2], sps[3])
}

/// The codec string of an `avcC` made by [`avcc_from_annexb`].
pub(crate) fn avcc_codec_string(avcc: &[u8]) -> String {
    // The SPS follows the six header bytes and its two length bytes.
    codec_string(&avcc[8..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_codes_become_lengths() {
        let annexb = [
            0, 0, 0, 1, 0x09, 0xF0, // a 4-byte start code
            0, 0, 1, 0x65, 0x88, 0x84, // a 3-byte one
            0, 0, 0, 1, 0x41, 0x9A,
        ];
        assert_eq!(
            annexb_to_avc(&annexb),
            [
                0, 0, 0, 2, 0x09, 0xF0, //
                0, 0, 0, 3, 0x65, 0x88, 0x84, //
                0, 0, 0, 2, 0x41, 0x9A,
            ]
        );
    }

    #[test]
    fn the_avcc_holds_the_first_sps_and_pps() {
        let sps = [0x67, 0x64, 0x00, 0x0A, 0xAC];
        let pps = [0x68, 0xEB, 0xE3];
        let mut annexb = vec![0, 0, 0, 1];
        annexb.extend_from_slice(&sps);
        annexb.extend_from_slice(&[0, 0, 0, 1]);
        annexb.extend_from_slice(&pps);
        let avcc = avcc_from_annexb(&annexb).unwrap();
        let mut expected = vec![1, 0x64, 0x00, 0x0A, 0xFF, 0xE1, 0, 5];
        expected.extend_from_slice(&sps);
        expected.extend_from_slice(&[1, 0, 3]);
        expected.extend_from_slice(&pps);
        // High profile: 4:2:0, 8 bits.
        expected.extend_from_slice(&[0xFD, 0xF8, 0xF8, 0]);
        assert_eq!(avcc, expected);
        assert_eq!(codec_string(&sps), "avc1.64000A");
        assert_eq!(avcc_codec_string(&avcc), "avc1.64000A");
        assert!(avcc_from_annexb(&[0, 0, 1, 0x68, 1]).is_err());
    }

    #[test]
    fn libx264s_high_10_and_high_4_4_4_spss_are_read() {
        // From libx264 (FFmpeg 8) for 64x64 yuv420p10le and yuv444p10le;
        // both hold emulation prevention bytes in their VUI.
        let high_10 = [
            0x67, 0x6E, 0x00, 0x0A, 0xA6, 0xCD, 0x94, 0x42, 0x6C, 0x04, 0x40, 0x00, 0x00, 0x03,
            0x00, 0x40, 0x00, 0x00, 0x03, 0x00, 0x83, 0xC4, 0x89, 0x65, 0x80,
        ];
        assert_eq!(chroma_format_and_depths(&high_10), Some((1, 2, 2)));
        let high_4_4_4 = [
            0x67, 0xF4, 0x00, 0x0A, 0x90, 0xD9, 0xB2, 0x88, 0x4D, 0x80, 0x88, 0x00, 0x00, 0x03,
            0x00, 0x08, 0x00, 0x00, 0x03, 0x00, 0x10, 0x78, 0x91, 0x2C, 0xB0,
        ];
        assert_eq!(chroma_format_and_depths(&high_4_4_4), Some((3, 2, 2)));
        let mut annexb = vec![0, 0, 0, 1];
        annexb.extend_from_slice(&high_10);
        annexb.extend_from_slice(&[0, 0, 0, 1, 0x68, 0xEB, 0xE3]);
        let avcc = avcc_from_annexb(&annexb).unwrap();
        assert_eq!(avcc[avcc.len() - 4..], [0xFD, 0xFA, 0xFA, 0]);
        assert_eq!(avcc_codec_string(&avcc), "avc1.6E000A");
    }

    #[test]
    fn emulation_prevention_bytes_are_skipped_before_reading() {
        // seq_parameter_set_id 2^23 - 1 (23 zero bits first), then 4:2:0
        // and 10 bits: `00 00 01 00 00 00 9B 80`, escaped.
        let sps = [
            0x67, 0x6E, 0x00, 0x1E, 0x00, 0x00, 0x03, 0x01, 0x00, 0x00, 0x03, 0x00, 0x9B, 0x80,
        ];
        assert_eq!(
            unescaped(&sps[4..]),
            [0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x9B, 0x80]
        );
        assert_eq!(chroma_format_and_depths(&sps), Some((1, 2, 2)));
    }

    #[test]
    fn out_of_range_sps_values_are_refused() {
        // chroma_format_idc 4 (`00101`), then depths 0.
        let chroma = [0x67, 0x64, 0x00, 0x0A, 0b1001_0111, 0b1000_0000];
        assert_eq!(chroma_format_and_depths(&chroma), None);
        // 4:2:0 (`010`), then a luma depth less 8 of 7 (`0001000`).
        let depth = [0x67, 0x64, 0x00, 0x0A, 0b1010_0001, 0b0001_0000];
        assert_eq!(chroma_format_and_depths(&depth), None);
        let mut annexb = vec![0, 0, 0, 1];
        annexb.extend_from_slice(&chroma);
        annexb.extend_from_slice(&[0, 0, 0, 1, 0x68, 0xEB, 0xE3]);
        assert!(avcc_from_annexb(&annexb).is_err());
    }
}
