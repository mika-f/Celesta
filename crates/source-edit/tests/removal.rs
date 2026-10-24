use celesta_source_edit::{
    ApplyError, Edit, EditTarget, EditValue, ElementRef, ResolveContext, apply, element_at, parses,
    removal, resolve_through,
};

/// The element whose `<` is the first `needle` in `text` (ASCII only).
fn element(text: &str, needle: &str) -> ElementRef {
    let offset = text.find(needle).unwrap();
    let before = &text[..offset];
    let line = before.matches('\n').count() as u32 + 1;
    let column = (offset - before.rfind('\n').map_or(0, |i| i + 1)) as u32 + 1;
    element_at(text, line, column).unwrap()
}

/// `text` with `prop` removed from the first `<{tag}`.
fn remove(text: &str, tag: &str, prop: &str) -> String {
    let edit = removal(text, &element(text, &format!("<{tag}")), prop)
        .unwrap_or_else(|| panic!("{prop} is not removable in {text:?}"));
    apply(text, &[edit]).unwrap()
}

fn removable(text: &str, tag: &str, prop: &str) -> bool {
    removal(text, &element(text, &format!("<{tag}")), prop).is_some()
}

#[test]
fn an_attribute_goes_with_its_leading_whitespace() {
    assert_eq!(
        remove("<Text maxWidth={400}>Sale</Text>;", "Text", "maxWidth"),
        "<Text>Sale</Text>;"
    );
    assert_eq!(
        remove(
            "<Rect x={1} cornerRadius={8} y={2} />;",
            "Rect",
            "cornerRadius"
        ),
        "<Rect x={1} y={2} />;"
    );
}

#[test]
fn a_bare_boolean_attribute_goes() {
    assert_eq!(
        remove("<Video loop src=\"a.mp4\" />;", "Video", "loop"),
        "<Video src=\"a.mp4\" />;"
    );
    assert_eq!(
        remove("<Video src=\"a.mp4\" loop/>;", "Video", "loop"),
        "<Video src=\"a.mp4\"/>;"
    );
}

#[test]
fn a_quoted_string_and_a_container_go() {
    assert_eq!(
        remove("<Image src=\"a.png\" fit='cover' />;", "Image", "fit"),
        "<Image src=\"a.png\" />;"
    );
    assert_eq!(
        remove("<Rect fill={'#fff'} stroke={`#000`} />;", "Rect", "fill"),
        "<Rect stroke={`#000`} />;"
    );
    // The value may be any expression; the attribute goes whole.
    assert_eq!(
        remove("<Rect width={w + 2 * pad} height={10} />;", "Rect", "width"),
        "<Rect height={10} />;"
    );
}

#[test]
fn an_attribute_after_a_line_comment_stays_off_the_comment() {
    // The attribute starts its line: it goes with the space after it, and
    // the next attribute keeps the line (and its indentation).
    let text = "<Rect\n  // background\n  fill=\"#fff\" stroke=\"#000\"\n/>;";
    let removed = remove(text, "Rect", "fill");
    assert_eq!(removed, "<Rect\n  // background\n  stroke=\"#000\"\n/>;");
    assert!(parses(&removed));

    // After a line ending in a comment, the tag's end stays on its line.
    let text = "<Rect\n  x={1} // left\n  cornerRadius={8} />;";
    let removed = remove(text, "Rect", "cornerRadius");
    assert_eq!(removed, "<Rect\n  x={1} // left\n  />;");
    assert!(parses(&removed));
}

#[test]
fn an_attribute_alone_on_its_line_takes_the_line() {
    let text = "<Rect\n  x={1}\n  cornerRadius={8}\n  y={2}\n/>;";
    assert_eq!(
        remove(text, "Rect", "cornerRadius"),
        "<Rect\n  x={1}\n  y={2}\n/>;"
    );
    assert_eq!(
        remove(text, "Rect", "y"),
        "<Rect\n  x={1}\n  cornerRadius={8}\n/>;"
    );
    // Windows line breaks too.
    let crlf = "<Rect\r\n  x={1}\r\n  y={2}\r\n/>;";
    assert_eq!(remove(crlf, "Rect", "x"), "<Rect\r\n  y={2}\r\n/>;");
}

#[test]
fn parses_tells_a_text_with_syntax_errors() {
    assert!(parses("export const A = () => <Rect x={1} />;"));
    assert!(!parses("export const A = () => <Rect x={1} // />;"));
}

#[test]
fn comments_and_line_breaks_around_an_attribute_survive() {
    let text = "<Rect\n  // the box\n  width={10}\n  /* keep */ height={20}\n/>;";
    assert_eq!(
        remove(text, "Rect", "width"),
        "<Rect\n  // the box\n  /* keep */ height={20}\n/>;"
    );
    assert_eq!(
        remove(text, "Rect", "height"),
        "<Rect\n  // the box\n  width={10}\n  /* keep */\n/>;"
    );
}

#[test]
fn a_style_property_goes_with_its_comma() {
    let text = "<Text style={{ fontSize: 48, fontWeight: 700, fill: 'red' }}>A</Text>;";
    assert_eq!(
        remove(text, "Text", "style.fontSize"),
        "<Text style={{ fontWeight: 700, fill: 'red' }}>A</Text>;"
    );
    assert_eq!(
        remove(text, "Text", "style.fontWeight"),
        "<Text style={{ fontSize: 48, fill: 'red' }}>A</Text>;"
    );
    assert_eq!(
        remove(text, "Text", "style.fill"),
        "<Text style={{ fontSize: 48, fontWeight: 700 }}>A</Text>;"
    );
}

#[test]
fn the_only_style_property_leaves_an_empty_object() {
    assert_eq!(
        remove(
            "<Text style={{ fontSize: 48 }}>A</Text>;",
            "Text",
            "style.fontSize"
        ),
        "<Text style={{}}>A</Text>;"
    );
    assert_eq!(
        remove(
            "<Text style={{\n  fontSize: 48,\n}}>A</Text>;",
            "Text",
            "style.fontSize"
        ),
        "<Text style={{}}>A</Text>;"
    );
    // A comment in the object stays.
    assert_eq!(
        remove(
            "<Text style={{ /* big */ fontSize: 48 }}>A</Text>;",
            "Text",
            "style.fontSize"
        ),
        "<Text style={{ /* big */  }}>A</Text>;"
    );
}

#[test]
fn style_properties_on_their_own_lines_keep_the_layout() {
    let text = "<Text\n  style={{\n    fontSize: 48,\n    fontWeight: 700,\n  }}\n>A</Text>;";
    assert_eq!(
        remove(text, "Text", "style.fontSize"),
        "<Text\n  style={{\n    fontWeight: 700,\n  }}\n>A</Text>;"
    );
    assert_eq!(
        remove(text, "Text", "style.fontWeight"),
        "<Text\n  style={{\n    fontSize: 48,\n  }}\n>A</Text>;"
    );
    // A comment before the last property stays, with the comma before it.
    let commented = "<Text style={{ fontSize: 48, /* w */ fontWeight: 700 }}>A</Text>;";
    assert_eq!(
        remove(commented, "Text", "style.fontWeight"),
        "<Text style={{ fontSize: 48, /* w */  }}>A</Text>;"
    );
    // A quoted key is the same key.
    assert_eq!(
        remove(
            "<Text style={{ 'fontSize': 48, fill: 'red' }}>A</Text>;",
            "Text",
            "style.fontSize"
        ),
        "<Text style={{ fill: 'red' }}>A</Text>;"
    );
}

#[test]
fn a_call_site_attribute_goes_and_the_parameter_default_applies() {
    let component = "export function PriceTag({ x = 3 }) {\n  return <Rect x={x} />;\n}\n";
    let scene = "export const Scene = () => (\n  <PriceTag x={40} label=\"Sale\" />\n);\n";
    let call = element(scene, "<PriceTag");
    let ctx = ResolveContext {
        primitive: true,
        component_props: false,
        candidate_frames: vec![0.0],
        received: Some(EditValue::Number(40.0)),
        render_count: 1,
    };
    let target = resolve_through(
        component,
        &element(component, "<Rect"),
        "x",
        &ctx,
        Some((scene, &call)),
    );
    let EditTarget::PassThrough {
        call_site, param, ..
    } = &target
    else {
        panic!("expected PassThrough, got {target:?}");
    };
    let edit = removal(scene, call_site, param).unwrap();
    assert_eq!(
        apply(scene, &[edit]).unwrap(),
        "export const Scene = () => (\n  <PriceTag label=\"Sale\" />\n);\n"
    );
}

#[test]
fn nothing_to_remove() {
    // Children are not an attribute; an absent prop has nothing written.
    assert!(!removable(
        "<Text maxWidth={4}>Sale</Text>;",
        "Text",
        "children"
    ));
    assert!(!removable(
        "<Text maxWidth={4}>Sale</Text>;",
        "Text",
        "fontSize"
    ));
    assert!(!removable(
        "<Text style={{ fill: 'red' }}>A</Text>;",
        "Text",
        "style.fontSize"
    ));
    assert!(!removable(
        "<Text style={s}>A</Text>;",
        "Text",
        "style.fontSize"
    ));
    // A spread after it may set it again; the style's spread too.
    assert!(!removable("<Rect x={1} {...p} />;", "Rect", "x"));
    assert!(!removable(
        "<Text style={{ fontSize: 4, ...s }}>A</Text>;",
        "Text",
        "style.fontSize"
    ));
    // Written twice: removing one leaves the other.
    assert!(!removable("<Rect x={1} x={2} />;", "Rect", "x"));
    assert!(!removable(
        "<Text style={{ fontSize: 4, fontSize: 5 }}>A</Text>;",
        "Text",
        "style.fontSize"
    ));
    // A file with syntax errors is never written.
    let broken = "<Rect x={1} />;\nconst = ;";
    assert!(
        removal(
            broken,
            &ElementRef {
                start: 0,
                end_of_attributes: 11,
                children: None
            },
            "x"
        )
        .is_none()
    );
}

#[test]
fn a_removal_applies_beside_other_edits() {
    let text = "<Rect x={1} y={2} />;";
    let edits = [
        Edit::Remove { start: 5, end: 11 },
        Edit::Replace {
            start: 15,
            end: 16,
            value: EditValue::Number(9.0),
            form: celesta_source_edit::LiteralForm::Number,
        },
    ];
    assert_eq!(apply(text, &edits).unwrap(), "<Rect y={9} />;");
    let overlapping = [
        Edit::Remove { start: 5, end: 11 },
        Edit::Remove { start: 8, end: 12 },
    ];
    assert_eq!(apply(text, &overlapping), Err(ApplyError::Overlap));
}
