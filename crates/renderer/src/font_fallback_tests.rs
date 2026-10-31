use crate::font_run_tests::{bebas, glyph_families};
use crate::fonts::TextRasterizer;
use crate::text::MissingGlyphs;
use celesta_composition::{AssetLocation, FontFallbackMode, FontFamily, ResolvedAsset, TextStyle};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

// Of the two repository fonts, only IBM Plex Mono has Cyrillic (Б).

/// A directory with Bebas Neue, IBM Plex Mono, two more copies of Plex
/// Mono, and stylesheets: `split.css` puts Bebas and Plex under one family,
/// `a.css` and `b.css` load a copy of Plex each as 'Fallback A' and
/// 'Fallback B'.
fn fixture() -> TempDir {
    let directory = tempfile::tempdir().unwrap();
    let fonts = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/prism/assets/fonts");
    for (from, to) in [
        ("BebasNeue-Regular.ttf", "bebas.ttf"),
        ("IBMPlexMono-Regular.ttf", "plex.ttf"),
        ("IBMPlexMono-Regular.ttf", "plex-a.ttf"),
        ("IBMPlexMono-Regular.ttf", "plex-b.ttf"),
    ] {
        fs::copy(fonts.join(from), directory.path().join(to)).unwrap();
    }
    for (name, css) in [
        (
            "split.css",
            "@font-face {font-family: 'Celesta Split'; src: url(bebas.ttf)}
             @font-face {font-family: 'Celesta Split'; src: url(plex.ttf)}",
        ),
        (
            "a.css",
            "@font-face {font-family: 'Fallback A'; src: url(plex-a.ttf)}",
        ),
        (
            "b.css",
            "@font-face {font-family: 'Fallback B'; src: url(plex-b.ttf)}",
        ),
    ] {
        fs::write(directory.path().join(name), css).unwrap();
    }
    directory
}

fn load(rasterizer: &mut TextRasterizer, directory: &TempDir, files: &[&str]) {
    let fonts: Vec<ResolvedAsset> = files
        .iter()
        .map(|path| ResolvedAsset {
            id: (*path).to_owned(),
            location: AssetLocation::File {
                path: (*path).to_owned(),
            },
        })
        .collect();
    rasterizer.load_fonts(&fonts, directory.path()).unwrap();
}

fn names(rasterizer: &TextRasterizer) -> Vec<String> {
    rasterizer
        .fallback_families
        .iter()
        .map(|name| name.to_string())
        .collect()
}

#[test]
fn fallback_families_follow_the_font_list() {
    let directory = fixture();
    let mut rasterizer = TextRasterizer::new();
    load(&mut rasterizer, &directory, &["bebas.ttf", "plex.ttf"]);
    assert_eq!(names(&rasterizer), ["Bebas Neue", "IBM Plex Mono"]);
    // Nothing new loads, and the order still follows the list.
    load(&mut rasterizer, &directory, &["plex.ttf", "bebas.ttf"]);
    assert_eq!(names(&rasterizer), ["IBM Plex Mono", "Bebas Neue"]);
    // Bebas Neue stays loaded but is no candidate.
    load(&mut rasterizer, &directory, &["plex.ttf"]);
    assert_eq!(names(&rasterizer), ["IBM Plex Mono"]);
}

#[test]
fn a_stylesheet_lists_its_families() {
    let directory = fixture();
    let mut rasterizer = TextRasterizer::new();
    load(&mut rasterizer, &directory, &["split.css"]);
    // One family of two faces, which `family_faces` picks between.
    assert_eq!(names(&rasterizer), ["Celesta Split"]);
}

#[test]
fn a_stylesheet_loaded_twice_keeps_its_families() {
    let directory = fixture();
    let mut rasterizer = TextRasterizer::new();
    load(&mut rasterizer, &directory, &["split.css"]);
    let first = names(&rasterizer);
    load(&mut rasterizer, &directory, &["bebas.ttf"]);
    load(&mut rasterizer, &directory, &["split.css"]);
    assert_eq!(names(&rasterizer), first);
}

#[test]
fn fonts_revision_changes_with_the_list_order_only_when_it_changes() {
    let directory = fixture();
    let mut rasterizer = TextRasterizer::new();
    load(&mut rasterizer, &directory, &["bebas.ttf", "plex.ttf"]);
    let loaded = rasterizer.fonts_revision();
    load(&mut rasterizer, &directory, &["bebas.ttf", "plex.ttf"]);
    assert_eq!(rasterizer.fonts_revision(), loaded);
    load(&mut rasterizer, &directory, &["plex.ttf", "bebas.ttf"]);
    assert!(rasterizer.fonts_revision() > loaded);
}

#[test]
fn a_fork_keeps_the_fallback_families() {
    let directory = fixture();
    let mut rasterizer = TextRasterizer::new();
    load(&mut rasterizer, &directory, &["split.css", "a.css"]);
    let fork = rasterizer.fork();
    assert_eq!(names(&fork), names(&rasterizer));
    assert_eq!(fork.fonts_revision(), rasterizer.fonts_revision());
}

fn loaded(style: TextStyle) -> TextStyle {
    TextStyle {
        font_fallback: Some(FontFallbackMode::Loaded),
        ..style
    }
}

/// Each glyph's source code point and first family name, on one line.
fn families_by_point(
    rasterizer: &mut TextRasterizer,
    text: &str,
    style: &TextStyle,
) -> Vec<(usize, String)> {
    glyph_families(rasterizer, text, style, None)
        .into_iter()
        .map(|(_, _, point, family)| (point, family))
        .collect()
}

/// Every family name of the face that draws the glyph at code point `point`.
fn face_families_at(
    rasterizer: &mut TextRasterizer,
    text: &str,
    style: &TextStyle,
    point: usize,
) -> Vec<String> {
    let buffer = rasterizer.shaped_buffer(text, style, None, 1.0);
    let offsets = crate::shaping::source_offsets(&buffer, text, style);
    let database = rasterizer.font_system.db();
    buffer
        .layout_runs()
        .flat_map(|run| run.glyphs.iter().map(move |glyph| (run.line_i, glyph)))
        .find(|(line, glyph)| offsets[*line][glyph.start] == point)
        .map(|(_, glyph)| {
            database
                .face(glyph.font_id)
                .unwrap()
                .families
                .iter()
                .map(|(name, _)| name.clone())
                .collect()
        })
        .unwrap()
}

fn bebas_and_plex() -> (TempDir, TextRasterizer) {
    let directory = fixture();
    let mut rasterizer = TextRasterizer::new();
    load(&mut rasterizer, &directory, &["bebas.ttf", "plex.ttf"]);
    (directory, rasterizer)
}

#[test]
fn a_character_the_family_lacks_comes_from_a_loaded_font() {
    let (_directory, mut rasterizer) = bebas_and_plex();
    assert_eq!(
        families_by_point(&mut rasterizer, "AБ", &loaded(bebas(48.0))),
        [
            (0, "Bebas Neue".to_owned()),
            (1, "IBM Plex Mono".to_owned())
        ]
    );
    let system = families_by_point(&mut rasterizer, "AБ", &bebas(48.0));
    assert_ne!(system[1].1, "IBM Plex Mono");
}

#[test]
fn reordering_the_font_list_swaps_the_fallback() {
    let directory = fixture();
    let mut rasterizer = TextRasterizer::new();
    let style = loaded(bebas(48.0));
    load(
        &mut rasterizer,
        &directory,
        &["bebas.ttf", "a.css", "b.css"],
    );
    let stylesheet_family = |rasterizer: &mut TextRasterizer| {
        let [private]: [String; 1] = face_families_at(rasterizer, "AБ", &style, 1)
            .try_into()
            .unwrap();
        rasterizer
            .stylesheet_family_of(&private)
            .unwrap()
            .to_owned()
    };
    assert_eq!(stylesheet_family(&mut rasterizer), "Fallback A");
    load(
        &mut rasterizer,
        &directory,
        &["bebas.ttf", "b.css", "a.css"],
    );
    assert_eq!(stylesheet_family(&mut rasterizer), "Fallback B");
}

#[test]
fn measuring_follows_the_loaded_fallback() {
    let (_directory, mut rasterizer) = bebas_and_plex();
    let plex = TextStyle {
        font_family: Some("IBM Plex Mono".into()),
        ..bebas(48.0)
    };
    let pair = rasterizer.measure("A", &bebas(48.0), None).width
        + rasterizer.measure("Б", &plex, None).width;
    let style = loaded(bebas(48.0));
    let one_line = rasterizer.measure("AБ AБ", &style, None);
    let space = rasterizer.measure(" ", &bebas(48.0), None).width;
    assert!(
        (one_line.width - (2.0 * pair + space)).abs() < 0.5,
        "{} vs {}",
        one_line.width,
        2.0 * pair + space
    );
    let wrapped = rasterizer.measure("AБ AБ", &style, Some(pair + space / 2.0));
    assert_eq!(wrapped.line_starts, [0, 3]);
}

#[test]
fn loaded_fallback_characters_are_not_reported_missing() {
    let (_directory, mut rasterizer) = bebas_and_plex();
    assert_eq!(
        rasterizer.missing_glyphs("layer", "AБ", &loaded(bebas(48.0))),
        []
    );
    assert_eq!(
        rasterizer.missing_glyphs("layer", "AБ", &bebas(48.0)),
        [MissingGlyphs {
            layer: "layer".to_owned(),
            families: vec!["Bebas Neue".to_owned()],
            weight: 400,
            characters: vec!['Б'],
        }]
    );
}

#[test]
fn characters_no_loaded_font_has_are_still_reported() {
    let (_directory, mut rasterizer) = bebas_and_plex();
    let missing = rasterizer.missing_glyphs("layer", "AБ日", &loaded(bebas(48.0)));
    assert_eq!(missing.len(), 1, "{missing:?}");
    assert_eq!(missing[0].characters, ['日']);
}

#[test]
fn a_loaded_family_in_the_stack_is_not_repeated() {
    let (_directory, mut rasterizer) = bebas_and_plex();
    let stacked = TextStyle {
        font_family: Some(FontFamily::Stack(vec!["Bebas Neue".to_owned()])),
        ..loaded(bebas(48.0))
    };
    let named = loaded(bebas(48.0));
    assert_eq!(
        rasterizer.face_runs("AБA", &stacked),
        rasterizer.face_runs("AБA", &named)
    );
    let families = FontFamily::Stack(vec!["Bebas Neue".to_owned()]);
    assert_eq!(&*rasterizer.primary_family(&families, 400), "Bebas Neue");
}

#[test]
fn text_without_a_family_ignores_loaded_fallback() {
    let (_directory, mut rasterizer) = bebas_and_plex();
    assert!(
        rasterizer
            .face_runs("AБ", &loaded(TextStyle::default()))
            .is_empty()
    );
}

#[test]
fn without_the_option_runs_are_unchanged() {
    let (_directory, mut rasterizer) = bebas_and_plex();
    let stack = TextStyle {
        font_family: Some(FontFamily::Stack(vec![
            "Bebas Neue".to_owned(),
            "IBM Plex Mono".to_owned(),
        ])),
        ..bebas(48.0)
    };
    for style in [bebas(48.0), stack] {
        let system = TextStyle {
            font_fallback: Some(FontFallbackMode::System),
            ..style.clone()
        };
        assert_eq!(
            rasterizer.face_runs("AБ→B", &style),
            rasterizer.face_runs("AБ→B", &system)
        );
    }
    assert!(rasterizer.face_runs("AБ", &bebas(48.0)).is_empty());
}

/// `IBMPlexMono-Regular.ttf` with its OS/2 weight class set to `weight`,
/// so one family can have two weights with the same glyphs.
fn plex_at_weight(weight: u16) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/prism/assets/fonts/IBMPlexMono-Regular.ttf");
    let mut data = fs::read(path).unwrap();
    let tables = u16::from_be_bytes([data[4], data[5]]) as usize;
    let os2 = (0..tables)
        .map(|index| 12 + index * 16)
        .find(|&record| &data[record..record + 4] == b"OS/2")
        .map(|record| {
            u32::from_be_bytes(data[record + 8..record + 12].try_into().unwrap()) as usize
        })
        .unwrap();
    data[os2 + 4..os2 + 6].copy_from_slice(&weight.to_be_bytes());
    data
}

#[test]
fn a_stylesheet_falls_back_with_the_face_its_weight_matches() {
    let directory = fixture();
    fs::write(directory.path().join("plex-400.ttf"), plex_at_weight(400)).unwrap();
    fs::write(directory.path().join("plex-700.ttf"), plex_at_weight(700)).unwrap();
    fs::write(
        directory.path().join("weights.css"),
        "@font-face {font-family: 'Two Weights'; font-weight: 400; src: url(plex-400.ttf)}
         @font-face {font-family: 'Two Weights'; font-weight: 700; src: url(plex-700.ttf)}",
    )
    .unwrap();
    let mut rasterizer = TextRasterizer::new();
    load(&mut rasterizer, &directory, &["bebas.ttf", "weights.css"]);
    for weight in [400, 700] {
        let style = loaded(TextStyle {
            font_weight: Some(weight),
            ..bebas(48.0)
        });
        let buffer = rasterizer.shaped_buffer("AБ", &style, None, 1.0);
        let database = rasterizer.font_system.db();
        let drawn = buffer
            .layout_runs()
            .flat_map(|run| run.glyphs.iter())
            .find(|glyph| glyph.start == 1)
            .map(|glyph| database.face(glyph.font_id).unwrap().weight.0)
            .unwrap();
        assert_eq!(drawn, weight);
    }
}
