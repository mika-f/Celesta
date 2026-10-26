use std::fmt;

use crate::resolve::{EditValue, LiteralForm};

/// A single span edit on the original text.
#[derive(Debug, Clone, PartialEq)]
pub enum Edit {
    /// Replaces the bytes `start..end` with `value` written in `form`.
    Replace {
        start: u32,
        end: u32,
        value: EditValue,
        form: LiteralForm,
    },
    /// Inserts ` name=…` at byte offset `at`, or ` name` alone for
    /// `Bool(true)`.
    Insert {
        at: u32,
        name: String,
        value: EditValue,
    },
    /// Inserts `key: value` into an object literal at byte offset `at`:
    /// nothing goes before it right after the `{`, ` ` after a comma, and
    /// `, ` after a property (including one inserted at the same offset).
    InsertProperty {
        at: u32,
        key: String,
        value: EditValue,
    },
    /// Deletes the bytes `start..end`: a prop with what separates it (see
    /// [`removal`](crate::removal)).
    Remove { start: u32, end: u32 },
    /// Replaces the bytes `start..end` with `text` as written: an
    /// identifier, a pattern entry, a type field, JSX.
    Splice { start: u32, end: u32, text: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyError {
    /// Two edits touch the same bytes.
    Overlap,
    /// An edit lies outside the text or off a character boundary.
    OutOfRange,
}

impl fmt::Display for ApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Overlap => f.write_str("edits overlap"),
            Self::OutOfRange => f.write_str("edit is outside the source text"),
        }
    }
}

impl std::error::Error for ApplyError {}

impl Edit {
    fn span(&self) -> (u32, u32) {
        match self {
            Self::Replace { start, end, .. }
            | Self::Remove { start, end }
            | Self::Splice { start, end, .. } => (*start, *end),
            Self::Insert { at, .. } | Self::InsertProperty { at, .. } => (*at, *at),
        }
    }
}

/// Applies all `edits` to `text` in one pass.
pub fn apply(text: &str, edits: &[Edit]) -> Result<String, ApplyError> {
    let mut sorted: Vec<&Edit> = edits.iter().collect();
    sorted.sort_by_key(|e| e.span());

    let mut out = String::with_capacity(text.len() + 16 * edits.len());
    let mut cursor = 0usize;
    for edit in sorted {
        let (start, end) = edit.span();
        let (start, end) = (start as usize, end as usize);
        if start < cursor {
            return Err(ApplyError::Overlap);
        }
        if start > end
            || end > text.len()
            || !text.is_char_boundary(start)
            || !text.is_char_boundary(end)
        {
            return Err(ApplyError::OutOfRange);
        }
        out.push_str(&text[cursor..start]);
        match edit {
            Edit::Replace { value, form, .. } => out.push_str(&write_value(value, *form)),
            Edit::Remove { .. } => {}
            Edit::Splice { text, .. } => out.push_str(text),
            Edit::Insert { name, value, .. } => {
                out.push_str(&format!(" {name}{}", after_name(value)));
            }
            Edit::InsertProperty { key, value, .. } => {
                // What is written so far, so a property inserted at the same
                // offset just before this one counts.
                let separator = match out.chars().next_back() {
                    Some('{') => "",
                    Some(',') => " ",
                    _ => ", ",
                };
                out.push_str(&format!(
                    "{separator}{}: {}",
                    property_key(key),
                    expression(value)
                ));
            }
        }
        cursor = end;
    }
    out.push_str(&text[cursor..]);
    Ok(out)
}

/// The text that replaces a literal written as `form`, keeping its style.
/// A value of another kind than the literal is written so the result still
/// parses (`label=12` becomes `label={12}`).
pub fn write_value(value: &EditValue, form: LiteralForm) -> String {
    match (form, value) {
        (LiteralForm::Quoted(quote), _) => attribute_value(value, quote),
        (LiteralForm::BareTrue, _) => after_name(value),
        (LiteralForm::ContainerQuoted(quote), EditValue::String(s)) => quote_string(s, quote),
        (LiteralForm::Template, EditValue::String(s)) => template(s),
        (LiteralForm::JsxText, EditValue::String(s)) if fits_jsx_text(s) => s.clone(),
        (LiteralForm::ChildContainer(quote), EditValue::String(s)) => {
            format!("{{{}}}", quote_string(s, quote))
        }
        (LiteralForm::JsxText | LiteralForm::ChildContainer(_), _) => {
            format!("{{{}}}", expression(value))
        }
        _ => expression(value),
    }
}

/// Whether JSXText renders as exactly `s`: no character that ends the text
/// or starts an entity (`{ } < > &`), and nothing whitespace collapsing
/// would change (a newline, a tab, leading or trailing whitespace).
fn fits_jsx_text(s: &str) -> bool {
    !s.contains(['{', '}', '<', '>', '&', '\n', '\r', '\t']) && s.trim() == s
}

/// An object literal key: `key` alone when it is an identifier, otherwise
/// a string.
fn property_key(key: &str) -> String {
    let mut chars = key.chars();
    let identifier = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$');
    if identifier {
        key.to_string()
    } else {
        quote_string(key, '"')
    }
}

/// `value` as an expression; a string in double quotes.
pub fn expression(value: &EditValue) -> String {
    match value {
        EditValue::Number(n) => format_number(*n),
        EditValue::String(s) => quote_string(s, '"'),
        EditValue::Bool(b) => b.to_string(),
    }
}

/// `value` as a JSX attribute value: a string in `quote`s when the attribute
/// string can hold it (it has no escapes, and `&` starts an entity), anything
/// else in a container.
fn attribute_value(value: &EditValue, quote: char) -> String {
    match value {
        EditValue::String(s) if !s.contains([quote, '&', '\n', '\r']) => {
            format!("{quote}{s}{quote}")
        }
        _ => format!("{{{}}}", expression(value)),
    }
}

/// What follows an attribute's name: nothing for `true` (the attribute
/// alone), otherwise `=` and the value.
fn after_name(value: &EditValue) -> String {
    match value {
        EditValue::Bool(true) => String::new(),
        _ => format!("={}", attribute_value(value, '"')),
    }
}

/// `s` as a string literal in `quote`s; with `"` it is JSON.
fn quote_string(s: &str, quote: char) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push(quote);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if c < ' ' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

/// `s` as a template literal without substitutions. A carriage return is
/// escaped because a template reads a written one as `\n`.
fn template(s: &str) -> String {
    let escaped = s
        .replace('\\', "\\\\")
        .replace('`', "\\`")
        .replace("${", "\\${")
        .replace('\r', "\\r");
    format!("`{escaped}`")
}

/// Integer when within 0.005 of one, else up to 2 decimals with trailing zeros
/// trimmed; negative zero is written `0`.
pub fn format_number(value: f64) -> String {
    let rounded = value.round();
    if (value - rounded).abs() < 0.005 {
        return format!("{}", rounded as i64);
    }
    // Round half away from zero ourselves; `{:.2}` would give 0.12 for 0.125.
    let s = format!("{:.2}", (value * 100.0).round() / 100.0);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}
