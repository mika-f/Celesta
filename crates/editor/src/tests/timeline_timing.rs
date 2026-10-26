//! Tests of an element's timing window as an element of its own: the
//! wrapper's report, its timing props as rows, and the commit that writes
//! them (as resolved or under an edit scope) as one history entry.

use super::*;
use crate::element_edit::{WindowTargets, resolve_window, window_files, window_report};
use crate::occurrences::OccurrenceState;
use crate::scope::{
    CommitGate, SceneCalls, Scope, ScopeLock, ScopeOptions, ScopeState, commit_scope, gesture_gate,
    window_scope_state,
};
use crate::timeline_drag::{BandGrip, band_writes, grip_refusal, timing_values};
use crate::timing::{SetTimingOffer, WindowRole, set_timing_files, set_timing_offer};
use celesta_react_bridge::{
    ElementOccurrences, InstanceOccurrences, OccurrenceIndex, ReactChapter, TimingWindow,
};
use celesta_source_edit::{ElementRef, ImportLock, TimingKind, WrapLock};

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

/// A `<Rect>` at 5:5 without timing of its own.
const UNTIMED: &str = "import { Rect } from \"@celesta/react\";\n\nexport const Intro = () => (\n  <>\n    <Rect width={200} />\n  </>\n);\n";

/// [`UNTIMED`] after "Set timing" at frame 45 of a 120-frame scene.
const TIMED: &str = "import { Rect, Sequence } from \"@celesta/react\";\n\nexport const Intro = () => (\n  <>\n    <Sequence from={45} durationInFrames={75}>\n      <Rect width={200} />\n    </Sequence>\n  </>\n);\n";

/// Indented with tabs, `Sequence` imported as `Seq`: a `<Rect>` over
/// three lines at 5:3.
const TABS: &str = "import { Rect, Sequence as Seq } from \"@celesta/react\";\n\nexport const Intro = () => (\n\t<>\n\t\t<Rect\n\t\t\twidth={200}\n\t\t/>\n\t</>\n);\n";

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
fn a_drag_of_a_lifted_prop_the_call_leaves_out_starts_at_its_default() {
    // After a "This scene" lift in Price, Intro's call doesn't pass
    // `sequenceFrom`: its window starts at the default, 30.
    let files = Files::new(
        "default",
        &[("price_tag.tsx", TAG_LIFTED), ("intro.tsx", INTRO)],
    );
    let report = report_of(vec![
        files.entry("Text", ElementKind::Primitive, "price_tag.tsx", (11, 7)),
        files.entry("Sequence", ElementKind::Wrapper, "price_tag.tsx", (10, 5)),
        files.entry("PriceTag", ElementKind::Component, "intro.tsx", (2, 3)),
    ]);
    let window = files.window(&report, &files.pos("price_tag.tsx", 10, 5));
    let now = timing_values(window.kind, &window.targets, (30.0, 90.0));
    assert_eq!(now.start, 30.0);
    let values = band_writes(window.kind, BandGrip::Body, 15, now, &window.targets);
    // The window has no `durationInFrames`: the body adds it.
    assert_eq!(
        values,
        [number("from", 45.0), number("durationInFrames", 60.0)]
    );

    let entry = files.commit(&window, &report, None, &values).unwrap();

    assert_eq!(entry.len(), 2);
    assert_eq!(
        files.read("intro.tsx"),
        "export const Intro = () => (\n  <PriceTag label=\"Hello\" sequenceFrom={45} />\n);\n"
    );
    assert_eq!(
        files.read("price_tag.tsx"),
        TAG_LIFTED.replace(
            "from={sequenceFrom}>",
            "from={sequenceFrom} durationInFrames={60}>"
        )
    );
}

/// `Delayed` wraps its children in `<Sequence from={30}>` at 4:10.
const DELAYED: &str = "import { Sequence } from \"@celesta/react\";\n\nexport function Delayed({ children }) {\n  return <Sequence from={30}>{children}</Sequence>;\n}\n";

/// Intro calls `Delayed` at 2:3 around a `<Rect>` at 3:5 and a
/// `<Circle>` at 4:5.
const INTRO_DELAYED: &str =
    "export const Intro = () => (\n  <Delayed>\n    <Rect />\n    <Circle />\n  </Delayed>\n);\n";

/// Main calls `Delayed` at 2:3 around a `<Text>` at 3:5.
const MAIN_DELAYED: &str =
    "export const Main = () => (\n  <Delayed>\n    <Text>Hi</Text>\n  </Delayed>\n);\n";

fn instance_of(
    chain: Vec<SourcePos>,
    ranges: Vec<(u64, u64)>,
    wrappers: Vec<SourcePos>,
) -> InstanceOccurrences {
    InstanceOccurrences {
        chain,
        ranges,
        max_per_frame: 1,
        wrappers,
    }
}

fn scene(id: &str, from: u64, duration_in_frames: u64) -> ReactChapter {
    ReactChapter {
        id: id.to_owned(),
        name: id.to_owned(),
        from,
        duration_in_frames,
    }
}

#[test]
fn a_window_two_scenes_call_is_shared_by_its_own_instances() {
    // `Delayed` is called in Intro and in Main, around other elements: the
    // `<Rect>` selected in Intro has one instance, but writing the
    // `<Sequence>` in `Delayed` would move Main's `<Text>` too.
    let files = Files::new(
        "delayed",
        &[
            ("delayed.tsx", DELAYED),
            ("intro.tsx", INTRO_DELAYED),
            ("main.tsx", MAIN_DELAYED),
        ],
    );
    let report = report_of(vec![
        files.entry("Rect", ElementKind::Primitive, "intro.tsx", (3, 5)),
        files.entry("Sequence", ElementKind::Wrapper, "delayed.tsx", (4, 10)),
        files.entry("Delayed", ElementKind::Component, "intro.tsx", (2, 3)),
    ]);
    let source = files.pos("delayed.tsx", 4, 10);
    let window = files.window(&report, &source);
    let intro_call = files.pos("intro.tsx", 2, 3);
    assert_eq!(
        window
            .targets
            .component
            .as_ref()
            .map(|component| &component.call_site),
        Some(&intro_call)
    );
    let rect = files.pos("intro.tsx", 3, 5);
    let circle = files.pos("intro.tsx", 4, 5);
    let text = files.pos("main.tsx", 3, 5);
    let main_call = files.pos("main.tsx", 2, 3);
    let element = |at: &SourcePos, call: &SourcePos, ranges| ElementOccurrences {
        element: at.clone(),
        instances: vec![instance_of(
            vec![at.clone(), call.clone()],
            ranges,
            vec![source.clone()],
        )],
    };
    let occurrences = OccurrenceState::Ready(OccurrenceIndex {
        elements: vec![
            element(&rect, &intro_call, vec![(30, 90)]),
            element(&circle, &intro_call, vec![(30, 90)]),
            element(&text, &main_call, vec![(120, 210)]),
        ],
    });
    let chapters = [scene("intro", 0, 90), scene("main", 90, 120)];
    // The Rect's own instance's call of `Delayed`, as read.
    let read = SceneCalls {
        element: rect.clone(),
        generation: 0,
        calls: vec![Some((intro_call.clone(), Vec::new()))],
    };

    let (state, calls) = window_scope_state(
        &report,
        &window,
        Some(&occurrences),
        Some(&read),
        &chapters,
        45,
    );

    let ScopeState::Options(options) = &state else {
        panic!("the window is shared: {state:?}");
    };
    assert_eq!(options.this_scene, Ok(()));
    assert_eq!(options.shared_reaches, ["intro", "main"]);
    // The Circle comes through the same call as the Rect; Main's call
    // wasn't read.
    let calls = calls.expect("the calls are mapped onto the window");
    assert_eq!(
        calls.calls,
        [
            Some((intro_call.clone(), Vec::new())),
            Some((intro_call.clone(), Vec::new())),
            None
        ]
    );
    // No choice made: "This scene" applies, never a write as resolved
    // into `Delayed`.
    let keys = [prop("from")];
    let scope = commit_scope(None, &state);
    assert_eq!(scope, Some(Scope::ThisScene));
    assert_eq!(
        gesture_gate(scope, &state, &window.targets, &keys, Some(&calls)),
        CommitGate::Go(Some(Scope::ThisScene))
    );
    files
        .commit(&window, &report, scope, &[number("from", 45.0)])
        .unwrap();
    assert_eq!(
        files.read("intro.tsx"),
        INTRO_DELAYED.replace("<Delayed>", "<Delayed sequenceFrom={45}>")
    );
    assert!(files.read("delayed.tsx").contains("from={sequenceFrom}"));
    assert_eq!(files.read("main.tsx"), MAIN_DELAYED);
}

/// Intro calls `Delayed` at 3:5 around a `<Rect>` at 4:7 and a `<Circle>`
/// at 5:7, and again at 7:5 around a `<Text>` at 8:7.
const INTRO_DELAYED_TWICE: &str = "export const Intro = () => (\n  <>\n    <Delayed>\n      <Rect />\n      <Circle />\n    </Delayed>\n    <Delayed>\n      <Text>Hi</Text>\n    </Delayed>\n  </>\n);\n";

#[test]
fn a_window_a_scene_calls_twice_writes_this_call_under_this_scene() {
    // Intro calls `Delayed` twice: the first call draws the window around
    // two elements, which is no window drawn more than once, so "This
    // scene" stays open and writes the first call only.
    let files = Files::new(
        "delayed-twice",
        &[("delayed.tsx", DELAYED), ("intro.tsx", INTRO_DELAYED_TWICE)],
    );
    let report = report_of(vec![
        files.entry("Rect", ElementKind::Primitive, "intro.tsx", (4, 7)),
        files.entry("Sequence", ElementKind::Wrapper, "delayed.tsx", (4, 10)),
        files.entry("Delayed", ElementKind::Component, "intro.tsx", (3, 5)),
    ]);
    let source = files.pos("delayed.tsx", 4, 10);
    let window = files.window(&report, &source);
    let first = files.pos("intro.tsx", 3, 5);
    let second = files.pos("intro.tsx", 7, 5);
    let element = |at: SourcePos, call: &SourcePos| ElementOccurrences {
        element: at.clone(),
        instances: vec![instance_of(
            vec![at, call.clone()],
            vec![(30, 90)],
            vec![source.clone()],
        )],
    };
    let occurrences = OccurrenceState::Ready(OccurrenceIndex {
        elements: vec![
            element(files.pos("intro.tsx", 4, 7), &first),
            element(files.pos("intro.tsx", 5, 7), &first),
            element(files.pos("intro.tsx", 8, 7), &second),
        ],
    });
    let chapters = [scene("intro", 0, 90)];

    let (state, _) = window_scope_state(&report, &window, Some(&occurrences), None, &chapters, 45);

    let ScopeState::Options(options) = &state else {
        panic!("the window is shared: {state:?}");
    };
    assert_eq!(options.this_scene, Ok(()));
    // A window's calls are not counted.
    assert_eq!(options.called_here, None);
    let scope = commit_scope(None, &state);
    assert_eq!(scope, Some(Scope::ThisScene));
    files
        .commit(&window, &report, scope, &[number("from", 45.0)])
        .unwrap();
    assert_eq!(
        files.read("intro.tsx"),
        INTRO_DELAYED_TWICE.replacen("<Delayed>", "<Delayed sequenceFrom={45}>", 1)
    );
    assert!(files.read("delayed.tsx").contains("from={sequenceFrom}"));
}

/// Intro writes `<Sequence from={30}>` at 5:5 around its `<Tag>` call at
/// 6:7.
const INTRO_TAGGED: &str = "import { Sequence } from \"@celesta/react\";\n\nexport function Intro() {\n  return (\n    <Sequence from={30}>\n      <Tag />\n    </Sequence>\n  );\n}\n";

/// The film calls Intro at 3:5 and Main at 4:5.
const FILM: &str = "export const Film = () => (\n  <>\n    <Intro />\n    <Main />\n  </>\n);\n";

#[test]
fn a_window_written_once_is_not_shared_by_its_elements_instances() {
    // `Tag`'s `<Rect>` is drawn in Intro and Main, but only Intro's goes
    // through Intro's `<Sequence>`: the window is written as resolved.
    let files = Files::new("tagged", &[("intro.tsx", INTRO_TAGGED), ("film.tsx", FILM)]);
    let tag_rect = SourcePos {
        file: PathBuf::from("/project/tag.tsx"),
        line: 2,
        column: 10,
    };
    let intro_tag = files.pos("intro.tsx", 6, 7);
    let intro_call = files.pos("film.tsx", 3, 5);
    let report = report_of(vec![
        LayerSource {
            column: 10,
            ..entry_at("Rect", ElementKind::Primitive, "/project/tag.tsx", 2)
        },
        files.entry("Tag", ElementKind::Component, "intro.tsx", (6, 7)),
        files.entry("Sequence", ElementKind::Wrapper, "intro.tsx", (5, 5)),
        files.entry("Intro", ElementKind::Component, "film.tsx", (3, 5)),
    ]);
    let source = files.pos("intro.tsx", 5, 5);
    let window = files.window(&report, &source);
    let main_tag = SourcePos {
        file: PathBuf::from("/project/main.tsx"),
        line: 3,
        column: 5,
    };
    let occurrences = OccurrenceState::Ready(OccurrenceIndex {
        elements: vec![ElementOccurrences {
            element: tag_rect.clone(),
            instances: vec![
                instance_of(
                    vec![tag_rect.clone(), intro_tag, intro_call],
                    vec![(30, 90)],
                    vec![source.clone()],
                ),
                instance_of(
                    vec![tag_rect.clone(), main_tag, files.pos("film.tsx", 4, 5)],
                    vec![(90, 210)],
                    Vec::new(),
                ),
            ],
        }],
    });
    let chapters = [scene("intro", 0, 90), scene("main", 90, 120)];

    let (state, _) = window_scope_state(&report, &window, Some(&occurrences), None, &chapters, 45);

    assert_eq!(state, ScopeState::NotShared);
    let scope = commit_scope(Some(Scope::ThisScene), &state);
    assert_eq!(
        gesture_gate(scope, &state, &window.targets, &[prop("from")], None),
        CommitGate::Go(None)
    );
    files
        .commit(&window, &report, scope, &[number("from", 45.0)])
        .unwrap();
    assert_eq!(
        files.read("intro.tsx"),
        INTRO_TAGGED.replace("from={30}", "from={45}")
    );
    assert_eq!(files.read("film.tsx"), FILM);
}

#[test]
fn a_window_the_index_cannot_answer_for_is_unknown() {
    let files = Files::new("window-unknown", &[("scene.tsx", SEQUENCE)]);
    let report = sequence_report(&files);
    let mut window = files.window(&report, &files.pos("scene.tsx", 4, 3));
    // Written in a component, so the index decides.
    window.targets.enclosing = Some("Intro".to_owned());
    let state = |occurrences: Option<&OccurrenceState>| {
        window_scope_state(&report, &window, occurrences, None, &[], 0).0
    };
    let note = |state: ScopeState| match state {
        ScopeState::Unknown { note } => note,
        other => panic!("not unknown: {other:?}"),
    };
    assert_eq!(note(state(None)), "The scenes are not scanned");
    assert_eq!(
        note(state(Some(&OccurrenceState::Scanning))),
        "Scanning scenes…"
    );
    // No instance renders through the window.
    let empty = OccurrenceIndex::default();
    assert_eq!(
        note(state(Some(&OccurrenceState::Ready(empty.clone())))),
        "Not found in the scene scan"
    );
    assert_eq!(
        note(state(Some(&OccurrenceState::Refreshing(empty)))),
        "Scanning scenes…"
    );
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
        Some(elsewhere),
        None,
        None
    ));
    let (window, _) = state
        .window_targets(Some("a"), Some((&report, 0)))
        .expect("the window's outcome is kept beside the element's");
    assert_eq!(window.err(), Some(why));
}

/// Gives the `<Rect>` at `at` in `file` the timing `from={45}
/// durationInFrames={75}` as the editor does: one write of the element's
/// file, found again where `expected` was.
fn set_timing(
    files: &Files,
    file: &str,
    at: (u32, u32),
    expected: &ElementRef,
) -> Result<Vec<crate::edit_history::FileEdit>, String> {
    let report = report_of(vec![files.entry("Rect", ElementKind::Primitive, file, at)]);
    write_all_recorded(vec![files.path(file)], |texts| {
        set_timing_files(texts, &report, expected, 45, 75).map_err(|error| error.to_string())
    })
}

#[test]
fn set_timing_files_wraps_and_imports_in_one_entry() {
    let files = Files::new("set-timing", &[("scene.tsx", UNTIMED)]);
    let expected = element_at(UNTIMED, 5, 5).unwrap();

    let entry = set_timing(&files, "scene.tsx", (5, 5), &expected).unwrap();

    assert_eq!(files.read("scene.tsx"), TIMED);
    assert_eq!(entry.len(), 1);
    let mut history = EditHistory::default();
    history.push(HistoryEntry { files: entry });
    history.undo().unwrap();
    assert_eq!(files.read("scene.tsx"), UNTIMED);
}

#[test]
fn set_timing_files_uses_the_alias_and_keeps_tabs() {
    let files = Files::new("set-timing-tabs", &[("scene.tsx", TABS)]);
    let expected = element_at(TABS, 5, 3).unwrap();

    set_timing(&files, "scene.tsx", (5, 3), &expected).unwrap();

    assert_eq!(
        files.read("scene.tsx"),
        "import { Rect, Sequence as Seq } from \"@celesta/react\";\n\nexport const Intro = () => (\n\t<>\n\t\t<Seq from={45} durationInFrames={75}>\n\t\t\t<Rect\n\t\t\t\twidth={200}\n\t\t\t/>\n\t\t</Seq>\n\t</>\n);\n"
    );
}

#[test]
fn set_timing_files_writes_through_a_namespace_import() {
    let text = "import * as C from \"@celesta/react\";\n\nexport const Intro = () => (\n  <>\n    <C.Rect width={200} />\n  </>\n);\n";
    let files = Files::new("set-timing-namespace", &[("scene.tsx", text)]);
    let expected = element_at(text, 5, 5).unwrap();

    set_timing(&files, "scene.tsx", (5, 5), &expected).unwrap();

    assert_eq!(
        files.read("scene.tsx"),
        "import * as C from \"@celesta/react\";\n\nexport const Intro = () => (\n  <>\n    <C.Sequence from={45} durationInFrames={75}>\n      <C.Rect width={200} />\n    </C.Sequence>\n  </>\n);\n"
    );
}

#[test]
fn set_timing_files_refuses_a_changed_source() {
    let files = Files::new("set-timing-changed", &[("scene.tsx", UNTIMED)]);
    let expected = element_at(UNTIMED, 5, 5).unwrap();
    let moved = UNTIMED.replace("\n\nexport const", "\n\n// The intro.\nexport const");
    fs::write(files.path("scene.tsx"), &moved).unwrap();

    let result = set_timing(&files, "scene.tsx", (5, 5), &expected);

    assert_eq!(result.err(), Some(CommitError::SourceChanged.to_string()));
    assert_eq!(files.read("scene.tsx"), moved);
}

fn timing_window(from: f64, to: f64) -> TimingWindow {
    TimingWindow {
        kind: "Sequence".to_owned(),
        from,
        to,
        source: None,
        remapped: false,
    }
}

#[test]
fn set_timing_offer_follows_the_role() {
    let window = timing_window(300.0, 420.0);
    let span = Some((45, 75));
    let offer = |role: &WindowRole, span, lock: Option<&WrapLock>| {
        set_timing_offer(role, span, lock, None, &ScopeState::NotShared, None)
    };
    let enabled = SetTimingOffer::Enabled {
        from: 45,
        duration: 75,
    };

    assert_eq!(
        offer(&WindowRole::Own(&window), span, None),
        SetTimingOffer::Hidden
    );
    assert_eq!(
        offer(&WindowRole::Remapped, span, None),
        SetTimingOffer::Hidden
    );
    assert_eq!(offer(&WindowRole::Scene(&window), span, None), enabled);
    assert_eq!(offer(&WindowRole::None, span, None), enabled);

    assert_eq!(
        offer(
            &WindowRole::Scene(&window),
            span,
            Some(&WrapLock::NotAChild)
        ),
        SetTimingOffer::Disabled("Wrap the element in a parent first".to_owned())
    );
    let no_frame_left = SetTimingOffer::Disabled("No frame left after the playhead".to_owned());
    assert_eq!(
        offer(&WindowRole::Scene(&window), None, None),
        no_frame_left
    );
    assert_eq!(offer(&WindowRole::None, None, None), no_frame_left);

    // `Sequence` bound otherwise in the file (a type-only import, a local
    // declaration): the import can't be written.
    let clash = ImportLock::Clash {
        name: "Sequence".to_owned(),
    };
    assert_eq!(
        set_timing_offer(
            &WindowRole::Scene(&window),
            span,
            None,
            Some(&clash),
            &ScopeState::NotShared,
            None
        ),
        SetTimingOffer::Disabled("`Sequence` here is not Celesta's".to_owned())
    );
}

#[test]
fn a_grip_the_kept_scope_would_refuse_is_locked_before_the_press() {
    // While the scenes are scanned, a window's literal props wait for the
    // scan: no grip starts, with the scan's note.
    let files = Files::new("refusal-scanning", &[("scene.tsx", SEQUENCE)]);
    let report = sequence_report(&files);
    let window = files.window(&report, &files.pos("scene.tsx", 4, 3));
    let scanning = ScopeState::Unknown {
        note: "Scanning scenes…".to_owned(),
    };
    for grip in [BandGrip::Start, BandGrip::End, BandGrip::Body] {
        let refusal = |state: &ScopeState| {
            grip_refusal(window.kind, grip, &window.targets, state, None, None)
        };
        assert_eq!(refusal(&scanning), Some("Scanning scenes…".to_owned()));
        assert_eq!(refusal(&ScopeState::NotShared), None);
    }

    // Under "Shared value", this scene's call passes `sequenceFrom` itself:
    // a drag of `from` would not show here, so the grips writing it are
    // locked; the end edge writes `durationInFrames` only.
    let price =
        "export const Price = () => (\n  <PriceTag label=\"Sale\" sequenceFrom={45} />\n);\n";
    let files = Files::new(
        "refusal-sets-own",
        &[("price_tag.tsx", TAG_LIFTED), ("price.tsx", price)],
    );
    let report = report_of(vec![
        files.entry("Text", ElementKind::Primitive, "price_tag.tsx", (11, 7)),
        files.entry("Sequence", ElementKind::Wrapper, "price_tag.tsx", (10, 5)),
        files.entry("PriceTag", ElementKind::Component, "price.tsx", (2, 3)),
    ]);
    let window = files.window(&report, &files.pos("price_tag.tsx", 10, 5));
    let open = ScopeState::Options(ScopeOptions {
        this_scene: Ok(()),
        shared: Ok(()),
        shared_reaches: vec!["Intro".to_owned()],
        sets_own: vec!["Price".to_owned()],
        called_here: None,
        drawn_alone: true,
    });
    let refusal = |grip, remembered| {
        grip_refusal(window.kind, grip, &window.targets, &open, remembered, None)
    };
    let sets_own = ScopeLock::SetsOwn {
        param: "sequenceFrom".to_owned(),
    }
    .text();
    assert_eq!(
        refusal(BandGrip::Start, Some(Scope::Shared)),
        Some(sets_own.clone())
    );
    assert_eq!(refusal(BandGrip::Body, Some(Scope::Shared)), Some(sets_own));
    assert_eq!(refusal(BandGrip::End, Some(Scope::Shared)), None);
    // Under "This scene" the call's value is written there.
    assert_eq!(refusal(BandGrip::Body, Some(Scope::ThisScene)), None);
}

#[test]
fn set_timing_offer_follows_the_scope() {
    let role = WindowRole::None;
    let span = Some((45, 75));
    let offer = |scope: &ScopeState, remembered| {
        set_timing_offer(&role, span, None, None, scope, remembered)
    };
    let enabled = SetTimingOffer::Enabled {
        from: 45,
        duration: 75,
    };
    let options = ScopeState::Options(ScopeOptions {
        this_scene: Ok(()),
        shared: Ok(()),
        shared_reaches: vec!["Intro".to_owned(), "Price".to_owned()],
        sets_own: Vec::new(),
        called_here: None,
        drawn_alone: false,
    });

    // A choice remembered for the element counts only where it is shared.
    assert_eq!(offer(&ScopeState::NotShared, None), enabled);
    assert_eq!(
        offer(&ScopeState::NotShared, Some(Scope::ThisScene)),
        enabled
    );
    assert_eq!(offer(&options, Some(Scope::Shared)), enabled);
    let per_scene = SetTimingOffer::Disabled("Can't be set per scene yet".to_owned());
    assert_eq!(offer(&options, Some(Scope::ThisScene)), per_scene);
    // "This scene" is the option in effect without a choice.
    assert_eq!(offer(&options, None), per_scene);

    let scanning = ScopeState::Unknown {
        note: "Scanning scenes…".to_owned(),
    };
    assert_eq!(
        offer(&scanning, None),
        SetTimingOffer::Disabled("Scanning scenes…".to_owned())
    );
}
