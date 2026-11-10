//! What a muxer needs from the parameter sets VideoToolbox gives in its
//! `avcC` and `hvcC` records: the codec string, and how many frames the
//! stream reorders (so decode timestamps can be shifted to precede
//! presentation).

use celesta_media::MediaError;

/// The reorder depth to assume when the parameter sets do not say: an upper
/// bound, since too large a decode timestamp shift is valid and too small
/// is not.
pub(crate) const UNKNOWN_REORDER_DEPTH: u32 = 16;

const HEVC_SPS: u8 = 33;

/// The profiles whose SPS gives its chroma format, bit depths, and scaling
/// matrix.
const PROFILES_WITH_CHROMA_FORMAT: [u8; 13] =
    [100, 110, 122, 244, 44, 83, 86, 118, 128, 138, 139, 134, 135];

fn malformed(what: &str) -> MediaError {
    MediaError::Codec(format!("VideoToolbox gave a malformed {what}"))
}

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
    fn new(bytes: Vec<u8>) -> Self {
        Self { bytes, position: 0 }
    }

    fn bit(&mut self) -> Option<u32> {
        let byte = *self.bytes.get(self.position / 8)?;
        let bit = u32::from(byte >> (7 - self.position % 8)) & 1;
        self.position += 1;
        Some(bit)
    }

    fn flag(&mut self) -> Option<bool> {
        Some(self.bit()? == 1)
    }

    fn bits(&mut self, count: u32) -> Option<u32> {
        (0..count).try_fold(0, |value, _| Some(value << 1 | self.bit()?))
    }

    fn skip(&mut self, count: usize) -> Option<()> {
        self.position += count;
        (self.position <= self.bytes.len() * 8).then_some(())
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
        Some(((1u64 << zeros) - 1 + u64::from(self.bits(zeros)?)) as u32)
    }

    /// A signed Exp-Golomb code, skipped.
    fn se(&mut self) -> Option<()> {
        self.ue().map(|_| ())
    }
}

/// The first SPS of an `avcC` record.
fn avc_sps(avcc: &[u8]) -> Result<&[u8], MediaError> {
    let bad = || malformed("avcC");
    // numOfSequenceParameterSets, then the first SPS's length.
    if avcc.len() < 8 || avcc[5] & 0x1F == 0 {
        return Err(bad());
    }
    let length = usize::from(u16::from_be_bytes([avcc[6], *avcc.get(7).ok_or_else(bad)?]));
    let sps = avcc.get(8..8 + length).ok_or_else(bad)?;
    if sps.len() < 4 {
        return Err(bad());
    }
    Ok(sps)
}

/// `avc1.` and the SPS's profile, constraint flags, and level in upper-case
/// hexadecimal, as WebCodecs names them.
pub(crate) fn avc_codec_string(avcc: &[u8]) -> Result<String, MediaError> {
    let sps = avc_sps(avcc)?;
    Ok(format!("avc1.{:02X}{:02X}{:02X}", sps[1], sps[2], sps[3]))
}

/// A scaling list of `size` entries, skipped.
fn skip_scaling_list(bits: &mut Bits, size: usize) -> Option<()> {
    let (mut last, mut next) = (8i64, 8i64);
    for _ in 0..size {
        if next != 0 {
            let delta = bits.ue()?;
            // se(v) from ue(v): odd is positive.
            let delta = if delta % 2 == 1 {
                i64::from(delta.div_ceil(2))
            } else {
                -i64::from(delta / 2)
            };
            next = (last + delta + 256) % 256;
        }
        last = if next == 0 { last } else { next };
    }
    Some(())
}

/// `max_num_reorder_frames` of an H.264 SPS's `bitstream_restriction`, None
/// when it has none.
fn avc_sps_reorder_depth(sps: &[u8]) -> Option<Option<u32>> {
    let profile = sps[1];
    let mut bits = Bits::new(unescaped(sps.get(4..)?));
    bits.ue()?; // seq_parameter_set_id
    if PROFILES_WITH_CHROMA_FORMAT.contains(&profile) {
        let chroma_format = bits.ue()?;
        if chroma_format == 3 {
            bits.bit()?; // separate_colour_plane_flag
        }
        bits.ue()?; // bit_depth_luma_minus8
        bits.ue()?; // bit_depth_chroma_minus8
        bits.bit()?; // qpprime_y_zero_transform_bypass_flag
        if bits.flag()? {
            for index in 0..(if chroma_format == 3 { 12 } else { 8 }) {
                if bits.flag()? {
                    skip_scaling_list(&mut bits, if index < 6 { 16 } else { 64 })?;
                }
            }
        }
    }
    bits.ue()?; // log2_max_frame_num_minus4
    match bits.ue()? {
        0 => {
            bits.ue()?; // log2_max_pic_order_cnt_lsb_minus4
        }
        1 => {
            bits.bit()?; // delta_pic_order_always_zero_flag
            bits.se()?; // offset_for_non_ref_pic
            bits.se()?; // offset_for_top_to_bottom_field
            for _ in 0..bits.ue()? {
                bits.se()?; // offset_for_ref_frame
            }
        }
        _ => {}
    }
    bits.ue()?; // max_num_ref_frames
    bits.bit()?; // gaps_in_frame_num_value_allowed_flag
    bits.ue()?; // pic_width_in_mbs_minus1
    bits.ue()?; // pic_height_in_map_units_minus1
    if !bits.flag()? {
        bits.bit()?; // mb_adaptive_frame_field_flag
    }
    bits.bit()?; // direct_8x8_inference_flag
    if bits.flag()? {
        // frame_cropping: left, right, top, bottom.
        for _ in 0..4 {
            bits.ue()?;
        }
    }
    if !bits.flag()? {
        return Some(None); // no VUI
    }
    if bits.flag()? {
        // aspect_ratio_info_present_flag: aspect_ratio_idc, and Extended_SAR's sizes.
        if bits.bits(8)? == 255 {
            bits.skip(32)?;
        }
    }
    if bits.flag()? {
        bits.bit()?; // overscan_appropriate_flag
    }
    if bits.flag()? {
        // video_format, video_full_range_flag
        bits.skip(4)?;
        if bits.flag()? {
            bits.skip(24)?; // colour description
        }
    }
    if bits.flag()? {
        bits.ue()?; // chroma_sample_loc_type_top_field
        bits.ue()?; // chroma_sample_loc_type_bottom_field
    }
    if bits.flag()? {
        // num_units_in_tick, time_scale, fixed_frame_rate_flag
        bits.skip(65)?;
    }
    let nal_hrd = bits.flag()?;
    if nal_hrd {
        skip_hrd(&mut bits)?;
    }
    let vcl_hrd = bits.flag()?;
    if vcl_hrd {
        skip_hrd(&mut bits)?;
    }
    if nal_hrd || vcl_hrd {
        bits.bit()?; // low_delay_hrd_flag
    }
    bits.bit()?; // pic_struct_present_flag
    if !bits.flag()? {
        return Some(None); // no bitstream_restriction
    }
    bits.bit()?; // motion_vectors_over_pic_boundaries_flag
    bits.ue()?; // max_bytes_per_pic_denom
    bits.ue()?; // max_bits_per_mb_denom
    bits.ue()?; // log2_max_mv_length_horizontal
    bits.ue()?; // log2_max_mv_length_vertical
    Some(Some(bits.ue()?))
}

/// An H.264 `hrd_parameters()`, skipped.
fn skip_hrd(bits: &mut Bits) -> Option<()> {
    let count = bits.ue()? + 1; // cpb_cnt_minus1
    bits.skip(8)?; // bit_rate_scale, cpb_size_scale
    for _ in 0..count {
        bits.ue()?; // bit_rate_value_minus1
        bits.ue()?; // cpb_size_value_minus1
        bits.bit()?; // cbr_flag
    }
    bits.skip(20) // four 5-bit lengths and delays
}

/// The reorder depth of an `avcC` record's first SPS: its VUI's
/// `max_num_reorder_frames`, None when the SPS has no `bitstream_restriction`.
pub(crate) fn avc_reorder_depth(avcc: &[u8]) -> Result<Option<u32>, MediaError> {
    avc_sps_reorder_depth(avc_sps(avcc)?).ok_or_else(|| malformed("avcC"))
}

/// The first SPS (NAL type 33) of an `hvcC` record, header included.
fn hevc_sps(hvcc: &[u8]) -> Result<&[u8], MediaError> {
    let bad = || malformed("hvcC");
    let arrays = *hvcc.get(22).ok_or_else(bad)?;
    let mut at = 23;
    for _ in 0..arrays {
        let kind = hvcc.get(at).ok_or_else(bad)? & 0x3F;
        let count = u16::from_be_bytes([
            *hvcc.get(at + 1).ok_or_else(bad)?,
            *hvcc.get(at + 2).ok_or_else(bad)?,
        ]);
        at += 3;
        for index in 0..count {
            let length = usize::from(u16::from_be_bytes([
                *hvcc.get(at).ok_or_else(bad)?,
                *hvcc.get(at + 1).ok_or_else(bad)?,
            ]));
            let unit = hvcc.get(at + 2..at + 2 + length).ok_or_else(bad)?;
            if kind == HEVC_SPS && index == 0 {
                if unit.len() < 3 {
                    return Err(bad());
                }
                return Ok(unit);
            }
            at += 2 + length;
        }
    }
    Err(bad())
}

/// `hvc1.` per ISO/IEC 14496-15 Annex E: the profile space and idc, the
/// compatibility flags bit-reversed in hexadecimal, the tier and level, and
/// the constraint bytes less their trailing zeros.
pub(crate) fn hevc_codec_string(hvcc: &[u8]) -> Result<String, MediaError> {
    hevc_sps(hvcc)?;
    let header = hvcc.get(..13).ok_or_else(|| malformed("hvcC"))?;
    let space = match header[1] >> 6 {
        0 => "",
        1 => "A",
        2 => "B",
        _ => "C",
    };
    let compatibility =
        u32::from_be_bytes([header[2], header[3], header[4], header[5]]).reverse_bits();
    let mut constraints = header[6..12].to_vec();
    while constraints.last() == Some(&0) {
        constraints.pop();
    }
    let mut text = format!(
        "hvc1.{space}{}.{compatibility:X}.{}{}",
        header[1] & 0x1F,
        if header[1] & 0x20 == 0 { 'L' } else { 'H' },
        header[12],
    );
    for byte in constraints {
        text.push_str(&format!(".{byte:X}"));
    }
    Ok(text)
}

/// An HEVC `profile_tier_level()` with `max_sub_layers_minus1` sub-layers
/// beyond the first, skipped.
fn skip_profile_tier_level(bits: &mut Bits, max_sub_layers_minus1: u32) -> Option<()> {
    bits.skip(96)?; // general profile (88 bits) and level (8)
    let mut present = Vec::new();
    for _ in 0..max_sub_layers_minus1 {
        present.push((bits.flag()?, bits.flag()?));
    }
    if max_sub_layers_minus1 > 0 {
        bits.skip(2 * (8 - max_sub_layers_minus1 as usize))?;
    }
    for (profile, level) in present {
        if profile {
            bits.skip(88)?;
        }
        if level {
            bits.skip(8)?;
        }
    }
    Some(())
}

/// `sps_max_num_reorder_pics` of the highest sub-layer in an HEVC SPS.
fn hevc_sps_reorder_depth(sps: &[u8]) -> Option<u32> {
    let mut bits = Bits::new(unescaped(sps.get(2..)?));
    bits.skip(4)?; // sps_video_parameter_set_id
    let max_sub_layers_minus1 = bits.bits(3)?;
    bits.bit()?; // sps_temporal_id_nesting_flag
    skip_profile_tier_level(&mut bits, max_sub_layers_minus1)?;
    bits.ue()?; // sps_seq_parameter_set_id
    if bits.ue()? == 3 {
        bits.bit()?; // separate_colour_plane_flag
    }
    bits.ue()?; // pic_width_in_luma_samples
    bits.ue()?; // pic_height_in_luma_samples
    if bits.flag()? {
        for _ in 0..4 {
            bits.ue()?; // conformance_window offsets
        }
    }
    bits.ue()?; // bit_depth_luma_minus8
    bits.ue()?; // bit_depth_chroma_minus8
    bits.ue()?; // log2_max_pic_order_cnt_lsb_minus4
    let all_layers = bits.flag()?; // sps_sub_layer_ordering_info_present_flag
    let mut reorder = None;
    for _ in (if all_layers { 0 } else { max_sub_layers_minus1 })..=max_sub_layers_minus1 {
        bits.ue()?; // sps_max_dec_pic_buffering_minus1
        reorder = Some(bits.ue()?);
        bits.ue()?; // sps_max_latency_increase_plus1
    }
    reorder
}

/// The reorder depth of an `hvcC` record's first SPS:
/// `sps_max_num_reorder_pics` of the highest sub-layer.
pub(crate) fn hevc_reorder_depth(hvcc: &[u8]) -> Result<Option<u32>, MediaError> {
    hevc_sps_reorder_depth(hevc_sps(hvcc)?)
        .map(Some)
        .ok_or_else(|| malformed("hvcC"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
            .collect()
    }

    const AVCC: &str = "0164000dffe1000f2764000dac56505067e79a8080808101000428ee3cb0fdf8f800";
    const HVCC: &str = "010220000000b000000000003cf000fcfdfafa00000b03a00001001840010c01ffff022000000300b0000003000003003c15c090a100010023420101022000000300b0000003000003003ca00a080c1f3b62057b9165535010101008a2000100084401c02835182992";

    #[test]
    fn avc_strings_and_depth() {
        let avcc = hex(AVCC);
        assert_eq!(avc_codec_string(&avcc).unwrap(), "avc1.64000D");
        // VideoToolbox's SPS has a VUI without bitstream_restriction.
        assert_eq!(avc_reorder_depth(&avcc).unwrap(), None);
        // H.264's largest DPB: the fallback bounds any reorder depth.
        assert_eq!(UNKNOWN_REORDER_DEPTH, 16);
    }

    /// Writes bits most significant first, with Exp-Golomb codes.
    #[derive(Default)]
    struct Writer(Vec<bool>);

    impl Writer {
        fn bits(&mut self, value: u32, count: u32) -> &mut Self {
            self.0.extend((0..count).rev().map(|i| value >> i & 1 == 1));
            self
        }

        fn ue(&mut self, value: u32) -> &mut Self {
            let coded = value + 1;
            let length = 32 - coded.leading_zeros();
            self.bits(0, length - 1).bits(coded, length)
        }

        fn bytes(&mut self) -> Vec<u8> {
            self.bits(1, 1);
            self.0
                .chunks(8)
                .map(|chunk| {
                    (0..8).fold(0, |byte, i| {
                        byte << 1 | u8::from(chunk.get(i) == Some(&true))
                    })
                })
                .collect()
        }
    }

    #[test]
    fn an_avc_sps_with_a_bitstream_restriction_gives_its_depth() {
        // High profile with a scaling list, a VUI with timing and an NAL HRD.
        let mut w = Writer::default();
        w.ue(0).ue(1).ue(0).ue(0).bits(0, 1); // id, 4:2:0, 8 bits, no bypass
        w.bits(1, 1); // scaling matrix present
        w.bits(1, 1).ue(1).ue(2); // list 0: a delta, then 15 more
        for _ in 0..14 {
            w.ue(0); // delta 0
        }
        w.bits(0, 1)
            .bits(0, 1)
            .bits(0, 1)
            .bits(0, 1)
            .bits(0, 1)
            .bits(0, 1)
            .bits(0, 1); // lists 1-7
        w.ue(0).ue(0).ue(0); // log2_max_frame_num, poc type 0, log2_max_poc_lsb
        w.ue(4).bits(0, 1).ue(79).ue(44).bits(1, 1).bits(1, 1); // refs, gaps, size, frame_mbs_only, direct_8x8
        w.bits(0, 1); // no cropping
        w.bits(1, 1); // VUI
        w.bits(1, 1).bits(1, 8); // aspect_ratio_idc 1
        w.bits(0, 1).bits(0, 1); // no overscan, no video signal
        w.bits(0, 1); // no chroma loc
        w.bits(1, 1).bits(1001, 32).bits(60000, 32).bits(1, 1); // timing
        w.bits(1, 1)
            .ue(0)
            .bits(0, 8)
            .ue(1000)
            .ue(2000)
            .bits(0, 1)
            .bits(0, 20); // NAL HRD
        w.bits(0, 1); // no VCL HRD
        w.bits(0, 1); // low_delay_hrd_flag
        w.bits(0, 1); // pic_struct_present_flag
        w.bits(1, 1)
            .bits(1, 1)
            .ue(0)
            .ue(1)
            .ue(16)
            .ue(16)
            .ue(3)
            .ue(4); // restriction: 3 reorder, 4 buffered
        let mut sps = vec![0x67, 100, 0, 30];
        sps.extend(w.bytes());
        let mut avcc = vec![1, 100, 0, 30, 0xFF, 0xE1];
        avcc.extend((sps.len() as u16).to_be_bytes());
        avcc.extend(sps);
        assert_eq!(avc_reorder_depth(&avcc).unwrap(), Some(3));
    }

    #[test]
    fn hevc_strings_and_depth() {
        let hvcc = hex(HVCC);
        assert_eq!(hevc_codec_string(&hvcc).unwrap(), "hvc1.2.4.L60.B0");
        assert_eq!(hevc_reorder_depth(&hvcc).unwrap(), Some(2));
    }

    #[test]
    fn malformed_records_are_refused() {
        let avcc = hex(AVCC);
        let hvcc = hex(HVCC);
        assert!(matches!(
            avc_codec_string(&avcc[..5]),
            Err(MediaError::Codec(_))
        ));
        assert!(matches!(
            avc_reorder_depth(&avcc[..5]),
            Err(MediaError::Codec(_))
        ));
        assert!(matches!(
            hevc_codec_string(&hvcc[..30]),
            Err(MediaError::Codec(_))
        ));
        assert!(matches!(
            hevc_reorder_depth(&hvcc[..30]),
            Err(MediaError::Codec(_))
        ));
        assert!(avc_reorder_depth(&[]).is_err());
        assert!(hevc_reorder_depth(&[]).is_err());
    }
}
