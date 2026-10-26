//! Whose timing the window nearest an element is: the element's own (a
//! `<Sequence>` written around it), its scene's, or a loop's or freeze
//! frame's; and the window "Set timing" writes when it has none of its own.

use celesta_react_bridge::{LayerSourceReport, ReactChapter, TimingWindow, outermost_chapter_at};

/// What `report.windows[0]`, the window nearest the element, is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum WindowRole<'a> {
    /// A wrapper written around the element: its timing is the element's own.
    Own(&'a TimingWindow),
    /// A scene's: a `<Scenes>` scene, or a wrapper whose span is a chapter's.
    Scene(&'a TimingWindow),
    /// Inside a `<Loop>` or `<FreezeFrame>`, whose frames can't be dragged.
    Remapped,
    /// No windows: the element is on the composition's own timeline.
    None,
}

/// The role of `report.windows[0]` at the lookup `frame`. A window is the
/// scene's when it has no source, or when its span, cut at
/// `composition_end` and rounded to whole frames, is the span of a chapter
/// holding `frame` (a part or the scene around it).
pub(crate) fn window_role<'a>(
    report: &'a LayerSourceReport,
    chapters: &[ReactChapter],
    composition_end: u64,
    frame: u64,
) -> WindowRole<'a> {
    let Some(window) = report.windows.first() else {
        return WindowRole::None;
    };
    if window.remapped {
        return WindowRole::Remapped;
    }
    if window.source.is_none() {
        return WindowRole::Scene(window);
    }
    let span = whole_frames(window, composition_end);
    let spans_a_chapter = chapters.iter().any(|chapter| {
        chapter.from <= frame
            && frame < chapter.end()
            && (chapter.from as i64, chapter.end() as i64) == span
    });
    if spans_a_chapter {
        WindowRole::Scene(window)
    } else {
        WindowRole::Own(window)
    }
}

/// `window`'s span cut at `composition_end`, rounded to whole frames. A
/// `<Sequence>` may start before frame 0, so `from` can be negative.
fn whole_frames(window: &TimingWindow, composition_end: u64) -> (i64, i64) {
    let to = window.to.min(composition_end as f64);
    (window.from.round() as i64, to.round() as i64)
}

/// `from` and `durationInFrames` for "Set timing" at `playhead`: from the
/// playhead to the end of the scene's window, or, without a window, on the
/// composition's timeline to the end of the scene at the playhead (the
/// composition's end outside one). `None` for `Own` and `Remapped`, and when
/// less than a frame is left.
#[allow(dead_code)]
pub(crate) fn set_timing_span(
    role: &WindowRole,
    chapters: &[ReactChapter],
    composition_end: u64,
    playhead: u64,
) -> Option<(i64, i64)> {
    let (from, duration) = match role {
        WindowRole::Scene(window) => {
            let (from, to) = whole_frames(window, composition_end);
            (playhead as i64 - from, to - playhead as i64)
        }
        WindowRole::None => {
            let end =
                outermost_chapter_at(chapters, playhead).map_or(composition_end, ReactChapter::end);
            (playhead as i64, end as i64 - playhead as i64)
        }
        WindowRole::Own(_) | WindowRole::Remapped => return None,
    };
    (duration >= 1).then_some((from, duration))
}

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_react_bridge::{ReceivedProps, SourcePos};

    fn chapter(id: &str, from: u64, duration_in_frames: u64) -> ReactChapter {
        ReactChapter {
            id: id.to_owned(),
            name: id.to_owned(),
            from,
            duration_in_frames,
        }
    }

    fn window(from: f64, to: f64, source: bool, remapped: bool) -> TimingWindow {
        TimingWindow {
            kind: "Sequence".to_owned(),
            from,
            to,
            source: source.then(|| SourcePos {
                file: "/a/title.tsx".into(),
                line: 5,
                column: 4,
            }),
            remapped,
        }
    }

    /// A report whose nearest window is `windows[0]`.
    fn report(windows: Vec<TimingWindow>) -> LayerSourceReport {
        LayerSourceReport {
            chain: vec![],
            local_frame: 0.0,
            candidate_frames: vec![],
            props: ReceivedProps::default(),
            render_count: 1,
            windows,
            span: None,
        }
    }

    fn is_scene(role: WindowRole) -> bool {
        matches!(role, WindowRole::Scene(_))
    }

    fn is_own(role: WindowRole) -> bool {
        matches!(role, WindowRole::Own(_))
    }

    #[test]
    fn no_windows_is_none() {
        let report = report(vec![]);
        assert_eq!(
            window_role(&report, &[chapter("a", 0, 60)], 60, 10),
            WindowRole::None
        );
    }

    #[test]
    fn a_remapped_window_is_locked() {
        let chapters = [chapter("a", 0, 60)];
        for source in [true, false] {
            let report = report(vec![window(0.0, 60.0, source, true)]);
            assert_eq!(
                window_role(&report, &chapters, 60, 10),
                WindowRole::Remapped
            );
        }
    }

    #[test]
    fn a_window_without_source_is_the_scene_s() {
        let report = report(vec![window(30.0, 50.0, false, false)]);
        assert_eq!(
            window_role(&report, &[chapter("a", 0, 60)], 60, 40),
            WindowRole::Scene(&report.windows[0])
        );
    }

    #[test]
    fn a_window_spanning_a_chapter_is_the_scene_s() {
        let scene_only = [chapter("intro", 300, 120)];
        let report_at = |from, to| report(vec![window(from, to, true, false)]);

        let whole = report_at(300.0, 420.0);
        assert!(is_scene(window_role(&whole, &scene_only, 960, 345)));

        // A part inside the scene holds the frame: both the part's and the
        // scene's span match.
        let with_part = [chapter("intro", 300, 120), chapter("intro-title", 330, 60)];
        assert!(is_scene(window_role(&whole, &with_part, 960, 345)));
        let part = report_at(330.0, 390.0);
        assert!(is_scene(window_role(&part, &with_part, 960, 345)));

        // The last chapter, cut at the composition's end.
        let last = report_at(900.0, 1020.0);
        assert!(is_scene(window_role(
            &last,
            &[chapter("outro", 900, 60)],
            960,
            930
        )));

        // Fractional edges round to whole frames.
        let fractional = report_at(299.6, 420.2);
        assert!(is_scene(window_role(&fractional, &scene_only, 960, 345)));
    }

    #[test]
    fn a_hand_placed_sequence_inside_a_chapter_is_own() {
        let inside = report(vec![window(330.0, 360.0, true, false)]);
        let role = window_role(&inside, &[chapter("intro", 300, 120)], 960, 345);
        assert!(is_own(role));
        assert_eq!(role, WindowRole::Own(&inside.windows[0]));

        // A window starting before frame 0 never spans a chapter, which
        // starts at 0 at the earliest.
        let early = report(vec![window(-10.0, 60.0, true, false)]);
        assert!(is_own(window_role(
            &early,
            &[chapter("intro", 0, 60)],
            960,
            30
        )));
    }

    #[test]
    fn set_timing_span_runs_from_the_playhead_to_the_window_end() {
        let window_at = |from, to| window(from, to, true, false);
        let chapters = [chapter("intro", 300, 120)];
        let scene = window_at(300.0, 420.0);
        assert_eq!(
            set_timing_span(&WindowRole::Scene(&scene), &chapters, 960, 345),
            Some((45, 75))
        );

        // A window starting before frame 0.
        let early = window_at(-10.0, 60.0);
        assert_eq!(
            set_timing_span(&WindowRole::Scene(&early), &[], 960, 30),
            Some((40, 30))
        );

        // At the window's end nothing is left to show.
        assert_eq!(
            set_timing_span(&WindowRole::Scene(&scene), &chapters, 960, 420),
            None
        );
    }

    #[test]
    fn set_timing_span_without_a_window_is_on_the_composition_s_timeline() {
        let chapters = [chapter("intro", 300, 120), chapter("intro-title", 330, 30)];
        assert_eq!(
            set_timing_span(&WindowRole::None, &chapters, 960, 345),
            Some((345, 75))
        );
        assert_eq!(
            set_timing_span(&WindowRole::None, &[], 600, 345),
            Some((345, 255))
        );
        assert_eq!(set_timing_span(&WindowRole::None, &[], 600, 600), None);

        let own = window(330.0, 360.0, true, false);
        assert_eq!(
            set_timing_span(&WindowRole::Own(&own), &chapters, 960, 345),
            None
        );
        assert_eq!(
            set_timing_span(&WindowRole::Remapped, &chapters, 960, 345),
            None
        );
    }
}
