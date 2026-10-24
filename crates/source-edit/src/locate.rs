use oxc_allocator::Allocator;
use oxc_ast::ast::JSXElement;
use oxc_ast_visit::Visit;
use oxc_span::GetSpan;

use crate::parse;
use crate::resolve::Span;

/// Byte offsets of a located JSX element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElementRef {
    /// Offset of the opening `<`.
    pub start: u32,
    /// Insertion point: after the last attribute, or after the tag name (and
    /// its type arguments).
    pub end_of_attributes: u32,
    /// Everything between the opening and the closing tag; `None` when the
    /// element is self-closing.
    pub children: Option<Span>,
}

/// Finds the JSX element whose `<` sits at the 1-based `line`/`column` that
/// esbuild's `jsxDEV` reports (columns count UTF-16 code units).
pub fn element_at(text: &str, line: u32, column: u32) -> Option<ElementRef> {
    let offset = to_offset(text, line, column)?;
    let alloc = Allocator::default();
    let program = parse(text, &alloc);
    let mut finder = Finder {
        offset,
        found: None,
    };
    finder.visit_program(&program);
    finder.found
}

/// Converts a 1-based line / UTF-16 column to a byte offset. Only `\n` ends a
/// line; a preceding `\r` stays part of it.
fn to_offset(text: &str, line: u32, column: u32) -> Option<u32> {
    let mut line_start = 0usize;
    for _ in 1..line {
        line_start += text[line_start..].find('\n')? + 1;
    }
    let mut units = 0u32;
    let target = column.checked_sub(1)?;
    for (i, c) in text[line_start..].char_indices() {
        if units == target {
            return u32::try_from(line_start + i).ok();
        }
        if c == '\n' {
            return None;
        }
        units += c.len_utf16() as u32;
    }
    (units == target)
        .then(|| u32::try_from(text.len()).ok())
        .flatten()
}

struct Finder {
    offset: u32,
    found: Option<ElementRef>,
}

impl<'a> Visit<'a> for Finder {
    fn visit_jsx_element(&mut self, it: &JSXElement<'a>) {
        let opening = &it.opening_element;
        if opening.span.start == self.offset && self.found.is_none() {
            // `<Foo<T>>`: after the type arguments, not inside them.
            let after_name = opening
                .type_arguments
                .as_ref()
                .map_or(opening.name.span().end, |args| args.span.end);
            let end_of_attributes = opening
                .attributes
                .last()
                .map_or(after_name, |a| a.span().end);
            self.found = Some(ElementRef {
                start: opening.span.start,
                end_of_attributes,
                children: it.closing_element.as_ref().map(|closing| Span {
                    start: opening.span.end,
                    end: closing.span.start,
                }),
            });
        }
        oxc_ast_visit::walk::walk_jsx_element(self, it);
    }
}
