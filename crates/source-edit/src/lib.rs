//! Locate JSX elements in user TSX source and apply byte-span edits to them.
//!
//! Edits are splices on the original text; the AST is never re-printed, so
//! comments and formatting outside the edited spans survive byte-for-byte.

mod apply;
mod inputs;
mod locate;
mod pass_through;
mod resolve;
mod scope;
mod shape;

pub use apply::{ApplyError, Edit, apply, format_number, write_value};
pub use inputs::resolve_inputs;
pub use locate::{ElementRef, element_at};
pub use pass_through::{Parameter, enclosing_component, parameter_of, resolve_through};
pub use resolve::{
    EditTarget, EditValue, LiteralForm, LockReason, ResolveContext, Span, default_for,
    has_attribute, resolve,
};

use oxc_allocator::Allocator;
use oxc_ast::ast::Program;
use oxc_parser::Parser;
use oxc_span::SourceType;

/// Parses `text` as TSX. Syntax errors are tolerated; the recovered program is returned.
pub(crate) fn parse<'a>(text: &'a str, alloc: &'a Allocator) -> Program<'a> {
    parse_checked(text, alloc).0
}

/// Like [`parse`], also reporting whether the parser found any error. A
/// recovered program must never be written into.
pub(crate) fn parse_checked<'a>(text: &'a str, alloc: &'a Allocator) -> (Program<'a>, bool) {
    let ret = Parser::new(alloc, text, SourceType::tsx()).parse();
    let failed = ret.fatal_error || ret.diagnostics.has_errors();
    (ret.program, failed)
}
