//! Tests of an element's timing window as an element of its own: the
//! wrapper's report, its timing props as rows, and the commit that writes
//! them (as resolved or under an edit scope) as one history entry.

use super::*;
use crate::element_edit::{WindowTargets, resolve_window, window_files, window_report};
use crate::scope::{Scope, ScopeLock, ScopeOptions, ScopeState};
use crate::timeline_drag::{BandGrip, grip_refusal};
use crate::timing::{SetTimingOffer, WindowRole, set_timing_files, set_timing_offer};
use celesta_react_bridge::TimingWindow;
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
