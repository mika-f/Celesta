use celesta_source_edit::{
    EditTarget, EditValue, ElementRef, LiteralForm, LockReason, ResolveContext, Span, element_at,
    resolve_inputs,
};

/// The primitive's context: it rendered once and received `200`, the
/// expression's result.
fn ctx() -> ResolveContext {
    ResolveContext {
        primitive: true,
        component_props: false,
        candidate_frames: vec![0.0],
        received: Some(EditValue::Number(200.0)),
        render_count: 1,
    }
}

/// The element whose `<` is the first `needle` in `text` (ASCII only).
fn element(text: &str, needle: &str) -> ElementRef {
    let offset = text.find(needle).unwrap();
    let before = &text[..offset];
    let line = before.matches('\n').count() as u32 + 1;
    let column = (offset - before.rfind('\n').map_or(0, |i| i + 1)) as u32 + 1;
    element_at(text, line, column).unwrap()
}

/// A component `Scene` declaring `consts` and returning `jsx`.
fn scene(consts: &str, jsx: &str) -> String {
    format!("export function Scene() {{\n  {consts}\n  return {jsx};\n}}\n")
}

/// The inputs of `prop` on the first `<{tag}` of `text`.
fn inputs(text: &str, tag: &str, prop: &str) -> Vec<(String, EditTarget)> {
    resolve_inputs(text, &element(text, &format!("<{tag}")), prop, &ctx())
}

fn unresolved(name: &str) -> (String, EditTarget) {
    (
        name.to_string(),
        EditTarget::Locked(LockReason::UnresolvedIdentifier {
            name: name.to_string(),
        }),
    )
}

/// The literal `value` written as `literal`, the first in `text` after `after`.
fn literal(
    text: &str,
    after: &str,
    literal: &str,
    value: EditValue,
    form: LiteralForm,
) -> EditTarget {
    let from = text.find(after).unwrap();
    let start = (from + text[from..].find(literal).unwrap()) as u32;
    EditTarget::Literal {
        span: Span {
            start,
            end: start + literal.len() as u32,
        },
        value,
        form,
    }
}

#[test]
fn literal_inputs() {
    let text = scene(
        "const padding = 16;",
        "<Rect width={textWidth + padding * 2} />",
    );
    assert_eq!(
        inputs(&text, "Rect", "width"),
        vec![
            unresolved("textWidth"),
            (
                "padding".to_string(),
                literal(
                    &text,
                    "padding =",
                    "16",
                    EditValue::Number(16.0),
                    LiteralForm::Number
                ),
            ),
        ]
    );
}

#[test]
fn member_object_only() {
    let text = scene("", "<Rect x={C.x + 4} />");
    assert_eq!(inputs(&text, "Rect", "x"), vec![unresolved("C")]);
}

#[test]
fn one_level() {
    // A const that is an expression is not expanded.
    let text = scene("const a = b + 1;", "<Rect x={a * 2} />");
    assert_eq!(inputs(&text, "Rect", "x"), vec![unresolved("a")]);
}

#[test]
fn not_an_expression() {
    let text = scene("", "<Rect x={12} />");
    assert_eq!(inputs(&text, "Rect", "x"), vec![]);
    // Locked for another reason than an expression.
    let text = scene("", "<Rect x={y + 1} {...rest} />");
    assert_eq!(inputs(&text, "Rect", "x"), vec![]);
    let text = scene("", "<Rect y={1} />");
    assert_eq!(inputs(&text, "Rect", "x"), vec![]);
}

#[test]
fn each_name_once_in_order() {
    let text = scene("", "<Rect x={b + a * b - a} />");
    assert_eq!(
        inputs(&text, "Rect", "x"),
        vec![unresolved("b"), unresolved("a")]
    );
}

#[test]
fn children_and_style_inputs() {
    let text = scene("const label = 'Sale';", "<Text>{label}: {count}</Text>");
    assert_eq!(
        inputs(&text, "Text", "children"),
        vec![
            (
                "label".to_string(),
                literal(
                    &text,
                    "label =",
                    "'Sale'",
                    EditValue::String("Sale".to_string()),
                    LiteralForm::ContainerQuoted('\''),
                ),
            ),
            unresolved("count"),
        ]
    );

    let text = scene("const w = 10;", "<Rect style={{ width: w + 2 }} />");
    assert_eq!(
        inputs(&text, "Rect", "style.width"),
        vec![(
            "w".to_string(),
            literal(
                &text,
                "w =",
                "10",
                EditValue::Number(10.0),
                LiteralForm::Number
            ),
        )]
    );
}

#[test]
fn skips_callees_tags_types_and_locals() {
    let text = scene(
        "",
        "<Rect x={values.reduce((sum: Total, v) => sum + v * scale, 0) as Width} />",
    );
    assert_eq!(inputs(&text, "Rect", "x"), vec![unresolved("scale")]);

    let text = scene("", "<Box>{show && <Text.Label>{label}</Text.Label>}</Box>");
    assert_eq!(
        inputs(&text, "Box", "children"),
        vec![unresolved("show"), unresolved("label")]
    );
}

#[test]
fn every_locked_expression_has_inputs() {
    // An `interpolate()` that does not read the frame.
    let text = scene("", "<Rect x={interpolate(t, [0, 10], [0, 1])} />");
    assert_eq!(inputs(&text, "Rect", "x"), vec![unresolved("t")]);

    // The same call in the `const` the prop reads: the inputs are the
    // initializer's.
    let text = scene(
        "const o = interpolate(t, [0, 10], [0, 1]);",
        "<Rect x={o} />",
    );
    assert_eq!(inputs(&text, "Rect", "x"), vec![unresolved("t")]);

    // A `style` value that is not an object literal.
    let text = scene("", "<Rect style={box} />");
    assert_eq!(
        inputs(&text, "Rect", "style.width"),
        vec![unresolved("box")]
    );

    // A member child.
    let text = scene("", "<Text>{C.label}</Text>");
    assert_eq!(inputs(&text, "Text", "children"), vec![unresolved("C")]);
}

#[test]
fn shared_const_input() {
    let text = scene(
        "const padding = 16;",
        "<Rect width={textWidth + padding} height={padding} />",
    );
    assert_eq!(
        inputs(&text, "Rect", "width"),
        vec![
            unresolved("textWidth"),
            (
                "padding".to_string(),
                EditTarget::Locked(LockReason::SharedBinding {
                    name: "padding".to_string(),
                    uses: 2,
                }),
            ),
        ]
    );
}

#[test]
fn parameter_input_needs_a_call_site() {
    let text = "export function Card({ pad }) {\n  return <Rect x={pad + offset} />;\n}\n";
    assert_eq!(
        inputs(text, "Rect", "x"),
        vec![
            (
                "pad".to_string(),
                EditTarget::Locked(LockReason::CallSiteNotFound {
                    component: "Card".to_string(),
                }),
            ),
            unresolved("offset"),
        ]
    );
}
