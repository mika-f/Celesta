use crate::annexb::{Codec, nal_units, record_units};
use crate::params::{avc_sps_format, hevc_sps_info};
use celesta_media::{DecodedColor, DecodedMatrix, DecodedRange, MediaError, VideoColor};

pub(crate) fn even_size(width: u32, height: u32) -> bool {
    width != 0 && height != 0 && width.is_multiple_of(2) && height.is_multiple_of(2)
}

fn units(codec: Codec, header: &[u8]) -> Vec<&[u8]> {
    if header.first() == Some(&1) {
        record_units(codec, header).unwrap_or_default()
    } else {
        nal_units(header)
    }
}

pub(crate) fn source_signal(codec: Codec, header: &[u8]) -> Option<(bool, Option<u8>)> {
    let unit = units(codec, header).into_iter().find(|unit| match codec {
        Codec::H264 => unit[0] & 31 == 7,
        Codec::Hevc => unit[0] >> 1 & 63 == 33,
    })?;
    Some((
        crate::vui::sps_full_range(codec, unit)?,
        crate::vui::sps_colour(codec, unit).map(|c| c.matrix),
    ))
}

pub(crate) fn source_8bit420(codec: Codec, header: &[u8]) -> bool {
    let mut found = false;
    for unit in units(codec, header) {
        if codec == Codec::Hevc && (unit.len() < 2 || unit[0] & 1 != 0 || unit[1] & 0xf8 != 0) {
            return false;
        }
        let sps = match codec {
            Codec::H264 => unit[0] & 31 == 7,
            Codec::Hevc => unit[0] >> 1 & 63 == 33,
        };
        if !sps {
            continue;
        }
        found = true;
        if crate::vui::sps_left_chroma(codec, unit) == Some(false) {
            return false;
        }
        if crate::vui::sps_colour(codec, unit).is_some_and(|c| {
            !matches!(c.primaries, 1 | 2 | 5 | 6)
                || !matches!(c.transfer, 1 | 2 | 6)
                || !matches!(c.matrix, 1 | 2 | 5 | 6)
        }) {
            return false;
        }
        let eligible = match codec {
            Codec::H264 => {
                unit.len() >= 5
                    && (matches!(unit[1], 66 | 77 | 88) || avc_sps_format(unit) == Some((1, 0, 0)))
            }
            Codec::Hevc => hevc_sps_info(unit).is_some_and(|info| {
                info.profile & 31 == 1
                    && info.chroma_format == 1
                    && info.bit_depth_luma_minus8 == 0
                    && info.bit_depth_chroma_minus8 == 0
            }),
        };
        if !eligible {
            return false;
        }
    }
    found
}

pub(crate) fn color(tags: &VideoColor, range: u32, chroma: u32) -> Option<DecodedColor> {
    if !matches!(
        tags.primaries.as_deref(),
        None | Some("bt709" | "smpte170m" | "bt470bg")
    ) || !matches!(tags.transfer.as_deref(), None | Some("bt709" | "smpte170m"))
        || !matches!(chroma, 0 | 5)
    {
        return None;
    }
    Some(DecodedColor {
        matrix: match tags.matrix.as_deref() {
            None | Some("smpte170m" | "bt470bg") => DecodedMatrix::Bt601,
            Some("bt709") => DecodedMatrix::Bt709,
            _ => return None,
        },
        range: match range {
            0 | 2 => DecodedRange::Limited,
            1 => DecodedRange::Full,
            _ => return None,
        },
        chroma_location: Default::default(),
    })
}

pub(crate) fn matrix_matches(value: Option<u32>, expected: DecodedMatrix) -> bool {
    match value {
        None | Some(2) => expected == DecodedMatrix::Bt601,
        Some(1) => expected == DecodedMatrix::Bt709,
        _ => false,
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Crop {
    pub width: u32,
    pub height: u32,
    pub coded_width: u32,
    pub coded_height: u32,
    pub left: u32,
    pub top: u32,
}

impl Crop {
    pub fn extent(self, pitch: usize) -> Option<usize> {
        let Self {
            width,
            height,
            coded_width,
            coded_height,
            left,
            top,
        } = self;
        if [width, height, coded_width, coded_height].contains(&0)
            || [width, height, coded_width, coded_height, left, top]
                .iter()
                .any(|n| !n.is_multiple_of(2))
            || left.checked_add(width)? > coded_width
            || top.checked_add(height)? > coded_height
            || pitch < coded_width as usize
        {
            return None;
        }
        (coded_height as usize)
            .checked_add((top + height) as usize / 2 - 1)?
            .checked_mul(pitch)?
            .checked_add((left + width) as usize)
            .filter(|n| *n <= isize::MAX as usize)
    }

    pub fn pack(self, bytes: &[u8], pitch: usize) -> Result<Vec<u8>, MediaError> {
        let bad = || MediaError::Codec("invalid Media Foundation NV12 crop or pitch".into());
        let extent = self.extent(pitch).ok_or_else(bad)?;
        let bytes = bytes.get(..extent).ok_or_else(bad)?;
        let y = self.top as usize * pitch + self.left as usize;
        let uv = (self.coded_height as usize + self.top as usize / 2) * pitch + self.left as usize;
        celesta_media::pack_nv12(
            self.width,
            self.height,
            &bytes[y..],
            pitch,
            &bytes[uv..],
            pitch,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_eligibility_requires_nonzero_even_display_dimensions() {
        assert!(even_size(64, 36));
        assert!(even_size(1920, 1080));
        for (width, height) in [(63, 35), (63, 36), (64, 35), (0, 36), (64, 0), (0, 0)] {
            assert!(!even_size(width, height));
        }
    }

    #[test]
    fn aligned_1088_surface_crops_to_1080_and_strips_pitch() {
        let crop = Crop {
            width: 4,
            height: 1080,
            coded_width: 4,
            coded_height: 1088,
            left: 0,
            top: 0,
        };
        let mut bytes = vec![99; 8 * 1088 * 3 / 2];
        bytes[8 * 1088..].fill(17);
        let packed = crop.pack(&bytes, 8).unwrap();
        assert_eq!(packed.len(), 4 * 1080 * 3 / 2);
        assert!(packed[..4 * 1080].iter().all(|v| *v == 99));
        assert!(packed[4 * 1080..].iter().all(|v| *v == 17));
        let crop = Crop {
            width: 2,
            height: 2,
            coded_width: 4,
            coded_height: 4,
            left: 2,
            top: 2,
        };
        let bytes: Vec<u8> = (0..24).collect();
        assert_eq!(crop.pack(&bytes, 4).unwrap(), [10, 11, 14, 15, 22, 23]);
        assert!(crop.pack(&bytes[..23], 4).is_err());
        assert!(crop.pack(&bytes, 3).is_err());
        assert!(Crop { left: 1, ..crop }.pack(&bytes, 4).is_err());
        assert!(Crop { width: 0, ..crop }.pack(&[], 4).is_err());
        assert!(crop.extent(usize::MAX).is_none());
    }

    #[test]
    fn policy_rejects_unsupported_color_and_preserves_range() {
        assert_eq!(
            color(&VideoColor::default(), 0, 0).unwrap().matrix,
            DecodedMatrix::Bt601
        );
        let mut tags = VideoColor {
            matrix: Some("bt709".into()),
            ..Default::default()
        };
        assert_eq!(color(&tags, 1, 5).unwrap().range, DecodedRange::Full);
        assert!(color(&tags, 3, 5).is_none());
        assert!(color(&tags, 2, 7).is_none());
        tags.transfer = Some("smpte2084".into());
        assert!(color(&tags, 2, 5).is_none());
        tags.transfer = None;
        tags.matrix = Some("unknown".into());
        assert!(color(&tags, 2, 5).is_none());
    }

    #[test]
    fn negotiated_matrix_uses_the_untagged_601_rule() {
        for expected in [DecodedMatrix::Bt601, DecodedMatrix::Bt709] {
            for value in [None, Some(0), Some(1), Some(2), Some(3), Some(4), Some(99)] {
                let matches = match expected {
                    DecodedMatrix::Bt601 => matches!(value, None | Some(2)),
                    DecodedMatrix::Bt709 => value == Some(1),
                };
                assert_eq!(
                    matrix_matches(value, expected),
                    matches,
                    "{value:?} {expected:?}"
                );
            }
        }
    }

    #[test]
    fn source_requires_supported_sps_in_annex_b_or_record() {
        for (codec, sps) in [
            (
                Codec::H264,
                "6764001eac2cac47f9f01100000303e90000ea600da0884538",
            ),
            (
                Codec::Hevc,
                "420101014000000300b00000030000030003a020831f3f2a4edcb9808080000032000005da84",
            ),
        ] {
            let mut header = vec![0, 0, 0, 1];
            header.extend(
                (0..sps.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&sps[i..i + 2], 16).unwrap()),
            );
            assert!(source_8bit420(codec, &header));
            assert_eq!(source_signal(codec, &header), Some((false, None)));
            assert_eq!(crate::vui::sps_left_chroma(codec, &header[4..]), Some(true));
        }
        assert!(!source_8bit420(Codec::H264, &[]));
        assert!(!source_8bit420(Codec::Hevc, &[0, 0, 1, 0x42]));
        let baseline = [0, 0, 1, 0x67, 66, 0, 30, 0x80];
        assert!(source_8bit420(Codec::H264, &baseline));
        let mut high = baseline;
        high[4] = 110;
        assert!(!source_8bit420(Codec::H264, &high));
        let avcc = [1, 66, 0, 30, 0xff, 0xe1, 0, 5, 0x67, 66, 0, 30, 0x80, 0];
        assert!(source_8bit420(Codec::H264, &avcc));
        assert!(!source_8bit420(Codec::H264, &avcc[..10]));
    }

    #[test]
    fn only_untagged_or_601_709_primaries_and_transfer_are_native() {
        for primaries in [None, Some("bt709"), Some("smpte170m"), Some("bt470bg")] {
            for transfer in [None, Some("bt709"), Some("smpte170m")] {
                let tags = VideoColor {
                    primaries: primaries.map(str::to_owned),
                    transfer: transfer.map(str::to_owned),
                    ..Default::default()
                };
                assert!(color(&tags, 2, 5).is_some(), "{tags:?}");
            }
        }
        for (primaries, transfer) in [
            ("smpte432", "bt709"),
            ("smpte431", "bt709"),
            ("bt2020", "bt709"),
            ("unknown", "bt709"),
            ("bt709", "linear"),
            ("bt709", "iec61966-2-1"),
            ("bt709", "gamma22"),
            ("bt709", "smpte2084"),
            ("bt709", "unknown"),
        ] {
            let tags = VideoColor {
                primaries: Some(primaries.into()),
                transfer: Some(transfer.into()),
                ..Default::default()
            };
            assert!(color(&tags, 2, 5).is_none(), "{tags:?}");
        }
    }
}
