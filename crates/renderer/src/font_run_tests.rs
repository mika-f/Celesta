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

/// Each glyph's line, byte offset in its line, source code point, and first
/// family name, with the text shaped at `width`.
fn glyph_families(
    rasterizer: &mut TextRasterizer,
    text: &str,
    style: &TextStyle,
    width: Option<f32>,
) -> Vec<(usize, usize, usize, String)> {
    let buffer = rasterizer.shaped_buffer(text, style, width, 1.0);
    let offsets = crate::shaping::source_offsets(&buffer, text, style);
    let database = rasterizer.font_system.db();
    let mut families = Vec::new();
    for layout in buffer.layout_runs() {
        for glyph in layout.glyphs {
            let family = database.face(glyph.font_id).unwrap().families[0].0.clone();
            let point = offsets[layout.line_i][glyph.start];
            families.push((layout.line_i, glyph.start, point, family));
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
    for (_, start, _, family) in glyph_families(&mut rasterizer, "ABCDEF", &spanned, None) {
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
        glyph_families(&mut rasterizer, "AB\r\nCD", &style, None),
        [
            (0, 0, 0, "Bebas Neue".to_owned()),
            (0, 1, 1, "IBM Plex Mono".to_owned()),
            (1, 0, 4, "IBM Plex Mono".to_owned()),
            (1, 1, 5, "Bebas Neue".to_owned()),
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
    for (byte, _) in line.text().match_indices(crate::linebreak::WORD_JOINER) {
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
fn emoji_in_a_run_use_the_color_emoji_font_when_the_text_family_is_one() {
    let mut rasterizer = two_family_rasterizer();
    let Some(emoji_family) = rasterizer.color_emoji_family() else {
        eprintln!("skipping: no color-emoji font available in this environment");
        return;
    };
    // The text's own family is the color emoji font, so no emoji span is
    // needed outside the run; inside a Plex Mono run the heart still has to
    // come from the color emoji font, not from a text font that has ❤.
    let style = TextStyle {
        font_family: Some(emoji_family.clone()),
        font_size: Some(48.0),
        font_runs: vec![run(0, 3, None, Some("IBM Plex Mono"))],
        ..TextStyle::default()
    };
    let heart = glyph_families(&mut rasterizer, "A❤\u{FE0F}B", &style, None)
        .into_iter()
        .find(|glyph| glyph.2 == 1)
        .unwrap();
    assert_eq!(heart.3, emoji_family);
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

#[test]
fn a_run_in_a_taller_family_does_not_move_the_baseline() {
    let mut rasterizer = two_family_rasterizer();
    let plain = TextStyle {
        line_height: Some(60.0),
        ..bebas(48.0)
    };
    let spanned = TextStyle {
        font_runs: vec![run(1, 2, None, Some("IBM Plex Mono"))],
        ..plain.clone()
    };
    let plain_metrics = rasterizer.measure("ABC\nDEF", &plain, None);
    let spanned_metrics = rasterizer.measure("ABC\nDEF", &spanned, None);
    // The recomputation mirrors cosmic-text's arithmetic; allow f32 rounding.
    assert!((spanned_metrics.ascent - plain_metrics.ascent).abs() < 1e-3);
    let plain_text = rasterizer.rasterize("ABC\nDEF", &plain, None, 1.0).unwrap();
    let spanned_text = rasterizer
        .rasterize("ABC\nDEF", &spanned, None, 1.0)
        .unwrap();
    // Padding for the taller run moves the image's top edge, not the
    // baseline within the layout: compare against the anchor box top.
    let baseline_in_box =
        |text: &crate::RasterizedText| text.baseline() - text.anchor_box.top as f32;
    assert!((baseline_in_box(&spanned_text) - baseline_in_box(&plain_text)).abs() < 1e-3);
}

#[test]
fn a_run_ending_inside_a_phrase_does_not_move_the_baseline() {
    use celesta_composition::LineBreak;
    let mut rasterizer = two_family_rasterizer();
    // Japanese, so the kanji and kana fall back to the same Japanese font
    // with or without runs; see `with_a_language_runs_leave_their_neighbors_fallback_fonts_alone`.
    let plain = TextStyle {
        line_break: Some(LineBreak::Phrase),
        lang: Some("ja".to_owned()),
        ..bebas(48.0)
    };
    let text = "今日は天気です";
    let plain_y = rasterizer
        .shaped_buffer(text, &plain, Some(1000.0), 1.0)
        .layout_runs()
        .next()
        .unwrap()
        .line_y;
    // 日 alone in Plex Mono: the word joiner phrase breaking puts after it
    // belongs to は, outside the run, and must keep the text's font.
    for runs in [
        vec![run(1, 2, None, Some("IBM Plex Mono"))],
        vec![run(0, 3, None, Some("IBM Plex Mono"))],
    ] {
        let spanned = TextStyle {
            font_runs: runs,
            ..plain.clone()
        };
        let buffer = rasterizer.shaped_buffer(text, &spanned, Some(1000.0), 1.0);
        let baselines = rasterizer.run_baselines(&buffer, text, &spanned).unwrap();
        assert!(
            (baselines[0] - plain_y).abs() < 1e-3,
            "{:?}: {} vs {plain_y}",
            spanned.font_runs,
            baselines[0]
        );
    }
}

#[test]
fn with_a_language_runs_leave_their_neighbors_fallback_fonts_alone() {
    use celesta_composition::LineBreak;
    // A run splits shaping, and cosmic-text picks fallback fonts per shaped
    // piece. Without a language, a piece next to a run can fall back to
    // another font than the whole phrase did (in an English locale, は went
    // from PingFang SC to Hiragino Sans), which can move baselines. With the
    // text's language, fallback follows it on both sides of the run.
    let mut rasterizer = two_family_rasterizer();
    let plain = TextStyle {
        line_break: Some(LineBreak::Phrase),
        lang: Some("ja".to_owned()),
        ..bebas(48.0)
    };
    let spanned = TextStyle {
        font_runs: vec![run(1, 2, None, Some("IBM Plex Mono"))],
        ..plain.clone()
    };
    let text = "今日は天気です";
    let before = glyph_families(&mut rasterizer, text, &plain, Some(1000.0));
    let after = glyph_families(&mut rasterizer, text, &spanned, Some(1000.0));
    // Joining 日 to a run in another font must not change how the text is
    // cut into glyphs, only which font draws the run's own ones.
    let points = |families: &[(usize, usize, usize, String)]| -> Vec<usize> {
        families.iter().map(|glyph| glyph.2).collect()
    };
    assert_eq!(points(&before), points(&after));
    let changed: Vec<_> = before
        .iter()
        .zip(&after)
        .filter(|(a, b)| a.3 != b.3 && !(1..2).contains(&b.2))
        .collect();
    assert!(
        changed.is_empty(),
        "fallback changed outside the run: {changed:?}"
    );
}

#[test]
fn a_line_entirely_in_a_run_lines_up_with_the_others() {
    let mut rasterizer = two_family_rasterizer();
    let plain = TextStyle {
        line_height: Some(60.0),
        ..bebas(48.0)
    };
    // Line 2 ("DEF") is all Plex Mono; its baseline follows the text's
    // other glyphs, so it sits exactly one line height below line 1's.
    let spanned = TextStyle {
        font_runs: vec![run(4, 7, None, Some("IBM Plex Mono"))],
        ..plain.clone()
    };
    let buffer = rasterizer.shaped_buffer("ABC\nDEF", &spanned, None, 1.0);
    let baselines = rasterizer
        .run_baselines(&buffer, "ABC\nDEF", &spanned)
        .unwrap();
    assert_eq!(baselines.len(), 2);
    assert!((baselines[1] - baselines[0] - 60.0).abs() < 1e-3);
}

#[test]
fn text_entirely_in_runs_keeps_cosmic_texts_baseline() {
    let mut rasterizer = two_family_rasterizer();
    let spanned = TextStyle {
        font_runs: vec![run(0, 3, None, Some("IBM Plex Mono"))],
        ..bebas(48.0)
    };
    let buffer = rasterizer.shaped_buffer("ABC", &spanned, None, 1.0);
    assert_eq!(rasterizer.run_baselines(&buffer, "ABC", &spanned), None);
    assert_eq!(rasterizer.run_baselines(&buffer, "ABC", &bebas(48.0)), None);
}

#[test]
fn a_taller_run_is_not_clipped_by_a_tight_line_box() {
    let mut rasterizer = two_family_rasterizer();
    // The text's own font fixes the baseline low in a 20 px line box, so
    // Plex Mono's taller "b" reaches well above it.
    let spanned = TextStyle {
        line_height: Some(20.0),
        font_runs: vec![run(1, 2, None, Some("IBM Plex Mono"))],
        ..bebas(48.0)
    };
    let alone = TextStyle {
        font_family: Some("IBM Plex Mono".to_owned()),
        line_height: Some(120.0),
        ..bebas(48.0)
    };
    // Ink in the columns `x..x + width` of `text`'s image.
    let ink = |text: &crate::RasterizedText, x: f64, width: f64| {
        let left = (x.floor() as usize).saturating_sub(1);
        let right = (x + width).ceil() as usize + 1;
        text.pixels()
            .chunks_exact(4)
            .enumerate()
            .filter(|(i, p)| {
                let column = i % text.width() as usize;
                column >= left && column < right && p[3] > 0
            })
            .count()
    };
    let b_alone = rasterizer.rasterize("b", &alone, None, 1.0).unwrap();
    let whole_b = ink(&b_alone, 0.0, f64::from(b_alone.width()));
    let glyph = rasterizer.measure("Ab\nA", &spanned, None).glyphs[1].clone();
    assert_eq!(glyph.text, "b");
    let drawn = rasterizer.rasterize("Ab\nA", &spanned, None, 1.0).unwrap();
    let drawn_b = ink(&drawn, glyph.x, glyph.width);
    // Subpixel placement moves a few edge pixels, hence the 3% slack.
    assert!(
        drawn_b as f64 >= whole_b as f64 * 0.97,
        "the run's glyph was cut off: {drawn_b} < {whole_b}"
    );
}

#[test]
fn warnings_judge_each_character_by_the_family_it_asks_for() {
    use crate::text::MissingGlyphs;
    let mut rasterizer = two_family_rasterizer();
    // Bebas has no kana; Plex Mono has the Latin letters Bebas draws.
    let style = TextStyle {
        font_runs: vec![run(2, 4, None, Some("IBM Plex Mono"))],
        ..bebas(48.0)
    };
    assert!(rasterizer.font_fallback("title", &style).is_empty());
    assert_eq!(
        rasterizer.missing_glyphs("title", "ABabず", &style),
        vec![MissingGlyphs {
            layer: "title".to_owned(),
            family: "Bebas Neue".to_owned(),
            weight: 400,
            characters: vec!['ず'],
        }]
    );
    // The run's own characters are judged against Plex Mono.
    let japanese_run = TextStyle {
        font_runs: vec![run(0, 1, None, Some("IBM Plex Mono"))],
        ..bebas(48.0)
    };
    let missing = rasterizer.missing_glyphs("title", "ずA", &japanese_run);
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].family, "IBM Plex Mono");
    assert_eq!(missing[0].characters, ['ず']);
}

#[test]
fn an_empty_run_asks_for_no_face() {
    let mut rasterizer = two_family_rasterizer();
    // Valid but covers nothing, so shaping drops it: no warning either.
    let style = TextStyle {
        font_runs: vec![run(1, 1, None, Some("Celesta Missing Family"))],
        ..bebas(48.0)
    };
    assert!(crate::validate_font_runs("AB", &style).is_ok());
    assert!(rasterizer.font_fallback("title", &style).is_empty());
}

#[test]
fn a_run_family_with_no_face_is_a_fallback() {
    let mut rasterizer = two_family_rasterizer();
    let style = TextStyle {
        font_runs: vec![run(1, 2, Some(700), Some("Celesta Missing Family"))],
        ..bebas(48.0)
    };
    let fallbacks = rasterizer.font_fallback("title", &style);
    assert_eq!(fallbacks.len(), 1);
    assert_eq!(fallbacks[0].family, "Celesta Missing Family");
    assert_eq!(fallbacks[0].weight, 700);
    assert!(rasterizer.missing_glyphs("title", "AB", &style).is_empty());
}

/// A face fontdb lists under `family` at `weight`, with no glyphs: a `name`
/// table and an `OS/2` table carrying only the weight. Tests that check which
/// family and weight shaping asks for use these, so they don't depend on the
/// fonts a machine has installed.
fn named_face(family: &str, weight: u16) -> Vec<u8> {
    let utf16 =
        |text: &str| -> Vec<u8> { text.encode_utf16().flat_map(u16::to_be_bytes).collect() };
    let family_name = utf16(family);
    let post_script = utf16(&format!("{}-{weight}", family.replace(' ', "")));
    let mut name = Vec::new();
    for value in [0_u16, 2, 6 + 2 * 12] {
        name.extend(value.to_be_bytes());
    }
    // Windows / Unicode BMP / en-US records for family (1) and PostScript (6) names.
    for (name_id, length, offset) in [
        (1_u16, family_name.len(), 0),
        (6, post_script.len(), family_name.len()),
    ] {
        for value in [3_u16, 1, 0x409, name_id, length as u16, offset as u16] {
            name.extend(value.to_be_bytes());
        }
    }
    name.extend(&family_name);
    name.extend(&post_script);
    name.resize(name.len().next_multiple_of(4), 0);
    // OS/2 version 0 is 78 bytes; usWeightClass sits at offset 4.
    let mut os2 = vec![0_u8; 78];
    os2[4..6].copy_from_slice(&weight.to_be_bytes());
    os2.resize(80, 0);

    let tables: [(&[u8; 4], &Vec<u8>); 2] = [(b"OS/2", &os2), (b"name", &name)];
    let mut font = Vec::new();
    font.extend(0x0001_0000_u32.to_be_bytes());
    for value in [tables.len() as u16, 32, 1, 0] {
        font.extend(value.to_be_bytes());
    }
    let mut offset = 12 + 16 * tables.len();
    for (tag, data) in &tables {
        font.extend(*tag);
        font.extend(0_u32.to_be_bytes());
        font.extend((offset as u32).to_be_bytes());
        font.extend((data.len() as u32).to_be_bytes());
        offset += data.len();
    }
    for (_, data) in &tables {
        font.extend(*data);
    }
    font
}

/// The attributes the buffer's first line asks shaping for at `byte`.
fn requested_at(buffer: &cosmic_text::Buffer, byte: usize) -> (String, u16) {
    let attrs = buffer.lines[0].attrs_list().get_span(byte);
    let family = match attrs.family {
        cosmic_text::Family::Name(name) => name.to_owned(),
        other => format!("{other:?}"),
    };
    (family, attrs.weight.0)
}

#[test]
fn a_weight_run_asks_for_the_family_face_at_that_weight_everywhere() {
    // Unlike `a_weight_run_draws_its_range_with_the_bold_face`, this needs no
    // installed bold face: the family's faces are made here.
    let mut rasterizer = TextRasterizer::new();
    for weight in [400, 700] {
        rasterizer
            .load_font_data(
                "weight-test",
                named_face("Celesta Weight Test", weight),
                None,
            )
            .unwrap();
    }
    assert_eq!(
        rasterizer.matched_weight("Celesta Weight Test", 700),
        Some(700)
    );
    let style = TextStyle {
        font_family: Some("Celesta Weight Test".to_owned()),
        font_size: Some(48.0),
        font_runs: vec![run(2, 4, Some(700), None)],
        ..TextStyle::default()
    };
    let buffer = rasterizer.shaped_buffer("abcdef", &style, None, 1.0);
    for (byte, weight) in [(0, 400), (1, 400), (2, 700), (3, 700), (4, 400), (5, 400)] {
        assert_eq!(
            requested_at(&buffer, byte),
            ("Celesta Weight Test".to_owned(), weight),
            "byte {byte}"
        );
    }
}

#[test]
fn emoji_ask_for_the_color_emoji_font_inside_runs_everywhere() {
    // Only the repository's fonts and a stand-in color emoji family, so the
    // check runs the same on every machine, with or without a real emoji font.
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/prism");
    let mut rasterizer = TextRasterizer::new();
    rasterizer.font_system = cosmic_text::FontSystem::new_with_locale_and_db(
        "en-US".to_owned(),
        cosmic_text::fontdb::Database::new(),
    );
    rasterizer.initial_db = rasterizer.font_system.db().clone();
    let font = |path: &str| ResolvedAsset {
        id: path.to_owned(),
        location: AssetLocation::File {
            path: path.to_owned(),
        },
    };
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
        .load_font_data("emoji-test", named_face("Noto Color Emoji", 400), None)
        .unwrap();
    assert_eq!(
        rasterizer.color_emoji_family().as_deref(),
        Some("Noto Color Emoji")
    );
    // "A❤️B": the heart starts at byte 1. The run covers it in both styles;
    // in the second, the text's own family is the color emoji font.
    for base in ["Bebas Neue", "Noto Color Emoji"] {
        let style = TextStyle {
            font_family: Some(base.to_owned()),
            font_size: Some(48.0),
            font_runs: vec![run(0, 3, None, Some("IBM Plex Mono"))],
            ..TextStyle::default()
        };
        let buffer = rasterizer.shaped_buffer("A❤\u{FE0F}B", &style, None, 1.0);
        assert_eq!(
            requested_at(&buffer, 0).0,
            "IBM Plex Mono",
            "text family {base}"
        );
        assert_eq!(
            requested_at(&buffer, 1).0,
            "Noto Color Emoji",
            "text family {base}"
        );
    }
}
