//! Removing a prop written on an element, so the element's (or a
//! parameter's) default applies again.

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    Expression, JSXAttribute, JSXAttributeItem, JSXAttributeName, JSXAttributeValue,
    JSXOpeningElement, ObjectPropertyKind,
};
use oxc_ast_visit::Visit;
use oxc_span::GetSpan;

use crate::apply::Edit;
use crate::locate::ElementRef;
use crate::parse_checked;
use crate::resolve::{PropPath, Span, skip_trivia};
use crate::shape::span_of;

/// The edit that removes `prop` from `element` in `text`, deleting bytes
/// only:
///
/// - an attribute (`maxWidth={400}`, `src="a.png"`, a bare `loop`) with the
///   whitespace before it;
/// - for `style.<key>`, the property of the `style={{ … }}` object with the
///   comma that separates it from the next property (the previous one for
///   the last); the only property leaves `{}`.
///
/// A pass-through's call site is removed the same way, with the call's text
/// and element and the parameter's name as `prop`.
///
/// `None` when nothing removes the prop: the text does not parse, no
/// element starts at `element`, the prop is `children` or not written, it
/// is written twice (the other would apply), a spread after it may set it
/// again, or `style` is not an object literal.
pub fn removal(text: &str, element: &ElementRef, prop: &str) -> Option<Edit> {
    let alloc = Allocator::default();
    let (program, failed) = parse_checked(text, &alloc);
    if failed {
        return None;
    }
    let path = PropPath::parse(prop);
    let mut find = Find {
        element_start: element.start,
        path,
        found: false,
        attributes: Vec::new(),
        spread_after: false,
        style: None,
    };
    find.visit_program(&program);
    let [attribute] = find.attributes.as_slice() else {
        return None;
    };
    if !find.found || find.spread_after {
        return None;
    }
    let (start, end) = match path {
        PropPath::Children => return None,
        PropPath::Attribute(_) => attribute_removal(text, *attribute),
        PropPath::Style(key) => find.style?.removal(text, key)?,
    };
    Some(Edit::Remove { start, end })
}

/// The bytes that remove the attribute at `attribute`, never across a line
/// break (the text before it may end in a `//` comment): alone on its line,
/// the whole line goes; first on its line, the spaces after it go, so what
/// follows keeps the indentation; otherwise the spaces before it.
fn attribute_removal(text: &str, attribute: Span) -> (u32, u32) {
    let (start, end) = (attribute.start as usize, attribute.end as usize);
    let is_space = |c: char| c == ' ' || c == '\t';
    let line_start = text[..start].rfind('\n').map_or(0, |at| at + 1);
    let before = &text[line_start..start];
    let rest = &text[end..];
    let after = rest.len() - rest.trim_start_matches(is_space).len();
    let first = before.chars().all(is_space);
    let line_end = rest.find('\n');
    let alone = first
        && line_end
            .is_some_and(|newline| rest[..newline].trim_end_matches('\r').chars().all(is_space));
    match line_end {
        Some(newline) if alone && line_start > 0 => (line_start as u32, (end + newline + 1) as u32),
        _ if first => (start as u32, (end + after) as u32),
        _ => {
            let spaces = before.len() - before.trim_end_matches(is_space).len();
            ((start - spaces) as u32, end as u32)
        }
    }
}

/// The properties of a `style` object literal, each with its name
/// (`None` for a spread or a computed key).
struct StyleObject {
    object: Span,
    properties: Vec<(Span, Option<String>)>,
}

impl StyleObject {
    fn of(attribute: &JSXAttribute<'_>) -> Option<Self> {
        let Some(JSXAttributeValue::ExpressionContainer(container)) = &attribute.value else {
            return None;
        };
        let Expression::ObjectExpression(object) =
            container.expression.as_expression()?.without_parentheses()
        else {
            return None;
        };
        let properties = object
            .properties
            .iter()
            .map(|property| {
                let name = match property {
                    ObjectPropertyKind::ObjectProperty(p) if !p.computed => {
                        p.key.static_name().map(|name| name.to_string())
                    }
                    _ => None,
                };
                (span_of(property.span()), name)
            })
            .collect();
        Some(Self {
            object: span_of(object.span),
            properties,
        })
    }

    /// The bytes that remove property `key`; `None` when it is not written
    /// exactly once, or a spread or a computed key after it may set it.
    fn removal(&self, text: &str, key: &str) -> Option<(u32, u32)> {
        let mut named = self
            .properties
            .iter()
            .enumerate()
            .filter(|(_, (_, name))| name.as_deref() == Some(key));
        let (index, _) = named.next()?;
        if named.next().is_some()
            || self.properties[index + 1..]
                .iter()
                .any(|(_, name)| name.is_none())
        {
            return None;
        }
        let property = self.properties[index].0;
        let comma_after = || {
            let at = skip_trivia(text, property.end as usize);
            text[at..].starts_with(',').then_some(at as u32)
        };
        // Not the last: through its comma and the whitespace after it, so
        // the next property takes its place.
        if self.properties.get(index + 1).is_some() {
            let comma = comma_after()? as usize;
            let rest = &text[comma + 1..];
            let end = comma + 1 + (rest.len() - rest.trim_start().len());
            return Some((property.start, end as u32));
        }
        let end = comma_after().map_or(property.end, |comma| comma + 1);
        if index > 0 {
            // The last: from the comma after the previous property, unless
            // a comment sits between them (the property alone goes; a
            // trailing comma is valid).
            let previous = self.properties[index - 1].0;
            let comma = skip_trivia(text, previous.end as usize);
            let between = text.get(comma + 1..property.start as usize)?;
            if text[comma..].starts_with(',') && between.trim().is_empty() {
                return Some((comma as u32, property.end));
            }
            return Some((property.start, end));
        }
        // The only one: an empty `{}` unless a comment stays inside.
        let (open, close) = (self.object.start + 1, self.object.end - 1);
        let around = [
            &text[open as usize..property.start as usize],
            &text[end as usize..close as usize],
        ];
        if around.iter().all(|part| part.trim().is_empty()) {
            Some((open, close))
        } else {
            Some((property.start, end))
        }
    }
}

struct Find<'p> {
    element_start: u32,
    path: PropPath<'p>,
    found: bool,
    /// Every attribute that holds the prop.
    attributes: Vec<Span>,
    /// A spread follows the last of them.
    spread_after: bool,
    /// For `style.<key>`: the last `style` attribute's object.
    style: Option<StyleObject>,
}

impl<'a> Visit<'a> for Find<'_> {
    fn visit_jsx_opening_element(&mut self, it: &JSXOpeningElement<'a>) {
        if it.span.start == self.element_start && !self.found {
            self.found = true;
            for item in &it.attributes {
                match item {
                    JSXAttributeItem::SpreadAttribute(_) => {
                        self.spread_after = !self.attributes.is_empty();
                    }
                    JSXAttributeItem::Attribute(attr) => {
                        let JSXAttributeName::Identifier(name) = &attr.name else {
                            continue;
                        };
                        if name.name != self.path.attribute() {
                            continue;
                        }
                        self.attributes.push(span_of(attr.span));
                        self.spread_after = false;
                        if let PropPath::Style(_) = self.path {
                            self.style = StyleObject::of(attr);
                        }
                    }
                }
            }
        }
        oxc_ast_visit::walk::walk_jsx_opening_element(self, it);
    }
}
