use crate::fonts::TextRasterizer;
use crate::linebreak::emoji_presentation_spans;
use crate::text::{FontFallback, MissingGlyphs};
use celesta_composition::TextStyle;
use cosmic_text::{FontSystem, fontdb};
use std::fs;
use std::path::Path;

use celesta_composition::{AssetLocation, ResolvedAsset};

const FAMILY: &str = "Celesta Web Font Test";

/// A minimal sfnt holding only a `name` table: enough for fontdb to list
/// a face under `FAMILY`.
fn sfnt() -> Vec<u8> {
    let utf16 =
        |text: &str| -> Vec<u8> { text.encode_utf16().flat_map(u16::to_be_bytes).collect() };
    let family = utf16(FAMILY);
    let post_script = utf16("CelestaWebFontTest");
    let mut name = Vec::new();
    for value in [0_u16, 2, 6 + 2 * 12] {
        name.extend(value.to_be_bytes());
    }
    // Windows / Unicode BMP / en-US records for family (1) and PostScript (6) names.
    for (name_id, length, offset) in [
        (1_u16, family.len(), 0),
        (6, post_script.len(), family.len()),
    ] {
        for value in [3_u16, 1, 0x409, name_id, length as u16, offset as u16] {
            name.extend(value.to_be_bytes());
        }
    }
    name.extend(&family);
    name.extend(&post_script);

    let mut font = Vec::new();
    font.extend(0x0001_0000_u32.to_be_bytes());
    for value in [1_u16, 16, 0, 0] {
        font.extend(value.to_be_bytes());
    }
    font.extend(b"name");
    font.extend(0_u32.to_be_bytes());
    font.extend(28_u32.to_be_bytes());
    font.extend((name.len() as u32).to_be_bytes());
    font.extend(&name);
    font.resize(font.len().next_multiple_of(4), 0);
    font
}

/// The `name` table bytes of `sfnt()`.
fn name_table() -> Vec<u8> {
    let font = sfnt();
    let length = u32::from_be_bytes(font[24..28].try_into().unwrap()) as usize;
    font[28..28 + length].to_vec()
}

/// `sfnt()` as an uncompressed WOFF 1.0 file.
fn woff1() -> Vec<u8> {
    let table = name_table();
    let padded = table.len().next_multiple_of(4);
    let mut woff = Vec::new();
    woff.extend(b"wOFF");
    woff.extend(0x0001_0000_u32.to_be_bytes());
    woff.extend(((44 + 20 + padded) as u32).to_be_bytes());
    woff.extend(1_u16.to_be_bytes());
    woff.extend(0_u16.to_be_bytes());
    woff.extend(((12 + 16 + padded) as u32).to_be_bytes());
    woff.extend(1_u16.to_be_bytes());
    woff.extend(0_u16.to_be_bytes());
    woff.extend([0; 20]);
    woff.extend(b"name");
    woff.extend(64_u32.to_be_bytes());
    woff.extend((table.len() as u32).to_be_bytes());
    woff.extend((table.len() as u32).to_be_bytes());
    woff.extend(0_u32.to_be_bytes());
    woff.extend(&table);
    woff.resize(44 + 20 + padded, 0);
    woff
}

/// `sfnt()` as a WOFF 2.0 file whose Brotli stream is one uncompressed
/// meta-block.
fn woff2() -> Vec<u8> {
    let table = name_table();
    // WBITS=16 (0), ISLAST=0, MNIBBLES=4 (00), MLEN-1 (16 bits),
    // ISUNCOMPRESSED=1, then byte-aligned raw bytes and an empty last block.
    let bits = ((table.len() as u32 - 1) << 4) | (1 << 20);
    let mut brotli = bits.to_le_bytes()[..3].to_vec();
    brotli.extend(&table);
    brotli.push(0b11);
    let mut woff = Vec::new();
    woff.extend(b"wOF2");
    woff.extend(0x0001_0000_u32.to_be_bytes());
    let length_at = woff.len();
    woff.extend(0_u32.to_be_bytes());
    woff.extend(1_u16.to_be_bytes());
    woff.extend(0_u16.to_be_bytes());
    woff.extend(((12 + 16 + table.len().next_multiple_of(4)) as u32).to_be_bytes());
    woff.extend((brotli.len() as u32).to_be_bytes());
    woff.extend(1_u16.to_be_bytes());
    woff.extend(0_u16.to_be_bytes());
    woff.extend([0; 20]);
    // Arbitrary tag (63) with the null transform, then UIntBase128 length.
    woff.push(63);
    woff.extend(b"name");
    assert!(table.len() < 128, "one UIntBase128 byte");
    woff.push(table.len() as u8);
    woff.extend(&brotli);
    woff.resize(woff.len().next_multiple_of(4), 0);
    let total = woff.len() as u32;
    woff[length_at..length_at + 4].copy_from_slice(&total.to_be_bytes());
    woff
}

fn has_family(rasterizer: &TextRasterizer, name: &str) -> bool {
    rasterizer
        .font_system
        .db()
        .faces()
        .any(|face| face.families.iter().any(|(family, _)| family == name))
}

fn file_font(path: &str) -> ResolvedAsset {
    ResolvedAsset {
        id: "brand".to_owned(),
        location: AssetLocation::File {
            path: path.to_owned(),
        },
    }
}

#[test]
fn loads_truetype_woff_and_woff2_fonts() {
    for (name, data) in [("a.ttf", sfnt()), ("a.woff", woff1()), ("a.woff2", woff2())] {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join(name), data).unwrap();
        let mut rasterizer = TextRasterizer::new();
        rasterizer
            .load_fonts(&[file_font(name)], directory.path())
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert!(has_family(&rasterizer, FAMILY), "{name} was not loaded");
    }
}

#[test]
fn loads_stylesheet_faces_under_their_css_family() {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("css")).unwrap();
    fs::create_dir(directory.path().join("files")).unwrap();
    fs::write(directory.path().join("files/brand.woff2"), woff2()).unwrap();
    fs::write(
        directory.path().join("css/brand.css"),
        "@font-face {\n  font-family: 'Brand';\n  src: local('Brand'), url('../files/brand.woff2?v=1') format('woff2');\n}\n",
    )
    .unwrap();
    let mut rasterizer = TextRasterizer::new();
    rasterizer
        .load_fonts(&[file_font("css/brand.css")], directory.path())
        .unwrap();
    // Under the family stored in the file and the stylesheet's CSS name.
    assert!(has_family(&rasterizer, FAMILY));
    assert!(has_family(&rasterizer, "Brand"));
}

/// A rasterizer with `Bebas Neue`, whose only face is Regular (400).
fn regular_only_rasterizer() -> TextRasterizer {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/prism");
    let mut rasterizer = TextRasterizer::new();
    rasterizer
        .load_fonts(
            &[file_font("assets/fonts/BebasNeue-Regular.ttf")],
            &examples,
        )
        .unwrap();
    rasterizer
}

#[test]
fn matches_the_nearest_weight_within_a_loaded_family() {
    let mut rasterizer = regular_only_rasterizer();
    let style = |weight| TextStyle {
        font_family: Some("Bebas Neue".to_owned()),
        font_size: Some(48.0),
        font_weight: weight,
        ..TextStyle::default()
    };
    let regular = rasterizer
        .rasterize("CELESTA", &style(Some(400)), None, 1.0)
        .unwrap();
    for weight in [None, Some(100), Some(700), Some(900)] {
        assert_eq!(
            rasterizer.matched_weight("Bebas Neue", weight.unwrap_or(400)),
            Some(400)
        );
        let text = rasterizer
            .rasterize("CELESTA", &style(weight), None, 1.0)
            .unwrap();
        assert!(
            text == regular,
            "weight {weight:?} did not draw with Bebas Neue"
        );
        assert_eq!(rasterizer.font_fallback("title", &style(weight)), None);
    }
}

#[test]
fn reports_a_family_with_no_face() {
    let mut rasterizer = regular_only_rasterizer();
    let style = TextStyle {
        font_family: Some("Celesta Missing Family".to_owned()),
        font_weight: Some(700),
        ..TextStyle::default()
    };
    let fallback = rasterizer.font_fallback("title", &style).unwrap();
    assert_eq!(
        fallback,
        FontFallback {
            layer: "title".to_owned(),
            family: "Celesta Missing Family".to_owned(),
            weight: 700,
        }
    );
    assert_eq!(
        fallback.to_string(),
        "font family \"Celesta Missing Family\" (weight 700) is not installed or loaded; text layer \"title\" uses a fallback font"
    );
    // Still drawn, with a fallback font.
    assert!(rasterizer.rasterize("A", &style, None, 1.0).is_ok());
    // No family means the default font, not a fallback.
    assert_eq!(
        rasterizer.font_fallback("title", &TextStyle::default()),
        None
    );
}

fn bebas_style() -> TextStyle {
    TextStyle {
        font_family: Some("Bebas Neue".to_owned()),
        font_size: Some(48.0),
        ..TextStyle::default()
    }
}

#[test]
fn a_fork_rasterizes_text_exactly_like_the_original() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/prism");
    let mut original = TextRasterizer::new();
    original
        .load_fonts(
            &[file_font("assets/fonts/BebasNeue-Regular.ttf")],
            &examples,
        )
        .unwrap();
    let mut fork = original.fork();
    assert_eq!(fork.loaded_font_count(), original.loaded_font_count());
    let plain = TextStyle {
        font_size: Some(36.0),
        ..TextStyle::default()
    };
    let cases = [
        ("Celesta", plain.clone(), None),
        (
            "CH07 +42.125",
            TextStyle {
                letter_spacing: Some(3.5),
                ..plain.clone()
            },
            None,
        ),
        // A loaded family, with characters it lacks falling back to
        // system fonts.
        ("CELESTA 日本語 🎉", bebas_style(), None),
        // Wrapped onto several lines.
        (
            "Celesta renders every frame of a composition",
            plain,
            Some(160.0),
        ),
    ];
    for (text, style, max_width) in cases {
        let expected = original.rasterize(text, &style, max_width, 1.5).unwrap();
        let actual = fork.rasterize(text, &style, max_width, 1.5).unwrap();
        assert_eq!(
            (actual.width(), actual.height()),
            (expected.width(), expected.height()),
            "{text}"
        );
        assert!(actual.into_pixels() == expected.into_pixels(), "{text}");
    }
}

#[test]
fn reports_characters_the_family_has_no_glyph_for() {
    let mut rasterizer = regular_only_rasterizer();
    // Bebas Neue has Latin glyphs only: the kana and kanji come from
    // another font, once each and in order.
    let missing = rasterizer
        .missing_glyphs("title", "CELESTA ずんだもん 2026 だ", &bebas_style())
        .unwrap();
    assert_eq!(
        missing,
        MissingGlyphs {
            layer: "title".to_owned(),
            family: "Bebas Neue".to_owned(),
            weight: 400,
            characters: "ずんだも".chars().collect(),
        }
    );
    assert_eq!(
        missing.to_string(),
        "font family \"Bebas Neue\" (weight 400) has no glyph for \"ずんだも\"; text layer \"title\" draws them with a fallback font"
    );

    // A long list is cut short.
    let missing = rasterizer
        .missing_glyphs("title", "あいうえおかきくけこさしすせそ", &bebas_style())
        .unwrap();
    assert_eq!(missing.characters.len(), 15);
    assert_eq!(
        missing.to_string(),
        "font family \"Bebas Neue\" (weight 400) has no glyph for \"あいうえおかきくけこ\" and 5 more characters; text layer \"title\" draws them with a fallback font"
    );
}

#[test]
fn does_not_report_glyphs_the_family_has_or_emoji() {
    let mut rasterizer = regular_only_rasterizer();
    for text in [
        "CELESTA 2026",
        // Whitespace and control characters draw nothing.
        "CELESTA\n\tSTUDIO\u{3000}",
        // Emoji are meant to come from a color emoji font.
        "🎉",
        "CELESTA 🎉👍🏽",
        "❤\u{FE0F}",
        "1\u{FE0F}\u{20E3}",
        "👩\u{200D}💻",
        "🇯🇵",
    ] {
        assert_eq!(
            rasterizer.missing_glyphs("title", text, &bebas_style()),
            None,
            "{text:?}"
        );
    }
    // Not when the text names no family, either.
    assert_eq!(
        rasterizer.missing_glyphs("title", "ずんだもん", &TextStyle::default()),
        None
    );
}

#[test]
fn reports_emoji_no_font_has_a_glyph_for() {
    // Only Bebas Neue, without the system's fonts: there is no color
    // emoji font to fall back to, so the emoji is drawn as a missing
    // glyph box and is reported like any other character.
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/prism");
    let mut rasterizer = TextRasterizer::new();
    rasterizer.font_system =
        FontSystem::new_with_locale_and_db("en-US".to_owned(), fontdb::Database::new());
    rasterizer
        .load_fonts(
            &[file_font("assets/fonts/BebasNeue-Regular.ttf")],
            &examples,
        )
        .unwrap();
    let missing = rasterizer
        .missing_glyphs("title", "CELESTA 🎉 ず ❤\u{FE0F}", &bebas_style())
        .unwrap();
    assert_eq!(missing.characters, ['🎉', 'ず', '❤']);
}

#[test]
fn leaves_a_family_with_no_face_to_the_font_fallback() {
    let mut rasterizer = regular_only_rasterizer();
    let style = TextStyle {
        font_family: Some("Celesta Missing Family".to_owned()),
        ..TextStyle::default()
    };
    assert!(rasterizer.font_fallback("title", &style).is_some());
    assert_eq!(
        rasterizer.missing_glyphs("title", "ずんだもん", &style),
        None
    );
}

#[test]
fn marks_graphemes_meant_as_emoji() {
    let spans = |text: &str| {
        emoji_presentation_spans(text)
            .into_iter()
            .map(|range| text[range].to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(spans("CELESTA ずんだもん 2026"), Vec::<String>::new());
    // Emoji by default, with a skin tone, and ZWJ sequences.
    assert_eq!(spans("A🎉B👍🏽C👩\u{200D}💻"), ["🎉", "👍🏽", "👩\u{200D}💻"]);
    // Text by default, emoji with U+FE0F: a heart and a keycap.
    assert_eq!(
        spans("❤ ❤\u{FE0F} 1 1\u{FE0F}\u{20E3}"),
        ["❤\u{FE0F}", "1\u{FE0F}\u{20E3}"]
    );
    // U+FE0E keeps an emoji-by-default character text.
    assert_eq!(spans("☔\u{FE0E}"), Vec::<String>::new());
    // An emoji in a grapheme that starts with a prepended character.
    assert_eq!(spans("\u{600}🎉"), ["\u{600}🎉"]);
    // A flag's regional indicators, and adjacent emoji, share one span.
    assert_eq!(spans("🇯🇵🎉 x"), ["🇯🇵🎉"]);
}

/// The family of the face each glyph cluster of `text` is drawn with.
fn cluster_families(
    rasterizer: &mut TextRasterizer,
    text: &str,
    style: &TextStyle,
) -> Vec<(String, String)> {
    let buffer = rasterizer.shaped_buffer(text, style, None, 1.0);
    let database = rasterizer.font_system.db();
    let mut clusters = Vec::new();
    for run in buffer.layout_runs() {
        for glyph in run.glyphs {
            let family = database
                .face(glyph.font_id)
                .and_then(|face| face.families.first())
                .map_or_else(String::new, |(family, _)| family.clone());
            clusters.push((run.text[glyph.start..glyph.end].to_owned(), family));
        }
    }
    clusters
}

#[test]
fn draws_emoji_presentation_with_the_color_emoji_font() {
    let mut rasterizer = regular_only_rasterizer();
    let Some(emoji_family) = rasterizer.color_emoji_family() else {
        eprintln!("skipping: no color emoji font is installed");
        return;
    };
    for weight in [None, Some(700)] {
        let style = TextStyle {
            font_family: Some("Bebas Neue".to_owned()),
            font_size: Some(48.0),
            font_weight: weight,
            ..TextStyle::default()
        };
        // Text fonts have a plain heart, keycap, and regional indicator
        // letters too; these still come from the color emoji font.
        for emoji in ["❤\u{FE0F}", "1\u{FE0F}\u{20E3}", "🇯🇵", "🎉"] {
            let text = format!("A{emoji}B");
            let clusters = cluster_families(&mut rasterizer, &text, &style);
            let families = clusters
                .iter()
                .map(|(_, family)| family.as_str())
                .collect::<Vec<_>>();
            assert_eq!(families.first(), Some(&"Bebas Neue"), "{clusters:?}");
            assert_eq!(families.last(), Some(&"Bebas Neue"), "{clusters:?}");
            assert!(
                families[1..families.len() - 1]
                    .iter()
                    .all(|family| *family == emoji_family),
                "{weight:?} {emoji:?} was not drawn with {emoji_family}: {clusters:?}"
            );
        }
    }
}

#[test]
fn keeps_the_line_structure_of_text_with_emoji() {
    let mut rasterizer = regular_only_rasterizer();
    if rasterizer.color_emoji_family().is_none() {
        eprintln!("skipping: no color emoji font is installed");
        return;
    }
    let style = TextStyle {
        font_family: Some("Bebas Neue".to_owned()),
        font_size: Some(48.0),
        line_height: Some(60.0),
        ..TextStyle::default()
    };
    // A trailing newline adds an empty last line, emoji or not.
    for (plain, emoji) in [
        ("A\n", "A❤\u{FE0F}\n"),
        ("A\nB", "A🎉\nB"),
        ("A\r\nB\r\n", "A🎉\r\nB🇯🇵\r\n"),
    ] {
        let plain_metrics = rasterizer.measure(plain, &style, None);
        let emoji_metrics = rasterizer.measure(emoji, &style, None);
        assert_eq!(
            (emoji_metrics.lines, emoji_metrics.height),
            (plain_metrics.lines, plain_metrics.height),
            "{emoji:?}"
        );
    }
}

#[test]
fn rejects_files_that_hold_no_font() {
    let directory = tempfile::tempdir().unwrap();
    let cases: [(&str, &[u8], &str); 3] = [
        ("broken.woff2", b"wOF2 not really", "invalid WOFF2 data"),
        ("notes.txt", b"not a font", "no font faces found"),
        (
            "empty.css",
            b"@font-face { font-family: X }",
            "no @font-face url()",
        ),
    ];
    for (name, data, reason) in cases {
        fs::write(directory.path().join(name), data).unwrap();
        let error = TextRasterizer::new()
            .load_fonts(&[file_font(name)], directory.path())
            .unwrap_err();
        assert!(error.to_string().contains(reason), "{name}: {error}");
    }
}
