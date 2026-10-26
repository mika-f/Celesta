//! Tests of commits under an edit scope (`commit_scoped`, `commit_gate`):
//! what "This scene" and "Shared value" write, in which files, as one
//! history entry, and when a commit waits for the occurrence index.

use super::*;
use crate::edit_history::FileEdit;
use crate::element_edit::{ComponentTargets, commit_report, commit_scoped, editable};
use crate::occurrences::OccurrenceState;
use crate::scope::{
    CommitGate, SceneCalls, Scope, ScopeLock, ScopeState, commit_gate, current_calls, gesture_lock,
    read_scene_calls, scope_state,
};
use celesta_react_bridge::{
    ElementOccurrences, InstanceOccurrences, OccurrenceIndex, ReactChapter,
};
use celesta_source_edit::{ElementRef, written_attributes};

/// `PriceTag` writes `<Rect>` at 6:10 with a literal `width`; its props
/// type is a local interface.
const TAG: &str = "interface Props {\n  label: string;\n}\n\nexport function PriceTag({ label }: Props) {\n  return <Rect width={200} />;\n}\n";

const TAG_RECT: u32 = 6;

/// [`TAG`] after "This scene" lifted `width` in the Price scene: `<Rect>`
/// is at 7:10.
const LIFTED: &str = "interface Props {\n  label: string;\n  rectWidth?: number;\n}\n\nexport function PriceTag({ label, rectWidth = 200 }: Props) {\n  return <Rect width={rectWidth} />;\n}\n";

const LIFTED_RECT: u32 = 7;

/// The Price scene's call of `PriceTag` at 2:3.
const PRICE: &str = "export const Price = () => (\n  <PriceTag label=\"Sale\" />\n);\n";

/// [`PRICE`] after the lift: it sets its own `rectWidth`.
const PRICE_OWN: &str =
    "export const Price = () => (\n  <PriceTag label=\"Sale\" rectWidth={260} />\n);\n";

/// The Intro scene's call of `PriceTag` at 2:3.
const INTRO: &str = "export const Intro = () => (\n  <PriceTag label=\"Hello\" />\n);\n";

/// `price_tag.tsx`, `price.tsx` and `intro.tsx` in a directory of their
/// own, removed when dropped.
struct Files {
    dir: PathBuf,
}

impl Files {
    fn new(name: &str, tag: &str, price: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "celesta-scope-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let files = Self { dir };
        fs::write(files.tag(), tag).unwrap();
        fs::write(files.price(), price).unwrap();
        fs::write(files.intro(), INTRO).unwrap();
        files
    }

    fn tag(&self) -> PathBuf {
        self.dir.join("price_tag.tsx")
    }

    fn price(&self) -> PathBuf {
        self.dir.join("price.tsx")
    }

    fn intro(&self) -> PathBuf {
        self.dir.join("intro.tsx")
    }

    fn read(&self, path: &Path) -> String {
        fs::read_to_string(path).unwrap()
    }

    /// The `<Rect>` at `line`:10 of the tag file, called by `call` (2:3).
    fn report(&self, line: u32, call: &Path, received: &[(&str, f64)]) -> LayerSourceReport {
        let rect = LayerSource {
            column: 10,
            ..entry_at("Rect", ElementKind::Primitive, path_str(&self.tag()), line)
        };
        let call = entry_at("PriceTag", ElementKind::Component, path_str(call), 2);
        let mut report = report_of(vec![rect, call]);
        for (name, value) in received {
            report
                .props
                .others
                .insert((*name).to_owned(), ReceivedValue::Number(*value));
        }
        report
    }

    fn targets(&self, report: &LayerSourceReport) -> ElementTargets {
        let read = |path: &Path| fs::read_to_string(path).map_err(|error| error.to_string());
        resolve_targets(read, report, Some(&tag_table())).expect("the element resolves")
    }

    /// Commits `values` under `scope` as the editor does: one write of every
    /// file the targets read. The files written.
    fn commit(
        &self,
        targets: &ElementTargets,
        report: &LayerSourceReport,
        scope: Option<Scope>,
        values: &[(PropKey, Option<EditValue>)],
    ) -> Result<Vec<FileEdit>, String> {
        write_all_recorded(targets.files.clone(), |texts| {
            commit_files(texts, targets, report, scope, None, values)
                .map_err(|error| error.to_string())
        })
    }
}

impl Drop for Files {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.dir).ok();
    }
}

fn path_str(path: &Path) -> &str {
    path.to_str().unwrap()
}

fn tag_table() -> PrimitivePropertyTable {
    property_table(vec![
        (
            "Transform",
            vec![
                ("x", number_field(Some(0.0))),
                ("y", number_field(Some(0.0))),
            ],
        ),
        ("Size", vec![("width", number_field(None))]),
    ])
}

fn number(key: &str, value: f64) -> (PropKey, Option<EditValue>) {
    (prop(key), Some(EditValue::Number(value)))
}

fn written(files: &[FileEdit]) -> Vec<&Path> {
    files.iter().map(|file| file.path.as_path()).collect()
}

#[test]
fn this_scene_on_a_pass_through_writes_the_call_site_only() {
    let files = Files::new("pass-through", LIFTED, PRICE_OWN);
    let report = files.report(LIFTED_RECT, &files.price(), &[("width", 260.0)]);
    let targets = files.targets(&report);

    let entry = files
        .commit(
            &targets,
            &report,
            Some(Scope::ThisScene),
            &[number("width", 300.0)],
        )
        .unwrap();

    assert_eq!(written(&entry), [files.price().as_path()]);
    assert_eq!(files.read(&files.price()), PRICE_OWN.replace("260", "300"));
    assert_eq!(files.read(&files.tag()), LIFTED);
}

#[test]
fn this_scene_lifts_a_literal_and_passes_it() {
    let files = Files::new("lift", TAG, PRICE);
    let report = files.report(TAG_RECT, &files.price(), &[("width", 200.0)]);
    let targets = files.targets(&report);

    let entry = files
        .commit(
            &targets,
            &report,
            Some(Scope::ThisScene),
            &[number("width", 260.0)],
        )
        .unwrap();

    assert_eq!(files.read(&files.tag()), LIFTED);
    assert_eq!(files.read(&files.price()), PRICE_OWN);
    // One entry, both files; undo restores both.
    assert_eq!(entry.len(), 2);
    let mut history = EditHistory::default();
    history.push(HistoryEntry { files: entry });
    history.undo().unwrap();
    assert_eq!(files.read(&files.tag()), TAG);
    assert_eq!(files.read(&files.price()), PRICE);
}

#[test]
fn a_lift_avoids_the_names_the_call_writes() {
    // Price passes `rectWidth` on to the `<Rect>` through `...rest`.
    let tag = "interface Props {\n  label: string;\n}\n\nexport function PriceTag({ label, ...rest }: Props) {\n  return <Rect {...rest} width={200} />;\n}\n";
    let price = "export const Price = () => (\n  <PriceTag label=\"Sale\" rectWidth={5} />\n);\n";
    let files = Files::new("lift-clash", tag, price);
    let report = files.report(TAG_RECT, &files.price(), &[("width", 200.0)]);
    let targets = files.targets(&report);

    files
        .commit(
            &targets,
            &report,
            Some(Scope::ThisScene),
            &[number("width", 260.0)],
        )
        .unwrap();

    assert_eq!(
        files.read(&files.tag()),
        "interface Props {\n  label: string;\n  rectWidth2?: number;\n}\n\nexport function PriceTag({ label, rectWidth2 = 200, ...rest }: Props) {\n  return <Rect {...rest} width={rectWidth2} />;\n}\n"
    );
    assert_eq!(
        files.read(&files.price()),
        "export const Price = () => (\n  <PriceTag label=\"Sale\" rectWidth={5} rectWidth2={260} />\n);\n"
    );
}

#[test]
fn a_second_scene_after_a_lift_resolves_as_pass_through() {
    let files = Files::new("second-scene", LIFTED, PRICE_OWN);
    let report = files.report(LIFTED_RECT, &files.intro(), &[("width", 200.0)]);
    let targets = files.targets(&report);
    assert!(matches!(
        targets.target(&prop("width")),
        Some(EditTarget::PassThrough { param, .. }) if param == "rectWidth"
    ));

    let entry = files
        .commit(
            &targets,
            &report,
            Some(Scope::ThisScene),
            &[number("width", 180.0)],
        )
        .unwrap();

    assert_eq!(written(&entry), [files.intro().as_path()]);
    assert_eq!(
        files.read(&files.intro()),
        "export const Intro = () => (\n  <PriceTag label=\"Hello\" rectWidth={180} />\n);\n"
    );
    assert_eq!(files.read(&files.tag()), LIFTED);
    assert_eq!(files.read(&files.price()), PRICE_OWN);
}

#[test]
fn shared_value_on_a_lifted_prop_writes_the_default() {
    // Price leaves `rectWidth` out: it uses the shared value.
    let files = Files::new("shared-default", LIFTED, PRICE);
    let report = files.report(LIFTED_RECT, &files.price(), &[("width", 200.0)]);
    let targets = files.targets(&report);

    let entry = files
        .commit(
            &targets,
            &report,
            Some(Scope::Shared),
            &[number("width", 240.0)],
        )
        .unwrap();

    assert_eq!(written(&entry), [files.tag().as_path()]);
    assert_eq!(
        files.read(&files.tag()),
        LIFTED.replace("rectWidth = 200", "rectWidth = 240")
    );
    assert_eq!(files.read(&files.price()), PRICE);
}

#[test]
fn shared_value_adds_a_default_when_there_is_none() {
    let tag = LIFTED.replace("rectWidth = 200", "rectWidth");
    let files = Files::new("shared-absent", &tag, PRICE);
    let report = files.report(LIFTED_RECT, &files.price(), &[]);
    let targets = files.targets(&report);

    files
        .commit(
            &targets,
            &report,
            Some(Scope::Shared),
            &[number("width", 240.0)],
        )
        .unwrap();

    assert_eq!(
        files.read(&files.tag()),
        LIFTED.replace("rectWidth = 200", "rectWidth = 240")
    );
    assert_eq!(files.read(&files.price()), PRICE);
}

#[test]
fn shared_value_refuses_a_row_this_scene_sets_itself() {
    // Price passes `rectWidth={260}`: a shared edit would not show here.
    let refusal = "This scene sets its own `rectWidth`".to_owned();
    let files = Files::new("shared-own", LIFTED, PRICE_OWN);
    let report = files.report(LIFTED_RECT, &files.price(), &[("width", 260.0)]);
    let targets = files.targets(&report);

    let refused = files.commit(
        &targets,
        &report,
        Some(Scope::Shared),
        &[number("width", 240.0)],
    );

    assert_eq!(refused, Err(refusal.clone()));
    assert_eq!(files.read(&files.tag()), LIFTED);
    assert_eq!(files.read(&files.price()), PRICE_OWN);

    // An expression at S is its own value too, though it can't be written.
    let price = PRICE_OWN.replace("{260}", "{size * 2}");
    let files = Files::new("shared-own-expression", LIFTED, &price);
    let report = files.report(LIFTED_RECT, &files.price(), &[("width", 260.0)]);
    let targets = files.targets(&report);
    assert!(!editable(targets.target(&prop("width")).unwrap()));
    assert_eq!(
        files.commit(
            &targets,
            &report,
            Some(Scope::Shared),
            &[number("width", 240.0)],
        ),
        Err(refusal)
    );
    assert_eq!(files.read(&files.tag()), LIFTED);
    assert_eq!(files.read(&files.price()), price);
}

#[test]
fn removal_under_shared_value_deletes_this_scene_s_own_at_s() {
    // "Use the shared value" on a row this scene sets itself.
    let files = Files::new("shared-removal", LIFTED, PRICE_OWN);
    let report = files.report(LIFTED_RECT, &files.price(), &[("width", 260.0)]);
    let targets = files.targets(&report);

    let entry = files
        .commit(
            &targets,
            &report,
            Some(Scope::Shared),
            &[(prop("width"), None)],
        )
        .unwrap();

    assert_eq!(written(&entry), [files.price().as_path()]);
    assert_eq!(files.read(&files.price()), PRICE);
    assert_eq!(files.read(&files.tag()), LIFTED);
}

#[test]
fn a_move_lifts_x_and_y_in_one_entry() {
    let tag = "export function PriceTag({ label, ...rest }) {\n  return <Rect width={200} />;\n}\n";
    let files = Files::new("move", tag, PRICE);
    let report = files.report(2, &files.price(), &[("width", 200.0)]);
    let targets = files.targets(&report);

    let entry = files
        .commit(
            &targets,
            &report,
            Some(Scope::ThisScene),
            &[number("x", 60.0), number("y", 80.0)],
        )
        .unwrap();

    assert_eq!(
        files.read(&files.tag()),
        "export function PriceTag({ label, rectX = 0, rectY = 0, ...rest }) {\n  return <Rect width={200} x={rectX} y={rectY} />;\n}\n"
    );
    assert_eq!(
        files.read(&files.price()),
        "export const Price = () => (\n  <PriceTag label=\"Sale\" rectX={60} rectY={80} />\n);\n"
    );
    let mut history = EditHistory::default();
    history.push(HistoryEntry { files: entry });
    history.undo().unwrap();
    assert_eq!(files.read(&files.tag()), tag);
    assert_eq!(files.read(&files.price()), PRICE);
}

/// The Price scene calls `PriceTag` twice: the Sale tag at 2:3, then the
/// Second tag.
const TWO_CALLS: &str = "export const Price = () => [\n  <PriceTag label=\"Sale\" rectWidth={260} />,\n  <PriceTag label=\"Second\" />,\n];\n";

#[test]
fn this_scene_with_two_calls_writes_the_selected_call_only() {
    let files = Files::new("two-calls", LIFTED, TWO_CALLS);
    let report = LayerSourceReport {
        render_count: 2,
        ..files.report(LIFTED_RECT, &files.price(), &[("width", 260.0)])
    };
    let targets = files.targets(&report);

    let entry = files
        .commit(
            &targets,
            &report,
            Some(Scope::ThisScene),
            &[number("width", 280.0)],
        )
        .unwrap();

    // The Second call keeps the shared value; the default stays.
    assert_eq!(written(&entry), [files.price().as_path()]);
    assert_eq!(
        files.read(&files.price()),
        TWO_CALLS.replace("{260}", "{280}")
    );
    assert_eq!(files.read(&files.tag()), LIFTED);
}

#[test]
fn a_move_of_one_of_two_copies_lifts_to_its_call_only() {
    let tag = "interface Props {\n  label: string;\n}\n\nexport function PriceTag({ label }: Props) {\n  return <Rect x={10} y={20} width={200} />;\n}\n";
    let price = "export const Price = () => [\n  <PriceTag label=\"Sale\" />,\n  <PriceTag label=\"Second\" />,\n];\n";
    let files = Files::new("move-one", tag, price);
    let mut report = LayerSourceReport {
        render_count: 2,
        ..files.report(TAG_RECT, &files.price(), &[("width", 200.0)])
    };
    report.props.x = Some(10.0);
    report.props.y = Some(20.0);
    let targets = files.targets(&report);
    let repeated = EditTarget::Locked(LockReason::Repeated { count: 2 });
    let move_to = [number("x", 60.0), number("y", 80.0)];

    // Written on the element, the move would move both tags.
    assert_eq!(targets.target(&prop("x")), Some(&repeated));
    for scope in [None, Some(Scope::Shared)] {
        assert_eq!(
            files.commit(&targets, &report, scope, &move_to),
            Err("X can't be edited here".to_owned())
        );
    }
    // As the Sale call's one instance, resolved again as one, it lifts to
    // that call.
    let this_call = targets.one.as_deref().expect("resolved as one instance");
    let as_one = commit_report(&targets, this_call, &report);
    assert_eq!(as_one.render_count, 1);
    let entry = files
        .commit(this_call, &as_one, Some(Scope::ThisScene), &move_to)
        .unwrap();

    assert_eq!(
        files.read(&files.tag()),
        "interface Props {\n  label: string;\n  rectX?: number;\n  rectY?: number;\n}\n\nexport function PriceTag({ label, rectX = 10, rectY = 20 }: Props) {\n  return <Rect x={rectX} y={rectY} width={200} />;\n}\n"
    );
    assert_eq!(
        files.read(&files.price()),
        "export const Price = () => [\n  <PriceTag label=\"Sale\" rectX={60} rectY={80} />,\n  <PriceTag label=\"Second\" />,\n];\n"
    );
    // One entry, both files; undo restores both.
    assert_eq!(entry.len(), 2);
    let mut history = EditHistory::default();
    history.push(HistoryEntry { files: entry });
    history.undo().unwrap();
    assert_eq!(files.read(&files.tag()), tag);
    assert_eq!(files.read(&files.price()), price);
}

#[test]
fn removal_under_this_scene_deletes_at_s() {
    // "Use the shared value": the attribute at S goes.
    let files = Files::new("removal", LIFTED, PRICE_OWN);
    let report = files.report(LIFTED_RECT, &files.price(), &[("width", 260.0)]);
    let targets = files.targets(&report);

    let entry = files
        .commit(
            &targets,
            &report,
            Some(Scope::ThisScene),
            &[(prop("width"), None)],
        )
        .unwrap();

    assert_eq!(written(&entry), [files.price().as_path()]);
    assert_eq!(files.read(&files.price()), PRICE);
    assert_eq!(files.read(&files.tag()), LIFTED);

    // A literal at the element would reach every scene.
    let files = Files::new("removal-literal", TAG, PRICE);
    let report = files.report(TAG_RECT, &files.price(), &[("width", 200.0)]);
    let targets = files.targets(&report);
    let texts = Texts::from([
        (files.tag(), TAG.to_owned()),
        (files.price(), PRICE.to_owned()),
    ]);
    assert_eq!(
        commit_files(
            &texts,
            &targets,
            &report,
            Some(Scope::ThisScene),
            None,
            &[(prop("width"), None)]
        ),
        Err(CommitError::NotRemovable(prop("width").label()))
    );
}

#[test]
fn per_scene_unsupported_values_are_refused() {
    let in_memory = |tag: &str, report: &LayerSourceReport, table: &PrimitivePropertyTable| {
        let texts = Texts::from([
            (PathBuf::from("/project/price_tag.tsx"), tag.to_owned()),
            (PathBuf::from("/project/price.tsx"), PRICE.to_owned()),
        ]);
        let targets = resolve_targets(read_from(&texts), report, Some(table)).unwrap();
        (texts, targets)
    };
    let report_at = |name: &str| {
        let element = LayerSource {
            column: 10,
            ..entry_at(name, ElementKind::Primitive, "/project/price_tag.tsx", 3)
        };
        let call = entry_at("PriceTag", ElementKind::Component, "/project/price.tsx", 2);
        report_of(vec![element, call])
    };

    // A keyframe.
    let tag = "export function PriceTag({ label }) {\n  const frame = useCurrentFrame();\n  return <Rect x={interpolate(frame, [0, 10], [0, 100])} />;\n}\n";
    let mut report = report_at("Rect");
    report.props.x = Some(0.0);
    report.candidate_frames = vec![0.0];
    let (texts, targets) = in_memory(tag, &report, &tag_table());
    assert!(matches!(
        targets.target(&prop("x")),
        Some(EditTarget::Keyframe { .. })
    ));
    assert_eq!(
        commit_files(
            &texts,
            &targets,
            &report,
            Some(Scope::ThisScene),
            None,
            &[number("x", 50.0)]
        ),
        Err(CommitError::Scope(ScopeLock::PerSceneUnsupported))
    );

    // A `style` property.
    let tag = "export function PriceTag({ label }) {\n  // The size is set in `style`.\n  return <Text style={{ fontSize: 40 }}>Sale</Text>;\n}\n";
    let mut report = report_at("Text");
    for (name, value) in [
        ("style.fontSize", ReceivedValue::Number(40.0)),
        ("children", ReceivedValue::String("Sale".to_owned())),
    ] {
        report.props.others.insert(name.to_owned(), value);
    }
    let (texts, targets) = in_memory(tag, &report, &text_table());
    assert_eq!(
        targets.entry(&prop("fontSize")).unwrap().path,
        "style.fontSize"
    );
    let refused = commit_files(
        &texts,
        &targets,
        &report,
        Some(Scope::ThisScene),
        None,
        &[number("fontSize", 48.0)],
    );
    assert_eq!(
        refused,
        Err(CommitError::Scope(ScopeLock::PerSceneUnsupported))
    );
    assert_eq!(
        refused.unwrap_err().to_string(),
        "Can't be set per scene yet"
    );
}

#[test]
fn a_locked_lift_is_refused_with_its_text() {
    let tag = TAG.replace(
        "interface Props {\n  label: string;\n}\n",
        "import type { Props } from \"./props\";\n\n\n",
    );
    let files = Files::new("locked-lift", &tag, PRICE);
    let report = files.report(TAG_RECT, &files.price(), &[("width", 200.0)]);
    let targets = files.targets(&report);

    let refused = files.commit(
        &targets,
        &report,
        Some(Scope::ThisScene),
        &[number("width", 260.0)],
    );

    assert_eq!(
        refused,
        Err("`PriceTag`'s props type is in another file".to_owned())
    );
    assert_eq!(files.read(&files.tag()), tag);
    assert_eq!(files.read(&files.price()), PRICE);
}

#[test]
fn scoped_writes_outside_the_project_are_refused() {
    // Price leaves `rectWidth` out, so "Shared value" writes its default.
    let texts = Texts::from([
        (PathBuf::from("/project/price_tag.tsx"), LIFTED.to_owned()),
        (PathBuf::from("/project/price.tsx"), PRICE.to_owned()),
    ]);
    let rect = LayerSource {
        column: 10,
        ..entry_at(
            "Rect",
            ElementKind::Primitive,
            "/project/price_tag.tsx",
            LIFTED_RECT,
        )
    };
    let call = entry_at("PriceTag", ElementKind::Component, "/project/price.tsx", 2);
    let mut report = report_of(vec![rect, call]);
    report
        .props
        .others
        .insert("width".to_owned(), ReceivedValue::Number(200.0));
    let resolve = |outside: &str| {
        let mut targets = resolve_targets(read_from(&texts), &report, Some(&tag_table())).unwrap();
        lock_outside_project(&mut targets, |file| file == Path::new(outside));
        targets
    };
    let outside = Err(CommitError::Scope(ScopeLock::OutsideProject));

    // The default is written in the component's file.
    let targets = resolve("/project/price_tag.tsx");
    assert!(matches!(
        targets.target(&prop("width")),
        Some(EditTarget::PassThrough { .. })
    ));
    assert_eq!(
        commit_files(
            &texts,
            &targets,
            &report,
            Some(Scope::Shared),
            None,
            &[number("width", 240.0)]
        ),
        outside
    );
    // A lift writes S's file too.
    let targets = resolve("/project/price.tsx");
    assert_eq!(
        commit_files(
            &texts,
            &targets,
            &report,
            Some(Scope::ThisScene),
            None,
            &[number("x", 60.0)]
        ),
        outside
    );
    assert_eq!(
        ScopeLock::OutsideProject.text(),
        "This file is outside the project"
    );
}

#[test]
fn commit_scoped_takes_any_element_s_targets() {
    let tag = "export function PriceTag({ label }: { label: string }) {\n  return <Sequence from={30}><Rect width={200} /></Sequence>;\n}\n";
    let (tag_file, price_file) = (
        PathBuf::from("/project/price_tag.tsx"),
        PathBuf::from("/project/price.tsx"),
    );
    let texts = Texts::from([
        (tag_file.clone(), tag.to_owned()),
        (price_file.clone(), PRICE.to_owned()),
    ]);
    let from_at = |text: &str| {
        let start = text.find("30}").unwrap() as u32;
        EditTarget::Literal {
            span: Span {
                start,
                end: start + 2,
            },
            value: EditValue::Number(30.0),
            form: LiteralForm::Number,
        }
    };
    let call: ElementRef = element_at(PRICE, 2, 3).unwrap();
    let sequence = ElementTargets {
        file: tag_file.clone(),
        files: vec![tag_file.clone(), price_file.clone()],
        element: element_at(tag, 2, 10).unwrap(),
        targets: vec![(
            prop("from"),
            Target {
                file: tag_file.clone(),
                path: "from".to_owned(),
                target: from_at(tag),
                call_site: None,
                inputs: Vec::new(),
            },
        )],
        table: None,
        component: Some(ComponentTargets {
            name: "PriceTag".to_owned(),
            call_site: SourcePos {
                file: price_file.clone(),
                line: 2,
                column: 3,
            },
            call,
            props: Vec::new(),
            schema: None,
        }),
        enclosing: Some("PriceTag".to_owned()),
        defaults: Vec::new(),
        lift_lock: None,
        outside: Vec::new(),
        one: None,
    };
    let values = [number("from", 45.0)];

    let written = commit_scoped(
        &texts,
        &sequence,
        |_| Ok(sequence.clone()),
        Some(Scope::ThisScene),
        None,
        &values,
    )
    .unwrap();

    // In path order.
    assert_eq!(
        written,
        [
            (
                price_file,
                "export const Price = () => (\n  <PriceTag label=\"Sale\" sequenceFrom={45} />\n);\n"
                    .to_owned()
            ),
            (
                tag_file.clone(),
                "export function PriceTag({ label, sequenceFrom = 30 }: { label: string; sequenceFrom?: number }) {\n  return <Sequence from={sequenceFrom}><Rect width={200} /></Sequence>;\n}\n".to_owned()
            ),
        ]
    );

    // Resolved again somewhere else: the source changed.
    let mut moved = sequence.clone();
    moved.targets[0].1.target = from_at(&tag.replace("from={30}", "from={ 30}"));
    assert_eq!(
        commit_scoped(
            &texts,
            &sequence,
            |_| Ok(moved),
            Some(Scope::ThisScene),
            None,
            &values,
        ),
        Err(CommitError::SourceChanged)
    );
}

fn pos(file: &str, line: u32, column: u32) -> SourcePos {
    SourcePos {
        file: PathBuf::from(file),
        line,
        column,
    }
}

/// [`LIFTED`] with a literal `x`, called by Price, resolved in memory.
fn gate_fixture() -> (LayerSourceReport, ElementTargets) {
    let tag = LIFTED.replace("<Rect width", "<Rect x={10} width");
    let texts = Texts::from([
        (PathBuf::from("/project/price_tag.tsx"), tag),
        (PathBuf::from("/project/price.tsx"), PRICE_OWN.to_owned()),
    ]);
    let rect = LayerSource {
        column: 10,
        ..entry_at("Rect", ElementKind::Primitive, "/project/price_tag.tsx", 7)
    };
    let call = entry_at("PriceTag", ElementKind::Component, "/project/price.tsx", 2);
    let mut report = report_of(vec![rect, call]);
    report.props.x = Some(10.0);
    report
        .props
        .others
        .insert("width".to_owned(), ReceivedValue::Number(260.0));
    let targets = resolve_targets(read_from(&texts), &report, Some(&tag_table())).unwrap();
    (report, targets)
}

fn chapters() -> Vec<ReactChapter> {
    vec![
        ReactChapter {
            id: "intro".to_owned(),
            name: "Intro".to_owned(),
            from: 0,
            duration_in_frames: 30,
        },
        ReactChapter {
            id: "price".to_owned(),
            name: "Price".to_owned(),
            from: 30,
            duration_in_frames: 30,
        },
    ]
}

/// `PriceTag`'s `<Rect>` (at `element`) called by Price and Intro.
fn two_scenes(element: SourcePos) -> OccurrenceIndex {
    let instance = |call: SourcePos, ranges| InstanceOccurrences {
        chain: vec![element.clone(), call],
        ranges,
        max_per_frame: 1,
        wrappers: Vec::new(),
    };
    OccurrenceIndex {
        elements: vec![ElementOccurrences {
            element: element.clone(),
            instances: vec![
                instance(pos("/project/price.tsx", 2, 3), vec![(30, 60)]),
                instance(pos("/project/intro.tsx", 2, 3), vec![(0, 30)]),
            ],
        }],
    }
}

#[test]
fn commit_gate_holds_until_the_index_answers() {
    let (report, targets) = gate_fixture();
    let rect = pos("/project/price_tag.tsx", 7, 10);
    let state = |occurrences: &OccurrenceState| {
        scope_state(&report, &targets, Some(occurrences), None, &chapters(), 40)
    };
    let gate = |state: &ScopeState, remembered, key: PropKey| {
        commit_gate(state, remembered, &targets, &[key])
    };
    let scanning = "Scanning scenes…".to_owned();

    // Scanning: a value passed at S is written there as resolved; anything
    // else waits.
    let unknown = state(&OccurrenceState::Scanning);
    assert!(matches!(
        targets.target(&prop("width")),
        Some(EditTarget::PassThrough { target, .. })
            if matches!(**target, EditTarget::Literal { .. })
    ));
    assert_eq!(gate(&unknown, None, prop("width")), CommitGate::Go(None));
    assert_eq!(
        gate(&unknown, Some(Scope::ThisScene), prop("width")),
        CommitGate::Go(None)
    );
    assert_eq!(
        gate(&unknown, Some(Scope::Shared), prop("width")),
        CommitGate::Hold(scanning.clone())
    );
    assert_eq!(
        gate(&unknown, None, prop("x")),
        CommitGate::Hold(scanning.clone())
    );
    assert_eq!(
        gate(&unknown, None, PropKey::CallSite("label".to_owned())),
        CommitGate::Go(None)
    );
    // A refreshing index that no longer finds the element (a lift moved
    // it) waits for the sweep under way.
    let moved = state(&OccurrenceState::Refreshing(two_scenes(pos(
        "/project/price_tag.tsx",
        9,
        10,
    ))));
    assert_eq!(gate(&moved, None, prop("x")), CommitGate::Hold(scanning));
    // No sweep would answer these: a value at S still goes as resolved,
    // anything else is refused.
    let failed = state(&OccurrenceState::Failed("x".to_owned()));
    assert_eq!(
        gate(&failed, None, prop("x")),
        CommitGate::Refuse("Couldn't scan scenes: x".to_owned())
    );
    assert_eq!(gate(&failed, None, prop("width")), CommitGate::Go(None));
    assert_eq!(
        gate(&failed, Some(Scope::Shared), prop("width")),
        CommitGate::Refuse("Couldn't scan scenes: x".to_owned())
    );
    let missed = state(&OccurrenceState::Ready(two_scenes(pos(
        "/project/price_tag.tsx",
        9,
        10,
    ))));
    assert_eq!(
        gate(&missed, None, prop("x")),
        CommitGate::Refuse("Not found in the scene scan".to_owned())
    );
    assert_eq!(gate(&missed, None, prop("width")), CommitGate::Go(None));
    // No scan runs (Select layers off): never held.
    let unscanned = scope_state(&report, &targets, None, None, &chapters(), 40);
    assert_eq!(
        gate(&unscanned, None, prop("x")),
        CommitGate::Refuse("The scenes are not scanned".to_owned())
    );
    assert_eq!(gate(&unscanned, None, prop("width")), CommitGate::Go(None));

    // Answered.
    assert_eq!(
        gate(&ScopeState::NotShared, None, prop("x")),
        CommitGate::Go(None)
    );
    let ready = state(&OccurrenceState::Ready(two_scenes(rect.clone())));
    assert_eq!(
        gate(&ready, None, prop("x")),
        CommitGate::Go(Some(Scope::ThisScene))
    );
    assert_eq!(
        gate(&ready, Some(Scope::Shared), prop("x")),
        CommitGate::Go(Some(Scope::Shared))
    );
    // A remembered "This scene" now locked is refused, never widened.
    let mut spread = two_scenes(rect);
    spread.elements[0].instances[0].ranges = vec![(0, 60)];
    let locked = state(&OccurrenceState::Ready(spread));
    let lock = ScopeLock::OtherScenes {
        component: "PriceTag".to_owned(),
    };
    assert_eq!(
        gate(&locked, Some(Scope::ThisScene), prop("x")),
        CommitGate::Refuse(lock.text())
    );
    assert_eq!(
        gate(&locked, None, prop("x")),
        CommitGate::Go(Some(Scope::Shared))
    );
}

#[test]
fn a_held_commit_keeps_the_scope_it_was_made_with() {
    let held = |value: f64, scope| HeldCommit {
        layer: "tag".to_owned(),
        key: prop("width"),
        value: Some(EditValue::Number(value)),
        frame: 0,
        scope,
        scanning: false,
        gesture: false,
    };
    let mut commits = HeldCommits::default();
    commits.push(held(1.0, Some(Scope::ThisScene)));
    commits.push(held(2.0, Some(Scope::Shared)));

    assert_eq!(
        commits.pop(Some("tag")),
        Some(held(2.0, Some(Scope::Shared)))
    );
    assert_eq!(commits.pop(Some("tag")), None);
}

#[test]
fn a_shared_edit_this_scene_sets_its_own_locks_gestures() {
    let (report, targets) = gate_fixture();
    let calls = SceneCalls {
        element: SourcePos::from(&report.chain[0]),
        generation: 1,
        calls: vec![
            Some((
                pos("/project/price.tsx", 2, 3),
                vec!["label".to_owned(), "rectWidth".to_owned()],
            )),
            Some((pos("/project/intro.tsx", 2, 3), vec!["label".to_owned()])),
        ],
    };

    // Price passes `rectWidth`: a shared edit would not show here.
    assert_eq!(
        gesture_lock(Some(Scope::Shared), &targets, &prop("width"), Some(&calls)),
        Some(ScopeLock::SetsOwn {
            param: "rectWidth".to_owned()
        })
    );
    assert_eq!(
        gesture_lock(
            Some(Scope::ThisScene),
            &targets,
            &prop("width"),
            Some(&calls)
        ),
        None
    );
    assert_eq!(
        gesture_lock(None, &targets, &prop("width"), Some(&calls)),
        None
    );
    // A row locked under the scope says why.
    assert_eq!(
        gesture_lock(Some(Scope::Shared), &targets, &prop("x"), Some(&calls)),
        None
    );
    let mut locked = targets.clone();
    locked.lift_lock = Some(celesta_source_edit::LiftLock::NotObjectPattern {
        component: "PriceTag".to_owned(),
    });
    assert_eq!(
        gesture_lock(Some(Scope::ThisScene), &locked, &prop("x"), Some(&calls)),
        Some(ScopeLock::NotObjectPattern {
            component: "PriceTag".to_owned()
        })
    );
}

#[test]
fn scene_calls_count_only_for_the_index_and_call_they_were_read_for() {
    let (report, targets) = gate_fixture();
    let rect = pos("/project/price_tag.tsx", 7, 10);
    let index = two_scenes(rect.clone());
    let element = &index.elements[0];
    let s = &targets.component.as_ref().unwrap().call_site;
    let calls = SceneCalls {
        element: rect,
        generation: 4,
        calls: vec![
            Some((
                pos("/project/price.tsx", 2, 3),
                vec!["rectWidth".to_owned()],
            )),
            Some((pos("/project/intro.tsx", 2, 3), Vec::new())),
        ],
    };

    assert_eq!(
        current_calls(Some(&calls), 4, &report, element, s),
        Some(&calls)
    );
    // Read for an older index.
    assert_eq!(current_calls(Some(&calls), 5, &report, element, s), None);
    // The index's calls are elsewhere now.
    let mut moved = index.clone();
    moved.elements[0].instances[1].chain[1] = pos("/project/intro.tsx", 4, 3);
    assert_eq!(
        current_calls(Some(&calls), 4, &report, &moved.elements[0], s),
        None
    );
    assert_eq!(current_calls(None, 4, &report, element, s), None);
}

#[test]
fn scene_calls_read_each_call_s_attributes() {
    let texts = Texts::from([
        (PathBuf::from("/project/price.tsx"), PRICE_OWN.to_owned()),
        (PathBuf::from("/project/intro.tsx"), INTRO.to_owned()),
    ]);
    let rect = pos("/project/price_tag.tsx", 7, 10);

    let calls = read_scene_calls(
        read_from(&texts),
        rect.clone(),
        3,
        vec![
            Some(pos("/project/price.tsx", 2, 3)),
            Some(pos("/project/intro.tsx", 2, 3)),
            Some(pos("/project/outro.tsx", 2, 3)),
            None,
        ],
    );

    let call = element_at(PRICE_OWN, 2, 3).unwrap();
    assert_eq!(written_attributes(PRICE_OWN, &call), ["label", "rectWidth"]);
    assert_eq!(
        calls,
        SceneCalls {
            element: rect,
            generation: 3,
            calls: vec![
                Some((
                    pos("/project/price.tsx", 2, 3),
                    vec!["label".to_owned(), "rectWidth".to_owned()]
                )),
                Some((pos("/project/intro.tsx", 2, 3), vec!["label".to_owned()])),
                None,
                None,
            ],
        }
    );
}
