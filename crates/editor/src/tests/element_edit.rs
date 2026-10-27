//! Tests of resolving and committing element rows (`element_edit`).

use super::*;

/// `prop` set to the number `value`, as a commit takes it.
fn num(prop: TransformProp, value: f64) -> (PropKey, Option<EditValue>) {
    (prop.key(), Some(EditValue::Number(value)))
}

/// Commits `values` with `text` as the element's file, returning its new text.
fn commit_in(
    text: &str,
    expected: &ElementTargets,
    report: &LayerSourceReport,
    values: &[(PropKey, Option<EditValue>)],
) -> Result<String, CommitError> {
    let texts = Texts::from([(expected.file.clone(), text.to_owned())]);
    let files = commit_files(&texts, expected, report, None, None, values)?;
    Ok(files
        .into_iter()
        .next()
        .map_or_else(|| text.to_owned(), |(_, text)| text))
}

#[test]
fn transform_resolve_targets_prefers_scale_xy() {
    let text = "export const Scene = () => (\n  <Rect x={1} scaleX={2} />\n);\n";
    let report = primitive_report(
        2,
        3,
        ReceivedProps {
            x: Some(1.0),
            scale_x: Some(2.0),
            ..ReceivedProps::default()
        },
    );

    let targets = resolve_in(text, &report).expect("the element resolves");

    assert_eq!(
        keys(&targets),
        [
            TransformProp::X.key(),
            TransformProp::Y.key(),
            TransformProp::Rotation.key(),
            TransformProp::ScaleX.key(),
            TransformProp::ScaleY.key()
        ]
    );
    assert!(matches!(
        targets.targets[3].1.target,
        EditTarget::Literal { value: EditValue::Number(value), .. } if value == 2.0
    ));
    assert!(matches!(
        targets.targets[4].1.target,
        EditTarget::Missing { default: Some(EditValue::Number(default)), .. } if default == 1.0
    ));
    assert_eq!(targets.file, PathBuf::from("/project/scene.tsx"));
}

#[test]
fn transform_resolve_targets_uniform_scale() {
    let targets = resolve_in(TRANSFORM_FIXTURE, &primitive_report(2, 3, x_only()))
        .expect("the element resolves");

    assert_eq!(
        keys(&targets),
        [
            TransformProp::X.key(),
            TransformProp::Y.key(),
            TransformProp::Rotation.key(),
            TransformProp::Scale.key()
        ]
    );
    assert_eq!(TransformProp::ScaleX.name(), "scaleX");
    assert!(matches!(
        targets.targets[0].1.target,
        EditTarget::Literal { value: EditValue::Number(value), .. } if value == 100.0
    ));
}

#[test]
fn transform_resolve_targets_fails_without_the_element() {
    assert!(resolve_in(TRANSFORM_FIXTURE, &primitive_report(1, 1, x_only())).is_err());
    assert!(resolve_in(TRANSFORM_FIXTURE, &report_of(Vec::new())).is_err());
}

#[test]
fn transform_commit_writes_literal_and_inserts() {
    let report = primitive_report(2, 3, x_only());
    let expected = resolve_in(TRANSFORM_FIXTURE, &report).unwrap();

    let written = commit_in(
        TRANSFORM_FIXTURE,
        &expected,
        &report,
        &[
            num(TransformProp::X, 120.0),
            num(TransformProp::Rotation, 45.5),
        ],
    )
    .expect("both targets are editable");

    assert_eq!(
        written,
        "export const Scene = () => (\n  <Rect x={120} width={10} rotation={45.5} />\n);\n"
    );
}

#[test]
fn transform_commit_aborts_when_target_moved() {
    let report = primitive_report(2, 3, x_only());
    let expected = resolve_in(TRANSFORM_FIXTURE, &report).unwrap();
    // The element moved within its line, so the report's position misses it.
    let moved = TRANSFORM_FIXTURE.replace("  <Rect", "    <Rect");
    // The element stayed but its `x` became longer, so the span moved.
    let widened = TRANSFORM_FIXTURE.replace("x={100}", "x={1000}");

    for text in [moved, widened] {
        let error = commit_in(&text, &expected, &report, &[num(TransformProp::X, 120.0)])
            .expect_err("the source changed");
        assert_eq!(error, CommitError::SourceChanged);
        assert_eq!(error.to_string(), "The source changed while editing");
    }
}

#[test]
fn transform_commit_refuses_locked_and_non_finite_values() {
    let report = primitive_report(2, 3, x_only());
    let expected = resolve_in(TRANSFORM_FIXTURE, &report).unwrap();

    assert!(matches!(
        commit_in(
            TRANSFORM_FIXTURE,
            &expected,
            &report,
            &[num(TransformProp::X, f64::NAN)]
        ),
        Err(CommitError::NotANumber)
    ));
    // `scaleX` is not a target of a uniformly scaled element.
    assert_eq!(
        commit_in(
            TRANSFORM_FIXTURE,
            &expected,
            &report,
            &[num(TransformProp::ScaleX, 2.0)]
        ),
        Err(CommitError::NotEditable("Scale X".to_owned()))
    );
    assert_eq!(
        CommitError::NotEditable("Scale X".to_owned()).to_string(),
        "Scale X can't be edited here"
    );
}

#[test]
fn commit_files_writes_a_string() {
    let text = "export const Scene = () => (\n  <Text>Sale</Text>\n);\n";
    let report = other_prop_report("Text", "children", ReceivedValue::String("Sale".to_owned()));
    let start = text.find("Sale").unwrap() as u32;
    let expected = single_row(
        text,
        "children",
        EditTarget::Literal {
            span: Span {
                start,
                end: start + 4,
            },
            value: EditValue::String("Sale".to_owned()),
            form: LiteralForm::JsxText,
        },
    );

    let written = commit_in(
        text,
        &expected,
        &report,
        &[(
            PropKey::Prop("children".to_owned()),
            Some(EditValue::String("Sold out".to_owned())),
        )],
    );

    assert_eq!(
        written,
        Ok("export const Scene = () => (\n  <Text>Sold out</Text>\n);\n".to_owned())
    );
}

#[test]
fn commit_files_writes_a_bool() {
    let text = "export const Scene = () => (\n  <Video loop />\n);\n";
    let report = other_prop_report("Video", "loop", ReceivedValue::Bool(true));
    let at = (text.find("loop").unwrap() + "loop".len()) as u32;
    let expected = single_row(
        text,
        "loop",
        EditTarget::Literal {
            span: Span { start: at, end: at },
            value: EditValue::Bool(true),
            form: LiteralForm::BareTrue,
        },
    );

    let written = commit_in(
        text,
        &expected,
        &report,
        &[(
            PropKey::Prop("loop".to_owned()),
            Some(EditValue::Bool(false)),
        )],
    );

    assert_eq!(
        written,
        Ok("export const Scene = () => (\n  <Video loop={false} />\n);\n".to_owned())
    );
}

#[test]
fn written_values_keep_their_type() {
    let mut state = ElementEdit::default();
    state.invalidate();
    let children = PropKey::Prop("children".to_owned());
    state.record_written(
        Some("a"),
        &[(
            children.clone(),
            Some(EditValue::String("Sold out".to_owned())),
        )],
    );
    assert_eq!(
        state.written_value(Some("a"), &children),
        Some(&Some(EditValue::String("Sold out".to_owned())))
    );
}

#[test]
fn transform_reason_texts_match_spec() {
    let cases = [
        (LockReason::SyntaxError, "The source has syntax errors"),
        (
            LockReason::ElementNotFound,
            "Couldn't find this element in the source",
        ),
        (
            LockReason::NotPrimitive,
            "Set by a component, not written on this element",
        ),
        (
            LockReason::Expression {
                source: "W / 2".to_string(),
            },
            "Written as an expression: W / 2",
        ),
        (
            LockReason::UnresolvedIdentifier {
                name: "name".to_string(),
            },
            "Comes from name, which can't be traced",
        ),
        (
            LockReason::SharedBinding {
                name: "enter".to_string(),
                uses: 3,
            },
            "enter is used in 3 places",
        ),
        (
            LockReason::BetweenKeyframes {
                previous: 0.0,
                next: 30.0,
                frame: 15.0,
            },
            "Between keyframes at 0 and 30",
        ),
        (LockReason::OutsideKeyframes, "Outside the keyframes"),
        (
            LockReason::UnsupportedCall {
                callee: "spring".to_string(),
            },
            "Computed by spring()",
        ),
        (
            LockReason::OverriddenByParent,
            "A layout component sets this value",
        ),
        (LockReason::Spread, "May be set by a spread ({...props})"),
        (
            LockReason::Repeated { count: 12 },
            "Rendered 12 times from this element",
        ),
        (
            LockReason::FrameUncertain,
            "Can't tell which frame this animation reads",
        ),
        (
            LockReason::OutsideProject,
            "This file is outside the project",
        ),
        (LockReason::StyleMaySet, "May be set by `style`"),
        (
            LockReason::Exported {
                name: "INTRO".to_string(),
            },
            "Exported, so other files may read it",
        ),
        (
            LockReason::NoSceneDuration {
                id: "intro".to_string(),
            },
            "Scene `intro` has no durationInFrames",
        ),
    ];
    for (reason, text) in cases {
        assert_eq!(reason_text(&reason), text);
    }
}

#[test]
fn pass_through_reason_texts() {
    assert_eq!(
        reason_text(&LockReason::ParameterOfCaller {
            component: "Card".to_owned()
        }),
        "Comes from `<Card>`'s caller"
    );
    assert_eq!(
        reason_text(&LockReason::CallSiteNotFound {
            component: "PriceTag".to_owned()
        }),
        "Can't find where `<PriceTag>` is called"
    );
}

#[test]
fn transform_state_drops_stale_resolutions() {
    let report = primitive_report(2, 3, x_only());
    let moved = primitive_report(2, 3, ReceivedProps::default());
    let targets = || resolve_in(TRANSFORM_FIXTURE, &report);
    let mut state = ElementEdit::default();

    let first = state.begin("a", &report, 0).expect("nothing resolved yet");
    assert_eq!(state.begin("a", &report, 0), None, "already resolving it");
    assert!(state.finish(first, Some("a"), targets()));
    let lookup = Some((&report, 0));
    assert!(state.targets(Some("a"), lookup).is_some());
    assert!(
        state.targets(Some("b"), lookup).is_none(),
        "belongs to layer a"
    );

    // After a write the targets are unknown until resolved again.
    state.invalidate();
    assert!(state.targets(Some("a"), lookup).is_none());
    // The old report describes the old code; wait for the reload's lookup.
    assert_eq!(state.begin("a", &report, 0), None);
    state.reset();
    let second = state
        .begin("a", &report, 0)
        .expect("the reload's lookup resolves again");

    // A newer report supersedes the one in flight.
    let third = state.begin("a", &moved, 0).expect("another report");
    assert!(!state.finish(second, Some("a"), targets()));
    // The selection changed while it resolved.
    assert!(!state.finish(third, Some("b"), targets()));
    assert!(state.targets(Some("a"), Some((&moved, 0))).is_none());
}

#[test]
fn transform_resolve_targets_ignores_a_scale_axis_only_a_parent_sets() {
    // A parent passes `scaleX`, but the element writes neither axis.
    let report = primitive_report(
        2,
        3,
        ReceivedProps {
            x: Some(100.0),
            scale_x: Some(2.0),
            ..ReceivedProps::default()
        },
    );

    let targets = resolve_in(TRANSFORM_FIXTURE, &report).expect("the element resolves");

    assert_eq!(
        keys(&targets),
        [
            TransformProp::X.key(),
            TransformProp::Y.key(),
            TransformProp::Rotation.key(),
            TransformProp::Scale.key()
        ]
    );
}

#[test]
fn transform_reset_resolves_an_identical_report_again() {
    // `<Rect width={10} x={100} />` became `width={100}` elsewhere: the report
    // is the same, but `x` moved.
    let before = "export const Scene = () => (\n  <Rect width={10} x={100} />\n);\n";
    let after = before.replace("width={10}", "width={100}");
    let report = primitive_report(2, 3, x_only());
    let mut state = ElementEdit::default();

    let first = state.begin("a", &report, 0).unwrap();
    assert!(state.finish(first, Some("a"), resolve_in(before, &report)));
    state.set_error(
        Some("a"),
        Some("The source changed while editing".to_owned()),
    );

    // The reload cleared the selection; selecting the layer again.
    state.reset();
    assert_eq!(state.error(Some("a")), None, "the old error is gone");
    let second = state.begin("a", &report, 0).expect("resolved again");
    assert!(state.finish(second, Some("a"), resolve_in(&after, &report)));

    let (targets, report) = state.targets(Some("a"), Some((&report, 0))).unwrap();
    let written = commit_in(&after, targets, report, &[num(TransformProp::X, 120.0)]).unwrap();
    assert_eq!(written, after.replace("x={100}", "x={120}"));
}

#[test]
fn transform_explains_why_there_is_nothing_to_commit() {
    let report = primitive_report(2, 3, x_only());
    let mut state = ElementEdit::default();
    assert_eq!(
        state.unavailable(Some("a")),
        "The source is still being read"
    );

    let request = state.begin("a", &report, 0).unwrap();
    assert_eq!(
        state.unavailable(Some("a")),
        "The source is still being read"
    );
    assert!(state.finish(
        request,
        Some("a"),
        Err("Couldn't read scene.tsx".to_owned())
    ));
    assert_eq!(state.unavailable(Some("a")), "Couldn't read scene.tsx");

    let request = state.begin("a", &report, 1).unwrap();
    assert!(state.finish(request, Some("a"), resolve_in(TRANSFORM_FIXTURE, &report)));
    state.invalidate();
    assert_eq!(state.unavailable(Some("a")), "Select the layer again");
}

#[test]
fn transform_keyframe_jumps_use_the_lookup_frame() {
    // Looked up at timeline frame 100 inside a sequence starting at 40.
    assert_eq!(keyframe_timeline_frame(0.0, 100, 60.0), 40);
    assert_eq!(keyframe_timeline_frame(90.0, 100, 60.0), 130);
    // Not shifted by a sequence.
    assert_eq!(keyframe_timeline_frame(30.0, 12, 12.0), 30);
}

#[test]
fn transform_keyframe_jumps_use_the_candidate_frame() {
    // The report's owner guess is 60, but `frame` reads the Sequence's 30
    // (frame 15 of [0, 30] gives the received 250, frame 60 would be outside).
    let text = "export const Scene = () => {\n  const frame = useCurrentFrame();\n  return <Sequence from={30}><Rect x={interpolate(frame - 15, [0, 30], [100, 400])} /></Sequence>;\n};\n";
    let report = LayerSourceReport {
        local_frame: 60.0,
        candidate_frames: vec![60.0, 30.0],
        ..primitive_report(
            3,
            30,
            ReceivedProps {
                x: Some(250.0),
                ..ReceivedProps::default()
            },
        )
    };
    let targets = resolve_in(text, &report).unwrap();
    let target = targets.transform(TransformProp::X).unwrap();
    assert_eq!(
        target,
        &EditTarget::Locked(LockReason::BetweenKeyframes {
            previous: 15.0,
            next: 45.0,
            frame: 30.0,
        })
    );
    // Looked up at timeline frame 60: keyframes 15 and 45 of the Sequence's
    // frames are timeline frames 45 and 75.
    assert_eq!(keyframe_jump_frames(target, 60), Some((45, 75)));
    assert_eq!(
        keyframe_jump_frames(&EditTarget::Locked(LockReason::OutsideKeyframes), 60),
        None
    );
}

#[test]
fn transform_keyframe_is_the_one_the_owner_frame_reads() {
    let targets = resolve_in(ABOVE_SEQUENCE, &above_sequence_report(vec![])).unwrap();
    let Some(EditTarget::Keyframe { span, frame, .. }) = targets.transform(TransformProp::X) else {
        panic!(
            "expected a keyframe, got {:?}",
            targets.transform(TransformProp::X)
        );
    };
    assert_eq!(span.start as usize, ABOVE_SEQUENCE.rfind("100").unwrap());
    assert_eq!(*frame, 60.0);

    // The frame could be the Sequence's 30 as well, which is the keyframe
    // whose value is also 100: no telling which.
    let targets = resolve_in(ABOVE_SEQUENCE, &above_sequence_report(vec![30.0, 60.0])).unwrap();
    assert_eq!(
        targets.transform(TransformProp::X),
        Some(&EditTarget::Locked(LockReason::FrameUncertain))
    );
    // A literal elsewhere on the element stays editable.
    assert!(matches!(
        targets.transform(TransformProp::Y),
        Some(EditTarget::Missing { .. })
    ));
}

#[test]
fn transform_targets_need_the_current_lookup() {
    let report = primitive_report(2, 3, x_only());
    let other = primitive_report(2, 3, ReceivedProps::default());
    let mut state = ElementEdit::default();
    let request = state.begin("a", &report, 10).unwrap();
    assert!(state.finish(request, Some("a"), resolve_in(TRANSFORM_FIXTURE, &report)));
    assert!(state.targets(Some("a"), Some((&report, 10))).is_some());

    // Seeked: the lookup at the new frame has not been resolved yet.
    assert!(state.targets(Some("a"), Some((&report, 11))).is_none());
    // A newer report for the same frame.
    assert!(state.targets(Some("a"), Some((&other, 10))).is_none());
    // Still looking up.
    assert!(state.targets(Some("a"), None).is_none());
}

#[test]
fn transform_only_a_reset_clears_an_own_write() {
    let report = primitive_report(2, 3, x_only());
    let newer = primitive_report(2, 3, ReceivedProps::default());
    let mut state = ElementEdit::default();
    let request = state.begin("a", &report, 0).unwrap();
    assert!(state.finish(request, Some("a"), resolve_in(TRANSFORM_FIXTURE, &report)));

    // A lookup answered by the old runtime after the write resolves, but the
    // file it was read from is about to be reloaded.
    state.invalidate();
    let request = state.begin("a", &newer, 0).unwrap();
    assert!(state.finish(request, Some("a"), resolve_in(TRANSFORM_FIXTURE, &newer)));
    assert!(state.targets(Some("a"), Some((&newer, 0))).is_none());

    // The kept reload resets the state.
    state.reset();
    let request = state.begin("a", &newer, 0).unwrap();
    assert!(state.finish(request, Some("a"), resolve_in(TRANSFORM_FIXTURE, &newer)));
    assert!(state.targets(Some("a"), Some((&newer, 0))).is_some());
}

#[test]
fn expire_keeps_the_resolution_visible_but_not_current() {
    let report = primitive_report(2, 3, x_only());
    let mut state = ElementEdit::default();
    let request = state.begin("a", &report, 0).unwrap();
    assert!(state.finish(request, Some("a"), resolve_in(TRANSFORM_FIXTURE, &report)));

    state.invalidate();
    state.expire();
    // The rows keep their values, read-only, while the reloaded code answers.
    let (_, current) = state.resolved(Some("a")).expect("still shown");
    assert!(!current);
    assert!(state.targets(Some("a"), Some((&report, 0))).is_none());
    // The reloaded code's answer resolves again even when it is the same.
    assert!(state.begin("a", &report, 0).is_some());
}

#[test]
fn finish_after_expire_is_current_again() {
    let report = primitive_report(2, 3, x_only());
    let mut state = ElementEdit::default();
    let request = state.begin("a", &report, 0).unwrap();
    assert!(state.finish(request, Some("a"), resolve_in(TRANSFORM_FIXTURE, &report)));
    state.invalidate();

    state.expire();
    let request = state.begin("a", &report, 0).unwrap();
    assert!(state.finish(request, Some("a"), resolve_in(TRANSFORM_FIXTURE, &report)));
    assert!(state.resolved(Some("a")).unwrap().1);
    assert!(state.targets(Some("a"), Some((&report, 0))).is_some());
}

#[test]
fn expire_then_another_write_stays_stale() {
    let report = primitive_report(2, 3, x_only());
    let newer = primitive_report(2, 3, ReceivedProps::default());
    let targets = |report| resolve_in(TRANSFORM_FIXTURE, report);
    let mut state = ElementEdit::default();
    state.invalidate();
    state.expire();
    let request = state.begin("a", &report, 0).unwrap();

    // A second write lands before the reloaded code's answer resolves.
    state.invalidate();
    assert!(!state.finish(request, Some("a"), targets(&report)));
    // A lookup the old code answers after that write is not current.
    let request = state.begin("a", &newer, 0).unwrap();
    assert!(state.finish(request, Some("a"), targets(&newer)));
    assert!(state.targets(Some("a"), Some((&newer, 0))).is_none());
}

#[test]
fn written_values_show_until_the_reloaded_code_resolves() {
    let report = primitive_report(2, 3, x_only());
    let targets = || resolve_in(TRANSFORM_FIXTURE, &report);
    let mut state = ElementEdit::default();
    let request = state.begin("a", &report, 0).unwrap();
    assert!(state.finish(request, Some("a"), targets()));

    state.invalidate();
    state.record_written(Some("a"), &[num(TransformProp::X, 184.0)]);
    state.record_written(Some("a"), &[num(TransformProp::Y, 40.0)]);
    let (x, y) = (TransformProp::X.key(), TransformProp::Y.key());
    assert_eq!(
        state.written_value(Some("a"), &x),
        Some(&Some(EditValue::Number(184.0)))
    );
    assert_eq!(
        state.written_value(Some("a"), &y),
        Some(&Some(EditValue::Number(40.0)))
    );
    assert_eq!(
        state.written_value(Some("a"), &TransformProp::Rotation.key()),
        None
    );
    assert_eq!(state.written_value(Some("b"), &x), None);

    // Still shown while the reload runs.
    state.expire();
    assert_eq!(
        state.written_value(Some("a"), &x),
        Some(&Some(EditValue::Number(184.0)))
    );
    // The reloaded code's resolution shows the file's values instead.
    let request = state.begin("a", &report, 0).unwrap();
    assert!(state.finish(request, Some("a"), targets()));
    assert_eq!(state.written_value(Some("a"), &x), None);
}

#[test]
fn written_values_are_dropped_with_the_selection() {
    let mut state = ElementEdit::default();
    state.invalidate();
    state.record_written(Some("a"), &[num(TransformProp::X, 184.0)]);
    state.reset();
    assert_eq!(
        state.written_value(Some("a"), &TransformProp::X.key()),
        None
    );
}

#[test]
fn a_current_resolution_ignores_written_values() {
    // Nothing was written since the targets were read: nothing to show.
    let mut state = ElementEdit::default();
    state.record_written(Some("a"), &[num(TransformProp::X, 184.0)]);
    assert_eq!(
        state.written_value(Some("a"), &TransformProp::X.key()),
        None
    );
}

#[test]
fn transform_files_outside_the_project_are_locked() {
    let entry_dir = Path::new("/project/src");
    assert!(inside_project(
        Path::new("/project/src/scene.tsx"),
        entry_dir
    ));
    assert!(inside_project(
        Path::new("/project/src/parts/title.tsx"),
        entry_dir
    ));
    assert!(!inside_project(
        Path::new("/project/lib/shared.tsx"),
        entry_dir
    ));
    assert!(!inside_project(
        Path::new("/project/src-old/scene.tsx"),
        entry_dir
    ));
    assert!(!inside_project(
        Path::new("/project/src/node_modules/kit/rect.tsx"),
        entry_dir
    ));
    assert!(!inside_project(
        Path::new("/project/src/../lib/shared.tsx"),
        entry_dir
    ));

    let report = primitive_report(2, 3, x_only());
    let mut targets = resolve_in(TRANSFORM_FIXTURE, &report).unwrap();
    lock_outside_project(&mut targets, |_| true);
    assert!(
        targets
            .targets
            .iter()
            .all(|(_, target)| target.target == EditTarget::Locked(LockReason::OutsideProject))
    );
    // A commit refuses a locked target before reading the file's targets.
    assert_eq!(
        commit_in(
            TRANSFORM_FIXTURE,
            &targets,
            &report,
            &[num(TransformProp::X, 5.0)]
        ),
        Err(CommitError::NotEditable("X".to_owned()))
    );
}

fn price_tag_files() -> Texts {
    Texts::from([
        (
            PathBuf::from("/project/component.tsx"),
            PRICE_TAG.to_owned(),
        ),
        (
            PathBuf::from("/project/scene.tsx"),
            PRICE_TAG_SCENE.to_owned(),
        ),
    ])
}

#[test]
fn x_passed_at_the_call_site_is_a_pass_through_target() {
    let report = price_tag_report();
    let targets = resolve_targets(read_from(&price_tag_files()), &report, None)
        .expect("the element resolves");

    assert_eq!(targets.file, PathBuf::from("/project/component.tsx"));
    assert_eq!(
        targets.file_of(&TransformProp::X.key()),
        Path::new("/project/scene.tsx")
    );
    let Some(EditTarget::PassThrough { param, target, .. }) = targets.transform(TransformProp::X)
    else {
        panic!(
            "x passes through: {:?}",
            targets.transform(TransformProp::X)
        );
    };
    assert_eq!(param, "x");
    assert!(matches!(
        **target,
        EditTarget::Literal { value: EditValue::Number(value), .. } if value == 40.0
    ));
    assert_eq!(
        targets.targets[0].1.call_site,
        Some(SourcePos {
            file: PathBuf::from("/project/scene.tsx"),
            line: 2,
            column: 3,
        })
    );

    // `rotation` is written on the element itself.
    assert_eq!(
        targets.file_of(&TransformProp::Rotation.key()),
        Path::new("/project/component.tsx")
    );
    assert!(matches!(
        targets.transform(TransformProp::Rotation),
        Some(EditTarget::Literal { value: EditValue::Number(value), .. }) if *value == 10.0
    ));
    assert_eq!(targets.targets[2].1.call_site, None);
}

#[test]
fn a_value_written_on_the_element_reads_only_its_file() {
    // Only the element's file can be read: no prop passes through.
    let texts = Texts::from([(
        PathBuf::from("/project/scene.tsx"),
        TRANSFORM_FIXTURE.to_owned(),
    )]);
    let report = LayerSourceReport {
        chain: vec![
            primitive_report(2, 3, x_only()).chain[0].clone(),
            entry_at("Scene", ElementKind::Component, "/project/index.tsx", 2),
        ],
        ..primitive_report(2, 3, x_only())
    };

    let targets = resolve_targets(read_from(&texts), &report, None).expect("the element resolves");

    assert!(
        targets
            .targets
            .iter()
            .all(|(_, target)| target.file == Path::new("/project/scene.tsx"))
    );
}

#[test]
fn a_drag_writes_two_files() {
    let texts = price_tag_files();
    let report = price_tag_report();
    let targets = resolve_targets(read_from(&texts), &report, None).unwrap();

    let written = commit_files(
        &texts,
        &targets,
        &report,
        None,
        None,
        &[
            num(TransformProp::X, 60.0),
            num(TransformProp::Rotation, 45.0),
        ],
    )
    .expect("both targets are editable");

    assert_eq!(
        written,
        vec![
            (
                PathBuf::from("/project/component.tsx"),
                PRICE_TAG.replace("rotation={10}", "rotation={45}")
            ),
            (
                PathBuf::from("/project/scene.tsx"),
                PRICE_TAG_SCENE.replace("x={40}", "x={60}")
            ),
        ]
    );
}

#[test]
fn a_value_missing_at_the_call_site_is_inserted_there() {
    let component = PRICE_TAG.replace("({ x })", "({ x = 0 })");
    let scene = PRICE_TAG_SCENE.replace(" x={40}", "");
    let texts = Texts::from([
        (PathBuf::from("/project/component.tsx"), component),
        (PathBuf::from("/project/scene.tsx"), scene.clone()),
    ]);
    let report = LayerSourceReport {
        props: ReceivedProps {
            x: Some(0.0),
            rotation: Some(10.0),
            ..ReceivedProps::default()
        },
        ..price_tag_report()
    };
    let targets = resolve_targets(read_from(&texts), &report, None).unwrap();
    let Some(EditTarget::PassThrough { target, .. }) = targets.transform(TransformProp::X) else {
        panic!(
            "x passes through: {:?}",
            targets.transform(TransformProp::X)
        );
    };
    assert!(matches!(**target, EditTarget::Missing { .. }));

    let written = commit_files(
        &texts,
        &targets,
        &report,
        None,
        None,
        &[num(TransformProp::X, 60.0)],
    )
    .unwrap();

    assert_eq!(
        written,
        vec![(
            PathBuf::from("/project/scene.tsx"),
            scene.replace("<PriceTag />", "<PriceTag x={60} />")
        )]
    );
}

#[test]
fn a_component_called_in_its_own_file_is_written_once() {
    let text = format!("{PRICE_TAG}{PRICE_TAG_SCENE}");
    let texts = Texts::from([(PathBuf::from("/project/scene.tsx"), text.clone())]);
    let report = LayerSourceReport {
        chain: vec![
            entry_at("Rect", ElementKind::Primitive, "/project/scene.tsx", 2),
            entry_at("PriceTag", ElementKind::Component, "/project/scene.tsx", 5),
        ],
        ..price_tag_report()
    };
    let targets = resolve_targets(read_from(&texts), &report, None).unwrap();

    let written = commit_files(
        &texts,
        &targets,
        &report,
        None,
        None,
        &[
            num(TransformProp::X, 60.0),
            num(TransformProp::Rotation, 45.0),
        ],
    )
    .unwrap();

    assert_eq!(
        written,
        vec![(
            PathBuf::from("/project/scene.tsx"),
            text.replace("rotation={10}", "rotation={45}")
                .replace("x={40}", "x={60}")
        )]
    );
}

#[test]
fn a_changed_call_site_aborts() {
    let report = price_tag_report();
    let targets = resolve_targets(read_from(&price_tag_files()), &report, None).unwrap();
    // `<PriceTag` moved within its line, so the report's position misses it.
    let moved = PRICE_TAG_SCENE.replace("  <PriceTag", "    <PriceTag");
    // The call stayed but its `x` became longer, so the span moved.
    let widened = PRICE_TAG_SCENE.replace("x={40}", "x={400}");

    for scene in [moved, widened] {
        let mut texts = price_tag_files();
        texts.insert(PathBuf::from("/project/scene.tsx"), scene);
        // Rotation is in the unchanged file, but the edit is one: nothing
        // is written.
        assert_eq!(
            commit_files(
                &texts,
                &targets,
                &report,
                None,
                None,
                &[
                    num(TransformProp::X, 60.0),
                    num(TransformProp::Rotation, 45.0)
                ],
            ),
            Err(CommitError::SourceChanged)
        );
    }
}

#[test]
fn a_call_site_outside_the_project_locks_only_its_targets() {
    let texts = price_tag_files();
    let report = price_tag_report();
    let mut targets = resolve_targets(read_from(&texts), &report, None).unwrap();

    lock_outside_project(&mut targets, |file| file == Path::new("/project/scene.tsx"));

    assert_eq!(
        targets.transform(TransformProp::X),
        Some(&EditTarget::Locked(LockReason::OutsideProject))
    );
    assert_eq!(
        commit_files(
            &texts,
            &targets,
            &report,
            None,
            None,
            &[num(TransformProp::X, 60.0)]
        ),
        Err(CommitError::NotEditable("X".to_owned()))
    );
    // The element's own file is still written.
    assert_eq!(
        commit_files(
            &texts,
            &targets,
            &report,
            None,
            None,
            &[num(TransformProp::Rotation, 45.0)]
        ),
        Ok(vec![(
            PathBuf::from("/project/component.tsx"),
            PRICE_TAG.replace("rotation={10}", "rotation={45}")
        )])
    );
}

#[test]
fn a_value_passed_on_again_still_commits_the_element_s_own_props() {
    // `Card` passes its own `x` on to `<PriceTag>`: a second hop.
    let mut texts = price_tag_files();
    texts.insert(
        PathBuf::from("/project/scene.tsx"),
        "export const Card = ({ x }) => (\n  <PriceTag x={x} />\n);\n".to_owned(),
    );
    let report = price_tag_report();
    let targets = resolve_targets(read_from(&texts), &report, None).unwrap();

    assert_eq!(
        targets.transform(TransformProp::X),
        Some(&EditTarget::Locked(LockReason::ParameterOfCaller {
            component: "Card".to_owned()
        }))
    );
    // The call site was read, so a commit reads it again.
    assert_eq!(
        targets.files,
        [
            PathBuf::from("/project/component.tsx"),
            PathBuf::from("/project/scene.tsx")
        ]
    );
    let read: Texts = targets
        .files
        .iter()
        .map(|file| (file.clone(), texts[file].clone()))
        .collect();
    assert_eq!(
        commit_files(
            &read,
            &targets,
            &report,
            None,
            None,
            &[num(TransformProp::Rotation, 45.0)]
        ),
        Ok(vec![(
            PathBuf::from("/project/component.tsx"),
            PRICE_TAG.replace("rotation={10}", "rotation={45}")
        )])
    );
}

/// `PriceTag` and the `Frame` it passes `<Rect>` to as children are both
/// `memo` of an arrow, which the runtime reports as "Anonymous".
const MEMO_PRICE_TAG: &str = "export const Frame = memo(({ children }) => <Group>{children}</Group>);\nexport const PriceTag = memo(({ x = 0 }) => (\n  <Frame>\n    <Rect x={x} />\n  </Frame>\n));\n";

fn memo_report(chain: Vec<LayerSource>) -> LayerSourceReport {
    LayerSourceReport {
        props: ReceivedProps {
            x: Some(40.0),
            ..ReceivedProps::default()
        },
        ..report_of(chain)
    }
}

#[test]
fn an_anonymous_call_site_must_call_the_enclosing_component() {
    let texts = Texts::from([
        (
            PathBuf::from("/project/component.tsx"),
            MEMO_PRICE_TAG.to_owned(),
        ),
        (
            PathBuf::from("/project/scene.tsx"),
            PRICE_TAG_SCENE.to_owned(),
        ),
    ]);
    let rect = LayerSource {
        column: 5,
        ..entry_at("Rect", ElementKind::Primitive, "/project/component.tsx", 4)
    };
    let price_tag_call = entry_at("Anonymous", ElementKind::Component, "/project/scene.tsx", 2);

    // The nearest anonymous entry is `<Frame>`, not a call of `PriceTag`.
    let report = memo_report(vec![
        rect.clone(),
        entry_at(
            "Anonymous",
            ElementKind::Component,
            "/project/component.tsx",
            3,
        ),
        price_tag_call.clone(),
    ]);
    let targets = resolve_targets(read_from(&texts), &report, None).unwrap();
    assert_eq!(
        targets.transform(TransformProp::X),
        Some(&EditTarget::Locked(LockReason::CallSiteNotFound {
            component: "PriceTag".to_owned()
        }))
    );
    assert_eq!(
        commit_files(
            &texts,
            &targets,
            &report,
            None,
            None,
            &[num(TransformProp::X, 60.0)]
        ),
        Err(CommitError::NotEditable("X".to_owned()))
    );

    // An anonymous entry whose tag is `<PriceTag>` is its call.
    let report = memo_report(vec![rect, price_tag_call]);
    let targets = resolve_targets(read_from(&texts), &report, None).unwrap();
    assert_eq!(
        targets.file_of(&TransformProp::X.key()),
        Path::new("/project/scene.tsx")
    );
    assert!(matches!(
        targets.transform(TransformProp::X),
        Some(EditTarget::PassThrough { .. })
    ));
}

#[test]
fn an_unreadable_call_site_locks_only_the_props_passed_through() {
    let texts = Texts::from([(
        PathBuf::from("/project/component.tsx"),
        PRICE_TAG.to_owned(),
    )]);
    let report = price_tag_report();

    let targets = resolve_targets(read_from(&texts), &report, None).expect("the element resolves");

    assert_eq!(
        targets.transform(TransformProp::X),
        Some(&EditTarget::Locked(LockReason::CallSiteNotFound {
            component: "PriceTag".to_owned()
        }))
    );
    assert_eq!(targets.files, [PathBuf::from("/project/component.tsx")]);
    assert_eq!(
        commit_files(
            &texts,
            &targets,
            &report,
            None,
            None,
            &[num(TransformProp::Rotation, 45.0)]
        ),
        Ok(vec![(
            PathBuf::from("/project/component.tsx"),
            PRICE_TAG.replace("rotation={10}", "rotation={45}")
        )])
    );
}

#[test]
fn an_edit_is_written_at_the_call_site_only_when_every_editable_prop_passes_through() {
    let price_tag_call = SourcePos {
        file: PathBuf::from("/project/scene.tsx"),
        line: 2,
        column: 3,
    };
    let component = "export const PriceTag = ({ x, y = 0, rotation = 0, scale = 1 }) => (\n  <Rect x={x} y={y} rotation={rotation} scale={scale} />\n);\n";
    let texts = Texts::from([
        (
            PathBuf::from("/project/component.tsx"),
            component.to_owned(),
        ),
        (
            PathBuf::from("/project/scene.tsx"),
            PRICE_TAG_SCENE.to_owned(),
        ),
    ]);
    let report = LayerSourceReport {
        props: ReceivedProps {
            x: Some(40.0),
            y: Some(0.0),
            rotation: Some(0.0),
            scale: Some(1.0),
            ..ReceivedProps::default()
        },
        ..price_tag_report()
    };
    let mut targets = resolve_targets(read_from(&texts), &report, None).unwrap();
    assert_eq!(targets.written_at(), Some(&price_tag_call));

    // A locked prop on the element is not written, so it does not count.
    targets.targets[2].1 = Target {
        file: PathBuf::from("/project/component.tsx"),
        path: "rotation".to_owned(),
        target: EditTarget::Locked(LockReason::Spread),
        call_site: None,
        inputs: Vec::new(),
    };
    assert_eq!(targets.written_at(), Some(&price_tag_call));

    // `rotation` written on the element changes every instance.
    let targets =
        resolve_targets(read_from(&price_tag_files()), &price_tag_report(), None).unwrap();
    assert_eq!(targets.written_at(), None);

    // Nothing editable writes nowhere.
    let mut locked = targets;
    lock_outside_project(&mut locked, |_| true);
    assert_eq!(locked.written_at(), None);
}

#[test]
fn table_defaults_per_kind() {
    assert_eq!(
        table_default(&number_field(Some(2.0))),
        Some(EditValue::Number(2.0))
    );
    assert_eq!(table_default(&number_field(None)), None);
    assert_eq!(
        table_default(&PrimitivePropertyField::Select {
            options: vec!["contain".to_owned(), "cover".to_owned()],
            default_value: Some("cover".to_owned()),
        }),
        Some(EditValue::String("cover".to_owned()))
    );
    assert_eq!(
        table_default(&PrimitivePropertyField::Boolean {
            default_value: Some(false)
        }),
        Some(EditValue::Bool(false))
    );
    assert_eq!(table_default(&PrimitivePropertyField::Path), None);
}

#[test]
fn a_table_resolves_every_row_in_order() {
    let text = "export const Scene = () => (\n  <Rect x={10} fill=\"#fff\" width={10} height={10} />\n);\n";
    let mut report = primitive_report(
        2,
        3,
        ReceivedProps {
            x: Some(10.0),
            ..ReceivedProps::default()
        },
    );
    report
        .props
        .others
        .insert("fill".to_owned(), ReceivedValue::String("#fff".to_owned()));

    let targets = resolve_table(text, &report, &rect_table());

    assert_eq!(
        keys(&targets),
        [
            "x",
            "y",
            "rotation",
            "scale",
            "opacity",
            "fill",
            "cornerRadius"
        ]
        .map(prop)
    );
    assert!(matches!(
        targets.target(&prop("fill")),
        Some(EditTarget::Literal { value: EditValue::String(fill), .. }) if fill == "#fff"
    ));
    assert!(matches!(
        targets.target(&prop("cornerRadius")),
        Some(EditTarget::Missing { .. })
    ));
    assert!(matches!(
        targets.target(&prop("scale")),
        Some(EditTarget::Missing { .. })
    ));
}

#[test]
fn per_axis_scale_replaces_scale_in_place() {
    let text = "export const Scene = () => (\n  <Rect x={10} scaleX={2} />\n);\n";
    let report = primitive_report(
        2,
        3,
        ReceivedProps {
            x: Some(10.0),
            scale_x: Some(2.0),
            ..ReceivedProps::default()
        },
    );

    let targets = resolve_table(text, &report, &rect_table());

    assert_eq!(
        keys(&targets),
        [
            "x",
            "y",
            "rotation",
            "scaleX",
            "scaleY",
            "opacity",
            "fill",
            "cornerRadius"
        ]
        .map(prop)
    );
}

#[test]
fn text_shorthand_edits_the_style_property() {
    let file = Path::new("/project/scene.tsx");
    let text = "export const Scene = () => (\n  <Text style={{ fontSize: 48 }} fontWeight={700}>A</Text>\n);\n";
    let report = text_report(
        file,
        &[
            ("style.fontSize", ReceivedValue::Number(48.0)),
            ("fontWeight", ReceivedValue::Number(700.0)),
            ("children", ReceivedValue::String("A".to_owned())),
        ],
    );

    let targets = resolve_table(text, &report, &text_table());

    let font_size = targets.entry(&prop("fontSize")).unwrap();
    assert_eq!(font_size.path, "style.fontSize");
    assert!(matches!(
        font_size.target,
        EditTarget::Literal { value: EditValue::Number(size), .. } if size == 48.0
    ));
    let font_weight = targets.entry(&prop("fontWeight")).unwrap();
    assert_eq!(font_weight.path, "fontWeight");
    assert!(matches!(
        font_weight.target,
        EditTarget::Literal { value: EditValue::Number(weight), .. } if weight == 700.0
    ));

    // `style` is not an object literal: it may set either.
    let computed = text.replace("{{ fontSize: 48 }}", "{mono(20)}");
    let targets = resolve_table(&computed, &report, &text_table());
    for key in ["fontSize", "fontWeight"] {
        let row = targets.entry(&prop(key)).unwrap();
        assert_eq!(row.path, key);
        assert_eq!(row.target, EditTarget::Locked(LockReason::StyleMaySet));
    }
}

#[test]
fn a_literal_of_another_kind_locks() {
    let text = "export const Scene = () => (\n  <Text anchorY=\"baseline\">A</Text>\n);\n";
    let report = text_report(
        Path::new("/project/scene.tsx"),
        &[
            ("anchorY", ReceivedValue::String("baseline".to_owned())),
            ("children", ReceivedValue::String("A".to_owned())),
        ],
    );

    let targets = resolve_table(text, &report, &text_table());

    assert_eq!(
        targets.target(&prop("anchorY")),
        Some(&EditTarget::Locked(LockReason::Expression {
            source: "\"baseline\"".to_owned()
        }))
    );
    // A string row with a string literal stays editable.
    assert!(matches!(
        targets.target(&prop("children")),
        Some(EditTarget::Literal { .. })
    ));
}

const TAG: &str = "export function Tag({ padding = 8 }) {\n  const w = 100;\n  return <Rect width={w + padding * 2} height={10} />;\n}\n";
const TAG_SCENE: &str = "export const Scene = () => (\n  <Tag padding={16} />\n);\n";

fn tag_files() -> Texts {
    Texts::from([
        (PathBuf::from("/project/component.tsx"), TAG.to_owned()),
        (PathBuf::from("/project/scene.tsx"), TAG_SCENE.to_owned()),
    ])
}

/// The `<Rect>` in `Tag`, called from the scene as `<Tag padding={16} />`.
fn tag_report() -> LayerSourceReport {
    let rect = LayerSource {
        column: 10,
        ..entry_at("Rect", ElementKind::Primitive, "/project/component.tsx", 3)
    };
    LayerSourceReport {
        props: ReceivedProps {
            others: [
                ("width".to_owned(), ReceivedValue::Number(132.0)),
                ("height".to_owned(), ReceivedValue::Number(10.0)),
            ]
            .into(),
            ..ReceivedProps::default()
        },
        ..report_of(vec![
            rect,
            entry_at("Tag", ElementKind::Component, "/project/scene.tsx", 2),
        ])
    }
}

fn size_table() -> PrimitivePropertyTable {
    property_table(vec![(
        "Size",
        vec![
            ("width", number_field(None)),
            ("height", number_field(None)),
        ],
    )])
}

#[test]
fn an_expression_row_lists_its_inputs() {
    let report = tag_report();

    let targets = resolve_targets(read_from(&tag_files()), &report, Some(&size_table()))
        .expect("the element resolves");

    let width = targets.entry(&prop("width")).unwrap();
    assert!(matches!(
        width.target,
        EditTarget::Locked(LockReason::Expression { .. })
    ));
    let names: Vec<&str> = width.inputs.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["w", "padding"]);
    let (_, w) = &width.inputs[0];
    assert_eq!(w.file, Path::new("/project/component.tsx"));
    assert_eq!(w.call_site, None);
    assert!(matches!(
        w.target,
        EditTarget::Literal { value: EditValue::Number(value), .. } if value == 100.0
    ));
    let (_, padding) = &width.inputs[1];
    assert_eq!(padding.file, Path::new("/project/scene.tsx"));
    assert_eq!(
        padding.call_site,
        Some(SourcePos {
            file: PathBuf::from("/project/scene.tsx"),
            line: 2,
            column: 3,
        })
    );
    assert!(matches!(padding.target, EditTarget::PassThrough { .. }));
    // Only an expression row has inputs.
    assert!(targets.entry(&prop("height")).unwrap().inputs.is_empty());
    assert_eq!(
        targets.files,
        [
            PathBuf::from("/project/component.tsx"),
            PathBuf::from("/project/scene.tsx")
        ]
    );
}

#[test]
fn committing_an_input_writes_its_target() {
    let texts = tag_files();
    let report = tag_report();
    let targets = resolve_targets(read_from(&texts), &report, Some(&size_table())).unwrap();
    let padding = PropKey::Input {
        row: "width".to_owned(),
        name: "padding".to_owned(),
    };

    let written = commit_files(
        &texts,
        &targets,
        &report,
        None,
        None,
        &[(padding, Some(EditValue::Number(24.0)))],
    );

    assert_eq!(
        written,
        Ok(vec![(
            PathBuf::from("/project/scene.tsx"),
            TAG_SCENE.replace("padding={16}", "padding={24}")
        )])
    );
}

#[test]
fn a_text_with_outer_spaces_is_written_in_braces() {
    let text = "export const Scene = () => (\n  <Text>Sale</Text>\n);\n";
    let report = text_report(
        Path::new("/project/scene.tsx"),
        &[("children", ReceivedValue::String("Sale".to_owned()))],
    );
    let targets = resolve_table(text, &report, &text_table());

    let written = commit_in(
        text,
        &targets,
        &report,
        &[(
            prop("children"),
            Some(EditValue::String(" Sale ".to_owned())),
        )],
    );

    assert_eq!(written, Ok(text.replace(">Sale<", ">{\" Sale \"}<")));
}

#[test]
fn a_font_row_beside_a_style_object_inserts_the_shorthand() {
    let dir = std::env::temp_dir().join(format!(
        "celesta-font-row-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("scene.tsx");
    let text = "export const Scene = () => (\n  <Text style={{ fontWeight: 700 }}>A</Text>\n);\n";
    fs::write(&file, text).unwrap();
    let report = text_report(
        &file,
        &[
            ("style.fontWeight", ReceivedValue::Number(700.0)),
            ("children", ReceivedValue::String("A".to_owned())),
        ],
    );
    let read = |path: &Path| fs::read_to_string(path).map_err(|error| error.to_string());
    let targets = resolve_targets(read, &report, Some(&text_table())).unwrap();

    // The object does not set it: the shorthand is the prop to write.
    let font_size = targets.entry(&prop("fontSize")).unwrap();
    assert_eq!(font_size.path, "fontSize");
    assert!(matches!(font_size.target, EditTarget::Missing { .. }));

    let files = write_all_recorded(targets.files.clone(), |texts| {
        commit_files(
            texts,
            &targets,
            &report,
            None,
            None,
            &[(prop("fontSize"), Some(EditValue::Number(48.0)))],
        )
        .map_err(|error| error.to_string())
    })
    .unwrap();
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "export const Scene = () => (\n  <Text style={{ fontWeight: 700 }} fontSize={48}>A</Text>\n);\n"
    );
    let mut history = EditHistory::default();
    history.push(HistoryEntry { files });
    history.undo().unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), text);
    fs::remove_dir_all(&dir).ok();

    // An object that sets it is written inside.
    let both = "export const Scene = () => (\n  <Text style={{ fontSize: 40, fontWeight: 700 }}>A</Text>\n);\n";
    let report = text_report(
        Path::new("/project/scene.tsx"),
        &[
            ("style.fontSize", ReceivedValue::Number(40.0)),
            ("style.fontWeight", ReceivedValue::Number(700.0)),
            ("children", ReceivedValue::String("A".to_owned())),
        ],
    );
    let targets = resolve_table(both, &report, &text_table());
    assert_eq!(
        commit_in(
            both,
            &targets,
            &report,
            &[(prop("fontSize"), Some(EditValue::Number(48.0)))]
        ),
        Ok(both.replace("fontSize: 40", "fontSize: 48"))
    );
}

#[test]
fn component_section_lists_the_call_sites_props() {
    let targets = resolve_targets(read_from(&props_tag_files()), &props_tag_report(), None)
        .expect("the element resolves");

    let component = targets.component.as_ref().expect("a component section");
    assert_eq!(component.name, "PriceTag");
    assert_eq!(
        component.call_site,
        SourcePos {
            file: PathBuf::from("/project/scene.tsx"),
            line: 2,
            column: 3,
        }
    );
    // `x` shows once, in its own row passing through to the call site.
    let props: Vec<(&str, &ControlKind)> = component
        .props
        .iter()
        .map(|prop| (prop.name.as_str(), &prop.kind))
        .collect();
    assert_eq!(
        props,
        [
            ("label", &ControlKind::String),
            ("tone", &ControlKind::Number),
            ("on", &ControlKind::Boolean),
        ]
    );
    let label = &component.props[0].target;
    assert_eq!(label.file, Path::new("/project/scene.tsx"));
    assert!(matches!(
        &label.target,
        EditTarget::Literal { value: EditValue::String(text), .. } if text == "Sale"
    ));
    assert!(matches!(
        targets.transform(TransformProp::X),
        Some(EditTarget::PassThrough { .. })
    ));
}

#[test]
fn component_props_use_the_registered_schema() {
    let mut targets =
        resolve_targets(read_from(&props_tag_files()), &props_tag_report(), None).unwrap();
    let schemas = std::collections::BTreeMap::from([(
        "PriceTag".to_owned(),
        std::collections::BTreeMap::from([(
            "label".to_owned(),
            celesta_react_bridge::ComponentPropertyField::Color {
                label: None,
                default_value: "#000000".to_owned(),
            }
            .into(),
        )]),
    )]);

    type_component_props(&mut targets, &schemas, read_from(&props_tag_files()));

    let component = targets.component.as_ref().unwrap();
    assert_eq!(component.props[0].name, "label");
    assert_eq!(component.props[0].kind, ControlKind::Color);
    // A prop the schema does not declare keeps its literal's kind.
    assert_eq!(component.props[1].kind, ControlKind::Number);
}

#[test]
fn a_schema_locked_prop_does_not_block_other_commits() {
    let texts = typed_props_tag_files();
    let targets = typed_props_tag_targets(&texts);
    let report = props_tag_report();

    let written = commit_files(
        &texts,
        &targets,
        &report,
        None,
        None,
        &[(
            PropKey::CallSite("tone".to_owned()),
            Some(EditValue::Number(5.0)),
        )],
    );

    let scene = &texts[Path::new("/project/scene.tsx")];
    assert_eq!(
        written,
        Ok(vec![(
            PathBuf::from("/project/scene.tsx"),
            scene.replace("tone={2}", "tone={5}")
        )])
    );
}

#[test]
fn a_moved_call_site_attribute_refuses_the_commit() {
    let texts = props_tag_files();
    let report = props_tag_report();
    let targets = resolve_targets(read_from(&texts), &report, None).unwrap();
    // `label` moved at the call; the Rect's own `y` is what is written.
    let mut moved = texts.clone();
    moved.insert(
        PathBuf::from("/project/scene.tsx"),
        PROPS_TAG_SCENE.replace("label=\"Sale\"", "label=\"On sale\""),
    );

    assert_eq!(
        commit_files(
            &moved,
            &targets,
            &report,
            None,
            None,
            &[num(TransformProp::Y, 5.0)]
        ),
        Err(CommitError::SourceChanged)
    );
}

#[test]
fn the_component_section_leaves_out_children() {
    let mut texts = props_tag_files();
    texts.insert(
        PathBuf::from("/project/scene.tsx"),
        PROPS_TAG_SCENE.replace(" on />", " children=\"A\" />"),
    );
    let targets = resolve_targets(read_from(&texts), &props_tag_report(), None).unwrap();
    let names: Vec<&str> = targets
        .component
        .as_ref()
        .unwrap()
        .props
        .iter()
        .map(|prop| prop.name.as_str())
        .collect();
    assert_eq!(names, ["label", "tone"]);
}

#[test]
fn component_prop_commit_writes_the_call_site() {
    let texts = props_tag_files();
    let report = props_tag_report();
    let targets = resolve_targets(read_from(&texts), &report, None).unwrap();

    let written = commit_files(
        &texts,
        &targets,
        &report,
        None,
        None,
        &[(
            PropKey::CallSite("label".to_owned()),
            Some(EditValue::String("Sold".to_owned())),
        )],
    );

    assert_eq!(
        written,
        Ok(vec![(
            PathBuf::from("/project/scene.tsx"),
            PROPS_TAG_SCENE.replace("label=\"Sale\"", "label=\"Sold\"")
        )])
    );
}

#[test]
fn no_component_section_without_a_call_site() {
    let report = primitive_report(2, 3, x_only());
    let targets = resolve_in(TRANSFORM_FIXTURE, &report).unwrap();
    assert_eq!(targets.component, None);
}

#[test]
fn removing_a_prop_is_one_undoable_write() {
    let dir = std::env::temp_dir().join(format!(
        "celesta-remove-prop-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("scene.tsx");
    let (text, _, mut report) = removable_rect();
    fs::write(&file, &text).unwrap();
    report.chain[0].file = file.clone();
    let read = |path: &Path| fs::read_to_string(path).map_err(|error| error.to_string());
    let table = with_required(rect_table(), &[]);
    let targets = resolve_targets(read, &report, Some(&table)).unwrap();

    let files = write_all_recorded(targets.files.clone(), |texts| {
        commit_files(
            texts,
            &targets,
            &report,
            None,
            None,
            &[(prop("cornerRadius"), None)],
        )
        .map_err(|error| error.to_string())
    })
    .unwrap();
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        text.replace(" cornerRadius={8}", "")
    );
    let mut history = EditHistory::default();
    history.push(HistoryEntry { files });
    history.undo().unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), text);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_edit_whose_result_does_not_parse_is_refused() {
    let text = "export const A = () => <Rect x={1} y={2} />;";
    let at = text.find("x={1}").unwrap() as u32;
    // Deleting the name alone leaves `={1}`.
    let broken = [Edit::Remove {
        start: at,
        end: at + 1,
    }];
    let path = Path::new("/project/scene.tsx");
    assert_eq!(
        apply_checked(path, text, &broken),
        Err(CommitError::Unparseable)
    );
    assert_eq!(
        CommitError::Unparseable.to_string(),
        "Couldn't edit the source: the result does not parse"
    );
    let whole = [Edit::Remove {
        start: at - 1,
        end: at + 5,
    }];
    assert_eq!(
        apply_checked(path, text, &whole),
        Ok("export const A = () => <Rect y={2} />;".to_owned())
    );

    // Checked as the file's type: `<number>v` is a `.ts` cast, not TSX.
    let text = "const v: unknown = 1;\nconst n = <number>v;\n";
    let at = text.find('1').unwrap() as u32;
    let edit = [Edit::Replace {
        start: at,
        end: at + 1,
        value: EditValue::Number(2.0),
        form: LiteralForm::Number,
    }];
    assert_eq!(
        apply_checked(Path::new("/project/timeline.ts"), text, &edit),
        Ok(text.replace('1', "2"))
    );
    assert_eq!(
        apply_checked(Path::new("/project/timeline.tsx"), text, &edit),
        Err(CommitError::Unparseable)
    );
}

#[test]
fn a_required_or_absent_prop_is_not_removed() {
    let (text, targets, report) = removable_rect();
    assert_eq!(
        commit_in(&text, &targets, &report, &[(prop("width"), None)]),
        Err(CommitError::NotRemovable("Width".to_owned()))
    );
    assert_eq!(
        commit_in(&text, &targets, &report, &[(prop("x"), None)]),
        Err(CommitError::NotRemovable("X".to_owned()))
    );
    assert_eq!(
        CommitError::NotRemovable("Width".to_owned()).to_string(),
        "Width can't be removed here"
    );
}

#[test]
fn a_style_property_row_is_removed_from_the_object() {
    let text = "export const Scene = () => (\n  <Text style={{ fontSize: 40, fontWeight: 700 }}>A</Text>\n);\n";
    let report = text_report(
        Path::new("/project/scene.tsx"),
        &[
            ("style.fontSize", ReceivedValue::Number(40.0)),
            ("style.fontWeight", ReceivedValue::Number(700.0)),
            ("children", ReceivedValue::String("A".to_owned())),
        ],
    );
    let targets = resolve_table(text, &report, &text_table());
    assert_eq!(
        commit_in(text, &targets, &report, &[(prop("fontSize"), None)]),
        Ok(text.replace("fontSize: 40, ", ""))
    );
}

#[test]
fn a_long_expression_shows_on_one_shortened_line() {
    let source =
        "[\n  { x: 10, label: 'Sale' },\n  { x: 20, label: 'New' },\n  { x: 30, label: 'Hot' },\n]";
    let text = reason_text(&LockReason::Expression {
        source: source.to_owned(),
    });
    assert_eq!(
        text,
        "Written as an expression: [ { x: 10, label: 'Sale' }, { x: 20, label: 'New' }, { x: 30…"
    );
    // A short one stays whole, its whitespace collapsed.
    assert_eq!(
        reason_text(&LockReason::Expression {
            source: "W\n  / 2".to_owned()
        }),
        "Written as an expression: W / 2"
    );
}

#[test]
fn a_held_commit_is_stamped_with_the_frame_its_row_showed() {
    let report = primitive_report(2, 3, x_only());
    let mut state = ElementEdit::default();
    assert_eq!(state.shown_frame(Some("a")), None);
    let request = state.begin("a", &report, 10).unwrap();
    assert!(state.finish(request, Some("a"), resolve_in(TRANSFORM_FIXTURE, &report)));
    assert_eq!(state.shown_frame(Some("a")), Some(10));
    // A seek's lookup answered (frame 20) but is not resolved yet: the rows
    // still show frame 10's targets, so that is the commit's frame.
    state.begin("a", &report, 20).unwrap();
    assert_eq!(state.shown_frame(Some("a")), Some(10));
    assert_eq!(state.shown_frame(Some("b")), None);
}

#[test]
fn a_failed_reload_fails_commits_instead_of_holding_them() {
    let report = primitive_report(2, 3, x_only());
    let mut state = ElementEdit::default();
    let request = state.begin("a", &report, 0).unwrap();
    assert!(state.finish(request, Some("a"), resolve_in(TRANSFORM_FIXTURE, &report)));
    state.invalidate();
    assert!(!state.failed(Some("a")));
    state.reload_failed();
    assert!(state.failed(Some("a")));
    assert_eq!(
        state.unavailable(Some("a")),
        "Not saved: the code failed to reload"
    );
    // The next reload reads the code again.
    state.expire();
    assert!(!state.failed(Some("a")));
}

#[test]
fn a_stopped_preview_fails_commits_for_good() {
    let report = primitive_report(2, 3, x_only());
    let mut state = ElementEdit::default();
    let request = state.begin("a", &report, 0).unwrap();
    assert!(state.finish(request, Some("a"), resolve_in(TRANSFORM_FIXTURE, &report)));
    state.invalidate();
    // No lookup will answer again: held commits are dropped, and new ones
    // fail at once instead of waiting.
    state.preview_stopped();
    assert!(state.failed(Some("a")));
    assert_eq!(
        state.unavailable(Some("a")),
        "Not saved: the preview stopped unexpectedly"
    );
    // Neither a reload nor another selection brings the preview back.
    state.expire();
    state.reset();
    assert!(state.failed(Some("b")));
    assert!(state.preview_has_stopped());
}

#[test]
fn a_commit_waits_unless_the_current_resolution_failed() {
    let report = primitive_report(2, 3, x_only());
    let mut state = ElementEdit::default();
    // Still being read: the commit waits for it.
    assert!(!state.failed(Some("a")));
    let request = state.begin("a", &report, 0).unwrap();
    assert!(state.finish(request, Some("a"), resolve_in(TRANSFORM_FIXTURE, &report)));
    assert!(!state.failed(Some("a")));

    let request = state.begin("a", &report, 1).unwrap();
    assert!(state.finish(request, Some("a"), Err("Couldn't read".to_owned())));
    assert!(state.failed(Some("a")));
    assert!(!state.failed(Some("b")));
    // An own write's reload may read it again.
    state.invalidate();
    assert!(!state.failed(Some("a")));
}
