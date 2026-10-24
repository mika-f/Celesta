use celesta_source_edit::{
    Edit, EditTarget, EditValue, ElementRef, LiteralForm, LockReason, ResolveContext, Span, apply,
    element_at, has_attribute, resolve,
};

fn ctx() -> ResolveContext {
    ResolveContext {
        primitive: true,
        component_props: false,
        candidate_frames: vec![0.0],
        received: None,
        render_count: 1,
    }
}

fn with_received(received: f64) -> ResolveContext {
    ResolveContext {
        received: Some(EditValue::Number(received)),
        ..ctx()
    }
}

/// The numeric literal `target` points at, as `(source, value)`.
fn number_literal<'t>(text: &'t str, target: &EditTarget) -> (&'t str, f64) {
    let EditTarget::Literal {
        span,
        value: EditValue::Number(value),
        form: LiteralForm::Number,
    } = target
    else {
        panic!("expected a numeric Literal, got {target:?}");
    };
    (span_text(text, *span), *value)
}

/// Resolves `prop` on the first `<Rect` in `text` (ASCII only).
fn run(text: &str, prop: &str, ctx: &ResolveContext) -> EditTarget {
    let offset = text.find("<Rect").unwrap();
    let before = &text[..offset];
    let line = before.matches('\n').count() as u32 + 1;
    let column = (offset - before.rfind('\n').map_or(0, |i| i + 1)) as u32 + 1;
    let element = element_at(text, line, column).unwrap();
    resolve(text, &element, prop, ctx)
}

fn component(body: &str) -> String {
    format!("export const A = () => {{\n{body}\n}};\n")
}

fn span_text(text: &str, span: Span) -> &str {
    &text[span.start as usize..span.end as usize]
}

fn locked(reason: LockReason) -> EditTarget {
    EditTarget::Locked(reason)
}

#[test]
fn not_primitive_locks() {
    let text = component("  return <Rect x={1} />;");
    let c = ResolveContext {
        primitive: false,
        ..ctx()
    };
    assert_eq!(run(&text, "x", &c), locked(LockReason::NotPrimitive));

    // A call site's props are read on purpose.
    let c = ResolveContext {
        primitive: false,
        component_props: true,
        ..with_received(1.0)
    };
    assert!(matches!(run(&text, "x", &c), EditTarget::Literal { .. }));
}

#[test]
fn repeated_locks() {
    let text = component("  return <Rect x={1} />;");
    let c = ResolveContext {
        render_count: 12,
        ..ctx()
    };
    assert_eq!(
        run(&text, "x", &c),
        locked(LockReason::Repeated { count: 12 })
    );
}

#[test]
fn syntax_error_locks() {
    let text = "export const A = () => {\n  return <Rect x={1} />;\n  const = ;\n};\n";
    // The recovered AST may not even contain the element; build the ref by hand.
    let start = text.find("<Rect").unwrap() as u32;
    let element = ElementRef {
        start,
        end_of_attributes: text.find(" />").unwrap() as u32,
        children: None,
    };
    assert_eq!(
        resolve(text, &element, "x", &with_received(1.0)),
        locked(LockReason::SyntaxError)
    );
}

#[test]
fn missing_inserts_default() {
    let text = component("  return <Rect width={1} />;");
    let at = text.find(" />").unwrap() as u32;
    assert_eq!(
        run(&text, "x", &ctx()),
        EditTarget::Missing {
            insert_at: at,
            default: Some(EditValue::Number(0.0))
        }
    );
    assert_eq!(
        run(&text, "scale", &ctx()),
        EditTarget::Missing {
            insert_at: at,
            default: Some(EditValue::Number(1.0))
        }
    );
}

#[test]
fn missing_with_spread_locks() {
    let text = component("  return <Rect {...p} />;");
    assert_eq!(run(&text, "x", &ctx()), locked(LockReason::Spread));

    let text = component("  return <Rect {...p} x={3} />;");
    let target = run(&text, "x", &with_received(3.0));
    assert_eq!(number_literal(&text, &target), ("3", 3.0));
}

#[test]
fn literal() {
    let text = component("  return <Rect x={120} />;");
    let target = run(&text, "x", &with_received(120.0));
    assert_eq!(number_literal(&text, &target), ("120", 120.0));

    let text = component("  return <Rect x={-12} />;");
    let target = run(&text, "x", &with_received(-12.0));
    assert_eq!(number_literal(&text, &target), ("-12", -12.0));
    // The sign is part of the span, so a write replaces it whole.
    assert_eq!(
        apply(
            &text,
            &[target.to_edit("x", &EditValue::Number(8.0)).unwrap()]
        )
        .unwrap(),
        component("  return <Rect x={8} />;")
    );
}

#[test]
fn const_literal() {
    let text = component("  const X = 40;\n  return <Rect x={X} />;");
    let target = run(&text, "x", &with_received(40.0));
    assert_eq!(number_literal(&text, &target), ("40", 40.0));
}

#[test]
fn const_shared() {
    let text = component("  const X = 40;\n  return <Group><Rect x={X} /><Rect x={X} /></Group>;");
    assert_eq!(
        run(&text, "x", &with_received(40.0)),
        locked(LockReason::SharedBinding {
            name: "X".into(),
            uses: 2
        })
    );
}

#[test]
fn let_or_param_unresolved() {
    let text = component("  let X = 4;\n  return <Rect x={X} />;");
    assert_eq!(
        run(&text, "x", &with_received(4.0)),
        locked(LockReason::UnresolvedIdentifier { name: "X".into() })
    );

    let text = "export const A = ({ X }) => {\n  return <Rect x={X} />;\n};\n";
    assert_eq!(
        run(text, "x", &with_received(4.0)),
        locked(LockReason::UnresolvedIdentifier { name: "X".into() })
    );

    let text = "export function A(X: number) {\n  return <Rect x={X} />;\n}\n";
    assert_eq!(
        run(text, "x", &with_received(4.0)),
        locked(LockReason::UnresolvedIdentifier { name: "X".into() })
    );
}

#[test]
fn module_level_const_unresolved() {
    let text = "const X = 40;\nexport const A = () => {\n  return <Rect x={X} />;\n};\n";
    assert_eq!(
        run(text, "x", &with_received(40.0)),
        locked(LockReason::UnresolvedIdentifier { name: "X".into() })
    );
}

#[test]
fn inner_binding_shadows_outer_const() {
    let text = "export const A = () => {\n  const X = 40;\n  const B = (X: number) => <Rect x={X} />;\n  return B(1);\n};\n";
    assert_eq!(
        run(text, "x", &with_received(40.0)),
        locked(LockReason::UnresolvedIdentifier { name: "X".into() })
    );
}

#[test]
fn expression_locks() {
    let text = component("  return <Rect x={W / 2} />;");
    assert_eq!(
        run(&text, "x", &with_received(5.0)),
        locked(LockReason::Expression {
            source: "W / 2".into()
        })
    );
}

#[test]
fn overridden_by_parent() {
    let text = component("  return <Rect x={120} />;");
    assert_eq!(
        run(&text, "x", &with_received(80.0)),
        locked(LockReason::OverriddenByParent)
    );

    let text = component("  return <Rect width={1} />;");
    assert_eq!(
        run(&text, "x", &with_received(5.0)),
        locked(LockReason::OverriddenByParent)
    );
}

#[test]
fn to_edit_rejects_non_finite_values() {
    let text = component("  return <Rect x={120} />;");
    let target = run(&text, "x", &with_received(120.0));
    assert!(target.to_edit("x", &EditValue::Number(f64::NAN)).is_none());
    assert!(
        target
            .to_edit("x", &EditValue::Number(f64::INFINITY))
            .is_none()
    );
    assert!(
        locked(LockReason::Spread)
            .to_edit("x", &EditValue::Number(1.0))
            .is_none()
    );
}

#[test]
fn to_edit_round_trips_through_apply() {
    let text = component("  return <Rect x={120} width={1} />;");
    let literal = run(&text, "x", &with_received(120.0));
    let missing = run(&text, "y", &ctx());
    let edits: Vec<Edit> = [
        literal.to_edit("x", &EditValue::Number(130.5)).unwrap(),
        missing.to_edit("y", &EditValue::Number(7.0)).unwrap(),
    ]
    .into();
    assert_eq!(
        apply(&text, &edits).unwrap(),
        component("  return <Rect x={130.5} width={1} y={7} />;")
    );
}

const FRAME: &str = "  const frame = useCurrentFrame();\n";

fn animated(frame_line: &str, attr: &str) -> String {
    component(&format!("{FRAME}{frame_line}  return <Rect {attr} />;"))
}

fn at_frame(local_frame: f64, received: Option<f64>) -> ResolveContext {
    at_frames(&[local_frame], received)
}

fn at_frames(frames: &[f64], received: Option<f64>) -> ResolveContext {
    ResolveContext {
        candidate_frames: frames.to_vec(),
        received: received.map(EditValue::Number),
        ..ctx()
    }
}

#[test]
fn keyframe_hit() {
    let text = animated("", "x={interpolate(frame, [0, 30], [100, 400])}");
    let target = run(&text, "x", &at_frame(30.0, Some(400.0)));
    let EditTarget::Keyframe { span, value, frame } = target else {
        panic!("expected Keyframe, got {target:?}");
    };
    assert_eq!((span_text(&text, span), value, frame), ("400", 400.0, 30.0));
}

#[test]
fn keyframe_via_const() {
    let text = animated(
        "  const x = interpolate(frame, [0, 30], [100, 400], { easing: Easings.easeOut });\n",
        "x={x}",
    );
    let target = run(&text, "x", &at_frame(0.0, Some(100.0)));
    let EditTarget::Keyframe { span, value, .. } = target else {
        panic!("expected Keyframe, got {target:?}");
    };
    assert_eq!((span_text(&text, span), value), ("100", 100.0));
}

#[test]
fn keyframe_via_shared_const_locks() {
    let text = component(&format!(
        "{FRAME}  const x = interpolate(frame, [0, 30], [100, 400]);\n  return <Group><Rect x={{x}} /><Rect x={{x}} /></Group>;"
    ));
    assert_eq!(
        run(&text, "x", &at_frame(0.0, Some(100.0))),
        locked(LockReason::SharedBinding {
            name: "x".into(),
            uses: 2
        })
    );
}

#[test]
fn keyframe_offset() {
    let text = animated("", "x={interpolate(frame - 30, [0, 10], [0, 50])}");
    let target = run(&text, "x", &at_frame(40.0, Some(50.0)));
    let EditTarget::Keyframe { span, value, frame } = target else {
        panic!("expected Keyframe, got {target:?}");
    };
    assert_eq!((span_text(&text, span), value, frame), ("50", 50.0, 40.0));
}

#[test]
fn between() {
    let text = animated("", "x={interpolate(frame, [0, 30], [100, 400])}");
    assert_eq!(
        run(&text, "x", &at_frame(15.0, Some(250.0))),
        locked(LockReason::BetweenKeyframes {
            previous: 0.0,
            next: 30.0,
            frame: 15.0
        })
    );

    let text = animated("", "x={interpolate(frame - 30, [0, 10], [0, 50])}");
    assert_eq!(
        run(&text, "x", &at_frame(35.0, Some(25.0))),
        locked(LockReason::BetweenKeyframes {
            previous: 30.0,
            next: 40.0,
            frame: 35.0
        })
    );
}

#[test]
fn outside() {
    let text = animated("", "x={interpolate(frame, [0, 30], [100, 400])}");
    assert_eq!(
        run(&text, "x", &at_frame(45.0, Some(550.0))),
        locked(LockReason::OutsideKeyframes)
    );
    assert_eq!(
        run(&text, "x", &at_frame(-5.0, Some(50.0))),
        locked(LockReason::OutsideKeyframes)
    );
}

#[test]
fn computed_range() {
    let source = "interpolate(frame, range, [100, 400])";
    let text = animated("", &format!("x={{{source}}}"));
    assert_eq!(
        run(&text, "x", &at_frame(0.0, Some(100.0))),
        locked(LockReason::Expression {
            source: source.into()
        })
    );

    // Mismatched lengths and non-literal entries are not editable either.
    let source = "interpolate(frame, [0, 30], [100])";
    let text = animated("", &format!("x={{{source}}}"));
    assert_eq!(
        run(&text, "x", &at_frame(0.0, Some(100.0))),
        locked(LockReason::Expression {
            source: source.into()
        })
    );
}

#[test]
fn input_not_frame() {
    let source = "interpolate(progress, [0, 30], [100, 400])";
    let text = animated("", &format!("x={{{source}}}"));
    assert_eq!(
        run(&text, "x", &at_frame(0.0, Some(100.0))),
        locked(LockReason::Expression {
            source: source.into()
        })
    );

    // `frame` bound to something other than `useCurrentFrame()`.
    let source = "interpolate(frame, [0, 30], [100, 400])";
    let text = component(&format!(
        "  const frame = 3;\n  return <Rect x={{{source}}} />;"
    ));
    assert_eq!(
        run(&text, "x", &at_frame(0.0, Some(100.0))),
        locked(LockReason::Expression {
            source: source.into()
        })
    );
}

#[test]
fn spring_locks() {
    let text = animated("  const fps = 30;\n", "x={spring({ frame, fps })}");
    assert_eq!(
        run(&text, "x", &at_frame(0.0, Some(0.0))),
        locked(LockReason::UnsupportedCall {
            callee: "spring".into()
        })
    );
}

#[test]
fn keyframe_overridden() {
    let text = animated("", "x={interpolate(frame, [0, 30], [100, 400])}");
    assert_eq!(
        run(&text, "x", &at_frame(30.0, Some(390.0))),
        locked(LockReason::OverriddenByParent)
    );
}

#[test]
fn has_attribute_ignores_spreads_and_absent_props() {
    let text = component("return <Rect {...rest} scaleX={W / 2} x={1} />;");
    let offset = text.find("<Rect").unwrap();
    let element = ElementRef {
        start: offset as u32,
        end_of_attributes: 0,
        children: None,
    };

    assert!(has_attribute(&text, &element, "scaleX"));
    assert!(has_attribute(&text, &element, "x"));
    assert!(!has_attribute(&text, &element, "scaleY"));
    assert!(!has_attribute(&text, &element, "rest"));
}

// `frame` read above a `<Sequence>` that wraps the element: the runtime
// reports the frame of the component that wrote the JSX (60), not the
// element's (30), and the keyframe in effect is the third one.
const ABOVE_SEQUENCE: &str = "export const A = () => {\n  const frame = useCurrentFrame();\n  return <Sequence from={30}><Rect x={interpolate(frame, [0, 30, 60], [0, 100, 100])} /></Sequence>;\n};\n";

#[test]
fn keyframe_at_the_owner_frame_above_a_sequence() {
    let target = run(ABOVE_SEQUENCE, "x", &at_frame(60.0, Some(100.0)));
    let EditTarget::Keyframe { span, value, frame } = target else {
        panic!("expected Keyframe, got {target:?}");
    };
    let third = ABOVE_SEQUENCE.rfind("100").unwrap() as u32;
    assert_eq!((span.start, value, frame), (third, 100.0, 60.0));
}

// The frames below come from two frame providers above the element (the
// composition at 60, and `<Sequence from={30}>` at 30); `frame` was read under
// one of them, and only the received value can tell which.

#[test]
fn candidates_giving_different_keyframes_of_the_same_value_lock() {
    // [0, 30, 60] -> [0, 100, 100]: frame 30 and frame 60 both give 100.
    assert_eq!(
        run(ABOVE_SEQUENCE, "x", &at_frames(&[30.0, 60.0], Some(100.0))),
        locked(LockReason::FrameUncertain)
    );
}

const DISTINCT: &str = "export const A = () => {\n  const frame = useCurrentFrame();\n  return <Sequence from={30}><Rect x={interpolate(frame, [0, 30, 60], [0, 100, 200])} /></Sequence>;\n};\n";

#[test]
fn the_received_value_picks_the_candidate_keyframe() {
    let target = run(DISTINCT, "x", &at_frames(&[30.0, 60.0], Some(200.0)));
    let EditTarget::Keyframe { span, value, frame } = target else {
        panic!("expected Keyframe, got {target:?}");
    };
    assert_eq!(
        (span_text(DISTINCT, span), value, frame),
        ("200", 200.0, 60.0)
    );

    let target = run(DISTINCT, "x", &at_frames(&[30.0, 60.0], Some(100.0)));
    let EditTarget::Keyframe { span, value, frame } = target else {
        panic!("expected Keyframe, got {target:?}");
    };
    assert_eq!(span.start as usize, DISTINCT.find("100").unwrap());
    assert_eq!((value, frame), (100.0, 30.0));
}

#[test]
fn a_between_and_a_keyframe_candidate_of_the_same_value_lock() {
    // Frame 45 lies on the flat segment 30..60 (value 100), frame 30 is the
    // keyframe whose value is 100.
    assert_eq!(
        run(ABOVE_SEQUENCE, "x", &at_frames(&[30.0, 45.0], Some(100.0))),
        locked(LockReason::FrameUncertain)
    );
}

#[test]
fn an_eased_between_candidate_cannot_be_ruled_out() {
    // The easing decides the value between keyframes, so frame 15 stays a
    // candidate next to the keyframe at 30.
    let text = animated(
        "",
        "x={interpolate(frame, [0, 30], [100, 400], { easing: Easings.easeOut })}",
    );
    assert_eq!(
        run(&text, "x", &at_frames(&[15.0, 30.0], Some(400.0))),
        locked(LockReason::FrameUncertain)
    );
    // So do extrapolation options outside the keyframes.
    let text = animated(
        "",
        "x={interpolate(frame, [0, 30], [100, 400], { extrapolateRight: 'clamp' })}",
    );
    assert_eq!(
        run(&text, "x", &at_frames(&[30.0, 45.0], Some(400.0))),
        locked(LockReason::FrameUncertain)
    );
}

#[test]
fn a_custom_easing_keyframe_candidate_cannot_be_ruled_out() {
    // At frame 30 the runtime gives 100 + easing(0) * (200 - 100) = 150, not
    // the keyframe's 100, so frame 30 stays a candidate next to frame 60.
    let text = "export const A = () => {\n  const frame = useCurrentFrame();\n  return <Sequence from={30}><Rect x={interpolate(frame, [0, 30, 45, 60], [0, 100, 200, 150], { easing: t => t === 0 ? 0.5 : t })} /></Sequence>;\n};\n";
    assert_eq!(
        run(text, "x", &at_frames(&[30.0, 60.0], Some(150.0))),
        locked(LockReason::FrameUncertain)
    );
    // Options the keyframe value cannot be read from are no better.
    for options in [
        "opts",
        "{ ...opts }",
        "{ easing }",
        "{ ['easing']: Easings.linear }",
    ] {
        let text = animated(
            "",
            &format!("x={{interpolate(frame, [0, 30, 60], [0, 100, 200], {options})}}"),
        );
        assert_eq!(
            run(&text, "x", &at_frames(&[30.0, 60.0], Some(200.0))),
            locked(LockReason::FrameUncertain),
            "{options}"
        );
    }
    // A named easing starts each segment at 0 and ends it at 1.
    let text = animated(
        "",
        "x={interpolate(frame, [0, 30, 60], [0, 100, 200], { easing: Easings.easeInOut, extrapolateLeft: 'clamp' })}",
    );
    let target = run(&text, "x", &at_frames(&[30.0, 60.0], Some(200.0)));
    assert!(
        matches!(target, EditTarget::Keyframe { frame: 60.0, .. }),
        "{target:?}"
    );
}

#[test]
fn a_linear_between_candidate_is_ruled_out_by_its_value() {
    let text = animated("", "x={interpolate(frame, [0, 30], [100, 400])}");
    // Frame 15 gives 250, so the received 400 is the keyframe at 30.
    let target = run(&text, "x", &at_frames(&[15.0, 30.0], Some(400.0)));
    assert!(
        matches!(
            target,
            EditTarget::Keyframe {
                value: 400.0,
                frame: 30.0,
                ..
            }
        ),
        "{target:?}"
    );
    // And a keyframe candidate is ruled out by a between value: frame 30
    // gives 400, the received 250 is frame 15's.
    assert_eq!(
        run(&text, "x", &at_frames(&[30.0, 15.0], Some(250.0))),
        locked(LockReason::BetweenKeyframes {
            previous: 0.0,
            next: 30.0,
            frame: 15.0
        })
    );
    // Outside, the default extrapolation extends the slope: frame 45 gives
    // 550, not 400.
    let target = run(&text, "x", &at_frames(&[45.0, 30.0], Some(400.0)));
    assert!(
        matches!(
            target,
            EditTarget::Keyframe {
                value: 400.0,
                frame: 30.0,
                ..
            }
        ),
        "{target:?}"
    );
}

#[test]
fn no_candidate_matching_the_received_value_is_overridden() {
    assert_eq!(
        run(DISTINCT, "x", &at_frames(&[30.0, 60.0], Some(150.0))),
        locked(LockReason::OverriddenByParent)
    );
}

#[test]
fn the_literal_offset_applies_to_each_candidate() {
    // `frame - 30` on [0, 30] -> [0, 300]: frame 60 is the keyframe at 30
    // (300), frame 45 is between (150).
    let text = animated("", "x={interpolate(frame - 30, [0, 30], [0, 300])}");
    let target = run(&text, "x", &at_frames(&[45.0, 60.0], Some(300.0)));
    assert!(
        matches!(
            target,
            EditTarget::Keyframe {
                value: 300.0,
                frame: 60.0,
                ..
            }
        ),
        "{target:?}"
    );
    assert_eq!(
        run(&text, "x", &at_frames(&[45.0, 60.0], Some(150.0))),
        locked(LockReason::BetweenKeyframes {
            previous: 30.0,
            next: 60.0,
            frame: 45.0
        })
    );
}

#[test]
fn candidates_do_not_matter_to_literals() {
    let text = component("  return <Rect x={120} />;");
    let c = ResolveContext {
        candidate_frames: vec![30.0, 60.0],
        ..with_received(120.0)
    };
    assert!(matches!(run(&text, "x", &c), EditTarget::Literal { .. }));
    // Through a const, keyframes are disambiguated the same way.
    let text = animated(
        "  const x = interpolate(frame, [0, 30, 60], [0, 100, 100]);\n",
        "x={x}",
    );
    assert_eq!(
        run(&text, "x", &at_frames(&[30.0, 60.0], Some(100.0))),
        locked(LockReason::FrameUncertain)
    );
}

#[test]
fn block_scoped_shadowing_is_unresolved() {
    let text = component(
        "  const x = 0;\n  if (show) {\n    const x = 0;\n    return <Rect x={x} />;\n  }\n  return null;",
    );
    assert_eq!(
        run(&text, "x", &with_received(0.0)),
        locked(LockReason::UnresolvedIdentifier { name: "x".into() })
    );

    let text = component(
        "  const x = 0;\n  for (const x of [0]) {\n    return <Rect x={x} />;\n  }\n  return null;",
    );
    assert_eq!(
        run(&text, "x", &with_received(0.0)),
        locked(LockReason::UnresolvedIdentifier { name: "x".into() })
    );

    // Also a catch parameter or a destructured name elsewhere in the function.
    let text = component(
        "  const x = 0;\n  try { go(); } catch (x) { log(x); }\n  return <Rect x={x} />;",
    );
    assert_eq!(
        run(&text, "x", &with_received(0.0)),
        locked(LockReason::UnresolvedIdentifier { name: "x".into() })
    );
    let text = component(
        "  const x = 0;\n  const { x: y, ...x2 } = p;\n  { const [x] = q; }\n  return <Rect x={x} />;",
    );
    assert_eq!(
        run(&text, "x", &with_received(0.0)),
        locked(LockReason::UnresolvedIdentifier { name: "x".into() })
    );
}

#[test]
fn shadowed_frame_binding_is_unresolved() {
    let text = component(
        "  const frame = useCurrentFrame();\n  if (show) {\n    const frame = 30;\n    return <Rect x={interpolate(frame, [0, 30], [100, 400])} />;\n  }\n  return null;",
    );
    assert_eq!(
        run(&text, "x", &at_frame(0.0, Some(100.0))),
        locked(LockReason::UnresolvedIdentifier {
            name: "frame".into()
        })
    );
}

#[test]
fn same_name_in_another_component_still_resolves() {
    // Each component of a file declaring its own `frame` and `x` is common;
    // a sibling function's bindings are never visible here.
    let text = "export const B = () => {\n  const frame = useCurrentFrame();\n  const x = 5;\n  return null;\n};\nexport const A = () => {\n  const frame = useCurrentFrame();\n  const x = interpolate(frame, [0, 30], [100, 400]);\n  return <Rect x={x} />;\n};\n";
    let target = run(text, "x", &at_frame(30.0, Some(400.0)));
    assert!(
        matches!(target, EditTarget::Keyframe { value: 400.0, .. }),
        "{target:?}"
    );

    // A function-level const shadows a module-level one, as in JS.
    let text =
        "const x = 1;\nexport const A = () => {\n  const x = 40;\n  return <Rect x={x} />;\n};\n";
    let target = run(text, "x", &with_received(40.0));
    let EditTarget::Literal { span, .. } = target else {
        panic!("expected Literal, got {target:?}");
    };
    assert_eq!(span.start as usize, text.find("40").unwrap());

    // A nested function's own declaration inside the component counts.
    let text = component(
        "  const x = 0;\n  const f = () => { const x = 0; return x; };\n  return <Rect x={x} />;",
    );
    assert_eq!(
        run(&text, "x", &with_received(0.0)),
        locked(LockReason::UnresolvedIdentifier { name: "x".into() })
    );
}

#[test]
fn spread_after_the_attribute_locks() {
    let text = component("  return <Rect x={5} {...p} />;");
    assert_eq!(
        run(&text, "x", &with_received(5.0)),
        locked(LockReason::Spread)
    );

    // A spread before it is overridden by the attribute.
    let text = component("  return <Rect {...p} x={5} />;");
    assert!(matches!(
        run(&text, "x", &with_received(5.0)),
        EditTarget::Literal { .. }
    ));
}
