use std::cell::Cell;

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    ArrowFunctionExpression, BindingIdentifier, Expression, Function, IdentifierReference,
    JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXElement, JSXOpeningElement, Program,
    VariableDeclarator,
};
use oxc_ast_visit::Visit;
use oxc_span::GetSpan;
use oxc_syntax::scope::ScopeFlags;

use crate::apply::Edit;
use crate::locate::ElementRef;
use crate::parse_checked;
use crate::scope::{Binding, Scopes};
use crate::shape::{
    Children, Input, Interpolate, Shape, StyleProperty, children_of, shape_of, span_of, string_of,
    style_property,
};

/// A byte range of the source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

/// A prop value read from or written to the source.
#[derive(Debug, Clone, PartialEq)]
pub enum EditValue {
    Number(f64),
    String(String),
    Bool(bool),
}

impl EditValue {
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(*value),
            _ => None,
        }
    }
}

/// How a literal is written, so a write keeps its style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiteralForm {
    /// `12`, `-12`.
    Number,
    /// `name="…"` / `name='…'`: an attribute string, no container.
    Quoted(char),
    /// `name={'…'}` / `name={"…"}`, or a `const` initializer.
    ContainerQuoted(char),
    /// `` name={`…`} `` without `${}`.
    Template,
    /// `name` alone, which is `true`.
    BareTrue,
    /// `name={true}` / `name={false}`.
    Bool,
    /// The element's one child is JSXText: `<Text>Sale</Text>`.
    JsxText,
    /// The element's one child is `{'…'}` / `{"…"}`.
    ChildContainer(char),
}

/// Where a prop's value lives in the source, or why it cannot be edited.
#[derive(Debug, Clone, PartialEq)]
pub enum EditTarget {
    /// A literal written as `form`; replace `span`. For `Quoted` the span
    /// covers the quotes; for `BareTrue` it is the empty point right after
    /// the attribute name; for `JsxText` and `ChildContainer` it covers
    /// everything between the tags.
    Literal {
        span: Span,
        value: EditValue,
        form: LiteralForm,
    },
    /// An `interpolate()` output entry at the current frame; replace `span`.
    Keyframe {
        span: Span,
        value: f64,
        frame: f64,
    },
    /// The prop is absent; insert it at `insert_at`. `default` is the value
    /// it takes meanwhile, when known.
    Missing {
        insert_at: u32,
        default: Option<EditValue>,
    },
    /// The prop comes from parameter `param` of the enclosing component;
    /// `target` is the prop at the call site `call_site` (in another text).
    PassThrough {
        call_site: ElementRef,
        param: String,
        target: Box<EditTarget>,
    },
    Locked(LockReason),
}

#[derive(Debug, Clone, PartialEq)]
pub enum LockReason {
    /// The parser reported an error; the file is never edited.
    SyntaxError,
    /// No JSX element starts at the given `ElementRef`.
    ElementNotFound,
    NotPrimitive,
    Expression {
        source: String,
    },
    UnresolvedIdentifier {
        name: String,
    },
    SharedBinding {
        name: String,
        uses: usize,
    },
    /// The frame `frame` reads lies between two `inputRange` entries.
    /// `previous` and `next` are in the same frames as `frame`, the candidate
    /// frame they were found from, so a timeline frame converts to them by
    /// the difference between the lookup's timeline frame and `frame`.
    BetweenKeyframes {
        previous: f64,
        next: f64,
        frame: f64,
    },
    OutsideKeyframes,
    UnsupportedCall {
        callee: String,
    },
    OverriddenByParent,
    Spread,
    Repeated {
        count: u32,
    },
    /// The animation could read more than one of the element's candidate
    /// frames, and they give different keyframes (or none can be told apart
    /// from the received value), so no keyframe can be picked.
    FrameUncertain,
    /// The element's file is outside the project (or in `node_modules`).
    OutsideProject,
    /// `style.<key>` on an element without a `style` attribute: there is no
    /// object to edit, and the shorthand prop is what sets the value.
    NoStyleObject,
    /// A `Text` shorthand (`fontSize`) next to a `style` that is not an
    /// object literal, or has a spread: `style` wins, and may set it.
    StyleMaySet,
    /// The value is a parameter of `component` that its caller passes on
    /// again (a second hop).
    ParameterOfCaller {
        component: String,
    },
    /// No chain entry calls the enclosing component.
    CallSiteNotFound {
        component: String,
    },
    /// The value is a `const` the file exports: other files may read it.
    Exported {
        name: String,
    },
    /// A `defineScenes` entry without `durationInFrames`.
    NoSceneDuration {
        id: String,
    },
}

/// What the runtime reported about the element in the current frame.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolveContext {
    /// The element is a primitive (not a user component).
    pub primitive: bool,
    /// Read the props written on a component element too (a pass-through's
    /// call site); otherwise a non-primitive element is `NotPrimitive`.
    pub component_props: bool,
    /// The frames an `interpolate(frame, …)` written for the element could
    /// read: what `useCurrentFrame()` returns under each frame provider
    /// above the element. Whichever component read `frame` sits above the
    /// element (or wrote it), so the frame it read is one of these. Empty
    /// locks keyframe targets as `FrameUncertain`.
    pub candidate_frames: Vec<f64>,
    /// The prop value the runtime saw, if any.
    pub received: Option<EditValue>,
    /// How many times the element rendered in the frame.
    pub render_count: u32,
}

/// Value of a prop that is absent from the source, when known.
pub fn default_for(prop: &str) -> Option<EditValue> {
    match prop {
        "scale" | "scaleX" | "scaleY" => Some(EditValue::Number(1.0)),
        "x" | "y" | "rotation" => Some(EditValue::Number(0.0)),
        _ => None,
    }
}

impl EditTarget {
    /// The edit that writes `value`, or `None` when the target is locked,
    /// `value` is a number that is not finite, or a keyframe is given
    /// anything but a number. A `PassThrough` writes its inner target at the
    /// call site as `param`, whatever `prop` the element reads it as; the
    /// edit applies to the call site's text.
    pub fn to_edit(&self, prop: &str, value: &EditValue) -> Option<Edit> {
        if value.as_number().is_some_and(|n| !n.is_finite()) {
            return None;
        }
        match self {
            Self::Literal { span, form, .. } => Some(Edit::Replace {
                start: span.start,
                end: span.end,
                value: value.clone(),
                form: *form,
            }),
            Self::Keyframe { span, .. } => Some(Edit::Replace {
                start: span.start,
                end: span.end,
                value: EditValue::Number(value.as_number()?),
                form: LiteralForm::Number,
            }),
            Self::Missing { insert_at, .. } => Some(match PropPath::parse(prop) {
                PropPath::Attribute(name) => Edit::Insert {
                    at: *insert_at,
                    name: name.to_string(),
                    value: value.clone(),
                },
                PropPath::Children => Edit::Replace {
                    start: *insert_at,
                    end: *insert_at,
                    value: value.clone(),
                    form: LiteralForm::JsxText,
                },
                PropPath::Style(key) => Edit::InsertProperty {
                    at: *insert_at,
                    key: key.to_string(),
                    value: value.clone(),
                },
            }),
            Self::PassThrough { param, target, .. } => target.to_edit(param, value),
            Self::Locked(_) => None,
        }
    }
}

/// What a prop path names on an element.
#[derive(Debug, Clone, Copy)]
pub(crate) enum PropPath<'p> {
    /// An attribute.
    Attribute(&'p str),
    /// `children`: the element's children, or its `children` attribute
    /// when it has none.
    Children,
    /// `style.<key>`: property `<key>` of a `style={{ … }}` object literal.
    Style(&'p str),
}

impl<'p> PropPath<'p> {
    pub(crate) fn parse(prop: &'p str) -> Self {
        if prop == "children" {
            Self::Children
        } else if let Some(key) = prop.strip_prefix("style.") {
            Self::Style(key)
        } else {
            Self::Attribute(prop)
        }
    }

    /// The attribute that holds the prop.
    pub(crate) fn attribute(self) -> &'p str {
        match self {
            Self::Attribute(name) => name,
            Self::Children => "children",
            Self::Style(_) => "style",
        }
    }
}

pub(crate) const TOLERANCE: f64 = 1e-6;

/// Finds the source location of `prop` on `element`. Rules run in the order
/// the spec lists them; the first that applies decides.
///
/// `prop` is an attribute name, `children` for the element's children, or
/// `style.<key>` for a property of a `style={{ … }}` object literal.
pub fn resolve(text: &str, element: &ElementRef, prop: &str, ctx: &ResolveContext) -> EditTarget {
    let absent = match resolve_written(text, element, prop, ctx, true) {
        Ok(target) => return target,
        Err(absent) => absent,
    };
    if ctx.received.is_some() {
        return EditTarget::Locked(LockReason::OverriddenByParent);
    }
    match (PropPath::parse(prop), absent.empty_children) {
        (PropPath::Children, Some(insert_at)) => EditTarget::Missing {
            insert_at,
            default: Some(EditValue::String(String::new())),
        },
        (PropPath::Style(_), _) => EditTarget::Locked(LockReason::NoStyleObject),
        _ => EditTarget::Missing {
            insert_at: element.end_of_attributes,
            default: default_for(prop),
        },
    }
}

/// [`resolve`] for a prop written at a component's call (`ctx` with
/// `component_props`), unchecked: the runtime reports no value for a
/// component's own props, so a literal is not compared with
/// `ctx.received`, as a pass-through's call site is read. An absent prop is
/// `Missing` at the end of the attributes, with no default known.
pub fn resolve_call_site_prop(
    text: &str,
    element: &ElementRef,
    prop: &str,
    ctx: &ResolveContext,
) -> EditTarget {
    resolve_written(text, element, prop, ctx, false).unwrap_or(EditTarget::Missing {
        insert_at: element.end_of_attributes,
        default: None,
    })
}

/// A prop nothing written on the element sets: no attribute, no children,
/// and no spread.
pub(crate) struct Absent {
    /// For `children` on an element with an opening and a closing tag but
    /// nothing between them: where the children go.
    pub(crate) empty_children: Option<u32>,
}

/// [`resolve`] up to an absent prop, which is `Err` for the caller to decide:
/// [`resolve`] takes the prop's default, a call site the parameter's.
///
/// `checked`: the value found must be the one the runtime saw,
/// `ctx.received` (see [`check_received`]). An expression's input is not
/// reported by the runtime, so its call site is read unchecked.
pub(crate) fn resolve_written(
    text: &str,
    element: &ElementRef,
    prop: &str,
    ctx: &ResolveContext,
    checked: bool,
) -> Result<EditTarget, Absent> {
    let alloc = Allocator::default();
    let (program, failed) = parse_checked(text, &alloc);
    if failed {
        return Ok(EditTarget::Locked(LockReason::SyntaxError));
    }
    let mut gather = Gather::new(element.start, PropPath::parse(prop));
    gather.visit_program(&program);
    read_written(&gather, text, prop, ctx, checked)
}

/// [`resolve_written`] on what `gather` found in the parsed text. A prop
/// locked as an expression leaves that expression's span in
/// `gather.locked`.
pub(crate) fn read_written(
    gather: &Gather,
    text: &str,
    prop: &str,
    ctx: &ResolveContext,
    checked: bool,
) -> Result<EditTarget, Absent> {
    if !ctx.primitive && !ctx.component_props {
        return Ok(EditTarget::Locked(LockReason::NotPrimitive));
    }
    if ctx.render_count > 1 {
        return Ok(EditTarget::Locked(LockReason::Repeated {
            count: ctx.render_count,
        }));
    }
    if !gather.found {
        return Ok(EditTarget::Locked(LockReason::ElementNotFound));
    }
    let check = |target| {
        if checked {
            check_received(target, ctx.received.as_ref(), text, prop)
        } else {
            target
        }
    };

    let path = gather.path;
    // JSX children win over a `children` attribute and a spread. Without
    // any, a `children` attribute follows the attribute rules below.
    if let (PropPath::Children, Some(children)) = (path, &gather.children) {
        let child = |span: Span, value: &str, form| {
            check(EditTarget::Literal {
                span,
                value: EditValue::String(value.to_string()),
                form,
            })
        };
        match children {
            Children::Text { span, value } => return Ok(child(*span, value, LiteralForm::JsxText)),
            Children::Container { span, value, quote } => {
                return Ok(child(*span, value, LiteralForm::ChildContainer(*quote)));
            }
            Children::Reference { shape } => {
                return Ok(check(resolve_shape(shape, gather, text, ctx, checked)));
            }
            Children::Other(span) => return Ok(gather.lock_expression(text, *span)),
            // 2b opens the tag before writing children into it.
            Children::SelfClosing if gather.attr.is_none() => {
                return Ok(EditTarget::Locked(LockReason::Expression {
                    source: "/>".to_string(),
                }));
            }
            Children::SelfClosing | Children::Empty { .. } => {}
        }
    }

    if gather.attr.is_some() && gather.spread_after {
        // `<Rect x={5} {...p} />`: the spread may set `x` after the attribute.
        return Ok(EditTarget::Locked(LockReason::Spread));
    }
    let Some(shape) = &gather.attr else {
        if gather.has_spread {
            return Ok(EditTarget::Locked(LockReason::Spread));
        }
        return Err(Absent {
            empty_children: match (path, &gather.children) {
                (PropPath::Children, Some(Children::Empty { insert_at })) => Some(*insert_at),
                _ => None,
            },
        });
    };

    let target = match (path, &gather.property) {
        (PropPath::Style(_), Some(property)) => match property {
            StyleProperty::Value(shape) => resolve_shape(shape, gather, text, ctx, checked),
            StyleProperty::Absent { object, last } => {
                if checked && ctx.received.is_some() {
                    return Ok(EditTarget::Locked(LockReason::OverriddenByParent));
                }
                return Ok(EditTarget::Missing {
                    insert_at: property_insert_at(text, *object, *last),
                    default: None,
                });
            }
            StyleProperty::Unknown => return Ok(EditTarget::Locked(LockReason::Spread)),
            StyleProperty::NotObject(span) => return Ok(gather.lock_expression(text, *span)),
        },
        _ => resolve_shape(shape, gather, text, ctx, checked),
    };
    Ok(check(target))
}

/// The value written at `value`, an expression's span in `program` (parsed
/// from `text`), by the rules of a prop value: a literal, a `const`, an
/// expression locked with its source. It is read unchecked, as nothing
/// reports the value received. At module level the file's top-level
/// `const`s are read too (see [`Gather::at_value`]).
pub(crate) fn resolve_value_at(program: &Program<'_>, text: &str, value: Span) -> EditTarget {
    let gather = Gather::at_value(program, value);
    read_written(&gather, text, "", &value_context(), false)
        .unwrap_or(EditTarget::Locked(LockReason::ElementNotFound))
}

/// What a value read by [`resolve_value_at`] is resolved with: written
/// once, no value received, no frame known.
pub(crate) fn value_context() -> ResolveContext {
    ResolveContext {
        primitive: true,
        component_props: true,
        candidate_frames: Vec::new(),
        received: None,
        render_count: 1,
    }
}

/// Whether `prop` is written as an attribute on `element` (a spread does not
/// count). False when the text does not parse or the element is not found.
pub fn has_attribute(text: &str, element: &ElementRef, prop: &str) -> bool {
    let alloc = Allocator::default();
    let (program, failed) = parse_checked(text, &alloc);
    if failed {
        return false;
    }
    let mut gather = Gather::new(element.start, PropPath::Attribute(prop));
    gather.visit_program(&program);
    gather.attr.is_some()
}

/// The value the rules read for the prop, when one is written.
pub(crate) enum Read<'g> {
    /// The element's children, other than none.
    Children(&'g Children),
    /// The prop's property of the last `style` attribute.
    Property(&'g StyleProperty),
    /// The last attribute that holds the prop.
    Attribute(&'g Shape),
}

/// Where a property is added to an object literal: right after its `{`
/// when it is empty, otherwise after the last property, or after the
/// trailing comma that follows it.
pub(crate) fn property_insert_at(text: &str, object: Span, last: Option<u32>) -> u32 {
    let Some(last) = last else {
        return object.start + 1;
    };
    let next = skip_trivia(text, last as usize);
    if text[next..].starts_with(',') {
        next as u32 + 1
    } else {
        last
    }
}

/// The first offset at or after `at` that is neither whitespace nor inside
/// a comment.
pub(crate) fn skip_trivia(text: &str, mut at: usize) -> usize {
    loop {
        let rest = &text[at..];
        let trimmed = rest.trim_start();
        at += rest.len() - trimmed.len();
        let comment = if trimmed.starts_with("//") {
            trimmed.find('\n')
        } else if let Some(body) = trimmed.strip_prefix("/*") {
            body.find("*/").map(|end| end + 4)
        } else {
            None
        };
        match comment {
            Some(len) => at += len,
            None => return at,
        }
    }
}

/// `checked` as [`resolve_written`]'s: whether `ctx.received` is this
/// value's.
fn resolve_shape(
    shape: &Shape,
    gather: &Gather,
    text: &str,
    ctx: &ResolveContext,
    checked: bool,
) -> EditTarget {
    match shape {
        Shape::Number(span, value) => EditTarget::Literal {
            span: *span,
            value: EditValue::Number(*value),
            form: LiteralForm::Number,
        },
        Shape::String(span, value, form) => EditTarget::Literal {
            span: *span,
            value: EditValue::String(value.clone()),
            form: *form,
        },
        Shape::Bool(span, value, form) => EditTarget::Literal {
            span: *span,
            value: EditValue::Bool(*value),
            form: *form,
        },
        Shape::Interpolate(call) => resolve_keyframe(call, gather, text, ctx, checked),
        Shape::Call(callee) => EditTarget::Locked(LockReason::UnsupportedCall {
            callee: callee.clone(),
        }),
        Shape::Ident(name) => resolve_identifier(name, gather, text, ctx, checked),
        Shape::Member { span, .. } | Shape::Other(span) => gather.lock_expression(text, *span),
    }
}

/// Locks `span` as an expression. The source is trimmed: a children span
/// includes the whitespace between the tags. A value the rules read locks
/// through [`Gather::lock_expression`]; this alone is for a literal of
/// another kind than the received value, which has no inputs.
fn expression(text: &str, span: Span) -> EditTarget {
    EditTarget::Locked(LockReason::Expression {
        source: text[span.start as usize..span.end as usize]
            .trim()
            .to_string(),
    })
}

/// The value of `name` as read at the element; `checked` as
/// [`resolve_written`]'s (an expression's input is not).
pub(crate) fn resolve_identifier(
    name: &str,
    gather: &Gather,
    text: &str,
    ctx: &ResolveContext,
    checked: bool,
) -> EditTarget {
    let unresolved = || {
        EditTarget::Locked(LockReason::UnresolvedIdentifier {
            name: name.to_string(),
        })
    };
    if gather.scopes.declared_more_than_once(name) {
        return unresolved();
    }
    let Some(Binding::Const(shape)) = gather.scopes.lookup(name) else {
        return unresolved();
    };
    match shape {
        Shape::Number(..) | Shape::String(..) | Shape::Bool(..) | Shape::Interpolate(_) => {
            let uses = gather.scopes.uses(name);
            if uses > 1 {
                return EditTarget::Locked(LockReason::SharedBinding {
                    name: name.to_string(),
                    uses,
                });
            }
            resolve_shape(shape, gather, text, ctx, checked)
        }
        Shape::Call(callee) => EditTarget::Locked(LockReason::UnsupportedCall {
            callee: callee.clone(),
        }),
        Shape::Ident(_) | Shape::Member { .. } | Shape::Other(_) => unresolved(),
    }
}

/// The keyframe rule: editable only at a frame that is an `inputRange` entry.
///
/// `frame` is one of `ctx.candidate_frames`, and the received value tells
/// them apart: a candidate whose value differs is ruled out. A keyframe's
/// value is its `outputRange` entry when the easing gives 0 and 1 at the ends
/// of each segment (none, or a named `Easings` one), and a keyframe candidate
/// is kept otherwise; between or outside the keyframes the value is known only
/// without an `options` argument (linear, `extend` extrapolation), and a
/// candidate there is kept otherwise.
/// The survivors must all point at the same target, or the frame is
/// uncertain. Unless `checked`, no received value is this value's (an
/// expression's input), so no candidate can be told apart: the frame is
/// uncertain.
fn resolve_keyframe(
    call: &Interpolate,
    gather: &Gather,
    text: &str,
    ctx: &ResolveContext,
    checked: bool,
) -> EditTarget {
    let not_literal = || gather.lock_expression(text, call.call);
    let Input::Frame { name, offset } = &call.input else {
        return not_literal();
    };
    if gather.scopes.declared_more_than_once(name) {
        return EditTarget::Locked(LockReason::UnresolvedIdentifier { name: name.clone() });
    }
    let frame_bound = matches!(
        gather.scopes.lookup(name),
        Some(Binding::Const(Shape::Call(callee))) if callee == "useCurrentFrame"
    );
    let (Some(inputs), Some(outputs)) = (&call.input_range, &call.output_range) else {
        return not_literal();
    };
    if !frame_bound || inputs.is_empty() || inputs.len() != outputs.len() {
        return not_literal();
    }
    if !checked {
        return EditTarget::Locked(LockReason::FrameUncertain);
    }
    // `interpolate()` always returns a number, so an element that received
    // none had the prop set by something else.
    let Some(received) = ctx.received.as_ref().and_then(EditValue::as_number) else {
        return EditTarget::Locked(LockReason::OverriddenByParent);
    };
    if ctx.candidate_frames.is_empty() {
        return EditTarget::Locked(LockReason::FrameUncertain);
    }

    let values: Vec<f64> = outputs.iter().map(|(_, value)| *value).collect();
    let mut survivors: Vec<Place> = Vec::new();
    for &candidate in &ctx.candidate_frames {
        let place = place_of(inputs, candidate - offset);
        let value = match place {
            Place::Keyframe(k) if call.keyframes_exact => Some(values[k]),
            Place::Keyframe(_) => None,
            Place::Between { .. } | Place::Outside if !call.has_options => {
                linear(inputs, &values, candidate - offset)
            }
            Place::Between { .. } | Place::Outside => None,
        };
        if value.is_some_and(|value| !same_value(value, received)) {
            continue;
        }
        let place = match place {
            Place::Between { previous, next, .. } => Place::Between {
                previous,
                next,
                frame: candidate,
            },
            other => other,
        };
        if !survivors.contains(&place) {
            survivors.push(place);
        }
    }

    match survivors.as_slice() {
        [] => EditTarget::Locked(LockReason::OverriddenByParent),
        [Place::Keyframe(k)] => {
            let (span, value) = outputs[*k];
            EditTarget::Keyframe {
                span,
                value,
                frame: inputs[*k] + offset,
            }
        }
        [
            Place::Between {
                previous,
                next,
                frame,
            },
        ] => EditTarget::Locked(LockReason::BetweenKeyframes {
            previous: previous + offset,
            next: next + offset,
            frame: *frame,
        }),
        [Place::Outside] => EditTarget::Locked(LockReason::OutsideKeyframes),
        // Different keyframes, or the same keyframes at different frames
        // (whose jumps would differ).
        _ => EditTarget::Locked(LockReason::FrameUncertain),
    }
}

/// Where a frame (already minus the literal offset) falls in `inputRange`.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Place {
    /// At `inputRange[k]`.
    Keyframe(usize),
    /// Strictly between two entries; `frame` is the candidate frame,
    /// filled in once the place is kept.
    Between {
        previous: f64,
        next: f64,
        frame: f64,
    },
    Outside,
}

fn place_of(inputs: &[f64], frame: f64) -> Place {
    if let Some(k) = inputs.iter().position(|i| (i - frame).abs() <= TOLERANCE) {
        return Place::Keyframe(k);
    }
    let previous = inputs
        .iter()
        .copied()
        .filter(|i| *i < frame)
        .reduce(f64::max);
    let next = inputs
        .iter()
        .copied()
        .filter(|i| *i > frame)
        .reduce(f64::min);
    match (previous, next) {
        (Some(previous), Some(next)) => Place::Between {
            previous,
            next,
            frame: 0.0,
        },
        _ => Place::Outside,
    }
}

/// `interpolate(frame, inputs, outputs)` without options, as the runtime
/// computes it (linear, extending the end segments outside), or `None` when
/// the ranges would make it throw.
fn linear(inputs: &[f64], outputs: &[f64], frame: f64) -> Option<f64> {
    if inputs.len() < 2 || inputs.windows(2).any(|w| w[1] <= w[0]) {
        return None;
    }
    let mut segment = 0;
    while segment < inputs.len() - 2 && frame >= inputs[segment + 1] {
        segment += 1;
    }
    let progress = (frame - inputs[segment]) / (inputs[segment + 1] - inputs[segment]);
    Some(outputs[segment] + progress * (outputs[segment + 1] - outputs[segment]))
}

/// Equal up to rounding, relative to the values' size.
fn same_value(a: f64, b: f64) -> bool {
    (a - b).abs() <= TOLERANCE * a.abs().max(b.abs()).max(1.0)
}

/// The resolved value must equal what the runtime saw; otherwise a parent or
/// another binding decided the prop.
/// Numbers match within the tolerance, strings and booleans exactly. A
/// literal of another kind than the received value (`x="12"` for a number)
/// is not written as the prop's value, so it locks as an expression.
fn check_received(
    target: EditTarget,
    received: Option<&EditValue>,
    text: &str,
    prop: &str,
) -> EditTarget {
    let same = match (&target, received) {
        (
            EditTarget::Literal {
                value: EditValue::Number(a),
                ..
            }
            | EditTarget::Keyframe { value: a, .. },
            Some(EditValue::Number(b)),
        ) => (a - b).abs() <= TOLERANCE,
        (EditTarget::Literal { span, value, form }, Some(received)) => {
            if std::mem::discriminant(value) != std::mem::discriminant(received) {
                return match form {
                    // The attribute alone: its source is its name.
                    LiteralForm::BareTrue => EditTarget::Locked(LockReason::Expression {
                        source: prop.to_string(),
                    }),
                    _ => expression(text, *span),
                };
            }
            value == received
        }
        (EditTarget::Literal { .. } | EditTarget::Keyframe { .. }, _) => false,
        _ => return target,
    };
    if same {
        target
    } else {
        EditTarget::Locked(LockReason::OverriddenByParent)
    }
}

pub(crate) struct Gather<'t> {
    element_start: u32,
    /// Anchored at the expression with this span instead of an element's
    /// attribute: the expression is read as the attribute's value.
    value: Option<Span>,
    path: PropPath<'t>,
    /// The span of the expression the rules last locked the prop on, so
    /// its inputs are read from exactly what was locked.
    ///
    /// Every lock path returns its `Expression` right after recording the
    /// span, so after [`read_written`] it is the span of the expression it
    /// returned (`None` for a lock without one). `resolve_inputs` reads it
    /// before resolving any input, which may lock again on the same
    /// `Gather`.
    pub(crate) locked: Cell<Option<Span>>,
    pub(crate) found: bool,
    has_spread: bool,
    /// A spread follows the last attribute that holds the prop.
    spread_after: bool,
    /// The last attribute that holds the prop.
    attr: Option<Shape>,
    /// For `style.<key>`: the key in the last `style` attribute.
    pub(crate) property: Option<StyleProperty>,
    children: Option<Children>,
    pub(crate) scopes: Scopes,
}

impl<'t> Gather<'t> {
    pub(crate) fn new(element_start: u32, path: PropPath<'t>) -> Self {
        Self {
            element_start,
            value: None,
            path,
            locked: Cell::new(None),
            found: false,
            has_spread: false,
            spread_after: false,
            attr: None,
            property: None,
            children: None,
            scopes: Scopes::new(element_start),
        }
    }

    /// Visits `program` anchored at the expression at `value`, found when
    /// one has exactly that span. Outside any function, the anchor reads
    /// the file's top-level `const`s (exported ones too), as a function's
    /// are read inside it: a `defineScenes` list sits at module level.
    pub(crate) fn at_value(program: &Program<'_>, value: Span) -> Gather<'static> {
        let mut gather = Gather {
            value: Some(value),
            ..Gather::new(value.start, PropPath::Attribute(""))
        };
        gather.visit_program(program);
        gather.scopes.enter_module(program);
        gather
    }

    /// What the rules read for the prop: JSX children before a `children`
    /// attribute, and nothing when a spread after the attribute may set it.
    pub(crate) fn read(&self) -> Option<Read<'_>> {
        match (self.path, &self.children, &self.property) {
            (
                PropPath::Children,
                Some(
                    children @ (Children::Text { .. }
                    | Children::Container { .. }
                    | Children::Reference { .. }
                    | Children::Other(_)),
                ),
                _,
            ) => Some(Read::Children(children)),
            _ if self.spread_after => None,
            (PropPath::Style(_), _, property) => property.as_ref().map(Read::Property),
            _ => self.attr.as_ref().map(Read::Attribute),
        }
    }

    /// Whether a spread may set the prop's attribute: one is written, and
    /// no attribute that holds the prop follows the last.
    pub(crate) fn spread_may_set(&self) -> bool {
        self.has_spread && (self.attr.is_none() || self.spread_after)
    }

    /// Locks the prop as the expression at `span`, and keeps the span in
    /// [`Self::locked`].
    fn lock_expression(&self, text: &str, span: Span) -> EditTarget {
        self.locked.set(Some(span));
        expression(text, span)
    }
}

impl<'a> Visit<'a> for Gather<'_> {
    fn visit_function(&mut self, it: &Function<'a>, flags: ScopeFlags) {
        self.scopes.enter_function(it);
        oxc_ast_visit::walk::walk_function(self, it, flags);
    }

    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        self.scopes.enter_arrow_function(it);
        oxc_ast_visit::walk::walk_arrow_function_expression(self, it);
    }

    fn visit_variable_declarator(&mut self, it: &VariableDeclarator<'a>) {
        self.scopes.declarator(it);
        oxc_ast_visit::walk::walk_variable_declarator(self, it);
    }

    fn visit_binding_identifier(&mut self, it: &BindingIdentifier<'a>) {
        self.scopes.declare(it);
    }

    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        self.scopes.reference(it);
    }

    fn visit_expression(&mut self, it: &Expression<'a>) {
        if self.value == Some(span_of(it.span())) && !self.found {
            self.found = true;
            self.attr = Some(shape_of(it));
        }
        oxc_ast_visit::walk::walk_expression(self, it);
    }

    fn visit_jsx_element(&mut self, it: &JSXElement<'a>) {
        if it.opening_element.span.start == self.element_start
            && self.value.is_none()
            && self.children.is_none()
        {
            self.children = Some(children_of(it));
        }
        oxc_ast_visit::walk::walk_jsx_element(self, it);
    }

    fn visit_jsx_opening_element(&mut self, it: &JSXOpeningElement<'a>) {
        if it.span.start == self.element_start && self.value.is_none() && !self.found {
            self.found = true;
            for item in &it.attributes {
                match item {
                    JSXAttributeItem::SpreadAttribute(_) => {
                        self.has_spread = true;
                        self.spread_after = self.attr.is_some();
                    }
                    JSXAttributeItem::Attribute(attr) => {
                        let JSXAttributeName::Identifier(name) = &attr.name else {
                            continue;
                        };
                        if name.name != self.path.attribute() {
                            continue;
                        }
                        if let PropPath::Style(key) = self.path {
                            self.property = Some(style_property(attr, key));
                        }
                        // A later duplicate wins, as at runtime.
                        let whole = Shape::Other(span_of(attr.span));
                        let shape = match &attr.value {
                            Some(JSXAttributeValue::ExpressionContainer(c)) => {
                                c.expression.as_expression().map_or(whole, shape_of)
                            }
                            Some(JSXAttributeValue::StringLiteral(s)) => match string_of(s) {
                                // oxc keeps entities (`&amp;`) undecoded, so
                                // a value with `&` may not be what the
                                // runtime receives.
                                Some((value, _)) if value.contains('&') => {
                                    Shape::Other(span_of(s.span))
                                }
                                Some((value, quote)) => Shape::String(
                                    span_of(s.span),
                                    value,
                                    LiteralForm::Quoted(quote),
                                ),
                                None => whole,
                            },
                            // `name` alone: the empty point after the name,
                            // where `={false}` would go.
                            None => {
                                let end = name.span.end;
                                Shape::Bool(Span { start: end, end }, true, LiteralForm::BareTrue)
                            }
                            Some(
                                JSXAttributeValue::Element(_) | JSXAttributeValue::Fragment(_),
                            ) => whole,
                        };
                        self.attr = Some(shape);
                        self.spread_after = false;
                    }
                }
            }
        }
        oxc_ast_visit::walk::walk_jsx_opening_element(self, it);
    }
}
