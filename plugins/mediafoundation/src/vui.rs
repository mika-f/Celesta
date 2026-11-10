//! The colour description in a video stream's own headers.
//!
//! Microsoft's software HEVC encoder (and its H.264 one) writes an SPS
//! whose VUI has no `video_signal_type`, whatever the output type and
//! `ICodecAPI` say (Task 7). A decoder that reads the stream and not the
//! container's `colr` box (libavcodec's HEVC one is such a decoder) then
//! converts to RGB as BT.601. [`with_bt709`] rewrites such an SPS so its
//! VUI says BT.709 limited range; every other bit is kept as it was.

use crate::annexb::Codec;
use crate::params::{Bits, avc_sps_to_vui_flag, skip_profile_tier_level, unescaped};

/// The colour a VUI describes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VuiColour {
    pub full_range: bool,
    pub primaries: u8,
    pub transfer: u8,
    pub matrix: u8,
}

/// BT.709 limited range, as the VUI numbers it (ITU-T H.273).
pub(crate) const BT709_LIMITED: VuiColour = VuiColour {
    full_range: false,
    primaries: 1,
    transfer: 1,
    matrix: 1,
};

/// How many bytes an SPS's header takes before its RBSP.
fn header_length(codec: Codec) -> usize {
    match codec {
        Codec::H264 => 4,
        Codec::Hevc => 2,
    }
}

/// An HEVC `scaling_list_data()`, skipped.
fn skip_hevc_scaling_list_data(bits: &mut Bits) -> Option<()> {
    for size in 0..4 {
        let mut matrix = 0;
        while matrix < 6 {
            if bits.flag()? {
                // scaling_list_pred_mode_flag
                let coefficients = 64.min(1usize << (4 + (size << 1)));
                if size > 1 {
                    bits.se()?; // scaling_list_dc_coef_minus8
                }
                for _ in 0..coefficients {
                    bits.se()?; // scaling_list_delta_coef
                }
            } else {
                bits.ue()?; // scaling_list_pred_matrix_id_delta
            }
            matrix += if size == 3 { 3 } else { 1 };
        }
    }
    Some(())
}

/// An HEVC SPS (header included) read up to `vui_parameters_present_flag`,
/// which is the next bit.
fn hevc_sps_to_vui_flag(sps: &[u8]) -> Option<Bits> {
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
    // log2_max_pic_order_cnt_lsb_minus4; a hostile SPS's may not fit.
    let poc_bits = bits.ue()?.checked_add(4)?;
    let all_layers = bits.flag()?; // sps_sub_layer_ordering_info_present_flag
    for _ in (if all_layers { 0 } else { max_sub_layers_minus1 })..=max_sub_layers_minus1 {
        bits.ue()?;
        bits.ue()?;
        bits.ue()?;
    }
    for _ in 0..6 {
        bits.ue()?; // block sizes, transform hierarchy depths
    }
    if bits.flag()? && bits.flag()? {
        // scaling_list_enabled_flag, sps_scaling_list_data_present_flag
        skip_hevc_scaling_list_data(&mut bits)?;
    }
    bits.bit()?; // amp_enabled_flag
    bits.bit()?; // sample_adaptive_offset_enabled_flag
    if bits.flag()? {
        // pcm_enabled_flag
        bits.skip(8)?; // pcm sample bit depths
        bits.ue()?;
        bits.ue()?;
        bits.bit()?; // pcm_loop_filter_disabled_flag
    }
    // st_ref_pic_set()s: only the number of pictures each holds matters,
    // for the sets that predict from it.
    let count = bits.ue()?;
    let mut sizes: Vec<u32> = Vec::new();
    for index in 0..count {
        let predicted = index != 0 && bits.flag()?; // inter_ref_pic_set_prediction_flag
        let size = if predicted {
            let reference = *sizes.last()?; // delta_idx_minus1 is 0 in an SPS
            bits.bit()?; // delta_rps_sign
            bits.ue()?; // abs_delta_rps_minus1
            inter_set_size(&mut bits, reference)?
        } else {
            let negative = bits.ue()?;
            let positive = bits.ue()?;
            let size = negative.checked_add(positive)?;
            for _ in 0..size {
                bits.ue()?; // delta_poc_sX_minus1
                bits.bit()?; // used_by_curr_pic_sX_flag
            }
            size
        };
        sizes.push(size);
    }
    if bits.flag()? {
        // long_term_ref_pics_present_flag
        for _ in 0..bits.ue()? {
            bits.skip(poc_bits as usize + 1)?; // lt_ref_pic_poc_lsb_sps, used_by_curr_pic_lt_sps_flag
        }
    }
    bits.bit()?; // sps_temporal_mvp_enabled_flag
    bits.bit()?; // strong_intra_smoothing_enabled_flag
    Some(bits)
}

/// The flags of an inter-predicted `st_ref_pic_set()` that follow its
/// `abs_delta_rps_minus1`, read: the number of pictures it holds, given the
/// number the set it predicts from holds.
fn inter_set_size(bits: &mut Bits, reference: u32) -> Option<u32> {
    let mut size = 0;
    for _ in 0..=reference {
        let used = bits.flag()?;
        if used || bits.flag()? {
            size += 1; // used_by_curr_pic_flag, or use_delta_flag
        }
    }
    Some(size)
}

/// The VUI at `bits`, which is just past `vui_parameters_present_flag`
/// being 1: where its `video_signal_type_present_flag` is, and what it holds.
enum Signal {
    /// No `video_signal_type`; the flag is at this bit.
    Absent(usize),
    /// A `video_signal_type` with no colour description; its
    /// `video_full_range_flag` is at this bit, and its
    /// `colour_description_present_flag` at the next.
    NoDescription(usize),
    Described(VuiColour),
}

fn read_signal(bits: &mut Bits) -> Option<Signal> {
    if bits.flag()? {
        // aspect_ratio_info_present_flag: aspect_ratio_idc, and Extended_SAR's sizes.
        if bits.bits(8)? == 255 {
            bits.skip(32)?;
        }
    }
    if bits.flag()? {
        bits.bit()?; // overscan_appropriate_flag
    }
    let at = bits.position;
    if !bits.flag()? {
        return Some(Signal::Absent(at));
    }
    bits.skip(3)?; // video_format
    let at = bits.position;
    let full_range = bits.flag()?;
    if !bits.flag()? {
        return Some(Signal::NoDescription(at));
    }
    Some(Signal::Described(VuiColour {
        full_range,
        primaries: bits.bits(8)? as u8,
        transfer: bits.bits(8)? as u8,
        matrix: bits.bits(8)? as u8,
    }))
}

fn to_vui_flag(codec: Codec, sps: &[u8]) -> Option<Bits> {
    match codec {
        Codec::H264 => avc_sps_to_vui_flag(sps),
        Codec::Hevc => hevc_sps_to_vui_flag(sps),
    }
}

/// The colour description in `sps`'s VUI (header included), None when it
/// has none or is cut short.
pub(crate) fn sps_colour(codec: Codec, sps: &[u8]) -> Option<VuiColour> {
    let mut bits = to_vui_flag(codec, sps)?;
    if !bits.flag()? {
        return None;
    }
    match read_signal(&mut bits)? {
        Signal::Described(colour) => Some(colour),
        _ => None,
    }
}

/// Bits, written from the most significant of the first byte on.
#[derive(Default)]
struct BitWriter {
    bytes: Vec<u8>,
    length: usize,
}

impl BitWriter {
    fn bit(&mut self, bit: bool) {
        if self.length.is_multiple_of(8) {
            self.bytes.push(0);
        }
        if bit {
            *self.bytes.last_mut().unwrap() |= 0x80 >> (self.length % 8);
        }
        self.length += 1;
    }

    fn bits(&mut self, value: u32, count: u32) {
        for shift in (0..count).rev() {
            self.bit(value >> shift & 1 == 1);
        }
    }
}

/// `bytes` with a `00 00 03` before any byte under 4 that follows two zeros.
fn escaped(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + 2);
    let mut zeros = 0;
    for &byte in bytes {
        if zeros >= 2 && byte <= 3 {
            out.push(3);
            zeros = 0;
        }
        zeros = if byte == 0 { zeros + 1 } else { 0 };
        out.push(byte);
    }
    out
}

/// The `video_signal_type` fields with BT.709 limited range, from
/// `video_format` on.
fn signal_type(writer: &mut BitWriter, with_flag: bool) {
    if with_flag {
        writer.bit(true); // video_signal_type_present_flag
    }
    writer.bits(5, 3); // video_format: unspecified
    writer.bit(false); // video_full_range_flag
    writer.bit(true); // colour_description_present_flag
    description(writer);
}

fn description(writer: &mut BitWriter) {
    for value in [
        BT709_LIMITED.primaries,
        BT709_LIMITED.transfer,
        BT709_LIMITED.matrix,
    ] {
        writer.bits(u32::from(value), 8);
    }
}

/// `sps` (header included) with its VUI saying BT.709 limited range, unless
/// it already has a colour description: then the same bytes. An SPS this
/// cannot read is also returned as it is.
pub(crate) fn with_bt709(codec: Codec, sps: &[u8]) -> Vec<u8> {
    rewritten(codec, sps).unwrap_or_else(|| sps.to_vec())
}

fn rewritten(codec: Codec, sps: &[u8]) -> Option<Vec<u8>> {
    let mut bits = to_vui_flag(codec, sps)?;
    let flag_at = bits.position;
    // `replace` bits of the RBSP from `at` give way to `insert`.
    let (at, replace, insert): (usize, usize, BitWriter) = if bits.flag()? {
        let mut insert = BitWriter::default();
        match read_signal(&mut bits)? {
            Signal::Described(_) => return None,
            Signal::Absent(at) => {
                signal_type(&mut insert, true);
                (at, 1, insert)
            }
            Signal::NoDescription(at) => {
                // Limited range, whatever the flag said, and a description.
                insert.bit(false); // video_full_range_flag
                insert.bit(true); // colour_description_present_flag
                description(&mut insert);
                (at, 2, insert)
            }
        }
    } else {
        // No VUI: one with only the video signal type. The fields after the
        // signal type are all absent (HEVC's has one more flag than H.264's).
        let mut insert = BitWriter::default();
        insert.bit(true); // vui_parameters_present_flag
        insert.bit(false); // aspect_ratio_info_present_flag
        insert.bit(false); // overscan_info_present_flag
        signal_type(&mut insert, true);
        let absent = match codec {
            // chroma_loc, timing, nal_hrd, vcl_hrd, pic_struct, bitstream_restriction
            Codec::H264 => 6,
            // chroma_loc, neutral_chroma, field_seq, frame_field_info,
            // default_display_window, timing, bitstream_restriction
            Codec::Hevc => 7,
        };
        for _ in 0..absent {
            insert.bit(false);
        }
        (flag_at, 1, insert)
    };
    let payload = &bits.bytes;
    // The RBSP ends with the last 1 bit, the stop bit.
    let end = payload
        .iter()
        .rposition(|&byte| byte != 0)
        .map(|index| index * 8 + 8 - payload[index].trailing_zeros() as usize)?;
    let copy = |writer: &mut BitWriter, from: usize, to: usize| {
        for position in from..to {
            writer.bit(payload[position / 8] >> (7 - position % 8) & 1 == 1);
        }
    };
    let mut writer = BitWriter::default();
    copy(&mut writer, 0, at);
    for position in 0..insert.length {
        writer.bit(insert.bytes[position / 8] >> (7 - position % 8) & 1 == 1);
    }
    copy(&mut writer, at + replace, end);
    let mut out = sps.get(..header_length(codec))?.to_vec();
    out.extend(escaped(&writer.bytes));
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annexb::{ParameterSets, avcc_from, hvcc_from};
    use crate::params::{
        avc_codec_string, avc_reorder_depth, hevc_codec_string, hevc_reorder_depth, hevc_sps_info,
    };

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
            .collect()
    }

    /// An SPS that already has a colour description (and no
    /// `bitstream_restriction`): the one in `params`' `AVCC` test vector.
    const AVC_DESCRIBED: &str = "2764000dac56505067e79a80808081";
    /// An HEVC SPS that already has a colour description: the one in
    /// `params`' `HVCC` test vector.
    const HEVC_DESCRIBED: &str =
        "420101022000000300b0000003000003003ca00a080c1f3b62057b9165535010101008";
    /// The SPS Microsoft's software H.264 encoder wrote for 64x36 (Windows
    /// 11): a VUI with timing and `bitstream_restriction`, no signal type.
    const AVC_SOFTWARE: &str = "6764001eac2cac47f9f01100000303e90000ea600da0884538";
    /// The SPS of Microsoft's software HEVC encoder for 64x36: a VUI with
    /// timing and no signal type.
    const HEVC_SOFTWARE: &str =
        "420101014000000300b00000030000030003a020831f3f2a4edcb9808080000032000005da84";
    const AVC_PPS: &str = "68ce3c30";
    const HEVC_VPS: &str = "40010c01ffff014000000300b000000300000300032a0240";
    const HEVC_PPS: &str = "4401c071809b20";

    fn avc_record(sps: &[u8]) -> Vec<u8> {
        avcc_from(&ParameterSets {
            vps: None,
            sps: sps.to_vec(),
            pps: hex(AVC_PPS),
        })
        .unwrap()
    }

    fn hevc_record(sps: &[u8]) -> Vec<u8> {
        hvcc_from(&ParameterSets {
            vps: Some(hex(HEVC_VPS)),
            sps: sps.to_vec(),
            pps: hex(HEVC_PPS),
        })
        .unwrap()
    }

    #[test]
    fn the_software_encoders_spss_have_no_colour_description() {
        assert_eq!(sps_colour(Codec::H264, &hex(AVC_SOFTWARE)), None);
        assert_eq!(sps_colour(Codec::Hevc, &hex(HEVC_SOFTWARE)), None);
    }

    #[test]
    fn an_sps_with_a_description_is_unchanged() {
        for (codec, text) in [(Codec::H264, AVC_DESCRIBED), (Codec::Hevc, HEVC_DESCRIBED)] {
            let sps = hex(text);
            assert!(sps_colour(codec, &sps).is_some(), "{text}");
            assert_eq!(with_bt709(codec, &sps), sps, "{text}");
        }
        let rewritten = with_bt709(Codec::H264, &hex(AVC_SOFTWARE));
        assert_eq!(with_bt709(Codec::H264, &rewritten), rewritten);
        let rewritten = with_bt709(Codec::Hevc, &hex(HEVC_SOFTWARE));
        assert_eq!(with_bt709(Codec::Hevc, &rewritten), rewritten);
    }

    #[test]
    fn an_avc_sps_gets_bt709_and_keeps_the_rest() {
        let sps = hex(AVC_SOFTWARE);
        let rewritten = with_bt709(Codec::H264, &sps);
        assert_ne!(rewritten, sps);
        assert_eq!(sps_colour(Codec::H264, &rewritten), Some(BT709_LIMITED));
        let (before, after) = (avc_record(&sps), avc_record(&rewritten));
        assert_eq!(
            avc_codec_string(&before).unwrap(),
            avc_codec_string(&after).unwrap()
        );
        // The software SPS has a bitstream_restriction, kept: depth 1.
        assert_eq!(avc_reorder_depth(&before).unwrap(), Some(1));
        assert_eq!(avc_reorder_depth(&after).unwrap(), Some(1));
    }

    #[test]
    fn an_hevc_sps_gets_bt709_and_keeps_the_rest() {
        let sps = hex(HEVC_SOFTWARE);
        let rewritten = with_bt709(Codec::Hevc, &sps);
        assert_ne!(rewritten, sps);
        assert_eq!(sps_colour(Codec::Hevc, &rewritten), Some(BT709_LIMITED));
        assert_eq!(hevc_sps_info(&rewritten), hevc_sps_info(&sps));
        let (before, after) = (hevc_record(&sps), hevc_record(&rewritten));
        assert_eq!(
            hevc_codec_string(&before).unwrap(),
            hevc_codec_string(&after).unwrap()
        );
        assert_eq!(
            hevc_reorder_depth(&before).unwrap(),
            hevc_reorder_depth(&after).unwrap()
        );
    }

    #[test]
    fn nothing_but_the_signal_type_changes() {
        // The RBSP's bits, up to its stop bit.
        let bits = |sps: &[u8], header: usize| {
            let payload = unescaped(&sps[header..]);
            let last = payload.iter().rposition(|&byte| byte != 0).unwrap();
            let end = last * 8 + 8 - payload[last].trailing_zeros() as usize;
            (0..end)
                .map(|at| payload[at / 8] >> (7 - at % 8) & 1)
                .collect::<Vec<_>>()
        };
        for (codec, header, text) in [
            (Codec::H264, 4, AVC_SOFTWARE),
            (Codec::Hevc, 2, HEVC_SOFTWARE),
        ] {
            let sps = hex(text);
            let (before, after) = (bits(&sps, header), bits(&with_bt709(codec, &sps), header));
            // video_signal_type_present_flag 1 replaces a 0, and 3 + 1 + 1
            // + 24 bits follow it.
            assert_eq!(after.len(), before.len() + 29, "{text}");
            let common = before
                .iter()
                .zip(&after)
                .take_while(|(a, b)| a == b)
                .count();
            assert_eq!(
                &after[after.len() - (before.len() - common - 1)..],
                &before[common + 1..]
            );
            assert_eq!(before[common], 0);
            assert_eq!(after[common], 1);
        }
    }

    /// `sps` (with a VUI) cut down to an SPS whose VUI flag is 0.
    fn without_vui(codec: Codec, sps: &[u8]) -> Vec<u8> {
        let mut bits = to_vui_flag(codec, sps).unwrap();
        let flag_at = bits.position;
        assert!(bits.flag().unwrap());
        let mut kept: Vec<u8> = (0..flag_at)
            .map(|at| bits.bytes[at / 8] >> (7 - at % 8) & 1)
            .collect();
        if codec == Codec::Hevc {
            kept.push(0); // vui_parameters_present_flag
            kept.extend([0, 1]); // sps_extension_present_flag 0, then the stop bit
        } else {
            kept.extend([0, 1]); // no VUI, then the stop bit
        }
        if codec == Codec::Hevc {
            kept.remove(flag_at + 1); // the extension flag of the cut SPS is the next bit
            kept.insert(flag_at + 1, 0);
        }
        while !kept.len().is_multiple_of(8) {
            kept.push(0);
        }
        let header = header_length(codec);
        let mut bare = sps[..header].to_vec();
        let packed: Vec<u8> = kept
            .chunks(8)
            .map(|chunk| chunk.iter().fold(0, |byte, bit| byte << 1 | bit))
            .collect();
        bare.extend(escaped(&packed));
        bare
    }

    #[test]
    fn a_vui_free_avc_sps_gets_one() {
        let bare = without_vui(Codec::H264, &hex(AVC_SOFTWARE));
        assert_eq!(sps_colour(Codec::H264, &bare), None);
        let rewritten = with_bt709(Codec::H264, &bare);
        assert_eq!(sps_colour(Codec::H264, &rewritten), Some(BT709_LIMITED));
        assert_eq!(
            avc_codec_string(&avc_record(&bare)).unwrap(),
            avc_codec_string(&avc_record(&rewritten)).unwrap()
        );
        assert_eq!(avc_reorder_depth(&avc_record(&rewritten)).unwrap(), None);
    }

    #[test]
    fn a_vui_free_hevc_sps_gets_one() {
        let bare = without_vui(Codec::Hevc, &hex(HEVC_SOFTWARE));
        assert_eq!(sps_colour(Codec::Hevc, &bare), None);
        let rewritten = with_bt709(Codec::Hevc, &bare);
        assert_eq!(sps_colour(Codec::Hevc, &rewritten), Some(BT709_LIMITED));
        assert_eq!(hevc_sps_info(&rewritten), hevc_sps_info(&bare));
        // vui_parameters_present_flag and the 7 absent fields after the
        // signal type, then the extension flag and the stop bit.
        let mut tail = to_vui_flag(Codec::Hevc, &rewritten).unwrap();
        assert!(tail.flag().unwrap());
        read_signal(&mut tail).unwrap();
        assert_eq!(tail.bits(7).unwrap(), 0);
        assert_eq!(tail.bits(2).unwrap(), 0b01);
    }

    /// `sps` (with a VUI and no signal type) with `signal` in place of its
    /// `video_signal_type_present_flag`.
    fn with_signal_bits(codec: Codec, sps: &[u8], signal: &[u8]) -> Vec<u8> {
        let mut bits = to_vui_flag(codec, sps).unwrap();
        assert!(bits.flag().unwrap());
        let Some(Signal::Absent(at)) = read_signal(&mut bits) else {
            panic!("the SPS has a signal type");
        };
        let payload = &bits.bytes;
        let last = payload.iter().rposition(|&byte| byte != 0).unwrap();
        let end = last * 8 + 8 - payload[last].trailing_zeros() as usize;
        let bit = |position: usize| payload[position / 8] >> (7 - position % 8) & 1 == 1;
        let mut writer = BitWriter::default();
        (0..at).for_each(|position| writer.bit(bit(position)));
        signal.iter().for_each(|&value| writer.bit(value == 1));
        (at + 1..end).for_each(|position| writer.bit(bit(position)));
        let mut out = sps[..header_length(codec)].to_vec();
        out.extend(escaped(&writer.bytes));
        out
    }

    #[test]
    fn a_signal_type_without_a_description_gets_one_in_limited_range() {
        for (codec, text) in [(Codec::H264, AVC_SOFTWARE), (Codec::Hevc, HEVC_SOFTWARE)] {
            let sps = hex(text);
            for full_range in [1, 0] {
                // video_signal_type_present_flag, video_format 5,
                // video_full_range_flag, colour_description_present_flag 0.
                let signal = with_signal_bits(codec, &sps, &[1, 1, 0, 1, full_range, 0]);
                assert_eq!(sps_colour(codec, &signal), None, "{text}");
                let rewritten = with_bt709(codec, &signal);
                assert_eq!(
                    sps_colour(codec, &rewritten),
                    Some(BT709_LIMITED),
                    "{text}, full range {full_range}"
                );
                if codec == Codec::H264 {
                    assert_eq!(
                        avc_codec_string(&avc_record(&rewritten)).unwrap(),
                        avc_codec_string(&avc_record(&sps)).unwrap()
                    );
                    assert_eq!(avc_reorder_depth(&avc_record(&rewritten)).unwrap(), Some(1));
                } else {
                    assert_eq!(hevc_sps_info(&rewritten), hevc_sps_info(&sps));
                }
            }
        }
    }

    /// Writes `value` as an unsigned Exp-Golomb code.
    fn write_ue(writer: &mut BitWriter, value: u32) {
        let code = u64::from(value) + 1;
        let length = 64 - code.leading_zeros();
        for _ in 1..length {
            writer.bit(false);
        }
        for shift in (0..length).rev() {
            writer.bit(code >> shift & 1 == 1);
        }
    }

    /// An HEVC SPS (header included) of one sub-layer, 4:2:0 and 8 bits,
    /// with `log2_max_pic_order_cnt_lsb_minus4` of `poc` and one short-term
    /// set of `negative` and `positive` pictures, cut short after them.
    fn hevc_sps_with(poc: u32, negative: u32, positive: u32) -> Vec<u8> {
        let mut writer = BitWriter::default();
        writer.bits(0, 4); // sps_video_parameter_set_id
        writer.bits(0, 3); // sps_max_sub_layers_minus1
        writer.bit(true); // sps_temporal_id_nesting_flag
        for _ in 0..3 {
            writer.bits(0, 32); // profile_tier_level: 96 bits for one sub-layer
        }
        write_ue(&mut writer, 0); // sps_seq_parameter_set_id
        write_ue(&mut writer, 1); // chroma_format_idc
        write_ue(&mut writer, 64); // pic_width_in_luma_samples
        write_ue(&mut writer, 36); // pic_height_in_luma_samples
        writer.bit(false); // conformance_window_flag
        write_ue(&mut writer, 0); // bit_depth_luma_minus8
        write_ue(&mut writer, 0); // bit_depth_chroma_minus8
        write_ue(&mut writer, poc);
        writer.bit(true); // sps_sub_layer_ordering_info_present_flag
        for _ in 0..3 + 6 {
            write_ue(&mut writer, 0); // ordering info, block sizes, depths
        }
        writer.bits(0, 4); // scaling lists, amp, sao, pcm: all off
        write_ue(&mut writer, 1); // num_short_term_ref_pic_sets
        write_ue(&mut writer, negative);
        write_ue(&mut writer, positive);
        writer.bit(true); // a stop bit
        let mut sps = vec![0x42, 0x01];
        sps.extend(escaped(&writer.bytes));
        sps
    }

    #[test]
    fn hostile_counts_are_malformed_not_overflows() {
        // The largest ue(v) reads.
        const HOSTILE: u32 = u32::MAX - 1;
        // With small counts it reads past them, and is cut short later.
        let mut fine = hevc_sps_with(0, 1, 0);
        fine.extend([0x80; 8]);
        assert!(hevc_sps_to_vui_flag(&fine).is_some());
        for hostile in [
            hevc_sps_with(HOSTILE, 0, 0),
            hevc_sps_with(0, HOSTILE, HOSTILE),
        ] {
            assert!(hevc_sps_to_vui_flag(&hostile).is_none());
            assert_eq!(sps_colour(Codec::Hevc, &hostile), None);
            assert_eq!(with_bt709(Codec::Hevc, &hostile), hostile);
        }
    }

    #[test]
    fn a_sps_it_cannot_read_is_left_alone() {
        let cut = hex(HEVC_SOFTWARE)[..10].to_vec();
        assert_eq!(with_bt709(Codec::Hevc, &cut), cut);
    }

    #[test]
    fn escapes_zeros() {
        assert_eq!(
            escaped(&[0, 0, 0, 0, 1, 0, 0, 5]),
            [0, 0, 3, 0, 0, 3, 1, 0, 0, 5]
        );
    }
}
