//! Locate JSX elements in user TSX source and apply byte-span edits to them.
//!
//! Edits are splices on the original text; the AST is never re-printed, so
//! comments and formatting outside the edited spans survive byte-for-byte.

mod apply;
mod attributes;
mod inputs;
mod lift;
mod locate;
mod pass_through;
mod remove;
mod resolve;
mod scope;
mod shape;

pub use apply::{ApplyError, Edit, apply, format_number, write_value};
pub use attributes::{text_style_path, written_attributes};
pub use inputs::resolve_inputs;
pub use lift::{Lift, LiftLock, LiftProp, lift_name, plan_lift};
pub use locate::{ElementRef, element_at};
pub use pass_through::{
    DefaultTarget, Parameter, enclosing_component, parameter_default, parameter_of, resolve_through,
};
pub use remove::removal;
pub use resolve::{
    EditTarget, EditValue, LiteralForm, LockReason, ResolveContext, Span, default_for,
    has_attribute, resolve, resolve_call_site_prop,
};

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
    let ret = Parser::new(alloc, text, SourceType::tsx()).parse();
    let failed = ret.fatal_error || ret.diagnostics.has_errors();
    (ret.program, failed)
}
