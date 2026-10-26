use celesta_source_edit::{
    DefaultTarget, EditTarget, EditValue, ElementRef, Lift, LiftLock, LiftProp, LiteralForm,
    ResolveContext, Span, apply, element_at, lift_name, parameter_default, parses, plan_lift,
    resolve,
};

/// The primitive's context: it rendered once and received `received`.
fn ctx(received: Option<EditValue>) -> ResolveContext {
    ResolveContext {
        primitive: true,
        component_props: false,
        candidate_frames: vec![0.0],
        received,
        render_count: 1,
    }
}

fn number(value: f64) -> EditValue {
    EditValue::Number(value)
}

fn string(value: &str) -> EditValue {
    EditValue::String(value.to_string())
}

/// The element whose `<` is the first `needle` in `text` (ASCII only).
fn element(text: &str, needle: &str) -> ElementRef {
    let offset = text.find(needle).unwrap();
    let before = &text[..offset];
    let line = before.matches('\n').count() as u32 + 1;
    let column = (offset - before.rfind('\n').map_or(0, |i| i + 1)) as u32 + 1;
    element_at(text, line, column).unwrap()
}

/// Plans lifting each `(prop, received)` of the first `needle` in `text`,
/// whose targets `resolve` finds; `received` also types the field.
fn plan(text: &str, needle: &str, props: &[(&str, Option<EditValue>)]) -> Result<Lift, LiftLock> {
    let at = element(text, needle);
    let targets: Vec<EditTarget> = props
        .iter()
        .map(|(prop, received)| resolve(text, &at, prop, &ctx(received.clone())))
        .collect();
    let values: Vec<EditValue> = props
        .iter()
        .map(|(_, received)| received.clone().unwrap_or(string("")))
        .collect();
    let lift_props: Vec<LiftProp<'_>> = props
        .iter()
        .zip(&targets)
        .zip(&values)
        .map(|(((prop, _), target), value)| LiftProp {
            prop,
            target,
            value,
        })
        .collect();
    plan_lift(text, &at, &lift_props)
}

/// The text after lifting, which must parse, and the new parameters.
fn lifted(text: &str, needle: &str, props: &[(&str, Option<EditValue>)]) -> (String, Vec<String>) {
    let lift = plan(text, needle, props).unwrap();
    let out = apply(text, &lift.edits).unwrap();
    assert!(parses(&out), "does not parse:\n{out}");
    (out, lift.params)
}

/// The text after lifting one prop that received `received`.
fn lift_one(text: &str, needle: &str, prop: &str, received: Option<EditValue>) -> String {
    lifted(text, needle, &[(prop, received)]).0
}

#[test]
fn lift_name_camel_cases_the_tag_and_appends_on_a_clash() {
    assert_eq!(lift_name("Rect", "width", &[]), "rectWidth");
    assert_eq!(lift_name("Text", "fontSize", &[]), "textFontSize");
    assert_eq!(lift_name("Rect", "x", &[]), "rectX");
    assert_eq!(lift_name("Text", "children", &[]), "textChildren");
    assert_eq!(
        lift_name("Rect", "width", &["rectWidth".to_string()]),
        "rectWidth2"
    );
    assert_eq!(
        lift_name(
            "Rect",
            "width",
            &["rectWidth".to_string(), "rectWidth2".to_string()]
        ),
        "rectWidth3"
    );
}

#[test]
fn lifts_into_an_object_pattern() {
    let text = "interface Props {\n  label: string;\n}\n\nfunction PriceTag({ label }: Props) {\n  return <Rect width={200} height={40} />;\n}\n";
    let (out, params) = lifted(text, "<Rect", &[("width", Some(number(200.0)))]);
    assert_eq!(params, ["rectWidth"]);
    assert_eq!(
        out,
        "interface Props {\n  label: string;\n  rectWidth?: number;\n}\n\nfunction PriceTag({ label, rectWidth = 200 }: Props) {\n  return <Rect width={rectWidth} height={40} />;\n}\n"
    );
}

#[test]
fn lifts_with_no_parameter() {
    let text = "function C() {\n  return <Rect width={200} />;\n}\n";
    assert_eq!(
        lift_one(text, "<Rect", "width", Some(number(200.0))),
        "function C({ rectWidth = 200 }) {\n  return <Rect width={rectWidth} />;\n}\n"
    );

    let text = "const C = () => <Rect width={200} />;\n";
    assert_eq!(
        lift_one(text, "<Rect", "width", Some(number(200.0))),
        "const C = ({ rectWidth = 200 }) => <Rect width={rectWidth} />;\n"
    );
}

#[test]
fn inserts_before_a_rest_element() {
    let text = "function C({ label, ...rest }) {\n  return <Rect {...rest} width={200} />;\n}\n";
    assert_eq!(
        lift_one(text, "<Rect", "width", Some(number(200.0))),
        "function C({ label, rectWidth = 200, ...rest }) {\n  return <Rect {...rest} width={rectWidth} />;\n}\n"
    );

    let text = "function C({ ...rest }) {\n  return <Rect width={200} />;\n}\n";
    assert_eq!(
        lift_one(text, "<Rect", "width", Some(number(200.0))),
        "function C({ rectWidth = 200, ...rest }) {\n  return <Rect width={rectWidth} />;\n}\n"
    );

    let text = "function C({}) {\n  return <Rect width={200} />;\n}\n";
    assert_eq!(
        lift_one(text, "<Rect", "width", Some(number(200.0))),
        "function C({ rectWidth = 200 }) {\n  return <Rect width={rectWidth} />;\n}\n"
    );
}

#[test]
fn lifts_into_an_inline_type_literal() {
    let text = "function C({ label }: { label: string }) {\n  return <Rect width={200} />;\n}\n";
    assert_eq!(
        lift_one(text, "<Rect", "width", Some(number(200.0))),
        "function C({ label, rectWidth = 200 }: { label: string; rectWidth?: number }) {\n  return <Rect width={rectWidth} />;\n}\n"
    );

    let text = "function C({ label, size }: { label: string, size: number }) {\n  return <Rect width={200} />;\n}\n";
    assert_eq!(
        lift_one(text, "<Rect", "width", Some(number(200.0))),
        "function C({ label, size, rectWidth = 200 }: { label: string, size: number, rectWidth?: number }) {\n  return <Rect width={rectWidth} />;\n}\n"
    );
}

#[test]
fn lifts_into_a_local_interface() {
    let text = "export interface Props {\n  label: string;\n}\n\nexport function C({ label }: Props) {\n  return <Rect width={200} />;\n}\n";
    assert_eq!(
        lift_one(text, "<Rect", "width", Some(number(200.0))),
        "export interface Props {\n  label: string;\n  rectWidth?: number;\n}\n\nexport function C({ label, rectWidth = 200 }: Props) {\n  return <Rect width={rectWidth} />;\n}\n"
    );

    let text = "function C({ label }: Props) {\n  return <Rect width={200} />;\n}\n\ninterface Props { label: string, size: number }\n";
    assert_eq!(
        lift_one(text, "<Rect", "width", Some(number(200.0))),
        "function C({ label, rectWidth = 200 }: Props) {\n  return <Rect width={rectWidth} />;\n}\n\ninterface Props { label: string, size: number, rectWidth?: number }\n"
    );

    let text =
        "interface Props {}\n\nfunction C({ label }: Props) {\n  return <Rect width={200} />;\n}\n";
    assert_eq!(
        lift_one(text, "<Rect", "width", Some(number(200.0))),
        "interface Props { rectWidth?: number }\n\nfunction C({ label, rectWidth = 200 }: Props) {\n  return <Rect width={rectWidth} />;\n}\n"
    );
}

#[test]
fn lifts_into_a_local_type_alias() {
    let text = "type Props = {\n  label: string,\n};\n\nfunction C({ label }: Props) {\n  return <Rect width={200} />;\n}\n";
    assert_eq!(
        lift_one(text, "<Rect", "width", Some(number(200.0))),
        "type Props = {\n  label: string,\n  rectWidth?: number,\n};\n\nfunction C({ label, rectWidth = 200 }: Props) {\n  return <Rect width={rectWidth} />;\n}\n"
    );

    let text = "type Props = { label: string };\n\nfunction C({ label }: Props) {\n  return <Rect width={200} />;\n}\n";
    assert_eq!(
        lift_one(text, "<Rect", "width", Some(number(200.0))),
        "type Props = { label: string; rectWidth?: number };\n\nfunction C({ label, rectWidth = 200 }: Props) {\n  return <Rect width={rectWidth} />;\n}\n"
    );
}

#[test]
fn lifts_into_an_intersection_alias_s_type_literal() {
    let text = "interface Base { id: string }\ntype Props = { label: string } & Base;\n\nfunction C({ label }: Props) {\n  return <Rect width={200} />;\n}\n";
    assert_eq!(
        lift_one(text, "<Rect", "width", Some(number(200.0))),
        "interface Base { id: string }\ntype Props = { label: string; rectWidth?: number } & Base;\n\nfunction C({ label, rectWidth = 200 }: Props) {\n  return <Rect width={rectWidth} />;\n}\n"
    );
}

#[test]
fn an_alias_without_a_type_literal_locks() {
    let text = "interface Base { id: string }\ninterface Other { label: string }\ntype Props = Base & Other;\n\nfunction C({ label }: Props) {\n  return <Rect width={200} />;\n}\n";
    assert_eq!(
        plan(text, "<Rect", &[("width", Some(number(200.0)))]),
        Err(LiftLock::PropsTypeNotEditable {
            component: "C".to_string(),
            param: "rectWidth".to_string(),
        })
    );
}

#[test]
fn reads_the_props_type_from_the_declarator_or_wrapper() {
    let props = "interface Props { label: string }\n";
    let extended = "interface Props { label: string; rectWidth?: number }\n";
    for (component, expected) in [
        (
            "const C: FC<Props> = ({ label }) => <Rect width={200} />;\n",
            "const C: FC<Props> = ({ label, rectWidth = 200 }) => <Rect width={rectWidth} />;\n",
        ),
        (
            "const C: React.FC<Props> = ({ label }) => <Rect width={200} />;\n",
            "const C: React.FC<Props> = ({ label, rectWidth = 200 }) => <Rect width={rectWidth} />;\n",
        ),
        (
            "const C: FC<Props> = () => <Rect width={200} />;\n",
            "const C: FC<Props> = ({ rectWidth = 200 }) => <Rect width={rectWidth} />;\n",
        ),
        (
            "const C = memo<Props>(({ label }) => <Rect width={200} />);\n",
            "const C = memo<Props>(({ label, rectWidth = 200 }) => <Rect width={rectWidth} />);\n",
        ),
        (
            "const C = forwardRef<HTMLDivElement, Props>(({ label }, ref) => <Rect width={200} />);\n",
            "const C = forwardRef<HTMLDivElement, Props>(({ label, rectWidth = 200 }, ref) => <Rect width={rectWidth} />);\n",
        ),
    ] {
        let text = format!("{props}{component}");
        assert_eq!(
            lift_one(&text, "<Rect", "width", Some(number(200.0))),
            format!("{extended}{expected}"),
            "{component}"
        );
    }

    let text = "interface Other { label: string }\nconst C: Other = ({ label }) => <Rect width={200} />;\n";
    assert_eq!(
        plan(text, "<Rect", &[("width", Some(number(200.0)))]),
        Err(LiftLock::PropsTypeNotEditable {
            component: "C".to_string(),
            param: "rectWidth".to_string(),
        })
    );
}

#[test]
fn an_imported_props_type_locks() {
    let text = "import type { Props } from './props';\n\nfunction PriceTag({ label }: Props) {\n  return <Rect width={200} />;\n}\n";
    assert_eq!(
        plan(text, "<Rect", &[("width", Some(number(200.0)))]),
        Err(LiftLock::ImportedPropsType {
            component: "PriceTag".to_string(),
        })
    );
}

#[test]
fn a_non_object_parameter_locks() {
    for text in [
        "function C(props: Props) {\n  return <Rect width={200} />;\n}\n",
        "function C(...args) {\n  return <Rect width={200} />;\n}\n",
    ] {
        assert_eq!(
            plan(text, "<Rect", &[("width", Some(number(200.0)))]),
            Err(LiftLock::NotObjectPattern {
                component: "C".to_string(),
            }),
            "{text}"
        );
    }

    let text = "const render = () => <Rect width={200} />;\n";
    assert_eq!(
        plan(text, "<Rect", &[("width", Some(number(200.0)))]),
        Err(LiftLock::NoComponent)
    );
}

#[test]
fn writes_the_identifier_by_literal_form() {
    // (element, prop, received, element after, default)
    let rows = [
        (
            "<Rect fill=\"red\" />",
            "fill",
            Some(string("red")),
            "<Rect fill={rectFill} />",
            "\"red\"",
        ),
        (
            "<Rect fill='red' />",
            "fill",
            Some(string("red")),
            "<Rect fill={rectFill} />",
            "'red'",
        ),
        (
            "<Rect x={-12.50} />",
            "x",
            Some(number(-12.5)),
            "<Rect x={rectX} />",
            "-12.50",
        ),
        (
            "<Rect visible={false} />",
            "visible",
            Some(EditValue::Bool(false)),
            "<Rect visible={rectVisible} />",
            "false",
        ),
        (
            "<Rect fill={'red'} />",
            "fill",
            Some(string("red")),
            "<Rect fill={rectFill} />",
            "'red'",
        ),
        (
            "<Rect fill={`red`} />",
            "fill",
            Some(string("red")),
            "<Rect fill={rectFill} />",
            "`red`",
        ),
        (
            "<Video loop />",
            "loop",
            Some(EditValue::Bool(true)),
            "<Video loop={videoLoop} />",
            "true",
        ),
        (
            "<Text>Sale</Text>",
            "children",
            Some(string("Sale")),
            "<Text>{textChildren}</Text>",
            "\"Sale\"",
        ),
        (
            "<Text>{'Sale'}</Text>",
            "children",
            Some(string("Sale")),
            "<Text>{textChildren}</Text>",
            "'Sale'",
        ),
        (
            "<Text></Text>",
            "children",
            None,
            "<Text>{textChildren}</Text>",
            "\"\"",
        ),
        (
            "<Rect width={200} />",
            "x",
            None,
            "<Rect width={200} x={rectX} />",
            "0",
        ),
    ];
    for (jsx, prop, received, after, default) in rows {
        let text = format!("function C() {{\n  return {jsx};\n}}\n");
        let tag = &jsx[..jsx.find([' ', '>']).unwrap()];
        let (out, params) = lifted(&text, tag, &[(prop, received)]);
        let name = &params[0];
        assert_eq!(
            out,
            format!("function C({{ {name} = {default} }}) {{\n  return {after};\n}}\n"),
            "{jsx}"
        );
    }

    // A `const` initializer becomes the identifier; the element still reads
    // the `const`.
    let text = "function C() {\n  const w = 200;\n  return <Rect width={w} />;\n}\n";
    assert_eq!(
        lift_one(text, "<Rect", "width", Some(number(200.0))),
        "function C({ rectWidth = 200 }) {\n  const w = rectWidth;\n  return <Rect width={w} />;\n}\n"
    );
}

#[test]
fn an_absent_prop_is_inserted_at_the_element() {
    let text = "interface Props {\n  label: string;\n}\n\nfunction C({ label }: Props) {\n  return <Rect width={200} />;\n}\n";
    assert_eq!(
        lift_one(text, "<Rect", "x", None),
        "interface Props {\n  label: string;\n  rectX?: number;\n}\n\nfunction C({ label, rectX = 0 }: Props) {\n  return <Rect width={200} x={rectX} />;\n}\n"
    );

    // No default is known: the parameter has none, and the field is typed by
    // the value written.
    let at = element(text, "<Rect");
    let target = resolve(text, &at, "fill", &ctx(None));
    assert_eq!(
        target,
        EditTarget::Missing {
            insert_at: at.end_of_attributes,
            default: None,
        }
    );
    let value = string("#f00");
    let lift = plan_lift(
        text,
        &at,
        &[LiftProp {
            prop: "fill",
            target: &target,
            value: &value,
        }],
    )
    .unwrap();
    let out = apply(text, &lift.edits).unwrap();
    assert!(parses(&out));
    assert_eq!(lift.params, ["rectFill"]);
    assert_eq!(
        out,
        "interface Props {\n  label: string;\n  rectFill?: string;\n}\n\nfunction C({ label, rectFill }: Props) {\n  return <Rect width={200} fill={rectFill} />;\n}\n"
    );
}

#[test]
fn taken_names_include_every_binding_and_read_in_the_file() {
    for (text, expected) in [
        (
            "function C({ label }) {\n  const rectWidth = 1;\n  return <Rect width={200} height={rectWidth} />;\n}\n",
            "function C({ label, rectWidth2 = 200 }) {\n  const rectWidth = 1;\n  return <Rect width={rectWidth2} height={rectWidth} />;\n}\n",
        ),
        (
            "import { rectWidth } from './sizes';\n\nfunction C({ label }) {\n  return <Rect width={200} />;\n}\n",
            "import { rectWidth } from './sizes';\n\nfunction C({ label, rectWidth2 = 200 }) {\n  return <Rect width={rectWidth2} />;\n}\n",
        ),
        (
            "function C({ items }) {\n  return items.map((rectWidth) => <Rect width={200} />);\n}\n",
            "function C({ items, rectWidth2 = 200 }) {\n  return items.map((rectWidth) => <Rect width={rectWidth2} />);\n}\n",
        ),
        (
            "function C({ rectWidth: w }) {\n  return <Rect width={200} height={w} />;\n}\n",
            "function C({ rectWidth: w, rectWidth2 = 200 }) {\n  return <Rect width={rectWidth2} height={w} />;\n}\n",
        ),
        (
            "function C({ label }: { label: string; rectWidth?: number }) {\n  return <Rect width={200} />;\n}\n",
            "function C({ label, rectWidth2 = 200 }: { label: string; rectWidth?: number; rectWidth2?: number }) {\n  return <Rect width={rectWidth2} />;\n}\n",
        ),
    ] {
        let (out, params) = lifted(text, "<Rect", &[("width", Some(number(200.0)))]);
        assert_eq!(params, ["rectWidth2"], "{text}");
        assert_eq!(out, expected);
    }
}

#[test]
fn a_move_lifts_x_and_y_together() {
    let text = "function C({ label }) {\n  return <Rect x={10} y={20} />;\n}\n";
    let (out, params) = lifted(
        text,
        "<Rect",
        &[("x", Some(number(10.0))), ("y", Some(number(20.0)))],
    );
    assert_eq!(params, ["rectX", "rectY"]);
    assert_eq!(
        out,
        "function C({ label, rectX = 10, rectY = 20 }) {\n  return <Rect x={rectX} y={rectY} />;\n}\n"
    );

    // Both absent, and no parameter yet: one pattern holds both.
    let text = "function C() {\n  return <Rect width={200} />;\n}\n";
    let (out, params) = lifted(text, "<Rect", &[("x", None), ("y", None)]);
    assert_eq!(params, ["rectX", "rectY"]);
    assert_eq!(
        out,
        "function C({ rectX = 0, rectY = 0 }) {\n  return <Rect width={200} x={rectX} y={rectY} />;\n}\n"
    );
}

#[test]
fn a_keyframe_is_not_a_literal() {
    let text = "function C() {\n  const frame = useCurrentFrame();\n  return <Rect x={interpolate(frame, [0, 10], [0, 100])} />;\n}\n";
    let at = element(text, "<Rect");
    let start = text.find("100").unwrap() as u32;
    let target = EditTarget::Keyframe {
        span: Span {
            start,
            end: start + 3,
        },
        value: 100.0,
        frame: 10.0,
    };
    let value = number(120.0);
    assert_eq!(
        plan_lift(
            text,
            &at,
            &[LiftProp {
                prop: "x",
                target: &target,
                value: &value,
            }],
        ),
        Err(LiftLock::NotALiteral)
    );

    // A `style` property is not lifted either.
    let text = "function C() {\n  return <Text style={{ fontSize: 12 }} />;\n}\n";
    assert_eq!(
        plan(text, "<Text", &[("style.fontSize", Some(number(12.0)))]),
        Err(LiftLock::NotALiteral)
    );
}

#[test]
fn parameter_default_finds_the_default() {
    let text = "function C({ rectWidth = 200 }) {\n  return <Rect width={rectWidth} />;\n}\n";
    let start = text.find("200").unwrap() as u32;
    assert_eq!(
        parameter_default(text, &element(text, "<Rect"), "rectWidth"),
        Some(DefaultTarget::Literal {
            span: Span {
                start,
                end: start + 3,
            },
            value: number(200.0),
            form: LiteralForm::Number,
        })
    );

    let text = "function C({ rectWidth: w = 200 }) {\n  return <Rect width={w} />;\n}\n";
    let start = text.find("200").unwrap() as u32;
    assert_eq!(
        parameter_default(text, &element(text, "<Rect"), "rectWidth"),
        Some(DefaultTarget::Literal {
            span: Span {
                start,
                end: start + 3,
            },
            value: number(200.0),
            form: LiteralForm::Number,
        })
    );

    let text = "function C({ rectWidth }) {\n  return <Rect width={rectWidth} />;\n}\n";
    let insert_at = (text.find("rectWidth").unwrap() + "rectWidth".len()) as u32;
    assert_eq!(
        parameter_default(text, &element(text, "<Rect"), "rectWidth"),
        Some(DefaultTarget::Absent { insert_at })
    );

    let text = "function C({ rectWidth = W * 2 }) {\n  return <Rect width={rectWidth} />;\n}\n";
    assert_eq!(
        parameter_default(text, &element(text, "<Rect"), "rectWidth"),
        Some(DefaultTarget::Expression)
    );

    let text = "function C(props) {\n  return <Rect width={props.rectWidth} />;\n}\n";
    assert_eq!(
        parameter_default(text, &element(text, "<Rect"), "rectWidth"),
        Some(DefaultTarget::NoPattern)
    );

    let text = "function C({ label }) {\n  return <Rect width={label} />;\n}\n";
    assert_eq!(
        parameter_default(text, &element(text, "<Rect"), "rectWidth"),
        None
    );
}
