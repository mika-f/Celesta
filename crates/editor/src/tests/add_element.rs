//! Tests of an addition's write (`add_files` as one history entry) and of
//! selecting the added layer after the reload (`added_fate`,
//! `added_confirmed`).

use super::*;
use crate::add_element::{
    AddComponent, AddedFate, Addition, PendingSelect, add_files, added_confirmed, added_fate,
    parent_facts, refused_model_file,
};
use crate::preview::AddedUnder;
use celesta_source_edit::{EditValue, NewElement, NewKind, Timing};

/// A scene whose `<Group>` at 4:3 holds a `<Text>`.
const SCENE: &str = "import { Group, Text } from '@celesta.studio/react';\n\nexport const Intro = () => (\n  <Group>\n    <Text>Hi</Text>\n  </Group>\n);\n";

/// [`SCENE`] after adding [`rect`] timed from 45 for 75 frames.
const ADDED: &str = "import { Group, Rect, Sequence, Text } from '@celesta.studio/react';\n\nexport const Intro = () => (\n  <Group>\n    <Text>Hi</Text>\n    <Sequence from={45} durationInFrames={75}>\n      <Rect x={120} y={80} width={320} height={180} fill=\"#4F7CFF\" />\n    </Sequence>\n  </Group>\n);\n";

fn scratch(name: &str) -> PathBuf {
    crate::scratch::scratch_dir(&format!("add-{name}"))
}

fn at(file: &Path, line: u32, column: u32) -> SourcePos {
    SourcePos {
        file: file.to_path_buf(),
        line,
        column,
    }
}

fn rect() -> NewElement {
    NewElement::Rect {
        x: 120.0,
        y: 80.0,
        width: 320.0,
        height: 180.0,
    }
}

fn element(new: NewElement) -> Addition {
    Addition::Element(new)
}

/// The flag sample's component, as its manifest offers it.
fn flag() -> AddComponent {
    AddComponent {
        plugin: "flag".into(),
        info: serde_json::from_value(serde_json::json!({
            "name": "Flag", "label": "Flag", "src": "image", "props": { "amplitude": 0.06 },
        }))
        .unwrap(),
    }
}

#[test]
fn a_plugin_component_is_drawn_as_its_kind_and_written_with_its_import() {
    let component = flag();
    assert_eq!(component.kind(), NewKind::Image);
    let drawn = NewElement::Image {
        src: "./portrait.png".into(),
        x: 40.0,
        y: 60.0,
        width: 320.0,
        height: None,
    };
    let dir = scratch("component");
    let file = dir.join("scene.tsx");
    let (files, _) = add_files(
        &texts(&file, SCENE),
        &file,
        &at(&file, 4, 3),
        "Group",
        &Addition::Component(component.element(&drawn)),
        None,
    )
    .unwrap();
    assert_eq!(
        files[0].1,
        "import { Group, Text } from '@celesta.studio/react';\nimport { Flag } from '@celesta-plugin/flag';\n\nexport const Intro = () => (\n  <Group>\n    <Text>Hi</Text>\n    <Flag src=\"./portrait.png\" x={40} y={60} width={320} amplitude={0.06} />\n  </Group>\n);\n"
    );

    // Without a file, a component is drawn as a box.
    let boxed = AddComponent {
        info: serde_json::from_value(serde_json::json!({ "name": "Panel" })).unwrap(),
        ..flag()
    };
    assert_eq!(boxed.kind(), NewKind::Rect);
    assert_eq!(
        boxed.element(&rect()).attributes,
        [
            ("x".to_owned(), EditValue::Number(120.0)),
            ("y".to_owned(), EditValue::Number(80.0)),
            ("width".to_owned(), EditValue::Number(320.0)),
            ("height".to_owned(), EditValue::Number(180.0)),
        ]
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn model_components_draw_as_images_and_check_the_file() {
    let model = AddComponent {
        plugin: "scenes".into(),
        info: serde_json::from_value(serde_json::json!({
            "name": "Model", "label": "3D model", "src": "model",
            "extensions": ["glb"], "aspect": 1.7777778,
        }))
        .unwrap(),
    };
    assert_eq!(model.kind(), NewKind::Image);
    assert_eq!(
        refused_model_file(&model.info, Path::new("a/scene.GLB")),
        None
    );
    assert_eq!(
        refused_model_file(&model.info, Path::new("a/scene.png")).as_deref(),
        Some("Choose a .glb file")
    );
    assert_eq!(
        refused_model_file(&model.info, Path::new("a/scene")).as_deref(),
        Some("Choose a .glb file")
    );
    let mut several = model.info.clone();
    several.extensions = vec!["glb".into(), "gltf".into(), "obj".into()];
    assert_eq!(
        refused_model_file(&several, Path::new("a/scene.png")).as_deref(),
        Some("Choose a .glb, .gltf, or .obj file")
    );
    several.extensions.pop();
    assert_eq!(
        refused_model_file(&several, Path::new("a/scene.png")).as_deref(),
        Some("Choose a .glb or .gltf file")
    );

    let drawn = NewElement::Image {
        src: "./assets/a.glb".into(),
        x: 10.0,
        y: 20.0,
        width: 960.0,
        height: None,
    };
    assert_eq!(
        model.element(&drawn).attributes,
        [
            ("src".to_owned(), EditValue::String("./assets/a.glb".into())),
            ("x".to_owned(), EditValue::Number(10.0)),
            ("y".to_owned(), EditValue::Number(20.0)),
            ("width".to_owned(), EditValue::Number(960.0)),
        ]
    );
}

const TIMING: Timing = Timing {
    from: 45,
    duration: 75,
};

fn texts(file: &Path, text: &str) -> Texts {
    Texts::from([(file.to_path_buf(), text.to_owned())])
}

#[test]
fn an_addition_writes_one_entry_and_undo_removes_it() {
    let dir = scratch("undo");
    let file = dir.join("intro.tsx");
    fs::write(&file, SCENE).unwrap();
    let parent = at(&file, 4, 3);

    let files = write_all_recorded(vec![file.clone()], |texts| {
        add_files(
            texts,
            &file,
            &parent,
            "Group",
            &element(rect()),
            Some(&TIMING),
        )
        .map(|(files, _)| files)
    })
    .unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(fs::read_to_string(&file).unwrap(), ADDED);

    let mut history = EditHistory::default();
    history.push(HistoryEntry { files });
    assert_eq!(history.undo().unwrap(), Some(vec![file.clone()]));
    assert_eq!(fs::read(&file).unwrap(), SCENE.as_bytes());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn add_files_refuses_a_moved_parent() {
    let dir = scratch("moved");
    let file = dir.join("intro.tsx");
    // The parent's line now holds a `<Rect>`, which takes children too.
    let moved = SCENE.replace("<Group>\n    <Text>Hi</Text>\n  </Group>", "<Rect />");
    fs::write(&file, &moved).unwrap();
    let parent = at(&file, 4, 3);

    let refused = write_all_recorded(vec![file.clone()], |texts| {
        add_files(
            texts,
            &file,
            &parent,
            "Group",
            &element(rect()),
            Some(&TIMING),
        )
        .map(|(files, _)| files)
    });
    assert_eq!(refused, Err("The source changed while editing".to_owned()));
    assert_eq!(fs::read_to_string(&file).unwrap(), moved);

    // Nothing at all there.
    assert_eq!(
        add_files(
            &texts(&file, SCENE),
            &file,
            &at(&file, 2, 1),
            "Group",
            &element(rect()),
            None
        ),
        Err("The source changed while editing".to_owned())
    );
    fs::remove_dir_all(&dir).ok();
}

/// [`SCENE`] with `Group` imported as `G`.
const ALIASED: &str = "import { Group as G, Text } from '@celesta.studio/react';\n\nexport const Intro = () => (\n  <G>\n    <Text>Hi</Text>\n  </G>\n);\n";

#[test]
fn an_aliased_group_is_added_to() {
    let file = PathBuf::from("/project/intro.tsx");
    let read = |_: &Path| Some(ALIASED.to_owned());
    // The runtime reports the group by its own name.
    let report = LayerSourceReport {
        chain: vec![LayerSource {
            name: "Group".to_owned(),
            file: file.clone(),
            line: 4,
            column: 3,
            kind: ElementKind::Primitive,
            layer_id: Some("0".to_owned()),
        }],
        ..lookup_at(&at(&file, 4, 3))
    };
    let facts = parent_facts(read, |_| true, &report, &[], 600, 0);
    let [group] = &facts[..] else {
        panic!("one group: {facts:?}");
    };
    assert_eq!(group.tag, "G");

    let (files, _) = add_files(
        &texts(&file, ALIASED),
        &file,
        &at(&file, 4, 3),
        &group.tag,
        &element(rect()),
        None,
    )
    .unwrap();
    assert!(files[0].1.contains("\n    <Rect x={120}"));
    // Another tag there now.
    assert_eq!(
        add_files(
            &texts(&file, ALIASED),
            &file,
            &at(&file, 4, 3),
            "Group",
            &element(rect()),
            None
        ),
        Err("The source changed while editing".to_owned())
    );
}

/// A group entry of a lookup's chain, at `line`:3 of `file`.
fn group_at(file: &Path, line: u32, layer_id: &str) -> LayerSource {
    LayerSource {
        name: "Group".to_owned(),
        file: file.to_path_buf(),
        line,
        column: 3,
        kind: ElementKind::Primitive,
        layer_id: Some(layer_id.to_owned()),
    }
}

#[test]
fn parent_facts_skips_the_groups_it_cannot_add_to() {
    let scene = PathBuf::from("/project/intro.tsx");
    let outside = PathBuf::from("/elsewhere/intro.tsx");
    let unreadable = PathBuf::from("/project/gone.tsx");
    let props = PathBuf::from("/project/box.tsx");
    let broken = PathBuf::from("/project/broken.tsx");
    // A group whose only child is its component's `children`.
    let boxed = "import { Group } from '@celesta.studio/react';\n\nexport const Box = ({ children }) => (\n  <Group>{children}</Group>\n);\n";
    let read = |path: &Path| match path.to_str()? {
        "/project/intro.tsx" | "/elsewhere/intro.tsx" => Some(SCENE.to_owned()),
        "/project/box.tsx" => Some(boxed.to_owned()),
        "/project/broken.tsx" => Some(format!("{SCENE}const b = <Rect;\n")),
        _ => None,
    };
    let report = LayerSourceReport {
        chain: vec![
            group_at(&props, 4, "box"),
            group_at(&unreadable, 4, "gone"),
            group_at(&outside, 4, "outside"),
            group_at(&broken, 4, "broken"),
            // No element at this position.
            group_at(&scene, 2, "nothing"),
            group_at(&scene, 4, "scene"),
        ],
        ..lookup_at(&at(&props, 4, 3))
    };
    let inside = |file: &Path| file.starts_with("/project");
    let facts = parent_facts(read, inside, &report, &[], 600, 0);
    let ats: Vec<&SourcePos> = facts.iter().map(|group| &group.at).collect();
    assert_eq!(ats, [&at(&scene, 4, 3)]);
    assert_eq!(facts[0].component.as_deref(), Some("Intro"));
}

#[test]
fn add_files_reports_the_position_after_the_import() {
    let file = PathBuf::from("/project/intro.tsx");
    // No import yet: one line goes before the parent.
    let bare = "export const Intro = () => (\n  <Group>\n    <Text>Hi</Text>\n  </Group>\n);\n";
    let (files, position) = add_files(
        &texts(&file, bare),
        &file,
        &at(&file, 2, 3),
        "Group",
        &element(NewElement::Text { x: 10.0, y: 20.0 }),
        None,
    )
    .unwrap();
    let [(written_file, written)] = &files[..] else {
        panic!("one file: {files:?}");
    };
    assert_eq!(written_file, &file);
    assert!(written.starts_with("import { Text } from \"@celesta.studio/react\";\n"));

    // Where the new `<Text` is in the written text, as the runtime counts.
    let start = written.find("<Text x=").unwrap();
    let line = written[..start].matches('\n').count() as u32 + 1;
    let line_start = written[..start].rfind('\n').map_or(0, |i| i + 1);
    let column = written[line_start..start].encode_utf16().count() as u32 + 1;
    assert_eq!(position, at(&file, line, column));
}

fn pending(from: Option<u64>) -> PendingSelect {
    PendingSelect {
        position: at(Path::new("/project/intro.tsx"), 6, 5),
        under: AddedUnder {
            parent: "0.1".to_owned(),
            wrapped: false,
        },
        from,
    }
}

fn answer(parent: &str, layer: Option<&str>) -> (String, Option<String>) {
    (parent.to_owned(), layer.map(str::to_owned))
}

/// A lookup whose element is at `element`.
fn lookup_at(element: &SourcePos) -> LayerSourceReport {
    LayerSourceReport {
        chain: vec![LayerSource {
            name: "Rect".to_owned(),
            file: element.file.clone(),
            line: element.line,
            column: element.column,
            kind: ElementKind::Primitive,
            layer_id: Some("0.1.2".to_owned()),
        }],
        local_frame: 0.0,
        candidate_frames: vec![],
        props: ReceivedProps::default(),
        render_count: 1,
        windows: vec![],
        span: None,
    }
}

#[test]
fn the_added_layer_is_selected_then_confirmed() {
    let waiting = pending(Some(10));
    let found = answer("0.1", Some("0.1.2"));
    // Drawn before the reloaded code.
    assert_eq!(added_fate(&waiting, 9, Some(&found)), AddedFate::Wait);
    // The reload has not applied yet.
    assert_eq!(
        added_fate(&pending(None), 12, Some(&found)),
        AddedFate::Wait
    );
    // A frame that did not ask.
    assert_eq!(added_fate(&waiting, 10, None), AddedFate::Wait);
    assert_eq!(
        added_fate(&waiting, 10, Some(&found)),
        AddedFate::Select("0.1.2".to_owned())
    );
    assert_eq!(
        added_fate(&waiting, 11, Some(&answer("0.1", None))),
        AddedFate::Drop
    );

    let position = &waiting.position;
    assert!(added_confirmed(position, &Ok(Some(lookup_at(position)))));
    let elsewhere = SourcePos {
        line: 5,
        ..position.clone()
    };
    // The selection is then cleared.
    assert!(!added_confirmed(position, &Ok(Some(lookup_at(&elsewhere)))));
    assert!(!added_confirmed(position, &Ok(None)));
    assert!(!added_confirmed(position, &Err("failed".to_owned())));
}

#[test]
fn a_wrapped_addition_outside_its_window_selects_a_sibling_then_clears() {
    // The new `<Sequence>` emits no layer at this frame, so the group's last
    // child is an older sibling group, whose first child is answered.
    let waiting = PendingSelect {
        under: AddedUnder {
            parent: "0.1".to_owned(),
            wrapped: true,
        },
        ..pending(Some(10))
    };
    let sibling = answer("0.1", Some("0.1.1.0"));
    assert_eq!(
        added_fate(&waiting, 10, Some(&sibling)),
        AddedFate::Select("0.1.1.0".to_owned())
    );
    // That layer's lookup is the sibling's element, not the inserted one.
    let sibling_element = SourcePos {
        line: 5,
        column: 7,
        ..waiting.position.clone()
    };
    assert!(!added_confirmed(
        &waiting.position,
        &Ok(Some(lookup_at(&sibling_element)))
    ));
}
