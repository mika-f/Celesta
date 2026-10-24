//! The identifiers an expression reads, each resolved like a prop value, so
//! a value computed from others can be edited at its inputs.

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    BindingIdentifier, CallExpression, IdentifierReference, JSXElementName, TSType,
};
use oxc_ast_visit::Visit;

use crate::locate::ElementRef;
use crate::parse_checked;
use crate::resolve::{
    EditTarget, Gather, LockReason, PropPath, Read, ResolveContext, Span, resolve,
    resolve_identifier,
};
use crate::scope::Binding;
use crate::shape::{Children, Shape, StyleProperty};

/// For a prop locked as `Expression`, each identifier the expression reads
/// (in order of first appearance, member objects only: `C.x` gives `C`),
/// resolved by the same rules as a prop value. One level: an input whose
/// `const` is itself an expression is not expanded, and locks as
/// `UnresolvedIdentifier` as a prop reading it would.
///
/// Callees, JSX tag names, types, and names the expression binds itself
/// (an arrow's parameters) are not inputs. An input that is a parameter of
/// the enclosing component is `Locked(CallSiteNotFound)`: no call site is
/// given here. `ctx.received` is ignored for the inputs, since the runtime
/// reports the expression's result, not its inputs.
pub fn resolve_inputs(
    text: &str,
    element: &ElementRef,
    prop: &str,
    ctx: &ResolveContext,
) -> Vec<(String, EditTarget)> {
    if !matches!(
        resolve(text, element, prop, ctx),
        EditTarget::Locked(LockReason::Expression { .. })
    ) {
        return Vec::new();
    }
    let alloc = Allocator::default();
    let (program, _) = parse_checked(text, &alloc);
    let mut gather = Gather::new(element.start, PropPath::parse(prop));
    gather.visit_program(&program);
    let Some(span) = locked_span(&gather) else {
        return Vec::new();
    };

    let mut names = Names {
        span,
        read: Vec::new(),
        bound: Vec::new(),
    };
    names.visit_program(&program);
    let ctx = ResolveContext {
        received: None,
        ..ctx.clone()
    };
    names
        .read
        .into_iter()
        .filter(|name| !names.bound.contains(name))
        .map(|name| {
            let target = match gather.scopes.parameter(&Shape::Ident(name.clone())) {
                Some(parameter) => EditTarget::Locked(LockReason::CallSiteNotFound {
                    component: parameter.component,
                }),
                None => resolve_identifier(&name, &gather, text, &ctx),
            };
            (name, target)
        })
        .collect()
}

/// The span of the expression `resolve` locked on: the value written for
/// the prop, or the `interpolate()` call of the `const` it names.
fn locked_span(gather: &Gather) -> Option<Span> {
    let shape = match gather.read()? {
        Read::Children(Children::Other(span)) | Read::Property(StyleProperty::NotObject(span)) => {
            return Some(*span);
        }
        Read::Children(Children::Reference { shape })
        | Read::Property(StyleProperty::Value(shape))
        | Read::Attribute(shape) => shape,
        Read::Children(_) | Read::Property(_) => return None,
    };
    match shape {
        Shape::Member { span, .. } | Shape::Other(span) => Some(*span),
        Shape::Interpolate(call) => Some(call.call),
        Shape::Ident(name) => match gather.scopes.lookup(name)? {
            Binding::Const(Shape::Interpolate(call)) => Some(call.call),
            _ => None,
        },
        _ => None,
    }
}

/// The names read and bound inside `span`.
struct Names {
    span: Span,
    /// In order of first appearance.
    read: Vec<String>,
    bound: Vec<String>,
}

impl Names {
    fn contains(&self, span: oxc_span::Span) -> bool {
        self.span.start <= span.start && span.end <= self.span.end
    }
}

impl<'a> Visit<'a> for Names {
    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        if self.contains(it.span) && !self.read.iter().any(|name| name == it.name.as_str()) {
            self.read.push(it.name.to_string());
        }
    }

    fn visit_binding_identifier(&mut self, it: &BindingIdentifier<'a>) {
        if self.contains(it.span) {
            self.bound.push(it.name.to_string());
        }
    }

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        // A callee names a function, not a value to edit.
        if self.contains(it.span) {
            self.visit_arguments(&it.arguments);
        } else {
            oxc_ast_visit::walk::walk_call_expression(self, it);
        }
    }

    // A tag names a component, not a value.
    fn visit_jsx_element_name(&mut self, _: &JSXElementName<'a>) {}

    // Types are erased.
    fn visit_ts_type(&mut self, _: &TSType<'a>) {}
}
