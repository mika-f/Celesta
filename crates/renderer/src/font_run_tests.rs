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

fn advance(rasterizer: &mut TextRasterizer, text: &str, style: &TextStyle) -> f64 {
    rasterizer.measure(text, style, None).width
}

/// Each glyph's line, byte offset in its line, and first family name.
fn glyph_families(rasterizer: &mut TextRasterizer, text: &str, style: &TextStyle) -> Vec<(usize, usize, String)> {
    let buffer = rasterizer.shaped_buffer(text, style, None, 1.0);
    let database = rasterizer.font_system.db();
    let mut families = Vec::new();
    for layout in buffer.layout_runs() {
        for glyph in layout.glyphs {
            let family = database.face(glyph.font_id).unwrap().families[0].0.clone();
            families.push((layout.line_i, glyph.start, family));
        }
    }
    families
}

#[test]
fn a_family_run_shapes_its_range_with_that_family() {
    let mut rasterizer = two_family_rasterizer();
    let plain = bebas(48.0);
    let spanned = TextStyle {
        font_runs: vec![run(2, 4, None, Some("IBM Plex Mono"))],
        ..plain.clone()
    };
    let mono = TextStyle {
        font_family: Some("IBM Plex Mono".to_owned()),
        ..plain.clone()
    };
    // "AB" in Bebas, "CD" in Plex Mono, "EF" in Bebas.
    let expected = advance(&mut rasterizer, "AB", &plain)
        + advance(&mut rasterizer, "CD", &mono)
        + advance(&mut rasterizer, "EF", &plain);
    let measured = rasterizer.measure("ABCDEF", &spanned, None);
    assert!(
        (measured.width - expected).abs() < 0.5,
        "{} vs {expected}",
        measured.width
    );
    assert!(measured.width > advance(&mut rasterizer, "ABCDEF", &plain) + 1.0);
    // Measuring and drawing use the same layout.
    let drawn = rasterizer.rasterize("ABCDEF", &spanned, None, 1.0).unwrap();
    assert_eq!(measured.width.ceil() as u32, drawn.anchor_box.width);
    for (_, start, family) in glyph_families(&mut rasterizer, "ABCDEF", &spanned) {
        let expected = if (2..4).contains(&start) {
            "IBM Plex Mono"
        } else {
            "Bebas Neue"
        };
        assert_eq!(family, expected, "glyph at {start}");
    }
}

#[test]
fn a_weight_run_draws_its_range_with_the_bold_face() {
    let mut rasterizer = TextRasterizer::new();
    let Some(family) = [
        "Helvetica Neue",
        "Arial",
        "DejaVu Sans",
        "Liberation Sans",
        "Noto Sans",
    ]
    .into_iter()
    .find(|family| {
        rasterizer.matched_weight(family, 400) == Some(400)
            && rasterizer.matched_weight(family, 700) == Some(700)
    }) else {
        eprintln!("skipping: no installed family with regular and bold faces");
        return;
    };
    let plain = TextStyle {
        font_family: Some(family.to_owned()),
        font_size: Some(48.0),
        ..TextStyle::default()
    };
    let bold = TextStyle {
        font_runs: vec![run(2, 4, Some(700), None)],
        ..plain.clone()
    };
    let buffer = rasterizer.shaped_buffer("abcdef", &bold, None, 1.0);
    for layout in buffer.layout_runs() {
        for glyph in layout.glyphs {
            let expected = if (2..4).contains(&glyph.start) {
                700
            } else {
                400
            };
            assert_eq!(glyph.font_weight.0, expected, "glyph at {}", glyph.start);
        }
    }
    let bold_width = rasterizer.measure("abcdef", &bold, None).width;
    let plain_width = rasterizer.measure("abcdef", &plain, None).width;
    assert!(
        (bold_width - plain_width).abs() > 0.5,
        "{bold_width} vs {plain_width}"
    );
}

#[test]
fn empty_font_runs_leave_the_layout_unchanged() {
    let mut rasterizer = two_family_rasterizer();
    let plain = bebas(48.0);
    let first = rasterizer.shaped_buffer("ABC 😀", &plain, None, 1.0);
    let again = rasterizer.shaped_buffer(
        "ABC 😀",
        &TextStyle {
            font_runs: Vec::new(),
            ..plain.clone()
        },
        None,
        1.0,
    );
    assert!(std::sync::Arc::ptr_eq(&first, &again));
}

#[test]
fn color_only_changes_reuse_font_run_layouts() {
    use celesta_composition::TextColorRun;
    let mut rasterizer = two_family_rasterizer();
    let spanned = TextStyle {
        font_runs: vec![run(1, 3, None, Some("IBM Plex Mono"))],
        ..bebas(48.0)
    };
    let first = rasterizer.shaped_buffer("ABCD", &spanned, None, 1.0);
    for color in ["#ff0000", "#00ff00"] {
        let painted = TextStyle {
            color_runs: vec![TextColorRun {
                start: 1,
                end: 3,
                color: color.to_owned(),
            }],
            ..spanned.clone()
        };
        assert!(std::sync::Arc::ptr_eq(
            &first,
            &rasterizer.shaped_buffer("ABCD", &painted, None, 1.0)
        ));
    }
    // A different run is a different layout.
    let other = TextStyle {
        font_runs: vec![run(0, 3, None, Some("IBM Plex Mono"))],
        ..spanned
    };
    assert!(!std::sync::Arc::ptr_eq(
        &first,
        &rasterizer.shaped_buffer("ABCD", &other, None, 1.0)
    ));
}

#[test]
fn font_runs_cross_crlf_line_endings() {
    let mut rasterizer = two_family_rasterizer();
    // Code points: A B \r \n C D; the run covers B through C.
    let style = TextStyle {
        font_runs: vec![run(1, 5, None, Some("IBM Plex Mono"))],
        ..bebas(48.0)
    };
    assert_eq!(
        glyph_families(&mut rasterizer, "AB\r\nCD", &style),
        [
            (0, 0, "Bebas Neue".to_owned()),
            (0, 1, "IBM Plex Mono".to_owned()),
            (1, 0, "IBM Plex Mono".to_owned()),
            (1, 1, "Bebas Neue".to_owned()),
        ]
    );
}

#[test]
fn a_run_starting_inside_a_cluster_leaves_the_cluster_alone() {
    let mut rasterizer = two_family_rasterizer();
    let plain = bebas(48.0);
    // "e" + U+0301 is one cluster; the run starts at the combining mark.
    let inside = TextStyle {
        font_runs: vec![run(1, 2, None, Some("IBM Plex Mono"))],
        ..plain.clone()
    };
    assert_eq!(
        rasterizer.measure("e\u{301}X", &inside, None),
        rasterizer.measure("e\u{301}X", &plain, None)
    );
}

#[test]
fn phrase_joiners_inside_a_run_keep_its_font() {
    use celesta_composition::LineBreak;
    let mut rasterizer = two_family_rasterizer();
    let style = TextStyle {
        font_family: Some("IBM Plex Mono".to_owned()),
        font_size: Some(32.0),
        letter_spacing: Some(2.0),
        line_break: Some(LineBreak::Phrase),
        font_runs: vec![run(0, 7, None, Some("Bebas Neue"))],
        ..TextStyle::default()
    };
    // Phrase breaking joins 今日は and 天気です with word joiners; inside
    // the run they must keep its family, not switch back to Plex Mono and
    // split the run's shaping.
    let buffer = rasterizer.shaped_buffer("今日は天気です", &style, Some(400.0), 1.0);
    let line = &buffer.lines[0];
    let mut joiners = 0;
    for (byte, _) in line
        .text()
        .match_indices(crate::linebreak::WORD_JOINER)
    {
        joiners += 1;
        assert_eq!(
            line.attrs_list().get_span(byte).family,
            cosmic_text::Family::Name("Bebas Neue"),
            "joiner at byte {byte}"
        );
    }
    assert!(joiners > 0, "phrase breaking inserted no joiners");
}

#[test]
fn emoji_inside_a_run_still_use_the_color_emoji_font() {
    let mut rasterizer = two_family_rasterizer();
    let Some(emoji_family) = rasterizer.color_emoji_family() else {
        eprintln!("skipping: no color-emoji font available in this environment");
        return;
    };
    let style = TextStyle {
        font_runs: vec![run(0, 3, None, Some("IBM Plex Mono"))],
        ..bebas(48.0)
    };
    let buffer = rasterizer.shaped_buffer("A❤\u{FE0F}B", &style, None, 1.0);
    let database = rasterizer.font_system.db();
    let heart = buffer
        .layout_runs()
        .flat_map(|layout| layout.glyphs.iter())
        .find(|glyph| glyph.start == 1)
        .unwrap();
    assert!(
        database
            .face(heart.font_id)
            .unwrap()
            .families
            .iter()
            .any(|(name, _)| *name == emoji_family)
    );
}
