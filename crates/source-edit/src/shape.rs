//! Expressions reduced to the shapes the resolve rules tell apart.

use oxc_ast::ast::{
    BinaryOperator, CallExpression, Expression, JSXAttribute, JSXAttributeValue, JSXChild,
    JSXElement, ObjectPropertyKind, PropertyKind, StringLiteral, UnaryOperator,
};
use oxc_span::GetSpan;

use crate::resolve::{LiteralForm, Span};

/// An element's children, reduced to what the rules need. A variant's
/// `span` covers everything between the tags; `Reference` has none, since
/// it is resolved through the binding it reads.
#[derive(Debug)]
pub(crate) enum Children {
    /// `<Text />`.
    SelfClosing,
    /// Nothing React renders; text goes at `insert_at`, the end of the
    /// opening tag.
    Empty { insert_at: u32 },
    /// One JSXText child; `value` is the text as React renders it.
    Text { span: Span, value: String },
    /// One `{'…'}` / `{"…"}` child.
    Container {
        span: Span,
        value: String,
        quote: char,
    },
    /// One `{name}` or `{name.key}` child; `shape` is the `Ident` or
    /// `Member` it reads.
    Reference { shape: Shape },
    /// Several children, an element, an expression.
    Other(Span),
}

pub(crate) fn children_of(element: &JSXElement<'_>) -> Children {
    let Some(closing) = &element.closing_element else {
        return Children::SelfClosing;
    };
    let span = Span {
        start: element.opening_element.span.end,
        end: closing.span.start,
    };
    // Text that collapses to nothing (indentation between tags) renders
    // nothing.
    let rendered: Vec<&JSXChild<'_>> = element
        .children
        .iter()
        .filter(
            |child| !matches!(child, JSXChild::Text(t) if collapse_jsx_text(&t.value).is_empty()),
        )
        .collect();
    match rendered.as_slice() {
        [] => Children::Empty {
            insert_at: span.start,
        },
        // oxc keeps entities (`&amp;`) undecoded, so text with `&` may not be
        // what React renders.
        [JSXChild::Text(t)] if !t.value.contains('&') => Children::Text {
            span,
            value: collapse_jsx_text(&t.value),
        },
        [JSXChild::ExpressionContainer(c)] => {
            let Some(expression) = c.expression.as_expression() else {
                return Children::Other(span);
            };
            match expression.without_parentheses() {
                Expression::StringLiteral(s) => match string_of(s) {
                    Some((value, quote)) => Children::Container { span, value, quote },
                    None => Children::Other(span),
                },
                _ => match shape_of(expression) {
                    shape @ (Shape::Ident(_) | Shape::Member { .. }) => {
                        Children::Reference { shape }
                    }
                    _ => Children::Other(span),
                },
            }
        }
        _ => Children::Other(span),
    }
}

/// JSXText as React renders it, collapsed as esbuild compiles it:
/// tabs read as spaces, every line but the first loses its leading spaces
/// and every line but the last its trailing ones, lines left empty are
/// dropped, and the rest are joined with single spaces.
pub(crate) fn collapse_jsx_text(raw: &str) -> String {
    let normalized = raw
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\t', " ");
    let lines: Vec<&str> = normalized.split('\n').collect();
    let last = lines.len() - 1;
    let kept: Vec<&str> = lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let line = if i > 0 {
                line.trim_start_matches(' ')
            } else {
                line
            };
            if i < last {
                line.trim_end_matches(' ')
            } else {
                line
            }
        })
        .filter(|line| !line.is_empty())
        .collect();
    kept.join(" ")
}

/// Property `key` of a `style` attribute's value.
#[derive(Debug)]
pub(crate) enum StyleProperty {
    /// The last property named `key`, with no spread or computed key after
    /// it.
    Value(Shape),
    /// No property is, or may be, named `key`. `last` is the end of the
    /// last property, if any.
    Absent { object: Span, last: Option<u32> },
    /// A spread or a computed key may set it.
    Unknown,
    /// The value is not an object literal; `span` is its source.
    NotObject(Span),
}

pub(crate) fn style_property(attr: &JSXAttribute<'_>, key: &str) -> StyleProperty {
    let value = match &attr.value {
        Some(JSXAttributeValue::ExpressionContainer(c)) => c.expression.as_expression(),
        _ => None,
    };
    let Some(value) = value else {
        // `style="…"`, or `style` alone.
        let span = attr.value.as_ref().map_or(attr.span, GetSpan::span);
        return StyleProperty::NotObject(span_of(span));
    };
    let Expression::ObjectExpression(object) = value.without_parentheses() else {
        return StyleProperty::NotObject(span_of(value.span()));
    };
    let mut result = None;
    for property in &object.properties {
        let named = match property {
            ObjectPropertyKind::ObjectProperty(p) if !p.computed => {
                p.key.static_name().map(|name| (name == key, p))
            }
            _ => None,
        };
        match named {
            Some((false, _)) => {}
            // A later key wins, as at runtime.
            Some((true, p)) => {
                result = Some(StyleProperty::Value(
                    if p.kind == PropertyKind::Init && !p.method {
                        shape_of(&p.value)
                    } else {
                        Shape::Other(span_of(p.span))
                    },
                ));
            }
            None => result = Some(StyleProperty::Unknown),
        }
    }
    result.unwrap_or_else(|| StyleProperty::Absent {
        object: span_of(object.span),
        last: object.properties.last().map(|p| p.span().end),
    })
}

/// An attribute value or initializer, reduced to what the rules need.
#[derive(Debug)]
pub(crate) enum Shape {
    Number(Span, f64),
    String(Span, String, LiteralForm),
    Bool(Span, bool, LiteralForm),
    Interpolate(Interpolate),
    Call(String),
    Ident(String),
    /// `object.property`, as `props.label`.
    Member {
        span: Span,
        object: String,
        property: String,
    },
    Other(Span),
}

/// An `interpolate(input, inputRange, outputRange, ...)` call whose ranges
/// are kept only when they are arrays of numeric literals.
#[derive(Debug)]
pub(crate) struct Interpolate {
    /// The whole call, reported when it is not editable.
    pub(crate) call: Span,
    pub(crate) input: Input,
    pub(crate) input_range: Option<Vec<f64>>,
    pub(crate) output_range: Option<Vec<(Span, f64)>>,
    /// A fourth (`options`) argument is written: easing or extrapolation
    /// may change the value between and outside the keyframes.
    pub(crate) has_options: bool,
    /// The value at each `inputRange` entry is its `outputRange` entry: the
    /// easing is absent or a named `Easings` member, so it gives 0 and 1 at
    /// the ends of a segment. A hand-written one may not
    /// (`t => t === 0 ? 0.5 : t`).
    pub(crate) keyframes_exact: bool,
}

#[derive(Debug)]
pub(crate) enum Input {
    /// `name`, or `name - offset`.
    Frame {
        name: String,
        offset: f64,
    },
    Other,
}

pub(crate) fn shape_of(expr: &Expression<'_>) -> Shape {
    match expr.without_parentheses() {
        Expression::NumericLiteral(n) => Shape::Number(
            Span {
                start: n.span.start,
                end: n.span.end,
            },
            n.value,
        ),
        Expression::UnaryExpression(u) if u.operator == UnaryOperator::UnaryNegation => {
            match u.argument.without_parentheses() {
                Expression::NumericLiteral(n) => Shape::Number(
                    Span {
                        start: u.span.start,
                        end: u.span.end,
                    },
                    -n.value,
                ),
                _ => other(expr),
            }
        }
        Expression::StringLiteral(s) => match string_of(s) {
            Some((value, quote)) => {
                Shape::String(span_of(s.span), value, LiteralForm::ContainerQuoted(quote))
            }
            None => other(expr),
        },
        Expression::TemplateLiteral(t) => match (t.expressions.is_empty(), t.quasis.first()) {
            (true, Some(quasi)) if !quasi.lone_surrogates => match &quasi.value.cooked {
                Some(cooked) => {
                    Shape::String(span_of(t.span), cooked.to_string(), LiteralForm::Template)
                }
                None => other(expr),
            },
            _ => other(expr),
        },
        Expression::BooleanLiteral(b) => Shape::Bool(span_of(b.span), b.value, LiteralForm::Bool),
        Expression::CallExpression(call) => {
            let callee = callee_name(&call.callee);
            if callee == "interpolate" {
                Shape::Interpolate(interpolate_of(call))
            } else {
                Shape::Call(callee)
            }
        }
        Expression::Identifier(id) => Shape::Ident(id.name.to_string()),
        Expression::StaticMemberExpression(m) => match m.object.without_parentheses() {
            Expression::Identifier(object) => Shape::Member {
                span: span_of(expr.span()),
                object: object.name.to_string(),
                property: m.property.name.to_string(),
            },
            _ => other(expr),
        },
        _ => other(expr),
    }
}

fn interpolate_of(call: &CallExpression<'_>) -> Interpolate {
    let arg = |i: usize| call.arguments.get(i).and_then(|a| a.as_expression());
    let input = match arg(0).map(Expression::without_parentheses) {
        Some(Expression::Identifier(id)) => Input::Frame {
            name: id.name.to_string(),
            offset: 0.0,
        },
        Some(Expression::BinaryExpression(b)) if b.operator == BinaryOperator::Subtraction => {
            match (b.left.without_parentheses(), b.right.without_parentheses()) {
                (Expression::Identifier(id), Expression::NumericLiteral(n)) => Input::Frame {
                    name: id.name.to_string(),
                    offset: n.value,
                },
                _ => Input::Other,
            }
        }
        _ => Input::Other,
    };
    let numbers = |i: usize| -> Option<Vec<(Span, f64)>> {
        let Some(Expression::ArrayExpression(array)) = arg(i).map(Expression::without_parentheses)
        else {
            return None;
        };
        array
            .elements
            .iter()
            .map(|e| match shape_of(e.as_expression()?) {
                Shape::Number(span, value) => Some((span, value)),
                _ => None,
            })
            .collect()
    };
    Interpolate {
        call: Span {
            start: call.span.start,
            end: call.span.end,
        },
        input,
        input_range: numbers(1).map(|v| v.into_iter().map(|(_, n)| n).collect()),
        output_range: numbers(2),
        has_options: call.arguments.len() > 3,
        keyframes_exact: arg(3).is_none_or(known_easing),
    }
}

/// Whether `options` is an object literal whose `easing`, if written, is a
/// plain `Easings.<name>`.
fn known_easing(options: &Expression<'_>) -> bool {
    let Expression::ObjectExpression(object) = options.without_parentheses() else {
        return false;
    };
    object.properties.iter().all(|property| {
        let ObjectPropertyKind::ObjectProperty(property) = property else {
            return false;
        };
        if property.computed {
            return false;
        }
        if property
            .key
            .static_name()
            .is_none_or(|name| name != "easing")
        {
            return true;
        }
        !property.shorthand
            && matches!(
                property.value.without_parentheses(),
                Expression::StaticMemberExpression(m)
                    if matches!(&m.object, Expression::Identifier(id) if id.name == "Easings")
            )
    })
}

/// A string literal's value and opening quote, or `None` when the value is
/// not exactly the written text (lone surrogates).
pub(crate) fn string_of(s: &StringLiteral<'_>) -> Option<(String, char)> {
    if s.lone_surrogates {
        return None;
    }
    let quote = s.raw.as_ref()?.chars().next()?;
    Some((s.value.to_string(), quote))
}

pub(crate) fn span_of(span: oxc_span::Span) -> Span {
    Span {
        start: span.start,
        end: span.end,
    }
}

fn other(expr: &Expression<'_>) -> Shape {
    Shape::Other(span_of(expr.span()))
}

fn callee_name(callee: &Expression<'_>) -> String {
    match callee.without_parentheses() {
        Expression::Identifier(id) => id.name.to_string(),
        // `Foo.bar(...)`: report the dotted path where it is a plain chain.
        Expression::StaticMemberExpression(m) => {
            format!("{}.{}", callee_name(&m.object), m.property.name)
        }
        _ => "<expression>".to_string(),
    }
}
