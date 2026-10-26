//! Tests of editing a text on the canvas (`inline_text_view`): entering an
//! edit, what ends it, a press placing the caret, and what a commit writes
//! and how long its text stays drawn.

use super::*;
use crate::element_edit::{editable, inner_target};
use crate::inline_text::{TextEdit, TextMap};
use crate::inline_text_view::{
    EditEnd, EditEvent, InlineEntry, InlineTextSession, KeptOverride, cancel_session,
    committed_override, edit_end, held_commit_override, inline_commit_refusal, inline_commit_value,
    inline_entry, is_text_layer, kept_override_after_write, kept_override_on_reload, live_override,
    press_inside, pressed_index,
};
use crate::preview::TextOverride;
use crate::refresh::ReloadOrder;
use crate::scope::CommitGate;
use crate::transform_drag::{DragState, editing_drag_state};
use celesta_composition::TextAlign;
use celesta_gpu_renderer::{GlyphMetrics, TextLayout, TextMetrics};
use gpui_kit::{Bounds, point, px, size};

fn at(x: f64, y: f64) -> Point {
    Point { x, y }
}

/// A 40 by 20 box at the scene's origin, as the renderer records an
/// untransformed one-line text's layout box.
fn flat_box() -> [Point; 4] {
    [at(0.0, 0.0), at(40.0, 0.0), at(40.0, 20.0), at(0.0, 20.0)]
}

/// The `<Text>` at `line`:`column` of `text`, resolved with the Text table
/// and received with `received` as its children.
fn text_at(
    text: &str,
    (line, column): (u32, u32),
    received: &str,
) -> (ElementTargets, LayerSourceReport) {
    let mut report = text_report(
        Path::new("/project/scene.tsx"),
        &[("children", ReceivedValue::String(received.to_owned()))],
    );
    report.chain[0].line = line;
    report.chain[0].column = column;
    let targets = resolve_table(text, &report, &text_table());
    (targets, report)
}

/// [`text_at`] for a `<Text>` at line 2, column 3.
fn text_targets(text: &str, received: &str) -> (ElementTargets, LayerSourceReport) {
    text_at(text, (2, 3), received)
}

fn children_target(targets: &ElementTargets) -> &EditTarget {
    targets
        .target(&prop("children"))
        .expect("a Text has a text row")
}

/// The entry for editable targets drawn in `flat_box`, nothing pending.
fn enter(targets: &ElementTargets, report: &LayerSourceReport) -> InlineEntry {
    inline_entry(
        Some((targets, report)),
        false,
        None,
        false,
        Some(flat_box()),
        None,
    )
}

#[test]
fn inline_entry_takes_the_received_children() {
    // A literal.
    let (targets, report) = text_targets(
        "export const Scene = () => (\n  <Text>Sale</Text>\n);\n",
        "Sale",
    );
    assert!(editable(children_target(&targets)));
    assert_eq!(
        enter(&targets, &report),
        InlineEntry::Begin("Sale".to_owned())
    );

    // A const the children read.
    let (targets, report) = text_at(
        "export const Scene = () => {\n  const LABEL = \"Sale\";\n  return <Text>{LABEL}</Text>;\n};\n",
        (3, 10),
        "Sale",
    );
    let children = children_target(&targets);
    assert!(editable(children), "{children:?}");
    assert_eq!(
        enter(&targets, &report),
        InlineEntry::Begin("Sale".to_owned())
    );

    // Passed through from the call site: `<Label text="Sale" />`.
    let texts = Texts::from([
        (
            PathBuf::from("/project/component.tsx"),
            "export function Label({ text }) {\n  return <Text>{text}</Text>;\n}\n".to_owned(),
        ),
        (
            PathBuf::from("/project/scene.tsx"),
            "export const Scene = () => (\n  <Label text=\"Sale\" />\n);\n".to_owned(),
        ),
    ]);
    let mut report = text_report(
        Path::new("/project/component.tsx"),
        &[("children", ReceivedValue::String("Sale".to_owned()))],
    );
    report.chain[0].column = 10;
    report.chain.push(entry_at(
        "Label",
        ElementKind::Component,
        "/project/scene.tsx",
        2,
    ));
    let targets = resolve_targets(read_from(&texts), &report, Some(&text_table())).unwrap();
    let children = children_target(&targets);
    assert!(matches!(children, EditTarget::PassThrough { .. }));
    assert!(editable(children));
    assert_eq!(
        enter(&targets, &report),
        InlineEntry::Begin("Sale".to_owned())
    );

    // Nothing written between the tags, nothing passed: an empty edit that
    // inserts the text.
    let report = text_report(Path::new("/project/scene.tsx"), &[]);
    let targets = resolve_table(
        "export const Scene = () => (\n  <Text></Text>\n);\n",
        &report,
        &text_table(),
    );
    let children = children_target(&targets);
    assert!(
        matches!(children, EditTarget::Missing { .. }),
        "{children:?}"
    );
    assert_eq!(enter(&targets, &report), InlineEntry::Begin(String::new()));
}

#[test]
fn inline_entry_skips_a_layer_without_a_text_box_while_its_lookup_is_pending() {
    // A double-click on a Rect just selected: no box for a text in the
    // frame, so it is no entry (no wait, no "reloading" note) and the
    // press goes on to drag or select as usual.
    assert!(!is_text_layer(&SourceLookup::Pending, false));
    // A drawn text waits for its lookup.
    assert!(is_text_layer(&SourceLookup::Pending, true));
    // The report decides once it answers, whatever the frame recorded.
    let (_, text) = text_targets(
        "export const Scene = () => (\n  <Text>Sale</Text>\n);\n",
        "Sale",
    );
    assert!(is_text_layer(&SourceLookup::Found(text.clone()), false));
    let mut rect = text;
    rect.chain[0].name = "Rect".to_owned();
    assert!(!is_text_layer(&SourceLookup::Found(rect), true));
    // Without a report, the frame's box says it is a text.
    for lookup in [
        SourceLookup::NotInFrame,
        SourceLookup::NoPosition,
        SourceLookup::Failed("Couldn't look it up".to_owned()),
    ] {
        assert!(is_text_layer(&lookup, true), "{lookup:?}");
        assert!(!is_text_layer(&lookup, false), "{lookup:?}");
    }
}

#[test]
fn inline_entry_waits_for_a_pending_lookup() {
    let still_reading = Some("The source is still being read");
    assert_eq!(
        inline_entry(None, true, still_reading, false, Some(flat_box()), None),
        InlineEntry::Wait
    );
    // Without a box, targets still coming wait too: a found `Text` whose
    // source is still being read. (A layer whose lookup is pending is taken
    // for a text only with a box, so the view never asks without one.)
    assert_eq!(
        inline_entry(None, true, still_reading, false, None, None),
        InlineEntry::Wait
    );
    // Answered without targets: the reason the rows can't be written.
    assert_eq!(
        inline_entry(None, false, Some("No source position"), false, None, None),
        InlineEntry::Refuse("No source position".to_owned())
    );
}

#[test]
fn inline_entry_refuses_a_locked_row_with_its_reason() {
    for (text, shown) in [
        (
            "export const Scene = () => (\n  <Text><Span>Sale</Span></Text>\n);\n",
            "<Span>Sale</Span>",
        ),
        (
            "export const Scene = () => (\n  <Text>{\"Sale\" + \"!\"}</Text>\n);\n",
            "\"Sale\" + \"!\"",
        ),
    ] {
        let (targets, report) = text_targets(text, "Sale");
        let EditTarget::Locked(reason) = inner_target(children_target(&targets)) else {
            panic!("{text}: the text row is locked");
        };
        let reason = reason_text(reason);
        assert!(reason.contains(shown), "{reason}");
        assert_eq!(enter(&targets, &report), InlineEntry::Refuse(reason));
    }
}

#[test]
fn inline_entry_refuses_while_the_code_reloads() {
    let (targets, report) = text_targets(
        "export const Scene = () => (\n  <Text>Sale</Text>\n);\n",
        "Sale",
    );
    let reloading = InlineEntry::Refuse("The code is reloading".to_owned());
    assert_eq!(
        inline_entry(
            Some((&targets, &report)),
            false,
            None,
            true,
            Some(flat_box()),
            None
        ),
        reloading
    );
    // Not waited for either: the reload answers the lookup again.
    assert_eq!(
        inline_entry(None, true, None, true, Some(flat_box()), None),
        reloading
    );
}

#[test]
fn inline_entry_refuses_a_text_that_isn_t_drawn() {
    let (targets, report) = text_targets(
        "export const Scene = () => (\n  <Text>Sale</Text>\n);\n",
        "Sale",
    );
    assert_eq!(
        inline_entry(Some((&targets, &report)), false, None, false, None, None),
        InlineEntry::Refuse("The text isn't drawn in this frame".to_owned())
    );
}

#[test]
fn inline_entry_refuses_an_edge_on_text() {
    let (targets, report) = text_targets(
        "export const Scene = () => (\n  <Text>Sale</Text>\n);\n",
        "Sale",
    );
    let edge_on = InlineEntry::Refuse("The text is edge-on to the view".to_owned());
    // Turned 90° about its vertical axis: no width on screen.
    let flat = [at(20.0, 0.0), at(20.0, 0.0), at(20.0, 20.0), at(20.0, 20.0)];
    // Turned past it far enough to cross over: a bow-tie.
    let crossed = [at(0.0, 0.0), at(40.0, 20.0), at(40.0, 0.0), at(0.0, 20.0)];
    let layout = one_line("Sale");
    for text_box in [flat, crossed] {
        for layout in [None, Some(&layout)] {
            assert_eq!(
                inline_entry(
                    Some((&targets, &report)),
                    false,
                    None,
                    false,
                    Some(text_box),
                    layout
                ),
                edge_on
            );
        }
    }
    // A mirrored box, its corners in reverse, is not edge-on.
    let mirrored = [at(40.0, 0.0), at(0.0, 0.0), at(0.0, 20.0), at(40.0, 20.0)];
    assert_eq!(
        inline_entry(
            Some((&targets, &report)),
            false,
            None,
            false,
            Some(mirrored),
            Some(&layout)
        ),
        InlineEntry::Begin("Sale".to_owned())
    );
}

#[test]
fn a_refused_commit_keeps_the_edit_the_person_ended() {
    let refused =
        CommitGate::Refuse("The scenes were scanned while editing; edit again".to_owned());
    let kept =
        |event, writes, gate: Option<&CommitGate>| inline_commit_refusal(event, writes, gate);

    // Enter, a blur or a press outside the text: the edit stays, its text
    // and caret with it, and the refusal shows.
    for event in [
        EditEvent::Enter,
        EditEvent::Blur,
        EditEvent::CanvasPress { inside: false },
    ] {
        assert_eq!(
            kept(event, true, Some(&refused)),
            Some("The scenes were scanned while editing; edit again".to_owned())
        );
    }
    // A gesture is never held for the scan: that is one more refusal.
    assert_eq!(
        kept(
            EditEvent::Enter,
            true,
            Some(&CommitGate::Hold("Scanning scenes…".to_owned()))
        ),
        Some("Scanning scenes…".to_owned())
    );
    // Written, or nothing to write: the edit ends as before.
    assert_eq!(
        kept(EditEvent::Enter, true, Some(&CommitGate::Go(None))),
        None
    );
    // Waiting its turn: held, and gated when it goes.
    assert_eq!(kept(EditEvent::Enter, true, None), None);
    assert_eq!(kept(EditEvent::Enter, false, Some(&refused)), None);
    // The selection, the playhead or the project moving on ends the edit.
    for event in [
        EditEvent::SelectionChange,
        EditEvent::ToggleInteractive,
        EditEvent::Seek,
        EditEvent::Play,
        EditEvent::ProjectCommand,
    ] {
        assert_eq!(kept(event, true, Some(&refused)), None);
    }
}

#[test]
fn edit_end_decides_every_event() {
    let changed = Some("The source changed while editing");
    assert_eq!(
        edit_end(EditEvent::Enter),
        EditEnd::Commit { refocus_root: true }
    );
    assert_eq!(
        edit_end(EditEvent::Escape),
        EditEnd::Cancel {
            reason: None,
            refocus_root: true
        }
    );
    // The focus went where the user put it.
    assert_eq!(
        edit_end(EditEvent::Blur),
        EditEnd::Commit {
            refocus_root: false
        }
    );
    // A press in the text places the caret; outside, it commits and then
    // acts as usual.
    assert_eq!(
        edit_end(EditEvent::CanvasPress { inside: true }),
        EditEnd::Keep
    );
    assert_eq!(
        edit_end(EditEvent::CanvasPress { inside: false }),
        EditEnd::Commit { refocus_root: true }
    );
    assert_eq!(
        edit_end(EditEvent::SelectionCleared {
            outside_change: true
        }),
        EditEnd::Cancel {
            reason: changed,
            refocus_root: true
        }
    );
    assert_eq!(
        edit_end(EditEvent::SelectionCleared {
            outside_change: false
        }),
        EditEnd::Cancel {
            reason: None,
            refocus_root: true
        }
    );
    // The rest come by a key or a menu with the focus still on the text
    // (a press elsewhere blurs it first), so the root takes it back.
    for event in [
        EditEvent::SelectionChange,
        EditEvent::ToggleInteractive,
        EditEvent::Seek,
        EditEvent::Play,
        EditEvent::ProjectCommand,
    ] {
        assert_eq!(
            edit_end(event),
            EditEnd::Commit { refocus_root: true },
            "{event:?}"
        );
    }
}

#[test]
fn editing_drag_state_hides_handles_and_cross() {
    let frame = || drag_frame([1.0, 0.0, 0.0, 1.0, 0.0, 0.0], at(10.0, 20.0));
    let editable = || DragState::Editable(frame(), vec![prop("x")], PivotMode::Pivot, None);
    let locked = || DragState::Locked(frame(), "Written as an expression: x".to_owned());
    // Editing: the outline only.
    assert!(matches!(
        editing_drag_state(editable(), true),
        DragState::Unknown
    ));
    assert!(matches!(
        editing_drag_state(locked(), true),
        DragState::Unknown
    ));
    assert!(matches!(
        editing_drag_state(DragState::NoFrame, true),
        DragState::Unknown
    ));
    // Otherwise as it was.
    assert!(matches!(
        editing_drag_state(editable(), false),
        DragState::Editable(_, props, PivotMode::Pivot, None) if props == [prop("x")]
    ));
    assert!(matches!(
        editing_drag_state(locked(), false),
        DragState::Locked(_, reason) if reason == "Written as an expression: x"
    ));
    assert!(matches!(
        editing_drag_state(DragState::NoFrame, false),
        DragState::NoFrame
    ));
}

/// `text` on one line, each code point a 10-wide cluster, 20 high.
fn one_line(text: &str) -> TextLayout {
    let glyphs: Vec<GlyphMetrics> = text
        .chars()
        .enumerate()
        .map(|(index, c)| GlyphMetrics {
            start: index,
            end: index + 1,
            rtl: false,
            text: c.to_string(),
            x: 10.0 * index as f64,
            width: 10.0,
            line: 0,
            ink_ascent: 0.0,
            ink_descent: 0.0,
        })
        .collect();
    TextLayout {
        text: text.to_owned(),
        metrics: TextMetrics {
            line_starts: vec![0],
            width: 10.0 * glyphs.len() as f64,
            height: 20.0,
            ascent: 16.0,
            descent: 4.0,
            line_height: 20.0,
            lines: 1,
            glyphs,
            ..TextMetrics::default()
        },
        max_width: None,
        align: TextAlign::Left,
    }
}

#[test]
fn a_press_in_the_layout_box_places_the_caret() {
    // A 400 by 200 frame fitted at half size into a viewer at (100, 50).
    let bounds = Bounds::new(point(px(100.0), px(50.0)), size(px(200.0), px(100.0)));
    let frame_size = (400.0, 200.0);
    let on_screen = |scene: Point| {
        point(
            px(100.0 + scene.x as f32 * 0.5),
            px(50.0 + scene.y as f32 * 0.5),
        )
    };
    let layout = one_line("abcd");
    let moved = |p: Point| at(p.x + 100.0, p.y + 100.0);
    // Drawn flat at (100, 100), and tilted back under a perspective (its
    // top edge shorter).
    let flat = flat_box().map(moved);
    let tilted = [at(10.0, 0.0), at(30.0, 0.0), at(40.0, 20.0), at(0.0, 20.0)].map(moved);
    for text_box in [flat, tilted] {
        let map = TextMap::fit(&layout, text_box).unwrap();
        let press = |x: f64, y: f64| {
            let position = on_screen(map.to_scene(at(x, y)));
            pressed_index(bounds, frame_size, position, &layout, &map)
        };
        // Nearer the boundary between "c" and "d" than any other.
        assert_eq!(press(27.0, 10.0), Some((3, false)));
        assert_eq!(press(2.0, 18.0), Some((0, false)));
        assert_eq!(press(39.0, 1.0), Some((4, false)));
        // Outside the layout box: no caret, the press commits.
        assert_eq!(press(45.0, 10.0), None);
        assert_eq!(press(20.0, -3.0), None);
        assert_eq!(press(20.0, 23.0), None);
    }
}

#[test]
fn escape_after_marking_drops_the_session_and_its_override() {
    let mut session = InlineTextSession::new("title".to_owned(), "Sale".to_owned());
    session.edit.insert("!");
    session.edit.mark_utf16(None, "に", None);
    let live = live_override(&session);
    assert_eq!(
        live,
        TextOverride {
            layer: "title".to_owned(),
            original: "Sale".to_owned(),
            text: "Sale!に".to_owned(),
        }
    );
    let mut session = Some(session);
    let mut text_override = Some(live);

    let EditEnd::Cancel { reason, .. } = edit_end(EditEvent::Escape) else {
        panic!("Escape cancels");
    };
    assert_eq!(reason, None);
    assert!(cancel_session(&mut session, &mut text_override));
    // No session, and the next frame draws the code's own text.
    assert!(session.is_none());
    assert_eq!(text_override, None);
    // Nothing more to cancel.
    assert!(!cancel_session(&mut session, &mut text_override));

    // Another layer's override (a commit waiting for its reload) stays.
    let mut session = Some(InlineTextSession::new(
        "title".to_owned(),
        "Sale".to_owned(),
    ));
    let other = TextOverride {
        layer: "price".to_owned(),
        original: "$5".to_owned(),
        text: "$4".to_owned(),
    };
    let mut text_override = Some(other.clone());
    assert!(cancel_session(&mut session, &mut text_override));
    assert_eq!(text_override, Some(other));
}

#[test]
fn a_press_before_the_first_edited_frame_on_the_layer_is_inside() {
    // No map yet (the frame drawn with the edit has not arrived): a press
    // that hits the edited layer, such as a triple-click's third, keeps the
    // edit and leaves the caret.
    assert!(press_inside(None, true));
    assert!(!press_inside(None, false));
    // Once mapped, the layout box decides, whatever the layer's hit area.
    assert!(press_inside(Some(true), false));
    assert!(!press_inside(Some(false), true));
}

#[test]
fn inline_commit_value_skips_unchanged_and_empty() {
    assert_eq!(inline_commit_value("Sale", "Sale"), None);
    // The text row is required: an emptied text shows the source again.
    assert_eq!(inline_commit_value("Sale", ""), None);
    assert_eq!(
        inline_commit_value("Sale", "Sold out"),
        Some(EditValue::String("Sold out".to_owned()))
    );
    // A `<Text></Text>` written for the first time.
    assert_eq!(
        inline_commit_value("", "New"),
        Some(EditValue::String("New".to_owned()))
    );
}

/// The edit of `original` after `change`, committed as the edit writes it:
/// a composition still marked is accepted.
fn committed(original: &str, change: impl FnOnce(&mut TextEdit)) -> Option<EditValue> {
    let mut session = InlineTextSession::new("title".to_owned(), original.to_owned());
    change(&mut session.edit);
    session.edit.unmark();
    inline_commit_value(&session.original, session.edit.text())
}

/// The value the `text` row writes, as one commit.
fn children(value: Option<EditValue>) -> [(PropKey, Option<EditValue>); 1] {
    [(prop("children"), value)]
}

#[test]
fn an_edit_writes_one_jsx_text_literal() {
    let text = "export const Scene = () => (\n  <Text>Sale</Text>\n);\n";
    let (targets, report) = text_targets(text, "Sale");
    let texts = Texts::from([(targets.file.clone(), text.to_owned())]);
    let write = |value| {
        let files = commit_files(&texts, &targets, &report, None, None, &children(value)).unwrap();
        assert_eq!(files.len(), 1, "one file, one entry");
        files.into_iter().next().unwrap().1
    };

    // Typed over the selection, with a composition still marked.
    let value = committed("Sale", |edit| {
        edit.select_all();
        edit.insert("Sold ");
        edit.mark_utf16(None, "out", None);
    });
    assert_eq!(
        write(value),
        "export const Scene = () => (\n  <Text>Sold out</Text>\n);\n"
    );

    // A line break is written as a string, which JSX text can't hold.
    let value = committed("Sale", |edit| {
        edit.select_all();
        edit.insert("a\nb");
    });
    assert_eq!(
        write(value),
        "export const Scene = () => (\n  <Text>{\"a\\nb\"}</Text>\n);\n"
    );
}

#[test]
fn a_const_edit_writes_the_const() {
    let text = "export const Scene = () => {\n  const LABEL = \"Sale\";\n  return <Text>{LABEL}</Text>;\n};\n";
    let (targets, report) = text_at(text, (3, 10), "Sale");
    let texts = Texts::from([(targets.file.clone(), text.to_owned())]);
    let value = committed("Sale", |edit| edit.insert("!"));
    let files = commit_files(&texts, &targets, &report, None, None, &children(value)).unwrap();
    assert_eq!(
        files,
        [(targets.file.clone(), text.replace("\"Sale\"", "\"Sale!\""))]
    );
}

/// `<Label text="Sale" />` in `scene.tsx` passing `text` through to the
/// `<Text>` in `component.tsx`, both written into `dir`.
fn pass_through_files(dir: &Path) -> (PathBuf, PathBuf, LayerSourceReport) {
    let component = dir.join("component.tsx");
    let scene = dir.join("scene.tsx");
    fs::write(
        &component,
        "export function Label({ text }) {\n  return <Text>{text}</Text>;\n}\n",
    )
    .unwrap();
    fs::write(
        &scene,
        "export const Scene = () => (\n  <Label text=\"Sale\" />\n);\n",
    )
    .unwrap();
    let mut report = text_report(
        &component,
        &[("children", ReceivedValue::String("Sale".to_owned()))],
    );
    report.chain[0].column = 10;
    report.chain.push(entry_at(
        "Label",
        ElementKind::Component,
        scene.to_str().unwrap(),
        2,
    ));
    (component, scene, report)
}

fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "celesta-{name}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Commits `value` as the `text` row's own write does: the files read and
/// written together, as one history entry.
fn write_children(
    targets: &ElementTargets,
    report: &LayerSourceReport,
    value: Option<EditValue>,
) -> HistoryEntry {
    let files = write_all_recorded(targets.files.clone(), |texts| {
        commit_files(texts, targets, report, None, None, &children(value))
            .map_err(|error| error.to_string())
    })
    .unwrap();
    HistoryEntry { files }
}

#[test]
fn a_pass_through_edit_writes_the_call_site() {
    let dir = scratch_dir("inline-pass-through");
    let (component, scene, report) = pass_through_files(&dir);
    let read = |path: &Path| fs::read_to_string(path).map_err(|error| error.to_string());
    let targets = resolve_targets(read, &report, Some(&text_table())).unwrap();
    // Drawn live by the layer's id, whatever element wrote the text.
    let mut session = InlineTextSession::new("label-text".to_owned(), "Sale".to_owned());
    session.edit.insert("!");
    assert_eq!(live_override(&session).layer, "label-text");

    let component_before = fs::read_to_string(&component).unwrap();
    write_children(&targets, &report, inline_commit_value("Sale", "Sale!"));
    assert_eq!(
        fs::read_to_string(&scene).unwrap(),
        "export const Scene = () => (\n  <Label text=\"Sale!\" />\n);\n"
    );
    assert_eq!(fs::read_to_string(&component).unwrap(), component_before);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_pass_through_edit_undoes_in_one_step() {
    let dir = scratch_dir("inline-pass-through-undo");
    let (component, scene, report) = pass_through_files(&dir);
    let read = |path: &Path| fs::read_to_string(path).map_err(|error| error.to_string());
    let targets = resolve_targets(read, &report, Some(&text_table())).unwrap();
    let before = (
        fs::read_to_string(&component).unwrap(),
        fs::read_to_string(&scene).unwrap(),
    );

    let mut history = EditHistory::default();
    history.push(write_children(
        &targets,
        &report,
        inline_commit_value("Sale", "Sold"),
    ));
    assert!(
        fs::read_to_string(&scene)
            .unwrap()
            .contains("text=\"Sold\"")
    );
    history.undo().unwrap();
    assert_eq!(
        (
            fs::read_to_string(&component).unwrap(),
            fs::read_to_string(&scene).unwrap(),
        ),
        before
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_override_is_dropped_on_an_error_or_without_a_reload() {
    // The write failed: the file holds the old text, which the next frame
    // draws.
    assert_eq!(
        kept_override_after_write(&Err("The source changed".to_owned()), false, 4),
        KeptOverride::Dropped
    );
    // Written, but the targets are still current: the text came out as it
    // was, so no reload comes.
    assert_eq!(
        kept_override_after_write(&Ok(()), true, 4),
        KeptOverride::Dropped
    );
    // Written: kept for the reload that reads it, numbered 4 or later.
    assert_eq!(
        kept_override_after_write(&Ok(()), false, 4),
        KeptOverride::UntilReload(4)
    );
}

#[test]
fn the_override_waits_for_the_first_reload_at_or_after_its_sequence() {
    let kept = KeptOverride::UntilReload(4);
    // A reload requested before the write finished read the old file.
    assert!(kept_override_on_reload(&kept, 3));
    // Any reload requested after it read the written file, whether it
    // keeps the selection (the own write's) or clears it (the file also
    // changed elsewhere).
    assert!(!kept_override_on_reload(&kept, 4));
    assert!(!kept_override_on_reload(&kept, 5));
    // Still being written (or waiting its turn): no reload draws it yet.
    assert!(kept_override_on_reload(&KeptOverride::Writing, 9));
    assert!(!kept_override_on_reload(&KeptOverride::Dropped, 9));
}

#[test]
fn a_superseded_own_reload_still_releases_the_override() {
    let mut reloads = ReloadOrder::default();
    // The write finished: its own reload gets the next number.
    let kept = kept_override_after_write(&Ok(()), false, reloads.next());
    let own = reloads.request();
    assert_eq!(kept, KeptOverride::UntilReload(own));
    // The file changed elsewhere right after: a reload that keeps nothing.
    let outside = reloads.request();
    // It finishes first and applies; the own one never applies after it.
    assert!(reloads.finish(outside));
    assert!(!kept_override_on_reload(&kept, outside));
    assert!(!reloads.finish(own));
}

#[test]
fn the_commit_frame_may_look_the_source_up() {
    // Enter after typing, nothing marked: the committed text is the one
    // already drawn, so the override does not change. The commit still asks
    // for a frame, which (no edit running) looks the layer up, so a commit
    // held for stale targets gets them resolved.
    let mut session = InlineTextSession::new("title".to_owned(), "Sale".to_owned());
    session.edit.insert("!");
    let live = live_override(&session);
    assert_eq!(committed_override(&mut session), live);
    assert!(lookup_allowed(false, false, false));
    // A composition still marked is accepted into it.
    session.edit.mark_utf16(None, "に", None);
    assert_eq!(committed_override(&mut session).text, "Sale!に");
}

#[test]
fn dropping_a_held_commit_drops_its_override() {
    let committed = TextOverride {
        layer: "title".to_owned(),
        original: "Sale".to_owned(),
        text: "Sale!".to_owned(),
    };
    let held_commit = |layer: &str, key: &str| HeldCommit {
        layer: layer.to_owned(),
        key: prop(key),
        value: Some(EditValue::String("Sale!".to_owned())),
        frame: 0,
        scope: None,
        scanning: false,
    };
    let mut held = HeldCommits::default();
    held.push(held_commit("title", "children"));
    // Its commit waits its turn: forgetting or dropping the held commits
    // drops the override, which nothing will write.
    assert!(held_commit_override(
        &KeptOverride::Writing,
        Some(&committed),
        &held
    ));
    // Being written already (not held), or written: it stays.
    let mut other = HeldCommits::default();
    other.push(held_commit("title", "fontSize"));
    other.push(held_commit("price", "children"));
    assert!(!held_commit_override(
        &KeptOverride::Writing,
        Some(&committed),
        &other
    ));
    assert!(!held_commit_override(
        &KeptOverride::UntilReload(2),
        Some(&committed),
        &held
    ));
    assert!(!held_commit_override(&KeptOverride::Writing, None, &held));
}

#[test]
fn seeking_or_playing_commits_first() {
    for event in [EditEvent::Seek, EditEvent::Play] {
        // Settled where the rows submit their typed text: as a commit.
        assert_eq!(
            edit_end(event),
            EditEnd::Commit { refocus_root: true },
            "{event:?}"
        );
        assert_eq!(
            committed("Sale", |edit| edit.insert("!")),
            Some(EditValue::String("Sale!".to_owned()))
        );
    }
}

#[test]
fn selecting_another_layer_commits_before_the_gate() {
    // The commit could not be written yet (another write runs): it is held
    // for "title" before the selection is asked for.
    assert_eq!(
        edit_end(EditEvent::SelectionChange),
        EditEnd::Commit { refocus_root: true }
    );
    let mut held = HeldCommits::default();
    held.push(HeldCommit {
        layer: "title".to_owned(),
        key: prop("children"),
        value: committed("Sale", |edit| edit.insert("!")),
        frame: 0,
        scope: None,
        scanning: false,
    });
    let mut gate = SelectionGate::default();
    // The gate sees it: "price" waits for the commit to be written.
    assert_eq!(
        gate.request(
            Some("price".to_owned()),
            Some("title"),
            held.any(Some("title"))
        ),
        None
    );
    assert!(gate.waiting());
    assert!(held.pop(Some("title")).is_some());
    assert_eq!(
        gate.release(held.any(Some("title"))),
        Some(Some("price".to_owned()))
    );
}
