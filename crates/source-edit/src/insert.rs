//! JSX insertion: wrapping an element in a parent.

use oxc_allocator::Allocator;
use oxc_ast::ast::{JSXChild, JSXElement, JSXFragment, StringLiteral, TemplateLiteral};
use oxc_ast_visit::Visit;

use crate::apply::{Edit, after_name};
use crate::locate::ElementRef;
use crate::resolve::{EditValue, Span};
use crate::{line_break, parse_checked};

/// Why an element can't be wrapped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WrapLock {
    /// The element is not a JSX child: the root of a return value or an
    /// arrow body, a prop value, or inside `{ … }`. Also when the text does
    /// not parse.
    NotAChild,
}

/// Wraps the element at `element` in `<{tag} {attrs}>…</{tag}>`, indented
/// like it, re-indenting it one level.
///
/// The opening tag goes on the element's line and the closing tag on a line
/// of its own after it; every other line of the element gets one
/// [`indent_unit`] more, except empty lines and lines that start inside a
/// string or a template literal. An element that shares a line with other
/// code is wrapped inline, with no line break added. Attributes are written
/// as [`Edit::Insert`] writes them.
pub fn wrap_element(
    text: &str,
    element: &ElementRef,
    tag: &str,
    attrs: &[(&str, EditValue)],
) -> Result<Vec<Edit>, WrapLock> {
    let alloc = Allocator::default();
    let (program, failed) = parse_checked(text, &alloc);
    if failed {
        return Err(WrapLock::NotAChild);
    }
    let mut find = FindChild {
        start: element.start,
        found: None,
    };
    find.visit_program(&program);
    let (span, strings) = find.found.ok_or(WrapLock::NotAChild)?;

    let attributes: String = attrs
        .iter()
        .map(|(name, value)| format!(" {name}{}", after_name(value)))
        .collect();
    let open = format!("<{tag}{attributes}>");
    let close = format!("</{tag}>");
    let (start, end) = (span.start as usize, span.end as usize);
    let line_start = text[..start].rfind('\n').map_or(0, |i| i + 1);
    let indent = &text[line_start..start];
    let line_end = text[end..].find('\n').map_or(text.len(), |i| end + i);
    if !indent.chars().all(char::is_whitespace) || !text[end..line_end].trim().is_empty() {
        return Ok(vec![insert(span.start, open), insert(span.end, close)]);
    }

    let unit = indent_unit(text, span.start);
    let newline = line_break(text);
    let mut edits = vec![insert(span.start, format!("{open}{newline}{indent}{unit}"))];
    for (i, _) in text[start..end].match_indices('\n') {
        let line = start + i + 1;
        let rest = &text[line..];
        let empty = rest.starts_with('\n') || rest.starts_with("\r\n");
        // A line break inside a string is part of its value.
        let in_string = strings
            .iter()
            .any(|s| (s.start as usize) < line && line <= s.end as usize);
        if !empty && !in_string {
            edits.push(insert(line as u32, unit.clone()));
        }
    }
    edits.push(insert(span.end, format!("{newline}{indent}{close}")));
    Ok(edits)
}

/// The file's indent unit near `at` (a tab, or the leading spaces of one
/// level): what the nearest line indented deeper than the line before it
/// adds, looking at `at`'s line first, then outward. Two spaces when no
/// line is.
pub fn indent_unit(text: &str, at: u32) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let at = (at as usize).min(text.len());
    let k = text.as_bytes()[..at]
        .iter()
        .filter(|&&b| b == b'\n')
        .count();
    fn indent_of(line: &str) -> Option<&str> {
        let trimmed = line.trim_start();
        (!trimmed.is_empty()).then(|| &line[..line.len() - trimmed.len()])
    }
    // What line `j` adds to the indent of the non-blank line before it.
    let step = |j: usize| {
        let inner = indent_of(lines[j])?;
        let outer = lines[..j].iter().rev().find_map(|line| indent_of(line))?;
        inner.strip_prefix(outer).filter(|extra| !extra.is_empty())
    };
    for distance in 0..lines.len() {
        let nearest = [
            k.checked_sub(distance),
            (distance > 0).then_some(k + distance),
        ];
        for j in nearest.into_iter().flatten() {
            if let Some(extra) = (j < lines.len()).then(|| step(j)).flatten() {
                return if extra.contains('\t') {
                    "\t".to_string()
                } else {
                    extra.to_string()
                };
            }
        }
    }
    "  ".to_string()
}

fn insert(at: u32, text: String) -> Edit {
    Edit::Splice {
        start: at,
        end: at,
        text,
    }
}

/// The element whose `<` is at `start`, when it is a JSX child, and the
/// strings in it.
struct FindChild {
    start: u32,
    found: Option<(Span, Vec<Span>)>,
}

impl FindChild {
    fn children<'a>(&mut self, children: &[JSXChild<'a>]) {
        if self.found.is_some() {
            return;
        }
        let child = children.iter().find_map(|child| match child {
            JSXChild::Element(e) if e.opening_element.span.start == self.start => Some(e),
            _ => None,
        });
        if let Some(child) = child {
            let mut strings = Strings::default();
            strings.visit_jsx_element(child);
            self.found = Some((
                Span {
                    start: child.span.start,
                    end: child.span.end,
                },
                strings.0,
            ));
        }
    }
}

impl<'a> Visit<'a> for FindChild {
    fn visit_jsx_element(&mut self, it: &JSXElement<'a>) {
        self.children(&it.children);
        if self.found.is_none() {
            oxc_ast_visit::walk::walk_jsx_element(self, it);
        }
    }

    fn visit_jsx_fragment(&mut self, it: &JSXFragment<'a>) {
        self.children(&it.children);
        if self.found.is_none() {
            oxc_ast_visit::walk::walk_jsx_fragment(self, it);
        }
    }
}

/// The spans of string literals and of the text parts of template
/// literals.
#[derive(Default)]
struct Strings(Vec<Span>);

impl<'a> Visit<'a> for Strings {
    fn visit_string_literal(&mut self, it: &StringLiteral<'a>) {
        self.0.push(Span {
            start: it.span.start,
            end: it.span.end,
        });
    }

    fn visit_template_literal(&mut self, it: &TemplateLiteral<'a>) {
        for quasi in &it.quasis {
            self.0.push(Span {
                start: quasi.span.start,
                end: quasi.span.end,
            });
        }
        oxc_ast_visit::walk::walk_template_literal(self, it);
    }
}
