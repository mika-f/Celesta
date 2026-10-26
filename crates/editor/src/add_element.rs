//! Adding an element: the group it goes into (the selected one or the
//! nearest around it, moved into the scene's own code under "This scene"),
//! the `<Sequence>` it is wrapped in, the values a gesture on the canvas
//! writes and the outline drawn meanwhile, the Add menu's items, and the
//! add mode's steps.
#![cfg_attr(not(test), allow(dead_code))]

use crate::canvas_geometry::apply;
use crate::occurrences::OccurrenceState;
use crate::scope::{Scope, ScopeState, chosen_scope};
use crate::timing::{WindowRole, set_timing_span, window_role};
use celesta_composition::Point;
use celesta_gpu_renderer::invert_affine;
use celesta_react_bridge::{
    ElementKind, LayerSource, LayerSourceReport, ReactChapter, SourcePos, scenes_of,
};
use celesta_source_edit::{NewElement, NewKind, Timing};
use std::f64::consts::TAU;

/// The Add menu's kinds, in menu order.
const KINDS: [NewKind; 6] = [
    NewKind::Rect,
    NewKind::Ellipse,
    NewKind::Circle,
    NewKind::Text,
    NewKind::Image,
    NewKind::Video,
];

const NO_PARENT: &str = "Select a group, or a layer inside one, to add into";
const NO_PARENT_IN_SCENE: &str = "No group in this scene's own code to add into";
const REMAPPED: &str = "Can't add inside a loop or freeze frame";
/// Design 3's note while the occurrence index is not ready.
const SCANNING: &str = "Scanning scenes…";

/// A click's box side (Rect, Ellipse), in the parent's units.
const CLICK_SIDE: f64 = 200.0;
/// A click's Circle radius: the click box's circle.
const CLICK_RADIUS: f64 = 100.0;
/// A click's Image and Video width.
const CLICK_MEDIA_WIDTH: f64 = 400.0;
/// The points of a Circle's outline.
const CIRCLE_POINTS: usize = 32;

/// The parent an element is added to: a `Group` entry of the selected
/// layer's source chain.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AddParent {
    pub(crate) entry: LayerSource,
    /// Its index in the chain.
    pub(crate) index: usize,
    pub(crate) layer_id: String,
    /// Moved outward from a shared group by "This scene".
    pub(crate) this_scene: bool,
}

/// The scope an addition is made under.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ParentScope {
    Chosen(Scope),
    /// The selected element offers no scope control.
    NoControl,
    /// Why no scope can be chosen.
    Unknown(String),
}

/// The selected element's scope for an addition: `Options` →
/// `chosen_scope` (Err → `Unknown` with the lock's text), `NotShared` →
/// `NoControl`, `Unknown` → its note.
pub(crate) fn parent_scope(state: &ScopeState, remembered: Option<Scope>) -> ParentScope {
    match state {
        ScopeState::Options(options) => match chosen_scope(remembered, options) {
            Ok(scope) => ParentScope::Chosen(scope),
            Err(lock) => ParentScope::Unknown(lock.text()),
        },
        ScopeState::NotShared => ParentScope::NoControl,
        ScopeState::Unknown { note } => ParentScope::Unknown(note.clone()),
    }
}

/// Whether the element at `entry` is shared: the index's instance chains
/// through it (of every element) have more than one outer part (the chain
/// after it), or their ranges show in more than one scene. Err is design
/// 3's note when the index can't answer: while it scans, when it failed,
/// and when no chain goes through `entry` (a sweep under way after a write
/// finds it).
pub(crate) fn is_shared(
    occurrences: Option<&OccurrenceState>,
    chapters: &[ReactChapter],
    entry: &SourcePos,
) -> Result<bool, String> {
    let (index, refreshing) = match occurrences {
        None | Some(OccurrenceState::Scanning) => return Err(SCANNING.to_owned()),
        Some(OccurrenceState::Failed(message)) => {
            return Err(format!("Couldn't scan scenes: {message}"));
        }
        Some(OccurrenceState::Ready(index)) => (index, false),
        Some(OccurrenceState::Refreshing(index)) => (index, true),
    };
    let mut outer: Vec<&[SourcePos]> = Vec::new();
    let mut ranges = Vec::new();
    for instance in index.elements.iter().flat_map(|element| &element.instances) {
        let Some(at) = instance.chain.iter().position(|pos| pos == entry) else {
            continue;
        };
        outer.push(&instance.chain[at + 1..]);
        ranges.extend_from_slice(&instance.ranges);
    }
    let Some(first) = outer.first() else {
        let note = if refreshing {
            SCANNING
        } else {
            "Not found in the scene scan"
        };
        return Err(note.to_owned());
    };
    Ok(outer.iter().any(|part| part != first) || scenes_of(&ranges, chapters).len() > 1)
}

/// Whether the element at `entry` is written in the scene's own code:
/// [`is_shared`] is `Ok(false)`.
pub(crate) fn in_scene_code(
    occurrences: Option<&OccurrenceState>,
    chapters: &[ReactChapter],
    entry: &SourcePos,
) -> bool {
    is_shared(occurrences, chapters, entry) == Ok(false)
}

/// The parent of an element added with `report`'s layer selected (none
/// selected: `None`). (1) The selected layer, if it is a `Group`; (2)
/// otherwise the nearest `Group` in its chain. Either has a `layer_id` and
/// `accepts` (its JSX has a children region). A shared one (`shared`, see
/// [`is_shared`]) is the parent under "Shared value"; under "This scene",
/// and without a scope control, the next such group outward that is not
/// shared is; with an `Unknown` scope none is. Err is the menu's reason.
pub(crate) fn pick_parent(
    report: Option<&LayerSourceReport>,
    scope: &ParentScope,
    accepts: impl Fn(&LayerSource) -> bool,
    shared: impl Fn(&LayerSource) -> Result<bool, String>,
) -> Result<AddParent, String> {
    let chain = report.map_or(&[][..], |report| &report.chain[..]);
    let mut groups = chain.iter().enumerate().filter_map(|(index, entry)| {
        let layer_id = entry.layer_id.as_ref()?;
        (entry.kind == ElementKind::Primitive && entry.name == "Group" && accepts(entry))
            .then_some((index, entry, layer_id))
    });
    let parent = |(index, entry, layer_id): (usize, &LayerSource, &String), this_scene| AddParent {
        entry: entry.clone(),
        index,
        layer_id: layer_id.clone(),
        this_scene,
    };
    let nearest = groups.next().ok_or_else(|| NO_PARENT.to_owned())?;
    if *scope == ParentScope::Chosen(Scope::Shared) || !shared(nearest.1)? {
        return Ok(parent(nearest, false));
    }
    if let ParentScope::Unknown(note) = scope {
        return Err(note.clone());
    }
    for group in groups {
        if !shared(group.1)? {
            return Ok(parent(group, true));
        }
    }
    Err(NO_PARENT_IN_SCENE.to_owned())
}

/// The Add menu's header: "Add to `<Group>` in `<PriceTag>`", "Add to
/// `<Group>`" without a component, "… · This scene" when moved there.
pub(crate) fn add_header(parent: &AddParent, component: Option<&str>) -> String {
    let mut header = format!("Add to `<{}>`", parent.entry.name);
    if let Some(component) = component {
        header.push_str(&format!(" in `<{component}>`"));
    }
    if parent.this_scene {
        header.push_str(" · This scene");
    }
    header
}

/// `report` with the windows of chain entries below `parent_index` (the
/// selected element's own `<Sequence>`) dropped: they do not wrap a child
/// added to the parent. A window without a source (a scene's) stays.
pub(crate) fn report_at_parent(
    report: &LayerSourceReport,
    parent_index: usize,
) -> LayerSourceReport {
    let below: Vec<SourcePos> = report
        .chain
        .iter()
        .take(parent_index)
        .map(SourcePos::from)
        .collect();
    LayerSourceReport {
        windows: report
            .windows
            .iter()
            .filter(|window| {
                window
                    .source
                    .as_ref()
                    .is_none_or(|source| !below.contains(source))
            })
            .cloned()
            .collect(),
        ..report.clone()
    }
}

/// The wrapper for a child of the parent at `parent_index`, from the
/// playhead (the report's lookup frame) to the end of the nearest window
/// around the parent, by design 4's "Set timing" rule. For a new child the
/// parent's own window is as much the window around it as a scene's, so an
/// `Own` window counts as a `Scene` one. `None` when no wrapper is written:
/// on that window's first frame, or without a window from frame 0 to the
/// composition's end; also when less than a frame is left. Err for a
/// remapped nearest window.
pub(crate) fn added_timing(
    report: &LayerSourceReport,
    parent_index: usize,
    chapters: &[ReactChapter],
    composition_end: u64,
    playhead: u64,
) -> Result<Option<Timing>, String> {
    let at_parent = report_at_parent(report, parent_index);
    let role = match window_role(&at_parent, chapters, composition_end, playhead) {
        WindowRole::Remapped => return Err(REMAPPED.to_owned()),
        WindowRole::Own(window) => WindowRole::Scene(window),
        role => role,
    };
    let Some((from, duration)) = set_timing_span(&role, chapters, composition_end, playhead) else {
        return Ok(None);
    };
    let whole = match role {
        WindowRole::Scene(_) => true,
        _ => from + duration == composition_end as i64,
    };
    Ok((from != 0 || !whole).then_some(Timing { from, duration }))
}

/// A gesture in the parent's children space.
struct Gesture {
    press: Point,
    release: Point,
    /// It moved less than the slop.
    click: bool,
}

/// The gesture from `start` to `end` (scene pixels) mapped through the
/// inverse of `parent_space`; `None` when that collapses the plane.
fn gesture((start, end): (Point, Point), parent_space: [f64; 6], slop: f64) -> Option<Gesture> {
    let inverse = invert_affine(parent_space)?;
    Some(Gesture {
        press: apply(&inverse, start),
        release: apply(&inverse, end),
        click: (end.x - start.x).hypot(end.y - start.y) < slop,
    })
}

/// The box drawn for a Rect or Ellipse, or with `media` for an Image or a
/// Video, as its least and greatest corners. A click makes a box at the
/// press; a drag spans the press and the release, except that media with
/// a known `aspect` take their height from the width, extending from the
/// press towards the release. Every side is at least 1.
fn drawn_box(gesture: &Gesture, media: bool, aspect: Option<f64>) -> (Point, Point) {
    let Gesture {
        press,
        release,
        click,
    } = *gesture;
    let aspect = aspect.filter(|aspect| aspect.is_finite() && *aspect > 0.0);
    let corner = |min: Point, width: f64, height: f64| {
        let max = Point {
            x: min.x + width.max(1.0),
            y: min.y + height.max(1.0),
        };
        (min, max)
    };
    if click {
        let width = if media { CLICK_MEDIA_WIDTH } else { CLICK_SIDE };
        return corner(press, width, aspect.map_or(width, |aspect| width / aspect));
    }
    let width = (release.x - press.x).abs();
    let height = match aspect {
        Some(aspect) if media => (width / aspect).max(1.0),
        _ => (release.y - press.y).abs(),
    };
    let y = if release.y < press.y {
        press.y - height
    } else {
        press.y
    };
    corner(
        Point {
            x: press.x.min(release.x),
            y,
        },
        width,
        height,
    )
}

/// A Circle's radius: the parent-space distance from the press, at least
/// 1; a click's is [`CLICK_RADIUS`].
fn drawn_radius(gesture: &Gesture) -> f64 {
    if gesture.click {
        return CLICK_RADIUS;
    }
    let (press, release) = (gesture.press, gesture.release);
    (release.x - press.x).hypot(release.y - press.y).max(1.0)
}

/// The written values for a gesture from `start` to `end` (scene pixels)
/// in the parent's children space; `slop` is the click distance in scene
/// pixels. Rect, Ellipse, Image and Video drag a box (see [`drawn_box`]),
/// a Circle drags from its center, Text is placed at the press. `None`
/// without `src` for an Image or a Video, and in a collapsed parent space.
pub(crate) fn drawn_geometry(
    kind: NewKind,
    src: Option<&str>,
    points: (Point, Point),
    parent_space: [f64; 6],
    aspect: Option<f64>,
    slop: f64,
) -> Option<NewElement> {
    let gesture = gesture(points, parent_space, slop)?;
    let sized = |media| {
        let (min, max) = drawn_box(&gesture, media, aspect);
        (min.x, min.y, max.x - min.x, max.y - min.y)
    };
    Some(match kind {
        NewKind::Rect => {
            let (x, y, width, height) = sized(false);
            NewElement::Rect {
                x,
                y,
                width,
                height,
            }
        }
        NewKind::Ellipse => {
            let (x, y, width, height) = sized(false);
            NewElement::Ellipse {
                x,
                y,
                width,
                height,
            }
        }
        NewKind::Circle => NewElement::Circle {
            x: gesture.press.x,
            y: gesture.press.y,
            radius: drawn_radius(&gesture),
        },
        NewKind::Text => NewElement::Text {
            x: gesture.press.x,
            y: gesture.press.y,
        },
        NewKind::Image => {
            let (x, y, width, _) = sized(true);
            NewElement::Image {
                src: src?.to_owned(),
                x,
                y,
                width,
            }
        }
        NewKind::Video => {
            let (x, y, width, _) = sized(true);
            NewElement::Video {
                src: src?.to_owned(),
                x,
                y,
                width,
            }
        }
    })
}

/// The overlay for the gesture: the parent-space shape [`drawn_geometry`]
/// writes, mapped back to scene pixels. A box's four corners clockwise from
/// its least one, a [`CIRCLE_POINTS`]-point polygon for a Circle, the press
/// point for Text; empty in a collapsed parent space.
pub(crate) fn drawn_outline(
    kind: NewKind,
    points: (Point, Point),
    parent_space: [f64; 6],
    aspect: Option<f64>,
    slop: f64,
) -> Vec<Point> {
    let Some(gesture) = gesture(points, parent_space, slop) else {
        return Vec::new();
    };
    let back = |p: Point| apply(&parent_space, p);
    match kind {
        NewKind::Text => vec![back(gesture.press)],
        NewKind::Circle => {
            let (center, radius) = (gesture.press, drawn_radius(&gesture));
            (0..CIRCLE_POINTS)
                .map(|step| {
                    let (sin, cos) = (TAU * step as f64 / CIRCLE_POINTS as f64).sin_cos();
                    back(Point {
                        x: center.x + radius * cos,
                        y: center.y + radius * sin,
                    })
                })
                .collect()
        }
        NewKind::Rect | NewKind::Ellipse | NewKind::Image | NewKind::Video => {
            let media = matches!(kind, NewKind::Image | NewKind::Video);
            let (min, max) = drawn_box(&gesture, media, aspect);
            [
                min,
                Point { x: max.x, y: min.y },
                max,
                Point { x: min.x, y: max.y },
            ]
            .map(back)
            .to_vec()
        }
    }
}

/// Each Add menu item and why it is disabled, in menu order: `reason` (no
/// parent, the scope, the timing, a group turned in 3D) for all; then a
/// kind whose `primitive_properties` table is missing (`has_table` of its
/// tag), as its module is not imported; then `lock_of` (`add_lock`'s text).
pub(crate) fn menu_items(
    reason: Option<&str>,
    has_table: impl Fn(&str) -> bool,
    lock_of: impl Fn(NewKind) -> Option<String>,
) -> Vec<(NewKind, Option<String>)> {
    KINDS
        .into_iter()
        .map(|kind| {
            let why = reason
                .map(str::to_owned)
                .or_else(|| {
                    (!has_table(kind.tag())).then(|| {
                        format!(
                            "`{}` comes from `{}`, which this composition doesn't import",
                            kind.tag(),
                            kind.module()
                        )
                    })
                })
                .or_else(|| lock_of(kind));
            (kind, why)
        })
        .collect()
}

/// The element the add mode draws.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AddTool {
    pub(crate) kind: NewKind,
    /// The chosen file, for Image and Video.
    pub(crate) src: Option<String>,
    /// The file's width over its height, once measured.
    pub(crate) aspect: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum AddMode {
    Off,
    /// The file dialog is open.
    Picking(NewKind),
    Armed(AddTool),
    Drawing {
        tool: AddTool,
        start: Point,
        now: Point,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum AddEvent {
    /// A kind chosen in the Add menu.
    Chosen(NewKind),
    /// The file dialog closed, with the file's `src` or cancelled.
    Picked(Option<String>),
    /// The chosen file's aspect, or that it has none known.
    Measured(Option<f64>),
    Press(Point),
    Move(Point),
    Release(Point),
    Escape,
    /// Something else ended the mode.
    Ended,
}

/// The next mode, and the gesture to write on a release. Choosing a kind
/// starts over (Image and Video pick a file first); Escape and `Ended` end
/// the mode from anywhere; an event a mode does not take leaves it as is.
pub(crate) fn add_step(
    mode: AddMode,
    event: AddEvent,
) -> (AddMode, Option<(AddTool, Point, Point)>) {
    let armed = |kind, src| {
        AddMode::Armed(AddTool {
            kind,
            src,
            aspect: None,
        })
    };
    let next = match (mode, event) {
        (_, AddEvent::Escape | AddEvent::Ended) => AddMode::Off,
        (_, AddEvent::Chosen(kind @ (NewKind::Image | NewKind::Video))) => AddMode::Picking(kind),
        (_, AddEvent::Chosen(kind)) => armed(kind, None),
        (AddMode::Picking(_), AddEvent::Picked(None)) => AddMode::Off,
        (AddMode::Picking(kind), AddEvent::Picked(Some(src))) => armed(kind, Some(src)),
        (AddMode::Armed(tool), AddEvent::Measured(aspect)) => {
            AddMode::Armed(AddTool { aspect, ..tool })
        }
        (AddMode::Drawing { tool, start, now }, AddEvent::Measured(aspect)) => AddMode::Drawing {
            tool: AddTool { aspect, ..tool },
            start,
            now,
        },
        (AddMode::Armed(tool), AddEvent::Press(start)) => AddMode::Drawing {
            tool,
            start,
            now: start,
        },
        (AddMode::Drawing { tool, start, .. }, AddEvent::Move(now)) => {
            AddMode::Drawing { tool, start, now }
        }
        (AddMode::Drawing { tool, start, .. }, AddEvent::Release(end)) => {
            return (AddMode::Off, Some((tool, start, end)));
        }
        (mode, _) => mode,
    };
    (next, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scope::{ScopeLock, ScopeOptions};
    use celesta_react_bridge::{
        ElementKind, ElementOccurrences, InstanceOccurrences, OccurrenceIndex, ReceivedProps,
        TimingWindow,
    };
    use celesta_source_edit::{AddLock, ImportLock};

    const NO_PARENT: &str = "Select a group, or a layer inside one, to add into";

    fn pos(file: &str, line: u32) -> SourcePos {
        SourcePos {
            file: format!("/a/{file}.tsx").into(),
            line,
            column: 4,
        }
    }

    fn entry(name: &str, kind: ElementKind, at: &SourcePos, layer_id: Option<&str>) -> LayerSource {
        LayerSource {
            name: name.to_owned(),
            file: at.file.clone(),
            line: at.line,
            column: at.column,
            kind,
            layer_id: layer_id.map(str::to_owned),
        }
    }

    fn primitive(name: &str, at: &SourcePos, layer_id: &str) -> LayerSource {
        entry(name, ElementKind::Primitive, at, Some(layer_id))
    }

    fn component(name: &str, at: &SourcePos) -> LayerSource {
        entry(name, ElementKind::Component, at, None)
    }

    fn sequence(at: &SourcePos) -> LayerSource {
        entry("Sequence", ElementKind::Wrapper, at, None)
    }

    fn report(chain: Vec<LayerSource>, windows: Vec<TimingWindow>) -> LayerSourceReport {
        LayerSourceReport {
            chain,
            local_frame: 0.0,
            candidate_frames: vec![],
            props: ReceivedProps::default(),
            render_count: 1,
            windows,
            span: None,
        }
    }

    fn window(from: f64, to: f64, source: Option<&SourcePos>) -> TimingWindow {
        TimingWindow {
            kind: "Sequence".to_owned(),
            from,
            to,
            source: source.cloned(),
            remapped: false,
        }
    }

    fn chapter(name: &str, from: u64, duration_in_frames: u64) -> ReactChapter {
        ReactChapter {
            id: name.to_lowercase(),
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

    /// A ready index of the elements at each instance's `chain[0]`.
    fn index(instances: Vec<InstanceOccurrences>) -> OccurrenceState {
        let mut elements: Vec<ElementOccurrences> = Vec::new();
        for instance in instances {
            match elements
                .iter_mut()
                .find(|element| element.element == instance.chain[0])
            {
                Some(element) => element.instances.push(instance),
                None => elements.push(ElementOccurrences {
                    element: instance.chain[0].clone(),
                    instances: vec![instance],
                }),
            }
        }
        OccurrenceState::Ready(OccurrenceIndex { elements })
    }

    fn every_group(_: &LayerSource) -> bool {
        true
    }

    fn none_shared(_: &LayerSource) -> Result<bool, String> {
        Ok(false)
    }

    /// `pick_parent` under `scope` with every group accepting, shared when
    /// its line is in `shared_lines`.
    fn pick(
        report: &LayerSourceReport,
        scope: ParentScope,
        shared_lines: &[u32],
    ) -> Result<AddParent, String> {
        pick_parent(Some(report), &scope, every_group, |entry| {
            Ok(shared_lines.contains(&entry.line))
        })
    }

    #[test]
    fn the_selected_group_is_the_parent() {
        let report = report(
            vec![
                primitive("Group", &pos("intro", 5), "g1"),
                primitive("Group", &pos("intro", 3), "g0"),
                component("Intro", &pos("root", 2)),
            ],
            vec![],
        );
        let parent = pick_parent(
            Some(&report),
            &ParentScope::NoControl,
            every_group,
            none_shared,
        )
        .unwrap();
        assert_eq!(parent.index, 0);
        assert_eq!(parent.layer_id, "g1");
        assert_eq!(parent.entry, report.chain[0]);
        assert!(!parent.this_scene);

        // A selected group without a children region is not the parent.
        let parent = pick_parent(
            Some(&report),
            &ParentScope::NoControl,
            |entry| entry.line != 5,
            none_shared,
        )
        .unwrap();
        assert_eq!((parent.index, parent.layer_id.as_str()), (1, "g0"));
    }

    #[test]
    fn the_nearest_group_in_the_chain_is_the_parent() {
        let report = report(
            vec![
                primitive("Rect", &pos("intro", 9), "r"),
                sequence(&pos("intro", 8)),
                // Emitted no layer: not a parent.
                entry("Group", ElementKind::Primitive, &pos("intro", 7), None),
                // The author's own `Group` component.
                component("Group", &pos("intro", 6)),
                primitive("Group", &pos("intro", 5), "g1"),
                primitive("Group", &pos("intro", 3), "g0"),
            ],
            vec![],
        );
        let parent = pick(&report, ParentScope::NoControl, &[]).unwrap();
        assert_eq!((parent.index, parent.layer_id.as_str()), (4, "g1"));
    }

    #[test]
    fn a_group_without_a_children_region_is_skipped() {
        let report = report(
            vec![
                primitive("Rect", &pos("intro", 9), "r"),
                primitive("Group", &pos("intro", 5), "g1"),
                primitive("Group", &pos("intro", 3), "g0"),
            ],
            vec![],
        );
        let parent = pick_parent(
            Some(&report),
            &ParentScope::NoControl,
            |entry| entry.line != 5,
            none_shared,
        )
        .unwrap();
        assert_eq!((parent.index, parent.layer_id.as_str()), (2, "g0"));

        let none = pick_parent(
            Some(&report),
            &ParentScope::NoControl,
            |_| false,
            none_shared,
        );
        assert_eq!(none, Err(NO_PARENT.to_owned()));
    }

    #[test]
    fn nothing_selected_gives_the_reason() {
        assert_eq!(
            pick_parent(None, &ParentScope::NoControl, every_group, none_shared),
            Err(NO_PARENT.to_owned())
        );
        let no_group = report(vec![primitive("Rect", &pos("intro", 9), "r")], vec![]);
        assert_eq!(
            pick(&no_group, ParentScope::NoControl, &[]),
            Err(NO_PARENT.to_owned())
        );
    }

    #[test]
    fn is_shared_reads_the_index() {
        let rect = pos("price-tag", 6);
        let group = pos("price-tag", 4);
        let first_call = pos("intro", 8);
        let second_call = pos("intro", 9);
        let scene = pos("intro", 3);
        let chapters = [chapter("Intro", 0, 60), chapter("Outro", 60, 60)];

        let once = index(vec![instance(
            vec![
                rect.clone(),
                group.clone(),
                first_call.clone(),
                scene.clone(),
            ],
            vec![(0, 30)],
        )]);
        assert_eq!(is_shared(Some(&once), &chapters, &group), Ok(false));
        assert!(in_scene_code(Some(&once), &chapters, &group));

        // Two calls of `PriceTag`, in one scene.
        let twice = index(vec![
            instance(
                vec![
                    rect.clone(),
                    group.clone(),
                    first_call.clone(),
                    scene.clone(),
                ],
                vec![(0, 30)],
            ),
            instance(
                vec![rect.clone(), group.clone(), second_call, scene.clone()],
                vec![(30, 60)],
            ),
        ]);
        assert_eq!(is_shared(Some(&twice), &chapters, &group), Ok(true));
        assert!(!in_scene_code(Some(&twice), &chapters, &group));
        // The scene's own group is above both calls.
        assert_eq!(is_shared(Some(&twice), &chapters, &scene), Ok(false));

        // One chain, showing in two scenes.
        let spanning = index(vec![instance(
            vec![rect, group.clone(), first_call, scene],
            vec![(30, 90)],
        )]);
        assert_eq!(is_shared(Some(&spanning), &chapters, &group), Ok(true));

        let scanning = Err("Scanning scenes…".to_owned());
        assert_eq!(is_shared(None, &chapters, &group), scanning);
        assert_eq!(
            is_shared(Some(&OccurrenceState::Scanning), &chapters, &group),
            scanning
        );
        let failed = OccurrenceState::Failed("x".to_owned());
        assert_eq!(
            is_shared(Some(&failed), &chapters, &group),
            Err("Couldn't scan scenes: x".to_owned())
        );
        assert!(!in_scene_code(Some(&failed), &chapters, &group));
    }

    #[test]
    fn is_shared_without_the_entry_in_the_index_can_t_answer() {
        let elsewhere = index(vec![instance(vec![pos("outro", 4)], vec![(0, 30)])]);
        let missing = pos("intro", 4);
        assert_eq!(
            is_shared(Some(&elsewhere), &[], &missing),
            Err("Not found in the scene scan".to_owned())
        );
        // A sweep under way finds what a write moved.
        let OccurrenceState::Ready(stale) = elsewhere else {
            unreachable!()
        };
        assert_eq!(
            is_shared(Some(&OccurrenceState::Refreshing(stale)), &[], &missing),
            Err("Scanning scenes…".to_owned())
        );
    }

    /// A `<Rect>` in a group inside `<PriceTag>`, called by `<Intro>` from
    /// inside the scene's own group.
    fn in_price_tag() -> LayerSourceReport {
        report(
            vec![
                primitive("Rect", &pos("price-tag", 6), "r"),
                primitive("Group", &pos("price-tag", 4), "tag"),
                component("PriceTag", &pos("intro", 8)),
                primitive("Group", &pos("intro", 5), "scene"),
                component("Intro", &pos("root", 2)),
            ],
            vec![],
        )
    }

    #[test]
    fn this_scene_falls_back_to_a_parent_in_the_scene_s_code() {
        let report = in_price_tag();
        let this_scene = pick(&report, ParentScope::Chosen(Scope::ThisScene), &[4]).unwrap();
        assert_eq!(
            (this_scene.index, this_scene.layer_id.as_str()),
            (3, "scene")
        );
        assert!(this_scene.this_scene);
        assert_eq!(
            add_header(&this_scene, Some("Intro")),
            "Add to `<Group>` in `<Intro>` · This scene"
        );

        let shared = pick(&report, ParentScope::Chosen(Scope::Shared), &[4]).unwrap();
        assert_eq!((shared.index, shared.layer_id.as_str()), (1, "tag"));
        assert!(!shared.this_scene);

        assert_eq!(
            pick(&report, ParentScope::Chosen(Scope::ThisScene), &[4, 5]),
            Err("No group in this scene's own code to add into".to_owned())
        );
    }

    #[test]
    fn a_parent_shared_through_children_is_not_used_unasked() {
        // The selected element is not shared, its group is (a `<Card>`
        // drawing `{children}` in its group, called by two scenes).
        let report = in_price_tag();
        let fallback = pick(&report, ParentScope::NoControl, &[4]).unwrap();
        assert_eq!((fallback.index, fallback.this_scene), (3, true));

        let unknown = ParentScope::Unknown("Scanning scenes…".to_owned());
        assert_eq!(
            pick(&report, unknown.clone(), &[4]),
            Err("Scanning scenes…".to_owned())
        );
        let unshared = pick(&report, unknown, &[]).unwrap();
        assert_eq!((unshared.index, unshared.this_scene), (1, false));

        // The index can't answer for the candidate.
        assert_eq!(
            pick_parent(
                Some(&report),
                &ParentScope::NoControl,
                every_group,
                |_| Err("Couldn't scan scenes: x".to_owned())
            ),
            Err("Couldn't scan scenes: x".to_owned())
        );
    }

    #[test]
    fn parent_scope_follows_the_selected_element() {
        let options = |shared| ScopeOptions {
            this_scene: Ok(()),
            shared,
            shared_reaches: vec![],
            sets_own: vec![],
        };
        let open = ScopeState::Options(options(Ok(())));
        assert_eq!(
            parent_scope(&open, None),
            ParentScope::Chosen(Scope::ThisScene)
        );
        assert_eq!(
            parent_scope(&open, Some(Scope::Shared)),
            ParentScope::Chosen(Scope::Shared)
        );

        let lock = ScopeLock::OtherScenes {
            component: "PriceTag".to_owned(),
        };
        let locked = ScopeState::Options(options(Err(lock.clone())));
        assert_eq!(
            parent_scope(&locked, Some(Scope::Shared)),
            ParentScope::Unknown(lock.text())
        );

        assert_eq!(
            parent_scope(&ScopeState::NotShared, Some(Scope::Shared)),
            ParentScope::NoControl
        );
        let scanning = ScopeState::Unknown {
            note: "Scanning scenes…".to_owned(),
        };
        assert_eq!(
            parent_scope(&scanning, None),
            ParentScope::Unknown("Scanning scenes…".to_owned())
        );
    }

    #[test]
    fn the_header_names_the_parent_and_its_component() {
        let parent = AddParent {
            entry: primitive("Group", &pos("price-tag", 4), "tag"),
            index: 1,
            layer_id: "tag".to_owned(),
            this_scene: false,
        };
        assert_eq!(
            add_header(&parent, Some("PriceTag")),
            "Add to `<Group>` in `<PriceTag>`"
        );
        assert_eq!(add_header(&parent, None), "Add to `<Group>`");
    }

    #[test]
    fn the_selected_element_s_own_sequence_does_not_time_the_child() {
        let own = pos("intro", 9);
        let groups = pos("intro", 5);
        let report = report(
            vec![
                primitive("Rect", &pos("intro", 10), "r"),
                sequence(&own),
                primitive("Group", &pos("intro", 6), "g"),
                sequence(&groups),
            ],
            vec![
                window(330.0, 390.0, Some(&own)),
                window(300.0, 420.0, Some(&groups)),
            ],
        );
        let chapters = [chapter("Intro", 300, 120)];

        assert_eq!(
            report_at_parent(&report, 2).windows,
            vec![window(300.0, 420.0, Some(&groups))]
        );
        assert_eq!(report_at_parent(&report, 0), report);

        assert_eq!(
            added_timing(&report, 2, &chapters, 960, 345),
            Ok(Some(Timing {
                from: 45,
                duration: 75
            }))
        );
        // The group's window starts here.
        assert_eq!(added_timing(&report, 2, &chapters, 960, 300), Ok(None));
    }

    #[test]
    fn an_own_window_around_the_parent_times_the_child() {
        let around = pos("intro", 5);
        let report = report(
            vec![
                primitive("Rect", &pos("intro", 7), "r"),
                primitive("Group", &pos("intro", 6), "g"),
                sequence(&around),
            ],
            vec![window(330.0, 360.0, Some(&around))],
        );
        assert_eq!(
            added_timing(&report, 1, &[chapter("Intro", 300, 120)], 960, 345),
            Ok(Some(Timing {
                from: 15,
                duration: 15
            }))
        );
    }

    #[test]
    fn without_a_window_the_child_runs_to_the_chapter_end() {
        let report = report(
            vec![
                primitive("Rect", &pos("intro", 7), "r"),
                primitive("Group", &pos("intro", 6), "g"),
            ],
            vec![],
        );
        assert_eq!(
            added_timing(&report, 1, &[chapter("Intro", 300, 120)], 960, 345),
            Ok(Some(Timing {
                from: 345,
                duration: 75
            }))
        );
        // The whole composition from its first frame.
        assert_eq!(added_timing(&report, 1, &[], 600, 0), Ok(None));
        // From the first frame, but only to the scene's end.
        assert_eq!(
            added_timing(&report, 1, &[chapter("Intro", 0, 120)], 600, 0),
            Ok(Some(Timing {
                from: 0,
                duration: 120
            }))
        );
    }

    #[test]
    fn a_remapped_window_locks_the_addition() {
        let report = report(
            vec![
                primitive("Rect", &pos("intro", 7), "r"),
                primitive("Group", &pos("intro", 6), "g"),
            ],
            vec![TimingWindow {
                remapped: true,
                ..window(0.0, 60.0, Some(&pos("intro", 5)))
            }],
        );
        assert_eq!(
            added_timing(&report, 1, &[], 600, 30),
            Err("Can't add inside a loop or freeze frame".to_owned())
        );
    }

    const IDENTITY: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    const SLOP: f64 = 4.0;

    fn at(x: f64, y: f64) -> Point {
        Point { x, y }
    }

    /// Translated (100, 50), rotated 30° and scaled 2.
    fn turned() -> [f64; 6] {
        let (sin, cos) = 30f64.to_radians().sin_cos();
        [2.0 * cos, 2.0 * sin, -2.0 * sin, 2.0 * cos, 100.0, 50.0]
    }

    /// `p` in parent space, in scene pixels.
    fn to_scene(space: [f64; 6], p: Point) -> Point {
        crate::canvas_geometry::apply(&space, p)
    }

    fn assert_near(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-9,
            "{actual} is not {expected}"
        );
    }

    fn assert_points(actual: &[Point], expected: &[Point]) {
        assert_eq!(actual.len(), expected.len(), "{actual:?}");
        for (actual, expected) in actual.iter().zip(expected) {
            assert_near(actual.x, expected.x);
            assert_near(actual.y, expected.y);
        }
    }

    /// The numbers `element` writes, in order, and its `src`.
    fn numbers(element: &NewElement) -> (NewKind, Vec<f64>, Option<&str>) {
        let (values, src) = match element {
            NewElement::Rect {
                x,
                y,
                width,
                height,
            }
            | NewElement::Ellipse {
                x,
                y,
                width,
                height,
            } => (vec![*x, *y, *width, *height], None),
            NewElement::Circle { x, y, radius } => (vec![*x, *y, *radius], None),
            NewElement::Text { x, y } => (vec![*x, *y], None),
            NewElement::Image { src, x, y, width } | NewElement::Video { src, x, y, width } => {
                (vec![*x, *y, *width], Some(src.as_str()))
            }
        };
        (element.kind(), values, src)
    }

    fn assert_element(actual: Option<NewElement>, expected: NewElement) {
        let actual = actual.expect("an element");
        let (kind, values, src) = numbers(&actual);
        let (expected_kind, expected_values, expected_src) = numbers(&expected);
        assert_eq!((kind, src), (expected_kind, expected_src), "{actual:?}");
        assert_eq!(values.len(), expected_values.len());
        for (value, expected) in values.into_iter().zip(expected_values) {
            assert_near(value, expected);
        }
    }

    /// The box `min`–`max` in parent space, clockwise from `min`, in scene
    /// pixels.
    fn box_in_scene(space: [f64; 6], min: Point, max: Point) -> Vec<Point> {
        [min, at(max.x, min.y), max, at(min.x, max.y)]
            .map(|corner| to_scene(space, corner))
            .to_vec()
    }

    #[test]
    fn a_box_in_a_translated_rotated_scaled_parent() {
        let space = turned();
        let press = to_scene(space, at(10.0, 20.0));
        let release = to_scene(space, at(60.0, 80.0));
        assert_element(
            drawn_geometry(NewKind::Rect, None, (press, release), space, None, SLOP),
            NewElement::Rect {
                x: 10.0,
                y: 20.0,
                width: 50.0,
                height: 60.0,
            },
        );
        let outline = drawn_outline(NewKind::Rect, (press, release), space, None, SLOP);
        assert_points(
            &outline,
            &box_in_scene(space, at(10.0, 20.0), at(60.0, 80.0)),
        );
        assert_points(&[outline[0], outline[2]], &[press, release]);
    }

    #[test]
    fn a_box_dragged_up_left_in_a_rotated_scaled_parent() {
        let space = turned();
        let press = to_scene(space, at(60.0, 80.0));
        let release = to_scene(space, at(10.0, 20.0));
        // Up and to the left on the canvas too.
        assert!(release.x < press.x && release.y < press.y);
        // The drawn diagonal is twice as long in scene pixels.
        assert_near(
            (press.x - release.x).hypot(press.y - release.y),
            2.0 * 50f64.hypot(60.0),
        );
        assert_element(
            drawn_geometry(NewKind::Ellipse, None, (press, release), space, None, SLOP),
            NewElement::Ellipse {
                x: 10.0,
                y: 20.0,
                width: 50.0,
                height: 60.0,
            },
        );
        let outline = drawn_outline(NewKind::Ellipse, (press, release), space, None, SLOP);
        assert_points(
            &outline,
            &box_in_scene(space, at(10.0, 20.0), at(60.0, 80.0)),
        );
        assert_points(&[outline[2], outline[0]], &[press, release]);
    }

    #[test]
    fn a_click_makes_200_by_200() {
        let space = turned();
        let press = to_scene(space, at(10.0, 20.0));
        // A move shorter than the slop is a click.
        let release = at(press.x + 2.0, press.y + 2.0);
        for kind in [NewKind::Rect, NewKind::Ellipse] {
            let drawn = drawn_geometry(kind, None, (press, release), space, None, SLOP);
            let (_, values, _) = numbers(drawn.as_ref().unwrap());
            for (value, expected) in values.into_iter().zip([10.0, 20.0, 200.0, 200.0]) {
                assert_near(value, expected);
            }
            assert_points(
                &drawn_outline(kind, (press, release), space, None, SLOP),
                &box_in_scene(space, at(10.0, 20.0), at(210.0, 220.0)),
            );
        }
    }

    #[test]
    fn image_and_video_keep_the_aspect_400_wide() {
        let press = at(100.0, 200.0);
        let click = (press, press);
        assert_element(
            drawn_geometry(
                NewKind::Image,
                Some("a.png"),
                click,
                IDENTITY,
                Some(2.0),
                SLOP,
            ),
            NewElement::Image {
                src: "a.png".to_owned(),
                x: 100.0,
                y: 200.0,
                width: 400.0,
            },
        );
        assert_points(
            &drawn_outline(NewKind::Image, click, IDENTITY, Some(2.0), SLOP),
            &box_in_scene(IDENTITY, press, at(500.0, 400.0)),
        );

        // 300 to the right and upwards: the height follows the aspect,
        // upwards from the press.
        let drag = (press, at(400.0, 120.0));
        assert_element(
            drawn_geometry(
                NewKind::Video,
                Some("a.mp4"),
                drag,
                IDENTITY,
                Some(2.0),
                SLOP,
            ),
            NewElement::Video {
                src: "a.mp4".to_owned(),
                x: 100.0,
                y: 50.0,
                width: 300.0,
            },
        );
        assert_points(
            &drawn_outline(NewKind::Video, drag, IDENTITY, Some(2.0), SLOP),
            &box_in_scene(IDENTITY, at(100.0, 50.0), at(400.0, 200.0)),
        );

        // Without a known aspect, the box as dragged.
        assert_element(
            drawn_geometry(NewKind::Image, Some("a.svg"), drag, IDENTITY, None, SLOP),
            NewElement::Image {
                src: "a.svg".to_owned(),
                x: 100.0,
                y: 120.0,
                width: 300.0,
            },
        );
        assert_points(
            &drawn_outline(NewKind::Image, drag, IDENTITY, None, SLOP),
            &box_in_scene(IDENTITY, at(100.0, 120.0), at(400.0, 200.0)),
        );

        // No file, nothing to write.
        assert_eq!(
            drawn_geometry(NewKind::Image, None, drag, IDENTITY, None, SLOP),
            None
        );
    }

    #[test]
    fn a_circle_drags_from_its_center() {
        let space = turned();
        let center = to_scene(space, at(10.0, 20.0));
        let edge = to_scene(space, at(13.0, 24.0));
        assert_element(
            drawn_geometry(NewKind::Circle, None, (center, edge), space, None, SLOP),
            NewElement::Circle {
                x: 10.0,
                y: 20.0,
                radius: 5.0,
            },
        );
        let outline = drawn_outline(NewKind::Circle, (center, edge), space, None, SLOP);
        assert_eq!(outline.len(), 32);
        assert_points(&outline[..1], &[to_scene(space, at(15.0, 20.0))]);
        assert_points(&outline[8..9], &[to_scene(space, at(10.0, 25.0))]);

        assert_element(
            drawn_geometry(NewKind::Circle, None, (center, center), space, None, SLOP),
            NewElement::Circle {
                x: 10.0,
                y: 20.0,
                radius: 100.0,
            },
        );
    }

    #[test]
    fn text_is_placed_where_clicked() {
        let space = turned();
        let press = to_scene(space, at(10.0, 20.0));
        for release in [press, to_scene(space, at(60.0, 80.0))] {
            assert_element(
                drawn_geometry(NewKind::Text, None, (press, release), space, None, SLOP),
                NewElement::Text { x: 10.0, y: 20.0 },
            );
            assert_points(
                &drawn_outline(NewKind::Text, (press, release), space, None, SLOP),
                &[press],
            );
        }
    }

    #[test]
    fn every_drawn_side_is_at_least_one() {
        let straight_down = (at(100.0, 100.0), at(100.0, 180.0));
        assert_element(
            drawn_geometry(NewKind::Rect, None, straight_down, IDENTITY, None, SLOP),
            NewElement::Rect {
                x: 100.0,
                y: 100.0,
                width: 1.0,
                height: 80.0,
            },
        );
        assert_element(
            drawn_geometry(
                NewKind::Image,
                Some("a.png"),
                straight_down,
                IDENTITY,
                Some(2.0),
                SLOP,
            ),
            NewElement::Image {
                src: "a.png".to_owned(),
                x: 100.0,
                y: 100.0,
                width: 1.0,
            },
        );
        let flat = (at(100.0, 100.0), at(180.0, 100.25));
        assert_element(
            drawn_geometry(NewKind::Ellipse, None, flat, IDENTITY, None, SLOP),
            NewElement::Ellipse {
                x: 100.0,
                y: 100.0,
                width: 80.0,
                height: 1.0,
            },
        );
        // Five scene pixels in a parent scaled 10 is half a unit.
        let scaled = [10.0, 0.0, 0.0, 10.0, 0.0, 0.0];
        assert_element(
            drawn_geometry(
                NewKind::Circle,
                None,
                (at(0.0, 0.0), at(5.0, 0.0)),
                scaled,
                None,
                SLOP,
            ),
            NewElement::Circle {
                x: 0.0,
                y: 0.0,
                radius: 1.0,
            },
        );
    }

    #[test]
    fn a_collapsed_parent_draws_nothing() {
        // A group scaled to zero.
        let collapsed = [0.0, 0.0, 0.0, 0.0, 10.0, 10.0];
        let drag = (at(0.0, 0.0), at(50.0, 50.0));
        for kind in [
            NewKind::Rect,
            NewKind::Ellipse,
            NewKind::Circle,
            NewKind::Text,
            NewKind::Image,
            NewKind::Video,
        ] {
            assert_eq!(
                drawn_geometry(kind, Some("a.png"), drag, collapsed, Some(1.0), SLOP),
                None
            );
            assert_eq!(
                drawn_outline(kind, drag, collapsed, Some(1.0), SLOP),
                vec![]
            );
        }
    }

    #[test]
    fn menu_items_say_why() {
        let all = |_: &str| true;
        let no_lock = |_: NewKind| None;
        let kinds = [
            NewKind::Rect,
            NewKind::Ellipse,
            NewKind::Circle,
            NewKind::Text,
            NewKind::Image,
            NewKind::Video,
        ];

        let open = menu_items(None, all, no_lock);
        assert_eq!(open, kinds.map(|kind| (kind, None)).to_vec());

        let reason = "Can't draw into a group turned in 3D";
        let disabled = menu_items(Some(reason), all, |_| Some("clash".to_owned()));
        assert_eq!(
            disabled,
            kinds.map(|kind| (kind, Some(reason.to_owned()))).to_vec()
        );

        let without_shapes = menu_items(None, |tag| tag != "Ellipse" && tag != "Circle", no_lock);
        let shapes = |tag: &str| {
            Some(format!(
                "`{tag}` comes from `@celesta/shapes`, which this composition doesn't import"
            ))
        };
        assert_eq!(
            without_shapes,
            vec![
                (NewKind::Rect, None),
                (NewKind::Ellipse, shapes("Ellipse")),
                (NewKind::Circle, shapes("Circle")),
                (NewKind::Text, None),
                (NewKind::Image, None),
                (NewKind::Video, None),
            ]
        );

        let clash = AddLock::Import(ImportLock::Clash {
            name: "Rect".to_owned(),
        })
        .text();
        let clashing = menu_items(None, all, |kind| {
            (kind == NewKind::Rect).then(|| clash.clone())
        });
        assert_eq!(
            clashing[0],
            (
                NewKind::Rect,
                Some("`Rect` here is not Celesta's".to_owned())
            )
        );
        assert!(clashing[1..].iter().all(|(_, why)| why.is_none()));
    }

    #[test]
    fn add_mode_steps() {
        let tool = |kind, src: Option<&str>, aspect| AddTool {
            kind,
            src: src.map(str::to_owned),
            aspect,
        };
        let step = |mode, event| add_step(mode, event);

        assert_eq!(
            step(AddMode::Off, AddEvent::Chosen(NewKind::Image)),
            (AddMode::Picking(NewKind::Image), None)
        );
        assert_eq!(
            step(AddMode::Off, AddEvent::Chosen(NewKind::Rect)),
            (AddMode::Armed(tool(NewKind::Rect, None, None)), None)
        );
        assert_eq!(
            step(AddMode::Picking(NewKind::Image), AddEvent::Picked(None)),
            (AddMode::Off, None)
        );
        let armed = AddMode::Armed(tool(NewKind::Video, Some("a.mp4"), None));
        assert_eq!(
            step(
                AddMode::Picking(NewKind::Video),
                AddEvent::Picked(Some("a.mp4".to_owned()))
            ),
            (armed.clone(), None)
        );
        assert_eq!(
            step(armed.clone(), AddEvent::Measured(Some(1.5))),
            (
                AddMode::Armed(tool(NewKind::Video, Some("a.mp4"), Some(1.5))),
                None
            )
        );

        let (press, now, end) = (at(1.0, 2.0), at(5.0, 6.0), at(9.0, 9.0));
        let drawing = |now, aspect| AddMode::Drawing {
            tool: tool(NewKind::Video, Some("a.mp4"), aspect),
            start: press,
            now,
        };
        assert_eq!(
            step(armed.clone(), AddEvent::Press(press)),
            (drawing(press, None), None)
        );
        assert_eq!(
            step(drawing(press, None), AddEvent::Move(now)),
            (drawing(now, None), None)
        );
        assert_eq!(
            step(drawing(now, None), AddEvent::Measured(Some(2.0))),
            (drawing(now, Some(2.0)), None)
        );
        assert_eq!(
            step(drawing(now, Some(2.0)), AddEvent::Release(end)),
            (
                AddMode::Off,
                Some((tool(NewKind::Video, Some("a.mp4"), Some(2.0)), press, end))
            )
        );

        for mode in [
            AddMode::Off,
            AddMode::Picking(NewKind::Image),
            armed,
            drawing(now, None),
        ] {
            for event in [AddEvent::Escape, AddEvent::Ended] {
                assert_eq!(step(mode.clone(), event), (AddMode::Off, None));
            }
        }
    }
}
