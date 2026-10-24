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

/// The inputs of `prop` on the first `<{tag}` of `text`, with no call site.
fn inputs(text: &str, tag: &str, prop: &str) -> Vec<(String, EditTarget)> {
    resolve_inputs(text, &element(text, &format!("<{tag}")), prop, &ctx(), None)
}

/// The inputs of `prop` on the first `<{tag}` of `text`, called by the first
/// `<{caller}` of `call_site`.
fn inputs_through(
    text: &str,
    tag: &str,
    prop: &str,
    call_site: &str,
    caller: &str,
) -> Vec<(String, EditTarget)> {
    let call = element(call_site, &format!("<{caller}"));
    resolve_inputs(
        text,
        &element(text, &format!("<{tag}")),
        prop,
        &ctx(),
        Some((call_site, &call)),
    )
}

/// The names of `inputs`, in order.
fn names(inputs: &[(String, EditTarget)]) -> Vec<&str> {
    inputs.iter().map(|(name, _)| name.as_str()).collect()
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

const TAG: &str = "function Tag({ padding = 8 }) { const w = 100; return <Rect width={w + padding * 2} height={10} />; }";

#[test]
fn parameter_input_resolves_at_the_call_site() {
    let call_site = "<Tag padding={16} />";
    assert_eq!(
        inputs_through(TAG, "Rect", "width", call_site, "Tag"),
        vec![
            (
                "w".to_string(),
                literal(
                    TAG,
                    "w =",
                    "100",
                    EditValue::Number(100.0),
                    LiteralForm::Number
                ),
            ),
            (
                "padding".to_string(),
                EditTarget::PassThrough {
                    call_site: element(call_site, "<Tag"),
                    param: "padding".to_string(),
                    target: Box::new(literal(
                        call_site,
                        "padding=",
                        "16",
                        EditValue::Number(16.0),
                        LiteralForm::Number
                    )),
                },
            ),
        ]
    );
}

#[test]
fn parameter_input_missing_at_the_call_site() {
    let call_site = "<Tag />";
    let found = inputs_through(TAG, "Rect", "width", call_site, "Tag");
    assert_eq!(
        found[1],
        (
            "padding".to_string(),
            EditTarget::PassThrough {
                call_site: element(call_site, "<Tag"),
                param: "padding".to_string(),
                target: Box::new(EditTarget::Missing {
                    insert_at: element(call_site, "<Tag").end_of_attributes,
                    default: Some(EditValue::Number(8.0)),
                }),
            },
        )
    );
}

#[test]
fn parameter_input_passed_on_by_the_caller() {
    // A second hop: the call site passes a parameter of its own component.
    let call_site = "function Card({ pad }) { return <Tag padding={pad} />; }";
    let found = inputs_through(TAG, "Rect", "width", call_site, "Tag");
    assert_eq!(
        found[1],
        (
            "padding".to_string(),
            EditTarget::Locked(LockReason::ParameterOfCaller {
                component: "Card".to_string(),
            }),
        )
    );
}

#[test]
fn parameter_input_without_a_call_site() {
    assert_eq!(
        inputs(TAG, "Rect", "width")[1],
        (
            "padding".to_string(),
            EditTarget::Locked(LockReason::CallSiteNotFound {
                component: "Tag".to_string(),
            }),
        )
    );
}

#[test]
fn an_animated_input_has_an_unknown_frame() {
    // The runtime reports the expression's result, not `w`, so no received
    // value tells the candidate frames apart.
    let text = scene(
        "const frame = useCurrentFrame(); const w = interpolate(frame, [0, 10], [0, 100]);",
        "<Rect width={w + 2} />",
    );
    assert_eq!(
        inputs(&text, "Rect", "width"),
        vec![(
            "w".to_string(),
            EditTarget::Locked(LockReason::FrameUncertain)
        )]
    );

    // The same at the call site.
    let tag = "function Tag({ w }) { return <Rect width={w + 2} />; }";
    let call_site = "function Scene() { const frame = useCurrentFrame(); return <Tag w={interpolate(frame, [0, 10], [0, 100])} />; }";
    assert_eq!(
        inputs_through(tag, "Rect", "width", call_site, "Tag"),
        vec![(
            "w".to_string(),
            EditTarget::PassThrough {
                call_site: element(call_site, "<Tag"),
                param: "w".to_string(),
                target: Box::new(EditTarget::Locked(LockReason::FrameUncertain)),
            }
        )]
    );
}

#[test]
fn an_input_used_twice_in_one_expression_is_one_place() {
    let text = scene("const padding = 16;", "<Rect width={padding + padding} />");
    assert_eq!(
        inputs(&text, "Rect", "width"),
        vec![(
            "padding".to_string(),
            literal(
                &text,
                "padding =",
                "16",
                EditValue::Number(16.0),
                LiteralForm::Number
            ),
        )]
    );

    // Read elsewhere too: two places.
    let text = scene(
        "const padding = 16;",
        "<Rect width={padding + padding} height={padding} />",
    );
    assert_eq!(
        inputs(&text, "Rect", "width"),
        vec![(
            "padding".to_string(),
            EditTarget::Locked(LockReason::SharedBinding {
                name: "padding".to_string(),
                uses: 2,
            }),
        )]
    );
}

#[test]
fn a_lambda_parameter_only_hides_its_own_body() {
    let text = scene(
        "const padding = 16;",
        "<Rect width={items.map((padding) => padding).length + padding} />",
    );
    assert_eq!(
        inputs(&text, "Rect", "width"),
        vec![(
            "padding".to_string(),
            literal(
                &text,
                "padding =",
                "16",
                EditValue::Number(16.0),
                LiteralForm::Number
            ),
        )]
    );
}

#[test]
fn every_callee_form_skips_only_the_function_name() {
    let text = scene(
        "const size = 1, gap = 2, depth = 3, i = 0;",
        "<Rect width={new Box(size).w + tag`${gap}`.length + fns[i](depth)} />",
    );
    assert_eq!(
        names(&inputs(&text, "Rect", "width")),
        vec!["size", "gap", "fns", "i", "depth"]
    );
}
