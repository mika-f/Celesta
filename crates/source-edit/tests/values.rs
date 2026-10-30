use celesta_source_edit::{
    EditTarget, EditValue, LiteralForm, LockReason, ResolveContext, Span, apply, element_at,
    resolve, write_value,
};

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

/// Resolves `prop` on the first `<{tag}` in `text` (ASCII only).
fn run(text: &str, tag: &str, prop: &str, received: Option<EditValue>) -> EditTarget {
    let offset = text.find(&format!("<{tag}")).unwrap();
    let before = &text[..offset];
    let line = before.matches('\n').count() as u32 + 1;
    let column = (offset - before.rfind('\n').map_or(0, |i| i + 1)) as u32 + 1;
    let element = element_at(text, line, column).unwrap();
    resolve(text, &element, prop, &ctx(received))
}

fn component(body: &str) -> String {
    format!("export const A = () => {{\n{body}\n}};\n")
}

fn span_text(text: &str, span: Span) -> &str {
    &text[span.start as usize..span.end as usize]
}

/// `text` with `value` written to `target`.
fn write(text: &str, target: &EditTarget, prop: &str, value: EditValue) -> String {
    let edit = target
        .to_edit(prop, &value)
        .expect("the target is editable");
    apply(text, &[edit]).unwrap()
}

fn text_label(attr: &str) -> String {
    component(&format!("  return <Text {attr} />;"))
}

#[test]
fn string_attribute_keeps_its_quotes() {
    let text = text_label("label=\"Sale\"");
    let target = run(&text, "Text", "label", Some(string("Sale")));
    let EditTarget::Literal { span, value, form } = &target else {
        panic!("expected Literal, got {target:?}");
    };
    assert_eq!(
        (span_text(&text, *span), value, *form),
        ("\"Sale\"", &string("Sale"), LiteralForm::Quoted('"'))
    );
    assert_eq!(
        write(&text, &target, "label", string("New")),
        text_label("label=\"New\"")
    );
}

#[test]
fn single_quoted_attribute() {
    let text = text_label("label='Sale'");
    let target = run(&text, "Text", "label", Some(string("Sale")));
    assert!(
        matches!(
            target,
            EditTarget::Literal {
                form: LiteralForm::Quoted('\''),
                ..
            }
        ),
        "{target:?}"
    );
    assert_eq!(
        write(&text, &target, "label", string("New")),
        text_label("label='New'")
    );
}

#[test]
fn container_string_escapes_its_quote() {
    let text = text_label("label={'Sale'}");
    let target = run(&text, "Text", "label", Some(string("Sale")));
    let EditTarget::Literal { span, form, .. } = &target else {
        panic!("expected Literal, got {target:?}");
    };
    assert_eq!(
        (span_text(&text, *span), *form),
        ("'Sale'", LiteralForm::ContainerQuoted('\''))
    );
    assert_eq!(
        write(&text, &target, "label", string("It's")),
        text_label("label={'It\\'s'}")
    );
    // A backslash is escaped too, and a newline written as `\n`.
    assert_eq!(
        write(&text, &target, "label", string("a\\b\nc")),
        text_label("label={'a\\\\b\\nc'}")
    );
}

#[test]
fn quoted_attribute_with_its_quote_becomes_a_container() {
    let text = text_label("label=\"Sale\"");
    let target = run(&text, "Text", "label", Some(string("Sale")));
    assert_eq!(
        write(&text, &target, "label", string("Say \"hi\"")),
        text_label("label={\"Say \\\"hi\\\"\"}")
    );
    // The other quote needs no container.
    assert_eq!(
        write(&text, &target, "label", string("It's")),
        text_label("label=\"It's\"")
    );
    // `&` would start an entity in an attribute string.
    assert_eq!(
        write(&text, &target, "label", string("a &amp; b")),
        text_label("label={\"a &amp; b\"}")
    );
}

#[test]
fn newline_becomes_a_container() {
    let text = text_label("label=\"a\"");
    let target = run(&text, "Text", "label", Some(string("a")));
    assert_eq!(
        write(&text, &target, "label", string("a\nb")),
        text_label("label={\"a\\nb\"}")
    );
}

#[test]
fn template_without_substitutions() {
    let text = text_label("label={`Sale`}");
    let target = run(&text, "Text", "label", Some(string("Sale")));
    let EditTarget::Literal { span, form, .. } = &target else {
        panic!("expected Literal, got {target:?}");
    };
    assert_eq!(
        (span_text(&text, *span), *form),
        ("`Sale`", LiteralForm::Template)
    );
    assert_eq!(
        write(&text, &target, "label", string("x`y")),
        text_label("label={`x\\`y`}")
    );
    assert_eq!(
        write(&text, &target, "label", string("${n} \\")),
        text_label("label={`\\${n} \\\\`}")
    );
}

#[test]
fn template_with_substitutions_is_an_expression() {
    let text = text_label("label={`${n} off`}");
    assert_eq!(
        run(&text, "Text", "label", Some(string("3 off"))),
        EditTarget::Locked(LockReason::Expression {
            source: "`${n} off`".into()
        })
    );
}

#[test]
fn bare_boolean_written_false() {
    let text = component("  return <Video loop />;");
    let target = run(&text, "Video", "loop", Some(EditValue::Bool(true)));
    let EditTarget::Literal { span, value, form } = &target else {
        panic!("expected Literal, got {target:?}");
    };
    let after_name = (text.find("loop").unwrap() + "loop".len()) as u32;
    assert_eq!(
        (*span, value, *form),
        (
            Span {
                start: after_name,
                end: after_name
            },
            &EditValue::Bool(true),
            LiteralForm::BareTrue
        )
    );
    assert_eq!(
        write(&text, &target, "loop", EditValue::Bool(false)),
        component("  return <Video loop={false} />;")
    );
    assert_eq!(write(&text, &target, "loop", EditValue::Bool(true)), text);
}

#[test]
fn bool_container() {
    let text = component("  return <Video loop={false} />;");
    let target = run(&text, "Video", "loop", Some(EditValue::Bool(false)));
    let EditTarget::Literal { span, value, form } = &target else {
        panic!("expected Literal, got {target:?}");
    };
    assert_eq!(
        (span_text(&text, *span), value, *form),
        ("false", &EditValue::Bool(false), LiteralForm::Bool)
    );
    assert_eq!(
        write(&text, &target, "loop", EditValue::Bool(true)),
        component("  return <Video loop={true} />;")
    );
}

#[test]
fn const_string_resolves_and_shares() {
    let text = component("  const L = 'a';\n  return <Text label={L} />;");
    let target = run(&text, "Text", "label", Some(string("a")));
    let EditTarget::Literal { span, value, form } = &target else {
        panic!("expected Literal, got {target:?}");
    };
    assert_eq!(
        (span_text(&text, *span), value, *form),
        ("'a'", &string("a"), LiteralForm::ContainerQuoted('\''))
    );
    assert_eq!(
        write(&text, &target, "label", string("b")),
        component("  const L = 'b';\n  return <Text label={L} />;")
    );

    let text = component(
        "  const L = 'a';\n  return <Group><Text label={L} /><Text label={L} /></Group>;",
    );
    assert_eq!(
        run(&text, "Text", "label", Some(string("a"))),
        EditTarget::Locked(LockReason::SharedBinding {
            name: "L".into(),
            uses: 2
        })
    );
}

#[test]
fn insert_string_and_bool() {
    let text = component("  return <Video src=\"a.mp4\" />;");
    let at = text.find(" />").unwrap() as u32;
    let target = run(&text, "Video", "label", None);
    assert_eq!(
        target,
        EditTarget::Missing {
            insert_at: at,
            default: None
        }
    );
    assert_eq!(
        write(&text, &target, "label", string("a")),
        component("  return <Video src=\"a.mp4\" label=\"a\" />;")
    );
    assert_eq!(
        write(&text, &target, "label", string("Say \"hi\"\n")),
        component("  return <Video src=\"a.mp4\" label={\"Say \\\"hi\\\"\\n\"} />;")
    );
    // `&` would start an entity in an attribute string.
    assert_eq!(
        write(&text, &target, "label", string("a & b")),
        component("  return <Video src=\"a.mp4\" label={\"a & b\"} />;")
    );
    assert_eq!(
        write(&text, &target, "loop", EditValue::Bool(false)),
        component("  return <Video src=\"a.mp4\" loop={false} />;")
    );
    assert_eq!(
        write(&text, &target, "loop", EditValue::Bool(true)),
        component("  return <Video src=\"a.mp4\" loop />;")
    );
    assert_eq!(
        write(&text, &target, "volume", EditValue::Number(0.5)),
        component("  return <Video src=\"a.mp4\" volume={0.5} />;")
    );
}

#[test]
fn received_string_mismatch_is_overridden() {
    let text = text_label("label=\"a\"");
    assert_eq!(
        run(&text, "Text", "label", Some(string("b"))),
        EditTarget::Locked(LockReason::OverriddenByParent)
    );
    let text = component("  return <Video loop={false} />;");
    assert_eq!(
        run(&text, "Video", "loop", Some(EditValue::Bool(true))),
        EditTarget::Locked(LockReason::OverriddenByParent)
    );
}

#[test]
fn a_literal_of_another_kind_than_received_is_an_expression() {
    let text = text_label("label={1}");
    assert_eq!(
        run(&text, "Text", "label", Some(string("1"))),
        EditTarget::Locked(LockReason::Expression { source: "1".into() })
    );
    let number = Some(EditValue::Number(12.0));
    for (attr, source) in [("x=\"12\"", "\"12\""), ("x={true}", "true"), ("x", "x")] {
        let text = component(&format!("  return <Rect {attr} />;"));
        assert_eq!(
            run(&text, "Rect", "x", number.clone()),
            EditTarget::Locked(LockReason::Expression {
                source: source.into()
            }),
            "{attr}"
        );
    }
}

#[test]
fn a_lone_surrogate_is_an_expression() {
    // A Rust string cannot hold the value, so it cannot be written back.
    for literal in ["\"\\uD800\"", "`\\uD800`"] {
        let text = text_label(&format!("label={{{literal}}}"));
        assert_eq!(
            run(&text, "Text", "label", Some(string("\u{FFFD}"))),
            EditTarget::Locked(LockReason::Expression {
                source: literal.into()
            }),
            "{literal}"
        );
    }
}

#[test]
fn attribute_string_with_an_entity_is_an_expression() {
    // oxc does not decode `&amp;`, so the value read is not the one received.
    let text = text_label("label=\"a &amp; b\"");
    assert_eq!(
        run(&text, "Text", "label", Some(string("a & b"))),
        EditTarget::Locked(LockReason::Expression {
            source: "\"a &amp; b\"".into()
        })
    );
}

#[test]
fn a_number_written_into_an_attribute_string_gets_a_container() {
    let text = text_label("label=\"a\"");
    let target = run(&text, "Text", "label", Some(string("a")));
    assert_eq!(
        write(&text, &target, "label", EditValue::Number(12.0)),
        text_label("label={12}")
    );
}

#[test]
fn a_keyframe_takes_only_numbers() {
    let text = component(
        "  const frame = useCurrentFrame();\n  return <Rect x={interpolate(frame, [0, 30], [100, 400])} />;",
    );
    let target = run(&text, "Rect", "x", Some(EditValue::Number(100.0)));
    assert!(matches!(target, EditTarget::Keyframe { .. }), "{target:?}");
    assert!(target.to_edit("x", &string("a")).is_none());
    assert!(target.to_edit("x", &EditValue::Bool(true)).is_none());
}

fn text_children(children: &str) -> String {
    component(&format!("  return <Text>{children}</Text>;"))
}

#[test]
fn jsx_text_child() {
    let text = text_children("Sale now");
    let target = run(&text, "Text", "children", Some(string("Sale now")));
    let EditTarget::Literal { span, value, form } = &target else {
        panic!("expected Literal, got {target:?}");
    };
    assert_eq!(
        (span_text(&text, *span), value, *form),
        ("Sale now", &string("Sale now"), LiteralForm::JsxText)
    );
    assert_eq!(
        write(&text, &target, "children", string("Big sale")),
        text_children("Big sale")
    );
}

#[test]
fn multiline_jsx_text_collapses() {
    let text = text_children("\n  Sale\n  now\n");
    let target = run(&text, "Text", "children", Some(string("Sale now")));
    let EditTarget::Literal { span, value, form } = &target else {
        panic!("expected Literal, got {target:?}");
    };
    // The whole children span is replaced.
    assert_eq!(
        (span_text(&text, *span), value, *form),
        (
            "\n  Sale\n  now\n",
            &string("Sale now"),
            LiteralForm::JsxText
        )
    );
    assert_eq!(
        write(&text, &target, "children", string("Big sale")),
        text_children("Big sale")
    );
    // Blank lines are dropped; a line's inner spaces stay.
    let text = text_children("\n  Sale  now\n\n  today\n");
    assert!(matches!(
        run(&text, "Text", "children", Some(string("Sale  now today"))),
        EditTarget::Literal { .. }
    ));
}

#[test]
fn jsx_text_needing_braces() {
    let text = text_children("Sale");
    let target = run(&text, "Text", "children", Some(string("Sale")));
    assert_eq!(
        write(&text, &target, "children", string("a {b}")),
        text_children("{\"a {b}\"}")
    );
    for (value, written) in [
        ("a < b", "{\"a < b\"}"),
        (" a", "{\" a\"}"),
        ("a ", "{\"a \"}"),
        ("a\nb", "{\"a\\nb\"}"),
        // A tab reads as a space, `&` starts an entity.
        ("a\tb", "{\"a\\tb\"}"),
        ("a & b", "{\"a & b\"}"),
    ] {
        assert_eq!(
            write(&text, &target, "children", string(value)),
            text_children(written),
            "{value:?}"
        );
    }
}

#[test]
fn string_child_container() {
    let text = text_children("{'Sale'}");
    let target = run(&text, "Text", "children", Some(string("Sale")));
    let EditTarget::Literal { span, value, form } = &target else {
        panic!("expected Literal, got {target:?}");
    };
    assert_eq!(
        (span_text(&text, *span), value, *form),
        (
            "{'Sale'}",
            &string("Sale"),
            LiteralForm::ChildContainer('\'')
        )
    );
    assert_eq!(
        write(&text, &target, "children", string("x")),
        text_children("{'x'}")
    );
    assert_eq!(
        write(&text, &target, "children", string("It's")),
        text_children("{'It\\'s'}")
    );
    // Lines of only whitespace around it render nothing.
    let text = text_children("\n    {\"Sale\"}\n  ");
    let target = run(&text, "Text", "children", Some(string("Sale")));
    assert!(
        matches!(
            target,
            EditTarget::Literal {
                form: LiteralForm::ChildContainer('"'),
                ..
            }
        ),
        "{target:?}"
    );
    assert_eq!(
        write(&text, &target, "children", string("x")),
        text_children("{\"x\"}")
    );
}

#[test]
fn span_children_are_locked() {
    for children in ["a<Span>b</Span>", "{label + 'b'}", "a{'b'}", "a &amp; b"] {
        let text = text_children(children);
        assert_eq!(
            run(&text, "Text", "children", Some(string("ab"))),
            EditTarget::Locked(LockReason::Expression {
                source: children.into()
            }),
            "{children}"
        );
    }
}

#[test]
fn identifier_children_follow_the_identifier_rules() {
    // A const string: the initializer is the literal.
    let text = component("  const LABEL = 'Sale';\n  return <Text>{LABEL}</Text>;");
    let target = run(&text, "Text", "children", Some(string("Sale")));
    let EditTarget::Literal { span, value, form } = &target else {
        panic!("expected Literal, got {target:?}");
    };
    assert_eq!(span_text(&text, *span), "'Sale'");
    assert_eq!(value, &string("Sale"));
    assert_eq!(*form, LiteralForm::ContainerQuoted('\''));
    assert_eq!(
        write(&text, &target, "children", string("Big")),
        component("  const LABEL = 'Big';\n  return <Text>{LABEL}</Text>;")
    );

    // The received check runs on the const's value.
    assert_eq!(
        run(&text, "Text", "children", Some(string("Other"))),
        EditTarget::Locked(LockReason::OverriddenByParent)
    );

    // A binding that can't be traced, and a member read.
    let text = text_children("{label}");
    assert_eq!(
        run(&text, "Text", "children", Some(string("Sale"))),
        EditTarget::Locked(LockReason::UnresolvedIdentifier {
            name: "label".into()
        })
    );
    let text = text_children("{\n    C.label\n  }");
    assert_eq!(
        run(&text, "Text", "children", Some(string("Sale"))),
        EditTarget::Locked(LockReason::Expression {
            source: "C.label".into()
        })
    );
}

#[test]
fn a_children_kind_mismatch_reports_the_trimmed_source() {
    for children in ["\n    12\n  ", "\n    {'12'}\n  "] {
        let text = text_children(children);
        assert_eq!(
            run(&text, "Text", "children", Some(EditValue::Number(12.0))),
            EditTarget::Locked(LockReason::Expression {
                source: children.trim().into()
            }),
            "{children:?}"
        );
    }
}

#[test]
fn empty_children_insert_text() {
    let text = text_children("");
    let target = run(&text, "Text", "children", None);
    assert_eq!(
        target,
        EditTarget::Missing {
            insert_at: (text.find("<Text>").unwrap() + "<Text>".len()) as u32,
            default: Some(string(""))
        }
    );
    assert_eq!(
        write(&text, &target, "children", string("Sale")),
        text_children("Sale")
    );
    assert_eq!(
        write(&text, &target, "children", string("a {b}")),
        text_children("{\"a {b}\"}")
    );

    // 2b opens the tag first.
    let text = component("  return <Text color=\"#fff\" />;");
    assert_eq!(
        run(&text, "Text", "children", None),
        EditTarget::Locked(LockReason::Expression {
            source: "/>".into()
        })
    );
}

#[test]
fn a_children_attribute_without_jsx_children() {
    let text = component("  return <Text children=\"Sale\" />;");
    let target = run(&text, "Text", "children", Some(string("Sale")));
    assert!(
        matches!(
            target,
            EditTarget::Literal {
                form: LiteralForm::Quoted('"'),
                ..
            }
        ),
        "{target:?}"
    );
}

#[test]
fn element_ref_has_the_children_span() {
    let text = text_children("Sale");
    let offset = text.find("<Text").unwrap() as u32;
    let element = element_at(&text, 2, 10).unwrap();
    assert_eq!(element.start, offset);
    assert_eq!(span_text(&text, element.children.unwrap()), "Sale");

    let text = component("  return <Text />;");
    assert_eq!(element_at(&text, 2, 10).unwrap().children, None);
}

fn styled(style: &str) -> String {
    text_label(&format!("style={style}"))
}

#[test]
fn style_number_property() {
    let text = styled("{{ fontSize: 48 }}");
    let target = run(
        &text,
        "Text",
        "style.fontSize",
        Some(EditValue::Number(48.0)),
    );
    let EditTarget::Literal { span, value, form } = &target else {
        panic!("expected Literal, got {target:?}");
    };
    assert_eq!(
        (span_text(&text, *span), value, *form),
        ("48", &EditValue::Number(48.0), LiteralForm::Number)
    );
    assert_eq!(
        write(&text, &target, "style.fontSize", EditValue::Number(64.0)),
        styled("{{ fontSize: 64 }}")
    );
}

#[test]
fn style_string_property_and_keys() {
    let text = styled("{{ 'color': '#fff', fontSize: 48 }}");
    let target = run(&text, "Text", "style.color", Some(string("#fff")));
    assert_eq!(
        write(&text, &target, "style.color", string("#000")),
        styled("{{ 'color': '#000', fontSize: 48 }}")
    );
    // A later key wins, as at runtime; a spread before it does not matter.
    let text = styled("{{ ...base, fontSize: 1, fontSize: 48 }}");
    let target = run(
        &text,
        "Text",
        "style.fontSize",
        Some(EditValue::Number(48.0)),
    );
    let EditTarget::Literal { span, .. } = target else {
        panic!("expected Literal, got {target:?}");
    };
    assert_eq!(span_text(&text, span), "48");
}

#[test]
fn style_const_property() {
    let text = component("  const SIZE = 48;\n  return <Text style={{ fontSize: SIZE }} />;");
    let target = run(
        &text,
        "Text",
        "style.fontSize",
        Some(EditValue::Number(48.0)),
    );
    let EditTarget::Literal { span, .. } = target else {
        panic!("expected Literal, got {target:?}");
    };
    assert_eq!(span_text(&text, span), "48");
}

#[test]
fn style_missing_property_inserts() {
    let text = styled("{{ fontSize: 48 }}");
    let target = run(&text, "Text", "style.color", None);
    assert_eq!(
        target,
        EditTarget::Missing {
            insert_at: (text.find("48").unwrap() + 2) as u32,
            default: None
        }
    );
    assert_eq!(
        write(&text, &target, "style.color", string("#fff")),
        styled("{{ fontSize: 48, color: \"#fff\" }}")
    );

    for (before, after) in [
        // After a trailing comma, even past a comment.
        ("{{ fontSize: 48, }}", "{{ fontSize: 48, color: \"#fff\" }}"),
        (
            "{{\n  fontSize: 48 /* big, */ ,\n}}",
            "{{\n  fontSize: 48 /* big, */ , color: \"#fff\"\n}}",
        ),
        // Before a comment that follows the last property.
        (
            "{{ fontSize: 48 // big\n}}",
            "{{ fontSize: 48, color: \"#fff\" // big\n}}",
        ),
        ("{{}}", "{{color: \"#fff\"}}"),
    ] {
        let text = styled(before);
        let target = run(&text, "Text", "style.color", None);
        assert_eq!(
            write(&text, &target, "style.color", string("#fff")),
            styled(after),
            "{before}"
        );
    }

    let text = styled("{{}}");
    let target = run(&text, "Text", "style.fontSize", None);
    assert_eq!(
        write(&text, &target, "style.fontSize", EditValue::Number(48.0)),
        styled("{{fontSize: 48}}")
    );
    assert_eq!(
        write(&text, &target, "style.font-size", EditValue::Bool(true)),
        styled("{{\"font-size\": true}}")
    );
}

#[test]
fn style_call_is_locked() {
    let text = styled("{mono(20)}");
    assert_eq!(
        run(
            &text,
            "Text",
            "style.fontSize",
            Some(EditValue::Number(20.0))
        ),
        EditTarget::Locked(LockReason::Expression {
            source: "mono(20)".into()
        })
    );
}

#[test]
fn style_spreads_lock() {
    for style in [
        "{{ ...base }}",
        "{{ fontSize: 48, ...base }}",
        "{{ [key]: 48 }}",
    ] {
        let text = styled(style);
        assert_eq!(
            run(
                &text,
                "Text",
                "style.fontSize",
                Some(EditValue::Number(48.0))
            ),
            EditTarget::Locked(LockReason::Spread),
            "{style}"
        );
    }
    let text = text_label("style={{ fontSize: 48 }} {...rest}");
    assert_eq!(
        run(
            &text,
            "Text",
            "style.fontSize",
            Some(EditValue::Number(48.0))
        ),
        EditTarget::Locked(LockReason::Spread)
    );
}

#[test]
fn two_style_properties_inserted_at_one_offset() {
    for (before, after) in [
        ("{{}}", "{{fontSize: 48, color: \"#fff\"}}"),
        (
            "{{ fontWeight: 700, }}",
            "{{ fontWeight: 700, fontSize: 48, color: \"#fff\" }}",
        ),
        (
            "{{ fontWeight: 700 }}",
            "{{ fontWeight: 700, fontSize: 48, color: \"#fff\" }}",
        ),
    ] {
        let text = styled(before);
        let size = run(&text, "Text", "style.fontSize", None)
            .to_edit("style.fontSize", &EditValue::Number(48.0))
            .unwrap();
        let color = run(&text, "Text", "style.color", None)
            .to_edit("style.color", &string("#fff"))
            .unwrap();
        assert_eq!(
            apply(&text, &[size, color]).unwrap(),
            styled(after),
            "{before}"
        );
    }
}

#[test]
fn style_without_a_style_attribute() {
    let text = text_label("fontSize={48}");
    assert_eq!(
        run(&text, "Text", "style.fontSize", None),
        EditTarget::Locked(LockReason::NoStyleObject)
    );
}

#[test]
fn a_value_of_another_kind_still_parses() {
    let number = EditValue::Number(12.0);
    assert_eq!(write_value(&number, LiteralForm::Quoted('\'')), "{12}");
    assert_eq!(write_value(&number, LiteralForm::BareTrue), "={12}");
    assert_eq!(write_value(&number, LiteralForm::Template), "12");
    assert_eq!(write_value(&string("a"), LiteralForm::Number), "\"a\"");
    assert_eq!(
        write_value(&EditValue::Bool(false), LiteralForm::Quoted('"')),
        "{false}"
    );
    assert_eq!(write_value(&string("a"), LiteralForm::BareTrue), "=\"a\"");
    assert_eq!(write_value(&number, LiteralForm::JsxText), "{12}");
    assert_eq!(
        write_value(&number, LiteralForm::ChildContainer('\'')),
        "{12}"
    );
}
