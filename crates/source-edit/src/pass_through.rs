//! Props a component passes on from its own parameters, followed to the
//! component's call site.

use oxc_allocator::Allocator;
use oxc_ast::ast::Expression;
use oxc_ast_visit::Visit;

use crate::locate::ElementRef;
use crate::parse_checked;
use crate::resolve::{
    Absent, EditTarget, EditValue, Gather, LockReason, PropPath, Read, ResolveContext, TOLERANCE,
    resolve, resolve_written,
};
use crate::scope::{ParamDefault, ParamInfo};
use crate::shape::{Children, StyleProperty};

/// What `resolve` found when the value is a parameter of the enclosing
/// component: the parameter's name at the call site and its literal default.
#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    pub component: String,
    pub param: String,
    pub default: Option<EditValue>,
}

/// The function that encloses `element` in `text`, as a call site would name
/// it: `function PriceTag(…)`, `const PriceTag = (…) => …`, also through
/// `memo(…)` and `forwardRef(…)`. The innermost such function wins; `None`
/// when the element is in none (an anonymous function, or a lowercase name
/// JSX reads as an intrinsic element) or the text does not parse.
pub fn enclosing_component(text: &str, element: &ElementRef) -> Option<String> {
    let alloc = Allocator::default();
    let (program, failed) = parse_checked(text, &alloc);
    if failed {
        return None;
    }
    let mut gather = Gather::new(element.start, PropPath::Children);
    gather.visit_program(&program);
    if !gather.found {
        return None;
    }
    gather.scopes.component().map(str::to_string)
}

/// The parameter of an enclosing component that `prop` on `element` reads:
/// an identifier bound by the component's first parameter when that is an
/// object pattern (`{ label }`, `{ label = 'Sale' }`, `{ label: text }`
/// reading `text`), or `props.label` when it is an identifier named
/// `props`. `None` for anything else, including an anonymous component, a
/// name declared twice, and a value a later spread may override.
pub fn parameter_of(text: &str, element: &ElementRef, prop: &str) -> Option<Parameter> {
    parameter_in(text, element, prop).map(|found| Parameter {
        component: found.component,
        param: found.param,
        default: match found.default {
            ParamDefault::Literal(value) => Some(value),
            ParamDefault::None | ParamDefault::Other => None,
        },
    })
}

/// Resolves `prop` on `element` (in `element_text`) and, when it is a
/// parameter of the enclosing component, continues at `call_site` in
/// `call_site_text`.
///
/// At the call site the parameter resolves with the call site's own rules,
/// and the result is wrapped in `PassThrough` so the UI can say where; an
/// absent prop takes the parameter's literal default (`Missing`). A call site
/// that passes on a parameter of its own enclosing component is
/// `ParameterOfCaller` (one hop only), and no call site is
/// `CallSiteNotFound`.
///
/// The received value is checked against the call site's value, the same
/// value flowing through. `ctx.render_count` counts the element's instances
/// across every call site, so it does not lock a pass-through: the edit
/// changes only the instances of the given call site. A call site that
/// itself renders several times (in a `.map()`) is not locked either.
pub fn resolve_through(
    element_text: &str,
    element: &ElementRef,
    prop: &str,
    ctx: &ResolveContext,
    call_site: Option<(&str, &ElementRef)>,
) -> EditTarget {
    let Some(parameter) = parameter_in(element_text, element, prop) else {
        return resolve(element_text, element, prop, ctx);
    };
    if !ctx.primitive && !ctx.component_props {
        return EditTarget::Locked(LockReason::NotPrimitive);
    }
    resolve_parameter(parameter, ctx, call_site, true)
}

/// The value of `parameter` at `call_site`, by the pass-through rules of
/// [`resolve_through`]: wrapped in `PassThrough`, `Missing` with the
/// parameter's default when the call site omits it, `ParameterOfCaller`
/// for a second hop, and `CallSiteNotFound` without a call site.
///
/// `checked`: the call site's value must be `ctx.received`. A prop the
/// element reads as the parameter is; an expression's input is not
/// reported by the runtime, so it is not checked.
pub(crate) fn resolve_parameter(
    parameter: ParamInfo,
    ctx: &ResolveContext,
    call_site: Option<(&str, &ElementRef)>,
    checked: bool,
) -> EditTarget {
    let Some((call_site_text, call_site)) = call_site else {
        return EditTarget::Locked(LockReason::CallSiteNotFound {
            component: parameter.component,
        });
    };
    if let Some(caller) = parameter_in(call_site_text, call_site, &parameter.param) {
        return EditTarget::Locked(LockReason::ParameterOfCaller {
            component: caller.component,
        });
    }
    let call_site_ctx = ResolveContext {
        primitive: false,
        component_props: true,
        render_count: 1,
        ..ctx.clone()
    };
    let target = match resolve_written(
        call_site_text,
        call_site,
        &parameter.param,
        &call_site_ctx,
        checked,
    ) {
        Ok(target) => target,
        Err(absent) => missing_parameter(
            call_site,
            &absent,
            &parameter.default,
            call_site_ctx.received.as_ref(),
            checked,
        ),
    };
    EditTarget::PassThrough {
        call_site: *call_site,
        param: parameter.param,
        target: Box::new(target),
    }
}

/// The parameter `prop` on `element` reads, when the value the rules would
/// read (the last attribute with no spread after it, the `style` property,
/// or a `{name}` child) is a parameter of an enclosing component. `None`
/// when the text does not parse.
fn parameter_in(text: &str, element: &ElementRef, prop: &str) -> Option<ParamInfo> {
    let alloc = Allocator::default();
    let (program, failed) = parse_checked(text, &alloc);
    if failed {
        return None;
    }
    let mut gather = Gather::new(element.start, PropPath::parse(prop));
    gather.visit_program(&program);
    if !gather.found {
        return None;
    }
    let shape = match gather.read()? {
        Read::Children(Children::Reference { shape })
        | Read::Property(StyleProperty::Value(shape))
        | Read::Attribute(shape) => shape,
        Read::Children(_) | Read::Property(_) => return None,
    };
    gather.scopes.parameter(shape)
}

/// A prop absent at a call site takes the parameter's default. The
/// primitive must have received that default when it is a literal, and
/// nothing when there is none, when `checked`.
fn missing_parameter(
    element: &ElementRef,
    absent: &Absent,
    default: &ParamDefault,
    received: Option<&EditValue>,
    checked: bool,
) -> EditTarget {
    let same = match (default, received) {
        (ParamDefault::Other, _) | (ParamDefault::None, None) => true,
        (ParamDefault::Literal(EditValue::Number(a)), Some(EditValue::Number(b))) => {
            (a - b).abs() <= TOLERANCE
        }
        (ParamDefault::Literal(value), Some(received)) => value == received,
        _ => false,
    };
    if checked && !same {
        return EditTarget::Locked(LockReason::OverriddenByParent);
    }
    EditTarget::Missing {
        insert_at: absent.empty_children.unwrap_or(element.end_of_attributes),
        default: match default {
            ParamDefault::Literal(value) => Some(value.clone()),
            ParamDefault::None | ParamDefault::Other => None,
        },
    }
}

/// The offset of the function a `const` initializer defines: the
/// initializer itself, or the function `memo(…)` / `forwardRef(…)` (also as
/// `React.memo` / `React.forwardRef`) wraps.
pub(crate) fn component_function(init: &Expression<'_>) -> Option<u32> {
    match init.without_parentheses() {
        Expression::ArrowFunctionExpression(f) => Some(f.span.start),
        Expression::FunctionExpression(f) => Some(f.span.start),
        Expression::CallExpression(call) if is_wrapper(&call.callee) => {
            component_function(call.arguments.first()?.as_expression()?)
        }
        _ => None,
    }
}

fn is_wrapper(callee: &Expression<'_>) -> bool {
    let name = match callee.without_parentheses() {
        Expression::Identifier(id) => id.name.as_str(),
        Expression::StaticMemberExpression(m) if matches!(&m.object, Expression::Identifier(object) if object.name == "React") => {
            m.property.name.as_str()
        }
        _ => return false,
    };
    matches!(name, "memo" | "forwardRef")
}

/// Whether a call site can name a component `name`: JSX reads a tag that
/// starts with a lowercase letter as an intrinsic element.
pub(crate) fn is_component_name(name: &str) -> bool {
    !name.starts_with(|c: char| c.is_ascii_lowercase())
}
