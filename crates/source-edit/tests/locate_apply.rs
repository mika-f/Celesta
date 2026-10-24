use celesta_source_edit::{
    ApplyError, Edit, EditValue, LiteralForm, apply, element_at, format_number,
};

fn at(text: &str, needle: &str) -> u32 {
    text.find(needle).unwrap() as u32
}

#[test]
fn element_at_finds_nested_self_closing() {
    let text =
        "export const A = () => {\n  return <Group>\n    <Rect width={10} />\n  </Group>;\n};\n";
    let r = element_at(text, 3, 5).unwrap();
    assert_eq!(r.start, at(text, "<Rect"));
    assert_eq!(r.end_of_attributes, at(text, " />"));
}

#[test]
fn element_at_bare_tag() {
    let text = "const A = () => <Group>\n</Group>;\n";
    let r = element_at(text, 1, 17).unwrap();
    assert_eq!(r.start, at(text, "<Group"));
    assert_eq!(
        r.end_of_attributes,
        at(text, "<Group") + "<Group".len() as u32
    );
}

#[test]
fn element_at_crlf() {
    let text = "const A = () => (\r\n  <Group>\r\n    <Rect width={10} />\r\n  </Group>\r\n);\r\n";
    let r = element_at(text, 3, 5).unwrap();
    assert_eq!(r.start, at(text, "<Rect"));
    assert_eq!(r.end_of_attributes, at(text, " />"));
}

#[test]
fn element_at_after_non_ascii() {
    let text = "const t = \"日本語\"; const e = <Rect x={1} />;";
    // esbuild's jsxDEV reports columnNumber 28 for `<Rect` here: 1-based, counted
    // in UTF-16 code units (27 units precede it; UTF-8 bytes would give 34).
    let r = element_at(text, 1, 28).unwrap();
    assert_eq!(r.start, at(text, "<Rect"));
    assert_eq!(r.end_of_attributes, at(text, " />"));
}

#[test]
fn element_at_none_off_element() {
    let text = "const A = () => <Group>\n</Group>;\n";
    assert_eq!(element_at(text, 1, 1), None);
    assert_eq!(element_at(text, 9, 1), None);
}

#[test]
fn format_number_rules() {
    for (v, s) in [
        (120.0, "120"),
        (120.004, "120"),
        (-12.0, "-12"),
        (0.125, "0.13"),
        (1.5, "1.5"),
        (-0.0, "0"),
        (-0.001, "0"),
    ] {
        assert_eq!(format_number(v), s, "{v}");
    }
}

#[test]
fn apply_replace_and_insert_in_one_pass() {
    let text = "<Rect x={1} y={2} />";
    let edits = [
        Edit::Insert {
            at: at(text, " />"),
            name: "rotation".into(),
            value: EditValue::Number(15.0),
        },
        Edit::Replace {
            start: at(text, "1}"),
            end: at(text, "1}") + 1,
            value: EditValue::Number(40.0),
            form: LiteralForm::Number,
        },
    ];
    assert_eq!(
        apply(text, &edits).unwrap(),
        "<Rect x={40} y={2} rotation={15} />"
    );
}

#[test]
fn apply_keeps_comments_and_spacing() {
    let text = "<Group>\n      {/* c */}\n\t<Rect   x={1}   />\n</Group>";
    let s = at(text, "1}");
    let out = apply(
        text,
        &[Edit::Replace {
            start: s,
            end: s + 1,
            value: EditValue::Number(2.5),
            form: LiteralForm::Number,
        }],
    )
    .unwrap();
    assert_eq!(
        out,
        "<Group>\n      {/* c */}\n\t<Rect   x={2.5}   />\n</Group>"
    );
}

#[test]
fn apply_overlap_is_error() {
    let edits = [
        Edit::Replace {
            start: 2,
            end: 6,
            value: EditValue::Number(1.0),
            form: LiteralForm::Number,
        },
        Edit::Replace {
            start: 4,
            end: 8,
            value: EditValue::Number(2.0),
            form: LiteralForm::Number,
        },
    ];
    assert_eq!(apply("0123456789", &edits), Err(ApplyError::Overlap));
}

#[test]
fn element_at_generic_tag_inserts_after_type_arguments() {
    let text = "const a = <Foo<T> />;\n";
    let element = element_at(text, 1, 11).unwrap();
    assert_eq!(element.start, at(text, "<Foo"));
    assert_eq!(element.end_of_attributes, at(text, "<T>") + 3);
    let edited = apply(
        text,
        &[Edit::Insert {
            at: element.end_of_attributes,
            name: "x".into(),
            value: EditValue::Number(4.0),
        }],
    )
    .unwrap();
    assert_eq!(edited, "const a = <Foo<T> x={4} />;\n");
}
