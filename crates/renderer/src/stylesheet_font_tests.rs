//! `@font-face` descriptors and family hiding, drawn as a browser draws
//! them. The fonts are added as installed with `install_font`, so the tests
//! do not depend on the computer's fonts.

use crate::fonts::TextRasterizer;
use celesta_composition::{AssetLocation, FontFamily, ResolvedAsset, TextStyle};
use std::fs;
use std::path::Path;

fn example_font(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/prism/assets/fonts")
            .join(name),
    )
    .unwrap()
}

/// A rasterizer with Bebas Neue and IBM Plex Mono installed and `css`
/// loaded. Δ is only in Bebas Neue, Б only in IBM Plex Mono.
fn rasterizer(css: &str) -> TextRasterizer {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("plex.ttf"),
        example_font("IBMPlexMono-Regular.ttf"),
    )
    .unwrap();
    fs::write(directory.path().join("fonts.css"), css).unwrap();
    let mut rasterizer = TextRasterizer::new();
    rasterizer.install_font(example_font("BebasNeue-Regular.ttf"));
    rasterizer.install_font(example_font("IBMPlexMono-Regular.ttf"));
    rasterizer
        .load_fonts(
            &[ResolvedAsset {
                id: "fonts".to_owned(),
                location: AssetLocation::File {
                    path: "fonts.css".to_owned(),
                },
            }],
            directory.path(),
        )
        .unwrap();
    rasterizer
}

fn style(family: impl Into<FontFamily>, weight: u16) -> TextStyle {
    TextStyle {
        font_family: Some(family.into()),
        font_weight: Some(weight),
        font_size: Some(48.0),
        ..TextStyle::default()
    }
}

/// For each character of `text`, the face drawing it: its PostScript name,
/// the stylesheet family it is a face of (or "-"), and its weight.
fn drawn_by(rasterizer: &mut TextRasterizer, text: &str, style: &TextStyle) -> Vec<String> {
    let buffer = rasterizer.shaped_buffer(text, style, None, 1.0);
    let offsets = crate::shaping::source_offsets(&buffer, text, style);
    let mut glyphs: Vec<(usize, String)> = Vec::new();
    for run in buffer.layout_runs() {
        for glyph in run.glyphs {
            let face = rasterizer.font_system.db().face(glyph.font_id).unwrap();
            let family = face
                .families
                .iter()
                .find_map(|(name, _)| rasterizer.stylesheet_family_of(name))
                .unwrap_or("-");
            glyphs.push((
                offsets[run.line_i][glyph.start],
                format!("{} {family} {}", face.post_script_name, face.weight.0),
            ));
        }
    }
    glyphs.sort();
    glyphs.into_iter().map(|(_, face)| face).collect()
}

#[test]
fn unicode_range_limits_what_a_face_draws() {
    let mut rasterizer = rasterizer(
        "@font-face { font-family: Digits; src: local(BebasNeue-Regular); unicode-range: U+30-39 }",
    );
    let style = style("Digits", 400);
    let drawn = drawn_by(&mut rasterizer, "1A", &style);
    assert_eq!(drawn[0], "BebasNeue-Regular Digits 400");
    // Bebas Neue has an A, but the rule leaves it out: another font draws it.
    assert!(!drawn[1].contains("Digits"), "{drawn:?}");
    let [missing] = rasterizer
        .missing_glyphs("digits", "1A", &style)
        .try_into()
        .unwrap();
    assert_eq!(missing.characters, ['A']);
    assert!(rasterizer.font_fallback("digits", &style).is_empty());
    // The same in a fork.
    assert_eq!(drawn_by(&mut rasterizer.fork(), "1A", &style), drawn);
}

#[test]
fn a_stylesheet_family_hides_the_installed_family_of_its_name() {
    let mut rasterizer = rasterizer(
        "@font-face { font-family: 'Bebas Neue'; src: local(IBMPlexMono-Regular) }\
         @font-face { font-family: 'IBM Plex Mono'; src: local(Missing) }",
    );
    // Text in Bebas Neue draws the stylesheet's IBM Plex Mono, even a
    // character installed Bebas Neue has and it lacks.
    let drawn = drawn_by(&mut rasterizer, "AΔ", &style("Bebas Neue", 400));
    assert_eq!(drawn[0], "IBMPlexMono-Regular Bebas Neue 400");
    assert!(!drawn[1].starts_with("BebasNeue"), "{drawn:?}");
    // In a stack too.
    let stack = style(
        FontFamily::Stack(vec!["Bebas Neue".to_owned(), "Celesta None".to_owned()]),
        400,
    );
    assert_eq!(
        drawn_by(&mut rasterizer, "A", &stack),
        ["IBMPlexMono-Regular Bebas Neue 400"]
    );
    // A family whose face is not installed matches no installed font of
    // its name: it falls back, with the warning.
    let missing = style("IBM Plex Mono", 400);
    assert_eq!(rasterizer.font_fallback("plex", &missing).len(), 1);
    let drawn = drawn_by(&mut rasterizer, "Б", &missing);
    assert!(!drawn[0].contains("Plex"), "{drawn:?}");
}

#[test]
fn font_weight_and_font_style_replace_the_fonts_own() {
    let mut rasterizer = rasterizer(
        "@font-face { font-family: Pair; font-weight: 700; src: local(BebasNeue-Regular) }\
         @font-face { font-family: Pair; font-weight: 400; src: url(plex.ttf) }\
         @font-face { font-family: Slanted; font-style: italic; src: local(BebasNeue-Regular) }\
         @font-face { font-family: Slanted; font-style: oblique 20deg; src: url(plex.ttf) }",
    );
    assert_eq!(
        drawn_by(&mut rasterizer, "A", &style("Pair", 700)),
        ["BebasNeue-Regular Pair 700"]
    );
    assert_eq!(
        drawn_by(&mut rasterizer, "A", &style("Pair", 300)),
        ["IBMPlexMono-Regular Pair 400"]
    );
    assert_eq!(rasterizer.matched_weight("Pair", 600), Some(700));
    // Upright text takes an oblique face before an italic one.
    assert_eq!(
        drawn_by(&mut rasterizer, "A", &style("Slanted", 400)),
        ["IBMPlexMono-Regular Slanted 400"]
    );
}

#[test]
fn a_weight_matched_face_does_not_borrow_glyphs_from_other_weights() {
    let mut rasterizer = rasterizer(
        "@font-face { font-family: Split; src: local(IBMPlexMono-Regular) }\
         @font-face { font-family: Split; font-weight: 700; src: local(BebasNeue-Regular); unicode-range: U+30-39 }",
    );
    // At 700 only the digits face matches; the 400 face does not draw A.
    let drawn = drawn_by(&mut rasterizer, "1A", &style("Split", 700));
    assert_eq!(drawn[0], "BebasNeue-Regular Split 700");
    assert!(!drawn[1].contains("Split"), "{drawn:?}");
    let drawn = drawn_by(&mut rasterizer, "1A", &style("Split", 400));
    assert_eq!(
        drawn,
        [
            "IBMPlexMono-Regular Split 400",
            "IBMPlexMono-Regular Split 400"
        ]
    );
}

#[test]
fn overlapping_faces_try_the_last_rule_first() {
    let mut rasterizer = rasterizer(
        "@font-face { font-family: Both; src: local(BebasNeue-Regular) }\
         @font-face { font-family: Both; src: local(IBMPlexMono-Regular) }",
    );
    // Both have A; the later rule draws it. Only the earlier has Δ.
    assert_eq!(
        drawn_by(&mut rasterizer, "AΔБ", &style("Both", 400)),
        [
            "IBMPlexMono-Regular Both 400",
            "BebasNeue-Regular Both 400",
            "IBMPlexMono-Regular Both 400",
        ]
    );
}

#[test]
fn a_weight_range_draws_at_the_weight_asked_for() {
    let mut rasterizer =
        rasterizer("@font-face { font-family: Range; font-weight: 300 800; src: url(plex.ttf) }");
    for (asked, drawn) in [(400, 400), (650, 650), (200, 300), (900, 800)] {
        assert_eq!(rasterizer.matched_weight("Range", asked), Some(drawn));
        assert_eq!(
            drawn_by(&mut rasterizer, "A", &style("Range", asked)),
            [format!("IBMPlexMono-Regular Range {drawn}")],
            "{asked}"
        );
    }
    // A fork draws the copies its original made, and makes its own.
    let mut fork = rasterizer.fork();
    for weight in [650, 700] {
        assert_eq!(
            drawn_by(&mut fork, "A", &style("Range", weight)),
            [format!("IBMPlexMono-Regular Range {weight}")]
        );
    }
}

#[test]
fn one_file_under_several_rules_is_several_faces() {
    let mut rasterizer = rasterizer(
        "@font-face { font-family: Same; font-weight: 400; src: url(plex.ttf) }\
         @font-face { font-family: Same; font-weight: 700; src: url(plex.ttf) }\
         @font-face { font-family: Other; src: url('plex.ttf') }",
    );
    for weight in [400, 700] {
        assert_eq!(
            drawn_by(&mut rasterizer, "A", &style("Same", weight)),
            [format!("IBMPlexMono-Regular Same {weight}")]
        );
    }
    assert_eq!(
        drawn_by(&mut rasterizer, "A", &style("Other", 400)),
        ["IBMPlexMono-Regular Other 400"]
    );
    assert_eq!(rasterizer.loaded_font_count(), 2);
}
