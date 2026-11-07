//! Timing wrappers (`Sequence`, `Series.Sequence`) told apart through the
//! file's `@celesta.studio/react` import, and the `Series` items after one.

use line_index::{LineIndex, WideEncoding};
use oxc_ast::ast::{JSXChild, JSXElement, JSXElementName, JSXFragment};
use oxc_ast_visit::Visit;

use crate::imports::Module;
use crate::locate::ElementRef;
use crate::with_program;

/// Which timing wrapper an element is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimingKind {
    /// `<Sequence from durationInFrames>`.
    Sequence,
    /// `<Series.Sequence durationInFrames offset>`, an item of a `<Series>`.
    SeriesSequence,
}

/// The timing wrapper `element` is, through the file's `@celesta.studio/react`
/// import (aliases and `import * as C` included); `None` for anything
/// else, or when the text does not parse.
pub fn timing_kind(text: &str, element: &ElementRef) -> Option<TimingKind> {
    with_program(text, |program| {
        let module = Module::of(program);
        let mut find = FindKind {
            module: &module,
            start: element.start,
            found: None,
        };
        find.visit_program(program);
        find.found.flatten()
    })
    .flatten()
}

/// The `<Series.Sequence>` items after `element` in its `<Series>`, as
/// 1-based line and UTF-16 column (what `element_at` takes). The items are
/// the `Series`' element children and the `<Series.Sequence>` elements of
/// its `{…}` children (`{cond && <Series.Sequence>}`, `{xs.map(…)}`), which
/// `<Series>` flattens, in source order; a fragment is not one (`<Series>`
/// throws on it). Empty when `element` is not one of them. An item a
/// `.map()` draws several times is one position.
pub fn later_series_items(text: &str, element: &ElementRef) -> Vec<(u32, u32)> {
    with_program(text, |program| {
        let module = Module::of(program);
        let mut find = FindSiblings {
            module: &module,
            start: element.start,
            found: None,
        };
        find.visit_program(program);
        let index = LineIndex::new(text);
        find.found
            .unwrap_or_default()
            .into_iter()
            .map(|offset| position_of(&index, offset))
            .collect()
    })
    .unwrap_or_default()
}

/// Whether the element at `parent` holds the element at `child` as a
/// child of its own: written among its children, or inside one of its
/// `{…}` children (`{on && <Rect />}`), not inside another element. False
/// when the text does not parse.
pub fn encloses_directly(text: &str, parent: &ElementRef, child: &ElementRef) -> bool {
    with_program(text, |program| {
        let mut find = FindParent {
            parent: parent.start,
            child: child.start,
            found: false,
        };
        find.visit_program(program);
        find.found
    })
    .unwrap_or(false)
}

/// Whether the element whose `<` is at `parent` holds the one at `child`
/// directly.
struct FindParent {
    parent: u32,
    child: u32,
    found: bool,
}

impl<'a> Visit<'a> for FindParent {
    fn visit_jsx_element(&mut self, it: &JSXElement<'a>) {
        if self.found {
            return;
        }
        if it.opening_element.span.start == self.parent {
            let mut children = ChildElements(Vec::new());
            for child in &it.children {
                match child {
                    JSXChild::Element(e) => children.visit_jsx_element(e),
                    JSXChild::ExpressionContainer(e) => children.visit_jsx_expression_container(e),
                    _ => {}
                }
            }
            self.found = children.0.contains(&self.child);
            return;
        }
        oxc_ast_visit::walk::walk_jsx_element(self, it);
    }
}

/// The elements of JSX children: the element itself, or those inside a
/// `{…}`, not what any element or fragment holds.
struct ChildElements(Vec<u32>);

impl<'a> Visit<'a> for ChildElements {
    fn visit_jsx_element(&mut self, it: &JSXElement<'a>) {
        self.0.push(it.opening_element.span.start);
    }

    fn visit_jsx_fragment(&mut self, _: &JSXFragment<'a>) {}
}

/// The 1-based line and UTF-16 column of byte `offset`, as esbuild's
/// `jsxDEV` reports an element (only `\n` ends a line).
fn position_of(index: &LineIndex, offset: u32) -> (u32, u32) {
    let line_col = index.line_col(offset.into());
    let wide = index
        .to_wide(WideEncoding::Utf16, line_col)
        .expect("a column within its line");
    (wide.line + 1, wide.col + 1)
}

impl Module {
    /// [`Self::export_of`] as a timing wrapper.
    pub(crate) fn kind_of(&self, name: &JSXElementName<'_>) -> Option<TimingKind> {
        let path = self.export_of(name)?;
        match path.iter().map(String::as_str).collect::<Vec<_>>()[..] {
            ["Sequence"] => Some(TimingKind::Sequence),
            ["Series", "Sequence"] => Some(TimingKind::SeriesSequence),
            _ => None,
        }
    }
}

/// The kind of the element whose `<` is at `start`.
struct FindKind<'m> {
    module: &'m Module,
    start: u32,
    found: Option<Option<TimingKind>>,
}

impl<'a> Visit<'a> for FindKind<'_> {
    fn visit_jsx_element(&mut self, it: &JSXElement<'a>) {
        if self.found.is_some() {
            return;
        }
        let opening = &it.opening_element;
        if opening.span.start == self.start {
            self.found = Some(self.module.kind_of(&opening.name));
            return;
        }
        oxc_ast_visit::walk::walk_jsx_element(self, it);
    }
}

/// The offsets of the `Series` items after the child whose `<` is at
/// `start`, once its `<Series>` is found.
struct FindSiblings<'m> {
    module: &'m Module,
    start: u32,
    found: Option<Vec<u32>>,
}

impl<'a> Visit<'a> for FindSiblings<'_> {
    fn visit_jsx_element(&mut self, it: &JSXElement<'a>) {
        if self.found.is_some() {
            return;
        }
        let series = self
            .module
            .export_of(&it.opening_element.name)
            .is_some_and(|path| path == ["Series"]);
        if series {
            let mut items = SeriesItems {
                module: self.module,
                found: Vec::new(),
            };
            for child in &it.children {
                match child {
                    JSXChild::Element(e) => items.visit_jsx_element(e),
                    JSXChild::ExpressionContainer(e) => items.visit_jsx_expression_container(e),
                    _ => {}
                }
            }
            if let Some(at) = items.found.iter().position(|&start| start == self.start) {
                self.found = Some(items.found.split_off(at + 1));
                return;
            }
        }
        oxc_ast_visit::walk::walk_jsx_element(self, it);
    }
}

/// The `<Series.Sequence>` elements of `<Series>` children: the element
/// itself, or those inside a `{…}`; nothing inside JSX (a fragment, or
/// another element's children) counts.
struct SeriesItems<'m> {
    module: &'m Module,
    found: Vec<u32>,
}

impl<'a> Visit<'a> for SeriesItems<'_> {
    fn visit_jsx_element(&mut self, it: &JSXElement<'a>) {
        if self.module.kind_of(&it.opening_element.name) == Some(TimingKind::SeriesSequence) {
            self.found.push(it.opening_element.span.start);
        }
    }

    fn visit_jsx_fragment(&mut self, _: &JSXFragment<'a>) {}
}
