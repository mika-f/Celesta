//! Adding an element: the group it goes into (the selected one or the
//! nearest around it, moved into the scene's own code under "This scene"),
//! the `<Sequence>` it is wrapped in, the values a gesture on the canvas
//! writes and the outline drawn meanwhile, the Add menu's items, and the
//! add mode's steps; then the write, the selection of the added layer after
//! the reload, and the view's side of all of it.

use crate::canvas_geometry::apply;
use crate::edit_history::Texts;
use crate::element_edit::{inside_project, tag_name};
use crate::layer_select::{LookupResult, SourceLookup, letterbox, scene_point, to_scene};
use crate::occurrences::OccurrenceState;
use crate::preview::AddedUnder;
use crate::scope::{Scope, ScopeState, chosen_scope, not_indexed_note, occurrence_index};
use crate::timing::{WindowRole, set_timing_span, window_role};
use crate::view::EditorView;
use celesta_composition::Point;
use celesta_gpu_renderer::invert_affine;
use celesta_media::FfmpegBackend;
use celesta_react_bridge::{
    ElementKind, LayerSource, LayerSourceReport, ReactChapter, SourcePos, import_path, scenes_of,
};
use celesta_source_edit::{
    NewElement, NewKind, Timing, add_child, add_locks, element_at, enclosing_component,
};
use gpui_kit::{Context, MouseMoveEvent, PathPromptOptions, Pixels};
use std::cell::RefCell;
use std::collections::HashMap;
use std::f64::consts::TAU;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

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
const TURNED_3D: &str = "Can't draw into a group turned in 3D";
const SCALED_TO_ZERO: &str = "Can't draw into a group scaled to zero";
const READING: &str = "The source is still being read";
const SOURCE_CHANGED: &str = "The source changed while editing";

/// A press that moves less than this, in viewer pixels, is a click.
const CLICK_SLOP: f32 = 4.0;

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
/// 3's note when the index can't answer (see [`occurrence_index`]), and
/// when no chain goes through `entry` (see [`not_indexed_note`]).
pub(crate) fn is_shared(
    occurrences: Option<&OccurrenceState>,
    chapters: &[ReactChapter],
    entry: &SourcePos,
) -> Result<bool, String> {
    let (index, refreshing) = occurrence_index(occurrences)?;
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
        return Err(not_indexed_note(refreshing));
    };
    Ok(outer.iter().any(|part| part != first) || scenes_of(&ranges, chapters).len() > 1)
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
    format!("Add to {}", parent_label(parent, component))
}

/// The parent as the header names it: "`<Group>` in `<PriceTag>`", and
/// "… · This scene" when moved there.
pub(crate) fn parent_label(parent: &AddParent, component: Option<&str>) -> String {
    let mut label = format!("`<{}>`", parent.entry.name);
    if let Some(component) = component {
        label.push_str(&format!(" in `<{component}>`"));
    }
    if parent.this_scene {
        label.push_str(" · This scene");
    }
    label
}

/// Why an addition is refused at the release: the parent is not the one
/// shown when its kind was chosen.
const PARENT_CHANGED: &str = "The parent changed; choose the item again";

/// `report` without the windows below the parent at `parent_index`: the
/// windows are nearest first, so every one up to and including the last
/// whose source is a chain entry below the parent (the selected element's
/// own `<Sequence>`) is dropped; they do not wrap a child added to the
/// parent. A source-less window among them goes too, and one between two
/// remaining windows stays.
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
    let inside = report
        .windows
        .iter()
        .rposition(|window| {
            window
                .source
                .as_ref()
                .is_some_and(|source| below.contains(source))
        })
        .map_or(0, |last| last + 1);
    LayerSourceReport {
        windows: report.windows[inside..].to_vec(),
        ..report.clone()
    }
}

/// The wrapper for a child of the parent at `parent_index`, from the
/// playhead (the report's lookup frame) to the end of the nearest window
/// around the parent, by design 4's "Set timing" rule. For a new child the
/// parent's own window is as much the window around it as a scene's, so an
/// `Own` window counts as a `Scene` one. `None` when no wrapper is written:
/// on that window's first frame, or without a window from frame 0 to the
/// composition's end. With less than a frame left the child runs one
/// frame. Err for a remapped nearest window.
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
        // Less than a frame left: one frame, wrapped even on the window's
        // first frame, as unwrapped the child would run the whole window.
        let start = match role {
            WindowRole::Scene(window) => window.from.round() as i64,
            _ => 0,
        };
        return Ok(Some(Timing {
            from: playhead as i64 - start,
            duration: 1,
        }));
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
/// parent, the scope, the timing, a group turned in 3D) for all, else
/// `lock_of` (`add_lock`'s text). A kind whose module the file doesn't
/// import yet is enabled: adding it imports it, `@celesta/shapes` being
/// bundled with the runtime like `@celesta/react`.
pub(crate) fn menu_items(
    reason: Option<&str>,
    lock_of: impl Fn(NewKind) -> Option<String>,
) -> Vec<(NewKind, Option<String>)> {
    KINDS
        .into_iter()
        .map(|kind| {
            let why = reason.map(str::to_owned).or_else(|| lock_of(kind));
            (kind, why)
        })
        .collect()
}

/// Whether a frame is to ask for the children space of `parent` (the group
/// an addition goes into, by its layer id): its space is not `reported`,
/// and no frame was asked for it since the facts were read (`asked`).
pub(crate) fn space_request_due(parent: Option<&str>, reported: bool, asked: Option<&str>) -> bool {
    parent.is_some_and(|parent| !reported && asked != Some(parent))
}

/// Why the Add menu's items are disabled by the parent's children `space`
/// as last reported (`None`: not yet, `Some(None)`: turned in 3D). An
/// unreported space disables them only while paused, when it is being
/// asked for; while `playing` they stay enabled, as choosing one pauses,
/// which asks for it (until it arrives nothing is outlined, and a release
/// refuses).
pub(crate) fn space_reason(space: Option<Option<[f64; 6]>>, playing: bool) -> Option<&'static str> {
    match space {
        Some(None) => Some(TURNED_3D),
        Some(Some(space)) if invert_affine(space).is_none() => Some(SCALED_TO_ZERO),
        Some(Some(_)) => None,
        None => (!playing).then_some(READING),
    }
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

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) enum AddMode {
    #[default]
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
    /// The aspect of the file `src`, or that it has none known.
    Measured {
        src: String,
        aspect: Option<f64>,
    },
    Press(Point),
    Move(Point),
    Release(Point),
    Escape,
    /// Something else ended the mode.
    Ended,
}

/// The next mode, and the gesture to write on a release. Choosing a kind
/// starts over (Image and Video pick a file first); Escape and `Ended` end
/// the mode from anywhere; a measured aspect lands on the tool of that
/// file only; an event a mode does not take leaves it as is.
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
    let measures = |tool: &AddTool, src: &str| tool.src.as_deref() == Some(src);
    let next = match (mode, event) {
        (_, AddEvent::Escape | AddEvent::Ended) => AddMode::Off,
        (_, AddEvent::Chosen(kind @ (NewKind::Image | NewKind::Video))) => AddMode::Picking(kind),
        (_, AddEvent::Chosen(kind)) => armed(kind, None),
        (AddMode::Picking(_), AddEvent::Picked(None)) => AddMode::Off,
        (AddMode::Picking(kind), AddEvent::Picked(Some(src))) => armed(kind, Some(src)),
        (AddMode::Armed(tool), AddEvent::Measured { src, aspect }) if measures(&tool, &src) => {
            AddMode::Armed(AddTool { aspect, ..tool })
        }
        (AddMode::Drawing { tool, start, now }, AddEvent::Measured { src, aspect })
            if measures(&tool, &src) =>
        {
            AddMode::Drawing {
                tool: AddTool { aspect, ..tool },
                start,
                now,
            }
        }
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

/// The texts after adding `new` under the parent written at `parent` in
/// `file` with the tag `tag` (as [`GroupFacts::tag`] read it, so an alias
/// of `Group` counts), and the new element's position. Err: "The source
/// changed while editing" when no element with that tag is at `parent` any
/// more, else the lock's text.
pub(crate) fn add_files(
    texts: &Texts,
    file: &Path,
    parent: &SourcePos,
    tag: &str,
    new: &NewElement,
    timing: Option<&Timing>,
) -> Result<(Vec<(PathBuf, String)>, SourcePos), String> {
    let changed = || SOURCE_CHANGED.to_owned();
    let text = texts.get(file).ok_or_else(changed)?;
    let element = element_at(text, parent.line, parent.column)
        .filter(|element| tag_name(text, element) == tag)
        .ok_or_else(changed)?;
    let added = add_child(text, &element, new, timing).map_err(|lock| lock.text())?;
    let written =
        celesta_source_edit::apply(text, &added.edits).map_err(|error| error.to_string())?;
    let position = SourcePos {
        file: file.to_path_buf(),
        line: added.line,
        column: added.column,
    };
    Ok((vec![(file.to_path_buf(), written)], position))
}

/// The selection of an element just added, waiting for the reload.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PendingSelect {
    /// Where the element was inserted.
    pub(crate) position: SourcePos,
    pub(crate) under: AddedUnder,
    /// The first preview generation drawn with the reloaded code.
    pub(crate) from: Option<u64>,
}

/// What an answer for the pending selection does with it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum AddedFate {
    /// Not an answer from the reloaded code (or none): keep waiting.
    Wait,
    /// Select this layer.
    Select(String),
    /// The parent has no added layer: the kept selection stays.
    Drop,
}

/// What an answer from preview `generation` does with the pending
/// selection: `added` is the parent asked about and the layer found under
/// it, `None` when the frame did not ask.
pub(crate) fn added_fate(
    pending: &PendingSelect,
    generation: u64,
    added: Option<&(String, Option<String>)>,
) -> AddedFate {
    match (pending.from, added) {
        (Some(from), Some((parent, layer)))
            if generation >= from && *parent == pending.under.parent =>
        {
            layer.clone().map_or(AddedFate::Drop, AddedFate::Select)
        }
        _ => AddedFate::Wait,
    }
}

/// Whether the selected added layer's lookup shows the inserted element
/// (`chain[0]` at `position`).
pub(crate) fn added_confirmed(position: &SourcePos, lookup: &LookupResult) -> bool {
    matches!(lookup, Ok(Some(report))
        if report.chain.first().is_some_and(|element| SourcePos::from(element) == *position))
}

/// What an addition into one group of the selected layer's chain would
/// be, read from its source.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GroupFacts {
    pub(crate) at: SourcePos,
    /// Its tag as written, its last `.` segment (`Group`, an alias).
    pub(crate) tag: String,
    /// The component the group is written in.
    pub(crate) component: Option<String>,
    /// The wrapper of a child added at the lookup frame (see
    /// [`added_timing`]).
    pub(crate) timing: Result<Option<Timing>, String>,
    /// `add_lock`'s text for each kind, in menu order.
    pub(crate) locks: Vec<(NewKind, Option<String>)>,
}

/// The groups of `report`'s chain an element can be added to: primitive
/// `Group`s with a layer id whose JSX, in a file `read` reads and `inside`
/// puts in the project, has a children region; with the facts of an
/// addition at `playhead` (the lookup's frame).
pub(crate) fn parent_facts(
    read: impl Fn(&Path) -> Option<String>,
    inside: impl Fn(&Path) -> bool,
    report: &LayerSourceReport,
    chapters: &[ReactChapter],
    composition_end: u64,
    playhead: u64,
) -> Vec<GroupFacts> {
    let mut texts: HashMap<&Path, Option<String>> = HashMap::new();
    let mut groups = Vec::new();
    for (index, entry) in report.chain.iter().enumerate() {
        let candidate = entry.kind == ElementKind::Primitive
            && entry.name == "Group"
            && entry.layer_id.is_some()
            && inside(&entry.file);
        if !candidate {
            continue;
        }
        let Some(text) = texts
            .entry(entry.file.as_path())
            .or_insert_with(|| read(&entry.file))
            .as_deref()
        else {
            continue;
        };
        let Some(element) = element_at(text, entry.line, entry.column) else {
            continue;
        };
        let timing = added_timing(report, index, chapters, composition_end, playhead);
        let wrapped = matches!(timing, Ok(Some(_)));
        // `None` without a children region.
        let Some(locks) = add_locks(text, &element, &KINDS, wrapped) else {
            continue;
        };
        groups.push(GroupFacts {
            at: SourcePos::from(entry),
            tag: tag_name(text, &element).to_owned(),
            component: enclosing_component(text, &element),
            timing,
            locks: locks
                .into_iter()
                .map(|(kind, lock)| (kind, lock.map(|lock| lock.text())))
                .collect(),
        });
    }
    groups
}

/// The width over the height of the image or video at `path`; `None` when
/// it can't be read (an SVG, say).
fn media_aspect(kind: NewKind, path: &Path) -> Option<f64> {
    let (width, height) = if kind == NewKind::Video {
        let mut backend = FfmpegBackend::new();
        let video = backend.probe(path).ok()?.video.as_ref()?;
        (video.width, video.height)
    } else {
        image::ImageReader::open(path)
            .ok()?
            .with_guessed_format()
            .ok()?
            .into_dimensions()
            .ok()?
    };
    (width > 0 && height > 0).then(|| f64::from(width) / f64::from(height))
}

/// The Add menu as the view shows it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AddMenu {
    /// Where the element goes (see [`add_header`]).
    pub(crate) header: Option<String>,
    /// Why every item is disabled.
    pub(crate) reason: Option<String>,
    pub(crate) items: Vec<(NewKind, Option<String>)>,
}

/// The selected layer, its lookup and the lookup's frame.
type LookupKey = (String, LayerSourceReport, i64);

/// The groups read for a lookup of the selected layer.
#[derive(Debug)]
struct ParentFacts {
    lookup: LookupKey,
    groups: Vec<GroupFacts>,
}

impl ParentFacts {
    fn group(&self, entry: &LayerSource) -> Option<&GroupFacts> {
        let at = SourcePos::from(entry);
        self.groups.iter().find(|group| group.at == at)
    }
}

/// What a parent is picked from (see [`pick_parent`]) besides the report
/// and the groups, which the facts read name: the panel, the canvas and
/// each frame request read the parent, during playback too.
#[derive(Debug, PartialEq)]
struct TargetKey {
    facts_read: u64,
    scope: ParentScope,
    /// The scene scan's, for [`is_shared`].
    occurrence_generation: u64,
    chapters: Vec<ReactChapter>,
}

/// The parent shown when a kind was chosen: the one the element goes into.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ChosenParent {
    pub(crate) at: SourcePos,
    pub(crate) layer_id: String,
    /// Where it is, as the armed hint says it (see [`parent_label`]).
    pub(crate) label: String,
}

/// What the view keeps for adding elements.
#[derive(Debug, Default)]
pub(crate) struct AddState {
    pub(crate) mode: AddMode,
    /// The parent shown when the drawn kind was chosen.
    pub(crate) chosen: Option<ChosenParent>,
    /// The newest groups read, and the lookup the newest read is for.
    facts: Option<ParentFacts>,
    wanted: Option<LookupKey>,
    /// Counts the groups read, naming the facts in [`TargetKey`].
    facts_read: u64,
    /// The parent last picked, and what it was picked from.
    target: RefCell<Option<(TargetKey, Result<AddParent, String>)>>,
    /// A group's children space as the preview last reported it, by its
    /// layer id: `None` inside when it turns in 3D.
    space: Option<(String, Option<[f64; 6]>)>,
    /// The group whose space a frame was asked for (see
    /// [`space_request_due`]), until the facts are read again.
    asked: Option<String>,
    /// The selection of the element just added, and the reload that reads
    /// it: one requested at or after that number.
    pending: Option<(PendingSelect, u64)>,
    /// The added layer the preview found, selected on the next render.
    select_due: Option<String>,
    /// The added layer selected, and where its first lookup must put it.
    confirm: Option<(String, SourcePos)>,
    /// Why the last addition was not written, for the viewer's notice.
    pub(crate) notice: Option<String>,
}

impl EditorView {
    /// The selected layer's lookup, while it describes the code on screen.
    fn add_lookup(&self) -> Option<(&str, &LayerSourceReport, i64)> {
        let layer = self.selected_layer.as_deref()?;
        match &self.source_lookup {
            Some(SourceLookup::Found(report)) if !self.source_lookup_stale => {
                Some((layer, report, self.source_lookup_frame))
            }
            _ => None,
        }
    }

    /// Starts reading the groups of the selected layer's chain (see
    /// [`parent_facts`]) when its lookup has an answer they were not read
    /// for. Reads and parses the files on the background executor; once
    /// read, the parent's space is asked for (see [`Self::ask_add_space`]).
    pub(crate) fn sync_add_facts(&mut self, cx: &mut Context<Self>) {
        if !self.picks_layers() {
            return;
        }
        let Some((layer, report, frame)) = self.add_lookup() else {
            return;
        };
        let wanted = |(at, of, on): &LookupKey| at == layer && of == report && *on == frame;
        if self.add.wanted.as_ref().is_some_and(wanted) {
            return;
        }
        let report = report.clone();
        let lookup = (layer.to_owned(), report.clone(), frame);
        self.add.wanted = Some(lookup.clone());
        let chapters = self.react_chapters().unwrap_or_default().to_vec();
        let end = u64::try_from(self.clock.end_frame()).unwrap_or(0);
        let playhead = u64::try_from(frame).unwrap_or(0);
        let entry_dir = self
            .react_entry()
            .and_then(Path::parent)
            .map(Path::to_path_buf);
        cx.spawn(async move |view, cx| {
            let groups = cx
                .background_executor()
                .spawn(async move {
                    let real = |path: &Path| fs::canonicalize(path).unwrap_or(path.to_path_buf());
                    let entry_dir = entry_dir.map(|dir| real(&dir));
                    parent_facts(
                        |path| fs::read_to_string(path).ok(),
                        |file| {
                            entry_dir
                                .as_ref()
                                .is_some_and(|dir| inside_project(&real(file), dir))
                        },
                        &report,
                        &chapters,
                        end,
                        playhead,
                    )
                })
                .await;
            view.update(cx, |this, cx| {
                if this.add.wanted.as_ref() == Some(&lookup) {
                    this.add.set_facts(ParentFacts { lookup, groups });
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// The parent an addition goes into now and its facts; Err is why
    /// nothing can be added.
    fn add_target(&self) -> Result<(AddParent, &GroupFacts), String> {
        let selected = self.selected_layer.as_deref();
        let report = match &self.source_lookup {
            _ if selected.is_none() => None,
            Some(SourceLookup::Pending) => return Err(READING.to_owned()),
            Some(SourceLookup::Found(_)) if self.source_lookup_stale => {
                return Err(READING.to_owned());
            }
            Some(SourceLookup::Found(report)) => Some(&**report),
            _ => None,
        };
        let facts = match (report, &self.add.facts) {
            // Nothing selected, or no answer: no chain to pick from.
            (None, _) => return Err(NO_PARENT.to_owned()),
            (Some(report), Some(facts))
                if Some(facts.lookup.0.as_str()) == selected
                    && facts.lookup.1 == *report
                    && facts.lookup.2 == self.source_lookup_frame =>
            {
                facts
            }
            (Some(_), _) => return Err(READING.to_owned()),
        };
        let scope = match self.scope_context() {
            Some(context) => parent_scope(&context.state, self.remembered_scope()),
            // No scope control is offered: "This scene", as without one.
            None if self.element_edit.failed(selected) => ParentScope::NoControl,
            None => ParentScope::Unknown(READING.to_owned()),
        };
        let occurrences = self.occurrences.as_ref().map(|(_, state)| state);
        let chapters = self.react_chapters().unwrap_or_default();
        // The facts answer the report: they name it with the groups.
        let parent = self
            .add
            .target(&scope, self.occurrence_generation, chapters, || {
                pick_parent(
                    Some(&facts.lookup.1),
                    &scope,
                    |entry| facts.group(entry).is_some(),
                    |entry| is_shared(occurrences, chapters, &SourcePos::from(entry)),
                )
            })?;
        let facts = facts
            .group(&parent.entry)
            .ok_or_else(|| READING.to_owned())?;
        Ok((parent, facts))
    }

    /// The children space last reported for the group `layer_id`: `None`
    /// before the preview answered, `Some(None)` when it turns in 3D.
    fn add_space(&self, layer_id: &str) -> Option<Option<[f64; 6]>> {
        self.add
            .space
            .as_ref()
            .filter(|(id, _)| id == layer_id)
            .map(|(_, space)| *space)
    }

    /// The Add menu: the parent in its header, why its items are disabled.
    pub(crate) fn add_menu(&self) -> AddMenu {
        let target = self.add_target();
        let header = target
            .as_ref()
            .ok()
            .map(|(parent, facts)| add_header(parent, facts.component.as_deref()));
        let reason = match &target {
            Err(reason) => Some(reason.clone()),
            Ok((parent, facts)) => facts.timing.as_ref().err().cloned().or_else(|| {
                space_reason(self.add_space(&parent.layer_id), self.playing).map(str::to_owned)
            }),
        };
        let locks = target.map(|(_, facts)| facts.locks.as_slice());
        let items = menu_items(reason.as_deref(), |kind| {
            let locks = locks.as_ref().ok()?;
            let (_, lock) = locks.iter().find(|(of, _)| *of == kind)?;
            lock.clone()
        });
        AddMenu {
            header,
            reason,
            items,
        }
    }

    /// The group whose children space a frame asks for: the parent an
    /// addition goes into, while layers are picked and nothing plays.
    pub(crate) fn add_space_of(&self) -> Option<String> {
        if !self.picks_layers() || self.playing {
            return None;
        }
        self.add_target().ok().map(|(parent, _)| parent.layer_id)
    }

    /// Draws a frame asking for the parent's children space when it is
    /// due (see [`space_request_due`]): the parent became known or changed
    /// (the facts landed, the scope was switched, the targets or the scene
    /// scan answered, playback paused) and no frame asked for it since.
    pub(crate) fn ask_add_space(&mut self) {
        let parent = self.add_space_of();
        let reported = parent
            .as_deref()
            .is_some_and(|parent| self.add_space(parent).is_some());
        if space_request_due(parent.as_deref(), reported, self.add.asked.as_deref()) {
            self.add.asked = parent;
            self.refresh_preview();
        }
    }

    /// The parent whose added layer a frame asks for: on every frame while
    /// a selection is pending, until it is answered.
    pub(crate) fn add_added_under(&self) -> Option<AddedUnder> {
        if !self.picks_layers() || self.playing {
            return None;
        }
        let (pending, _) = self.add.pending.as_ref()?;
        Some(pending.under.clone())
    }

    fn step_add(&mut self, event: AddEvent) -> Option<(AddTool, Point, Point)> {
        let (mode, gesture) = add_step(std::mem::take(&mut self.add.mode), event);
        self.add.mode = mode;
        gesture
    }

    /// A kind chosen in the Add menu: playback stops, and an Image or a
    /// Video asks for its file first.
    pub(crate) fn choose_add_kind(&mut self, kind: NewKind, cx: &mut Context<Self>) {
        self.pause();
        self.add.notice = None;
        // The element goes into the parent shown now, or into none.
        let chosen = self.add_target().map(|(parent, facts)| ChosenParent {
            at: SourcePos::from(&parent.entry),
            label: parent_label(&parent, facts.component.as_deref()),
            layer_id: parent.layer_id,
        });
        match chosen {
            Ok(chosen) => self.add.chosen = Some(chosen),
            Err(reason) => {
                self.add.notice = Some(reason);
                cx.notify();
                return;
            }
        }
        self.step_add(AddEvent::Chosen(kind));
        if self.add.mode == AddMode::Picking(kind) {
            self.pick_add_file(kind, cx);
        }
        cx.notify();
    }

    /// Asks for the file of an Image or a Video, imports it into the
    /// project as `Choose…` does, then measures its aspect.
    fn pick_add_file(&mut self, kind: NewKind, cx: &mut Context<Self>) {
        let entry_dir = self.entry_dir();
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose".into()),
        });
        let picking = move |this: &EditorView| this.add.mode == AddMode::Picking(kind);
        // Nothing to draw: the mode ends, saying why when something failed.
        let cancel = move |this: &mut EditorView, notice: Option<String>| {
            if picking(this) {
                this.add.notice = notice;
                this.step_add(AddEvent::Picked(None));
            }
        };
        cx.spawn(async move |view, cx| {
            let chosen = match selection.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) | Err(_) => None,
                Ok(Err(error)) => {
                    let notice = format!("Couldn't show the file dialog: {error}");
                    view.update(cx, |this, cx| {
                        cancel(this, Some(notice));
                        cx.notify();
                    })
                    .ok();
                    return;
                }
            };
            let Some(chosen) = chosen else {
                view.update(cx, |this, cx| {
                    cancel(this, None);
                    cx.notify();
                })
                .ok();
                return;
            };
            let dir = entry_dir.clone();
            let imported = cx
                .background_executor()
                .spawn(async move { import_path(&dir, &dir, &chosen) })
                .await;
            let picked = view
                .update(cx, |this, cx| {
                    cx.notify();
                    match imported {
                        Ok(src) if picking(this) => {
                            this.step_add(AddEvent::Picked(Some(src.clone())));
                            Some(src)
                        }
                        Ok(_) => None,
                        Err(error) => {
                            cancel(this, Some(format!("Couldn't use the chosen file: {error}")));
                            None
                        }
                    }
                })
                .ok()
                .flatten();
            let Some(src) = picked else {
                return;
            };
            let path = entry_dir.join(&src);
            let aspect = cx
                .background_executor()
                .spawn(async move { media_aspect(kind, &path) })
                .await;
            view.update(cx, |this, cx| {
                this.step_add(AddEvent::Measured { src, aspect });
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The click distance in scene pixels: [`CLICK_SLOP`] viewer pixels.
    fn add_slop(&self) -> f64 {
        let scale = self
            .viewer_bounds
            .get()
            .and_then(|bounds| letterbox(bounds, self.frame_size()))
            .map_or(1.0, |(_, _, scale)| scale);
        f64::from(CLICK_SLOP / scale)
    }

    fn scene_position(&self, position: gpui_kit::Point<Pixels>) -> Option<Point> {
        to_scene(self.viewer_bounds.get()?, self.frame_size(), position)
    }

    /// A press on the canvas while an element is to be drawn: the drawing
    /// starts there when it is inside the frame. True when the add mode
    /// took the press.
    pub(crate) fn press_add(&mut self, position: gpui_kit::Point<Pixels>) -> bool {
        if !matches!(self.add.mode, AddMode::Armed(_)) {
            return false;
        }
        let bounds = self.viewer_bounds.get();
        if let Some(start) =
            bounds.and_then(|bounds| scene_point(bounds, self.frame_size(), position))
        {
            self.add.notice = None;
            self.step_add(AddEvent::Press(start));
        }
        true
    }

    /// Follows the pointer while an element is drawn; a move without the
    /// button ends the drawing there.
    pub(crate) fn continue_add(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if !matches!(self.add.mode, AddMode::Drawing { .. }) {
            return;
        }
        if self.release_unseen(event, cx, |this, cx| this.end_add(event.position, cx)) {
            return;
        }
        if let Some(now) = self.scene_position(event.position) {
            self.step_add(AddEvent::Move(now));
            cx.notify();
        }
    }

    /// Release: writes the drawn element. True when one was being drawn.
    pub(crate) fn end_add(
        &mut self,
        position: gpui_kit::Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        if !matches!(self.add.mode, AddMode::Drawing { .. }) {
            return false;
        }
        let event = match self.scene_position(position) {
            Some(end) => AddEvent::Release(end),
            None => AddEvent::Escape,
        };
        if let Some(gesture) = self.step_add(event) {
            self.write_addition(gesture, cx);
        }
        cx.notify();
        true
    }

    /// Escape: ends the add mode. True when it was on.
    pub(crate) fn escape_add(&mut self) -> bool {
        if self.add.mode == AddMode::Off {
            return false;
        }
        self.step_add(AddEvent::Escape);
        true
    }

    /// Ends the add mode: the selection changed, Select layers went off,
    /// playback started, or the code reloaded.
    pub(crate) fn end_add_mode(&mut self) {
        self.step_add(AddEvent::Ended);
    }

    /// The outline of the element being drawn, in scene pixels, while the
    /// parent's space is known.
    pub(crate) fn add_outline(&self) -> Option<(NewKind, Vec<Point>)> {
        let AddMode::Drawing { tool, start, now } = &self.add.mode else {
            return None;
        };
        // In the parent the hint names, where it is written.
        let space = self.add_space(&self.add.chosen.as_ref()?.layer_id)??;
        let outline = drawn_outline(
            tool.kind,
            (*start, *now),
            space,
            tool.aspect,
            self.add_slop(),
        );
        Some((tool.kind, outline))
    }

    /// Writes the drawn element into the parent as one history entry; once
    /// written, the reload selects it (see [`PendingSelect`]). Why nothing
    /// was written shows over the canvas.
    fn write_addition(
        &mut self,
        (tool, start, end): (AddTool, Point, Point),
        cx: &mut Context<Self>,
    ) {
        let chosen = self.add.chosen.take();
        let drawn = self.add_target().and_then(|(parent, facts)| {
            // Only into the parent the hint named while this was drawn.
            if !is_chosen(chosen.as_ref(), &parent) {
                return Err(PARENT_CHANGED.to_owned());
            }
            let timing = facts.timing.clone()?;
            let space = match self.add_space(&parent.layer_id) {
                Some(Some(space)) => space,
                Some(None) => return Err(TURNED_3D.to_owned()),
                None => return Err(READING.to_owned()),
            };
            let slop = self.add_slop();
            let points = (start, end);
            let new = drawn_geometry(
                tool.kind,
                tool.src.as_deref(),
                points,
                space,
                tool.aspect,
                slop,
            )
            .ok_or_else(|| SCALED_TO_ZERO.to_owned())?;
            Ok((parent, facts.tag.clone(), timing, new))
        });
        let (parent, tag, timing, new) = match drawn {
            Ok(drawn) => drawn,
            Err(reason) => {
                self.add.notice = Some(reason);
                return;
            }
        };
        let file = parent.entry.file.clone();
        let at = SourcePos::from(&parent.entry);
        let under = AddedUnder {
            parent: parent.layer_id,
            wrapped: timing.is_some(),
        };
        let inserted = Arc::new(Mutex::new(None));
        let written = Arc::clone(&inserted);
        let selected = self.selected_layer.clone();
        self.record_own_writes(
            vec![file.clone()],
            move |texts| {
                let (files, position) = add_files(texts, &file, &at, &tag, &new, timing.as_ref())?;
                *written.lock().unwrap_or_else(PoisonError::into_inner) = Some(position);
                Ok(files)
            },
            move |this, result, cx| {
                let position = inserted
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .take();
                match (result, position) {
                    // Not over a selection made while the file was written.
                    (Ok(()), Some(position)) if this.selected_layer == selected => {
                        let pending = PendingSelect {
                            position,
                            under,
                            from: None,
                        };
                        // The reload this write requests next reads it.
                        this.add.pending = Some((pending, this.react_reloads.next()));
                    }
                    (Ok(()), _) => {}
                    (Err(message), _) => this.add.notice = Some(message),
                }
                cx.notify();
            },
            cx,
        );
    }

    /// The preview's answers for an addition from preview `generation`: the
    /// parent's space, and the layer the pending selection waits for.
    pub(crate) fn take_add_answers(
        &mut self,
        generation: u64,
        space: Option<(String, Option<[f64; 6]>)>,
        added: Option<(String, Option<String>)>,
    ) {
        if space.is_some() {
            self.add.space = space;
        }
        let Some((pending, _)) = &self.add.pending else {
            return;
        };
        match added_fate(pending, generation, added.as_ref()) {
            AddedFate::Wait => {}
            AddedFate::Drop => self.add.pending = None,
            AddedFate::Select(layer) => {
                if let Some((pending, _)) = self.add.pending.take() {
                    self.add.confirm = Some((layer.clone(), pending.position));
                }
                self.add.select_due = Some(layer);
            }
        }
    }

    /// Selects the added layer the preview found, now that a context is at
    /// hand; not over a selection asked for meanwhile that waits for held
    /// commits.
    pub(crate) fn settle_added_selection(&mut self, cx: &mut Context<Self>) {
        let Some(layer) = self.add.select_due.take() else {
            return;
        };
        if self.element_rows.selection.waiting() {
            self.add.confirm = None;
            return;
        }
        self.select_layer(Some(layer), cx);
    }

    /// Whether a lookup of the selected `layer` may show: not when it is
    /// the added layer's first and puts the element elsewhere than where it
    /// was inserted.
    pub(crate) fn added_lookup_holds(&mut self, layer: &str, lookup: &LookupResult) -> bool {
        match self
            .add
            .confirm
            .take_if(|(added, _)| added.as_str() == layer)
        {
            Some((_, position)) => added_confirmed(&position, lookup),
            None => true,
        }
    }

    /// A reload of the React entry applied: the add mode ends, and the
    /// pending selection waits for this reload's first frame (preview
    /// generation `from`) when it reads the addition (`sequence`).
    pub(crate) fn add_reload_applied(&mut self, sequence: u64, from: u64) {
        self.end_add_mode();
        self.add.reload_applied(sequence, from);
    }

    /// Reload `sequence` failed (see [`AddState::reload_failed`]).
    pub(crate) fn add_reload_failed(&mut self, sequence: u64) {
        self.add.reload_failed(sequence);
    }

    /// The selection was cleared (see [`AddState::selection_changed`]).
    pub(crate) fn add_selection_changed(&mut self, layer: Option<&str>) {
        self.add.selection_changed(layer);
    }

    /// The added layer's check, taken while the selection changes.
    pub(crate) fn add_take_confirm(&mut self) -> Option<(String, SourcePos)> {
        self.add.confirm.take()
    }

    /// Puts the check back once a selection was made: it stays only when
    /// the added layer is the one selected (see
    /// [`AddState::selection_made`]).
    pub(crate) fn add_restore_confirm(&mut self, confirm: Option<(String, SourcePos)>) {
        self.add.confirm = confirm;
        self.add.selection_made(self.selected_layer.as_deref());
    }
}

impl AddState {
    /// The groups read for a lookup: the parent's space is asked for again,
    /// as the parent may be another group now.
    fn set_facts(&mut self, facts: ParentFacts) {
        self.facts = Some(facts);
        self.facts_read = self.facts_read.wrapping_add(1);
        self.asked = None;
    }

    /// The parent `pick` picks from the facts held, under `scope` with the
    /// scene scan of `occurrence_generation` and `chapters`; the last pick
    /// while none of them changed.
    fn target(
        &self,
        scope: &ParentScope,
        occurrence_generation: u64,
        chapters: &[ReactChapter],
        pick: impl FnOnce() -> Result<AddParent, String>,
    ) -> Result<AddParent, String> {
        let mut cache = self.target.borrow_mut();
        if let Some((key, picked)) = cache.as_ref()
            && key.facts_read == self.facts_read
            && key.scope == *scope
            && key.occurrence_generation == occurrence_generation
            && key.chapters == chapters
        {
            return picked.clone();
        }
        let picked = pick();
        let key = TargetKey {
            facts_read: self.facts_read,
            scope: scope.clone(),
            occurrence_generation,
            chapters: chapters.to_vec(),
        };
        *cache = Some((key, picked.clone()));
        picked
    }

    /// Reload `sequence` applied, its first frame being preview generation
    /// `from`. The groups and their spaces are read again from the
    /// reloaded code (a write to the parent may have moved or turned it):
    /// the facts once the layer is looked up in it, which then asks for the
    /// space. A selection waiting for a reload that reads its write waits
    /// for `from` on.
    fn reload_applied(&mut self, sequence: u64, from: u64) {
        self.facts = None;
        self.wanted = None;
        self.space = None;
        self.asked = None;
        if let Some((pending, after)) = &mut self.pending
            && sequence >= *after
            && pending.from.is_none()
        {
            pending.from = Some(from);
        }
    }

    /// Reload `sequence` failed: a selection waiting for a reload that
    /// reads its write is dropped, as none drew the element (an undo may
    /// take it out before the next reload, which would then answer with an
    /// older sibling).
    fn reload_failed(&mut self, sequence: u64) {
        if self
            .pending
            .as_ref()
            .is_some_and(|(_, after)| sequence >= *after)
        {
            self.pending = None;
        }
    }

    /// The selection became `layer`: a check waiting for another layer's
    /// first lookup goes, as that layer is not selected after all. A
    /// reload does not drop it: a selection parked behind held commits is
    /// made after their reload, and is checked then.
    fn selection_changed(&mut self, layer: Option<&str>) {
        if self
            .confirm
            .as_ref()
            .is_some_and(|(added, _)| Some(added.as_str()) != layer)
        {
            self.confirm = None;
        }
    }

    /// A selection of `layer` was made (a click, the layer list, Select
    /// layers turned off): as [`Self::selection_changed`], and a selection
    /// waiting for the reload goes, so the added layer is not selected
    /// over it.
    fn selection_made(&mut self, layer: Option<&str>) {
        self.pending = None;
        self.selection_changed(layer);
    }
}

/// Whether `parent` is the one shown when the drawn kind was chosen.
fn is_chosen(chosen: Option<&ChosenParent>, parent: &AddParent) -> bool {
    chosen.is_some_and(|chosen| {
        chosen.at == SourcePos::from(&parent.entry) && chosen.layer_id == parent.layer_id
    })
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
        // The scene's own group is above both calls.
        assert_eq!(is_shared(Some(&twice), &chapters, &scene), Ok(false));

        // One chain, showing in two scenes.
        let spanning = index(vec![instance(
            vec![rect, group.clone(), first_call, scene],
            vec![(30, 90)],
        )]);
        assert_eq!(is_shared(Some(&spanning), &chapters, &group), Ok(true));

        // Design 3's notes, as the scope control shows them.
        assert_eq!(
            is_shared(None, &chapters, &group),
            Err("The scenes are not scanned".to_owned())
        );
        assert_eq!(
            is_shared(Some(&OccurrenceState::Scanning), &chapters, &group),
            Err("Scanning scenes…".to_owned())
        );
        let failed = OccurrenceState::Failed("x".to_owned());
        assert_eq!(
            is_shared(Some(&failed), &chapters, &group),
            Err("Couldn't scan scenes: x".to_owned())
        );
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
            called_here: None,
            drawn_alone: false,
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
        // The armed hint names it the same way.
        assert_eq!(
            parent_label(&parent, Some("PriceTag")),
            "`<Group>` in `<PriceTag>`"
        );
    }

    #[test]
    fn a_write_goes_only_into_the_chosen_parent() {
        let parent = AddParent {
            entry: primitive("Group", &pos("intro", 5), "g1"),
            index: 1,
            layer_id: "g1".to_owned(),
            this_scene: false,
        };
        let chosen = ChosenParent {
            at: pos("intro", 5),
            layer_id: "g1".to_owned(),
            label: "`<Group>`".to_owned(),
        };
        assert!(is_chosen(Some(&chosen), &parent));
        assert!(!is_chosen(None, &parent));
        // A scope change or a seek picked another group.
        let other = AddParent {
            entry: primitive("Group", &pos("intro", 3), "g0"),
            layer_id: "g0".to_owned(),
            ..parent.clone()
        };
        assert!(!is_chosen(Some(&chosen), &other));
        // The same source drawn as another instance.
        let instance = AddParent {
            layer_id: "g2".to_owned(),
            ..parent
        };
        assert!(!is_chosen(Some(&chosen), &instance));
    }

    /// A state waiting to select the element added by the write read by
    /// reload 4, with the parent's facts and space read before it.
    fn waiting_state() -> AddState {
        let report = report(vec![primitive("Group", &pos("intro", 5), "g")], vec![]);
        AddState {
            facts: Some(ParentFacts {
                lookup: ("g".to_owned(), report.clone(), 30),
                groups: vec![],
            }),
            wanted: Some(("g".to_owned(), report, 30)),
            space: Some(("g".to_owned(), Some(IDENTITY))),
            pending: Some((
                PendingSelect {
                    position: pos("intro", 6),
                    under: AddedUnder {
                        parent: "g".to_owned(),
                        wrapped: false,
                    },
                    from: None,
                },
                4,
            )),
            confirm: Some(("g.1".to_owned(), pos("intro", 6))),
            ..AddState::default()
        }
    }

    fn pending_from(state: &AddState) -> Option<Option<u64>> {
        state.pending.as_ref().map(|(pending, _)| pending.from)
    }

    #[test]
    fn a_reload_reads_the_parent_and_its_space_again() {
        let mut state = waiting_state();
        // An older reload: it does not read the write.
        state.reload_applied(3, 20);
        assert_eq!(pending_from(&state), Some(None));
        // The parent may have moved or turned: nothing read before stays.
        assert!(state.facts.is_none() && state.wanted.is_none() && state.space.is_none());

        state.reload_applied(4, 21);
        assert_eq!(pending_from(&state), Some(Some(21)));
        // A later one does not move it.
        state.reload_applied(5, 22);
        assert_eq!(pending_from(&state), Some(Some(21)));
        // The added layer's check outlives reloads.
        assert!(state.confirm.is_some());
    }

    #[test]
    fn a_failed_reload_drops_the_selection_waiting_for_it() {
        let mut state = waiting_state();
        state.reload_failed(3);
        assert!(state.pending.is_some());
        state.reload_failed(4);
        assert!(state.pending.is_none());
    }

    #[test]
    fn the_added_layer_s_check_goes_with_another_selection() {
        let mut state = waiting_state();
        state.selection_changed(Some("g.1"));
        assert!(state.confirm.is_some());
        state.selection_changed(Some("g.0"));
        assert!(state.confirm.is_none());

        let mut cleared = waiting_state();
        cleared.selection_changed(None);
        assert!(cleared.confirm.is_none());
    }

    #[test]
    fn a_selection_made_drops_the_one_waiting_for_the_reload() {
        // A selection cleared on the way (a lookup, a reload) keeps it.
        let mut cleared = waiting_state();
        cleared.selection_changed(None);
        assert!(cleared.pending.is_some());

        // One made, a layer clicked or none: the added layer is not
        // selected over it.
        for layer in [Some("g.0"), None] {
            let mut state = waiting_state();
            state.selection_made(layer);
            assert!(state.pending.is_none() && state.confirm.is_none());
        }
        // Selecting the added layer keeps its check.
        let mut added = waiting_state();
        added.selection_made(Some("g.1"));
        assert!(added.confirm.is_some());
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
    fn windows_up_to_the_last_one_below_the_parent_are_dropped() {
        let own = pos("intro", 9);
        let inner = pos("intro", 8);
        let groups = pos("intro", 5);
        let report = report(
            vec![
                primitive("Rect", &pos("intro", 10), "r"),
                sequence(&own),
                sequence(&inner),
                primitive("Group", &pos("intro", 6), "g"),
                sequence(&groups),
            ],
            vec![
                window(330.0, 390.0, Some(&own)),
                // Without a source, between two windows below the parent.
                window(320.0, 400.0, None),
                window(310.0, 410.0, Some(&inner)),
                window(300.0, 420.0, Some(&groups)),
            ],
        );
        assert_eq!(
            report_at_parent(&report, 3).windows,
            vec![window(300.0, 420.0, Some(&groups))]
        );
        // Under the selected group itself, every window stays.
        assert_eq!(report_at_parent(&report, 0), report);
    }

    #[test]
    fn less_than_a_frame_left_runs_one_frame() {
        let around = pos("intro", 5);
        let chain = vec![
            primitive("Rect", &pos("intro", 7), "r"),
            primitive("Group", &pos("intro", 6), "g"),
            sequence(&around),
        ];
        let windowed = report(chain.clone(), vec![window(330.0, 360.0, Some(&around))]);
        let chapters = [chapter("Intro", 300, 120)];
        // At the window's end.
        assert_eq!(
            added_timing(&windowed, 1, &chapters, 960, 360),
            Ok(Some(Timing {
                from: 30,
                duration: 1
            }))
        );
        // A window cut to nothing at the composition's end, on its first
        // frame: still one frame, not the whole window.
        let cut = report(chain.clone(), vec![window(600.0, 660.0, Some(&around))]);
        assert_eq!(
            added_timing(&cut, 1, &[], 600, 600),
            Ok(Some(Timing {
                from: 0,
                duration: 1
            }))
        );
        // Without a window, at the composition's end.
        let bare = report(chain, vec![]);
        assert_eq!(
            added_timing(&bare, 1, &[], 600, 600),
            Ok(Some(Timing {
                from: 600,
                duration: 1
            }))
        );
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
        let no_lock = |_: NewKind| None;
        let kinds = [
            NewKind::Rect,
            NewKind::Ellipse,
            NewKind::Circle,
            NewKind::Text,
            NewKind::Image,
            NewKind::Video,
        ];

        // Ellipse and Circle are enabled whether or not the file imports
        // `@celesta/shapes` yet: adding one imports it.
        let open = menu_items(None, no_lock);
        assert_eq!(open, kinds.map(|kind| (kind, None)).to_vec());

        let reason = "Can't draw into a group turned in 3D";
        let disabled = menu_items(Some(reason), |_| Some("clash".to_owned()));
        assert_eq!(
            disabled,
            kinds.map(|kind| (kind, Some(reason.to_owned()))).to_vec()
        );

        let clash = AddLock::Import(ImportLock::Clash {
            name: "Rect".to_owned(),
        })
        .text();
        let clashing = menu_items(None, |kind| (kind == NewKind::Rect).then(|| clash.clone()));
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
    fn a_parent_s_space_is_asked_for_once_until_reported() {
        // A parent whose space is not reported, not asked for yet.
        assert!(space_request_due(Some("g"), false, None));
        // Reported: nothing to ask.
        assert!(!space_request_due(Some("g"), true, None));
        assert!(!space_request_due(Some("g"), true, Some("g")));
        // Asked for and not answered yet: not again.
        assert!(!space_request_due(Some("g"), false, Some("g")));
        // Another parent (a scope switched, targets resolved): due again.
        assert!(space_request_due(Some("h"), false, Some("g")));
        // No parent.
        assert!(!space_request_due(None, false, None));
        assert!(!space_request_due(None, false, Some("g")));
    }

    #[test]
    fn an_unreported_space_disables_the_items_only_while_paused() {
        // Paused: the space is asked for now, and a drawing needs it.
        assert_eq!(space_reason(None, false), Some(READING));
        // Playing: choosing an item pauses, which asks for it.
        assert_eq!(space_reason(None, true), None);
        for playing in [false, true] {
            assert_eq!(space_reason(Some(Some(IDENTITY)), playing), None);
            assert_eq!(space_reason(Some(None), playing), Some(TURNED_3D));
            assert_eq!(
                space_reason(Some(Some([0.0; 6])), playing),
                Some(SCALED_TO_ZERO)
            );
        }
    }

    #[test]
    fn a_reload_or_new_facts_ask_for_the_space_again() {
        let mut state = AddState {
            asked: Some("g".to_owned()),
            ..waiting_state()
        };
        state.reload_applied(3, 20);
        assert!(state.asked.is_none());

        let mut landed = AddState {
            asked: Some("g".to_owned()),
            ..waiting_state()
        };
        landed.set_facts(ParentFacts {
            lookup: ("g".to_owned(), report(vec![], vec![]), 31),
            groups: vec![],
        });
        assert!(landed.asked.is_none());
    }

    #[test]
    fn the_parent_is_picked_again_only_when_what_it_reads_changes() {
        let mut state = waiting_state();
        let picks = std::cell::Cell::new(0);
        let picked = || {
            picks.set(picks.get() + 1);
            Err::<AddParent, _>(NO_PARENT.to_owned())
        };
        let shared = ParentScope::Chosen(Scope::Shared);
        let scene = ParentScope::Chosen(Scope::ThisScene);
        let chapters = [chapter("intro", 0, 60)];
        let other_chapters = [chapter("intro", 0, 90)];

        assert_eq!(
            state.target(&shared, 1, &chapters, picked),
            Err(NO_PARENT.to_owned())
        );
        assert_eq!(picks.get(), 1);
        // The same inputs: the pick is reused, also on the next render.
        assert_eq!(
            state.target(&shared, 1, &chapters, picked),
            Err(NO_PARENT.to_owned())
        );
        assert_eq!(picks.get(), 1);
        // The scope, the scene scan and the chapters each pick again.
        state.target(&scene, 1, &chapters, picked).ok();
        assert_eq!(picks.get(), 2);
        state.target(&scene, 2, &chapters, picked).ok();
        assert_eq!(picks.get(), 3);
        state.target(&scene, 2, &other_chapters, picked).ok();
        assert_eq!(picks.get(), 4);
        // So do groups read again, for the same lookup too.
        let lookup = state.facts.as_ref().unwrap().lookup.clone();
        state.set_facts(ParentFacts {
            lookup,
            groups: vec![],
        });
        state.target(&scene, 2, &other_chapters, picked).ok();
        assert_eq!(picks.get(), 5);
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
        let measured = |src: &str, aspect| AddEvent::Measured {
            src: src.to_owned(),
            aspect,
        };
        assert_eq!(
            step(armed.clone(), measured("a.mp4", Some(1.5))),
            (
                AddMode::Armed(tool(NewKind::Video, Some("a.mp4"), Some(1.5))),
                None
            )
        );
        // A probe of a file chosen before lands on no other tool.
        assert_eq!(
            step(armed.clone(), measured("b.mp4", Some(1.5))),
            (armed.clone(), None)
        );
        let rect = AddMode::Armed(tool(NewKind::Rect, None, None));
        assert_eq!(
            step(rect.clone(), measured("a.mp4", Some(1.5))),
            (rect, None)
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
            step(drawing(now, None), measured("a.mp4", Some(2.0))),
            (drawing(now, Some(2.0)), None)
        );
        assert_eq!(
            step(drawing(now, None), measured("b.mp4", Some(2.0))),
            (drawing(now, None), None)
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
