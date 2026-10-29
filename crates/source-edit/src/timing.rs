//! Timing wrappers (`Sequence`, `Series.Sequence`) told apart through the
//! file's `@celesta/react` import, and the `Series` items after one.

use line_index::{LineIndex, WideEncoding};
use oxc_allocator::Allocator;
use oxc_ast::ast::{JSXChild, JSXElement, JSXElementName};
use oxc_ast_visit::Visit;

use crate::imports::Module;
use crate::locate::ElementRef;
use crate::parse_checked;

/// Which timing wrapper an element is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimingKind {
    /// `<Sequence from durationInFrames>`.
    Sequence,
    /// `<Series.Sequence durationInFrames offset>`, an item of a `<Series>`.
    SeriesSequence,
}

/// The timing wrapper `element` is, through the file's `@celesta/react`
/// import (aliases and `import * as C` included); `None` for anything
/// else, or when the text does not parse.
pub fn timing_kind(text: &str, element: &ElementRef) -> Option<TimingKind> {
    let alloc = Allocator::default();
    let (program, failed) = parse_checked(text, &alloc);
    if failed {
        return None;
    }
    let module = Module::of(&program);
    let mut find = FindKind {
        module: &module,
        start: element.start,
        found: None,
    };
    find.visit_program(&program);
    find.found.flatten()
}

/// The `<Series.Sequence>` siblings after `element` in its `<Series>`, as
/// 1-based line and UTF-16 column (what `element_at` takes). Only the
/// `Series`' own children count; empty when `element` is not one of them.
pub fn later_series_items(text: &str, element: &ElementRef) -> Vec<(u32, u32)> {
    let alloc = Allocator::default();
    let (program, failed) = parse_checked(text, &alloc);
    if failed {
        return Vec::new();
    }
    let module = Module::of(&program);
    let mut find = FindSiblings {
        module: &module,
        start: element.start,
        found: None,
    };
    find.visit_program(&program);
    let index = LineIndex::new(text);
    find.found
        .unwrap_or_default()
        .into_iter()
        .map(|offset| position_of(&index, offset))
        .collect()
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
        let at = it.children.iter().position(
            |child| matches!(child, JSXChild::Element(e) if e.opening_element.span.start == self.start),
        );
        if let (true, Some(at)) = (series, at) {
            self.found = Some(
                it.children[at + 1..]
                    .iter()
                    .filter_map(|child| match child {
                        JSXChild::Element(e)
                            if self.module.kind_of(&e.opening_element.name)
                                == Some(TimingKind::SeriesSequence) =>
                        {
                            Some(e.opening_element.span.start)
                        }
                        _ => None,
                    })
                    .collect(),
            );
            return;
        }
        oxc_ast_visit::walk::walk_jsx_element(self, it);
    }
}
