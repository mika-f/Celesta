//! The names the decoder's probe gives, as the FFmpeg plugin's do: codecs by
//! FFmpeg's names for Media Foundation's subtypes, and colour tags by
//! FFmpeg's names for its `MFVideoPrimaries`, `MFVideoTransferFunction`, and
//! `MFVideoTransferMatrix` values (`MFVideoPrimaries_BT709` is `bt709`, …).
//!
//! It takes plain numbers, so it builds and is tested everywhere: each value
//! is the `windows` crate's constant, written as a literal and named in a
//! comment, and a Windows-only test checks every literal against the crate.
//! Its shape follows `plugins/avfoundation/src/names.rs` (P5).

/// FFmpeg's name for the codec of a subtype whose GUID is
/// `{fourcc}-0000-0010-8000-00AA00389B71`: `h264`, `hevc`, `aac`,
/// `pcm_s16le`, …; else the FourCC itself (its four characters, or its
/// number in hex when they are not printable). Uncompressed PCM is named
/// by its depth with [`pcm_name`]; alone, it is FFmpeg's default,
/// `pcm_s16le`.
pub(crate) fn codec_name(fourcc: u32) -> String {
    let name = match fourcc {
        // MFAudioFormat_PCM
        0x0000_0001 => "pcm_s16le",
        // MFAudioFormat_Float
        0x0000_0003 => "pcm_f32le",
        // MFAudioFormat_MPEG
        0x0000_0050 => "mp2",
        // MFAudioFormat_MP3
        0x0000_0055 => "mp3",
        // MFAudioFormat_AAC, MFAudioFormat_ADTS
        0x0000_1610 | 0x0000_1600 => "aac",
        // MFAudioFormat_ALAC
        0x0000_6C61 => "alac",
        // MFAudioFormat_FLAC
        0x0000_F1AC => "flac",
        // MFAudioFormat_Opus
        0x0000_704F => "opus",
        _ => match &fourcc.to_le_bytes() {
            // MFVideoFormat_H264 is `H264`.
            b"H264" | b"h264" | b"avc1" | b"AVC1" | b"avc3" => "h264",
            // MFVideoFormat_HEVC is `HEVC`, MFVideoFormat_HEVC_ES `HEVS`,
            // MFVideoFormat_H265 `H265`.
            b"HEVC" | b"HEVS" | b"H265" | b"hvc1" | b"hev1" => "hevc",
            b"mp4a" => "aac",
            // MFVideoFormat_MP4V
            b"MP4V" | b"mp4v" => "mpeg4",
            // MFVideoFormat_MJPG
            b"MJPG" => "mjpeg",
            // MFVideoFormat_VP80, MFVideoFormat_VP90, MFVideoFormat_AV1
            b"VP80" => "vp8",
            b"VP90" => "vp9",
            b"AV01" => "av1",
            code if code
                .iter()
                .all(|byte| byte.is_ascii_graphic() || *byte == b' ') =>
            {
                return code.map(char::from).iter().collect();
            }
            _ => return format!("{fourcc:#010x}"),
        },
    };
    name.to_owned()
}

/// FFmpeg's name for little-endian PCM of `bits` per sample, integer or
/// `float`; `None` for a layout FFmpeg has no name for.
pub(crate) fn pcm_name(bits: u32, float: bool) -> Option<&'static str> {
    Some(match (bits, float) {
        (8, false) => "pcm_u8",
        (16, false) => "pcm_s16le",
        (24, false) => "pcm_s24le",
        (32, false) => "pcm_s32le",
        (32, true) => "pcm_f32le",
        (64, true) => "pcm_f64le",
        _ => return None,
    })
}

/// FFmpeg's name for an `MF_MT_VIDEO_PRIMARIES` value.
pub(crate) fn primaries_name(value: u32) -> Option<&'static str> {
    Some(match value {
        // MFVideoPrimaries_BT709
        2 => "bt709",
        // MFVideoPrimaries_BT470_2_SysM
        3 => "bt470m",
        // MFVideoPrimaries_BT470_2_SysBG, MFVideoPrimaries_EBU3213
        4 | 7 => "bt470bg",
        // MFVideoPrimaries_SMPTE170M, MFVideoPrimaries_SMPTE_C
        5 | 8 => "smpte170m",
        // MFVideoPrimaries_SMPTE240M
        6 => "smpte240m",
        // MFVideoPrimaries_BT2020
        9 => "bt2020",
        // MFVideoPrimaries_DCI_P3
        11 => "smpte431",
        _ => return None,
    })
}

/// FFmpeg's name for an `MF_MT_TRANSFER_FUNCTION` value.
pub(crate) fn transfer_name(value: u32) -> Option<&'static str> {
    Some(match value {
        // MFVideoTransFunc_10
        1 => "linear",
        // MFVideoTransFunc_22
        4 => "gamma22",
        // MFVideoTransFunc_709
        5 => "bt709",
        // MFVideoTransFunc_240M
        6 => "smpte240m",
        // MFVideoTransFunc_sRGB
        7 => "iec61966-2-1",
        // MFVideoTransFunc_28
        8 => "gamma28",
        // MFVideoTransFunc_2020
        13 => "bt2020-10",
        // MFVideoTransFunc_2084
        15 => "smpte2084",
        // MFVideoTransFunc_HLG
        16 => "arib-std-b67",
        _ => return None,
    })
}

/// FFmpeg's name for an `MF_MT_YUV_MATRIX` value.
pub(crate) fn matrix_name(value: u32) -> Option<&'static str> {
    Some(match value {
        // MFVideoTransferMatrix_BT709
        1 => "bt709",
        // MFVideoTransferMatrix_BT601
        2 => "smpte170m",
        // MFVideoTransferMatrix_SMPTE240M
        3 => "smpte240m",
        // MFVideoTransferMatrix_BT2020_10, MFVideoTransferMatrix_BT2020_12
        4 | 5 => "bt2020nc",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(name: &[u8; 4]) -> u32 {
        u32::from_le_bytes(*name)
    }

    #[test]
    fn codecs_have_ffmpeg_names() {
        for (fourcc, name) in [
            (code(b"H264"), "h264"),
            (code(b"avc1"), "h264"),
            (code(b"HEVC"), "hevc"),
            (code(b"HEVS"), "hevc"),
            (code(b"hvc1"), "hevc"),
            (code(b"hev1"), "hevc"),
            (code(b"mp4a"), "aac"),
            (code(b"MP4V"), "mpeg4"),
            (code(b"MJPG"), "mjpeg"),
            (code(b"VP90"), "vp9"),
            (code(b"AV01"), "av1"),
            (0x1610, "aac"),
            (0x1600, "aac"),
            (0x0001, "pcm_s16le"),
            (0x0003, "pcm_f32le"),
            (0x0055, "mp3"),
            (0x6C61, "alac"),
            (0xF1AC, "flac"),
            (0x704F, "opus"),
        ] {
            assert_eq!(codec_name(fourcc), name, "{fourcc:#010x}");
        }
        // Anything else is its code.
        assert_eq!(codec_name(code(b"xyz1")), "xyz1");
        assert_eq!(codec_name(0x0161), "0x00000161");
    }

    #[test]
    fn pcm_layouts_have_ffmpeg_names() {
        assert_eq!(pcm_name(8, false), Some("pcm_u8"));
        assert_eq!(pcm_name(16, false), Some("pcm_s16le"));
        assert_eq!(pcm_name(24, false), Some("pcm_s24le"));
        assert_eq!(pcm_name(32, false), Some("pcm_s32le"));
        assert_eq!(pcm_name(32, true), Some("pcm_f32le"));
        assert_eq!(pcm_name(64, true), Some("pcm_f64le"));
        assert_eq!(pcm_name(12, false), None);
    }

    #[test]
    fn primaries_have_ffmpeg_names() {
        assert_eq!(primaries_name(2), Some("bt709"));
        assert_eq!(primaries_name(9), Some("bt2020"));
        assert_eq!(primaries_name(5), Some("smpte170m"));
        assert_eq!(primaries_name(8), Some("smpte170m"));
        assert_eq!(primaries_name(7), Some("bt470bg"));
        assert_eq!(primaries_name(4), Some("bt470bg"));
        assert_eq!(primaries_name(3), Some("bt470m"));
        assert_eq!(primaries_name(6), Some("smpte240m"));
        assert_eq!(primaries_name(11), Some("smpte431"));
        assert_eq!(primaries_name(0), None);
        assert_eq!(primaries_name(12), None);
    }

    #[test]
    fn transfers_have_ffmpeg_names() {
        assert_eq!(transfer_name(5), Some("bt709"));
        assert_eq!(transfer_name(15), Some("smpte2084"));
        assert_eq!(transfer_name(16), Some("arib-std-b67"));
        assert_eq!(transfer_name(7), Some("iec61966-2-1"));
        assert_eq!(transfer_name(1), Some("linear"));
        assert_eq!(transfer_name(6), Some("smpte240m"));
        assert_eq!(transfer_name(13), Some("bt2020-10"));
        assert_eq!(transfer_name(4), Some("gamma22"));
        assert_eq!(transfer_name(8), Some("gamma28"));
        assert_eq!(transfer_name(0), None);
        assert_eq!(transfer_name(9), None);
    }

    #[test]
    fn matrices_have_ffmpeg_names() {
        assert_eq!(matrix_name(1), Some("bt709"));
        assert_eq!(matrix_name(2), Some("smpte170m"));
        assert_eq!(matrix_name(3), Some("smpte240m"));
        assert_eq!(matrix_name(4), Some("bt2020nc"));
        assert_eq!(matrix_name(5), Some("bt2020nc"));
        assert_eq!(matrix_name(0), None);
        assert_eq!(matrix_name(6), None);
    }

    /// Every literal above is the `windows` crate's constant of the name its
    /// comment gives.
    #[cfg(windows)]
    #[test]
    fn the_literals_are_the_crates_constants() {
        use windows::Win32::Media::MediaFoundation::*;
        for (guid, literal) in [
            (MFAudioFormat_PCM, 0x0000_0001),
            (MFAudioFormat_Float, 0x0000_0003),
            (MFAudioFormat_MPEG, 0x0000_0050),
            (MFAudioFormat_MP3, 0x0000_0055),
            (MFAudioFormat_AAC, 0x0000_1610),
            (MFAudioFormat_ADTS, 0x0000_1600),
            (MFAudioFormat_ALAC, 0x0000_6C61),
            (MFAudioFormat_FLAC, 0x0000_F1AC),
            (MFAudioFormat_Opus, 0x0000_704F),
            (MFVideoFormat_H264, code(b"H264")),
            (MFVideoFormat_HEVC, code(b"HEVC")),
            (MFVideoFormat_HEVC_ES, code(b"HEVS")),
            (MFVideoFormat_H265, code(b"H265")),
            (MFVideoFormat_MP4V, code(b"MP4V")),
            (MFVideoFormat_MJPG, code(b"MJPG")),
            (MFVideoFormat_VP80, code(b"VP80")),
            (MFVideoFormat_VP90, code(b"VP90")),
            (MFVideoFormat_AV1, code(b"AV01")),
        ] {
            assert_eq!(guid.data1, literal, "{guid:?}");
        }
        for (constant, literal) in [
            (MFVideoPrimaries_BT709, 2),
            (MFVideoPrimaries_BT470_2_SysM, 3),
            (MFVideoPrimaries_BT470_2_SysBG, 4),
            (MFVideoPrimaries_SMPTE170M, 5),
            (MFVideoPrimaries_SMPTE240M, 6),
            (MFVideoPrimaries_EBU3213, 7),
            (MFVideoPrimaries_SMPTE_C, 8),
            (MFVideoPrimaries_BT2020, 9),
            (MFVideoPrimaries_DCI_P3, 11),
        ] {
            assert_eq!(constant.0, literal, "{constant:?}");
        }
        for (constant, literal) in [
            (MFVideoTransFunc_10, 1),
            (MFVideoTransFunc_22, 4),
            (MFVideoTransFunc_709, 5),
            (MFVideoTransFunc_240M, 6),
            (MFVideoTransFunc_sRGB, 7),
            (MFVideoTransFunc_28, 8),
            (MFVideoTransFunc_2020, 13),
            (MFVideoTransFunc_2084, 15),
            (MFVideoTransFunc_HLG, 16),
        ] {
            assert_eq!(constant.0, literal, "{constant:?}");
        }
        for (constant, literal) in [
            (MFVideoTransferMatrix_BT709, 1),
            (MFVideoTransferMatrix_BT601, 2),
            (MFVideoTransferMatrix_SMPTE240M, 3),
            (MFVideoTransferMatrix_BT2020_10, 4),
            (MFVideoTransferMatrix_BT2020_12, 5),
        ] {
            assert_eq!(constant.0, literal, "{constant:?}");
        }
    }
}
