//! What is written on an element, for the rows that choose a prop by it.

use oxc_allocator::Allocator;
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXOpeningElement};
use oxc_ast_visit::Visit;

use crate::locate::ElementRef;
use crate::parse_checked;
use crate::resolve::{Gather, LockReason, PropPath};
use crate::shape::StyleProperty;

/// Which prop a `Text` shorthand row edits: `style.<style_key>` when
/// `style` is an object literal whose own properties set `style_key`,
/// `shorthand` otherwise (also without a `style` attribute). Locked
/// `StyleMaySet` when `style` is written but is not an object literal, or
/// the object has a spread that may set `style_key`, and when the element
/// has a spread with no `style` attribute after it; `SyntaxError` /
/// `ElementNotFound` as `resolve`.
///
/// The object is read as `resolve` reads `style.<key>`: the last `style`
/// attribute, and its last property named `style_key` (quoted or not).
pub fn text_style_path(
    text: &str,
    element: &ElementRef,
    shorthand: &str,
    style_key: &str,
) -> Result<String, LockReason> {
    let alloc = Allocator::default();
    let (program, failed) = parse_checked(text, &alloc);
    if failed {
        return Err(LockReason::SyntaxError);
    }
    let mut gather = Gather::new(element.start, PropPath::Style(style_key));
    gather.visit_program(&program);
    if !gather.found {
        return Err(LockReason::ElementNotFound);
    }
    // `{...rest}` with no `style` after it may carry one, which wins.
    if gather.spread_may_set() {
        return Err(LockReason::StyleMaySet);
    }
    match gather.property {
        Some(StyleProperty::Value(_)) => Ok(format!("style.{style_key}")),
        None | Some(StyleProperty::Absent { .. }) => Ok(shorthand.to_string()),
        Some(StyleProperty::Unknown | StyleProperty::NotObject(_)) => Err(LockReason::StyleMaySet),
    }
}

/// The attribute names written on `element`, in source order, without
/// spreads, `key`, and `ref`. A name written twice is listed once, at its
/// last position. Empty when the text does not parse or no element starts there.
///
/// Namespaced names (`xlink:href`) are left out too: the rules read
/// attributes by plain name only.
pub fn written_attributes(text: &str, element: &ElementRef) -> Vec<String> {
    let alloc = Allocator::default();
    let (program, failed) = parse_checked(text, &alloc);
    if failed {
        return Vec::new();
    }
    let mut written = Written {
        element_start: element.start,
        names: None,
    };
    written.visit_program(&program);
    written.names.unwrap_or_default()
}

struct Written {
    element_start: u32,
    /// The names, once the element is found.
    names: Option<Vec<String>>,
}

impl<'a> Visit<'a> for Written {
    fn visit_jsx_opening_element(&mut self, it: &JSXOpeningElement<'a>) {
        if it.span.start == self.element_start && self.names.is_none() {
            let mut names: Vec<String> = Vec::new();
            for item in &it.attributes {
                let JSXAttributeItem::Attribute(attr) = item else {
                    continue;
                };
                let JSXAttributeName::Identifier(name) = &attr.name else {
                    continue;
                };
                if matches!(name.name.as_str(), "key" | "ref") {
                    continue;
                }
                names.retain(|written| written != name.name.as_str());
                names.push(name.name.to_string());
            }
            self.names = Some(names);
        }
        oxc_ast_visit::walk::walk_jsx_opening_element(self, it);
    }
}
