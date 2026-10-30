//! Minimal `@font-face` stylesheet reading, enough for web font services such
//! as Google Fonts: each rule's `src` names one font file, picked as a browser
//! would (CSS Fonts 4, "Font reference: the src descriptor").

use std::path::Path;

use celesta_composition::AssetLocation;
use url::Url;

use crate::{RemoteAssetError, is_remote_url};

/// Whether `data` looks like a CSS stylesheet declaring `@font-face` rules.
pub fn is_font_stylesheet(data: &[u8]) -> bool {
    data.windows(b"@font-face".len())
        .any(|window| window.eq_ignore_ascii_case(b"@font-face"))
}

/// One `@font-face` rule: the font file it loads and the CSS family name it
/// declares, which can differ from the family name stored inside the file.
#[derive(Clone, Debug, PartialEq)]
pub struct StylesheetFontFace {
    pub family: Option<String>,
    pub location: AssetLocation,
}

/// The font faces an `@font-face` stylesheet loaded from `stylesheet`
/// declares, in rule order. A rule contributes the first `url(...)` in its
/// last valid `src` whose `format()` and `tech()` hints Celesta supports,
/// resolved against the stylesheet's own location; `local(...)` and `data:`
/// sources are skipped.
///
/// Fails on the first remote reference that is not a valid URL, as a
/// download of it would.
pub fn stylesheet_font_faces(
    css: &str,
    stylesheet: &AssetLocation,
) -> Result<Vec<StylesheetFontFace>, RemoteAssetError> {
    let mut faces = Vec::new();
    for (family, reference) in font_face_rules(css) {
        let Some(location) = resolve_reference(stylesheet, &reference)? else {
            continue;
        };
        let face = StylesheetFontFace { family, location };
        if !faces.contains(&face) {
            faces.push(face);
        }
    }
    Ok(faces)
}

/// `(font-family, chosen src url)` for each `@font-face` rule with a url.
fn font_face_rules(css: &str) -> Vec<(Option<String>, String)> {
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
        for (name, value) in declarations(&css[open..close]) {
            if name.eq_ignore_ascii_case("font-family") {
                family = Some(unquote(value).to_owned()).filter(|family| !family.is_empty());
            } else if name.eq_ignore_ascii_case("src") {
                // A `src` with no supported entry is invalid, so an earlier
                // one stays in effect; otherwise the later one wins.
                let sources = supported_sources(value);
                if !sources.is_empty() {
                    source = sources.into_iter().flatten().next();
                }
            }
        }
        rules.extend(source.map(|source| (family, source)));
        offset = close;
    }
    rules
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

/// `format()` values the renderer reads: TrueType, OpenType, WOFF, and WOFF2,
/// plus the legacy `"*-variations"` strings, which also mean
/// `tech(variations)`. EOT and SVG fonts are skipped, and so are collections,
/// whose `#PostScriptName` fragment the renderer does not honour.
const SUPPORTED_FORMATS: &[&str] = &[
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

/// The entries of a `src` value Celesta supports, in order: `Some(url)` for a
/// `url()` and `None` for a `local()`. Entries that fail to parse or carry an
/// unsupported `format()` or `tech()` are dropped.
fn supported_sources(value: &str) -> Vec<Option<String>> {
    split_outside_nesting(value, ',')
        .into_iter()
        .filter_map(|entry| font_source(entry.trim()))
        .collect()
}

fn font_source(entry: &str) -> Option<Option<String>> {
    if let Some((_, rest)) = function_argument(entry, "local") {
        return rest.trim().is_empty().then_some(None);
    }
    let (url, mut rest) = function_argument(entry, "url")?;
    if url.is_empty() {
        return None;
    }
    if let Some((format, after)) = function_argument(rest.trim_start(), "format") {
        if !SUPPORTED_FORMATS
            .iter()
            .any(|supported| format.eq_ignore_ascii_case(supported))
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
    rest.trim().is_empty().then(|| Some(url.to_owned()))
}

/// For `text` starting with `name(...)`, the unquoted argument and the text
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
                return Some((unquote(&text[open + 1..close]), &text[close + 1..]));
            }
            _ => {}
        }
    }
    None
}

/// Where `reference` points, or `None` for a source that is skipped.
fn resolve_reference(
    stylesheet: &AssetLocation,
    reference: &str,
) -> Result<Option<AssetLocation>, RemoteAssetError> {
    if reference
        .as_bytes()
        .get(..5)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case(b"data:"))
    {
        return Ok(None);
    }
    let invalid = |url: &str| {
        let url = url.to_owned();
        move |source| RemoteAssetError::InvalidUrl { url, source }
    };
    if is_remote_url(reference) {
        let url = Url::parse(reference).map_err(invalid(reference))?;
        return Ok(Some(AssetLocation::Url { url: url.into() }));
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
            .map(|face| face.location)
            .collect()
    }

    #[test]
    fn reads_google_fonts_style_stylesheets() {
        let css = "/* latin */\n@font-face {\n  font-family: 'M PLUS Rounded 1c';\n  font-style: normal;\n  font-weight: 700;\n  src: url(https://fonts.gstatic.com/s/a/v22/bold.ttf) format('truetype');\n}\n@FONT-FACE{font-weight:400;src:local('X'),url(\"regular.woff2\") format(\"woff2\"),url(regular.woff) format('woff')}\n";
        let stylesheet = url("https://fonts.example.com/css2?family=A");
        assert_eq!(
            stylesheet_font_faces(css, &stylesheet).unwrap(),
            vec![
                StylesheetFontFace {
                    family: Some("M PLUS Rounded 1c".to_owned()),
                    location: url("https://fonts.gstatic.com/s/a/v22/bold.ttf"),
                },
                StylesheetFontFace {
                    family: None,
                    location: url("https://fonts.example.com/regular.woff2"),
                },
            ]
        );
        assert!(is_font_stylesheet(css.as_bytes()));
        assert!(!is_font_stylesheet(b"wOF2\0\0"));
    }

    #[test]
    fn the_last_valid_src_wins() {
        // Font Squirrel's "bulletproof" syntax: an EOT-only `src` for old IE,
        // then the list every other browser reads.
        let css = "@font-face { font-family: Brand;\
              src: url('brand.eot');\
              src: url('brand.eot?#iefix') format('embedded-opentype'),\
                   url('brand.woff2') format('woff2'),\
                   url('brand.woff') format('woff'),\
                   url('brand.svg#Brand') format('svg') }\
            @font-face { src: url(kept.woff2) format(woff2);\
              src: url(skipped.svg#X) format(svg) }\
            @font-face { src: url(remote.woff2);\
              src: local(Installed), url(after-local.woff2) }\
            @font-face { src: url(only-local.woff2); src: local('Installed') }";
        assert_eq!(
            locations(css, &url("https://example.com/css/fonts.css")),
            vec![
                url("https://example.com/css/brand.woff2"),
                url("https://example.com/css/kept.woff2"),
                url("https://example.com/css/after-local.woff2"),
            ]
        );
    }

    #[test]
    fn picks_the_first_source_with_a_supported_format_and_tech() {
        let css = "@font-face { src:\
                url(a.eot) format(\"embedded-opentype\"),\
                url(b.otc#Regular) format(collection),\
                url(c.woff2) format(woff2) tech(color-COLRv1),\
                url(d.woff2) format(woff2) tech(incremental),\
                url(e.woff2) format(woff2) tech(variations, palettes),\
                url(f.woff2) format(zebra),\
                url(g.woff2) bogus,\
                url(\"h (1).woff2\") FORMAT(\"woff2-variations\") tech(color-COLRv0) }\
            @font-face { src: url(i.ttf) format(truetype) tech(features-aat, color-sbix) }\
            @font-face { src: url(j.otf) format(opentype) tech(features-opentype, color-CBDT) }\
            @font-face { src: url(k.woff) format('woff'), url(l.ttf) }\
            @font-face { src: url(m.ttf) format(truetype) tech(features-graphite), url(n) }";
        assert_eq!(
            locations(css, &url("https://example.com/fonts.css")),
            vec![
                url("https://example.com/h%20(1).woff2"),
                url("https://example.com/i.ttf"),
                url("https://example.com/j.otf"),
                url("https://example.com/k.woff"),
                url("https://example.com/n"),
            ]
        );
    }

    #[test]
    fn skips_comments_data_urls_and_duplicates() {
        let css = "/* @font-face { src: url(commented.ttf) } */\
            @font-face { src: url(data:font/woff2;base64,AAAA) }\
            @font-face { src: url(a.ttf) }\
            @font-face { src: url('a.ttf') }\
            @font-face { font-family: NoSource }";
        assert_eq!(
            locations(css, &url("https://example.com/fonts.css")),
            vec![url("https://example.com/a.ttf")]
        );
    }

    fn join(base: &str, reference: &str) -> AssetLocation {
        resolve_reference(&url(base), reference).unwrap().unwrap()
    }

    #[test]
    fn references_that_are_not_urls_are_errors() {
        let base = url("https://example.com/css/fonts.css");
        for reference in ["https://exa mple.com/a.ttf", "//exa mple.com/a.ttf"] {
            let css = format!(
                "@font-face {{ src: url(a.woff2) }} @font-face {{ src: url('{reference}') }}"
            );
            let error = stylesheet_font_faces(&css, &base).unwrap_err();
            assert!(
                matches!(&error, RemoteAssetError::InvalidUrl { url, .. } if url == reference),
                "{error:?}"
            );
            assert!(error.to_string().contains(reference), "{error}");
        }
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
    fn absolute_references_are_normalized_too() {
        let css = "@font-face { src: url(\"HTTPS://Example.COM/Other Font.ttf?a b\") }";
        let expected = vec![url("https://example.com/Other%20Font.ttf?a%20b")];
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
        let css = "@font-face { src: url(Brand.woff2?v=1) }\
            @font-face { src: url(https://example.com/remote.ttf) }";
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
