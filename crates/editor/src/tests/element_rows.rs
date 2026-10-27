//! Tests of the Properties tab's element rows (`element_rows`).

use super::*;

#[test]
fn transform_missing_scale_axis_shows_the_written_scale() {
    // `<Rect scale={2} scaleX={3} />`: scaleY is absent but drawn at 2.
    let report = primitive_report(
        2,
        3,
        ReceivedProps {
            scale: Some(2.0),
            scale_x: Some(3.0),
            ..ReceivedProps::default()
        },
    );
    let missing = local_target(
        "scaleY",
        EditTarget::Missing {
            insert_at: 0,
            default: Some(EditValue::Number(1.0)),
        },
    );
    assert_eq!(
        row_text("scaleY", &missing, None, &report),
        (String::new(), "2".to_owned())
    );
    assert_eq!(
        row_text(
            "scaleY",
            &local_target("scaleY", EditTarget::Locked(LockReason::Spread)),
            None,
            &report
        ),
        (String::new(), "2".to_owned())
    );
    // Without a scale, the default.
    let plain = primitive_report(2, 3, ReceivedProps::default());
    assert_eq!(
        row_text("scaleY", &missing, None, &plain),
        (String::new(), "1".to_owned())
    );
    assert_eq!(
        row_text(
            "x",
            &local_target(
                "x",
                EditTarget::Missing {
                    insert_at: 0,
                    default: Some(EditValue::Number(0.0))
                }
            ),
            None,
            &report
        ),
        (String::new(), "0".to_owned())
    );
}

/// A target written on the element itself, at `path`.
fn local_target(path: &str, target: EditTarget) -> Target {
    Target {
        file: PathBuf::from("/project/scene.tsx"),
        path: path.to_owned(),
        target,
        call_site: None,
        inputs: Vec::new(),
    }
}

#[test]
fn transform_missing_parameter_without_a_default_shows_the_drawn_value() {
    // `({ scale }) => <Rect scale={scale} />` called as `<Tag />`: no
    // default is known, and the layer draws at the primitive's own default.
    let missing = EditTarget::Missing {
        insert_at: 0,
        default: None,
    };
    let plain = primitive_report(2, 3, ReceivedProps::default());
    let at = |path: &str| local_target(path, missing.clone());
    assert_eq!(
        row_text("scale", &at("scale"), None, &plain),
        (String::new(), "1".to_owned())
    );
    assert_eq!(
        row_text("x", &at("x"), None, &plain),
        (String::new(), "0".to_owned())
    );
    // `({ scale = base * 2 })`: the value received is the one drawn, also
    // for a scale axis next to a written `scale`.
    let received = primitive_report(
        2,
        3,
        ReceivedProps {
            scale: Some(2.0),
            scale_x: Some(3.0),
            ..ReceivedProps::default()
        },
    );
    assert_eq!(
        row_text("scale", &at("scale"), None, &received),
        (String::new(), "2".to_owned())
    );
    assert_eq!(
        row_text("scaleX", &at("scaleX"), None, &received),
        (String::new(), "3".to_owned())
    );
    // Through the call site, as the row shows it.
    let through = EditTarget::PassThrough {
        call_site: celesta_source_edit::ElementRef {
            start: 0,
            end_of_attributes: 0,
            children: None,
        },
        param: "scale".to_owned(),
        target: Box::new(missing),
    };
    assert_eq!(
        row_text("scale", &local_target("scale", through), None, &plain),
        (String::new(), "1".to_owned())
    );
}

#[test]
fn a_pass_through_row_names_its_call_site() {
    let call_site = SourcePos {
        file: PathBuf::from("/project/scenes/price.tsx"),
        line: 18,
        column: 5,
    };
    assert_eq!(
        set_at_text(&call_site, Path::new("/project")),
        "Set at scenes/price.tsx:18"
    );
}

fn num_range(default: f64, min: f64, max: f64) -> PrimitivePropertyField {
    PrimitivePropertyField::Number {
        default_value: Some(default),
        min: Some(min),
        max: Some(max),
        step: None,
        integer: None,
    }
}

fn fit_field() -> PrimitivePropertyField {
    PrimitivePropertyField::Select {
        options: vec!["contain".to_owned(), "cover".to_owned()],
        default_value: Some("cover".to_owned()),
    }
}

#[test]
fn row_labels() {
    for (key, label) in [
        ("cornerRadius", "Corner radius"),
        ("children", "Text"),
        ("scaleX", "Scale X"),
        ("anchorY", "Anchor Y"),
        ("strokeWidth", "Stroke width"),
        ("x", "X"),
        ("opacity", "Opacity"),
        ("src", "Source"),
    ] {
        assert_eq!(row_label(key), label, "{key}");
    }
    // A commit error names the row the way the panel labels it.
    assert_eq!(
        CommitError::NotEditable(prop("fontSize").label()).to_string(),
        "Font size can't be edited here"
    );
}

#[test]
fn control_kinds() {
    let none = || None;
    assert_eq!(control_kind(&number_field(None)), ControlKind::Number);
    assert_eq!(
        control_kind(&PrimitivePropertyField::String {
            default_value: none()
        }),
        ControlKind::String
    );
    assert_eq!(
        control_kind(&PrimitivePropertyField::Text {
            default_value: none()
        }),
        ControlKind::Text
    );
    assert_eq!(
        control_kind(&PrimitivePropertyField::Boolean {
            default_value: None
        }),
        ControlKind::Boolean
    );
    assert_eq!(
        control_kind(&PrimitivePropertyField::Color {
            default_value: none()
        }),
        ControlKind::Color
    );
    assert_eq!(
        control_kind(&fit_field()),
        ControlKind::Select(vec!["contain".to_owned(), "cover".to_owned()])
    );
    assert_eq!(
        control_kind(&PrimitivePropertyField::Path),
        ControlKind::Path
    );
}

#[test]
fn parse_row_values() {
    let number = number_field(None);
    assert_eq!(
        parse_row_value(&ControlKind::Number, Some(&number), "12.5"),
        Ok(EditValue::Number(12.5))
    );
    assert_eq!(
        parse_row_value(&ControlKind::Number, Some(&number), "twelve"),
        Err("Enter a number".to_owned())
    );
    let opacity = num_range(1.0, 0.0, 1.0);
    assert_eq!(
        parse_row_value(&ControlKind::Number, Some(&opacity), "1.5"),
        Err("Enter a number from 0 to 1".to_owned())
    );
    assert_eq!(
        parse_row_value(&ControlKind::Number, Some(&opacity), "0.5"),
        Ok(EditValue::Number(0.5))
    );
    for color in ["#ff8800", "accent"] {
        assert_eq!(
            parse_row_value(&ControlKind::Color, None, color),
            Ok(EditValue::String(color.to_owned()))
        );
    }
    for color in ["#ff88", "two words", "9lives"] {
        assert_eq!(
            parse_row_value(&ControlKind::Color, None, color),
            Err("Enter a color: #RRGGBB, #RRGGBBAA, or a theme color name".to_owned()),
            "{color}"
        );
    }
    let fit = control_kind(&fit_field());
    assert!(parse_row_value(&fit, Some(&fit_field()), "fill").is_err());
    assert_eq!(
        parse_row_value(&fit, Some(&fit_field()), "cover"),
        Ok(EditValue::String("cover".to_owned()))
    );
    assert_eq!(
        parse_row_value(&ControlKind::Text, None, "A\nB"),
        Ok(EditValue::String("A\nB".to_owned()))
    );
    // Text keeps its outer spaces.
    assert_eq!(
        parse_row_value(&ControlKind::String, None, " Sale "),
        Ok(EditValue::String(" Sale ".to_owned()))
    );
}

#[test]
fn select_value_outside_the_options_is_shown() {
    let report = primitive_report(2, 3, ReceivedProps::default());
    let fit = local_target(
        "fit",
        EditTarget::Literal {
            span: Span { start: 1, end: 7 },
            value: EditValue::String("fill".to_owned()),
            form: LiteralForm::Quoted('"'),
        },
    );
    assert_eq!(
        row_text("fit", &fit, Some(&fit_field()), &report),
        ("fill".to_owned(), String::new())
    );
}

#[test]
fn missing_rows_show_the_table_default() {
    let missing = local_target(
        "cornerRadius",
        EditTarget::Missing {
            insert_at: 0,
            default: None,
        },
    );
    let field = number_field(Some(2.0));
    let plain = primitive_report(2, 3, ReceivedProps::default());
    assert_eq!(
        row_text("cornerRadius", &missing, Some(&field), &plain),
        (String::new(), "2".to_owned())
    );
    let mut received = plain.clone();
    received
        .props
        .others
        .insert("cornerRadius".to_owned(), ReceivedValue::Number(5.0));
    assert_eq!(
        row_text("cornerRadius", &missing, Some(&field), &received),
        (String::new(), "5".to_owned())
    );
}

#[test]
fn the_scale_axis_placeholder_of_a_parameter_default() {
    // `({ sx = 3 }) => <Rect scale={2} scaleX={sx} />` called without `sx`.
    let report = primitive_report(
        2,
        3,
        ReceivedProps {
            scale: Some(2.0),
            ..ReceivedProps::default()
        },
    );
    let through = local_target(
        "scaleX",
        EditTarget::PassThrough {
            call_site: celesta_source_edit::ElementRef {
                start: 0,
                end_of_attributes: 0,
                children: None,
            },
            param: "sx".to_owned(),
            target: Box::new(EditTarget::Missing {
                insert_at: 0,
                default: Some(EditValue::Number(3.0)),
            }),
        },
    );
    let field = number_field(Some(1.0));
    assert_eq!(
        row_text("scaleX", &through, Some(&field), &report),
        (String::new(), "3".to_owned())
    );
    // An axis absent from the element itself is drawn at the written scale.
    let missing = local_target(
        "scaleX",
        EditTarget::Missing {
            insert_at: 0,
            default: Some(EditValue::Number(1.0)),
        },
    );
    assert_eq!(
        row_text("scaleX", &missing, Some(&field), &report),
        (String::new(), "2".to_owned())
    );
}

#[test]
fn locked_rows_show_the_received_value() {
    let report = text_report(
        Path::new("/project/scene.tsx"),
        &[("children", ReceivedValue::String("Sale".to_owned()))],
    );
    let locked = local_target(
        "children",
        EditTarget::Locked(LockReason::Expression {
            source: "label".to_owned(),
        }),
    );
    let field = PrimitivePropertyField::Text {
        default_value: None,
    };
    assert_eq!(
        row_text("children", &locked, Some(&field), &report),
        ("Sale".to_owned(), String::new())
    );
}

#[test]
fn component_rows_show_the_schema_label_else_the_name() {
    let mut targets =
        resolve_targets(read_from(&props_tag_files()), &props_tag_report(), None).unwrap();
    // Unregistered: the names as written, not put into words.
    let component = targets.component.as_ref().unwrap();
    let unregistered = component_row_label(component, &component.props[1].name);
    assert_eq!(unregistered, component.props[1].name);
    let schemas = std::collections::BTreeMap::from([(
        "PriceTag".to_owned(),
        std::collections::BTreeMap::from([(
            "label".to_owned(),
            celesta_react_bridge::ComponentPropertyField::String {
                label: Some("Caption".to_owned()),
                default_value: String::new(),
            }
            .into(),
        )]),
    )]);

    type_component_props(&mut targets, &schemas, read_from(&props_tag_files()));

    let component = targets.component.as_ref().unwrap();
    assert_eq!(component_row_label(component, "label"), "Caption");
    // A prop the schema does not declare keeps its name.
    let undeclared = &component.props[1].name;
    assert_eq!(component_row_label(component, undeclared), *undeclared);
}

#[test]
fn a_component_schema_can_ask_for_whole_numbers() {
    let field =
        crate::element_edit::schema_field(&celesta_react_bridge::ComponentPropertyField::Number {
            label: None,
            default_value: 3.0,
            min: Some(1.0),
            max: None,
            step: None,
            integer: Some(true),
        });
    assert_eq!(
        parse_row_value(&ControlKind::Number, Some(&field), "2.5"),
        Err("Enter a whole number of at least 1".to_owned())
    );
    assert_eq!(scrub_value(3.0, 0.4, Some(&field), false), 3.0);
}

#[test]
fn component_props_that_do_not_fit_their_schema_lock() {
    let texts = typed_props_tag_files();
    let targets = typed_props_tag_targets(&texts);
    let call_site = |name: &str| PropKey::CallSite(name.to_owned());

    assert_eq!(
        targets.target(&call_site("label")),
        Some(&EditTarget::Locked(LockReason::Expression {
            source: "\"Sale\"".to_owned()
        }))
    );
    assert!(matches!(
        targets.target(&call_site("on")),
        Some(EditTarget::Locked(LockReason::Expression { .. }))
    ));
    // A number in its range stays editable, and typed text is checked
    // against the range.
    assert!(matches!(
        targets.target(&call_site("tone")),
        Some(EditTarget::Literal { .. })
    ));
    let field = row_field(&targets, &call_site("tone"));
    assert_eq!(
        parse_row_value(&ControlKind::Number, field, "11"),
        Err("Enter a number from 0 to 10".to_owned())
    );
}

#[test]
fn none_on_an_absent_path_writes_nothing() {
    let missing = EditTarget::Missing {
        insert_at: 0,
        default: None,
    };
    assert_eq!(cleared_path(&missing), None);
    let written = EditTarget::Literal {
        span: Span { start: 1, end: 7 },
        value: EditValue::String("a.png".to_owned()),
        form: LiteralForm::Quoted('"'),
    };
    assert_eq!(
        cleared_path(&written),
        Some(EditValue::String(String::new()))
    );
}

#[test]
fn scrub_steps() {
    let free = number_field(None);
    let unit = num_range(1.0, 0.0, 1.0);
    let near = |actual: f64, expected: f64| {
        assert!(
            (actual - expected).abs() < 1e-9,
            "{actual} is not {expected}"
        );
    };
    // One per pixel.
    assert_eq!(scrub_value(10.0, 5.0, Some(&free), false), 15.0);
    assert_eq!(scrub_value(10.0, 5.0, None, false), 15.0);
    // 0.01 per pixel for a field that ends at 1 or below, inside its range.
    near(scrub_value(0.5, 10.0, Some(&unit), false), 0.6);
    assert_eq!(scrub_value(0.5, 100.0, Some(&unit), false), 1.0);
    assert_eq!(scrub_value(0.5, -100.0, Some(&unit), false), 0.0);
    // Shift steps ten times as far.
    assert_eq!(scrub_value(10.0, 5.0, Some(&free), true), 60.0);
    near(scrub_value(0.5, 2.0, Some(&unit), true), 0.7);
    // The step a field declares (an anchor, a scale): unbounded.
    let anchor = stepped_field(0.0, 0.01);
    near(scrub_value(0.5, 10.0, Some(&anchor), false), 0.6);
    near(scrub_value(0.5, 10.0, Some(&anchor), true), 1.5);
    near(scrub_value(0.5, -150.0, Some(&anchor), false), -1.0);
    // A declared step wins over the range's.
    let coarse = PrimitivePropertyField::Number {
        default_value: None,
        min: Some(0.0),
        max: Some(1.0),
        step: Some(0.1),
        integer: None,
    };
    near(scrub_value(0.5, 2.0, Some(&coarse), false), 0.7);

    // A component prop takes the step its registered schema declares.
    let texts = typed_props_tag_files();
    let schemas = std::collections::BTreeMap::from([(
        "PriceTag".to_owned(),
        std::collections::BTreeMap::from([(
            "tone".to_owned(),
            celesta_react_bridge::ComponentPropertyField::Number {
                label: None,
                default_value: 0.0,
                min: Some(0.0),
                max: Some(100.0),
                step: Some(5.0),
                integer: None,
            }
            .into(),
        )]),
    )]);
    let mut targets = resolve_targets(read_from(&texts), &props_tag_report(), None).unwrap();
    type_component_props(&mut targets, &schemas, read_from(&texts));
    let tone = (targets, props_tag_report());
    assert_eq!(
        scrubbed_to(&tone, &PropKey::CallSite("tone".to_owned()), 102.0),
        12.0
    );

    // A table's `scale` declares 0.01; without a table, the transform rows'
    // scale scrubs the same.
    let text = "export const Scene = () => (\n  <Rect x={10} scale={2} />\n);\n";
    let report = primitive_report(
        2,
        3,
        ReceivedProps {
            x: Some(10.0),
            scale: Some(2.0),
            ..ReceivedProps::default()
        },
    );
    let table = (resolve_table(text, &report, &rect_table()), report.clone());
    near(scrubbed_to(&table, &prop("scale"), 110.0), 2.1);
    let bare = (resolve_in(text, &report).unwrap(), report);
    near(scrubbed_to(&bare, &prop("scale"), 110.0), 2.1);
}

/// `<Rect x={10} {attributes} />` at line 2, column 3 of
/// `/project/scene.tsx`, drawn with `x` 10 and `received`.
fn drawn_rect(attributes: &str, received: &[(&str, f64)]) -> (ElementTargets, LayerSourceReport) {
    let text = format!("export const Scene = () => (\n  <Rect x={{10}} {attributes} />\n);\n");
    let mut report = primitive_report(
        2,
        3,
        ReceivedProps {
            x: Some(10.0),
            ..ReceivedProps::default()
        },
    );
    for (name, value) in received {
        report
            .props
            .others
            .insert((*name).to_owned(), ReceivedValue::Number(*value));
    }
    (resolve_table(&text, &report, &rect_table()), report)
}

/// The override value a scrub of `key` pressed at x 100 sends at `x`.
fn scrubbed_to(
    (targets, report): &(ElementTargets, LayerSourceReport),
    key: &PropKey,
    x: f32,
) -> f64 {
    let mut scrub = Scrub::start(targets, report, key, 100.0).expect("the row scrubs");
    scrub.follow(x, false);
    let overrides = scrub.overrides(false);
    assert_eq!(overrides.len(), 1, "{overrides:?}");
    overrides[0].value
}

#[test]
fn a_scrub_starts_from_the_value_the_row_shows() {
    // The placeholder of an absent prop without a default in the source
    // (`({ r = base * 2 })`): what the element received, unrounded.
    let text = "export const Scene = () => (\n  <Rect x={10} />\n);\n";
    let missing = EditTarget::Missing {
        insert_at: 0,
        default: None,
    };
    let received = (
        single_row(text, "cornerRadius", missing),
        other_prop_report("Rect", "cornerRadius", ReceivedValue::Number(2.125)),
    );
    assert_eq!(scrubbed_to(&received, &prop("cornerRadius"), 104.0), 6.125);
    // Else the table's default.
    let absent = drawn_rect("", &[]);
    assert!((scrubbed_to(&absent, &prop("opacity"), 90.0) - 0.9).abs() < 1e-9);
    // A parameter's default in the source, at the call that leaves it out.
    let texts = Texts::from([
        (
            PathBuf::from("/project/component.tsx"),
            PRICE_TAG.replace("({ x })", "({ x = 7 })"),
        ),
        (
            PathBuf::from("/project/scene.tsx"),
            PRICE_TAG_SCENE.replace(" x={40}", ""),
        ),
    ]);
    let report = LayerSourceReport {
        props: ReceivedProps {
            x: Some(7.0),
            rotation: Some(10.0),
            ..ReceivedProps::default()
        },
        ..price_tag_report()
    };
    let targets = resolve_targets(read_from(&texts), &report, None).unwrap();
    assert_eq!(scrubbed_to(&(targets, report), &prop("x"), 104.0), 11.0);
}

#[test]
fn a_scrub_takes_the_focus_unless_another_row_holds_typed_text() {
    let (radius, opacity) = (prop("cornerRadius"), prop("opacity"));
    // Nothing typed, or only in the scrubbed row (whose text gives way):
    // the editor takes the focus, so Cmd+Z undoes the scrub's write.
    assert!(scrub_takes_focus([], &radius));
    assert!(scrub_takes_focus([&radius], &radius));
    // Another row's typed text stays in its focused field, unsubmitted.
    assert!(!scrub_takes_focus([&opacity], &radius));
    assert!(!scrub_takes_focus([&radius, &opacity], &radius));
}

#[test]
fn a_scrub_back_at_its_start_writes_nothing_outside_the_range() {
    // `opacity={1.5}` scrubs from 1, the nearest value inside its range.
    let (targets, report) = drawn_rect("opacity={1.5}", &[("opacity", 1.5)]);
    let mut scrub = Scrub::start(&targets, &report, &prop("opacity"), 100.0).unwrap();
    assert!(scrub.follow(95.0, false));
    assert_eq!(
        scrub.overrides(false),
        [override_at("/project/scene.tsx", "opacity", 0.95)]
    );
    scrub.follow(100.0, false);
    assert!(!scrub.changed());
}

/// The overrides a scrub of `key` pressed at x 100 sends at x 104, 110 and
/// 96; `None` when the row does not scrub.
fn scrub_frames(
    targets: &ElementTargets,
    report: &LayerSourceReport,
    key: &PropKey,
) -> Option<Vec<Vec<PropOverride>>> {
    let mut scrub = Scrub::start(targets, report, key, 100.0)?;
    Some(
        [104.0, 110.0, 96.0]
            .into_iter()
            .map(|x| {
                scrub.follow(x, false);
                scrub.overrides(false)
            })
            .collect(),
    )
}

#[test]
fn scrub_overrides_the_row_prop() {
    // A literal on the element: its own prop there.
    let (targets, report) = corner_radius_rect();
    let at = |prop: &str, value: f64| vec![override_at("/project/scene.tsx", prop, value)];
    assert_eq!(
        scrub_frames(&targets, &report, &prop("cornerRadius")),
        Some(vec![
            at("cornerRadius", 8.0),
            at("cornerRadius", 14.0),
            at("cornerRadius", 0.0),
        ])
    );

    // A `Text` font row the `style` object sets: that key of `style`.
    let text = "export const Scene = () => (\n  <Text style={{ fontSize: 48 }} fontWeight={700}>A</Text>\n);\n";
    let report = text_report(
        Path::new("/project/scene.tsx"),
        &[("style.fontSize", ReceivedValue::Number(48.0))],
    );
    let targets = resolve_table(text, &report, &text_table());
    assert_eq!(
        scrub_frames(&targets, &report, &prop("fontSize")),
        Some(vec![
            at("style.fontSize", 52.0),
            at("style.fontSize", 58.0),
            at("style.fontSize", 44.0),
        ])
    );

    // A value passed through: the parameter's prop at the call site.
    let targets = mixed_targets();
    let report = price_tag_report();
    let called = |value: f64| vec![override_at("/project/scene.tsx", "left", value)];
    assert_eq!(
        scrub_frames(&targets, &report, &prop("x")),
        Some(vec![called(44.0), called(50.0), called(36.0)])
    );
    // An input is a value the expression reads, not a prop; a computed row
    // is locked.
    let input = PropKey::Input {
        row: "scale".to_owned(),
        name: "size".to_owned(),
    };
    assert_eq!(scrub_frames(&targets, &report, &input), None);
    assert_eq!(scrub_frames(&targets, &report, &prop("scale")), None);

    // A component prop: that prop at the component's call, inside its range.
    let texts = typed_props_tag_files();
    let targets = typed_props_tag_targets(&texts);
    let report = props_tag_report();
    let tone = |value: f64| vec![override_at("/project/scene.tsx", "tone", value)];
    assert_eq!(
        scrub_frames(&targets, &report, &PropKey::CallSite("tone".to_owned())),
        Some(vec![tone(6.0), tone(10.0), tone(0.0)])
    );

    // A keyframe has no single value to override; a color is not a number.
    let report = above_sequence_report(vec![]);
    let targets = resolve_in(ABOVE_SEQUENCE, &report).unwrap();
    assert_eq!(scrub_frames(&targets, &report, &prop("x")), None);
    let (targets, report) = corner_radius_rect();
    assert_eq!(scrub_frames(&targets, &report, &prop("fill")), None);
}

/// `<Rect x={10} fill="#fff" cornerRadius={4} />` at line 2, column 3 of
/// `/project/scene.tsx`, as the runtime drew it.
fn corner_radius_rect() -> (ElementTargets, LayerSourceReport) {
    let text =
        "export const Scene = () => (\n  <Rect x={10} fill=\"#fff\" cornerRadius={4} />\n);\n";
    let mut report = primitive_report(
        2,
        3,
        ReceivedProps {
            x: Some(10.0),
            ..ReceivedProps::default()
        },
    );
    for (name, value) in [
        ("fill", ReceivedValue::String("#fff".to_owned())),
        ("cornerRadius", ReceivedValue::Number(4.0)),
    ] {
        report.props.others.insert(name.to_owned(), value);
    }
    (resolve_table(text, &report, &rect_table()), report)
}

#[test]
fn a_scrub_overrides_nothing_before_it_moves_or_while_playing() {
    let (targets, report) = corner_radius_rect();
    let mut scrub = Scrub::start(&targets, &report, &prop("cornerRadius"), 100.0).unwrap();
    assert_eq!(scrub.overrides(false), []);
    assert!(!scrub.follow(100.0, false));
    assert_eq!(scrub.overrides(false), []);

    assert!(scrub.follow(103.0, false));
    assert!(scrub.changed());
    assert_eq!(
        scrub.overrides(false),
        [override_at("/project/scene.tsx", "cornerRadius", 7.0)]
    );
    assert_eq!(scrub.overrides(true), []);
    // Back at the start it keeps overriding (the same position), and has
    // nothing to write.
    assert!(scrub.follow(100.0, false));
    assert_eq!(
        scrub.overrides(false),
        [override_at("/project/scene.tsx", "cornerRadius", 4.0)]
    );
    assert!(!scrub.changed());
}

#[test]
fn only_a_written_optional_value_is_removable() {
    let (_, targets, _) = removable_rect();
    let can_remove = |key: &PropKey| removable(&targets, key, targets.entry(key).unwrap());
    // Written and optional.
    assert!(can_remove(&prop("cornerRadius")));
    assert!(can_remove(&prop("fill")));
    // Required by the props type, absent, or locked.
    assert!(!can_remove(&prop("width")));
    assert!(!can_remove(&prop("x")));
    assert!(!can_remove(&prop("strokeWidth")));
    // A keyframe is an entry of an animation, not the prop.
    let keyframe = local_target(
        "x",
        EditTarget::Keyframe {
            span: Span { start: 0, end: 1 },
            value: 1.0,
            frame: 0.0,
        },
    );
    assert!(!removable(&targets, &prop("x"), &keyframe));
    // An input is a value an expression reads, never a prop.
    let literal = targets.entry(&prop("cornerRadius")).unwrap();
    let input = PropKey::Input {
        row: "width".to_owned(),
        name: "w".to_owned(),
    };
    assert!(!removable(&targets, &input, literal));

    // A `<Text>`'s children are never removed, table or not.
    let text = "export const Scene = () => (\n  <Text>Sale</Text>\n);\n";
    let report = text_report(
        Path::new("/project/scene.tsx"),
        &[("children", ReceivedValue::String("Sale".to_owned()))],
    );
    let targets = resolve_table(text, &report, &text_table());
    let children = prop("children");
    assert!(!removable(
        &targets,
        &children,
        targets.entry(&children).unwrap()
    ));
}

#[test]
fn a_value_set_at_the_call_is_removable_there() {
    let texts = props_tag_files();
    let report = props_tag_report();
    let targets = resolve_targets(read_from(&texts), &report, None).unwrap();
    let x = prop("x");
    let label = PropKey::CallSite("label".to_owned());
    assert!(removable(&targets, &x, targets.entry(&x).unwrap()));
    assert!(removable(&targets, &label, targets.entry(&label).unwrap()));

    // The pass-through's attribute goes at the call (`PriceTag`'s `x = 0`
    // applies), and so does a component prop.
    let scene = PathBuf::from("/project/scene.tsx");
    assert_eq!(
        commit_files(&texts, &targets, &report, None, None, &[(x, None)]),
        Ok(vec![(
            scene.clone(),
            PROPS_TAG_SCENE.replace(" x={40}", "")
        )])
    );
    assert_eq!(
        commit_files(&texts, &targets, &report, None, None, &[(label, None)]),
        Ok(vec![(
            scene,
            PROPS_TAG_SCENE.replace(" label=\"Sale\"", "")
        )])
    );
}

#[test]
fn a_prop_the_component_schema_requires_is_not_removable() {
    // `PriceTag` registered with `x` and `label` required and `tone`
    // optional; `on` is left out of the schema.
    let texts = props_tag_files();
    let report = props_tag_report();
    let field = |required| celesta_react_bridge::ComponentSchemaField {
        field: schema_number(None, None),
        required,
    };
    let schemas = std::collections::BTreeMap::from([(
        "PriceTag".to_owned(),
        std::collections::BTreeMap::from([
            ("x".to_owned(), field(true)),
            ("label".to_owned(), field(true)),
            ("tone".to_owned(), field(false)),
        ]),
    )]);
    let mut targets = resolve_targets(read_from(&texts), &report, None).unwrap();
    type_component_props(&mut targets, &schemas, read_from(&texts));
    let can_remove = |key: &PropKey| removable(&targets, key, targets.entry(key).unwrap());

    // `x` passes through to the `<Rect>`'s `x`, removed at the call.
    assert!(!can_remove(&prop("x")));
    let tone = PropKey::CallSite("tone".to_owned());
    let on = PropKey::CallSite("on".to_owned());
    assert!(can_remove(&tone));
    assert!(can_remove(&on));
    assert_eq!(
        commit_files(&texts, &targets, &report, None, None, &[(prop("x"), None)]),
        Err(CommitError::NotRemovable("X".to_owned()))
    );
    assert_eq!(
        commit_files(&texts, &targets, &report, None, None, &[(tone, None)]),
        Ok(vec![(
            PathBuf::from("/project/scene.tsx"),
            PROPS_TAG_SCENE.replace(" tone={2}", "")
        )])
    );
}

#[test]
fn a_required_component_prop_keeps_its_value_when_emptied() {
    // `<Badge count={3} />` with `count` required: emptying the field writes
    // nothing, as for a primitive's required prop.
    let texts = props_tag_files();
    let report = props_tag_report();
    let schemas = std::collections::BTreeMap::from([(
        "PriceTag".to_owned(),
        std::collections::BTreeMap::from([(
            "tone".to_owned(),
            celesta_react_bridge::ComponentSchemaField {
                field: schema_number(None, None),
                required: true,
            },
        )]),
    )]);
    let mut targets = resolve_targets(read_from(&texts), &report, None).unwrap();
    type_component_props(&mut targets, &schemas, read_from(&texts));
    let tone = PropKey::CallSite("tone".to_owned());
    let tone_label = tone.label();
    let removes = removable(&targets, &tone, targets.entry(&tone).unwrap());
    assert!(!removes);
    assert_eq!(
        typed_text(
            &ControlKind::Number,
            row_field(&targets, &tone),
            "",
            removes
        ),
        TypedText::Restore
    );
    assert_eq!(
        commit_files(&texts, &targets, &report, None, None, &[(tone, None)]),
        Err(CommitError::NotRemovable(tone_label))
    );
}

#[test]
fn an_emptied_field_removes_a_removable_row() {
    let number = ControlKind::Number;
    assert_eq!(
        typed_text(&number, None, "  ", true),
        TypedText::Write(None)
    );
    assert_eq!(typed_text(&number, None, "", false), TypedText::Restore);
    // Strings too: an empty string is not written over a removable prop.
    assert_eq!(
        typed_text(&ControlKind::String, None, "", true),
        TypedText::Write(None)
    );
    assert_eq!(
        typed_text(&ControlKind::Text, None, "", false),
        TypedText::Restore
    );
    // Anything typed is parsed as before.
    assert_eq!(
        typed_text(&number, None, "12", true),
        TypedText::Write(Some(EditValue::Number(12.0)))
    );
    assert_eq!(
        typed_text(&number, None, "big", true),
        TypedText::Invalid("Enter a number".to_owned())
    );
}

#[test]
fn an_input_takes_its_kind_from_its_literal_or_default() {
    let (_, targets, _) = removable_rect();
    let input = PropKey::Input {
        row: "width".to_owned(),
        name: "w".to_owned(),
    };
    let kind = |target: EditTarget| row_kind(&targets, &input, &local_target("w", target));
    let missing = |default: EditValue| EditTarget::Missing {
        insert_at: 0,
        default: Some(default),
    };
    assert_eq!(
        kind(missing(EditValue::String("Sale".to_owned()))),
        ControlKind::String
    );
    assert_eq!(kind(missing(EditValue::Bool(true))), ControlKind::Boolean);
    assert_eq!(kind(missing(EditValue::Number(2.0))), ControlKind::Number);
    assert_eq!(
        kind(EditTarget::Literal {
            span: Span { start: 0, end: 4 },
            value: EditValue::Bool(true),
            form: LiteralForm::Bool,
        }),
        ControlKind::Boolean
    );
    // Through a parameter, the call site's.
    assert_eq!(
        kind(EditTarget::PassThrough {
            call_site: targets.element,
            param: "w".to_owned(),
            target: Box::new(missing(EditValue::String("a".to_owned()))),
        }),
        ControlKind::String
    );
    assert_eq!(
        kind(EditTarget::Missing {
            insert_at: 0,
            default: None
        }),
        ControlKind::Number
    );
}

#[test]
fn a_font_row_style_may_set_shows_what_style_received() {
    let text =
        "export const Scene = () => (\n  <Text style={mono(20)} color=\"red\">A</Text>\n);\n";
    let report = text_report(
        Path::new("/project/scene.tsx"),
        &[
            ("color", ReceivedValue::String("red".to_owned())),
            ("style.fill", ReceivedValue::String("#00f".to_owned())),
            ("children", ReceivedValue::String("A".to_owned())),
        ],
    );
    let targets = resolve_table(text, &report, &text_table());
    let color = targets.entry(&prop("color")).unwrap();
    assert_eq!(color.target, EditTarget::Locked(LockReason::StyleMaySet));
    let field = PrimitivePropertyField::Color {
        default_value: None,
    };
    // `style` decides what is drawn.
    assert_eq!(
        row_text("color", color, Some(&field), &report),
        ("#00f".to_owned(), String::new())
    );
    // When `style` received none, the shorthand's value.
    let mut without = report.clone();
    without.props.others.remove("style.fill");
    assert_eq!(
        row_text("color", color, Some(&field), &without),
        ("red".to_owned(), String::new())
    );
}

/// A whole-number field, as a polygon's `sides`.
fn whole_field(min: Option<f64>, max: Option<f64>) -> PrimitivePropertyField {
    PrimitivePropertyField::Number {
        default_value: None,
        min,
        max,
        step: None,
        integer: Some(true),
    }
}

#[test]
fn a_whole_number_field_refuses_fractions() {
    let number = ControlKind::Number;
    let sides = whole_field(Some(3.0), None);
    assert_eq!(
        parse_row_value(&number, Some(&sides), "5"),
        Ok(EditValue::Number(5.0))
    );
    for text in ["3.5", "2"] {
        assert_eq!(
            parse_row_value(&number, Some(&sides), text),
            Err("Enter a whole number of at least 3".to_owned()),
            "{text}"
        );
    }
    let ranged = whole_field(Some(1.0), Some(10.0));
    assert_eq!(
        parse_row_value(&number, Some(&ranged), "2.5"),
        Err("Enter a whole number from 1 to 10".to_owned())
    );
    let capped = whole_field(None, Some(10.0));
    assert_eq!(
        parse_row_value(&number, Some(&capped), "0.5"),
        Err("Enter a whole number of at most 10".to_owned())
    );
    assert_eq!(
        parse_row_value(&number, Some(&whole_field(None, None)), "0.5"),
        Err("Enter a whole number".to_owned())
    );
    // A field that does not say so takes fractions, whatever its step.
    assert_eq!(
        parse_row_value(&number, Some(&stepped_field(1.0, 1.0)), "2.5"),
        Ok(EditValue::Number(2.5))
    );
}

#[test]
fn a_whole_number_scrub_moves_in_whole_numbers() {
    let sides = whole_field(Some(3.0), None);
    // A start that is not whole is rounded first.
    assert_eq!(scrub_value(4.4, 0.0, Some(&sides), false), 4.0);
    assert_eq!(scrub_value(4.4, 2.0, Some(&sides), false), 6.0);
    assert_eq!(scrub_value(4.0, -5.0, Some(&sides), false), 3.0);
    // Without the declaration a scrub keeps the fraction.
    assert_eq!(scrub_value(4.4, 2.0, Some(&number_field(None)), false), 6.4);
}

#[test]
fn held_commits_keep_the_latest_value_per_row_in_order() {
    let number = |value: f64| Some(EditValue::Number(value));
    let held_at = |layer: &str, key: PropKey, value, frame| HeldCommit {
        layer: layer.to_owned(),
        key,
        value,
        frame,
        scope: None,
        scanning: false,
        gesture: false,
    };
    let mut held = HeldCommits::default();
    held.push(held_at("a", prop("x"), number(1.0), 0));
    held.push(held_at("a", prop("fill"), None, 0));
    held.push(held_at("a", prop("x"), number(3.0), 4));
    assert!(held.contains("a", &prop("x")));
    assert!(!held.contains("b", &prop("x")));
    assert!(held.any(Some("a")));
    assert!(!held.any(Some("b")));
    assert!(!held.any(None));
    // The newer value keeps the row's place, with the frame it was made at.
    assert_eq!(
        held.pop(Some("a")),
        Some(held_at("a", prop("x"), number(3.0), 4))
    );
    assert_eq!(
        held.pop(Some("a")),
        Some(held_at("a", prop("fill"), None, 0))
    );
    assert_eq!(held.pop(Some("a")), None);

    // Each layer's commits are its own; another layer's stay held.
    held.push(held_at("a", prop("x"), number(1.0), 0));
    held.push(held_at("b", prop("x"), number(2.0), 0));
    assert_eq!(
        held.pop(Some("b")),
        Some(held_at("b", prop("x"), number(2.0), 0))
    );
    assert!(held.contains("a", &prop("x")));
    assert!(!held.is_empty());
    assert_eq!(
        held.take_all(),
        vec![held_at("a", prop("x"), number(1.0), 0)]
    );
    assert!(!held.any(Some("a")));
    assert!(held.is_empty());
}

#[test]
fn a_selection_change_takes_the_commits_held_for_the_scan() {
    let held_at = |layer: &str, key: &str, scanning| HeldCommit {
        layer: layer.to_owned(),
        key: prop(key),
        value: Some(EditValue::Number(1.0)),
        frame: 0,
        scope: None,
        scanning,
        gesture: false,
    };
    let mut held = HeldCommits::default();
    held.push(held_at("a", "x", true));
    held.push(held_at("a", "y", false));
    held.push(held_at("a", "fill", true));
    held.push(held_at("b", "x", true));

    // The selected layer's commits waiting for the scan go, in order...
    assert_eq!(
        held.take_scanning(Some("a")),
        vec![held_at("a", "x", true), held_at("a", "fill", true)]
    );
    // ...and one waiting for a reload stays, so the change waits for it.
    assert!(held.any(Some("a")));
    assert_eq!(held.pop(Some("a")), Some(held_at("a", "y", false)));
    // Another layer's are left alone.
    assert!(held.contains("b", &prop("x")));
    assert!(held.take_scanning(None).is_empty());
    // A row committed again replaces its commit, waiting for what the
    // newer one waits for.
    held.push(held_at("b", "x", false));
    assert!(held.take_scanning(Some("b")).is_empty());
}

#[test]
fn a_selection_waits_for_the_held_commits_of_the_selected_layer() {
    let mut gate = SelectionGate::default();
    // Nothing held: the selection changes at once.
    assert_eq!(
        gate.request(Some("b".to_owned()), Some("a"), false),
        Some(Some("b".to_owned()))
    );
    // Held commits for "a": the click waits, and a second one replaces it.
    assert_eq!(gate.request(Some("b".to_owned()), Some("a"), true), None);
    assert_eq!(gate.request(None, Some("a"), true), None);
    assert!(gate.waiting());
    assert_eq!(gate.release(true), None);
    // Once they are written (or dropped), the latest request goes through.
    assert_eq!(gate.release(false), Some(None));
    assert!(!gate.waiting());
    assert_eq!(gate.release(false), None);
    // Clicking the selected layer again cancels a waiting change.
    gate.request(Some("b".to_owned()), Some("a"), true);
    assert_eq!(gate.request(Some("a".to_owned()), Some("a"), true), None);
    assert_eq!(gate.release(false), None);
}

#[test]
fn edited_rows_of_the_selected_layer_are_submitted_before_it_changes() {
    let (x, y, fill) = (prop("x"), prop("y"), prop("fill"));
    let rows = [
        (&x, true, Some("a")),
        (&y, false, Some("a")),
        (&fill, true, Some("b")),
    ];
    assert_eq!(edited_rows(rows, Some("a")), vec![x.clone()]);
    assert_eq!(edited_rows(rows, None), Vec::<PropKey>::new());
}

#[test]
fn a_held_keyframe_commit_does_not_follow_a_seek() {
    let keyframe = EditTarget::Keyframe {
        span: Span { start: 0, end: 1 },
        value: 1.0,
        frame: 10.0,
    };
    let literal = EditTarget::Literal {
        span: Span { start: 0, end: 1 },
        value: EditValue::Number(1.0),
        form: LiteralForm::Number,
    };
    // Made at frame 10, the targets now read at frame 20.
    assert!(moved_off_keyframe(10, 20, &keyframe));
    assert!(!moved_off_keyframe(10, 10, &keyframe));
    // A plain value is the same at every frame.
    assert!(!moved_off_keyframe(10, 20, &literal));
    // Between keyframes at the new frame: moved off its keyframe too.
    let between = EditTarget::Locked(LockReason::BetweenKeyframes {
        previous: 0.0,
        next: 30.0,
        frame: 20.0,
    });
    assert!(moved_off_keyframe(10, 20, &between));
    assert!(!moved_off_keyframe(20, 20, &between));
}

#[test]
fn leaving_the_interactive_mode_waits_for_the_held_commits() {
    use InteractiveToggle::*;
    assert_eq!(interactive_toggle(false, false, false), Enter);
    // Nothing held: the selection cleared at once, and the mode goes.
    assert_eq!(interactive_toggle(true, false, false), Leave);
    // The clearing waits for held commits: so does the mode, which keeps
    // looking the source up until they are written.
    assert_eq!(interactive_toggle(true, false, true), LeaveAfterCommits);
    // Toggled again while waiting: it stays interactive.
    assert_eq!(interactive_toggle(true, true, true), StayInteractive);
}

#[test]
fn a_finished_commit_touches_only_its_own_layers_row() {
    assert!(commit_owns_row(Some("a"), Some("a")));
    // The row shows layer "b" now: a failure of "a"'s write leaves it be.
    assert!(!commit_owns_row(Some("b"), Some("a")));
    assert!(!commit_owns_row(None, Some("a")));
    assert!(!commit_owns_row(Some("a"), None));
}

#[test]
fn a_removed_row_shows_its_placeholder_until_the_reload_resolves() {
    let mut state = ElementEdit::default();
    state.invalidate();
    state.record_written(Some("a"), &[(prop("cornerRadius"), None)]);
    assert_eq!(
        state.written_value(Some("a"), &prop("cornerRadius")),
        Some(&None)
    );

    let corner = number_field(Some(0.0));
    assert_eq!(
        written_text(&prop("cornerRadius"), Some(&corner), None),
        (String::new(), "0".to_owned())
    );
    // Without a table field, a transform prop's own default.
    assert_eq!(
        written_text(&prop("x"), None, None),
        (String::new(), "0".to_owned())
    );
    assert_eq!(
        written_text(
            &prop("fill"),
            None,
            Some(&EditValue::String("#fff".to_owned()))
        ),
        ("#fff".to_owned(), String::new())
    );
}
