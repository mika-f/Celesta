//! Tests of an element's timing window as an element of its own: the
//! wrapper's report, its timing props as rows, and the commit that writes
//! them (as resolved or under an edit scope) as one history entry.

use super::*;
use crate::element_edit::{WindowTargets, resolve_window, window_files, window_report};
use crate::scope::Scope;
use celesta_source_edit::TimingKind;

/// `<Sequence from={30}>` in a scene function at 4:3, around a `<Rect>` at
/// 5:5.
const SEQUENCE: &str = "import { Sequence } from \"@celesta/react\";\n\nexport const Intro = () => (\n  <Sequence from={30}>\n    <Rect width={200} />\n  </Sequence>\n);\n";

/// Three `Series` items of 60 frames at 5:5, 8:5 and 11:5.
const SERIES: &str = "import { Series } from \"@celesta/react\";\n\nexport const Intro = () => (\n  <Series>\n    <Series.Sequence durationInFrames={60}>\n      <Rect />\n    </Series.Sequence>\n    <Series.Sequence durationInFrames={60}>\n      <Rect />\n    </Series.Sequence>\n    <Series.Sequence durationInFrames={60}>\n      <Rect />\n    </Series.Sequence>\n  </Series>\n);\n";

/// A `<FreezeFrame>` at 4:3.
const FREEZE: &str = "import { FreezeFrame } from \"@celesta/react\";\n\nexport const Intro = () => (\n  <FreezeFrame frame={10}>\n    <Rect />\n  </FreezeFrame>\n);\n";

/// `Fade` passes its `start` to the `<Sequence>` at 4:5.
const FADE: &str = "import { Sequence } from \"@celesta/react\";\n\nexport function Fade({ start }) {\n  return (\n    <Sequence from={start}>\n      <Rect />\n    </Sequence>\n  );\n}\n";

/// The scene's call of `Fade` at 2:3.
const FADE_CALL: &str = "export const Intro = () => (\n  <Fade start={30} />\n);\n";

/// `PriceTag` writes `<Sequence from={30}>` at 9:5.
const TAG: &str = "import { Sequence } from \"@celesta/react\";\n\ninterface Props {\n  label: string;\n}\n\nexport function PriceTag({ label }: Props) {\n  return (\n    <Sequence from={30}>\n      <Text>{label}</Text>\n    </Sequence>\n  );\n}\n";

/// [`TAG`] after "This scene" lifted `from`.
const TAG_LIFTED: &str = "import { Sequence } from \"@celesta/react\";\n\ninterface Props {\n  label: string;\n  sequenceFrom?: number;\n}\n\nexport function PriceTag({ label, sequenceFrom = 30 }: Props) {\n  return (\n    <Sequence from={sequenceFrom}>\n      <Text>{label}</Text>\n    </Sequence>\n  );\n}\n";

/// The Price scene's call of `PriceTag` at 2:3.
const PRICE: &str = "export const Price = () => (\n  <PriceTag label=\"Sale\" />\n);\n";

/// The Intro scene's call of `PriceTag` at 2:3.
const INTRO: &str = "export const Intro = () => (\n  <PriceTag label=\"Hello\" />\n);\n";

/// Source files in a directory of their own, removed when dropped.
struct Files {
    dir: PathBuf,
}

impl Files {
    fn new(name: &str, files: &[(&str, &str)]) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "celesta-timing-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        for (file, text) in files {
            fs::write(dir.join(file), text).unwrap();
        }
        Self { dir }
    }

    fn path(&self, file: &str) -> PathBuf {
        self.dir.join(file)
    }

    fn read(&self, file: &str) -> String {
        fs::read_to_string(self.path(file)).unwrap()
    }

    fn entry(&self, name: &str, kind: ElementKind, file: &str, at: (u32, u32)) -> LayerSource {
        LayerSource {
            column: at.1,
            ..entry_at(name, kind, self.path(file).to_str().unwrap(), at.0)
        }
    }

    fn pos(&self, file: &str, line: u32, column: u32) -> SourcePos {
        SourcePos {
            file: self.path(file),
            line,
            column,
        }
    }

    fn window(&self, report: &LayerSourceReport, source: &SourcePos) -> WindowTargets {
        let read = |path: &Path| fs::read_to_string(path).map_err(|error| error.to_string());
        resolve_window(read, report, source).expect("the window resolves")
    }

    /// Commits `values` to `window` under `scope` as the editor does: one
    /// write of every file the targets read. The files written.
    fn commit(
        &self,
        window: &WindowTargets,
        report: &LayerSourceReport,
        scope: Option<Scope>,
        values: &[(PropKey, Option<EditValue>)],
    ) -> Result<Vec<crate::edit_history::FileEdit>, String> {
        write_all_recorded(window.targets.files.clone(), |texts| {
            window_files(texts, window, report, scope, None, values)
                .map_err(|error| error.to_string())
        })
    }
}

impl Drop for Files {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.dir).ok();
    }
}

fn number(key: &str, value: f64) -> (PropKey, Option<EditValue>) {
    (prop(key), Some(EditValue::Number(value)))
}

/// The keys of the window's rows, in order.
fn row_keys(window: &WindowTargets) -> Vec<PropKey> {
    window
        .targets
        .targets
        .iter()
        .map(|(key, _)| key.clone())
        .collect()
}

/// The `<Rect>` at 5:5 of [`SEQUENCE`] in `scene.tsx`, inside its
/// `<Sequence>` at 4:3.
fn sequence_report(files: &Files) -> LayerSourceReport {
    report_of(vec![
        files.entry("Rect", ElementKind::Primitive, "scene.tsx", (5, 5)),
        files.entry("Sequence", ElementKind::Wrapper, "scene.tsx", (4, 3)),
    ])
}

/// The `<Rect>` of the `Series` item at `line`:5 of [`SERIES`] in
/// `scene.tsx`.
fn series_report(files: &Files, line: u32) -> LayerSourceReport {
    report_of(vec![
        files.entry("Rect", ElementKind::Primitive, "scene.tsx", (line + 1, 7)),
        files.entry("Sequence", ElementKind::Wrapper, "scene.tsx", (line, 5)),
    ])
}

#[test]
fn window_report_starts_at_the_wrapper_entry() {
    let rect = entry_at("Rect", ElementKind::Primitive, "/a/title.tsx", 5);
    let sequence = entry_at("Sequence", ElementKind::Wrapper, "/a/title.tsx", 4);
    let intro = entry_at("Intro", ElementKind::Component, "/a/film.tsx", 9);
    let mut report = report_of(vec![rect.clone(), sequence.clone(), intro.clone()]);
    report.props.x = Some(40.0);
    report.render_count = 3;

    let window = window_report(&report, &SourcePos::from(&sequence)).unwrap();
    assert_eq!(window.chain, [sequence, intro]);
    assert_eq!(window.props, ReceivedProps::default());
    assert_eq!(window.render_count, 1);

    let elsewhere = SourcePos {
        file: PathBuf::from("/a/title.tsx"),
        line: 12,
        column: 3,
    };
    assert_eq!(window_report(&report, &elsewhere), None);
}

#[test]
fn resolve_window_reads_the_timing_props() {
    let files = Files::new(
        "read",
        &[
            ("scene.tsx", SEQUENCE),
            ("series.tsx", SERIES),
            ("freeze.tsx", FREEZE),
        ],
    );
    let read = |path: &Path| fs::read_to_string(path).map_err(|error| error.to_string());

    let source = files.pos("scene.tsx", 4, 3);
    let window = files.window(&sequence_report(&files), &source);
    assert_eq!(window.source, source);
    assert_eq!(window.kind, TimingKind::Sequence);
    assert_eq!(row_keys(&window), [prop("from"), prop("durationInFrames")]);
    assert!(matches!(
        window.targets.target(&prop("from")),
        Some(EditTarget::Literal {
            value: EditValue::Number(30.0),
            ..
        })
    ));
    assert!(matches!(
        window.targets.target(&prop("durationInFrames")),
        Some(EditTarget::Missing { .. })
    ));
    assert!(window.followers.is_empty());

    let at = |line| files.pos("series.tsx", line, 5);
    let series = report_of(vec![
        files.entry("Rect", ElementKind::Primitive, "series.tsx", (6, 7)),
        files.entry("Sequence", ElementKind::Wrapper, "series.tsx", (5, 5)),
    ]);
    let item = files.window(&series, &at(5));
    assert_eq!(item.kind, TimingKind::SeriesSequence);
    assert_eq!(row_keys(&item), [prop("durationInFrames"), prop("offset")]);
    assert!(matches!(
        item.targets.target(&prop("durationInFrames")),
        Some(EditTarget::Literal {
            value: EditValue::Number(60.0),
            ..
        })
    ));
    assert!(matches!(
        item.targets.target(&prop("offset")),
        Some(EditTarget::Missing { .. })
    ));
    assert_eq!(item.followers, [at(8), at(11)]);

    let freeze = report_of(vec![
        files.entry("Rect", ElementKind::Primitive, "freeze.tsx", (5, 5)),
        files.entry("FreezeFrame", ElementKind::Wrapper, "freeze.tsx", (4, 3)),
    ]);
    assert!(resolve_window(read, &freeze, &files.pos("freeze.tsx", 4, 3)).is_err());
}

#[test]
fn window_files_writes_a_literal() {
    let files = Files::new("literal", &[("scene.tsx", SEQUENCE)]);
    let report = sequence_report(&files);
    let window = files.window(&report, &files.pos("scene.tsx", 4, 3));

    let entry = files
        .commit(&window, &report, None, &[number("from", 45.0)])
        .unwrap();

    assert_eq!(entry.len(), 1);
    assert_eq!(
        files.read("scene.tsx"),
        SEQUENCE.replace("from={30}", "from={45}")
    );
}

#[test]
fn window_files_inserts_a_missing_duration() {
    let files = Files::new("missing", &[("scene.tsx", SEQUENCE)]);
    let report = sequence_report(&files);
    let window = files.window(&report, &files.pos("scene.tsx", 4, 3));

    files
        .commit(&window, &report, None, &[number("durationInFrames", 75.0)])
        .unwrap();

    assert_eq!(
        files.read("scene.tsx"),
        SEQUENCE.replace("from={30}>", "from={30} durationInFrames={75}>")
    );
}

#[test]
fn window_files_writes_a_pass_through_at_the_call_site() {
    let files = Files::new(
        "pass-through",
        &[("fade.tsx", FADE), ("scene.tsx", FADE_CALL)],
    );
    let report = report_of(vec![
        files.entry("Rect", ElementKind::Primitive, "fade.tsx", (6, 7)),
        files.entry("Sequence", ElementKind::Wrapper, "fade.tsx", (5, 5)),
        files.entry("Fade", ElementKind::Component, "scene.tsx", (2, 3)),
    ]);
    let window = files.window(&report, &files.pos("fade.tsx", 5, 5));
    assert!(matches!(
        window.targets.target(&prop("from")),
        Some(EditTarget::PassThrough { .. })
    ));

    let entry = files
        .commit(&window, &report, None, &[number("from", 45.0)])
        .unwrap();

    let written: Vec<&Path> = entry.iter().map(|file| file.path.as_path()).collect();
    assert_eq!(written, [files.path("scene.tsx").as_path()]);
    assert_eq!(
        files.read("scene.tsx"),
        FADE_CALL.replace("start={30}", "start={45}")
    );
    assert_eq!(files.read("fade.tsx"), FADE);
}

#[test]
fn window_files_changes_only_the_dragged_series_item() {
    let files = Files::new("series", &[("scene.tsx", SERIES)]);
    let report = series_report(&files, 8);
    let window = files.window(&report, &files.pos("scene.tsx", 8, 5));

    files
        .commit(&window, &report, None, &[number("durationInFrames", 75.0)])
        .unwrap();

    let expected: Vec<String> = SERIES
        .split('\n')
        .enumerate()
        .map(|(index, line)| match index + 1 {
            8 => line.replace("{60}", "{75}"),
            _ => line.to_owned(),
        })
        .collect();
    assert_eq!(files.read("scene.tsx"), expected.join("\n"));
}

#[test]
fn window_files_lifts_under_this_scene() {
    let files = Files::new(
        "lift",
        &[
            ("price_tag.tsx", TAG),
            ("price.tsx", PRICE),
            ("intro.tsx", INTRO),
        ],
    );
    let report = report_of(vec![
        files.entry("Text", ElementKind::Primitive, "price_tag.tsx", (10, 7)),
        files.entry("Sequence", ElementKind::Wrapper, "price_tag.tsx", (9, 5)),
        files.entry("PriceTag", ElementKind::Component, "price.tsx", (2, 3)),
    ]);
    let window = files.window(&report, &files.pos("price_tag.tsx", 9, 5));
    assert_eq!(window.targets.enclosing.as_deref(), Some("PriceTag"));

    let entry = files
        .commit(
            &window,
            &report,
            Some(Scope::ThisScene),
            &[number("from", 45.0)],
        )
        .unwrap();

    assert_eq!(files.read("price_tag.tsx"), TAG_LIFTED);
    assert_eq!(
        files.read("price.tsx"),
        "export const Price = () => (\n  <PriceTag label=\"Sale\" sequenceFrom={45} />\n);\n"
    );
    assert_eq!(files.read("intro.tsx"), INTRO);
    // One entry, both files; undo restores both.
    assert_eq!(entry.len(), 2);
    let mut history = EditHistory::default();
    history.push(HistoryEntry { files: entry });
    history.undo().unwrap();
    assert_eq!(files.read("price_tag.tsx"), TAG);
    assert_eq!(files.read("price.tsx"), PRICE);
}

#[test]
fn window_files_refuses_a_changed_source() {
    let files = Files::new("changed", &[("scene.tsx", SEQUENCE)]);
    let report = sequence_report(&files);
    let window = files.window(&report, &files.pos("scene.tsx", 4, 3));
    let moved = SEQUENCE.replace("\n\nexport const", "\n\n// The intro.\nexport const");
    fs::write(files.path("scene.tsx"), &moved).unwrap();

    let result = files.commit(&window, &report, None, &[number("from", 45.0)]);

    assert_eq!(result.err(), Some(CommitError::SourceChanged.to_string()));
    assert_eq!(files.read("scene.tsx"), moved);
}

#[test]
fn a_window_that_does_not_resolve_keeps_why() {
    let files = Files::new("unresolved", &[("scene.tsx", SEQUENCE)]);
    let report = sequence_report(&files);
    let read = |path: &Path| fs::read_to_string(path).map_err(|error| error.to_string());
    // A window whose source no chain entry has.
    let elsewhere = resolve_window(read, &report, &files.pos("scene.tsx", 2, 1));
    let why = "Couldn't find this element in the source";
    assert_eq!(elsewhere.as_ref().err().map(String::as_str), Some(why));

    let mut state = ElementEdit::default();
    let request = state.begin("a", &report, 0).unwrap();
    assert!(state.finish_with_window(
        request,
        Some("a"),
        Err("no rows".to_owned()),
        Some(elsewhere)
    ));
    let (window, _) = state
        .window_targets(Some("a"), Some((&report, 0)))
        .expect("the window's outcome is kept beside the element's");
    assert_eq!(window.err(), Some(why));
}
