use crate::RenderError;
use crate::fonts::TextRasterizer;
use celesta_composition::{AssetLocation, ResolvedAsset, TextFontRun, TextStyle};
use std::path::Path;

pub(crate) fn run(
    start: usize,
    end: usize,
    weight: Option<u16>,
    family: Option<&str>,
) -> TextFontRun {
    TextFontRun {
        start,
        end,
        font_weight: weight,
        font_family: family.map(str::to_owned),
    }
}

/// A rasterizer with the repository's Bebas Neue and IBM Plex Mono loaded.
pub(crate) fn two_family_rasterizer() -> TextRasterizer {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/prism");
    let font = |path: &str| ResolvedAsset {
        id: path.to_owned(),
        location: AssetLocation::File {
            path: path.to_owned(),
        },
    };
    let mut rasterizer = TextRasterizer::new();
    rasterizer
        .load_fonts(
            &[
                font("assets/fonts/BebasNeue-Regular.ttf"),
                font("assets/fonts/IBMPlexMono-Regular.ttf"),
            ],
            &examples,
        )
        .unwrap();
    rasterizer
}

pub(crate) fn bebas(size: f64) -> TextStyle {
    TextStyle {
        font_family: Some("Bebas Neue".to_owned()),
        font_size: Some(size),
        ..TextStyle::default()
    }
}

#[test]
fn font_runs_must_be_ordered_ranges_within_the_text() {
    let mut rasterizer = two_family_rasterizer();
    for runs in [
        vec![run(0, 5, Some(700), None)],
        vec![run(0, 2, Some(700), None), run(1, 3, Some(700), None)],
        vec![run(2, 1, Some(700), None)],
    ] {
        let style = TextStyle {
            font_runs: runs,
            ..bebas(40.0)
        };
        assert!(matches!(
            crate::validate_font_runs("ABCD", &style),
            Err(RenderError::InvalidTextFontRun { .. })
        ));
        assert!(matches!(
            rasterizer.rasterize("ABCD", &style, None, 1.0),
            Err(RenderError::InvalidTextFontRun { .. })
        ));
        // Shaping and measuring never panic on rejected runs.
        rasterizer.measure("ABCD", &style, None);
    }
    let style = TextStyle {
        font_runs: vec![
            run(0, 2, Some(700), None),
            run(2, 4, None, Some("IBM Plex Mono")),
        ],
        ..bebas(40.0)
    };
    assert!(crate::validate_font_runs("ABCD", &style).is_ok());
    assert_eq!(
        RenderError::InvalidTextFontRun {
            index: 1,
            start: 1,
            end: 3,
            text_length: 4
        }
        .to_string(),
        "text font run 1 (1..3) is invalid for 4 code points; runs must be ordered, non-overlapping ranges within the text"
    );
}
