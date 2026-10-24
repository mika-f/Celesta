use celesta_source_edit::{
    EditTarget, EditValue, ElementRef, LiteralForm, LockReason, Parameter, ResolveContext, Span,
    apply, element_at, enclosing_component, parameter_of, resolve_through,
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

fn span_text(text: &str, span: Span) -> &str {
    &text[span.start as usize..span.end as usize]
}

/// A scene calling the component once, as `call`.
fn scene(call: &str) -> String {
    format!("export const Scene = () => (\n  {call}\n);\n")
}

/// Resolves `prop` on the first `<{tag}` of `component` through the first
/// `<{caller}` of `scene`.
fn through(
    component: &str,
    tag: &str,
    prop: &str,
    scene: &str,
    caller: &str,
    ctx: &ResolveContext,
) -> EditTarget {
    let call_site = element(scene, &format!("<{caller}"));
    resolve_through(
        component,
        &element(component, &format!("<{tag}")),
        prop,
        ctx,
        Some((scene, &call_site)),
    )
}

/// `target` as a pass-through of `param` at the first `<{caller}` of
/// `scene`; returns the inner target.
fn passed<'t>(target: &'t EditTarget, scene: &str, caller: &str, param: &str) -> &'t EditTarget {
    let EditTarget::PassThrough {
        call_site,
        param: found,
        target,
    } = target
    else {
        panic!("expected PassThrough, got {target:?}");
    };
    assert_eq!(*call_site, element(scene, &format!("<{caller}")));
    assert_eq!(found, param);
    target
}

const PRICE_TAG: &str =
    "export function PriceTag({ label }) {\n  return <Text label={label} />;\n}\n";

#[test]
fn enclosing_component_names() {
    for text in [
        "export function PriceTag({ label }) {\n  return <Text label={label} />;\n}\n",
        "export const PriceTag = ({ label }) => <Text label={label} />;\n",
        "export const PriceTag = function ({ label }) {\n  return <Text label={label} />;\n};\n",
        "export const PriceTag = memo(({ label }) => <Text label={label} />);\n",
        "export const PriceTag = React.forwardRef((props, ref) => <Text label={props.label} />);\n",
        "export const PriceTag = memo(forwardRef((props, ref) => <Text label={props.label} />));\n",
        "export function PriceTag({ label }: Props) {\n  return <Text label={label} />;\n}\n",
        // A function inside the component that no call site can name.
        "export function PriceTag({ label }) {\n  const row = () => <Text label={label} />;\n  return row();\n}\n",
    ] {
        let text_element = element(text, "<Text");
        assert_eq!(
            enclosing_component(text, &text_element),
            Some("PriceTag".to_string()),
            "{text}"
        );
        assert_eq!(
            parameter_of(text, &text_element, "label").map(|p| (p.component, p.param)),
            Some(("PriceTag".to_string(), "label".to_string())),
            "{text}"
        );
    }

    // A function expression's own name is what the runtime reports.
    let text = "export const PriceTag = memo(function Tag({ label }) {\n  return <Text label={label} />;\n});\n";
    let text_element = element(text, "<Text");
    assert_eq!(
        enclosing_component(text, &text_element),
        Some("Tag".to_string())
    );
    assert_eq!(
        parameter_of(text, &text_element, "label").map(|p| p.component),
        Some("Tag".to_string())
    );

    // A component inside another: its call site never passes the outer
    // component's parameter.
    let text = "export function Outer({ label }) {\n  const Inner = () => <Text label={label} />;\n  return <Inner />;\n}\n";
    let text_element = element(text, "<Text");
    assert_eq!(
        enclosing_component(text, &text_element),
        Some("Inner".to_string())
    );
    assert_eq!(parameter_of(text, &text_element, "label"), None);
    let call_site = element(text, "<Inner");
    assert_eq!(
        resolve_through(
            text,
            &text_element,
            "label",
            &ctx(Some(string("Sale"))),
            Some((text, &call_site)),
        ),
        EditTarget::Locked(LockReason::UnresolvedIdentifier {
            name: "label".into()
        })
    );
    for text in [
        "export default ({ label }) => <Text label={label} />;\n",
        "export default function ({ label }) {\n  return <Text label={label} />;\n}\n",
        "<Text label=\"Sale\" />;\n",
    ] {
        assert_eq!(
            enclosing_component(text, &element(text, "<Text")),
            None,
            "{text}"
        );
    }
}

#[test]
fn literal_at_the_call_site() {
    let scene = scene("<PriceTag label=\"Sale\" />");
    let target = through(
        PRICE_TAG,
        "Text",
        "label",
        &scene,
        "PriceTag",
        &ctx(Some(string("Sale"))),
    );
    let EditTarget::Literal { span, value, form } = passed(&target, &scene, "PriceTag", "label")
    else {
        panic!("expected Literal, got {target:?}");
    };
    assert_eq!(span_text(&scene, *span), "\"Sale\"");
    assert_eq!(value, &string("Sale"));
    assert_eq!(*form, LiteralForm::Quoted('"'));

    // The received check runs on the call site's literal.
    let target = through(
        PRICE_TAG,
        "Text",
        "label",
        &scene,
        "PriceTag",
        &ctx(Some(string("Other"))),
    );
    assert_eq!(
        passed(&target, &scene, "PriceTag", "label"),
        &EditTarget::Locked(LockReason::OverriddenByParent)
    );
}

#[test]
fn missing_with_default() {
    let component = "export function Tag({ x = 40 }) {\n  return <Rect x={x} />;\n}\n";
    let scene = scene("<Tag />");
    let missing = EditTarget::Missing {
        insert_at: (scene.find("<Tag").unwrap() + "<Tag".len()) as u32,
        default: Some(EditValue::Number(40.0)),
    };
    let target = through(
        component,
        "Rect",
        "x",
        &scene,
        "Tag",
        &ctx(Some(EditValue::Number(40.0))),
    );
    assert_eq!(passed(&target, &scene, "Tag", "x"), &missing);

    // The primitive received something else than the default.
    let target = through(
        component,
        "Rect",
        "x",
        &scene,
        "Tag",
        &ctx(Some(EditValue::Number(12.0))),
    );
    assert_eq!(
        passed(&target, &scene, "Tag", "x"),
        &EditTarget::Locked(LockReason::OverriddenByParent)
    );

    // `parameter_of` reports the default.
    assert_eq!(
        parameter_of(component, &element(component, "<Rect"), "x"),
        Some(Parameter {
            component: "Tag".into(),
            param: "x".into(),
            default: Some(EditValue::Number(40.0)),
        })
    );
}

#[test]
fn missing_without_a_literal_default() {
    let scene = scene("<Tag />");
    let insert_at = (scene.find("<Tag").unwrap() + "<Tag".len()) as u32;

    // No default: the primitive receives nothing.
    let component = "export function Tag({ x }) {\n  return <Rect x={x} />;\n}\n";
    let target = through(component, "Rect", "x", &scene, "Tag", &ctx(None));
    assert_eq!(
        passed(&target, &scene, "Tag", "x"),
        &EditTarget::Missing {
            insert_at,
            default: None
        }
    );
    let target = through(
        component,
        "Rect",
        "x",
        &scene,
        "Tag",
        &ctx(Some(EditValue::Number(0.0))),
    );
    assert_eq!(
        passed(&target, &scene, "Tag", "x"),
        &EditTarget::Locked(LockReason::OverriddenByParent)
    );

    // A default that is not a literal: its value is unknown, so the
    // received value is not checked.
    let component = "export function Tag({ x = SIZE * 2 }) {\n  return <Rect x={x} />;\n}\n";
    let target = through(
        component,
        "Rect",
        "x",
        &scene,
        "Tag",
        &ctx(Some(EditValue::Number(80.0))),
    );
    assert_eq!(
        passed(&target, &scene, "Tag", "x"),
        &EditTarget::Missing {
            insert_at,
            default: None
        }
    );
}

#[test]
fn renamed_destructuring() {
    let component =
        "export function PriceTag({ label: text }) {\n  return <Text label={text} />;\n}\n";
    let scene = scene("<PriceTag label={'Sale'} />");
    let target = through(
        component,
        "Text",
        "label",
        &scene,
        "PriceTag",
        &ctx(Some(string("Sale"))),
    );
    assert!(matches!(
        passed(&target, &scene, "PriceTag", "label"),
        EditTarget::Literal { .. }
    ));
}

#[test]
fn props_member() {
    let component = "export const PriceTag = (props) => <Text label={props.label} />;\n";
    let scene = scene("<PriceTag label=\"Sale\" />");
    let target = through(
        component,
        "Text",
        "label",
        &scene,
        "PriceTag",
        &ctx(Some(string("Sale"))),
    );
    assert!(matches!(
        passed(&target, &scene, "PriceTag", "label"),
        EditTarget::Literal { .. }
    ));

    // Only a parameter named `props` is followed.
    let component = "export const PriceTag = (p) => <Text label={p.label} />;\n";
    assert_eq!(
        parameter_of(component, &element(component, "<Text"), "label"),
        None
    );
    assert_eq!(
        through(
            component,
            "Text",
            "label",
            &scene,
            "PriceTag",
            &ctx(Some(string("Sale")))
        ),
        EditTarget::Locked(LockReason::Expression {
            source: "p.label".into()
        })
    );
}

#[test]
fn second_hop_is_locked() {
    let card = "export function Card({ label }) {\n  return <PriceTag label={label} />;\n}\n";
    let target = through(
        PRICE_TAG,
        "Text",
        "label",
        card,
        "PriceTag",
        &ctx(Some(string("Sale"))),
    );
    assert_eq!(
        target,
        EditTarget::Locked(LockReason::ParameterOfCaller {
            component: "Card".into()
        })
    );
}

#[test]
fn spread_at_the_call_site() {
    let scene = scene("<PriceTag {...rest} />");
    let target = through(
        PRICE_TAG,
        "Text",
        "label",
        &scene,
        "PriceTag",
        &ctx(Some(string("Sale"))),
    );
    assert_eq!(
        passed(&target, &scene, "PriceTag", "label"),
        &EditTarget::Locked(LockReason::Spread)
    );
}

#[test]
fn no_call_site() {
    let target = resolve_through(
        PRICE_TAG,
        &element(PRICE_TAG, "<Text"),
        "label",
        &ctx(Some(string("Sale"))),
        None,
    );
    assert_eq!(
        target,
        EditTarget::Locked(LockReason::CallSiteNotFound {
            component: "PriceTag".into()
        })
    );
}

#[test]
fn not_a_parameter_resolves_in_place() {
    // An anonymous component's parameter can't be called by name.
    let component = "export default ({ label }) => <Text label={label} />;\n";
    assert_eq!(
        parameter_of(component, &element(component, "<Text"), "label"),
        None
    );
    assert_eq!(
        resolve_through(
            component,
            &element(component, "<Text"),
            "label",
            &ctx(Some(string("Sale"))),
            None,
        ),
        EditTarget::Locked(LockReason::UnresolvedIdentifier {
            name: "label".into()
        })
    );

    // A literal on the element itself.
    let component = "export function PriceTag() {\n  return <Text label=\"Sale\" />;\n}\n";
    let scene = scene("<PriceTag />");
    assert!(matches!(
        through(
            component,
            "Text",
            "label",
            &scene,
            "PriceTag",
            &ctx(Some(string("Sale")))
        ),
        EditTarget::Literal { .. }
    ));

    // A second parameter, or a nested pattern.
    for component in [
        "export function PriceTag(a, { label }) {\n  return <Text label={label} />;\n}\n",
        "export function PriceTag({ tag: { label } }) {\n  return <Text label={label} />;\n}\n",
    ] {
        assert_eq!(
            parameter_of(component, &element(component, "<Text"), "label"),
            None,
            "{component}"
        );
    }
}

#[test]
fn a_reused_component_still_passes_through() {
    // Two call sites render the inner element twice; the edit changes only
    // the given call site.
    let scene = "export const Scene = () => (\n  <>\n    <PriceTag label=\"A\" />\n    <PriceTag label=\"B\" />\n  </>\n);\n";
    let second = element(scene, "<PriceTag label=\"B\"");
    let c = ResolveContext {
        render_count: 2,
        ..ctx(Some(string("B")))
    };
    let target = resolve_through(
        PRICE_TAG,
        &element(PRICE_TAG, "<Text"),
        "label",
        &c,
        Some((scene, &second)),
    );
    let EditTarget::PassThrough {
        call_site, target, ..
    } = &target
    else {
        panic!("expected PassThrough, got {target:?}");
    };
    assert_eq!(*call_site, second);
    let EditTarget::Literal { span, .. } = &**target else {
        panic!("expected Literal, got {target:?}");
    };
    assert_eq!(span.start as usize, scene.find("\"B\"").unwrap());

    // A value written on the element stays locked as repeated.
    let component = "export function PriceTag() {\n  return <Text label=\"Sale\" />;\n}\n";
    assert_eq!(
        resolve_through(
            component,
            &element(component, "<Text"),
            "label",
            &c,
            Some((scene, &second)),
        ),
        EditTarget::Locked(LockReason::Repeated { count: 2 })
    );
}

#[test]
fn identifier_children_pass_through() {
    let component = "export function PriceTag({ label }) {\n  return <Text>{label}</Text>;\n}\n";
    let scene = scene("<PriceTag label=\"Sale\" />");
    let target = through(
        component,
        "Text",
        "children",
        &scene,
        "PriceTag",
        &ctx(Some(string("Sale"))),
    );
    let EditTarget::Literal { span, .. } = passed(&target, &scene, "PriceTag", "label") else {
        panic!("expected Literal, got {target:?}");
    };
    assert_eq!(span_text(&scene, *span), "\"Sale\"");

    // A `children` parameter reads the call site's children.
    let component =
        "export function PriceTag({ children }) {\n  return <Text>{children}</Text>;\n}\n";
    let scene = self::scene("<PriceTag>Sale</PriceTag>");
    let target = through(
        component,
        "Text",
        "children",
        &scene,
        "PriceTag",
        &ctx(Some(string("Sale"))),
    );
    let EditTarget::Literal { span, form, .. } = passed(&target, &scene, "PriceTag", "children")
    else {
        panic!("expected Literal, got {target:?}");
    };
    assert_eq!(span_text(&scene, *span), "Sale");
    assert_eq!(*form, LiteralForm::JsxText);
}

#[test]
fn write_through_inserts_at_the_call_site() {
    let component = "export function Tag({ x = 40 }) {\n  return <Rect x={x} />;\n}\n";
    let scene = scene("<Tag />");
    let target = through(
        component,
        "Rect",
        "x",
        &scene,
        "Tag",
        &ctx(Some(EditValue::Number(40.0))),
    );
    let edit = target.to_edit("x", &EditValue::Number(12.0)).unwrap();
    assert_eq!(
        apply(&scene, &[edit]).unwrap(),
        self::scene("<Tag x={12} />")
    );

    // The call site's prop is the parameter, whatever the element's prop.
    let component = "export function Label({ size }) {\n  return <Text style={{ fontSize: size }}>Hi</Text>;\n}\n";
    let scene = self::scene("<Label />");
    let target = through(
        component,
        "Text",
        "style.fontSize",
        &scene,
        "Label",
        &ctx(None),
    );
    let edit = target
        .to_edit("style.fontSize", &EditValue::Number(48.0))
        .unwrap();
    assert_eq!(
        apply(&scene, &[edit]).unwrap(),
        self::scene("<Label size={48} />")
    );
}
