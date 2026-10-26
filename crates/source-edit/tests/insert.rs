use celesta_source_edit::{
    EditValue, ElementRef, WrapLock, apply, element_at, indent_unit, parses, wrap_element,
};

/// The element whose `<` is the first `needle` in `text` (ASCII only).
fn element(text: &str, needle: &str) -> ElementRef {
    let offset = text.find(needle).unwrap();
    let before = &text[..offset];
    let line = before.matches('\n').count() as u32 + 1;
    let column = (offset - before.rfind('\n').map_or(0, |i| i + 1)) as u32 + 1;
    element_at(text, line, column).unwrap()
}

/// `text` with the first `needle` wrapped in `<{tag} from={45}
/// durationInFrames={75}>`.
fn wrap_as(text: &str, needle: &str, tag: &str) -> String {
    let edits = wrap_element(
        text,
        &element(text, needle),
        tag,
        &[
            ("from", EditValue::Number(45.0)),
            ("durationInFrames", EditValue::Number(75.0)),
        ],
    )
    .unwrap();
    let after = apply(text, &edits).unwrap();
    assert!(parses(&after), "{after}");
    after
}

fn wrap(text: &str, needle: &str) -> String {
    wrap_as(text, needle, "Sequence")
}

const IMPORT: &str = "import { G, Rect, Sequence } from '@celesta/react';\n\n";

#[test]
fn wraps_a_child_element_with_spaces() {
    let text = format!(
        "{IMPORT}export const Scene = () => (\n  <G>\n    <Rect width={{10}} />\n  </G>\n);\n"
    );
    assert_eq!(
        wrap(&text, "<Rect"),
        format!(
            "{IMPORT}export const Scene = () => (\n  <G>\n    <Sequence from={{45}} durationInFrames={{75}}>\n      <Rect width={{10}} />\n    </Sequence>\n  </G>\n);\n"
        )
    );

    // Four spaces a level.
    let text = "const a = (\n    <G>\n        <Rect />\n    </G>\n);\n";
    assert_eq!(
        wrap(text, "<Rect"),
        "const a = (\n    <G>\n        <Sequence from={45} durationInFrames={75}>\n            <Rect />\n        </Sequence>\n    </G>\n);\n"
    );
}

#[test]
fn wraps_with_tabs() {
    let text = "const a = (\n\t<G>\n\t\t<Rect width={10} />\n\t</G>\n);\n";
    assert_eq!(
        wrap_as(text, "<Rect", "Seq"),
        "const a = (\n\t<G>\n\t\t<Seq from={45} durationInFrames={75}>\n\t\t\t<Rect width={10} />\n\t\t</Seq>\n\t</G>\n);\n"
    );
}

#[test]
fn wraps_a_multi_line_element_re_indenting_every_line() {
    let text =
        "const a = (\n  <G>\n    <Rect\n      width={10}\n      height={20}\n    />\n  </G>\n);\n";
    assert_eq!(
        wrap(text, "<Rect"),
        "const a = (\n  <G>\n    <Sequence from={45} durationInFrames={75}>\n      <Rect\n        width={10}\n        height={20}\n      />\n    </Sequence>\n  </G>\n);\n"
    );

    // Children too; an empty line stays empty.
    let text = "const a = (\n  <G>\n    <Group>\n      <Rect />\n\n      <Text>hi</Text>\n    </Group>\n  </G>\n);\n";
    assert_eq!(
        wrap(text, "<Group"),
        "const a = (\n  <G>\n    <Sequence from={45} durationInFrames={75}>\n      <Group>\n        <Rect />\n\n        <Text>hi</Text>\n      </Group>\n    </Sequence>\n  </G>\n);\n"
    );
}

#[test]
fn re_indenting_skips_lines_inside_a_template_literal() {
    let text =
        "const a = (\n  <G>\n    <Text>\n      {`one\ntwo\n  three`}\n    </Text>\n  </G>\n);\n";
    assert_eq!(
        wrap(text, "<Text"),
        "const a = (\n  <G>\n    <Sequence from={45} durationInFrames={75}>\n      <Text>\n        {`one\ntwo\n  three`}\n      </Text>\n    </Sequence>\n  </G>\n);\n"
    );

    // The closing backtick's line is inside the string too.
    let text = "const a = (\n  <G>\n    <Text>\n      {`one\n`}\n    </Text>\n  </G>\n);\n";
    assert_eq!(
        wrap(text, "<Text"),
        "const a = (\n  <G>\n    <Sequence from={45} durationInFrames={75}>\n      <Text>\n        {`one\n`}\n      </Text>\n    </Sequence>\n  </G>\n);\n"
    );

    // A JSX attribute string holds its line breaks as written.
    let text = "const a = (\n  <G>\n    <Rect\n      title=\"a\nb\"\n    />\n  </G>\n);\n";
    assert_eq!(
        wrap(text, "<Rect"),
        "const a = (\n  <G>\n    <Sequence from={45} durationInFrames={75}>\n      <Rect\n        title=\"a\nb\"\n      />\n    </Sequence>\n  </G>\n);\n"
    );

    // Code inside a substitution is re-indented.
    let text = "const a = (\n  <G>\n    <Text>\n      {`one ${\n        b\n      }`}\n    </Text>\n  </G>\n);\n";
    assert_eq!(
        wrap(text, "<Text"),
        "const a = (\n  <G>\n    <Sequence from={45} durationInFrames={75}>\n      <Text>\n        {`one ${\n          b\n        }`}\n      </Text>\n    </Sequence>\n  </G>\n);\n"
    );
}

#[test]
fn an_element_sharing_its_line_is_wrapped_inline() {
    let text = "const a = <G><Rect /></G>;\n";
    assert_eq!(
        wrap(text, "<Rect"),
        "const a = <G><Sequence from={45} durationInFrames={75}><Rect /></Sequence></G>;\n"
    );

    // Code after it on its last line counts too.
    let text = "const a = (\n  <G>\n    <Rect\n    /> text\n  </G>\n);\n";
    assert_eq!(
        wrap(text, "<Rect"),
        "const a = (\n  <G>\n    <Sequence from={45} durationInFrames={75}><Rect\n    /></Sequence> text\n  </G>\n);\n"
    );
}

#[test]
fn refuses_a_root_or_a_prop_value() {
    for text in [
        "function A() {\n  return <Rect />;\n}\n",
        "const A = () => <Rect />;\n",
        "const a = <G icon={<Rect />} />;\n",
    ] {
        assert_eq!(
            wrap_element(text, &element(text, "<Rect"), "Sequence", &[]),
            Err(WrapLock::NotAChild),
            "{text}"
        );
    }
}

#[test]
fn indent_unit_reads_tabs_or_one_level_of_spaces() {
    let at = |text: &str, needle: &str| indent_unit(text, text.find(needle).unwrap() as u32);

    let two = "const a = (\n  <G>\n    <Rect />\n  </G>\n);\n";
    assert_eq!(at(two, "<Rect"), "  ");
    let four = "const a = (\n    <G>\n        <Rect />\n    </G>\n);\n";
    assert_eq!(at(four, "<Rect"), "    ");
    let tabs = "const a = (\n\t<G>\n\t\t<Rect />\n\t</G>\n);\n";
    assert_eq!(at(tabs, "<Rect"), "\t");

    // At the top level, from the lines below.
    let top = "<G>\n    <Rect />\n</G>\n";
    assert_eq!(at(top, "<G>"), "    ");
}
