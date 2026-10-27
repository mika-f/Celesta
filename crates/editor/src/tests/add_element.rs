//! Tests of an addition's write (`add_files` as one history entry) and of
//! selecting the added layer after the reload (`added_fate`,
//! `added_confirmed`).

use super::*;
use crate::add_element::{
    AddedFate, PendingSelect, add_files, added_confirmed, added_fate, parent_facts,
};
use crate::preview::AddedUnder;
use celesta_source_edit::{NewElement, Timing};

/// A scene whose `<Group>` at 4:3 holds a `<Text>`.
const SCENE: &str = "import { Group, Text } from '@celesta/react';\n\nexport const Intro = () => (\n  <Group>\n    <Text>Hi</Text>\n  </Group>\n);\n";

/// [`SCENE`] after adding [`rect`] timed from 45 for 75 frames.
const ADDED: &str = "import { Group, Rect, Sequence, Text } from '@celesta/react';\n\nexport const Intro = () => (\n  <Group>\n    <Text>Hi</Text>\n    <Sequence from={45} durationInFrames={75}>\n      <Rect x={120} y={80} width={320} height={180} fill=\"#4F7CFF\" />\n    </Sequence>\n  </Group>\n);\n";

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "celesta-add-{name}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
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
        add_files(texts, &file, &parent, "Group", &rect(), Some(&TIMING)).map(|(files, _)| files)
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
        add_files(texts, &file, &parent, "Group", &rect(), Some(&TIMING)).map(|(files, _)| files)
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
            &rect(),
            None
        ),
        Err("The source changed while editing".to_owned())
    );
    fs::remove_dir_all(&dir).ok();
}

/// [`SCENE`] with `Group` imported as `G`.
const ALIASED: &str = "import { Group as G, Text } from '@celesta/react';\n\nexport const Intro = () => (\n  <G>\n    <Text>Hi</Text>\n  </G>\n);\n";

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
        &rect(),
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
            &rect(),
            None
        ),
        Err("The source changed while editing".to_owned())
    );
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
        &NewElement::Text { x: 10.0, y: 20.0 },
        None,
    )
    .unwrap();
    let [(written_file, written)] = &files[..] else {
        panic!("one file: {files:?}");
    };
    assert_eq!(written_file, &file);
    assert!(written.starts_with("import { Text } from \"@celesta/react\";\n"));

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
