//! The names the decoder's probe gives, as the FFmpeg plugin's do: codecs by
//! FFmpeg's names for CoreMedia's four-character codes, and colour tags by
//! FFmpeg's names for CoreVideo's (`ITU_R_709_2` is `bt709`, …).

/// FFmpeg's name for the codec of a format description's media subtype:
/// `h264`, `hevc`, `prores`, `aac`, `pcm_s16le`, …; else the code itself.
/// CoreMedia describes every linear PCM as `lpcm`, whose layout
/// [`pcm_name`] names; alone, it is FFmpeg's default, `pcm_s16le`.
pub(crate) fn codec_name(fourcc: u32) -> String {
    let code = fourcc.to_be_bytes();
    let name = match &code {
        b"avc1" | b"avc3" => "h264",
        b"hvc1" | b"hev1" | b"dvh1" | b"dvhe" => "hevc",
        b"ap4h" | b"apch" | b"apcn" | b"apcs" | b"apco" | b"ap4x" => "prores",
        b"av01" => "av1",
        b"vp09" => "vp9",
        b"mp4v" => "mpeg4",
        b"jpeg" => "mjpeg",
        b"aac " | b"aach" | b"aacp" => "aac",
        b"lpcm" | b"sowt" => "pcm_s16le",
        b"twos" => "pcm_s16be",
        b"alac" => "alac",
        b"ac-3" => "ac3",
        b"ec-3" => "eac3",
        b".mp3" => "mp3",
        b"opus" => "opus",
        b"fLaC" => "flac",
        _ => return code.map(char::from).iter().collect(),
    };
    name.to_owned()
}

/// FFmpeg's name for linear PCM of `bits` per sample: `pcm_s16le`,
/// `pcm_f32be`, …; `None` for a layout FFmpeg has no name for.
pub(crate) fn pcm_name(bits: u32, float: bool, big_endian: bool) -> Option<&'static str> {
    Some(match (bits, float, big_endian) {
        (8, false, _) => "pcm_s8",
        (16, false, false) => "pcm_s16le",
        (16, false, true) => "pcm_s16be",
        (24, false, false) => "pcm_s24le",
        (24, false, true) => "pcm_s24be",
        (32, false, false) => "pcm_s32le",
        (32, false, true) => "pcm_s32be",
        (32, true, false) => "pcm_f32le",
        (32, true, true) => "pcm_f32be",
        (64, true, false) => "pcm_f64le",
        (64, true, true) => "pcm_f64be",
        _ => return None,
    })
}

/// FFmpeg's name for a `kCVImageBufferColorPrimaries_…` value.
pub(crate) fn primaries_name(cv: &str) -> Option<&'static str> {
    Some(match cv {
        "ITU_R_709_2" => "bt709",
        "ITU_R_2020" => "bt2020",
        "P3_D65" => "smpte432",
        "DCI_P3" => "smpte431",
        "SMPTE_C" => "smpte170m",
        "EBU_3213" => "bt470bg",
        _ => return None,
    })
}

/// FFmpeg's name for a `kCVImageBufferTransferFunction_…` value. sRGB's is
/// `IEC_sRGB`; `sRGB` is taken too.
pub(crate) fn transfer_name(cv: &str) -> Option<&'static str> {
    Some(match cv {
        "ITU_R_709_2" => "bt709",
        "SMPTE_ST_2084_PQ" => "smpte2084",
        "ITU_R_2100_HLG" => "arib-std-b67",
        "IEC_sRGB" | "sRGB" => "iec61966-2-1",
        "Linear" => "linear",
        "ITU_R_2020" => "bt2020-10",
        "SMPTE_240M_1995" => "smpte240m",
        "SMPTE_ST_428_1" => "smpte428",
        _ => return None,
    })
}

/// FFmpeg's name for a `kCVImageBufferYCbCrMatrix_…` value.
pub(crate) fn matrix_name(cv: &str) -> Option<&'static str> {
    Some(match cv {
        "ITU_R_709_2" => "bt709",
        "ITU_R_601_4" => "smpte170m",
        "ITU_R_2020" => "bt2020nc",
        "SMPTE_240M_1995" => "smpte240m",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(name: &[u8; 4]) -> u32 {
        u32::from_be_bytes(*name)
    }

    #[test]
    fn codecs_have_ffmpeg_names() {
        for (fourcc, name) in [
            (b"avc1", "h264"),
            (b"avc3", "h264"),
            (b"hvc1", "hevc"),
            (b"hev1", "hevc"),
            (b"ap4h", "prores"),
            (b"apch", "prores"),
            (b"apcn", "prores"),
            (b"apcs", "prores"),
            (b"apco", "prores"),
            (b"ap4x", "prores"),
            (b"aac ", "aac"),
            (b"lpcm", "pcm_s16le"),
        ] {
            assert_eq!(codec_name(code(fourcc)), name, "{fourcc:?}");
        }
        // Anything else is its code.
        assert_eq!(codec_name(code(b"xyz1")), "xyz1");
    }

    #[test]
    fn pcm_layouts_have_ffmpeg_names() {
        assert_eq!(pcm_name(16, false, false), Some("pcm_s16le"));
        assert_eq!(pcm_name(16, false, true), Some("pcm_s16be"));
        assert_eq!(pcm_name(24, false, false), Some("pcm_s24le"));
        assert_eq!(pcm_name(32, false, false), Some("pcm_s32le"));
        assert_eq!(pcm_name(32, true, false), Some("pcm_f32le"));
        assert_eq!(pcm_name(64, true, true), Some("pcm_f64be"));
        assert_eq!(pcm_name(8, false, false), Some("pcm_s8"));
        assert_eq!(pcm_name(12, false, false), None);
    }

    #[test]
    fn primaries_have_ffmpeg_names() {
        assert_eq!(primaries_name("ITU_R_709_2"), Some("bt709"));
        assert_eq!(primaries_name("ITU_R_2020"), Some("bt2020"));
        assert_eq!(primaries_name("P3_D65"), Some("smpte432"));
        assert_eq!(primaries_name("SMPTE_C"), Some("smpte170m"));
        assert_eq!(primaries_name("EBU_3213"), Some("bt470bg"));
        assert_eq!(primaries_name("unknown"), None);
    }

    #[test]
    fn transfers_have_ffmpeg_names() {
        assert_eq!(transfer_name("ITU_R_709_2"), Some("bt709"));
        assert_eq!(transfer_name("SMPTE_ST_2084_PQ"), Some("smpte2084"));
        assert_eq!(transfer_name("ITU_R_2100_HLG"), Some("arib-std-b67"));
        assert_eq!(transfer_name("sRGB"), Some("iec61966-2-1"));
        // `kCVImageBufferTransferFunction_sRGB` itself.
        assert_eq!(transfer_name("IEC_sRGB"), Some("iec61966-2-1"));
        assert_eq!(transfer_name("Linear"), Some("linear"));
        assert_eq!(transfer_name("UseGamma"), None);
    }

    #[test]
    fn matrices_have_ffmpeg_names() {
        assert_eq!(matrix_name("ITU_R_709_2"), Some("bt709"));
        assert_eq!(matrix_name("ITU_R_601_4"), Some("smpte170m"));
        assert_eq!(matrix_name("ITU_R_2020"), Some("bt2020nc"));
        assert_eq!(matrix_name("unknown"), None);
    }
}
