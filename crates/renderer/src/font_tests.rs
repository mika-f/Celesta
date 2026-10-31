use crate::RenderError;
use crate::fonts::{TextRasterizer, fallback_locale};
use crate::linebreak::emoji_presentation_spans;
use crate::text::{FontFallback, MissingGlyphs};
use celesta_composition::TextStyle;
use cosmic_text::{FontSystem, fontdb};
use std::fs;
use std::path::Path;

use celesta_composition::{AssetLocation, FontFallbackMode, ResolvedAsset};

const FAMILY: &str = "Celesta Web Font Test";

#[test]
fn normalizes_language_tags_for_cjk_fallback() {
    for (lang, expected) in [
        ("ja", "ja"),
        ("ja-JP", "ja"),
        ("JA_jp.UTF-8", "ja"),
        ("ko-KR", "ko"),
        ("zh", "zh-CN"),
        ("zh-Hans", "zh-CN"),
        ("zh-Hans-TW", "zh-CN"),
        ("zh-Hant", "zh-TW"),
        ("zh-Hant-TW", "zh-TW"),
        ("zh_HK", "zh-HK"),
        ("zh-hant-hk", "zh-HK"),
        ("zh-MO", "zh-HK"),
        ("zh-CN-x-tw", "zh-CN"),
        ("en-US", "en"),
        ("th-TH", "th"),
    ] {
        assert_eq!(fallback_locale(lang), expected, "{lang}");
    }
}

/// A minimal sfnt holding only a `name` table: enough for fontdb to list
/// a face under `FAMILY`.
fn sfnt() -> Vec<u8> {
    named_sfnt(&[(1, 0x409, FAMILY), (6, 0x409, "CelestaWebFontTest")])
}

/// A minimal sfnt whose `name` table holds Windows / Unicode BMP records
/// of `(name ID, language ID, value)`, in that order.
fn named_sfnt(records: &[(u16, u16, &str)]) -> Vec<u8> {
    let utf16 =
        |text: &str| -> Vec<u8> { text.encode_utf16().flat_map(u16::to_be_bytes).collect() };
    let mut name = Vec::new();
    let count = records.len() as u16;
    for value in [0_u16, count, 6 + count * 12] {
        name.extend(value.to_be_bytes());
    }
    let mut strings = Vec::new();
    for &(name_id, language, value) in records {
        let value = utf16(value);
        for field in [
            3_u16,
            1,
            language,
            name_id,
            value.len() as u16,
            strings.len() as u16,
        ] {
            name.extend(field.to_be_bytes());
        }
        strings.extend(value);
    }
    name.extend(&strings);

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
    // Under the stylesheet's CSS name only, as in a browser: the family
    // stored in the file matches nothing.
    assert!(rasterizer.matched_weight("Brand", 400).is_some());
    assert!(!has_family(&rasterizer, FAMILY));
    assert!(rasterizer.matched_weight(FAMILY, 400).is_none());
}

#[test]
fn stylesheets_with_no_loadable_source_say_so() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("legacy.css"),
        "@font-face { font-family: Legacy; src: url(a.eot) format('embedded-opentype'), url(a.svg#A) format('svg') }\
         @font-face { font-family: Unnamed; src: local('') }",
    )
    .unwrap();
    let error = TextRasterizer::new()
        .load_fonts(&[file_font("legacy.css")], directory.path())
        .unwrap_err();
    assert!(
        matches!(&error, RenderError::InvalidFont { reason, .. } if reason.contains("local() font name") && reason.contains("TrueType, OpenType, WOFF, or WOFF2")),
        "{error}"
    );
}

const INSTALLED: &str = "Celesta Installed Test";

/// A rasterizer on which a face of `INSTALLED` is installed, with the
/// full name "Celesta Installed Test Bold" in US English (and a French one)
/// and the PostScript name "CelestaInstalledTest-Bold".
fn installed_rasterizer() -> TextRasterizer {
    let mut rasterizer = TextRasterizer::new();
    rasterizer.install_font(named_sfnt(&[
        (1, 0x040c, INSTALLED),
        (4, 0x040c, "Celesta Installed Test Gras"),
        (1, 0x409, INSTALLED),
        (4, 0x409, "Celesta Installed Test Bold"),
        (6, 0x409, "CelestaInstalledTest-Bold"),
    ]));
    rasterizer
}

/// The PostScript names of the stylesheet family `family`'s faces, after
/// checking that each copy of them is listed under its private family only.
fn stylesheet_face_names(rasterizer: &TextRasterizer, family: &str) -> Vec<String> {
    let faces = rasterizer
        .stylesheet_faces
        .get(family)
        .map(Vec::as_slice)
        .unwrap_or_default();
    for face in faces {
        for (_, private) in face.instances() {
            let copies = face_families(rasterizer, private);
            assert_eq!(copies, vec![vec![private.to_string()]], "{family}");
        }
    }
    faces
        .iter()
        .map(|face| face.template().post_script_name.clone())
        .collect()
}

/// The family lists of the faces that match `family`.
fn face_families(rasterizer: &TextRasterizer, family: &str) -> Vec<Vec<String>> {
    rasterizer
        .font_system
        .db()
        .faces()
        .filter(|face| face.families.iter().any(|(name, _)| name == family))
        .map(|face| face.families.iter().map(|(name, _)| name.clone()).collect())
        .collect()
}

#[test]
fn stylesheet_local_faces_draw_installed_fonts_by_full_or_postscript_name() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("brand.woff2"), woff2()).unwrap();
    fs::write(
        directory.path().join("local.css"),
        "@font-face { font-family: ByFullName; src: local('celesta installed TEST bold') }\
         @font-face { font-family: ByPostScriptName; src: local(Missing), local(CELESTAINSTALLEDTEST-BOLD) }\
         @font-face { font-family: ByFamilyName; src: local('Celesta Installed Test') }\
         @font-face { font-family: ByFrenchName; src: local('Celesta Installed Test Gras') }\
         @font-face { font-family: NotInstalled; src: local(Missing) }\
         @font-face { font-family: Downloaded; src: local(CelestaInstalledTest-Bold), url(brand.woff2) }\
         @font-face { src: local(CelestaInstalledTest-Bold) }",
    )
    .unwrap();
    let mut rasterizer = installed_rasterizer();
    rasterizer
        .load_fonts(&[file_font("local.css")], directory.path())
        .unwrap();
    for family in ["ByFullName", "ByPostScriptName"] {
        assert_eq!(
            stylesheet_face_names(&rasterizer, family),
            vec!["CelestaInstalledTest-Bold"],
            "{family}"
        );
        assert!(!has_family(&rasterizer, family));
        assert!(rasterizer.fork().matched_weight(family, 400).is_some());
    }
    // Neither a family name nor a full name in another language names a face.
    for family in ["ByFamilyName", "ByFrenchName", "NotInstalled"] {
        assert!(
            stylesheet_face_names(&rasterizer, family).is_empty(),
            "{family}"
        );
        assert!(rasterizer.matched_weight(family, 400).is_none(), "{family}");
    }
    // Text in a family whose local() fonts are not installed falls back,
    // with the font fallback warning.
    let not_installed = TextStyle {
        font_family: Some("NotInstalled".into()),
        ..TextStyle::default()
    };
    assert_eq!(rasterizer.font_fallback("l", &not_installed).len(), 1);
    // A url() is drawn even after an installed local() font.
    assert_eq!(
        stylesheet_face_names(&rasterizer, "Downloaded"),
        vec!["CelestaWebFontTest"]
    );
    // The installed family keeps exactly its own face, and the last rule,
    // which has no font-family, adds none.
    assert_eq!(face_families(&rasterizer, INSTALLED).len(), 1);
    assert_eq!(
        rasterizer
            .font_system
            .db()
            .faces()
            .filter(|face| face.post_script_name == "CelestaInstalledTest-Bold")
            .count(),
        3
    );
}

#[test]
fn an_installed_face_is_added_once_per_stylesheet_family() {
    let directory = tempfile::tempdir().unwrap();
    // The same face, by two names in one stylesheet and again in another.
    fs::write(
        directory.path().join("a.css"),
        "@font-face { font-family: Brand; src: local(CelestaInstalledTest-Bold) }\
         @font-face { font-family: Brand; src: local('Celesta Installed Test Bold') }",
    )
    .unwrap();
    fs::write(
        directory.path().join("b.css"),
        "@font-face { font-family: Brand; src: local(CelestaInstalledTest-Bold) }",
    )
    .unwrap();
    let mut rasterizer = installed_rasterizer();
    rasterizer
        .load_fonts(&[file_font("a.css"), file_font("b.css")], directory.path())
        .unwrap();
    assert_eq!(
        stylesheet_face_names(&rasterizer, "Brand"),
        vec!["CelestaInstalledTest-Bold"]
    );
}

#[test]
fn stylesheet_local_faces_do_not_match_fonts_the_project_loaded() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("a.ttf"), sfnt()).unwrap();
    fs::write(
        directory.path().join("local.css"),
        "@font-face { font-family: Loaded; src: local(CelestaWebFontTest) }",
    )
    .unwrap();
    let mut rasterizer = TextRasterizer::new();
    rasterizer
        .load_fonts(
            &[file_font("a.ttf"), file_font("local.css")],
            directory.path(),
        )
        .unwrap();
    assert!(has_family(&rasterizer, FAMILY));
    assert!(!has_family(&rasterizer, "Loaded"));
}

#[test]
fn stylesheet_local_faces_draw_like_the_installed_font() {
    let directory = tempfile::tempdir().unwrap();
    let bebas = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/prism/assets/fonts/BebasNeue-Regular.ttf"),
    )
    .unwrap();
    fs::write(
        directory.path().join("local.css"),
        "@font-face { font-family: Headline; src: local(BebasNeue-Regular) }",
    )
    .unwrap();
    let mut rasterizer = TextRasterizer::new();
    rasterizer.install_font(bebas);
    rasterizer
        .load_fonts(&[file_font("local.css")], directory.path())
        .unwrap();
    let style = |family: &str| TextStyle {
        font_family: Some(family.into()),
        font_size: Some(48.0),
        ..TextStyle::default()
    };
    // Text in the stylesheet family, the same in a fork, and text falling
    // back to the font list's families all draw the installed font.
    let mut fork = rasterizer.fork();
    let loaded_fallback = TextStyle {
        font_fallback: Some(FontFallbackMode::Loaded),
        ..style("No Such Family")
    };
    let bebas = rasterizer
        .rasterize("CELESTA", &style("Bebas Neue"), None, 1.0)
        .unwrap();
    let bebas_size = (bebas.width(), bebas.height());
    let bebas = bebas.into_pixels();
    for (case, drawn) in [
        (
            "family",
            rasterizer.rasterize("CELESTA", &style("Headline"), None, 1.0),
        ),
        (
            "fork",
            fork.rasterize("CELESTA", &style("Headline"), None, 1.0),
        ),
        (
            "loaded fallback",
            rasterizer.rasterize("CELESTA", &loaded_fallback, None, 1.0),
        ),
    ] {
        let drawn = drawn.unwrap();
        assert_eq!((drawn.width(), drawn.height()), bebas_size, "{case}");
        assert!(drawn.into_pixels() == bebas, "{case}");
    }
    assert!(
        rasterizer
            .font_fallback("headline", &style("Headline"))
            .is_empty()
    );
}

#[test]
fn stylesheet_urls_that_do_not_parse_are_reported_for_the_font() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("brand.css"),
        "@font-face { font-family: Brand; src: url('https://exa mple.com/a.woff2') }",
    )
    .unwrap();
    let error = TextRasterizer::new()
        .load_fonts(&[file_font("brand.css")], directory.path())
        .unwrap_err();
    assert!(
        matches!(
            &error,
            RenderError::RemoteAsset {
                source: celesta_remote::RemoteAssetError::InvalidUrl { url, .. },
                ..
            } if url == "https://exa mple.com/a.woff2"
        ),
        "{error}"
    );
}

#[test]
fn stylesheet_subsets_select_each_file_for_its_glyphs_including_stacks_and_forks() {
    let directory = tempfile::tempdir().unwrap();
    let fonts = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/prism/assets/fonts");
    for name in ["BebasNeue-Regular.ttf", "IBMPlexMono-Regular.ttf"] {
        fs::copy(fonts.join(name), directory.path().join(name)).unwrap();
    }
    fs::write(
        directory.path().join("split.css"),
        "@font-face {font-family: 'Celesta Split'; src: url(BebasNeue-Regular.ttf)}
         @font-face {font-family: 'Celesta Split'; src: url(IBMPlexMono-Regular.ttf)}",
    )
    .unwrap();
    let mut rasterizer = TextRasterizer::new();
    rasterizer
        .load_fonts(&[file_font("split.css")], directory.path())
        .unwrap();
    // Δ exists only in Bebas Neue, Б only in IBM Plex Mono. One family
    // must select both files, as a Japanese Unicode-subset stylesheet does.
    for family in [
        celesta_composition::FontFamily::Name("Celesta Split".to_owned()),
        celesta_composition::FontFamily::Stack(vec![
            "Celesta Split".to_owned(),
            "Bebas Neue".to_owned(),
        ]),
    ] {
        for lang in ["ja-JP", "en-US"] {
            let style = TextStyle {
                font_family: Some(family.clone()),
                lang: Some(lang.to_owned()),
                ..TextStyle::default()
            };
            assert!(rasterizer.missing_glyphs("split", "ΔБ", &style).is_empty());
            assert!(
                rasterizer
                    .fork()
                    .missing_glyphs("split", "ΔБ", &style)
                    .is_empty()
            );
        }
    }
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
        font_family: Some("Bebas Neue".into()),
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
        assert!(rasterizer.font_fallback("title", &style(weight)).is_empty());
    }
}

#[test]
fn reports_a_family_with_no_face() {
    let mut rasterizer = regular_only_rasterizer();
    let style = TextStyle {
        font_family: Some("Celesta Missing Family".into()),
        font_weight: Some(700),
        ..TextStyle::default()
    };
    let [fallback]: [FontFallback; 1] = rasterizer
        .font_fallback("title", &style)
        .try_into()
        .unwrap();
    assert_eq!(
        fallback,
        FontFallback {
            layer: "title".to_owned(),
            families: vec!["Celesta Missing Family".to_owned()],
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
    assert!(
        rasterizer
            .font_fallback("title", &TextStyle::default())
            .is_empty()
    );
}

fn bebas_style() -> TextStyle {
    TextStyle {
        font_family: Some("Bebas Neue".into()),
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
    original.select_language(Some("zh-Hant"));
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
                letter_spacing: Some(3.5.into()),
                ..plain.clone()
            },
            None,
        ),
        // A loaded family, with characters it lacks falling back to
        // system fonts.
        ("CELESTA 日本語 🎉", bebas_style(), None),
        (
            "CELESTA 日本語 🎉",
            TextStyle {
                lang: Some("ja-JP".to_owned()),
                ..bebas_style()
            },
            None,
        ),
        (
            "CELESTA 漢字 🎉",
            TextStyle {
                lang: Some("zh-Hant".to_owned()),
                ..bebas_style()
            },
            None,
        ),
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
fn loading_fonts_updates_every_language_and_preserves_the_default_in_forks() {
    let mut rasterizer = TextRasterizer::new();
    rasterizer.select_language(Some("ja"));
    rasterizer.select_language(Some("zh-CN"));
    // Under a family no computer has installed.
    let directory = tempfile::tempdir().unwrap();
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/prism/assets/fonts/BebasNeue-Regular.ttf"),
        directory.path().join("bebas.ttf"),
    )
    .unwrap();
    fs::write(
        directory.path().join("locale.css"),
        format!("@font-face {{ font-family: '{FAMILY}'; src: url(bebas.ttf) }}"),
    )
    .unwrap();
    rasterizer
        .load_fonts(&[file_font("locale.css")], directory.path())
        .unwrap();
    let mut fork = rasterizer.fork();
    for rasterizer in [&mut rasterizer, &mut fork] {
        for lang in [Some("ja"), Some("zh-CN"), Some("zh-Hant"), None, Some("")] {
            let style = TextStyle {
                lang: lang.map(str::to_owned),
                font_family: Some(FAMILY.into()),
                ..TextStyle::default()
            };
            assert!(
                rasterizer
                    .missing_glyphs("title", "CELESTA", &style)
                    .is_empty()
            );
            assert!(rasterizer.font_fallback("title", &style).is_empty());
            if lang.is_none() || lang == Some("") {
                assert_eq!(rasterizer.font_system.locale(), rasterizer.default_locale);
            }
        }
    }
}

#[cfg(target_os = "macos")]
#[test]
fn selects_japanese_and_chinese_kanji_per_layer_on_macos() {
    let mut rasterizer = regular_only_rasterizer();
    // Start with an English default to verify explicit Japanese works on
    // any host, and omitted language restores the system fallback.
    rasterizer.default_locale = "en".to_owned();
    for family in [None, Some("Bebas Neue")] {
        for line_break in [
            celesta_composition::LineBreak::Normal,
            celesta_composition::LineBreak::Phrase,
        ] {
            for (lang, expected) in [
                (Some("ja-JP"), "Hiragino Sans"),
                (Some("zh-CN"), "PingFang SC"),
                (Some("ja"), "Hiragino Sans"),
                (Some("zh-Hant-TW"), "PingFang TC"),
                (None, "PingFang SC"),
            ] {
                // Some macOS images omit optional fonts such as PingFang.
                // Assert the exact family only when it is available; still
                // verify drawable glyphs and matching metrics in every case.
                let expected_available = has_family(&rasterizer, expected);
                let style = TextStyle {
                    lang: lang.map(str::to_owned),
                    font_family: family.map(Into::into),
                    line_break: Some(line_break),
                    ..TextStyle::default()
                };
                let buffer = rasterizer.shaped_buffer("漢字とかな", &style, Some(400.0), 1.0);
                let mut kanji = 0;
                for run in buffer.layout_runs() {
                    for glyph in run.glyphs {
                        let cluster = &run.text[glyph.start..glyph.end];
                        if !cluster.contains(['漢', '字']) {
                            continue;
                        }
                        kanji += 1;
                        assert_ne!(glyph.glyph_id, 0);
                        let face = rasterizer.font_system.db().face(glyph.font_id).unwrap();
                        if expected_available {
                            assert!(
                                face.families.iter().any(|(name, _)| name == expected),
                                "{lang:?}, {family:?}, {line_break:?}, {cluster:?}: {:?}",
                                face.families
                            );
                        }
                    }
                }
                assert!(kanji > 0);
                let measured = rasterizer.measure("漢字とかな", &style, None);
                let drawn = rasterizer
                    .rasterize("漢字とかな", &style, None, 1.0)
                    .unwrap();
                assert_eq!(measured.width.ceil() as u32, drawn.width());
            }
        }
    }
}

#[test]
fn reports_characters_the_family_has_no_glyph_for() {
    let mut rasterizer = regular_only_rasterizer();
    // Bebas Neue has Latin glyphs only: the kana and kanji come from
    // another font, once each and in order.
    let [missing]: [MissingGlyphs; 1] = rasterizer
        .missing_glyphs("title", "CELESTA ずんだもん 2026 だ", &bebas_style())
        .try_into()
        .unwrap();
    assert_eq!(
        missing,
        MissingGlyphs {
            layer: "title".to_owned(),
            families: vec!["Bebas Neue".to_owned()],
            weight: 400,
            characters: "ずんだも".chars().collect(),
        }
    );
    assert_eq!(
        missing.to_string(),
        "font family \"Bebas Neue\" (weight 400) has no glyph for \"ずんだも\"; text layer \"title\" draws them with a fallback font"
    );

    // A long list is cut short.
    let [missing]: [MissingGlyphs; 1] = rasterizer
        .missing_glyphs("title", "あいうえおかきくけこさしすせそ", &bebas_style())
        .try_into()
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
        assert!(
            rasterizer
                .missing_glyphs("title", text, &bebas_style())
                .is_empty(),
            "{text:?}"
        );
    }
    // Not when the text names no family, either.
    assert!(
        rasterizer
            .missing_glyphs("title", "ずんだもん", &TextStyle::default())
            .is_empty()
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
    rasterizer.initial_db = rasterizer.font_system.db().clone();
    rasterizer
        .load_fonts(
            &[file_font("assets/fonts/BebasNeue-Regular.ttf")],
            &examples,
        )
        .unwrap();
    let [missing]: [MissingGlyphs; 1] = rasterizer
        .missing_glyphs("title", "CELESTA 🎉 ず ❤\u{FE0F}", &bebas_style())
        .try_into()
        .unwrap();
    assert_eq!(missing.characters, ['🎉', 'ず', '❤']);
}

#[test]
fn missing_glyph_cache_survives_language_switches_and_clears_when_fonts_load() {
    let mut rasterizer = regular_only_rasterizer();
    let text = "CELESTA 漢字";
    let mut style = bebas_style();
    let mut expected = Vec::new();
    for lang in ["ja", "zh-Hant"] {
        style.lang = Some(lang.to_owned());
        expected.push(rasterizer.missing_glyphs("title", text, &style));
    }
    assert_eq!(rasterizer.missing_characters.len(), 2);

    for _ in 0..3 {
        // These aliases normalize to the same locales used above.
        for (lang, expected) in ["ja-JP", "zh-TW"].into_iter().zip(&expected) {
            rasterizer.select_language(Some(lang));
            assert_eq!(rasterizer.missing_characters.len(), 2);
            style.lang = Some(lang.to_owned());
            assert_eq!(&rasterizer.missing_glyphs("title", text, &style), expected);
            assert_eq!(rasterizer.missing_characters.len(), 2);
        }
    }

    rasterizer.load_font_data("cache-test", sfnt()).unwrap();
    assert!(rasterizer.missing_characters.is_empty());
    for (lang, expected) in ["ja", "zh-Hant"].into_iter().zip(&expected) {
        style.lang = Some(lang.to_owned());
        assert_eq!(&rasterizer.missing_glyphs("title", text, &style), expected);
    }
    assert_eq!(rasterizer.missing_characters.len(), 2);
}

#[test]
fn leaves_a_family_with_no_face_to_the_font_fallback() {
    let mut rasterizer = regular_only_rasterizer();
    let style = TextStyle {
        font_family: Some("Celesta Missing Family".into()),
        ..TextStyle::default()
    };
    assert_eq!(rasterizer.font_fallback("title", &style).len(), 1);
    assert!(
        rasterizer
            .missing_glyphs("title", "ずんだもん", &style)
            .is_empty()
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
            font_family: Some("Bebas Neue".into()),
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
        font_family: Some("Bebas Neue".into()),
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
            "no @font-face rule",
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
