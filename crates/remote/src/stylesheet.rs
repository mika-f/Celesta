//! Minimal `@font-face` stylesheet reading, enough for web font services such
//! as Google Fonts: each rule's first `src: url(...)` names one font file.

use std::path::Path;

use celesta_composition::AssetLocation;

use crate::is_remote_url;

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
/// `src`, resolved against the stylesheet's own location; `local(...)` and
/// `data:` sources are skipped.
pub fn stylesheet_font_faces(css: &str, stylesheet: &AssetLocation) -> Vec<StylesheetFontFace> {
    let mut faces = Vec::new();
    for (family, reference) in font_face_rules(css) {
        let Some(location) = resolve_reference(stylesheet, &reference) else {
            continue;
        };
        let face = StylesheetFontFace { family, location };
        if !faces.contains(&face) {
            faces.push(face);
        }
    }
    faces
}

/// `(font-family, first src url)` for each `@font-face` rule with a url.
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
            } else if name.eq_ignore_ascii_case("src") && source.is_none() {
                source = first_url(value);
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

/// Splits a declaration block on `;` outside quotes and parentheses.
fn declarations(block: &str) -> Vec<(&str, &str)> {
    let mut declarations = Vec::new();
    let mut depth = 0_u32;
    let mut quote = None;
    let mut start = 0;
    for (index, character) in block.char_indices() {
        match (quote, character) {
            (Some(open), _) if character == open => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(character),
            (None, '(') => depth += 1,
            (None, ')') => depth = depth.saturating_sub(1),
            (None, ';') if depth == 0 => {
                declarations.extend(declaration(&block[start..index]));
                start = index + 1;
            }
            _ => {}
        }
    }
    declarations.extend(declaration(&block[start..]));
    declarations
}

fn declaration(text: &str) -> Option<(&str, &str)> {
    let (name, value) = text.split_once(':')?;
    Some((name.trim(), value.trim()))
}

fn first_url(value: &str) -> Option<String> {
    let lower = value.to_ascii_lowercase();
    let start = lower.find("url(")? + "url(".len();
    let end = start + value[start..].find(')')?;
    let url = unquote(&value[start..end]);
    (!url.is_empty()).then(|| url.to_owned())
}

fn resolve_reference(stylesheet: &AssetLocation, reference: &str) -> Option<AssetLocation> {
    if reference
        .as_bytes()
        .get(..5)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case(b"data:"))
    {
        return None;
    }
    if is_remote_url(reference) {
        return Some(AssetLocation::Url {
            url: reference.to_owned(),
        });
    }
    match stylesheet {
        AssetLocation::Url { url } => {
            join_url(url, reference).map(|url| AssetLocation::Url { url })
        }
        AssetLocation::File { path } => {
            if reference.starts_with("//") {
                return None;
            }
            let reference = reference.split(['?', '#']).next().unwrap_or_default();
            let joined = Path::new(path)
                .parent()
                .unwrap_or_else(|| Path::new(""))
                .join(reference);
            Some(AssetLocation::File {
                path: joined.to_string_lossy().into_owned(),
            })
        }
    }
}

/// Resolves a relative URL reference against an absolute `http(s)` base.
fn join_url(base: &str, reference: &str) -> Option<String> {
    let (scheme, rest) = base.split_once("://")?;
    if let Some(network_path) = reference.strip_prefix("//") {
        return Some(format!("{scheme}://{network_path}"));
    }
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    let base_path = rest[authority_end..]
        .split(['?', '#'])
        .next()
        .unwrap_or_default();
    let (reference_path, suffix) = reference
        .find(['?', '#'])
        .map_or((reference, ""), |index| reference.split_at(index));
    let path = if reference_path.starts_with('/') {
        reference_path.to_owned()
    } else if reference_path.is_empty() {
        base_path.to_owned()
    } else {
        let directory = base_path
            .rfind('/')
            .map_or("/", |index| &base_path[..=index]);
        format!("{directory}{reference_path}")
    };
    Some(format!(
        "{scheme}://{authority}{}{suffix}",
        remove_dot_segments(&path)
    ))
}

fn remove_dot_segments(path: &str) -> String {
    let mut segments: Vec<&str> = Vec::new();
    let mut parts = path.split('/').skip(1).peekable();
    while let Some(segment) = parts.next() {
        let last = parts.peek().is_none();
        match segment {
            "." | ".." => {
                if segment == ".." {
                    segments.pop();
                }
                if last {
                    segments.push("");
                }
            }
            _ => segments.push(segment),
        }
    }
    format!("/{}", segments.join("/"))
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
            .into_iter()
            .map(|face| face.location)
            .collect()
    }

    #[test]
    fn reads_google_fonts_style_stylesheets() {
        let css = "/* latin */\n@font-face {\n  font-family: 'M PLUS Rounded 1c';\n  font-style: normal;\n  font-weight: 700;\n  src: url(https://fonts.gstatic.com/s/a/v22/bold.ttf) format('truetype');\n}\n@FONT-FACE{font-weight:400;src:local('X'),url(\"regular.woff2\") format(\"woff2\"),url(regular.woff) format('woff')}\n";
        let stylesheet = url("https://fonts.example.com/css2?family=A");
        assert_eq!(
            stylesheet_font_faces(css, &stylesheet),
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

    #[test]
    fn resolves_references_against_the_stylesheet_url() {
        let base = "https://cdn.example.com/kit/css/fonts.css?v=2";
        assert_eq!(
            join_url(base, "../fonts/a.woff2?v=3#iefix").unwrap(),
            "https://cdn.example.com/kit/fonts/a.woff2?v=3#iefix"
        );
        assert_eq!(
            join_url(base, "./b.woff").unwrap(),
            "https://cdn.example.com/kit/css/b.woff"
        );
        assert_eq!(
            join_url(base, "/root.ttf").unwrap(),
            "https://cdn.example.com/root.ttf"
        );
        assert_eq!(
            join_url(base, "//other.example.com/c.ttf").unwrap(),
            "https://other.example.com/c.ttf"
        );
        assert_eq!(
            join_url("https://example.com", "d.ttf").unwrap(),
            "https://example.com/d.ttf"
        );
        assert_eq!(
            join_url("https://example.com/a/b/", "../../../e.ttf").unwrap(),
            "https://example.com/e.ttf"
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
