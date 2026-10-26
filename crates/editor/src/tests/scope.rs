//! Tests of the edit scope rules (`scope`): whether an element is shared,
//! which of "This scene" and "Shared value" it allows, and what each row
//! writes under them.

use super::*;
use crate::impact::{impact_line, impact_view};
use crate::occurrences::OccurrenceState;
use crate::scope::{
    SceneCalls, Scope, ScopeChoices, ScopeLock, ScopeOptions, ScopeState, ScopedRow, calls_of,
    chosen_scope, scope_line, scope_state, scoped_removable, scoped_row,
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
        targets.written_at(),
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
    let view = impact_view(&report, Some(&occurrences), &chapters(), 10, None);
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

    assert_eq!(state(None), unknown("Scanning scenes…"));
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
    // Until the calls are read, that is not known.
    assert_eq!(state(None).shared, Ok(()));
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
    // Passed at S: the shared value is the default, and this scene keeps
    // its own.
    assert_eq!(
        both(&targets, prop("width")),
        (
            ScopedRow::AsResolved,
            ScopedRow::Default {
                shown: Some(EditValue::Number(200.0)),
                own: true
            }
        )
    );
    // Left out at S.
    assert_eq!(
        both(&targets, prop("cornerRadius")),
        (
            ScopedRow::AsResolved,
            ScopedRow::Default {
                shown: Some(EditValue::Number(8.0)),
                own: false
            }
        )
    );
    // No default; Intro and Outro leave it out, so it gets one.
    assert_eq!(
        both(&targets, prop("height")),
        (
            ScopedRow::AsResolved,
            ScopedRow::Default {
                shown: None,
                own: true
            }
        )
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
        (
            ScopedRow::AsResolved,
            ScopedRow::Default {
                shown: Some(EditValue::Number(200.0)),
                own: true
            }
        )
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

    // Drag, scrub and inline text say why a row this scene sets its own
    // value for can't show a shared edit.
    assert_eq!(
        ScopeLock::SetsOwn {
            param: "rectWidth".to_owned()
        }
        .text(),
        "This scene sets its own `rectWidth`"
    );
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
    // Shared value: the literal at the element goes as today; a default
    // is never removed.
    assert!(removable_under(Scope::Shared, "x"));
    assert!(!removable_under(Scope::Shared, "width"));
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
