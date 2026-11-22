use celesta_media::{DecodedColor, DecodedMatrix, DecodedRange, VideoColor};

pub(crate) fn color(tags: &VideoColor, range: DecodedRange) -> Option<DecodedColor> {
    if tags.dynamic_range().is_hdr() || tags.primaries.as_deref() == Some("bt2020") {
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
}
