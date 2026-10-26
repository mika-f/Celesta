use celesta_source_edit::{
    AddLock, ElementRef, ImportLock, NewElement, NewKind, Timing, accepts_children, add_child,
    add_lock, add_locks, apply, element_at, parses,
};

/// The element whose `<` is the first `needle` in `text` (ASCII only).
fn element(text: &str, needle: &str) -> ElementRef {
    let (line, column) = position(text, needle);
    element_at(text, line, column).unwrap()
}

/// The 1-based line and column of the first `needle` in `text` (ASCII only).
fn position(text: &str, needle: &str) -> (u32, u32) {
    let offset = text.find(needle).unwrap();
    let before = &text[..offset];
    let line = before.matches('\n').count() as u32 + 1;
    let column = (offset - before.rfind('\n').map_or(0, |i| i + 1)) as u32 + 1;
    (line, column)
}

/// `text` with `new` added under the first `needle`.
fn add(text: &str, needle: &str, new: &NewElement, timing: Option<&Timing>) -> String {
    let added = add_child(text, &element(text, needle), new, timing).unwrap();
    let after = apply(text, &added.edits).unwrap();
    assert!(parses(&after), "{after}");
    after
}

fn rect() -> NewElement {
    NewElement::Rect {
        x: 120.0,
        y: 80.0,
        width: 320.0,
        height: 180.0,
    }
}

const RECT: &str = "<Rect x={120} y={80} width={320} height={180} fill=\"#4F7CFF\" />";

const IMPORTS: &str = "import { Group, Image, Rect, Sequence, Text, Video } from '@celesta/react';\nimport { Circle, Ellipse } from '@celesta/shapes';\n\n";

const TIMING: Timing = Timing {
    from: 45,
    duration: 75,
};

#[test]
fn adds_after_the_last_child() {
    let text = format!(
        "{IMPORTS}export const Scene = () => (\n  <Group>\n    <Rect width={{10}} />\n  </Group>\n);\n"
    );
    assert_eq!(
        add(&text, "<Group", &rect(), None),
        format!(
            "{IMPORTS}export const Scene = () => (\n  <Group>\n    <Rect width={{10}} />\n    {RECT}\n  </Group>\n);\n"
        )
    );

    // After a text child and a comment too.
    let text = format!(
        "{IMPORTS}export const Scene = () => (\n  <Group>\n    <Text>Hi</Text>\n    {{/* last */}}\n  </Group>\n);\n"
    );
    assert_eq!(
        add(&text, "<Group", &rect(), None),
        format!(
            "{IMPORTS}export const Scene = () => (\n  <Group>\n    <Text>Hi</Text>\n    {{/* last */}}\n    {RECT}\n  </Group>\n);\n"
        )
    );
}

#[test]
fn adds_into_an_empty_parent() {
    let text = format!("{IMPORTS}export const Scene = () => (\n  <Group></Group>\n);\n");
    assert_eq!(
        add(&text, "<Group", &rect(), None),
        format!("{IMPORTS}export const Scene = () => (\n  <Group>\n    {RECT}\n  </Group>\n);\n")
    );

    // Only whitespace between the tags.
    let text = format!("{IMPORTS}export const Scene = () => (\n  <Group>\n  </Group>\n);\n");
    assert_eq!(
        add(&text, "<Group", &rect(), None),
        format!("{IMPORTS}export const Scene = () => (\n  <Group>\n    {RECT}\n  </Group>\n);\n")
    );
}

#[test]
fn opens_a_self_closing_parent() {
    let text = format!("{IMPORTS}const a = (\n\t<Group>\n\t\t<Group />\n\t</Group>\n);\n");
    assert_eq!(
        add(&text, "<Group />", &rect(), None),
        format!(
            "{IMPORTS}const a = (\n\t<Group>\n\t\t<Group>\n\t\t\t{RECT}\n\t\t</Group>\n\t</Group>\n);\n"
        )
    );

    // With attributes, and a member tag closed by its name.
    let text = "import * as C from '@celesta/react';\n\nconst a = (\n  <C.Group>\n    <C.Group x={10}/>\n  </C.Group>\n);\n";
    assert_eq!(
        add(text, "<C.Group x", &rect(), None),
        "import * as C from '@celesta/react';\n\nconst a = (\n  <C.Group>\n    <C.Group x={10}>\n      <C.Rect x={120} y={80} width={320} height={180} fill=\"#4F7CFF\" />\n    </C.Group>\n  </C.Group>\n);\n"
    );
}

#[test]
fn indents_with_tabs_and_spaces() {
    let two = format!("{IMPORTS}const a = (\n  <Group>\n    <Rect />\n  </Group>\n);\n");
    assert_eq!(
        add(&two, "<Group", &rect(), None),
        format!("{IMPORTS}const a = (\n  <Group>\n    <Rect />\n    {RECT}\n  </Group>\n);\n")
    );

    let four = format!("{IMPORTS}const a = (\n    <Group>\n        <Rect />\n    </Group>\n);\n");
    assert_eq!(
        add(&four, "<Group", &rect(), None),
        format!(
            "{IMPORTS}const a = (\n    <Group>\n        <Rect />\n        {RECT}\n    </Group>\n);\n"
        )
    );

    let tabs = format!("{IMPORTS}const a = (\n\t<Group>\n\t\t<Rect />\n\t</Group>\n);\n");
    assert_eq!(
        add(&tabs, "<Group", &rect(), None),
        format!("{IMPORTS}const a = (\n\t<Group>\n\t\t<Rect />\n\t\t{RECT}\n\t</Group>\n);\n")
    );

    // A parent after code on its line is indented from that line.
    let inline = format!("{IMPORTS}function A() {{\n  return <Group>\n  </Group>;\n}}\n");
    assert_eq!(
        add(&inline, "<Group", &rect(), None),
        format!("{IMPORTS}function A() {{\n  return <Group>\n    {RECT}\n  </Group>;\n}}\n")
    );
}

#[test]
fn writes_each_kind_s_literals() {
    let text = format!("{IMPORTS}const a = (\n  <Group>\n  </Group>\n);\n");
    let written = |new: NewElement| {
        let after = add(&text, "<Group", &new, None);
        let line = after.lines().find(|l| l.starts_with("    <")).unwrap();
        line.trim_start().to_string()
    };
    assert_eq!(written(rect()), RECT);
    assert_eq!(
        written(NewElement::Ellipse {
            x: 12.345,
            y: -4.0,
            width: 200.0,
            height: 100.5,
        }),
        "<Ellipse x={12.35} y={-4} width={200} height={100.5} fill=\"#4F7CFF\" />"
    );
    assert_eq!(
        written(NewElement::Circle {
            x: 300.0,
            y: 200.0,
            radius: 100.0,
        }),
        "<Circle x={300} y={200} anchorX={0.5} anchorY={0.5} radius={100} fill=\"#4F7CFF\" />"
    );
    assert_eq!(
        written(NewElement::Text { x: 10.0, y: 20.0 }),
        "<Text x={10} y={20} fontSize={48} color=\"#4F7CFF\">Text</Text>"
    );
    assert_eq!(
        written(NewElement::Image {
            src: "assets/a.png".to_string(),
            x: 1.0,
            y: 2.0,
            width: 400.0,
        }),
        "<Image src=\"assets/a.png\" x={1} y={2} width={400} />"
    );
    assert_eq!(
        written(NewElement::Video {
            src: "assets/clip.mp4".to_string(),
            x: 0.0,
            y: 0.0,
            width: 400.0,
        }),
        "<Video src=\"assets/clip.mp4\" x={0} y={0} width={400} />"
    );
    // A path the attribute string can't hold goes in a container.
    assert_eq!(
        written(NewElement::Image {
            src: "assets/a\"b.png".to_string(),
            x: 0.0,
            y: 0.0,
            width: 400.0,
        }),
        "<Image src={\"assets/a\\\"b.png\"} x={0} y={0} width={400} />"
    );

    assert_eq!(rect().kind(), NewKind::Rect);
    assert_eq!(NewKind::Ellipse.tag(), "Ellipse");
    assert_eq!(NewKind::Ellipse.module(), "@celesta/shapes");
    assert_eq!(NewKind::Circle.module(), "@celesta/shapes");
    for kind in [NewKind::Rect, NewKind::Text, NewKind::Image, NewKind::Video] {
        assert_eq!(kind.module(), "@celesta/react", "{kind:?}");
    }
}

#[test]
fn wraps_in_a_sequence() {
    let text = format!("{IMPORTS}const a = (\n  <Group>\n    <Rect />\n  </Group>\n);\n");
    assert_eq!(
        add(&text, "<Group", &rect(), Some(&TIMING)),
        format!(
            "{IMPORTS}const a = (\n  <Group>\n    <Rect />\n    <Sequence from={{45}} durationInFrames={{75}}>\n      {RECT}\n    </Sequence>\n  </Group>\n);\n"
        )
    );
    assert_eq!(
        add(&text, "<Group", &rect(), None),
        format!("{IMPORTS}const a = (\n  <Group>\n    <Rect />\n    {RECT}\n  </Group>\n);\n")
    );

    // Tabs, into a self-closing parent.
    let text = format!("{IMPORTS}const a = (\n\t<Group>\n\t\t<Group />\n\t</Group>\n);\n");
    assert_eq!(
        add(&text, "<Group />", &rect(), Some(&TIMING)),
        format!(
            "{IMPORTS}const a = (\n\t<Group>\n\t\t<Group>\n\t\t\t<Sequence from={{45}} durationInFrames={{75}}>\n\t\t\t\t{RECT}\n\t\t\t</Sequence>\n\t\t</Group>\n\t</Group>\n);\n"
        )
    );
}

#[test]
fn imports_from_react_and_shapes() {
    let body = "\nconst a = (\n  <Group>\n  </Group>\n);\n";
    let ellipse = NewElement::Ellipse {
        x: 0.0,
        y: 0.0,
        width: 10.0,
        height: 10.0,
    };
    let text = format!("import {{ Group }} from '@celesta/react';\n{body}");
    let after = add(&text, "<Group", &ellipse, None);
    assert!(
        after.starts_with(
            "import { Group } from '@celesta/react';\nimport { Ellipse } from '@celesta/shapes';\n\nconst a = (\n  <Group>\n    <Ellipse "
        ),
        "{after}"
    );

    // Wrapped: `Sequence` from one module, `Ellipse` from the other.
    let after = add(&text, "<Group", &ellipse, Some(&TIMING));
    assert!(
        after.starts_with(
            "import { Group, Sequence } from '@celesta/react';\nimport { Ellipse } from '@celesta/shapes';\n"
        ),
        "{after}"
    );

    // Into an existing `@celesta/shapes` import.
    let text = format!(
        "import {{ Group }} from '@celesta/react';\nimport {{ Circle }} from '@celesta/shapes';\n{body}"
    );
    let after = add(&text, "<Group", &ellipse, None);
    assert!(
        after.starts_with(
            "import { Group } from '@celesta/react';\nimport { Circle, Ellipse } from '@celesta/shapes';\n"
        ),
        "{after}"
    );
}

#[test]
fn rect_and_sequence_both_new_land_in_one_import() {
    let body = "\nconst a = (\n  <Group>\n  </Group>\n);\n";
    let text = format!("import {{ Group, Text }} from '@celesta/react';\n{body}");
    let added = add_child(&text, &element(&text, "<Group"), &rect(), Some(&TIMING)).unwrap();
    // The import edit and the insertion.
    assert_eq!(added.edits.len(), 2);
    let after = apply(&text, &added.edits).unwrap();
    assert!(parses(&after), "{after}");
    assert!(
        after.starts_with("import { Group, Rect, Sequence, Text } from '@celesta/react';\n\n"),
        "{after}"
    );

    // A file importing only `react` gets one new line, in its style.
    let text = format!("import React from 'react';\n{body}");
    let after = add(&text, "<Group", &rect(), Some(&TIMING));
    assert!(
        after.starts_with(
            "import React from 'react';\nimport { Rect, Sequence } from '@celesta/react';\n\n"
        ),
        "{after}"
    );
    let text = format!("import React from \"react\"\n{body}");
    let after = add(&text, "<Group", &rect(), Some(&TIMING));
    assert!(
        after.starts_with(
            "import React from \"react\"\nimport { Rect, Sequence } from \"@celesta/react\"\n\n"
        ),
        "{after}"
    );
}

#[test]
fn already_imported_needs_nothing() {
    let text = format!("{IMPORTS}const a = (\n  <Group>\n  </Group>\n);\n");
    let added = add_child(&text, &element(&text, "<Group"), &rect(), Some(&TIMING)).unwrap();
    assert_eq!(added.edits.len(), 1);
    let ellipse = NewElement::Ellipse {
        x: 0.0,
        y: 0.0,
        width: 10.0,
        height: 10.0,
    };
    let added = add_child(&text, &element(&text, "<Group"), &ellipse, Some(&TIMING)).unwrap();
    assert_eq!(added.edits.len(), 1);

    // An alias is written as imported.
    let text = "import { Group, Rect as R, Sequence as S } from '@celesta/react';\n\nconst a = <Group></Group>;\n";
    let after = add(text, "<Group", &rect(), Some(&TIMING));
    assert!(
        after.contains(
            "<Group>\n  <S from={45} durationInFrames={75}>\n    <R x={120} y={80} width={320} height={180} fill=\"#4F7CFF\" />\n  </S>\n</Group>"
        ),
        "{after}"
    );
}

#[test]
fn a_namespace_import_writes_members() {
    let text = "import * as C from '@celesta/react';\n\nconst a = (\n  <C.Group>\n    <C.Rect />\n  </C.Group>\n);\n";
    assert_eq!(
        add(text, "<C.Group", &rect(), Some(&TIMING)),
        "import * as C from '@celesta/react';\n\nconst a = (\n  <C.Group>\n    <C.Rect />\n    <C.Sequence from={45} durationInFrames={75}>\n      <C.Rect x={120} y={80} width={320} height={180} fill=\"#4F7CFF\" />\n    </C.Sequence>\n  </C.Group>\n);\n"
    );
}

#[test]
fn a_clashing_binding_locks() {
    let text = "import { Group, Text } from '@celesta/react';\n\nconst Rect = () => null;\n\nconst a = (\n  <Group>\n  </Group>\n);\n";
    let parent = element(text, "<Group");
    let lock = AddLock::Import(ImportLock::Clash {
        name: "Rect".to_string(),
    });
    assert_eq!(add_child(text, &parent, &rect(), None), Err(lock.clone()));
    assert_eq!(lock.text(), "`Rect` here is not Celesta's");
    assert_eq!(
        add_lock(text, &parent, NewKind::Rect, false),
        Some(lock.clone())
    );
    assert_eq!(add_lock(text, &parent, NewKind::Rect, true), Some(lock));
    assert_eq!(add_lock(text, &parent, NewKind::Text, false), None);
    assert_eq!(add_lock(text, &parent, NewKind::Text, true), None);

    // A clashing `Sequence` locks only a wrapped addition.
    let text = "import { Group } from '@celesta/react';\n\nfunction Sequence() {}\n\nconst a = <Group></Group>;\n";
    let parent = element(text, "<Group");
    let lock = AddLock::Import(ImportLock::Clash {
        name: "Sequence".to_string(),
    });
    assert_eq!(
        add_lock(text, &parent, NewKind::Ellipse, true),
        Some(lock.clone())
    );
    assert_eq!(add_child(text, &parent, &rect(), Some(&TIMING)), Err(lock));
    assert_eq!(add_lock(text, &parent, NewKind::Ellipse, false), None);
}

#[test]
fn every_kind_s_lock_at_once() {
    let kinds = [
        NewKind::Rect,
        NewKind::Ellipse,
        NewKind::Circle,
        NewKind::Text,
        NewKind::Image,
        NewKind::Video,
    ];
    let clash = |name: &str| {
        Some(AddLock::Import(ImportLock::Clash {
            name: name.to_string(),
        }))
    };
    let locks = |each: [Option<AddLock>; 6]| Some(kinds.into_iter().zip(each).collect::<Vec<_>>());
    let none = || [None, None, None, None, None, None];

    for text in [
        format!("{IMPORTS}const a = <Group></Group>;\n"),
        "const a = <Group />;\n".to_string(),
    ] {
        let parent = element(&text, "<Group");
        for wrapped in [false, true] {
            assert_eq!(add_locks(&text, &parent, &kinds, wrapped), locks(none()));
        }
    }

    // `Rect` clashes; so does `Sequence`, for a wrapped addition only, and
    // a wrapped `Rect` names `Rect`, imported first.
    let text = "import { Group } from '@celesta/react';\n\nconst Rect = () => null;\nfunction Sequence() {}\n\nconst a = (\n  <Group>\n  </Group>\n);\n";
    let parent = element(text, "<Group");
    assert_eq!(
        add_locks(text, &parent, &kinds, false),
        locks([clash("Rect"), None, None, None, None, None])
    );
    let sequence = || clash("Sequence");
    assert_eq!(
        add_locks(text, &parent, &kinds, true),
        locks([
            clash("Rect"),
            sequence(),
            sequence(),
            sequence(),
            sequence(),
            sequence()
        ])
    );

    // `Circle` clashes, from the other module.
    let text = "import { Group } from '@celesta/react';\n\nconst Circle = 1;\n\nconst a = <Group></Group>;\n";
    let parent = element(text, "<Group");
    for wrapped in [false, true] {
        assert_eq!(
            add_locks(text, &parent, &kinds, wrapped),
            locks([None, None, clash("Circle"), None, None, None])
        );
        for kind in kinds {
            let each = add_locks(text, &parent, &kinds, wrapped)
                .unwrap()
                .into_iter()
                .find(|(of, _)| *of == kind)
                .and_then(|(_, lock)| lock);
            assert_eq!(add_lock(text, &parent, kind, wrapped), each);
        }
    }
    // No children region: every kind's lock is `NoChildrenRegion`.
    let text = format!("{IMPORTS}const Box = ({{ children }}) => <Group>{{children}}</Group>;\n");
    let parent = element(&text, "<Group");
    assert_eq!(add_locks(&text, &parent, &kinds, false), None);
    // Nor in a text that does not parse.
    let text = format!("{IMPORTS}const a = <Group></Group>;\n");
    let parent = element(&text, "<Group");
    let broken = format!("{text}<");
    assert_eq!(add_locks(&broken, &parent, &kinds, true), None);
    assert_eq!(
        add_lock(&broken, &parent, NewKind::Rect, true),
        Some(AddLock::NoChildrenRegion)
    );
}

#[test]
fn a_children_prop_is_not_a_region() {
    for text in [
        "const Box = ({ children }) => <Group>{children}</Group>;\n",
        "function Box(props) {\n  return <Group>{props.children}</Group>;\n}\n",
        "const Box = ({ content }: { content: React.ReactNode }) => (\n  <Group>\n    {content}\n  </Group>\n);\n",
    ] {
        let text = format!("{IMPORTS}{text}");
        let parent = element(&text, "<Group");
        assert!(!accepts_children(&text, &parent), "{text}");
        assert_eq!(
            add_lock(&text, &parent, NewKind::Rect, false),
            Some(AddLock::NoChildrenRegion),
            "{text}"
        );
        assert_eq!(
            add_child(&text, &parent, &rect(), None),
            Err(AddLock::NoChildrenRegion),
            "{text}"
        );
    }
    assert!(!AddLock::NoChildrenRegion.text().is_empty());
}

#[test]
fn a_map_expression_is_a_region() {
    let text = format!(
        "{IMPORTS}const Box = ({{ items }}) => (\n  <Group>\n    {{items.map((item) => (\n      <Rect key={{item}} />\n    ))}}\n  </Group>\n);\n"
    );
    assert!(accepts_children(&text, &element(&text, "<Group")));
    assert_eq!(
        add(&text, "<Group", &rect(), None),
        format!(
            "{IMPORTS}const Box = ({{ items }}) => (\n  <Group>\n    {{items.map((item) => (\n      <Rect key={{item}} />\n    ))}}\n    {RECT}\n  </Group>\n);\n"
        )
    );
}

#[test]
fn other_children_are_regions() {
    for text in [
        // A prop beside an element.
        "const Box = ({ children }) => (\n  <Group>\n    <Rect />\n    {children}\n  </Group>\n);\n",
        // A local value, not a prop.
        "const logo = <Rect />;\nconst a = <Group>{logo}</Group>;\n",
        // Self-closing and empty parents.
        "const a = <Group />;\n",
        "const a = <Group></Group>;\n",
    ] {
        let text = format!("{IMPORTS}{text}");
        assert!(accepts_children(&text, &element(&text, "<Group")), "{text}");
        add(&text, "<Group", &rect(), None);
    }
}

#[test]
fn reports_the_new_element_s_position() {
    let body = "\nexport const Scene = () => (\n  <Group>\n    <Text>Hi</Text>\n  </Group>\n);\n";
    let text = format!("import React from 'react';\n{body}");
    for timing in [None, Some(&TIMING)] {
        let added = add_child(&text, &element(&text, "<Group"), &rect(), timing).unwrap();
        let after = apply(&text, &added.edits).unwrap();
        assert!(parses(&after), "{after}");
        assert!(after.contains("\nimport { Rect"), "{after}");
        assert_eq!(
            (added.line, added.column),
            position(&after, "<Rect x={120}"),
            "{after}"
        );
        let found = element_at(&after, added.line, added.column).unwrap();
        assert_eq!(found.start as usize, after.find("<Rect x={120}").unwrap());
    }

    // In a self-closing parent.
    let text = format!("{IMPORTS}const a = (\n  <Group>\n    <Group />\n  </Group>\n);\n");
    let added = add_child(&text, &element(&text, "<Group />"), &rect(), Some(&TIMING)).unwrap();
    let after = apply(&text, &added.edits).unwrap();
    assert_eq!(
        (added.line, added.column),
        position(&after, "<Rect x={120}"),
        "{after}"
    );
}
