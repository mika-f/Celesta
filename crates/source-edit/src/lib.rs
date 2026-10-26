//! Locate JSX elements in user TSX source and apply byte-span edits to them.
//!
//! Edits are splices on the original text; the AST is never re-printed, so
//! comments and formatting outside the edited spans survive byte-for-byte.

mod add;
mod apply;
mod attributes;
mod imports;
mod inputs;
mod insert;
mod lift;
mod locate;
mod pass_through;
mod remove;
mod resolve;
mod scenes;
mod scope;
mod shape;
mod timing;

pub use add::{
    AddLock, Added, NewElement, NewKind, Timing, accepts_children, add_child, add_lock, add_locks,
};
pub use apply::{ApplyError, Edit, apply, expression, format_number, write_value};
pub use attributes::{text_style_path, written_attributes};
pub use imports::{
    ImportLock, ImportName, PACKAGE as CELESTA_REACT, ensure_import, ensure_imports,
};
pub use inputs::resolve_inputs;
pub use insert::{WrapLock, indent_unit, wrap_element};
pub use lift::{Lift, LiftLock, LiftProp, lift_name, plan_lift, plan_lift_avoiding};
pub use locate::{ElementRef, element_at};
pub use pass_through::{
    DefaultTarget, Parameter, enclosing_component, parameter_default, parameter_of,
    resolve_through, resolve_unchecked,
};
pub use remove::removal;
pub use resolve::{
    EditTarget, EditValue, LiteralForm, LockReason, ResolveContext, Span, default_for,
    has_attribute, resolve, resolve_call_site_prop,
};

pub use scenes::{CompositionLength, SceneItem, SceneList, composition_length, scene_lists};
pub use timing::{TimingKind, later_series_items, timing_kind};

use std::path::Path;

use oxc_allocator::Allocator;
use oxc_ast::ast::Program;
use oxc_parser::Parser;
use oxc_span::SourceType;

/// Parses `text` as TSX. Syntax errors are tolerated; the recovered program is returned.
pub(crate) fn parse<'a>(text: &'a str, alloc: &'a Allocator) -> Program<'a> {
    parse_checked(text, alloc).0
}

/// Whether `text` parses as TSX without any error: an edited text that
/// does not is never written.
pub fn parses(text: &str) -> bool {
    let alloc = Allocator::default();
    !parse_checked(text, &alloc).1
}

/// Like [`parse`], also reporting whether the parser found any error. A
/// recovered program must never be written into.
pub(crate) fn parse_checked<'a>(text: &'a str, alloc: &'a Allocator) -> (Program<'a>, bool) {
    parse_checked_as(text, SourceType::tsx(), alloc)
}

/// Whether `text` parses without error as `path`'s source type (TSX when
/// the extension says nothing).
pub fn parses_file(text: &str, path: &Path) -> bool {
    let alloc = Allocator::default();
    !parse_checked_as(text, source_type_of(path), &alloc).1
}

/// [`parse_checked`] as `source_type`: a `.ts` file reads `<T>(x) => x`
/// and `<number>v`, which are not TSX.
pub(crate) fn parse_checked_as<'a>(
    text: &'a str,
    source_type: SourceType,
    alloc: &'a Allocator,
) -> (Program<'a>, bool) {
    let ret = Parser::new(alloc, text, source_type).parse();
    let failed = ret.fatal_error || ret.diagnostics.has_errors();
    (ret.program, failed)
}

/// The line break `text` writes: `\r\n` when it has one, else `\n`.
pub(crate) fn line_break(text: &str) -> &'static str {
    if text.contains("\r\n") { "\r\n" } else { "\n" }
}

/// The source type `path`'s extension names, TSX when it names none.
pub(crate) fn source_type_of(path: &Path) -> SourceType {
    SourceType::from_path(path).unwrap_or_else(|_| SourceType::tsx())
}
