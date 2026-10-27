//! Which scenes an edit of the selected layer changes, from the occurrence
//! sweep (see [`crate::occurrences`]) and the layer's own report.

use crate::occurrences::OccurrenceState;
use crate::timeline_drag::{DragPreview, preview_occurrences};
use celesta_react_bridge::{
    InstanceOccurrences, LayerSourceReport, ReactChapter, SourcePos, outermost_chapter_at,
    scenes_of,
};

/// What an edit of the selected instance changes, in scenes. "Instances" are
/// the element's instances the edit changes: all of them, or those made by
/// the call site it writes (see [`impact_view`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Impact {
    /// Outermost chapters this instance's occurrences overlap (names).
    pub appears_in: Vec<String>,
    /// The ids of `appears_in`, in the same order.
    pub appears_in_ids: Vec<String>,
    /// This instance's ranges, for "Appears in" without chapters.
    pub own_ranges: Vec<(u64, u64)>,
    /// Outermost chapters any instance overlaps, minus the current one.
    pub other_scenes: Vec<String>,
    pub instance_count: usize,
    /// Copies in the current scene: Σ max_per_frame of instances overlapping it.
    pub places_in_scene: u32,
    /// Where each instance is called from, for the hover list: its chain
    /// entry at the first index where the element's instance chains differ,
    /// or its last entry when they never do (or it ends before that index),
    /// or the element when its chain is empty. One per instance, in instance
    /// order.
    pub call_sites: Vec<SourcePos>,
    /// Filled bands: this instance's ranges inside the current scene, each
    /// with its instance's index into `call_sites`.
    pub own_bands: Vec<((u64, u64), usize)>,
    /// Outlined bands: every other occurrence of the element, each with its
    /// instance's index into `call_sites`.
    pub other_bands: Vec<((u64, u64), usize)>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ImpactView {
    /// No index yet (scanning, failed, or the element is not in it): the report's span only.
    SpanOnly {
        span: Option<(f64, f64)>,
        note: String,
    },
    Known(Impact),
}

/// Which of the element's instances an edit changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ImpactFilter {
    All,
    /// Those whose chain contains this call: the call site the edit writes.
    Through(SourcePos),
    /// Those whose chain contains none of these calls: the shared value,
    /// which the calls that set their own value keep out.
    Except(Vec<SourcePos>),
}

impl ImpactFilter {
    pub(crate) fn changes(&self, instance: &InstanceOccurrences) -> bool {
        match self {
            Self::All => true,
            Self::Through(site) => instance.chain.contains(site),
            Self::Except(calls) => !calls.iter().any(|call| instance.chain.contains(call)),
        }
    }
}

/// The impact of editing the layer `report` describes, with the playhead at
/// `frame`. The current scene is the outermost chapter at `frame`; without
/// chapters of any length the whole timeline is one scene, and in a gap
/// between chapters no occurrence is in the current scene. Only the
/// instances `filter` keeps change, so only those are counted, listed, and
/// banded.
pub(crate) fn impact_view(
    report: &LayerSourceReport,
    occurrences: Option<&OccurrenceState>,
    chapters: &[ReactChapter],
    frame: u64,
    filter: &ImpactFilter,
) -> ImpactView {
    moved_impact_view(report, occurrences, chapters, frame, filter, None)
}

/// [`impact_view`] with the element's instances where `preview`, a running
/// band drag, moves them (see [`preview_occurrences`]).
pub(crate) fn moved_impact_view(
    report: &LayerSourceReport,
    occurrences: Option<&OccurrenceState>,
    chapters: &[ReactChapter],
    frame: u64,
    filter: &ImpactFilter,
    preview: Option<&DragPreview>,
) -> ImpactView {
    let span_only = |note: String| ImpactView::SpanOnly {
        // `to <= from` when the layer's windows start past the composition end.
        span: report
            .span
            .as_ref()
            .filter(|span| span.from < span.to)
            .map(|span| (span.from, span.to)),
        note,
    };
    let index = match occurrences {
        None | Some(OccurrenceState::Scanning) => return span_only("Scanning scenes…".to_owned()),
        Some(OccurrenceState::Failed(message)) => {
            return span_only(format!("Couldn't scan scenes: {message}"));
        }
        Some(OccurrenceState::Ready(index) | OccurrenceState::Refreshing(index)) => index,
    };
    let Some((element, own)) = index.instance_for(report) else {
        return span_only("Not found in the scene scan".to_owned());
    };
    let moved;
    let (element, own) = match preview {
        None => (element, own),
        Some(preview) => {
            moved = preview_occurrences(element, preview);
            // The instance key is the chain, which a preview keeps.
            let Some(own) = moved
                .instances
                .iter()
                .find(|instance| instance.chain == own.chain)
            else {
                return span_only("Not found in the scene scan".to_owned());
            };
            (&moved, own)
        }
    };
    let instances: Vec<&InstanceOccurrences> = element
        .instances
        .iter()
        .filter(|instance| filter.changes(instance))
        .collect();

    let current = outermost_chapter_at(chapters, frame);
    let no_scenes = chapters
        .iter()
        .all(|chapter| chapter.duration_in_frames == 0);
    let scene = match current {
        Some(chapter) => Some((chapter.from, chapter.end())),
        None if no_scenes => Some((0, u64::MAX)),
        None => None,
    };
    let in_scene = |instance: &InstanceOccurrences| {
        scene.is_some_and(|scene| instance.ranges.iter().any(|&range| overlaps(range, scene)))
    };

    let appears = scenes_of(&own.ranges, chapters);
    let all_ranges: Vec<(u64, u64)> = instances
        .iter()
        .flat_map(|instance| instance.ranges.iter().copied())
        .collect();
    let other_scenes = scenes_of(&all_ranges, chapters)
        .into_iter()
        .filter(|chapter| current.is_none_or(|current| current.id != chapter.id))
        .map(|chapter| chapter.name.clone())
        .collect();

    let mut own_bands = Vec::new();
    let mut other_bands = Vec::new();
    for (index, instance) in instances.iter().enumerate() {
        for &range in &instance.ranges {
            match scene {
                Some(scene) if std::ptr::eq(*instance, own) => {
                    cut(range, scene, index, &mut own_bands, &mut other_bands);
                }
                _ => other_bands.push((range, index)),
            }
        }
    }

    ImpactView::Known(Impact {
        appears_in: appears.iter().map(|chapter| chapter.name.clone()).collect(),
        appears_in_ids: appears.iter().map(|chapter| chapter.id.clone()).collect(),
        own_ranges: own.ranges.clone(),
        other_scenes,
        instance_count: instances.len(),
        places_in_scene: instances
            .iter()
            .filter(|instance| in_scene(instance))
            .map(|instance| instance.max_per_frame)
            .sum(),
        call_sites: call_sites(&element.element, &instances),
        own_bands,
        other_bands,
    })
}

/// See [`Impact::call_sites`]. The entries the chains share are the element
/// and whatever its component wraps it in (a `<Group>` inside `Card`), the
/// same for every instance; the first entry that differs is the call. An
/// instance with an empty chain is called from `element`, so the bands'
/// indices still line up with the instances.
fn call_sites(element: &SourcePos, instances: &[&InstanceOccurrences]) -> Vec<SourcePos> {
    let Some(first) = instances.first() else {
        return Vec::new();
    };
    let shared = instances
        .iter()
        .map(|instance| {
            first
                .chain
                .iter()
                .zip(&instance.chain)
                .take_while(|(a, b)| a == b)
                .count()
        })
        .min()
        .unwrap_or(0);
    instances
        .iter()
        .map(|instance| {
            instance
                .chain
                .get(shared)
                .or(instance.chain.last())
                .unwrap_or(element)
                .clone()
        })
        .collect()
}

/// Whether `[from, to)` frame ranges `a` and `b` share a frame.
fn overlaps(a: (u64, u64), b: (u64, u64)) -> bool {
    a.0 < b.1 && b.0 < a.1
}

/// Cuts `range` of instance `index` at `scene`'s edges: the part inside it
/// goes to `inside`, the parts before and after it to `outside`.
fn cut(
    (from, to): (u64, u64),
    (start, end): (u64, u64),
    index: usize,
    inside: &mut Vec<((u64, u64), usize)>,
    outside: &mut Vec<((u64, u64), usize)>,
) {
    let push = |bands: &mut Vec<((u64, u64), usize)>, from: u64, to: u64| {
        if from < to {
            bands.push(((from, to), index));
        }
    };
    push(outside, from, to.min(start));
    push(inside, from.max(start), to.min(end));
    push(outside, from.max(end), to);
}

/// More other scenes than this are counted instead of named.
const NAMED_SCENES: usize = 3;

/// The other scenes as the Shared row and the impact line name them:
/// "Intro, Outro" up to [`NAMED_SCENES`], "11 other scenes" past that.
pub(crate) fn other_scenes_label(scenes: &[String]) -> String {
    if scenes.len() > NAMED_SCENES {
        format!("{} other scenes", scenes.len())
    } else {
        scenes.join(", ")
    }
}

/// Every other scene's name, for a hover over a label that only counts
/// them; `None` when [`other_scenes_label`] names them all.
pub(crate) fn counted_scenes(scenes: &[String]) -> Option<String> {
    (scenes.len() > NAMED_SCENES).then(|| scenes.join(", "))
}

/// The impact line's text and whether it uses the warning color.
pub(crate) fn impact_line(view: &ImpactView) -> (String, bool) {
    match view {
        ImpactView::SpanOnly { note, .. } => (note.clone(), false),
        ImpactView::Known(impact) if !impact.other_scenes.is_empty() => (
            format!("Also changes {}", other_scenes_label(&impact.other_scenes)),
            true,
        ),
        ImpactView::Known(impact) if impact.places_in_scene > 1 => (
            format!("Changes {} places in this scene", impact.places_in_scene),
            false,
        ),
        ImpactView::Known(_) => ("Changes this scene only".to_owned(), false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_react_bridge::{
        ElementKind, ElementOccurrences, FrameSpan, LayerSource, OccurrenceIndex, ReceivedProps,
    };

    fn pos(line: u32) -> SourcePos {
        SourcePos {
            file: "/a/title.tsx".into(),
            line,
            column: 4,
        }
    }

    fn chapter(id: &str, name: &str, from: u64, duration_in_frames: u64) -> ReactChapter {
        ReactChapter {
            id: id.to_owned(),
            name: name.to_owned(),
            from,
            duration_in_frames,
        }
    }

    fn instance(chain: Vec<SourcePos>, ranges: Vec<(u64, u64)>) -> InstanceOccurrences {
        InstanceOccurrences {
            chain,
            ranges,
            max_per_frame: 1,
            wrappers: vec![],
        }
    }

    /// An index holding one element, `instances[0].chain[0]`.
    fn index(instances: Vec<InstanceOccurrences>) -> OccurrenceState {
        OccurrenceState::Ready(OccurrenceIndex {
            elements: vec![ElementOccurrences {
                element: instances[0].chain[0].clone(),
                instances,
            }],
        })
    }

    /// A report for a `<Rect>` at `chain[0]` under components at the rest
    /// of `chain`, inside a `<Sequence>`.
    fn report(chain: &[SourcePos], span: Option<(f64, f64)>) -> LayerSourceReport {
        let entry = |name: &str, at: &SourcePos, kind| LayerSource {
            name: name.to_owned(),
            file: at.file.clone(),
            line: at.line,
            column: at.column,
            kind,
            layer_id: None,
        };
        let mut entries = vec![entry("Rect", &chain[0], ElementKind::Primitive)];
        entries.push(entry("Sequence", &pos(1), ElementKind::Wrapper));
        entries.extend(
            chain[1..]
                .iter()
                .map(|at| entry("Card", at, ElementKind::Component)),
        );
        LayerSourceReport {
            chain: entries,
            local_frame: 0.0,
            candidate_frames: vec![],
            props: ReceivedProps::default(),
            render_count: 1,
            windows: vec![],
            span: span.map(|(from, to)| FrameSpan { from, to }),
        }
    }

    fn known(view: ImpactView) -> Impact {
        match view {
            ImpactView::Known(impact) => impact,
            other => panic!("expected a known impact, got {other:?}"),
        }
    }

    fn span_only(span: Option<(f64, f64)>, note: &str) -> ImpactView {
        ImpactView::SpanOnly {
            span,
            note: note.to_owned(),
        }
    }

    #[test]
    fn shared_across_scenes_names_the_others() {
        let chapters = [
            chapter("intro", "Intro", 0, 30),
            chapter("price", "Price", 30, 60),
        ];
        let occurrences = index(vec![
            instance(vec![pos(3), pos(8)], vec![(0, 30)]),
            instance(vec![pos(3), pos(20)], vec![(30, 90)]),
        ]);
        let view = impact_view(
            &report(&[pos(3), pos(20)], Some((30.0, 90.0))),
            Some(&occurrences),
            &chapters,
            40,
            &ImpactFilter::All,
        );
        let line = impact_line(&view);
        let impact = known(view);
        assert_eq!(impact.appears_in, vec!["Price"]);
        assert_eq!(impact.appears_in_ids, vec!["price"]);
        assert_eq!(impact.other_scenes, vec!["Intro"]);
        assert_eq!(impact.instance_count, 2);
        assert_eq!(impact.places_in_scene, 1);
        assert_eq!(impact.call_sites, vec![pos(8), pos(20)]);
        assert_eq!(line, ("Also changes Intro".to_owned(), true));
    }

    #[test]
    fn three_other_scenes_are_named_and_more_are_counted() {
        let names = |count: usize| -> Vec<String> {
            (1..=count).map(|scene| format!("S{scene}")).collect()
        };
        assert_eq!(other_scenes_label(&names(3)), "S1, S2, S3");
        assert_eq!(counted_scenes(&names(3)), None);
        assert_eq!(other_scenes_label(&names(4)), "4 other scenes");
        assert_eq!(counted_scenes(&names(4)).as_deref(), Some("S1, S2, S3, S4"));

        let chapters: Vec<ReactChapter> = (0..12)
            .map(|scene| chapter(&format!("s{scene}"), &format!("S{scene}"), scene * 30, 30))
            .collect();
        let occurrences = index(vec![instance(vec![pos(3)], vec![(0, 360)])]);
        let view = impact_view(
            &report(&[pos(3)], Some((0.0, 360.0))),
            Some(&occurrences),
            &chapters,
            10,
            &ImpactFilter::All,
        );
        assert_eq!(
            impact_line(&view),
            ("Also changes 11 other scenes".to_owned(), true)
        );
    }

    #[test]
    fn impact_scoped_to_a_call_site() {
        // `<Card>` is called at 8 (in Intro) and 20 (in Price); the edit
        // writes the call at 20.
        let chapters = [
            chapter("intro", "Intro", 0, 30),
            chapter("price", "Price", 30, 60),
        ];
        let occurrences = index(vec![
            instance(vec![pos(3), pos(8)], vec![(0, 30)]),
            instance(vec![pos(3), pos(20)], vec![(30, 90)]),
        ]);
        let view = impact_view(
            &report(&[pos(3), pos(20)], Some((30.0, 90.0))),
            Some(&occurrences),
            &chapters,
            40,
            &ImpactFilter::Through(pos(20)),
        );
        let line = impact_line(&view);
        let impact = known(view);
        assert_eq!(impact.other_scenes, Vec::<String>::new());
        assert_eq!(impact.instance_count, 1);
        assert_eq!(impact.places_in_scene, 1);
        assert_eq!(impact.call_sites, vec![pos(20)]);
        assert_eq!(impact.own_bands, vec![((30, 90), 0)]);
        assert!(impact.other_bands.is_empty());
        assert_eq!(line, ("Changes this scene only".to_owned(), false));
    }

    #[test]
    fn a_call_site_inside_a_reused_component_counts_every_instance_through_it() {
        // `<Card>` is called at 20 inside a component that Intro calls at 30
        // and Outro at 40; Price calls `<Card>` at 8. The edit writes 20.
        let chapters = [
            chapter("intro", "Intro", 0, 30),
            chapter("outro", "Outro", 30, 30),
            chapter("price", "Price", 60, 30),
        ];
        let occurrences = index(vec![
            instance(vec![pos(3), pos(20), pos(30)], vec![(0, 30)]),
            instance(vec![pos(3), pos(20), pos(40)], vec![(30, 60)]),
            instance(vec![pos(3), pos(8)], vec![(60, 90)]),
        ]);
        let view = impact_view(
            &report(&[pos(3), pos(20), pos(30)], Some((0.0, 30.0))),
            Some(&occurrences),
            &chapters,
            10,
            &ImpactFilter::Through(pos(20)),
        );
        let line = impact_line(&view);
        let impact = known(view);
        assert_eq!(impact.other_scenes, vec!["Outro"]);
        assert_eq!(impact.instance_count, 2);
        assert_eq!(impact.call_sites, vec![pos(30), pos(40)]);
        assert_eq!(impact.own_bands, vec![((0, 30), 0)]);
        assert_eq!(impact.other_bands, vec![((30, 60), 1)]);
        assert_eq!(line, ("Also changes Outro".to_owned(), true));
    }

    #[test]
    fn impact_view_filters_through_and_except() {
        // `<Card>` is called at 8 (Intro), 20 (Price) and 30 (Outro); the
        // playhead is in Price.
        let chapters = [
            chapter("intro", "Intro", 0, 30),
            chapter("price", "Price", 30, 30),
            chapter("outro", "Outro", 60, 30),
        ];
        let occurrences = index(vec![
            instance(vec![pos(3), pos(8)], vec![(0, 30)]),
            instance(vec![pos(3), pos(20)], vec![(30, 60)]),
            instance(vec![pos(3), pos(30)], vec![(60, 90)]),
        ]);
        let report = report(&[pos(3), pos(20)], Some((30.0, 60.0)));
        let view = |filter: &ImpactFilter| {
            known(impact_view(
                &report,
                Some(&occurrences),
                &chapters,
                40,
                filter,
            ))
        };

        let all = view(&ImpactFilter::All);
        assert_eq!(all.instance_count, 3);
        assert_eq!(all.other_scenes, vec!["Intro", "Outro"]);

        let through = view(&ImpactFilter::Through(pos(20)));
        assert_eq!(through.instance_count, 1);
        assert!(through.other_scenes.is_empty());
        assert_eq!(through.own_bands, vec![((30, 60), 0)]);
        assert!(through.other_bands.is_empty());

        // Price sets its own: the shared value reaches Intro and Outro.
        let except = view(&ImpactFilter::Except(vec![pos(20)]));
        assert_eq!(except.instance_count, 2);
        assert_eq!(except.other_scenes, vec!["Intro", "Outro"]);
        assert_eq!(except.call_sites, vec![pos(8), pos(30)]);
        assert!(except.own_bands.is_empty());
        assert_eq!(except.other_bands, vec![((0, 30), 0), ((60, 90), 1)]);
        // This scene's own range is still where it appears.
        assert_eq!(except.appears_in, vec!["Price"]);

        assert_eq!(view(&ImpactFilter::Except(Vec::new())), all);
    }

    #[test]
    fn one_scene_component_reused_by_scenes_is_shared() {
        let chapters = [chapter("a", "A", 0, 30), chapter("b", "B", 30, 30)];
        let occurrences = index(vec![instance(vec![pos(3)], vec![(0, 20), (40, 60)])]);
        let impact = known(impact_view(
            &report(&[pos(3)], Some((0.0, 60.0))),
            Some(&occurrences),
            &chapters,
            10,
            &ImpactFilter::All,
        ));
        assert_eq!(impact.appears_in, vec!["A", "B"]);
        assert_eq!(impact.other_scenes, vec!["B"]);
        assert_eq!(impact.call_sites, vec![pos(3)]);
    }

    #[test]
    fn call_sites_name_where_instances_differ() {
        // `pos(5)` is a `<Group>` inside Card; 8 and 20 are the two `<Card/>` calls.
        let occurrences = index(vec![
            instance(vec![pos(3), pos(5), pos(8)], vec![(0, 30)]),
            instance(vec![pos(3), pos(5), pos(20)], vec![(30, 90)]),
        ]);
        let impact = known(impact_view(
            &report(&[pos(3), pos(5), pos(20)], Some((30.0, 90.0))),
            Some(&occurrences),
            &[],
            40,
            &ImpactFilter::All,
        ));
        assert_eq!(impact.call_sites, vec![pos(8), pos(20)]);
    }

    #[test]
    fn call_sites_for_one_instance_use_the_outermost_entry() {
        let occurrences = index(vec![instance(vec![pos(3), pos(5), pos(8)], vec![(0, 30)])]);
        let impact = known(impact_view(
            &report(&[pos(3), pos(5), pos(8)], Some((0.0, 30.0))),
            Some(&occurrences),
            &[],
            10,
            &ImpactFilter::All,
        ));
        assert_eq!(impact.call_sites, vec![pos(8)]);
    }

    #[test]
    fn an_instance_without_a_chain_keeps_its_call_site_slot() {
        let occurrences = OccurrenceState::Ready(OccurrenceIndex {
            elements: vec![ElementOccurrences {
                element: pos(3),
                instances: vec![
                    instance(vec![pos(3), pos(8)], vec![(0, 30)]),
                    instance(vec![], vec![(40, 60)]),
                    instance(vec![pos(3), pos(20)], vec![(70, 90)]),
                ],
            }],
        });
        let impact = known(impact_view(
            &report(&[pos(3), pos(20)], Some((70.0, 90.0))),
            Some(&occurrences),
            &[],
            80,
            &ImpactFilter::All,
        ));
        // The empty chain shares nothing, so every call site is the element.
        assert_eq!(impact.call_sites, vec![pos(3), pos(3), pos(3)]);
        assert_eq!(impact.own_bands, vec![((70, 90), 2)]);
        assert_eq!(impact.other_bands, vec![((0, 30), 0), ((40, 60), 1)]);
    }

    #[test]
    fn map_copies_count_places_in_this_scene() {
        let chapters = [chapter("a", "A", 0, 30)];
        let occurrences = index(vec![InstanceOccurrences {
            chain: vec![pos(3)],
            ranges: vec![(0, 30)],
            max_per_frame: 3,
            wrappers: vec![],
        }]);
        let view = impact_view(
            &report(&[pos(3)], Some((0.0, 30.0))),
            Some(&occurrences),
            &chapters,
            10,
            &ImpactFilter::All,
        );
        assert_eq!(
            impact_line(&view),
            ("Changes 3 places in this scene".to_owned(), false)
        );
    }

    #[test]
    fn without_chapters_nothing_is_shared() {
        let occurrences = index(vec![
            instance(vec![pos(3), pos(8)], vec![(15, 35)]),
            instance(vec![pos(3), pos(20)], vec![(60, 90)]),
        ]);
        let view = impact_view(
            &report(&[pos(3), pos(8)], Some((15.0, 35.0))),
            Some(&occurrences),
            &[],
            20,
            &ImpactFilter::All,
        );
        let line = impact_line(&view);
        let impact = known(view);
        assert!(impact.appears_in.is_empty());
        assert_eq!(impact.own_ranges, vec![(15, 35)]);
        assert!(impact.other_scenes.is_empty());
        assert_eq!(impact.own_bands, vec![((15, 35), 0)]);
        assert_eq!(impact.other_bands, vec![((60, 90), 1)]);
        assert_eq!(line, ("Changes 2 places in this scene".to_owned(), false));

        let alone = index(vec![instance(vec![pos(3)], vec![(15, 35)])]);
        let view = impact_view(
            &report(&[pos(3)], Some((15.0, 35.0))),
            Some(&alone),
            &[],
            20,
            &ImpactFilter::All,
        );
        assert_eq!(
            impact_line(&view),
            ("Changes this scene only".to_owned(), false)
        );
    }

    #[test]
    fn marks_alone_are_not_scenes() {
        let chapters = [chapter("drop", "Drop", 20, 0)];
        let occurrences = index(vec![instance(vec![pos(3)], vec![(15, 35)])]);
        let impact = known(impact_view(
            &report(&[pos(3)], Some((15.0, 35.0))),
            Some(&occurrences),
            &chapters,
            20,
            &ImpactFilter::All,
        ));
        assert!(impact.appears_in.is_empty());
        assert_eq!(impact.places_in_scene, 1);
        assert_eq!(impact.own_bands, vec![((15, 35), 0)]);
    }

    #[test]
    fn missing_from_the_index_falls_back_to_the_span() {
        let occurrences = index(vec![instance(vec![pos(9)], vec![(0, 30)])]);
        let view = impact_view(
            &report(&[pos(3)], Some((15.0, 35.0))),
            Some(&occurrences),
            &[],
            20,
            &ImpactFilter::All,
        );
        assert_eq!(
            view,
            span_only(Some((15.0, 35.0)), "Not found in the scene scan")
        );
        assert_eq!(
            impact_line(&view),
            ("Not found in the scene scan".to_owned(), false)
        );
    }

    #[test]
    fn the_last_scan_is_read_while_refreshing() {
        let chapters = [chapter("a", "A", 0, 30), chapter("b", "B", 30, 30)];
        let last = OccurrenceIndex {
            elements: vec![ElementOccurrences {
                element: pos(3),
                instances: vec![instance(vec![pos(3)], vec![(0, 20), (40, 60)])],
            }],
        };
        let report = report(&[pos(3)], Some((0.0, 60.0)));
        let refreshing = impact_view(
            &report,
            Some(&OccurrenceState::Refreshing(last.clone())),
            &chapters,
            10,
            &ImpactFilter::All,
        );
        let ready = impact_view(
            &report,
            Some(&OccurrenceState::Ready(last)),
            &chapters,
            10,
            &ImpactFilter::All,
        );
        assert_eq!(refreshing, ready);
        assert_eq!(known(refreshing).other_scenes, vec!["B"]);
    }

    #[test]
    fn scanning_and_failed_notes() {
        let report = report(&[pos(3)], Some((15.0, 35.0)));
        assert_eq!(
            impact_view(
                &report,
                Some(&OccurrenceState::Scanning),
                &[],
                20,
                &ImpactFilter::All
            ),
            span_only(Some((15.0, 35.0)), "Scanning scenes…")
        );
        assert_eq!(
            impact_view(&report, None, &[], 20, &ImpactFilter::All),
            span_only(Some((15.0, 35.0)), "Scanning scenes…")
        );
        assert_eq!(
            impact_view(
                &report,
                Some(&OccurrenceState::Failed("boom".to_owned())),
                &[],
                20,
                &ImpactFilter::All
            ),
            span_only(Some((15.0, 35.0)), "Couldn't scan scenes: boom")
        );
    }

    #[test]
    fn a_span_past_the_composition_end_is_no_span() {
        let view = impact_view(
            &report(&[pos(3)], Some((120.0, 90.0))),
            None,
            &[],
            20,
            &ImpactFilter::All,
        );
        assert_eq!(view, span_only(None, "Scanning scenes…"));
        let view = impact_view(
            &report(&[pos(3)], Some((90.0, 90.0))),
            None,
            &[],
            20,
            &ImpactFilter::All,
        );
        assert_eq!(view, span_only(None, "Scanning scenes…"));
    }

    #[test]
    fn bands_split_own_scene_from_the_rest() {
        let chapters = [chapter("a", "A", 0, 30), chapter("b", "B", 30, 90)];
        let occurrences = index(vec![
            instance(vec![pos(3), pos(8)], vec![(0, 30), (60, 90)]),
            instance(vec![pos(3), pos(20)], vec![(100, 120)]),
        ]);
        let impact = known(impact_view(
            &report(&[pos(3), pos(8)], Some((0.0, 90.0))),
            Some(&occurrences),
            &chapters,
            10,
            &ImpactFilter::All,
        ));
        assert_eq!(impact.own_bands, vec![((0, 30), 0)]);
        assert_eq!(impact.other_bands, vec![((60, 90), 0), ((100, 120), 1)]);
    }

    #[test]
    fn a_range_across_the_scene_edge_is_cut_there() {
        let chapters = [chapter("a", "A", 0, 30), chapter("b", "B", 30, 30)];
        let occurrences = index(vec![instance(vec![pos(3)], vec![(20, 50)])]);
        let impact = known(impact_view(
            &report(&[pos(3)], Some((20.0, 50.0))),
            Some(&occurrences),
            &chapters,
            40,
            &ImpactFilter::All,
        ));
        assert_eq!(impact.own_bands, vec![((30, 50), 0)]);
        assert_eq!(impact.other_bands, vec![((20, 30), 0)]);
    }

    #[test]
    fn a_band_drag_moves_the_bands_through_its_window() {
        use crate::timeline_drag::{BandGrip, DragPreview};

        // The `<Sequence>` the report puts at `pos(1)` holds the first
        // instance; the second renders elsewhere.
        let chapters = [chapter("a", "A", 0, 120)];
        let occurrences = index(vec![
            InstanceOccurrences {
                wrappers: vec![pos(1)],
                ..instance(vec![pos(3), pos(8)], vec![(30, 90)])
            },
            instance(vec![pos(3), pos(20)], vec![(200, 260)]),
        ]);
        let preview = DragPreview {
            source: pos(1),
            window: (30.0, 90.0),
            grip: BandGrip::End,
            delta: 15,
            followers: vec![],
        };
        let impact = known(moved_impact_view(
            &report(&[pos(3), pos(8)], Some((30.0, 90.0))),
            Some(&occurrences),
            &chapters,
            40,
            &ImpactFilter::All,
            Some(&preview),
        ));
        assert_eq!(impact.own_ranges, vec![(30, 105)]);
        assert_eq!(impact.own_bands, vec![((30, 105), 0)]);
        assert_eq!(impact.other_bands, vec![((200, 260), 1)]);
    }

    #[test]
    fn in_a_gap_between_scenes_nothing_is_in_this_scene() {
        let chapters = [chapter("a", "A", 0, 30), chapter("b", "B", 60, 30)];
        let occurrences = index(vec![instance(vec![pos(3)], vec![(20, 70)])]);
        let impact = known(impact_view(
            &report(&[pos(3)], Some((20.0, 70.0))),
            Some(&occurrences),
            &chapters,
            45,
            &ImpactFilter::All,
        ));
        assert_eq!(impact.other_scenes, vec!["A", "B"]);
        assert_eq!(impact.places_in_scene, 0);
        assert!(impact.own_bands.is_empty());
        assert_eq!(impact.other_bands, vec![((20, 70), 0)]);
    }
}
