//! Minimal `@font-face` stylesheet reading, enough for web font services such
//! as Google Fonts: each rule's `src` names at most one font file, picked by
//! the CSS Fonts 4 parsing rules ("Font reference: the src descriptor"), or,
//! with no file, the installed fonts its `local()` entries name. A file is
//! preferred to an installed font even after a `local()`, so a render depends
//! on the machine only where the stylesheet gives nothing to download. Unlike
//! a browser, nothing falls back to a later entry when that file fails to load.

use std::ops::RangeInclusive;
use std::path::Path;

use celesta_composition::AssetLocation;
use url::Url;

use crate::{RemoteAssetError, is_data_url, is_remote_url};

/// Whether `data` looks like a CSS stylesheet declaring `@font-face` rules.
pub fn is_font_stylesheet(data: &[u8]) -> bool {
    data.windows(b"@font-face".len())
        .any(|window| window.eq_ignore_ascii_case(b"@font-face"))
}

/// One `@font-face` rule: the font it draws, the CSS family name it
/// declares, which can differ from the family name stored inside the font,
/// and the descriptors font matching reads in place of the font's own.
#[derive(Clone, Debug, PartialEq)]
pub struct StylesheetFontFace {
    pub family: String,
    pub source: FontFaceSource,
    pub descriptors: FontFaceDescriptors,
}

/// The `font-weight`, `font-style`, and `unicode-range` descriptors of an
/// `@font-face` rule, each its last valid declaration or, with none, its
/// `normal` (every code point) value, which is also what the web renderer
/// passes to `FontFace` for a missing one.
#[derive(Clone, Debug, PartialEq)]
pub struct FontFaceDescriptors {
    /// The inclusive weights the face is matched and drawn at, rounded; one
    /// value is a range with equal ends. `auto` reads as `normal` (400).
    pub weight: (u16, u16),
    pub style: FontFaceStyle,
    /// The code points the face may draw, sorted and merged; `None` when
    /// that is every one.
    pub unicode_range: Option<Vec<RangeInclusive<u32>>>,
}

impl Default for FontFaceDescriptors {
    fn default() -> Self {
        Self {
            weight: (400, 400),
            style: FontFaceStyle::Normal,
            unicode_range: None,
        }
    }
}

/// A `font-style` descriptor. `auto` reads as `normal`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FontFaceStyle {
    Normal,
    Italic,
    /// The inclusive slant angles in degrees, 14 when none is given.
    Oblique {
        min: f32,
        max: f32,
    },
}

/// Where a [`StylesheetFontFace`]'s font comes from.
#[derive(Clone, Debug, PartialEq)]
pub enum FontFaceSource {
    /// A font file to load.
    File(AssetLocation),
    /// The `local()` names of installed fonts, in `src` order: the first
    /// installed one whose full or PostScript name matches is drawn.
    Local(Vec<String>),
}

/// The font faces an `@font-face` stylesheet loaded from `stylesheet`
/// declares, in rule order. A rule contributes the first `url(...)` in its
/// last valid `src` whose `format()` and `tech()` hints Celesta supports,
/// resolved against the stylesheet's own location, even when a `local(...)`
/// comes before it. A `src` with no such `url(...)` contributes its
/// `local(...)` names instead. A rule with no `font-family` is invalid in
/// CSS, and contributes nothing.
///
/// Fails on the first relative reference that does not resolve to a valid
/// URL; an absolute one is checked when it is fetched.
pub fn stylesheet_font_faces(
    css: &str,
    stylesheet: &AssetLocation,
) -> Result<Vec<StylesheetFontFace>, RemoteAssetError> {
    let mut faces = Vec::new();
    for rule in font_face_rules(css) {
        let source = match rule.source {
            Source::Url(reference) => match resolve_reference(stylesheet, &reference)? {
                Some(location) => FontFaceSource::File(location),
                None => continue,
            },
            Source::Local(names) => FontFaceSource::Local(names),
        };
        let face = StylesheetFontFace {
            family: rule.family,
            source,
            descriptors: rule.descriptors,
        };
        if !faces.contains(&face) {
            faces.push(face);
        }
    }
    Ok(faces)
}

/// A `url()` reference as written, or `local()` names.
#[derive(Debug, PartialEq)]
enum Source {
    Url(String),
    Local(Vec<String>),
}

/// An `@font-face` rule with a family and a supported `src`.
struct Rule {
    family: String,
    /// The first url of the winning `src`, or with none, all its local names.
    source: Source,
    descriptors: FontFaceDescriptors,
}

fn font_face_rules(css: &str) -> Vec<Rule> {
    let css = strip_comments(css);
    let lower = css.to_ascii_lowercase();
    let mut rules = Vec::new();
    let mut offset = 0;
    while let Some(found) = lower[offset..].find("@font-face") {
        let start = offset + found;
        let Some(open) = css[start..].find('{').map(|index| start + index + 1) else {
            break;
        };
        let close = css[open..]
            .find('}')
            .map_or(css.len(), |index| open + index);
        let mut family = None;
        let mut source = None;
        let mut descriptors = FontFaceDescriptors::default();
        // An invalid declaration is dropped, so an earlier one stays in
        // effect; otherwise the later one wins.
        for (name, value) in declarations(&css[open..close]) {
            let name = name.to_ascii_lowercase();
            match name.as_str() {
                "font-family" => {
                    family = Some(unquote(value).to_owned()).filter(|family| !family.is_empty());
                }
                "src" => {
                    let sources = supported_sources(value);
                    if !sources.is_empty() {
                        source = Some(chosen_source(sources));
                    }
                }
                "font-weight" => {
                    if let Some(weight) = font_weight(value) {
                        descriptors.weight = weight;
                    }
                }
                "font-style" => {
                    if let Some(style) = font_style(value) {
                        descriptors.style = style;
                    }
                }
                "unicode-range" => {
                    if let Some(ranges) = unicode_range(value) {
                        descriptors.unicode_range = ranges;
                    }
                }
                _ => {}
            }
        }
        if let (Some(family), Some(source)) = (family, source) {
            rules.push(Rule {
                family,
                source,
                descriptors,
            });
        }
        offset = close;
    }
    rules
}

/// A `font-weight` descriptor: `normal`, `bold`, `auto`, or one or two
/// numbers from 1 to 1000, the range swapped when it decreases.
fn font_weight(value: &str) -> Option<(u16, u16)> {
    let words: Vec<&str> = value.split_whitespace().collect();
    let weight = |word: &str| -> Option<f32> {
        match word.to_ascii_lowercase().as_str() {
            "normal" => Some(400.0),
            "bold" => Some(700.0),
            _ => css_number(word).filter(|weight| (1.0..=1000.0).contains(weight)),
        }
    };
    let (min, max) = match words.as_slice() {
        [word] if word.eq_ignore_ascii_case("auto") => (400.0, 400.0),
        [word] => {
            let weight = weight(word)?;
            (weight, weight)
        }
        [start, end] => {
            let (start, end) = (weight(start)?, weight(end)?);
            (start.min(end), start.max(end))
        }
        _ => return None,
    };
    Some((min.round() as u16, max.round() as u16))
}

/// A `font-style` descriptor: `normal`, `italic`, `auto`, or `oblique` with
/// up to two angles from -90deg to 90deg, the range swapped when it
/// decreases.
fn font_style(value: &str) -> Option<FontFaceStyle> {
    let words: Vec<String> = value
        .split_whitespace()
        .map(str::to_ascii_lowercase)
        .collect();
    let words: Vec<&str> = words.iter().map(String::as_str).collect();
    let angle = |word: &str| -> Option<f32> {
        let (number, per_degree) = [
            ("deg", 1.0),
            ("grad", 0.9),
            ("rad", 180.0 / std::f32::consts::PI),
            ("turn", 360.0),
        ]
        .into_iter()
        .find_map(|(unit, per_degree)| Some((word.strip_suffix(unit)?, per_degree)))?;
        Some(css_number(number)? * per_degree).filter(|angle| (-90.0..=90.0).contains(angle))
    };
    match words.as_slice() {
        ["normal" | "auto"] => Some(FontFaceStyle::Normal),
        ["italic"] => Some(FontFaceStyle::Italic),
        ["oblique"] => Some(FontFaceStyle::Oblique {
            min: 14.0,
            max: 14.0,
        }),
        ["oblique", value] => {
            let angle = angle(value)?;
            Some(FontFaceStyle::Oblique {
                min: angle,
                max: angle,
            })
        }
        ["oblique", start, end] => {
            let (start, end) = (angle(start)?, angle(end)?);
            Some(FontFaceStyle::Oblique {
                min: start.min(end),
                max: start.max(end),
            })
        }
        _ => None,
    }
}

/// A CSS `<number>`: digits with an optional sign, fraction, and exponent.
fn css_number(text: &str) -> Option<f32> {
    let valid = !text.is_empty()
        && text
            .chars()
            .all(|character| character.is_ascii_digit() || "+-.eE".contains(character))
        && text.chars().any(|character| character.is_ascii_digit());
    valid.then(|| text.parse().ok()).flatten()
}

/// A `unicode-range` descriptor: comma-separated `U+` code points, `-`
/// intervals, and `?` wildcards, up to U+10FFFF. `Some(None)` when the
/// ranges cover every code point; `None` when any range is invalid.
fn unicode_range(value: &str) -> Option<Option<Vec<RangeInclusive<u32>>>> {
    let mut ranges = Vec::new();
    for range in value.split(',') {
        let range = range.trim();
        let digits = range
            .get(..2)
            .filter(|prefix| prefix.eq_ignore_ascii_case("u+"))
            .map(|_| &range[2..])?;
        let hex = |digits: &str| {
            (!digits.is_empty() && digits.len() <= 6)
                .then(|| u32::from_str_radix(digits, 16).ok())
                .flatten()
                .filter(|_| digits.chars().all(|digit| digit.is_ascii_hexdigit()))
        };
        let (start, end) = if let Some((start, end)) = digits.split_once('-') {
            (hex(start)?, hex(end)?)
        } else if let Some(wildcards) = digits.find('?') {
            let (fixed, wild) = digits.split_at(wildcards);
            if digits.len() > 6 || wild.len() > 5 || !wild.chars().all(|digit| digit == '?') {
                return None;
            }
            let fixed = if fixed.is_empty() { 0 } else { hex(fixed)? };
            let shift = 4 * wild.len() as u32;
            (fixed << shift, (fixed << shift) | ((1 << shift) - 1))
        } else {
            let point = hex(digits)?;
            (point, point)
        };
        if start > end || end > 0x10FFFF {
            return None;
        }
        ranges.push(start..=end);
    }
    ranges.sort_by_key(|range| *range.start());
    let mut merged: Vec<RangeInclusive<u32>> = Vec::new();
    for range in ranges {
        match merged.last_mut() {
            Some(last) if *range.start() <= last.end().saturating_add(1) => {
                *last = *last.start()..=*last.end().max(range.end());
            }
            _ => merged.push(range),
        }
    }
    let everything = matches!(merged.as_slice(), [range] if *range == (0..=0x10FFFF));
    Some((!everything).then_some(merged))
}

fn unquote(value: &str) -> &str {
    value
        .trim()
        .trim_matches(|character| character == '"' || character == '\'')
        .trim()
}

fn strip_comments(css: &str) -> String {
    let mut output = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        output.push_str(&rest[..start]);
        rest = rest[start + 2..]
            .find("*/")
            .map_or("", |end| &rest[start + 2 + end + 2..]);
    }
    output.push_str(rest);
    output
}

/// Splits a declaration block into `(name, value)` declarations.
fn declarations(block: &str) -> Vec<(&str, &str)> {
    split_outside_nesting(block, ';')
        .into_iter()
        .filter_map(declaration)
        .collect()
}

fn declaration(text: &str) -> Option<(&str, &str)> {
    let (name, value) = text.split_once(':')?;
    Some((name.trim(), value.trim()))
}

/// Splits `text` on `separator` outside quotes and parentheses.
fn split_outside_nesting(text: &str, separator: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0_u32;
    let mut quote = None;
    let mut start = 0;
    for (index, character) in text.char_indices() {
        match (quote, character) {
            (Some(open), _) if character == open => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(character),
            (None, '(') => depth += 1,
            (None, ')') => depth = depth.saturating_sub(1),
            (None, _) if character == separator && depth == 0 => {
                parts.push(&text[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
}

/// `format()` keywords the renderer reads: TrueType, OpenType, WOFF, and
/// WOFF2. EOT and SVG fonts are skipped, and so are collections, whose
/// `#PostScriptName` fragment the renderer does not honour.
const SUPPORTED_FORMATS: &[&str] = &["woff2", "woff", "truetype", "opentype"];

/// `format()` strings the renderer reads: the keywords, plus the legacy
/// `*-variations` forms, which also mean `tech(variations)` and exist only as
/// strings.
const SUPPORTED_FORMAT_STRINGS: &[&str] = &[
    "woff2",
    "woff",
    "truetype",
    "opentype",
    "woff2-variations",
    "woff-variations",
    "truetype-variations",
    "opentype-variations",
];

/// `tech()` values the renderer handles: OpenType and AAT shaping, `wght`
/// variations, and COLRv0 or bitmap color glyphs. COLRv1, OpenType-SVG,
/// Graphite, palettes, and incremental loading are not.
const SUPPORTED_TECHS: &[&str] = &[
    "features-opentype",
    "features-aat",
    "variations",
    "color-colrv0",
    "color-sbix",
    "color-cbdt",
];

/// The entries of a `src` value Celesta supports, in order, each `local()`
/// with one name. Entries that fail to parse or carry an unsupported
/// `format()` or `tech()` are dropped.
fn supported_sources(value: &str) -> Vec<Source> {
    split_outside_nesting(value, ',')
        .into_iter()
        .filter_map(|entry| font_source(entry.trim()))
        .collect()
}

/// The first url among `sources`, or with none, every local name in order.
fn chosen_source(sources: Vec<Source>) -> Source {
    let mut names = Vec::new();
    for source in sources {
        match source {
            Source::Url(url) => return Source::Url(url),
            Source::Local(local) => names.extend(local),
        }
    }
    Source::Local(names)
}

fn font_source(entry: &str) -> Option<Source> {
    if let Some((name, rest)) = function_argument(entry, "local") {
        // Unquoted, a name is identifiers joined by one space.
        let name = if name.trim_start().starts_with(['"', '\'']) {
            unquote(name).to_owned()
        } else {
            name.split_whitespace().collect::<Vec<_>>().join(" ")
        };
        return (rest.trim().is_empty() && !name.is_empty()).then(|| Source::Local(vec![name]));
    }
    let (url, mut rest) = function_argument(entry, "url")?;
    let url = unquote(url);
    if url.is_empty() {
        return None;
    }
    if let Some((format, after)) = function_argument(rest.trim_start(), "format") {
        let format = format.trim();
        let supported = if format.starts_with(['"', '\'']) {
            SUPPORTED_FORMAT_STRINGS
        } else {
            SUPPORTED_FORMATS
        };
        if !supported
            .iter()
            .any(|supported| unquote(format).eq_ignore_ascii_case(supported))
        {
            return None;
        }
        rest = after;
    }
    if let Some((techs, after)) = function_argument(rest.trim_start(), "tech") {
        if !techs.split(',').all(|tech| {
            SUPPORTED_TECHS
                .iter()
                .any(|supported| tech.trim().eq_ignore_ascii_case(supported))
        }) {
            return None;
        }
        rest = after;
    }
    rest.trim().is_empty().then(|| Source::Url(url.to_owned()))
}

/// For `text` starting with `name(...)`, the argument as written and the text
/// after the closing parenthesis.
fn function_argument<'a>(text: &'a str, name: &str) -> Option<(&'a str, &'a str)> {
    let open = name.len();
    if !text.get(..open)?.eq_ignore_ascii_case(name) || !text[open..].starts_with('(') {
        return None;
    }
    let mut quote = None;
    for (index, character) in text[open + 1..].char_indices() {
        match (quote, character) {
            (Some(open), _) if character == open => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(character),
            (None, ')') => {
                let close = open + 1 + index;
                return Some((&text[open + 1..close], &text[close + 1..]));
            }
            _ => {}
        }
    }
    None
}

/// Where `reference` points, or `None` for a protocol-relative (`//host`)
/// reference in a stylesheet read from a local file, which has no scheme to
/// inherit.
fn resolve_reference(
    stylesheet: &AssetLocation,
    reference: &str,
) -> Result<Option<AssetLocation>, RemoteAssetError> {
    let invalid = |url: &str| {
        let url = url.to_owned();
        move |source| RemoteAssetError::InvalidUrl { url, source }
    };
    // `RemoteAssetCache::fetch` normalizes an absolute URL and decodes a
    // data: URL, which holds the font itself, as a browser's fetch would.
    if is_remote_url(reference) || is_data_url(reference) {
        return Ok(Some(AssetLocation::Url {
            url: reference.to_owned(),
        }));
    }
    match stylesheet {
        AssetLocation::Url { url } => {
            let url = Url::parse(url)
                .map_err(invalid(url))?
                .join(reference)
                .map_err(invalid(reference))?;
            Ok(Some(AssetLocation::Url { url: url.into() }))
        }
        AssetLocation::File { path } => {
            if reference.starts_with("//") {
                return Ok(None);
            }
            let reference = reference.split(['?', '#']).next().unwrap_or_default();
            let joined = Path::new(path)
                .parent()
                .unwrap_or_else(|| Path::new(""))
                .join(reference);
            Ok(Some(AssetLocation::File {
                path: joined.to_string_lossy().into_owned(),
            }))
        }
    }
}

#[cfg(test)]
// `vec![a..=b]` is a list of one code point range, not a list of code points.
#[allow(clippy::single_range_in_vec_init)]
mod tests {
    use super::*;

    fn url(value: &str) -> AssetLocation {
        AssetLocation::Url {
            url: value.to_owned(),
        }
    }

    fn locations(css: &str, stylesheet: &AssetLocation) -> Vec<AssetLocation> {
        stylesheet_font_faces(css, stylesheet)
            .unwrap()
            .into_iter()
            .map(|face| match face.source {
                FontFaceSource::File(location) => location,
                FontFaceSource::Local(names) => panic!("local({names:?}) is not a file"),
            })
            .collect()
    }

    #[test]
    fn reads_google_fonts_style_stylesheets() {
        let css = "/* latin */\n@font-face {\n  font-family: 'M PLUS Rounded 1c';\n  font-style: normal;\n  font-weight: 700;\n  src: url(https://fonts.gstatic.com/s/a/v22/bold.ttf) format('truetype');\n  unicode-range: U+0000-00FF, U+0131;\n}\n@FONT-FACE{font-family:B;font-weight:400;src:local('X'),url(\"regular.woff2\") format(\"woff2\"),url(regular.woff) format('woff')}\n";
        let stylesheet = url("https://fonts.example.com/css2?family=A");
        assert_eq!(
            stylesheet_font_faces(css, &stylesheet).unwrap(),
            vec![
                StylesheetFontFace {
                    family: "M PLUS Rounded 1c".to_owned(),
                    source: FontFaceSource::File(url("https://fonts.gstatic.com/s/a/v22/bold.ttf")),
                    descriptors: FontFaceDescriptors {
                        weight: (700, 700),
                        unicode_range: Some(vec![0..=0xFF, 0x131..=0x131]),
                        ..FontFaceDescriptors::default()
                    },
                },
                StylesheetFontFace {
                    family: "B".to_owned(),
                    source: FontFaceSource::File(url("https://fonts.example.com/regular.woff2")),
                    descriptors: FontFaceDescriptors::default(),
                },
            ]
        );
        assert!(is_font_stylesheet(css.as_bytes()));
        assert!(!is_font_stylesheet(b"wOF2\0\0"));
    }

    /// Reads the fixture in `packages/web/test/fixtures/font-face-src/`, which
    /// @celesta/web's `fonts.test.mts` also reads, so both renderers pick the
    /// same files, or `local()` names, under the same families.
    #[test]
    fn picks_the_same_sources_as_the_web_renderer() {
        let css = include_str!("../../../packages/web/test/fixtures/font-face-src/stylesheet.css");
        let expected =
            include_str!("../../../packages/web/test/fixtures/font-face-src/expected.txt");
        let faces = stylesheet_font_faces(css, &url("https://fonts.test/css/faces.css")).unwrap();
        let picked: Vec<String> = faces
            .into_iter()
            .map(|face| {
                let source = match face.source {
                    FontFaceSource::File(AssetLocation::Url { url }) => url,
                    FontFaceSource::Local(names) => names
                        .iter()
                        .map(|name| format!("local(\"{name}\")"))
                        .collect::<Vec<_>>()
                        .join(", "),
                    FontFaceSource::File(location) => panic!("{location:?} is not at a URL"),
                };
                format!("{}\t{source}", face.family)
            })
            .collect();
        assert_eq!(picked, expected.lines().collect::<Vec<_>>());
    }

    #[test]
    fn a_src_with_no_url_names_its_installed_fonts_in_order() {
        let css = "@font-face { font-family: Gentium; src: local(  Gentium\n  Bold ), local('Gentium-Bold'), url(g.svg#G) format(svg) }\
            @font-face { font-family: Downloaded; src: local(Installed), url(after-local.woff2) }\
            @font-face { font-family: Empty; src: local(''), local() }\
            @font-face { font-family: Twice; src: local(Same) }\
            @font-face { font-family: Twice; src: local(\"Same\") }";
        let face = |family: &str, source| StylesheetFontFace {
            family: family.to_owned(),
            source,
            descriptors: FontFaceDescriptors::default(),
        };
        assert_eq!(
            stylesheet_font_faces(css, &url("https://example.com/fonts.css")).unwrap(),
            vec![
                face(
                    "Gentium",
                    FontFaceSource::Local(vec![
                        "Gentium Bold".to_owned(),
                        "Gentium-Bold".to_owned()
                    ])
                ),
                face(
                    "Downloaded",
                    FontFaceSource::File(url("https://example.com/after-local.woff2"))
                ),
                face("Twice", FontFaceSource::Local(vec!["Same".to_owned()])),
            ]
        );
    }

    #[test]
    fn skips_comments_and_duplicates() {
        let css = "/* @font-face { font-family: A; src: url(commented.ttf) } */\
            @font-face { font-family: A; src: url(a.ttf) }\
            @font-face { font-family: 'A'; src: url('a.ttf'); font-weight: normal }\
            @font-face { font-family: NoSource }\
            @font-face { src: url(no-family.ttf) }";
        assert_eq!(
            locations(css, &url("https://example.com/fonts.css")),
            vec![url("https://example.com/a.ttf")]
        );
    }

    fn descriptors(block: &str) -> FontFaceDescriptors {
        let css = format!("@font-face {{ font-family: A; src: url(a.ttf); {block} }}");
        let [face] = stylesheet_font_faces(&css, &url("https://example.com/fonts.css"))
            .unwrap()
            .try_into()
            .unwrap();
        face.descriptors
    }

    #[test]
    fn reads_font_weight_as_a_range() {
        let weight = |block: &str| descriptors(block).weight;
        assert_eq!(weight(""), (400, 400));
        assert_eq!(weight("font-weight: 700"), (700, 700));
        assert_eq!(weight("FONT-WEIGHT: Bold"), (700, 700));
        assert_eq!(weight("font-weight: normal"), (400, 400));
        assert_eq!(weight("font-weight: 100 900"), (100, 900));
        // A decreasing range is swapped, and fractions round.
        assert_eq!(weight("font-weight: 900 100.4"), (100, 900));
        assert_eq!(weight("font-weight: 1 1000"), (1, 1000));
        // `auto` selects as `normal` does.
        assert_eq!(weight("font-weight: auto"), (400, 400));
        // An invalid declaration is dropped, so an earlier one stays.
        for invalid in [
            "0",
            "1001",
            "bolder",
            "lighter",
            "100 200 300",
            "700px",
            "-5",
            "",
        ] {
            assert_eq!(
                weight(&format!("font-weight: 300; font-weight: {invalid}")),
                (300, 300),
                "{invalid}"
            );
        }
    }

    #[test]
    fn reads_font_style() {
        let style = |block: &str| descriptors(block).style;
        assert_eq!(style(""), FontFaceStyle::Normal);
        assert_eq!(style("font-style: Italic"), FontFaceStyle::Italic);
        assert_eq!(style("font-style: auto"), FontFaceStyle::Normal);
        let oblique = |min, max| FontFaceStyle::Oblique { min, max };
        assert_eq!(style("font-style: oblique"), oblique(14.0, 14.0));
        assert_eq!(style("font-style: oblique 10deg"), oblique(10.0, 10.0));
        assert_eq!(
            style("font-style: oblique 20deg -5deg"),
            oblique(-5.0, 20.0)
        );
        assert_eq!(style("font-style: oblique 0.25turn"), oblique(90.0, 90.0));
        for invalid in ["oblique 91deg", "oblique 10", "slanted", "italic 10deg"] {
            assert_eq!(
                style(&format!("font-style: italic; font-style: {invalid}")),
                FontFaceStyle::Italic,
                "{invalid}"
            );
        }
    }

    #[test]
    fn reads_unicode_range() {
        let ranges = |block: &str| descriptors(block).unicode_range;
        assert_eq!(ranges(""), None);
        assert_eq!(ranges("unicode-range: U+0-10FFFF"), None);
        assert_eq!(ranges("unicode-range: U+30-39"), Some(vec![0x30..=0x39]));
        // Single code points, wildcards, and case; overlaps are merged and
        // the list is sorted.
        assert_eq!(
            ranges("unicode-range: u+4??, U+A5, U+3000-30ff, U+30A0-31FF, U+a6"),
            Some(vec![0xA5..=0xA6, 0x400..=0x4FF, 0x3000..=0x31FF])
        );
        assert_eq!(ranges("unicode-range: U+???"), Some(vec![0..=0xFFF]));
        assert_eq!(
            ranges("unicode-range: U+1????"),
            Some(vec![0x10000..=0x1FFFF])
        );
        // One invalid range drops the whole declaration.
        for invalid in [
            "U+??????",
            "U+110000",
            "U+39-30",
            "U+30-39, U+G0",
            "U+1234567",
            "30-39",
            "U+30-",
            "U+3?-4",
            "",
        ] {
            assert_eq!(
                ranges(&format!("unicode-range: U+41; unicode-range: {invalid}")),
                Some(vec![0x41..=0x41]),
                "{invalid}"
            );
        }
    }

    #[test]
    fn rules_differing_only_in_descriptors_are_separate_faces() {
        let css = "@font-face { font-family: A; src: url(a.ttf); font-weight: 400 }\
            @font-face { font-family: A; src: url(a.ttf); font-weight: 700 }\
            @font-face { font-family: A; src: url(a.ttf); font-weight: 700; unicode-range: U+30-39 }";
        let faces = stylesheet_font_faces(css, &url("https://example.com/fonts.css")).unwrap();
        assert_eq!(faces.len(), 3);
    }

    fn join(base: &str, reference: &str) -> AssetLocation {
        resolve_reference(&url(base), reference).unwrap().unwrap()
    }

    #[test]
    fn references_that_are_not_urls_are_errors() {
        // An absolute URL is checked when it is fetched
        // (`normalizes_urls_as_a_browser_fetch_does` in lib.rs).
        let base = url("https://example.com/css/fonts.css");
        let reference = "//exa mple.com/a.ttf";
        let css = format!(
            "@font-face {{ font-family: A; src: url(a.woff2) }} @font-face {{ font-family: B; src: url('{reference}') }}"
        );
        let error = stylesheet_font_faces(&css, &base).unwrap_err();
        assert!(
            matches!(&error, RemoteAssetError::InvalidUrl { url, .. } if url == reference),
            "{error:?}"
        );
        assert!(error.to_string().contains(reference), "{error}");
    }

    #[test]
    fn resolves_references_against_the_stylesheet_url() {
        let base = "https://cdn.example.com/kit/css/fonts.css?v=2";
        assert_eq!(
            join(base, "../fonts/a.woff2?v=3#iefix"),
            url("https://cdn.example.com/kit/fonts/a.woff2?v=3#iefix")
        );
        assert_eq!(
            join(base, "./b.woff"),
            url("https://cdn.example.com/kit/css/b.woff")
        );
        assert_eq!(
            join(base, "/root.ttf"),
            url("https://cdn.example.com/root.ttf")
        );
        assert_eq!(
            join(base, "//other.example.com/c.ttf"),
            url("https://other.example.com/c.ttf")
        );
        assert_eq!(
            join("https://example.com", "d.ttf"),
            url("https://example.com/d.ttf")
        );
        assert_eq!(
            join("https://example.com/a/b/", "../../../e.ttf"),
            url("https://example.com/e.ttf")
        );
    }

    #[test]
    fn fragment_only_references_keep_the_base_query() {
        assert_eq!(
            join("https://example.com/a/s.css?v=1", "#x"),
            url("https://example.com/a/s.css?v=1#x")
        );
    }

    #[test]
    fn resolves_edge_case_references_like_browsers() {
        let base = "https://cdn.example.com/kit/css/fonts.css?v=2";
        assert_eq!(
            join(base, "?q"),
            url("https://cdn.example.com/kit/css/fonts.css?q")
        );
        assert_eq!(
            join(base, "%2e%2e/a.ttf"),
            url("https://cdn.example.com/kit/a.ttf")
        );
        assert_eq!(
            join(base, "https:a.ttf"),
            url("https://cdn.example.com/kit/css/a.ttf")
        );
        assert_eq!(
            join(base, "a\tb.ttf"),
            url("https://cdn.example.com/kit/css/ab.ttf")
        );
        assert_eq!(
            join(base, "//EXAMPLE.org/a.ttf"),
            url("https://example.org/a.ttf")
        );
    }

    #[test]
    fn absolute_references_are_kept_for_the_fetch_to_normalize() {
        let css =
            "@font-face { font-family: A; src: url(\"HTTPS://Example.COM/Other Font.ttf?a b\") }";
        let expected = vec![url("HTTPS://Example.COM/Other Font.ttf?a b")];
        assert_eq!(
            locations(css, &url("https://example.com/css/fonts.css")),
            expected
        );
        let local = AssetLocation::File {
            path: "fonts/brand.css".to_owned(),
        };
        assert_eq!(locations(css, &local), expected);
    }

    #[test]
    fn joined_urls_are_normalized() {
        assert_eq!(
            join("HTTPS://Example.COM:443/css/s.css", "My Font.ttf"),
            url("https://example.com/css/My%20Font.ttf")
        );
        assert_eq!(
            join("https://example.com/css/s.css", "フォント.woff2"),
            url("https://example.com/css/%E3%83%95%E3%82%A9%E3%83%B3%E3%83%88.woff2")
        );
        assert_eq!(
            join("https://example.com/css/s.css", "..\\fonts\\a.ttf"),
            url("https://example.com/fonts/a.ttf")
        );
        assert_eq!(
            join("https://example.com/css/s.css", "//例え.jp/a.ttf"),
            url("https://xn--r8jz45g.jp/a.ttf")
        );
        assert_eq!(
            join("https://example.com/css/s.css", "ftp://example.com/a.ttf"),
            url("ftp://example.com/a.ttf")
        );
    }

    #[test]
    fn resolves_local_stylesheets_against_their_folder() {
        let stylesheet = AssetLocation::File {
            path: "fonts/brand.css".to_owned(),
        };
        let css = "@font-face { font-family: Brand; src: url(Brand.woff2?v=1) }\
            @font-face { font-family: Remote; src: url(https://example.com/remote.ttf) }";
        assert_eq!(
            locations(css, &stylesheet),
            vec![
                AssetLocation::File {
                    path: Path::new("fonts")
                        .join("Brand.woff2")
                        .to_string_lossy()
                        .into_owned(),
                },
                url("https://example.com/remote.ttf"),
            ]
        );
    }
}
