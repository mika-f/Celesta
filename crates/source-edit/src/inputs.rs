//! The identifiers an expression reads, each resolved like a prop value, so
//! a value computed from others can be edited at its inputs.

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    ArrowFunctionExpression, BindingIdentifier, CallExpression, Expression, Function,
    IdentifierReference, JSXElementName, NewExpression, TSType, TaggedTemplateExpression,
};
use oxc_ast_visit::Visit;
use oxc_syntax::scope::ScopeFlags;

use crate::locate::ElementRef;
use crate::parse_checked;
use crate::pass_through::resolve_parameter;
use crate::resolve::{
    EditTarget, Gather, LockReason, PropPath, ResolveContext, Span, read_written,
    resolve_identifier,
};
use crate::shape::Shape;

/// For a prop locked as `Expression`, each identifier the expression reads
/// (in order of first appearance, member objects only: `C.x` gives `C`),
/// resolved by the same rules as a prop value. One level: an input whose
/// `const` is itself an expression is not expanded, and locks as
/// `UnresolvedIdentifier` as a prop reading it would.
///
/// Not inputs: a callee or tag that names a function (`f(…)`, `Math.max(…)`,
/// `new Box(…)`, `` tag`…` ``; a computed one, as `fns[i](…)`, is read like
/// any expression), JSX tag names, types, and a name a function inside the
/// expression binds, within that function (`(padding) => padding`). The
/// expression's reads of a name count as one place for `SharedBinding`.
///
/// An input that is a parameter of the component enclosing `element`
/// resolves at `call_site` (the same text/element pair `resolve_through`
/// takes) as `PassThrough { call_site, param, target }`, following the
/// pass-through rules (literal, `Missing` with the parameter's default,
/// `ParameterOfCaller` for a second hop); without a call site it is
/// `CallSiteNotFound`. `ctx.received` is ignored for the inputs, since the
/// runtime reports the expression's result, not its inputs; so an input
/// whose value is `interpolate(frame, …)` is `FrameUncertain`, as no
/// received value tells the candidate frames apart.
pub fn resolve_inputs(
    text: &str,
    element: &ElementRef,
    prop: &str,
    ctx: &ResolveContext,
    call_site: Option<(&str, &ElementRef)>,
) -> Vec<(String, EditTarget)> {
    let alloc = Allocator::default();
    let (program, failed) = parse_checked(text, &alloc);
    if failed {
        return Vec::new();
    }
    let mut gather = Gather::new(element.start, PropPath::parse(prop));
    gather.visit_program(&program);
    // `resolve`'s rules, up to an absent prop, which never locks as an
    // expression.
    let locked = read_written(&gather, text, prop, ctx, true);
    let (Ok(EditTarget::Locked(LockReason::Expression { .. })), Some(span)) =
        (locked, gather.locked.get())
    else {
        return Vec::new();
    };

    let mut names = Names::new(span);
    names.visit_program(&program);
    gather.scopes.one_place(span);
    let ctx = ResolveContext {
        received: None,
        ..ctx.clone()
    };
    names
        .inputs()
        .into_iter()
        .map(|name| {
            let target = match gather.scopes.parameter(&Shape::Ident(name.clone())) {
                Some(parameter) => resolve_parameter(parameter, &ctx, call_site, false),
                None => resolve_identifier(&name, &gather, text, &ctx, false),
            };
            (name, target)
        })
        .collect()
}

/// The names read inside `span`, and those the functions inside it bind.
struct Names {
    span: Span,
    /// Each read, in source order, with its offset.
    reads: Vec<(String, u32)>,
    /// The expression, then each function inside it, with the names it
    /// binds itself (not those of the functions inside it).
    scopes: Vec<(Span, Vec<String>)>,
    /// Indices into `scopes` of those being visited, innermost last.
    open: Vec<usize>,
}

impl Names {
    fn new(span: Span) -> Self {
        Self {
            span,
            reads: Vec::new(),
            scopes: vec![(span, Vec::new())],
            open: vec![0],
        }
    }

    fn contains(&self, span: oxc_span::Span) -> bool {
        self.span.start <= span.start && span.end <= self.span.end
    }

    /// Each name read, once, in order of first appearance, but where a
    /// scope around the read binds it.
    fn inputs(&self) -> Vec<String> {
        let mut inputs: Vec<String> = Vec::new();
        for (name, at) in &self.reads {
            let bound = self
                .scopes
                .iter()
                .any(|(span, bound)| span.start <= *at && *at < span.end && bound.contains(name));
            if !bound && !inputs.contains(name) {
                inputs.push(name.clone());
            }
        }
        inputs
    }

    /// Opens the scope of a function inside the expression.
    fn open(&mut self, span: oxc_span::Span) {
        self.open.push(self.scopes.len());
        self.scopes.push((
            Span {
                start: span.start,
                end: span.end,
            },
            Vec::new(),
        ));
    }
}

impl<'a> Visit<'a> for Names {
    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        if self.contains(it.span) {
            self.reads.push((it.name.to_string(), it.span.start));
        }
    }

    fn visit_binding_identifier(&mut self, it: &BindingIdentifier<'a>) {
        if self.contains(it.span)
            && let Some(&scope) = self.open.last()
        {
            self.scopes[scope].1.push(it.name.to_string());
        }
    }

    fn visit_function(&mut self, it: &Function<'a>, flags: ScopeFlags) {
        if !self.contains(it.span) {
            return oxc_ast_visit::walk::walk_function(self, it, flags);
        }
        // A declaration's name binds in the scope around it, too.
        if it.is_declaration()
            && let Some(id) = &it.id
        {
            self.visit_binding_identifier(id);
        }
        self.open(it.span);
        oxc_ast_visit::walk::walk_function(self, it, flags);
        self.open.pop();
    }

    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        if !self.contains(it.span) {
            return oxc_ast_visit::walk::walk_arrow_function_expression(self, it);
        }
        self.open(it.span);
        oxc_ast_visit::walk::walk_arrow_function_expression(self, it);
        self.open.pop();
    }

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        if !names_function(&it.callee) {
            self.visit_expression(&it.callee);
        }
        self.visit_arguments(&it.arguments);
    }

    fn visit_new_expression(&mut self, it: &NewExpression<'a>) {
        if !names_function(&it.callee) {
            self.visit_expression(&it.callee);
        }
        self.visit_arguments(&it.arguments);
    }

    fn visit_tagged_template_expression(&mut self, it: &TaggedTemplateExpression<'a>) {
        if !names_function(&it.tag) {
            self.visit_expression(&it.tag);
        }
        self.visit_template_literal(&it.quasi);
    }

    // A tag names a component, not a value.
    fn visit_jsx_element_name(&mut self, _: &JSXElementName<'a>) {}

    // Types are erased.
    fn visit_ts_type(&mut self, _: &TSType<'a>) {}
}

/// Whether a callee or tag names a function, not a value to edit: a name
/// or a chain of names (`f`, `Math.max`), unlike a computed one (`fns[i]`).
fn names_function(callee: &Expression<'_>) -> bool {
    match callee.without_parentheses() {
        Expression::Identifier(_) => true,
        Expression::StaticMemberExpression(member) => names_function(&member.object),
        _ => false,
    }
}
