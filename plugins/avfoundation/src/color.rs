//! The colour names the VideoToolbox encoders take: BT.709 primaries,
//! transfer, and matrix in limited range, which they tag as
//! `kCVImageBuffer…_ITU_R_709_2` and the `420v` / `x420` video-range pixel
//! formats. Anything else is refused as the FFmpeg plugin words it.

use celesta_media::{Color, MediaError};

/// `Ok` for BT.709 primaries, transfer, and matrix in limited range, the one
/// colour the encoders tag; the first other name is refused, naming its
/// field.
pub(crate) fn check_color(color: &Color) -> Result<(), MediaError> {
    for (field, name, known) in [
        ("primaries", &color.primaries, "bt709"),
        ("transfer", &color.transfer, "bt709"),
        ("matrix", &color.matrix, "bt709"),
        ("range", &color.range, "limited"),
    ] {
        if name != known {
            return Err(MediaError::Codec(format!("unknown color {field} `{name}`")));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refusal(color: &Color) -> String {
        match check_color(color) {
            Err(MediaError::Codec(message)) => message,
            other => panic!("{color:?} was not refused: {other:?}"),
        }
    }

    #[test]
    fn only_bt709_limited_is_known() {
        assert!(check_color(&Color::bt709_limited()).is_ok());
        let nonsense = Color {
            matrix: "nonsense".into(),
            ..Color::bt709_limited()
        };
        assert!(
            refusal(&nonsense).contains("unknown color matrix `nonsense`"),
            "{}",
            refusal(&nonsense)
        );
        let full = Color {
            range: "full".into(),
            ..Color::bt709_limited()
        };
        assert!(refusal(&full).contains("unknown color range `full`"));
        let p3 = Color {
            primaries: "smpte432".into(),
            ..Color::bt709_limited()
        };
        assert!(refusal(&p3).contains("unknown color primaries `smpte432`"));
        let pq = Color {
            transfer: "smpte2084".into(),
            ..Color::bt709_limited()
        };
        assert!(refusal(&pq).contains("unknown color transfer `smpte2084`"));
        // A name is known only as spelled.
        let loose = Color {
            primaries: "BT709".into(),
            ..Color::bt709_limited()
        };
        assert!(refusal(&loose).contains("unknown color primaries `BT709`"));
    }
}
