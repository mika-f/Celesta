//! Tests of the edit scope rules (`scope`): whether an element is shared,
//! which of "This scene" and "Shared value" it allows, and what each row
//! writes under them.

use super::*;
use crate::element_edit::{Resolved, resolved_scope_state};
use crate::element_rows::default_row_text;
use crate::impact::{ImpactFilter, ImpactView, impact_line, impact_view};
use crate::occurrences::OccurrenceState;
use crate::scope::{
    CHANGED_MEANWHILE, CommitGate, HeldOrigin, SCANNED_MEANWHILE, SceneCalls, Scope, ScopeChoices,
    ScopeLock, ScopeOptions, ScopeState, ScopedRow, calls_of, chosen_scope, commit_gate,
    commit_scope, drain_gate, gesture_gate, gesture_lock, gesture_start_refusal, held_gate,
    impact_filter, previewed_scope, remove_tooltip, scope_in_effect, scope_line, scope_state,
    scoped_removable, scoped_row,
};
use celesta_react_bridge::{
    ElementOccurrences, InstanceOccurrences, OccurrenceIndex, ReactChapter,
};
use celesta_source_edit::{DefaultTarget, LiftLock};

const TAG_FILE: &str = "/project/price_tag.tsx";
const SCENES_FILE: &str = "/project/scenes.tsx";

/// `PriceTag` writes `<Rect>` at 2:10 with a literal `x`, an absent `y`,
/// and the rest from parameters: `rectWidth` with a literal default,
/// `rectHeight` without one, `rectCornerRadius` with one, `rectOpacity`
/// with an expression.
const TAG: &str = "export function PriceTag({ label, rectWidth = 200, rectHeight, rectCornerRadius = 8, rectOpacity = base * 2 }: { label: string; rectWidth?: number; rectHeight: number; rectCornerRadius?: number; rectOpacity?: number }) {\n  return <Rect x={10} width={rectWidth} height={rectHeight} cornerRadius={rectCornerRadius} opacity={rectOpacity} />;\n}\n";

/// The Price scene's call of `PriceTag` at 2:3: it sets `rectWidth` and
/// `rectHeight` and leaves the others out.
const CALL: &str = "export const Price = () => (\n  <PriceTag label=\"Sale\" rectWidth={260} rectHeight={40} />\n);\n";

fn pos(file: &str, line: u32, column: u32) -> SourcePos {
    SourcePos {
        file: PathBuf::from(file),
        line,
        column,
    }
}

/// The `<Rect>` in `PriceTag`.
fn e() -> SourcePos {
    pos(TAG_FILE, 2, 10)
}

/// The Price scene's call of `PriceTag`.
fn s() -> SourcePos {
    pos(SCENES_FILE, 2, 3)
}

fn intro_call() -> SourcePos {
    pos("/project/intro.tsx", 2, 3)
}

fn outro_call() -> SourcePos {
    pos("/project/outro.tsx", 2, 3)
}

fn chapter(id: &str, name: &str, from: u64, duration_in_frames: u64) -> ReactChapter {
    ReactChapter {
        id: id.to_owned(),
        name: name.to_owned(),
        from,
        duration_in_frames,
    }
}

/// Intro, Price and Outro, 30 frames each; the playhead is at 40, in Price.
fn chapters() -> Vec<ReactChapter> {
    vec![
        chapter("intro", "Intro", 0, 30),
        chapter("price", "Price", 30, 30),
        chapter("outro", "Outro", 60, 30),
    ]
}

const FRAME: u64 = 40;

fn instance(chain: Vec<SourcePos>, ranges: Vec<(u64, u64)>) -> InstanceOccurrences {
    InstanceOccurrences {
        chain,
        ranges,
        max_per_frame: 1,
    }
}

fn element_of(element: SourcePos, instances: Vec<InstanceOccurrences>) -> ElementOccurrences {
    ElementOccurrences { element, instances }
}

/// An index holding `PriceTag`'s `<Rect>` with `instances`.
fn index(instances: Vec<InstanceOccurrences>) -> OccurrenceState {
    OccurrenceState::Ready(OccurrenceIndex {
        elements: vec![element_of(e(), instances)],
    })
}

/// The `<Rect>` in `PriceTag` under `outer` (innermost first), as it
/// received the values [`TAG`] and [`CALL`] give it.
fn tag_report(outer: Vec<LayerSource>) -> LayerSourceReport {
    let rect = LayerSource {
        column: 10,
        ..entry_at("Rect", ElementKind::Primitive, TAG_FILE, 2)
    };
    let mut chain = vec![rect];
    chain.extend(outer);
    let mut report = report_of(chain);
    report.props.x = Some(10.0);
    for (name, value) in [("width", 260.0), ("height", 40.0), ("cornerRadius", 8.0)] {
        report
            .props
            .others
            .insert(name.to_owned(), ReceivedValue::Number(value));
    }
    report
}

/// Called by the Price scene at [`s`].
fn price_report() -> LayerSourceReport {
    tag_report(vec![entry_at(
        "PriceTag",
        ElementKind::Component,
        SCENES_FILE,
        2,
    )])
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
        (
            "Size",
            vec![
                ("width", number_field(None)),
                ("height", number_field(None)),
                ("cornerRadius", number_field(Some(0.0))),
                ("opacity", number_field(Some(1.0))),
            ],
        ),
    ])
}

/// [`TAG`], with [`CALL`] in `call_file`.
fn tag_texts(call_file: &str) -> Texts {
    Texts::from([
        (PathBuf::from(TAG_FILE), TAG.to_owned()),
        (PathBuf::from(call_file), CALL.to_owned()),
    ])
}

fn tag_targets(report: &LayerSourceReport, texts: &Texts) -> ElementTargets {
    resolve_targets(read_from(texts), report, Some(&tag_table())).expect("the element resolves")
}

/// The Price scene's call sets `rectWidth` and `rectHeight`; Intro's and
/// Outro's set only `label`. Instances as [`three_scenes`] orders them.
fn price_sets_its_own() -> SceneCalls {
    let attributes = |names: &[&str]| names.iter().map(|name| (*name).to_owned()).collect();
    SceneCalls {
        element: e(),
        generation: 1,
        calls: vec![
            Some((s(), attributes(&["label", "rectWidth", "rectHeight"]))),
            Some((intro_call(), attributes(&["label"]))),
            Some((outro_call(), attributes(&["label"]))),
        ],
    }
}

/// `PriceTag` called once per scene: by Price at [`s`], Intro and Outro
/// elsewhere.
fn three_scenes() -> OccurrenceState {
    index(vec![
        instance(vec![e(), s()], vec![(30, 60)]),
        instance(vec![e(), intro_call()], vec![(0, 30)]),
        instance(vec![e(), outro_call()], vec![(60, 90)]),
    ])
}

fn options_of(state: ScopeState) -> ScopeOptions {
    match state {
        ScopeState::Options(options) => options,
        other => panic!("expected scope options, got {other:?}"),
    }
}

fn options(this_scene: Result<(), ScopeLock>, shared: Result<(), ScopeLock>) -> ScopeOptions {
    ScopeOptions {
        this_scene,
        shared,
        shared_reaches: vec!["Intro".to_owned(), "Outro".to_owned()],
        sets_own: Vec::new(),
    }
}

fn price_tag(component: fn(String) -> ScopeLock) -> ScopeLock {
    component("PriceTag".to_owned())
}

#[test]
fn an_element_with_one_instance_is_not_shared() {
    let report = price_report();
    let targets = tag_targets(&report, &tag_texts(SCENES_FILE));
    assert_eq!(targets.enclosing.as_deref(), Some("PriceTag"));
    let occurrences = index(vec![instance(vec![e(), s()], vec![(30, 60)])]);

    let state = scope_state(
        &report,
        &targets,
        Some(&occurrences),
        None,
        &chapters(),
        FRAME,
    );

    assert_eq!(state, ScopeState::NotShared);
    let view = impact_view(
        &report,
        Some(&occurrences),
        &chapters(),
        FRAME,
        &impact_filter(None, &targets, None),
    );
    assert_eq!(
        impact_line(&view),
        ("Changes this scene only".to_owned(), false)
    );
}

#[test]
fn an_element_outside_a_component_is_not_shared() {
    // A `defineScenes` scene function is no component a call names.
    let text = "export const scenes = defineScenes({\n  intro: () => <Rect x={10} />,\n});\n";
    let mut report = report_of(vec![LayerSource {
        column: 16,
        ..entry_at("Rect", ElementKind::Primitive, SCENES_FILE, 2)
    }]);
    report.props.x = Some(10.0);
    let targets = resolve_targets(|_| Ok(text.to_owned()), &report, None).unwrap();
    assert_eq!(targets.enclosing, None);
    assert!(targets.defaults.is_empty());
    assert_eq!(targets.lift_lock, None);
    let element = pos(SCENES_FILE, 2, 16);
    let occurrences = OccurrenceState::Ready(OccurrenceIndex {
        elements: vec![element_of(
            element.clone(),
            vec![
                instance(vec![element.clone()], vec![(0, 30)]),
                instance(vec![element, intro_call()], vec![(60, 90)]),
            ],
        )],
    });

    let state = scope_state(&report, &targets, Some(&occurrences), None, &chapters(), 10);

    assert_eq!(state, ScopeState::NotShared);
    let view = impact_view(
        &report,
        Some(&occurrences),
        &chapters(),
        10,
        &ImpactFilter::All,
    );
    assert_eq!(impact_line(&view), ("Also changes Outro".to_owned(), true));
}

#[test]
fn an_unanswered_index_is_unknown() {
    let report = price_report();
    let targets = tag_targets(&report, &tag_texts(SCENES_FILE));
    let state = |occurrences: Option<&OccurrenceState>| {
        scope_state(&report, &targets, occurrences, None, &chapters(), FRAME)
    };
    let unknown = |note: &str| ScopeState::Unknown {
        note: note.to_owned(),
    };

    // No sweep runs.
    assert_eq!(state(None), unknown("The scenes are not scanned"));
    assert_eq!(
        state(Some(&OccurrenceState::Scanning)),
        unknown("Scanning scenes…")
    );
    assert_eq!(
        state(Some(&OccurrenceState::Failed("x".to_owned()))),
        unknown("Couldn't scan scenes: x")
    );
    // A lift moved the element: the last index no longer finds it.
    let moved = OccurrenceState::Refreshing(OccurrenceIndex {
        elements: vec![element_of(
            pos(TAG_FILE, 9, 10),
            vec![
                instance(vec![pos(TAG_FILE, 9, 10), s()], vec![(30, 60)]),
                instance(vec![pos(TAG_FILE, 9, 10), intro_call()], vec![(0, 30)]),
            ],
        )],
    });
    assert_eq!(state(Some(&moved)), unknown("Scanning scenes…"));
}

#[test]
fn this_scene_is_confined_when_every_instance_through_s_is_in_this_scene() {
    let report = price_report();
    let targets = tag_targets(&report, &tag_texts(SCENES_FILE));

    let options = options_of(scope_state(
        &report,
        &targets,
        Some(&three_scenes()),
        None,
        &chapters(),
        FRAME,
    ));

    assert_eq!(options.this_scene, Ok(()));
    assert_eq!(options.shared, Ok(()));
    // Without the scene calls read, no scene is known to set its own.
    assert_eq!(options.shared_reaches, ["Intro", "Price", "Outro"]);
    assert!(options.sets_own.is_empty());
}

#[test]
fn no_call_locks_this_scene() {
    // The chain has no `PriceTag` (nor an anonymous component) to call it.
    let report = tag_report(vec![entry_at(
        "Card",
        ElementKind::Component,
        SCENES_FILE,
        2,
    )]);
    let targets = tag_targets(&report, &tag_texts(SCENES_FILE));
    assert_eq!(targets.enclosing.as_deref(), Some("PriceTag"));
    assert_eq!(targets.component, None);

    let options = options_of(scope_state(
        &report,
        &targets,
        Some(&three_scenes()),
        None,
        &chapters(),
        FRAME,
    ));

    let lock = price_tag(|component| ScopeLock::NoCall { component });
    assert_eq!(options.this_scene, Err(lock.clone()));
    assert_eq!(lock.text(), "Can't find where `<PriceTag>` is called");
}

#[test]
fn a_component_after_s_locks_as_passes_through() {
    // `Card` calls `PriceTag`, and the scene calls `Card`.
    let card_file = "/project/card.tsx";
    let report = tag_report(vec![
        entry_at("PriceTag", ElementKind::Component, card_file, 2),
        entry_at("Card", ElementKind::Component, SCENES_FILE, 2),
    ]);
    let targets = tag_targets(&report, &tag_texts(card_file));
    let in_card = pos(card_file, 2, 3);
    let occurrences = index(vec![
        instance(vec![e(), in_card.clone(), s()], vec![(30, 60)]),
        instance(vec![e(), in_card, intro_call()], vec![(0, 30)]),
    ]);

    let options = options_of(scope_state(
        &report,
        &targets,
        Some(&occurrences),
        None,
        &chapters(),
        FRAME,
    ));

    let lock = ScopeLock::PassesThrough {
        component: "Card".to_owned(),
    };
    assert_eq!(options.this_scene, Err(lock.clone()));
    assert_eq!(lock.text(), "Passes through `<Card>`");
}

#[test]
fn one_chain_spanning_scenes_locks_as_reused() {
    // Intro and Price are two `defineScenes` entries showing the same
    // `PriceTag` call through one `<Scenes>`.
    let report = tag_report(vec![
        entry_at("PriceTag", ElementKind::Component, SCENES_FILE, 2),
        entry_at("Scenes", ElementKind::Wrapper, "/project/video.tsx", 5),
    ]);
    let targets = tag_targets(&report, &tag_texts(SCENES_FILE));
    let occurrences = index(vec![
        instance(vec![e(), s()], vec![(0, 60)]),
        instance(vec![e(), outro_call()], vec![(60, 90)]),
    ]);

    let options = options_of(scope_state(
        &report,
        &targets,
        Some(&occurrences),
        None,
        &chapters(),
        FRAME,
    ));

    assert_eq!(options.this_scene, Err(ScopeLock::ReusedCall));
    assert_eq!(chosen_scope(None, &options), Ok(Scope::Shared));
    assert_eq!(
        ScopeLock::ReusedCall.text(),
        "The scenes reuse the same call (`.map()` or `defineScenes`)"
    );
}

#[test]
fn s_reaching_another_scene_locks_with_its_component() {
    let report = price_report();
    let targets = tag_targets(&report, &tag_texts(SCENES_FILE));
    let occurrences = index(vec![
        instance(vec![e(), s()], vec![(30, 90)]),
        instance(vec![e(), intro_call()], vec![(0, 30)]),
    ]);

    let options = options_of(scope_state(
        &report,
        &targets,
        Some(&occurrences),
        None,
        &chapters(),
        FRAME,
    ));

    let lock = price_tag(|component| ScopeLock::OtherScenes { component });
    assert_eq!(options.this_scene, Err(lock.clone()));
    assert_eq!(
        lock.text(),
        "`<PriceTag>` is called here for other scenes too"
    );
}

#[test]
fn two_calls_in_this_scene_lock() {
    let report = price_report();
    let targets = tag_targets(&report, &tag_texts(SCENES_FILE));
    let occurrences = index(vec![
        instance(vec![e(), s()], vec![(30, 60)]),
        instance(vec![e(), pos(SCENES_FILE, 3, 3)], vec![(40, 50)]),
        instance(vec![e(), intro_call()], vec![(0, 30)]),
    ]);

    let options = options_of(scope_state(
        &report,
        &targets,
        Some(&occurrences),
        None,
        &chapters(),
        FRAME,
    ));

    let lock = price_tag(|component| ScopeLock::CalledTwiceHere { component });
    assert_eq!(options.this_scene, Err(lock.clone()));
    assert_eq!(
        lock.text(),
        "`<PriceTag>` is called more than once in this scene"
    );
}

#[test]
fn text_locks_disable_this_scene_when_no_row_can_be_written() {
    let tag = "import type { Props } from \"./types\";\nexport function PriceTag({ label }: Props) {\n  return <Rect x={10} />;\n}\n";
    let texts = Texts::from([
        (PathBuf::from(TAG_FILE), tag.to_owned()),
        (PathBuf::from(SCENES_FILE), CALL.to_owned()),
    ]);
    let rect = LayerSource {
        column: 10,
        ..entry_at("Rect", ElementKind::Primitive, TAG_FILE, 3)
    };
    let mut report = report_of(vec![
        rect,
        entry_at("PriceTag", ElementKind::Component, SCENES_FILE, 2),
    ]);
    report.props.x = Some(10.0);
    let targets = resolve_targets(read_from(&texts), &report, None).unwrap();
    let lock = price_tag(|component| ScopeLock::ImportedPropsType { component });
    assert_eq!(
        targets.lift_lock,
        Some(LiftLock::ImportedPropsType {
            component: "PriceTag".to_owned()
        })
    );
    let element = pos(TAG_FILE, 3, 10);
    let occurrences = OccurrenceState::Ready(OccurrenceIndex {
        elements: vec![element_of(
            element.clone(),
            vec![
                instance(vec![element.clone(), s()], vec![(30, 60)]),
                instance(vec![element, intro_call()], vec![(0, 30)]),
            ],
        )],
    });

    let options = options_of(scope_state(
        &report,
        &targets,
        Some(&occurrences),
        None,
        &chapters(),
        FRAME,
    ));

    assert_eq!(options.this_scene, Err(lock.clone()));
    assert_eq!(options.shared, Ok(()));
    assert_eq!(chosen_scope(None, &options), Ok(Scope::Shared));
    assert_eq!(scope_line(&options, Some(Scope::Shared)), lock.text());
}

#[test]
fn the_default_is_this_scene_and_falls_back_to_shared_when_locked() {
    let open = options(Ok(()), Ok(()));
    assert_eq!(chosen_scope(None, &open), Ok(Scope::ThisScene));
    assert_eq!(chosen_scope(Some(Scope::Shared), &open), Ok(Scope::Shared));

    let reused = options(Err(ScopeLock::ReusedCall), Ok(()));
    assert_eq!(chosen_scope(None, &reused), Ok(Scope::Shared));

    let both = options(
        Err(ScopeLock::ReusedCall),
        Err(ScopeLock::EverySceneSets {
            param: "rectWidth".to_owned(),
        }),
    );
    assert_eq!(chosen_scope(None, &both), Err(ScopeLock::ReusedCall));
}

#[test]
fn a_remembered_choice_now_locked_is_refused_not_widened() {
    let lock = price_tag(|component| ScopeLock::OtherScenes { component });
    let locked = options(Err(lock.clone()), Ok(()));

    assert_eq!(
        chosen_scope(Some(Scope::ThisScene), &locked),
        Err(lock.clone())
    );
    // The control shows neither option chosen; the line says why.
    assert_eq!(scope_line(&locked, None), lock.text());
}

#[test]
fn the_line_names_reaching_and_own_scenes() {
    let report = price_report();
    let targets = tag_targets(&report, &tag_texts(SCENES_FILE));
    let calls = price_sets_its_own();

    let options = options_of(scope_state(
        &report,
        &targets,
        Some(&three_scenes()),
        Some(&calls),
        &chapters(),
        FRAME,
    ));

    assert_eq!(options.shared_reaches, ["Intro", "Outro"]);
    assert_eq!(options.sets_own, ["Price"]);
    assert_eq!(
        scope_line(&options, Some(Scope::ThisScene)),
        "Shared value is used in Intro, Outro · Price sets its own"
    );
    assert_eq!(
        scope_line(&options, Some(Scope::Shared)),
        "Shared value is used in Intro, Outro · Price sets its own"
    );

    let locked = ScopeOptions {
        this_scene: Err(ScopeLock::ReusedCall),
        ..options
    };
    assert_eq!(
        scope_line(&locked, Some(Scope::Shared)),
        "The scenes reuse the same call (`.map()` or `defineScenes`)"
    );
}

#[test]
fn shared_is_disabled_when_every_row_is_set_by_every_scene() {
    let tag = "export function PriceTag({ rectWidth }: { rectWidth: number }) {\n  return <Rect width={rectWidth} />;\n}\n";
    let call = "export const Price = () => (\n  <PriceTag rectWidth={260} />\n);\n";
    let texts = Texts::from([
        (PathBuf::from(TAG_FILE), tag.to_owned()),
        (PathBuf::from(SCENES_FILE), call.to_owned()),
    ]);
    let rect = LayerSource {
        column: 10,
        ..entry_at("Rect", ElementKind::Primitive, TAG_FILE, 2)
    };
    let mut report = report_of(vec![
        rect,
        entry_at("PriceTag", ElementKind::Component, SCENES_FILE, 2),
    ]);
    report
        .props
        .others
        .insert("width".to_owned(), ReceivedValue::Number(260.0));
    let table = property_table(vec![("Size", vec![("width", number_field(None))])]);
    let targets = resolve_targets(read_from(&texts), &report, Some(&table)).unwrap();
    let every = |param: &str| (s(), vec![param.to_owned()]);
    let calls = SceneCalls {
        element: e(),
        generation: 1,
        calls: vec![
            Some(every("rectWidth")),
            Some((intro_call(), vec!["rectWidth".to_owned()])),
            Some((outro_call(), vec!["rectWidth".to_owned()])),
        ],
    };
    let state = |calls: Option<&SceneCalls>| {
        options_of(scope_state(
            &report,
            &targets,
            Some(&three_scenes()),
            calls,
            &chapters(),
            FRAME,
        ))
    };

    let options = state(Some(&calls));

    let lock = ScopeLock::EverySceneSets {
        param: "rectWidth".to_owned(),
    };
    assert_eq!(options.this_scene, Ok(()));
    assert_eq!(options.shared, Err(lock.clone()));
    assert_eq!(chosen_scope(None, &options), Ok(Scope::ThisScene));
    assert_eq!(lock.text(), "Every scene sets `rectWidth` itself");
    assert_eq!(scope_line(&options, Some(Scope::ThisScene)), lock.text());
    // Until the calls are read, that is not known: only that this scene
    // sets its own.
    assert_eq!(
        state(None).shared,
        Err(ScopeLock::SetsOwn {
            param: "rectWidth".to_owned()
        })
    );
}

#[test]
fn a_remembered_shared_value_this_scene_overrides_refuses_the_removal() {
    // Price passes `rectWidth={260}`, the only row, over a default of 200.
    let tag = "export function PriceTag({ rectWidth = 200 }: { rectWidth?: number }) {\n  return <Rect width={rectWidth} />;\n}\n";
    let call = "export const Price = () => (\n  <PriceTag rectWidth={260} />\n);\n";
    let texts = Texts::from([
        (PathBuf::from(TAG_FILE), tag.to_owned()),
        (PathBuf::from(SCENES_FILE), call.to_owned()),
    ]);
    let rect = LayerSource {
        column: 10,
        ..entry_at("Rect", ElementKind::Primitive, TAG_FILE, 2)
    };
    let mut report = report_of(vec![
        rect,
        entry_at("PriceTag", ElementKind::Component, SCENES_FILE, 2),
    ]);
    report
        .props
        .others
        .insert("width".to_owned(), ReceivedValue::Number(260.0));
    // `<Rect>` requires `width`.
    let table = with_required(
        property_table(vec![("Size", vec![("width", number_field(None))])]),
        &["width"],
    );
    let targets = resolve_targets(read_from(&texts), &report, Some(&table)).unwrap();
    let state = scope_state(
        &report,
        &targets,
        Some(&three_scenes()),
        None,
        &chapters(),
        FRAME,
    );
    let options = options_of(state.clone());
    let lock = ScopeLock::SetsOwn {
        param: "rectWidth".to_owned(),
    };

    // "Shared value" is disabled: every row is one this scene sets itself.
    assert_eq!(options.shared, Err(lock.clone()));
    // Remembered all the same, no scope is in effect: the rows show as
    // resolved, and their remove button with them...
    assert_eq!(
        chosen_scope(Some(Scope::Shared), &options),
        Err(lock.clone())
    );
    let width = prop("width");
    assert!(removable(&targets, &width, targets.entry(&width).unwrap()));
    // ...but a click on it is refused with the lock, as any commit.
    assert_eq!(
        commit_gate(&state, Some(Scope::Shared), &targets, &[width]),
        CommitGate::Refuse(lock.text())
    );
}

#[test]
fn a_required_prop_passed_at_s_is_removable_when_its_parameter_has_a_default() {
    // `<Rect>` requires `width` and `height`; `rectWidth = 200` applies once
    // Price's value goes, `rectHeight` has no default.
    let report = price_report();
    let table = with_required(tag_table(), &["width", "height"]);
    let targets = resolve_targets(read_from(&tag_texts(SCENES_FILE)), &report, Some(&table))
        .expect("the element resolves");
    let calls = price_sets_its_own();
    let (width, height) = (prop("width"), prop("height"));
    assert_eq!(
        scoped_row(Scope::Shared, &targets, &width, Some(&calls)),
        ScopedRow::Locked(ScopeLock::SetsOwn {
            param: "rectWidth".to_owned()
        })
    );

    for scope in [Scope::Shared, Scope::ThisScene] {
        assert!(scoped_removable(scope, &targets, &width, Some(&calls)));
        assert!(!scoped_removable(scope, &targets, &height, Some(&calls)));
    }
    // As resolved too.
    assert!(removable(&targets, &width, targets.entry(&width).unwrap()));
    assert!(!removable(
        &targets,
        &height,
        targets.entry(&height).unwrap()
    ));
}

#[test]
fn scoped_row_follows_the_table() {
    let report = price_report();
    let mut targets = tag_targets(&report, &tag_texts(SCENES_FILE));
    assert_eq!(targets.enclosing.as_deref(), Some("PriceTag"));
    assert_eq!(targets.lift_lock, None);
    let defaults: Vec<(&str, &DefaultTarget)> = targets
        .defaults
        .iter()
        .map(|(param, default)| (param.as_str(), default))
        .collect();
    assert_eq!(
        defaults.iter().map(|(param, _)| *param).collect::<Vec<_>>(),
        ["rectWidth", "rectHeight", "rectCornerRadius", "rectOpacity"]
    );
    assert!(matches!(
        defaults[0].1,
        DefaultTarget::Literal { value: EditValue::Number(value), .. } if *value == 200.0
    ));
    assert!(matches!(defaults[1].1, DefaultTarget::Absent { .. }));
    assert!(matches!(defaults[3].1, DefaultTarget::Expression));

    let calls = price_sets_its_own();
    let row = |targets: &ElementTargets, scope, key: &PropKey, calls: Option<&SceneCalls>| {
        scoped_row(scope, targets, key, calls)
    };
    let both = |targets: &ElementTargets, key: PropKey| {
        (
            row(targets, Scope::ThisScene, &key, Some(&calls)),
            row(targets, Scope::Shared, &key, Some(&calls)),
        )
    };

    // A literal, or an absent prop, at the element.
    assert_eq!(
        both(&targets, prop("x")),
        (ScopedRow::Lift, ScopedRow::AsResolved)
    );
    assert_eq!(
        both(&targets, prop("y")),
        (ScopedRow::Lift, ScopedRow::AsResolved)
    );
    // Passed at S: this scene sets its own value, so the row is locked
    // under "Shared value" (its value at S is removed to edit the default).
    let sets_own = |param: &str| {
        ScopedRow::Locked(ScopeLock::SetsOwn {
            param: param.to_owned(),
        })
    };
    assert_eq!(
        both(&targets, prop("width")),
        (ScopedRow::AsResolved, sets_own("rectWidth"))
    );
    // Left out at S.
    assert_eq!(
        both(&targets, prop("cornerRadius")),
        (
            ScopedRow::AsResolved,
            ScopedRow::Default {
                shown: Some(EditValue::Number(8.0))
            }
        )
    );
    // No default; Intro and Outro leave it out, but S passes it.
    assert_eq!(
        both(&targets, prop("height")),
        (ScopedRow::AsResolved, sets_own("rectHeight"))
    );
    let every = SceneCalls {
        calls: calls
            .calls
            .iter()
            .map(|call| {
                call.clone()
                    .map(|(at, _)| (at, vec!["rectHeight".to_owned()]))
            })
            .collect(),
        ..calls.clone()
    };
    assert_eq!(
        row(&targets, Scope::Shared, &prop("height"), Some(&every)),
        ScopedRow::Locked(ScopeLock::EverySceneSets {
            param: "rectHeight".to_owned()
        })
    );
    // A default written as an expression.
    let expression = ScopeLock::DefaultIsExpression {
        param: "rectOpacity".to_owned(),
    };
    assert_eq!(
        both(&targets, prop("opacity")),
        (ScopedRow::AsResolved, ScopedRow::Locked(expression.clone()))
    );
    assert_eq!(
        expression.text(),
        "`rectOpacity`'s shared value is written as an expression"
    );
    // A Component-section row is outside the scope.
    assert_eq!(
        both(&targets, PropKey::CallSite("label".to_owned())),
        (ScopedRow::AsResolved, ScopedRow::AsResolved)
    );

    // Rows the fixture can't write: a keyframe, a `style` property, an
    // expression reading a literal `const` and a parameter, `props.x`, and
    // a locked one.
    let call = targets.component.as_ref().unwrap().call;
    let at_element = |path: &str, target: EditTarget| Target {
        file: PathBuf::from(TAG_FILE),
        path: path.to_owned(),
        target,
        call_site: None,
        inputs: Vec::new(),
    };
    let literal = || EditTarget::Literal {
        span: Span { start: 1, end: 2 },
        value: EditValue::Number(1.0),
        form: LiteralForm::Number,
    };
    let passed = |param: &str| Target {
        file: PathBuf::from(SCENES_FILE),
        path: param.to_owned(),
        target: EditTarget::PassThrough {
            call_site: call,
            param: param.to_owned(),
            target: Box::new(literal()),
        },
        call_site: Some(s()),
        inputs: Vec::new(),
    };
    let keyframe = EditTarget::Keyframe {
        span: Span { start: 1, end: 2 },
        value: 0.0,
        frame: 0.0,
    };
    let mut fill = at_element(
        "fill",
        EditTarget::Locked(LockReason::Expression {
            source: "w * rectWidth".to_owned(),
        }),
    );
    fill.inputs = vec![
        ("w".to_owned(), at_element("w", literal())),
        ("rectWidth".to_owned(), passed("rectWidth")),
    ];
    targets.targets.extend([
        (prop("rotation"), at_element("rotation", keyframe)),
        (prop("fontSize"), at_element("style.fontSize", literal())),
        (prop("fill"), fill),
        (prop("scale"), passed("rectScale")),
        (
            prop("anchorY"),
            at_element("anchorY", EditTarget::Locked(LockReason::Spread)),
        ),
    ]);
    targets
        .defaults
        .push(("rectScale".to_owned(), DefaultTarget::NoPattern));
    let unsupported = (
        ScopedRow::Locked(ScopeLock::PerSceneUnsupported),
        ScopedRow::AsResolved,
    );
    assert_eq!(both(&targets, prop("rotation")), unsupported);
    assert_eq!(both(&targets, prop("fontSize")), unsupported);
    let input = |name: &str| PropKey::Input {
        row: "fill".to_owned(),
        name: name.to_owned(),
    };
    assert_eq!(both(&targets, input("w")), unsupported);
    assert_eq!(
        both(&targets, input("rectWidth")),
        (ScopedRow::AsResolved, sets_own("rectWidth"))
    );
    assert_eq!(
        ScopeLock::PerSceneUnsupported.text(),
        "Can't be set per scene yet"
    );
    let no_pattern = price_tag(|component| ScopeLock::NotObjectPattern { component });
    assert_eq!(
        both(&targets, prop("scale")),
        (ScopedRow::AsResolved, ScopedRow::Locked(no_pattern.clone()))
    );
    assert_eq!(
        no_pattern.text(),
        "`PriceTag` doesn't take its props as `{ … }`"
    );
    assert_eq!(
        both(&targets, prop("anchorY")),
        (ScopedRow::AsResolved, ScopedRow::AsResolved)
    );
    // A parameter no pattern key holds a default for.
    targets
        .targets
        .push((prop("anchorX"), passed("rectAnchorX")));
    let not_found = ScopeLock::DefaultNotFound {
        param: "rectAnchorX".to_owned(),
    };
    assert_eq!(
        both(&targets, prop("anchorX")),
        (ScopedRow::AsResolved, ScopedRow::Locked(not_found.clone()))
    );
    assert_eq!(
        not_found.text(),
        "Can't find where `rectAnchorX`'s shared value is written"
    );

    // The lift can't extend the props type.
    let imported = price_tag(|component| ScopeLock::ImportedPropsType { component });
    targets.lift_lock = Some(LiftLock::ImportedPropsType {
        component: "PriceTag".to_owned(),
    });
    assert_eq!(
        row(&targets, Scope::ThisScene, &prop("x"), None),
        ScopedRow::Locked(imported.clone())
    );
    assert_eq!(
        imported.text(),
        "`PriceTag`'s props type is in another file"
    );
    targets.lift_lock = Some(LiftLock::PropsTypeNotEditable {
        component: "PriceTag".to_owned(),
        param: "rectWidth".to_owned(),
    });
    let not_editable = row(&targets, Scope::ThisScene, &prop("x"), None);
    assert_eq!(
        not_editable,
        ScopedRow::Locked(ScopeLock::PropsTypeNotEditable {
            component: "PriceTag".to_owned(),
            param: "rectWidth".to_owned(),
        })
    );
    let ScopedRow::Locked(not_editable) = not_editable else {
        unreachable!()
    };
    assert_eq!(
        not_editable.text(),
        "Can't add `rectWidth` to `PriceTag`'s props type"
    );
    targets.lift_lock = Some(LiftLock::NotALiteral);
    assert_eq!(
        row(&targets, Scope::ThisScene, &prop("x"), None),
        ScopedRow::Locked(ScopeLock::PerSceneUnsupported)
    );

    // The row, drag, scrub and inline text say why a row this scene sets
    // its own value for can't take a shared edit.
    assert_eq!(
        ScopeLock::SetsOwn {
            param: "rectWidth".to_owned()
        }
        .text(),
        "This scene sets its own `rectWidth`"
    );
}

#[test]
fn shared_value_locks_a_row_s_passes_whatever_it_passes() {
    // S passes `rectWidth` and `rectOpacity` as expressions.
    let call = "export const Price = () => (\n  <PriceTag label=\"Sale\" rectWidth={size * 2} rectHeight={40} rectOpacity={size / 2} />\n);\n";
    let texts = Texts::from([
        (PathBuf::from(TAG_FILE), TAG.to_owned()),
        (PathBuf::from(SCENES_FILE), call.to_owned()),
    ]);
    let report = price_report();
    let mut targets = tag_targets(&report, &texts);
    assert!(matches!(
        targets.target(&prop("width")),
        Some(EditTarget::PassThrough { target, .. })
            if matches!(**target, EditTarget::Locked(LockReason::Expression { .. }))
    ));

    let both = |targets: &ElementTargets, key: PropKey| {
        (
            scoped_row(Scope::ThisScene, targets, &key, None),
            scoped_row(Scope::Shared, targets, &key, None),
        )
    };
    // An expression at S is this scene's own value too.
    assert_eq!(
        both(&targets, prop("width")),
        (
            ScopedRow::AsResolved,
            ScopedRow::Locked(ScopeLock::SetsOwn {
                param: "rectWidth".to_owned()
            })
        )
    );
    // A default the shared value can't write keeps its own lock.
    let expression = ScopeLock::DefaultIsExpression {
        param: "rectOpacity".to_owned(),
    };
    assert_eq!(
        both(&targets, prop("opacity")),
        (ScopedRow::AsResolved, ScopedRow::Locked(expression.clone()))
    );

    // With only that row, it decides the option.
    targets.targets.retain(|(key, _)| *key == prop("opacity"));
    let options = options_of(scope_state(
        &report,
        &targets,
        Some(&three_scenes()),
        None,
        &chapters(),
        FRAME,
    ));
    assert_eq!(options.shared, Err(expression));
    assert_eq!(options.this_scene, Ok(()));
}

#[test]
fn removal_follows_the_scope() {
    let report = price_report();
    let targets = tag_targets(&report, &tag_texts(SCENES_FILE));
    let removable_under = |scope, key: &str| scoped_removable(scope, &targets, &prop(key), None);

    // This scene: the value passed at S goes ("Use the shared value"); a
    // literal at the element would reach every scene.
    assert!(removable_under(Scope::ThisScene, "width"));
    assert!(!removable_under(Scope::ThisScene, "x"));
    // Shared value: the literal at the element goes as today; the value
    // this scene passes at S goes too; a default is never removed.
    assert!(removable_under(Scope::Shared, "x"));
    assert!(removable_under(Scope::Shared, "width"));
    assert!(!removable_under(Scope::Shared, "cornerRadius"));
}

#[test]
fn a_row_this_scene_sets_itself_is_locked_under_shared_value() {
    // Price passes `rectWidth={260}`; the shared value is 200.
    let report = price_report();
    let targets = tag_targets(&report, &tag_texts(SCENES_FILE));
    let calls = price_sets_its_own();
    let key = prop("width");
    let sets_own = ScopeLock::SetsOwn {
        param: "rectWidth".to_owned(),
    };

    let row = scoped_row(Scope::Shared, &targets, &key, Some(&calls));
    assert_eq!(row, ScopedRow::Locked(sets_own.clone()));
    // It shows this scene's value, what the canvas draws, and where it is
    // set.
    assert_eq!(
        default_row_text(&row, &key, row_field(&targets, &key)),
        None
    );
    let target = targets.entry(&key).unwrap();
    assert_eq!(
        row_text("width", target, row_field(&targets, &key), &report),
        ("260".to_owned(), String::new())
    );
    assert_eq!(target.call_site, Some(s()));
    // Gestures say the same.
    assert_eq!(
        gesture_lock(Some(Scope::Shared), &targets, &key, Some(&calls)),
        Some(sets_own.clone())
    );
    // The way out: removing its value at S.
    assert!(scoped_removable(
        Scope::Shared,
        &targets,
        &key,
        Some(&calls)
    ));
    assert_eq!(
        remove_tooltip(Some(Scope::Shared), &row),
        "Use the shared value"
    );
    // "This scene" is unchanged.
    assert_eq!(
        scoped_row(Scope::ThisScene, &targets, &key, Some(&calls)),
        ScopedRow::AsResolved
    );

    // Removed at S: the row shows the shared value and edits it.
    let call = CALL.replace(" rectWidth={260}", "");
    let texts = Texts::from([
        (PathBuf::from(TAG_FILE), TAG.to_owned()),
        (PathBuf::from(SCENES_FILE), call),
    ]);
    let mut report = price_report();
    report
        .props
        .others
        .insert("width".to_owned(), ReceivedValue::Number(200.0));
    let targets = tag_targets(&report, &texts);
    let calls = SceneCalls {
        calls: vec![
            Some((s(), vec!["label".to_owned(), "rectHeight".to_owned()])),
            Some((intro_call(), vec!["label".to_owned()])),
            Some((outro_call(), vec!["label".to_owned()])),
        ],
        ..price_sets_its_own()
    };
    let row = scoped_row(Scope::Shared, &targets, &key, Some(&calls));
    assert_eq!(
        row,
        ScopedRow::Default {
            shown: Some(EditValue::Number(200.0))
        }
    );
    assert_eq!(
        default_row_text(&row, &key, row_field(&targets, &key)),
        Some(("200".to_owned(), String::new()))
    );
    assert_eq!(
        gesture_lock(Some(Scope::Shared), &targets, &key, Some(&calls)),
        None
    );
    assert!(!scoped_removable(
        Scope::Shared,
        &targets,
        &key,
        Some(&calls)
    ));
    assert_eq!(remove_tooltip(Some(Scope::Shared), &row), "Remove");
}

#[test]
fn a_const_outside_the_component_leaves_the_other_rows_liftable() {
    // `width` reads a module-level `const`, outside `PriceTag`: it is
    // locked as resolved, and `x`, written on the element, still lifts.
    let tag = "const w = 10;\nexport function PriceTag({ label }: { label: string }) {\n  return <Rect x={10} width={w} />;\n}\n";
    let texts = Texts::from([
        (PathBuf::from(TAG_FILE), tag.to_owned()),
        (PathBuf::from(SCENES_FILE), CALL.to_owned()),
    ]);
    let rect = LayerSource {
        column: 10,
        ..entry_at("Rect", ElementKind::Primitive, TAG_FILE, 3)
    };
    let mut report = report_of(vec![
        rect,
        entry_at("PriceTag", ElementKind::Component, SCENES_FILE, 2),
    ]);
    report.props.x = Some(10.0);
    report
        .props
        .others
        .insert("width".to_owned(), ReceivedValue::Number(10.0));
    let table = property_table(vec![
        ("Transform", vec![("x", number_field(Some(0.0)))]),
        ("Size", vec![("width", number_field(None))]),
    ]);
    let targets = resolve_targets(read_from(&texts), &report, Some(&table)).unwrap();

    assert_eq!(targets.lift_lock, None);
    assert_eq!(
        scoped_row(Scope::ThisScene, &targets, &prop("x"), None),
        ScopedRow::Lift
    );
    assert!(matches!(
        targets.target(&prop("width")),
        Some(EditTarget::Locked(LockReason::UnresolvedIdentifier { .. }))
    ));
    for scope in [Scope::ThisScene, Scope::Shared] {
        assert_eq!(
            scoped_row(scope, &targets, &prop("width"), None),
            ScopedRow::AsResolved
        );
    }
}

#[test]
fn a_choice_moves_with_the_element_after_a_kept_reload() {
    let before = pos(TAG_FILE, 2, 10);
    let after = pos(TAG_FILE, 3, 10);
    let mut choices = ScopeChoices::default();
    choices.set(before.clone(), Scope::Shared);

    choices.carry(&before, &after);

    assert_eq!(choices.get(&after), Some(Scope::Shared));
    assert_eq!(choices.get(&before), None);

    // Carrying from a position with no choice changes nothing.
    let elsewhere = pos(TAG_FILE, 7, 3);
    choices.carry(&elsewhere, &after);
    choices.carry(&elsewhere, &before);
    assert_eq!(choices.get(&after), Some(Scope::Shared));
    assert_eq!(choices.get(&before), None);
}

#[test]
fn a_reload_forgets_the_choices_in_the_files_it_changed() {
    let selected = pos(TAG_FILE, 2, 10);
    // A sibling a lift's added lines may move onto another position.
    let sibling = pos(TAG_FILE, 3, 10);
    let unchanged = pos("/project/badge.tsx", 2, 10);
    let mut choices = ScopeChoices::default();
    choices.set(selected.clone(), Scope::Shared);
    choices.set(sibling.clone(), Scope::Shared);
    choices.set(unchanged.clone(), Scope::Shared);

    // An own write of the component's and the scene's files: only the
    // choice carried for the element being edited stays in them.
    choices.forget_in(
        &[PathBuf::from(TAG_FILE), PathBuf::from(SCENES_FILE)],
        Some(&selected),
    );
    assert_eq!(choices.get(&selected), Some(Scope::Shared));
    assert_eq!(choices.get(&sibling), None);
    assert_eq!(choices.get(&unchanged), Some(Scope::Shared));

    // Nothing carried (the selection went): none stays in them.
    choices.forget_in(&[PathBuf::from(TAG_FILE)], None);
    assert_eq!(choices.get(&selected), None);
    assert_eq!(choices.get(&unchanged), Some(Scope::Shared));

    // A change from outside the editor, in files unknown: every one goes.
    choices.clear();
    assert_eq!(choices.get(&unchanged), None);
}

/// The impact of an edit of the selected `<Rect>` (in Price) under
/// `filter`, with `PriceTag` called once per scene.
fn impact_under(filter: &ImpactFilter) -> crate::impact::Impact {
    match impact_view(
        &price_report(),
        Some(&three_scenes()),
        &chapters(),
        FRAME,
        filter,
    ) {
        ImpactView::Known(impact) => impact,
        other => panic!("expected a known impact, got {other:?}"),
    }
}

#[test]
fn impact_filter_follows_the_chosen_scope() {
    let report = price_report();
    let targets = tag_targets(&report, &tag_texts(SCENES_FILE));
    let calls = price_sets_its_own();

    // This scene: the instances `S` makes.
    let this_scene = impact_filter(Some(Scope::ThisScene), &targets, Some(&calls));
    assert_eq!(this_scene, ImpactFilter::Through(s()));
    let impact = impact_under(&this_scene);
    assert_eq!(impact.instance_count, 1);
    assert!(impact.other_scenes.is_empty());

    // Shared value: a value written on the element (`x`) changes every
    // instance, also those whose call sets its own for another parameter.
    let shared = impact_filter(Some(Scope::Shared), &targets, Some(&calls));
    assert_eq!(shared, ImpactFilter::All);
    let impact = impact_under(&shared);
    assert_eq!(impact.instance_count, 3);
    assert_eq!(impact.other_scenes, ["Intro", "Outro"]);

    // No scope: design 2's rule. `x` is written on the element, so an
    // edit reaches every instance...
    assert_eq!(targets.written_at(), None);
    assert_eq!(
        impact_filter(None, &targets, Some(&calls)),
        ImpactFilter::All
    );
    // ...and only those of the call it writes when every editable
    // transform prop passes through to it.
    let tag = "export function PriceTag({ rectX }: { rectX: number }) {\n  return <Rect x={rectX} />;\n}\n";
    let call = "export const Price = () => (\n  <PriceTag rectX={5} />\n);\n";
    let texts = Texts::from([
        (PathBuf::from(TAG_FILE), tag.to_owned()),
        (PathBuf::from(SCENES_FILE), call.to_owned()),
    ]);
    let mut passed = report.clone();
    passed.props.x = Some(5.0);
    let table = property_table(vec![("Transform", vec![("x", number_field(Some(0.0)))])]);
    let through = resolve_targets(read_from(&texts), &passed, Some(&table)).unwrap();
    assert_eq!(through.written_at(), Some(&s()));
    assert_eq!(
        impact_filter(None, &through, None),
        ImpactFilter::Through(s())
    );

    // Shared value with every row passing through: every instance but
    // those whose call sets its own.
    let sets_x = SceneCalls {
        calls: vec![
            Some((s(), vec!["rectX".to_owned()])),
            Some((intro_call(), Vec::new())),
            Some((outro_call(), Vec::new())),
        ],
        ..calls.clone()
    };
    let shared = impact_filter(Some(Scope::Shared), &through, Some(&sets_x));
    assert_eq!(shared, ImpactFilter::Except(vec![s()]));
    let impact = impact_under(&shared);
    assert_eq!(impact.instance_count, 2);
    assert_eq!(impact.other_scenes, ["Intro", "Outro"]);
    // Until the calls are read, no scene is known to set its own.
    assert_eq!(
        impact_filter(Some(Scope::Shared), &through, None),
        ImpactFilter::Except(Vec::new())
    );
    // A call that sets only props no row reads keeps the shared value.
    let label_only = SceneCalls {
        calls: vec![
            Some((s(), vec!["label".to_owned()])),
            Some((intro_call(), vec!["label".to_owned()])),
            Some((outro_call(), vec!["label".to_owned()])),
        ],
        ..calls.clone()
    };
    assert_eq!(
        impact_filter(Some(Scope::Shared), &through, Some(&label_only)),
        ImpactFilter::Except(Vec::new())
    );
}

#[test]
fn hovering_previews_the_other_option() {
    let open = options(Ok(()), Ok(()));
    // Nothing hovered: the chosen scope, "This scene" by default.
    assert_eq!(previewed_scope(None, &open, None), Some(Scope::ThisScene));
    assert_eq!(
        previewed_scope(None, &open, Some(Scope::Shared)),
        Some(Scope::Shared)
    );
    // Hovering an option previews it, whichever is chosen.
    assert_eq!(
        previewed_scope(Some(Scope::Shared), &open, None),
        Some(Scope::Shared)
    );
    assert_eq!(
        previewed_scope(Some(Scope::ThisScene), &open, Some(Scope::Shared)),
        Some(Scope::ThisScene)
    );
    // A locked option previews nothing: the chosen one stays.
    let reused = options(Err(ScopeLock::ReusedCall), Ok(()));
    assert_eq!(
        previewed_scope(Some(Scope::ThisScene), &reused, None),
        Some(Scope::Shared)
    );
    // A remembered choice now locked shows no scope until one is hovered.
    assert_eq!(previewed_scope(None, &reused, Some(Scope::ThisScene)), None);
    assert_eq!(
        previewed_scope(Some(Scope::Shared), &reused, Some(Scope::ThisScene)),
        Some(Scope::Shared)
    );

    // The bands follow the previewed option.
    let report = price_report();
    let targets = tag_targets(&report, &tag_texts(SCENES_FILE));
    let calls = price_sets_its_own();
    let hovered = previewed_scope(Some(Scope::Shared), &open, None);
    let impact = impact_under(&impact_filter(hovered, &targets, Some(&calls)));
    assert_eq!(impact.instance_count, 3);
    assert_eq!(impact.other_scenes, ["Intro", "Outro"]);
    let chosen = previewed_scope(None, &open, None);
    let impact = impact_under(&impact_filter(chosen, &targets, Some(&calls)));
    assert_eq!(impact.instance_count, 1);
    assert_eq!(impact.own_bands, vec![((30, 60), 0)]);
}

#[test]
fn scope_in_effect_names_the_scope() {
    assert_eq!(scope_in_effect(Scope::ThisScene), "Edits change this scene");
    assert_eq!(
        scope_in_effect(Scope::Shared),
        "Edits change the shared value"
    );
}

#[test]
fn the_remove_button_says_use_the_shared_value_at_s() {
    let report = price_report();
    let targets = tag_targets(&report, &tag_texts(SCENES_FILE));
    let tooltip = |scope: Option<Scope>, key: &str| {
        let row = scope.map_or(ScopedRow::AsResolved, |scope| {
            scoped_row(scope, &targets, &prop(key), None)
        });
        remove_tooltip(scope, &row)
    };

    // This scene: removing the value passed at S uses the shared one.
    assert!(scoped_removable(
        Scope::ThisScene,
        &targets,
        &prop("width"),
        None
    ));
    assert_eq!(
        tooltip(Some(Scope::ThisScene), "width"),
        "Use the shared value"
    );
    // Shared value: the literal at the element is removed as today.
    assert!(scoped_removable(Scope::Shared, &targets, &prop("x"), None));
    assert_eq!(tooltip(Some(Scope::Shared), "x"), "Remove");
    // Not shared.
    assert_eq!(tooltip(None, "width"), "Remove");
}

#[test]
fn a_default_row_shows_the_default() {
    // Price passes neither `rectWidth` nor `rectHeight` (a row it passes
    // is locked and shows as resolved, see
    // `a_row_this_scene_sets_itself_is_locked_under_shared_value`).
    let call = CALL.replace(" rectWidth={260} rectHeight={40}", "");
    let texts = Texts::from([
        (PathBuf::from(TAG_FILE), TAG.to_owned()),
        (PathBuf::from(SCENES_FILE), call),
    ]);
    let mut report = price_report();
    report
        .props
        .others
        .insert("width".to_owned(), ReceivedValue::Number(200.0));
    report.props.others.remove("height");
    let targets = tag_targets(&report, &texts);
    let calls = SceneCalls {
        calls: vec![
            Some((s(), vec!["label".to_owned()])),
            Some((intro_call(), vec!["label".to_owned()])),
            Some((outro_call(), vec!["label".to_owned()])),
        ],
        ..price_sets_its_own()
    };
    let shown = |scope: Scope, key: &str| {
        let key = prop(key);
        let row = scoped_row(scope, &targets, &key, Some(&calls));
        default_row_text(&row, &key, row_field(&targets, &key))
    };

    // The shared value is the default, 200.
    assert_eq!(
        shown(Scope::Shared, "width"),
        Some(("200".to_owned(), String::new()))
    );
    // Left out at S: the default it draws with.
    assert_eq!(
        shown(Scope::Shared, "cornerRadius"),
        Some(("8".to_owned(), String::new()))
    );
    // No default written yet: none to show, nor a table default.
    assert_eq!(
        shown(Scope::Shared, "height"),
        Some((String::new(), String::new()))
    );
    // Any other row shows as resolved.
    assert_eq!(shown(Scope::ThisScene, "width"), None);
    assert_eq!(shown(Scope::Shared, "x"), None);
}

#[test]
fn a_commit_keeps_the_scope_shown_when_it_was_made() {
    let report = price_report();
    let targets = tag_targets(&report, &tag_texts(SCENES_FILE));
    let reused = ScopeState::Options(options(Err(ScopeLock::ReusedCall), Ok(())));
    let open = ScopeState::Options(options(Ok(()), Ok(())));

    // Nothing remembered: the option the control showed in effect.
    assert_eq!(commit_scope(None, &reused), Some(Scope::Shared));
    assert_eq!(commit_scope(None, &open), Some(Scope::ThisScene));
    // A remembered choice goes as it is, locked or not.
    assert_eq!(
        commit_scope(Some(Scope::ThisScene), &reused),
        Some(Scope::ThisScene)
    );
    // No control shown: nothing to keep.
    assert_eq!(commit_scope(None, &ScopeState::NotShared), None);
    // Not shared, a choice remembered from before counts for nothing.
    assert_eq!(
        commit_scope(Some(Scope::Shared), &ScopeState::NotShared),
        None
    );
    assert_eq!(
        commit_scope(Some(Scope::ThisScene), &ScopeState::NotShared),
        None
    );
    let scanning = ScopeState::Unknown {
        note: "Scanning scenes…".to_owned(),
    };
    assert_eq!(commit_scope(None, &scanning), None);

    // Shared value was shown; by the time the commit goes, "This scene"
    // is open again: it still writes the shared value.
    let kept = commit_scope(None, &reused);
    assert_eq!(
        commit_gate(&open, kept, &targets, &[prop("x")]),
        CommitGate::Go(Some(Scope::Shared))
    );
    // "This scene" was shown and is locked now: refused, never widened.
    let kept = commit_scope(None, &open);
    assert_eq!(
        commit_gate(&reused, kept, &targets, &[prop("x")]),
        CommitGate::Refuse(ScopeLock::ReusedCall.text())
    );
}

#[test]
fn a_gesture_writes_under_the_scope_it_started_with() {
    let report = price_report();
    let targets = tag_targets(&report, &tag_texts(SCENES_FILE));
    let calls = price_sets_its_own();
    let open = ScopeState::Options(options(Ok(()), Ok(())));
    let reused = ScopeState::Options(options(Err(ScopeLock::ReusedCall), Ok(())));
    let scanning = ScopeState::Unknown {
        note: "Scanning scenes…".to_owned(),
    };
    let gate = |started, state: &ScopeState, key: &str| {
        gesture_gate(started, state, &targets, &[prop(key)], Some(&calls))
    };

    // Under the scope it started with, whatever is chosen by now.
    assert_eq!(
        gate(Some(Scope::Shared), &open, "x"),
        CommitGate::Go(Some(Scope::Shared))
    );
    // That scope locked by the end: refused, never another one.
    assert_eq!(
        gate(Some(Scope::ThisScene), &reused, "x"),
        CommitGate::Refuse(ScopeLock::ReusedCall.text())
    );
    // A row it can't write, or whose shared edit this scene would not
    // show, under that scope.
    assert_eq!(
        gate(Some(Scope::Shared), &open, "width"),
        CommitGate::Refuse(
            ScopeLock::SetsOwn {
                param: "rectWidth".to_owned()
            }
            .text()
        )
    );
    // Started with no scope shown (the scenes being scanned), and the
    // options known by the end: no scope was shown, so none is written.
    assert_eq!(
        gate(None, &open, "x"),
        CommitGate::Refuse(SCANNED_MEANWHILE.to_owned())
    );
    // Never held: a gesture released while the scenes are scanned is
    // refused with the scan's note, as a drag is.
    assert_eq!(
        gate(None, &scanning, "x"),
        CommitGate::Refuse("Scanning scenes…".to_owned())
    );
    assert_eq!(
        gate(None, &ScopeState::NotShared, "x"),
        CommitGate::Go(None)
    );
}

#[test]
fn a_gesture_that_would_be_refused_does_not_start() {
    let report = price_report();
    let targets = tag_targets(&report, &tag_texts(SCENES_FILE));
    let calls = price_sets_its_own();
    let open = ScopeState::Options(options(Ok(()), Ok(())));
    let reused = ScopeState::Options(options(Err(ScopeLock::ReusedCall), Ok(())));
    let scanning = ScopeState::Unknown {
        note: "Scanning scenes…".to_owned(),
    };
    let start = |remembered, state: &ScopeState, key: &str| {
        gesture_start_refusal(remembered, state, &targets, &prop(key), Some(&calls))
    };

    // The scope it would keep: with nothing remembered, the one shown.
    assert_eq!(start(None, &open, "x"), None);
    assert_eq!(start(None, &reused, "x"), None);
    // A remembered choice now locked: refused at release, so it does not
    // start, rather than starting under the other option.
    assert_eq!(
        start(Some(Scope::ThisScene), &reused, "x"),
        Some(ScopeLock::ReusedCall.text())
    );
    // A row the kept scope would not show in this scene.
    assert_eq!(
        start(Some(Scope::Shared), &open, "width"),
        Some(
            ScopeLock::SetsOwn {
                param: "rectWidth".to_owned()
            }
            .text()
        )
    );
    // While the scenes are scanned: a value written on the element waits
    // for the scan, so the gesture does not start, with the scan's note;
    // one passed at the call goes there.
    assert_eq!(
        start(None, &scanning, "x"),
        Some("Scanning scenes…".to_owned())
    );
    assert_eq!(start(None, &scanning, "width"), None);
    // Not shared: as resolved, whatever was remembered.
    assert_eq!(
        start(Some(Scope::Shared), &ScopeState::NotShared, "x"),
        None
    );
}

#[test]
fn a_held_row_commit_with_no_scope_shown_drains_only_into_this_scene() {
    let report = price_report();
    let targets = tag_targets(&report, &tag_texts(SCENES_FILE));
    let open = ScopeState::Options(options(Ok(()), Ok(())));
    let reused = ScopeState::Options(options(Err(ScopeLock::ReusedCall), Ok(())));
    let scanning = ScopeState::Unknown {
        note: "Scanning scenes…".to_owned(),
    };
    let gate = |held, state: &ScopeState| drain_gate(held, state, &targets, &[prop("x")]);

    // Held with no scope shown (the scenes being scanned): written once
    // the sweep lands when the default is This scene...
    assert_eq!(gate(None, &open), CommitGate::Go(Some(Scope::ThisScene)));
    // ...but never into the shared value the locked This scene falls
    // back to.
    assert_eq!(
        gate(None, &reused),
        CommitGate::Refuse(SCANNED_MEANWHILE.to_owned())
    );
    // Still scanning: it keeps waiting.
    assert_eq!(
        gate(None, &scanning),
        CommitGate::Hold("Scanning scenes…".to_owned())
    );
    // Made with a scope shown: under that scope, as any row commit.
    assert_eq!(
        gate(Some(Scope::Shared), &reused),
        CommitGate::Go(Some(Scope::Shared))
    );
    assert_eq!(gate(None, &ScopeState::NotShared), CommitGate::Go(None));
}

#[test]
fn a_held_gesture_drains_through_the_gesture_gate() {
    let report = price_report();
    let targets = tag_targets(&report, &tag_texts(SCENES_FILE));
    let calls = price_sets_its_own();
    let open = ScopeState::Options(options(Ok(()), Ok(())));
    let reused = ScopeState::Options(options(Err(ScopeLock::ReusedCall), Ok(())));
    let scanning = ScopeState::Unknown {
        note: "Scanning scenes…".to_owned(),
    };
    let gate = |scope, origin, state: &ScopeState, key: &str| {
        held_gate(scope, origin, state, &targets, &[prop(key)], Some(&calls))
    };
    let gesture = HeldOrigin {
        gesture: true,
        scanning: false,
    };
    let row = HeldOrigin {
        gesture: false,
        scanning: false,
    };

    // A scrub under Shared value held for a reload, after which this scene
    // sets its own `rectWidth`: refused as at a release, never written to
    // the default this scene would not show.
    assert_eq!(
        gate(Some(Scope::Shared), gesture, &open, "width"),
        CommitGate::Refuse(
            ScopeLock::SetsOwn {
                param: "rectWidth".to_owned()
            }
            .text()
        )
    );
    // A held row commit writes the default, as before.
    assert_eq!(
        gate(Some(Scope::Shared), row, &open, "width"),
        CommitGate::Go(Some(Scope::Shared))
    );
    // A gesture is never held for the scan.
    assert_eq!(
        gate(None, gesture, &scanning, "x"),
        CommitGate::Refuse("Scanning scenes…".to_owned())
    );
    assert_eq!(
        gate(None, row, &scanning, "x"),
        CommitGate::Hold("Scanning scenes…".to_owned())
    );

    // Held with no option shown and refused by the time it drains: the
    // text names the scan only when it waited for one.
    assert_eq!(
        gate(None, row, &reused, "x"),
        CommitGate::Refuse(CHANGED_MEANWHILE.to_owned())
    );
    let scanned = HeldOrigin {
        gesture: false,
        scanning: true,
    };
    assert_eq!(
        gate(None, scanned, &reused, "x"),
        CommitGate::Refuse(SCANNED_MEANWHILE.to_owned())
    );
    assert_eq!(
        gate(None, gesture, &open, "x"),
        CommitGate::Refuse(CHANGED_MEANWHILE.to_owned())
    );
}

#[test]
fn the_shown_options_are_read_at_the_frame_the_targets_were_looked_up_at() {
    let report = price_report();
    let targets = tag_targets(&report, &tag_texts(SCENES_FILE));
    let occurrences = three_scenes();
    // Looked up at 40, in Price; the playhead has since moved to Intro.
    let resolved = Resolved {
        layer: "rect".to_owned(),
        report: report.clone(),
        timeline_frame: 40,
        outcome: Ok(targets.clone()),
    };

    let shown = options_of(
        resolved_scope_state(&resolved, Some(&occurrences), None, &chapters())
            .expect("the targets resolved"),
    );

    assert_eq!(shown.this_scene, Ok(()));
    // Read at the playhead instead, Price's call would be "elsewhere".
    let at_playhead = options_of(scope_state(
        &report,
        &targets,
        Some(&occurrences),
        None,
        &chapters(),
        10,
    ));
    assert_eq!(
        at_playhead.this_scene,
        Err(price_tag(|component| ScopeLock::OtherScenes { component }))
    );
}

#[test]
fn calls_of_takes_each_instance_s_entry_at_s_s_index() {
    // The wrapper is not in the index's chains, so S is at index 1.
    let report = tag_report(vec![
        entry_at("Sequence", ElementKind::Wrapper, TAG_FILE, 1),
        entry_at("PriceTag", ElementKind::Component, SCENES_FILE, 2),
    ]);
    let element = element_of(
        e(),
        vec![
            instance(vec![e(), s()], vec![(30, 60)]),
            instance(
                vec![e(), intro_call(), pos(SCENES_FILE, 9, 3)],
                vec![(0, 30)],
            ),
            instance(vec![e()], vec![(60, 90)]),
        ],
    );

    assert_eq!(
        calls_of(&report, &element, &s()),
        vec![Some(s()), Some(intro_call()), None]
    );
}
