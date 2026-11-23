use celesta_media::{DecodedColor, DecodedMatrix, DecodedRange, VideoColor};

pub(crate) fn color(tags: &VideoColor, range: DecodedRange) -> Option<DecodedColor> {
    if !matches!(
        tags.primaries.as_deref(),
        None | Some("bt709" | "smpte170m" | "bt470bg")
    ) || !matches!(tags.transfer.as_deref(), None | Some("bt709" | "smpte170m"))
    {
        return None;
    }
    let matrix = match tags.matrix.as_deref() {
        Some("bt709") => DecodedMatrix::Bt709,
        None | Some("bt470bg" | "smpte170m") => DecodedMatrix::Bt601,
        _ => return None,
    };
    Some(DecodedColor {
        matrix,
        range,
        chroma_location: Default::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untagged_is_601_limited_and_supported_tags_preserve_range() {
        assert_eq!(
            color(&VideoColor::default(), DecodedRange::Limited)
                .unwrap()
                .matrix,
            DecodedMatrix::Bt601
        );
        let tags = VideoColor {
            matrix: Some("bt709".into()),
            ..Default::default()
        };
        let answer = color(&tags, DecodedRange::Full).unwrap();
        assert_eq!(answer.matrix, DecodedMatrix::Bt709);
        assert_eq!(answer.range, DecodedRange::Full);
    }

    #[test]
    fn hdr_wide_gamut_and_unknown_matrices_are_not_native() {
        for tags in [
            VideoColor {
                transfer: Some("smpte2084".into()),
                ..Default::default()
            },
            VideoColor {
                transfer: Some("arib-std-b67".into()),
                ..Default::default()
            },
            VideoColor {
                primaries: Some("bt2020".into()),
                ..Default::default()
            },
            VideoColor {
                matrix: Some("bt2020nc".into()),
                ..Default::default()
            },
            VideoColor {
                matrix: Some("unknown".into()),
                ..Default::default()
            },
        ] {
            assert!(color(&tags, DecodedRange::Limited).is_none());
        }
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
                assert!(color(&tags, DecodedRange::Limited).is_some(), "{tags:?}");
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
            assert!(color(&tags, DecodedRange::Limited).is_none(), "{tags:?}");
        }
    }
}
