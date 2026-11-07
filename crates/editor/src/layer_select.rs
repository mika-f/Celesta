//! Selecting a layer in the React preview: mapping a click on the viewer to
//! scene pixels, and what the Layer section shows for the selection.

use celesta_composition::Point;
use celesta_react_bridge::{ElementKind, LayerSource, LayerSourceReport};
use gpui_kit::{Bounds, Pixels};
use std::path::Path;

/// What looking up a layer's source returned: `Ok(None)` when the frame has
/// no layer with that id, `Ok(Some(report))` with the JSX chain and the props
/// the runtime saw otherwise.
pub(crate) type LookupResult = Result<Option<LayerSourceReport>, String>;

/// Where the selected layer's source lookup stands. The report is boxed: it
/// is about ten times the size of the other variants.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SourceLookup {
    Pending,
    NotInFrame,
    NoPosition,
    Found(Box<LayerSourceReport>),
    Failed(String),
}

/// Where the frame sits when it is fitted, aspect kept, into `bounds`:
/// `(left, top, scale)`, the left and top in the same space as `bounds`.
/// `None` for a frame or an area without size.
pub(crate) fn letterbox(
    bounds: Bounds<Pixels>,
    (frame_width, frame_height): (f32, f32),
) -> Option<(f32, f32, f32)> {
    let area_width = f32::from(bounds.size.width);
    let area_height = f32::from(bounds.size.height);
    if frame_width <= 0.0 || frame_height <= 0.0 || area_width <= 0.0 || area_height <= 0.0 {
        return None;
    }
    let scale = (area_width / frame_width).min(area_height / frame_height);
    let left = f32::from(bounds.origin.x) + (area_width - frame_width * scale) / 2.0;
    let top = f32::from(bounds.origin.y) + (area_height - frame_height * scale) / 2.0;
    Some((left, top, scale))
}

/// The scene pixel under `position` in the frame fitted into `bounds`, or
/// `None` when `position` is outside the frame (on a letterbox bar).
pub(crate) fn scene_point(
    bounds: Bounds<Pixels>,
    frame: (f32, f32),
    position: gpui_kit::Point<Pixels>,
) -> Option<Point> {
    let Point { x, y } = to_scene(bounds, frame, position)?;
    ((0.0..=f64::from(frame.0)).contains(&x) && (0.0..=f64::from(frame.1)).contains(&y))
        .then_some(Point { x, y })
}

/// `position` in scene pixels of the frame fitted into `bounds`, also when it
/// is outside the frame (a drag that leaves it).
pub(crate) fn to_scene(
    bounds: Bounds<Pixels>,
    frame: (f32, f32),
    position: gpui_kit::Point<Pixels>,
) -> Option<Point> {
    let (left, top, scale) = letterbox(bounds, frame)?;
    Some(Point {
        x: f64::from((f32::from(position.x) - left) / scale),
        y: f64::from((f32::from(position.y) - top) / scale),
    })
}

/// What the Layer section shows for a lookup's outcome.
pub(crate) fn source_lookup(result: LookupResult) -> SourceLookup {
    match result {
        Ok(None) => SourceLookup::NotInFrame,
        Ok(Some(report)) if report.chain.is_empty() => SourceLookup::NoPosition,
        Ok(Some(report)) => SourceLookup::Found(Box::new(report)),
        Err(error) => SourceLookup::Failed(error),
    }
}

/// The selected layer to look up the source of in the next frame, if any.
/// Only while picking layers (interactive mode); the source follows the
/// paused frame, and during playback the panel keeps the last answer unless
/// the selection changed and its lookup has not been answered yet.
pub(crate) fn source_lookup_target(
    selected: Option<&str>,
    picking: bool,
    playing: bool,
    lookup: Option<&SourceLookup>,
) -> Option<String> {
    selected
        .filter(|_| picking && (!playing || lookup == Some(&SourceLookup::Pending)))
        .map(str::to_owned)
}

/// Whether a frame may ask for the selected layer's source while a value is
/// being changed: not during a canvas drag (a released one until its
/// reload), nor while a row's scrub is pressed. A released scrub's frame
/// asks, so the targets its held write waits for get resolved. The runtime
/// looks the source up without the overrides after drawing the frame with
/// them, so the drawn frame keeps the scrubbed value. Nor while a text is
/// edited on the canvas (`text_edit`): a keystroke asks the runtime
/// nothing. A committed text, drawn until its reload, asks as a released
/// scrub does.
pub(crate) fn lookup_allowed(transform_drag: bool, scrub_pressed: bool, text_edit: bool) -> bool {
    !transform_drag && !scrub_pressed && !text_edit
}

/// Whether the outcome of opening a source belongs in the panel: `started`
/// is the session and selected layer when the open began, `now` the ones the
/// panel shows. A late error must not appear under another layer.
pub(crate) fn launch_outcome_applies(
    started: (u64, Option<&str>),
    now: (u64, Option<&str>),
) -> bool {
    started == now
}

/// `file` relative to the entry's directory when it is inside it, and in
/// full otherwise.
pub(crate) fn display_path(file: &Path, entry_dir: &Path) -> String {
    file.strip_prefix(entry_dir)
        .unwrap_or(file)
        .display()
        .to_string()
}

/// `file:line:column` for `source`, the file as [`display_path`] shows it.
pub(crate) fn source_position(source: &LayerSource, entry_dir: &Path) -> String {
    format!(
        "{}:{}:{}",
        display_path(&source.file, entry_dir),
        source.line,
        source.column
    )
}

/// The layer the user can select from a chain entry: the layer a primitive
/// (a `<Group>` or `<Stack>`) emitted, or the root of a component's
/// instance, the one layer its call draws everything in. A wrapper
/// (`<Sequence>`) has none.
pub(crate) fn parent_layer_id(entry: &LayerSource) -> Option<&str> {
    match entry.kind {
        ElementKind::Primitive | ElementKind::Component => entry.layer_id.as_deref(),
        ElementKind::Wrapper => None,
    }
}

/// The chain entry that calls the component enclosing the selected element:
/// the nearest `Component` entry above it whose `name` equals `component`.
/// Wrappers, and components the element is only passed to as children, are
/// skipped. When none matches, the nearest component the runtime calls
/// "Anonymous" (`memo` or `forwardRef` of an arrow function has no name).
pub(crate) fn owner_call_site<'a>(
    report: &'a LayerSourceReport,
    component: &str,
) -> Option<&'a LayerSource> {
    let components = || {
        report
            .chain
            .iter()
            .skip(1)
            .filter(|entry| entry.kind == ElementKind::Component)
    };
    components()
        .find(|entry| entry.name == component)
        .or_else(|| components().find(|entry| entry.name == "Anonymous"))
}

/// The quads to outline for the selection: its own hit regions, or, for a
/// group that has none, the box `layer_frame` measured around its children.
pub(crate) fn outline_quads(
    regions: Vec<[Point; 4]>,
    frame: Option<[Point; 4]>,
) -> Vec<[Point; 4]> {
    if regions.is_empty() {
        frame.into_iter().collect()
    } else {
        regions
    }
}

/// The root of a component instance a chain entry is, if it is one: a
/// component the runtime reported a single root layer for.
fn instance_root(entry: &LayerSource) -> Option<&str> {
    (entry.kind == ElementKind::Component)
        .then_some(entry.layer_id.as_deref())
        .flatten()
}

/// The layer a click with the Instance pick mode selects instead of the
/// layer it hit, or `None` to keep the hit layer (see
/// `docs/handoff/2026-11-07-instance-select.md`). `hit` is the hit layer's
/// report; `selected` and `selected_id` are the selection's before the
/// click. The context is the instance just outside the selection, or the
/// selection itself when `descend` (a double-click): a hit inside the
/// context's root picks the outermost instance inside it, any other the
/// outermost instance of all. An instance sharing the context's root
/// (a component passing one child through) is not inside it.
pub(crate) fn instance_target(
    hit: &LayerSourceReport,
    selected: Option<&LayerSourceReport>,
    selected_id: Option<&str>,
    descend: bool,
) -> Option<String> {
    let context = if descend {
        selected_id
    } else {
        selected.and_then(|report| {
            report
                .chain
                .iter()
                .filter_map(instance_root)
                .find(|root| Some(*root) != selected_id)
        })
    };
    // Chains are innermost first: inside the context is before its root.
    let bound = context
        .and_then(|root| {
            hit.chain
                .iter()
                .position(|entry| entry.layer_id.as_deref() == Some(root))
        })
        .unwrap_or(hit.chain.len());
    let target = hit.chain[..bound]
        .iter()
        .rev()
        .filter_map(instance_root)
        .find(|root| Some(*root) != context)?;
    (hit.chain
        .first()
        .and_then(|entry| entry.layer_id.as_deref())
        != Some(target))
    .then(|| target.to_owned())
}

/// What a click on the canvas selects: the layer under the pointer, or the
/// component instance around it (the tool row's Select [Layer | Instance]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum PickMode {
    #[default]
    Layer,
    Instance,
}

/// How one press on the canvas picks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Press {
    /// The layer under the pointer.
    Layer,
    /// An instance around it, one level inside the selection when
    /// `descend` (a double-click).
    Instance { descend: bool },
}

/// How a press picks: with `PickMode::Instance` an instance, unless the
/// platform modifier (Cmd on macOS, Ctrl elsewhere) is held. A descent
/// needs no test of the hit being inside the selection: `instance_target`
/// finds no context in the hit's chain then and picks as a plain click.
pub(crate) fn press_pick(mode: PickMode, secondary: bool, double: bool) -> Press {
    match mode {
        PickMode::Instance if !secondary => Press::Instance { descend: double },
        _ => Press::Layer,
    }
}

/// A click with `PickMode::Instance` waiting for its hit layer's source to
/// move the selection to the instance around it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PendingInstance {
    /// The layer the click hit, selected meanwhile.
    pub(crate) hit: String,
    /// The selection's report before the click, when it had been found.
    pub(crate) selected: Option<Box<LayerSourceReport>>,
    /// The selection before the click.
    pub(crate) selected_id: Option<String>,
    pub(crate) descend: bool,
    /// A double-click's second press came before this pick resolved: go
    /// one level inside what it picks.
    pub(crate) then_descend: bool,
}

/// The pick a press leaves waiting, given the one already waiting: a
/// double-click's second press on the same hit, before the first one's
/// lookup answered, keeps the first pick and goes inside what it picks
/// (the selection it would descend from is not known yet). Any other
/// press replaces it.
pub(crate) fn merge_pending(
    waiting: Option<PendingInstance>,
    press: PendingInstance,
) -> PendingInstance {
    match waiting {
        Some(first) if press.descend && first.hit == press.hit => PendingInstance {
            then_descend: true,
            ..first
        },
        _ => press,
    }
}

/// The waiting pick when `answered` is the layer it hit; a lookup for any
/// other layer (the old selection, while the change to the hit waits for a
/// held commit) leaves it waiting.
pub(crate) fn take_pending_for(
    waiting: &mut Option<PendingInstance>,
    answered: &str,
) -> Option<PendingInstance> {
    waiting.take_if(|pending| pending.hit == answered)
}

/// A resolved pick waiting for the next render to select `target`, which
/// it does only while `hit` is still the selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InstanceDue {
    pub(crate) hit: String,
    pub(crate) target: String,
}

/// The layer `due` selects, or `None` when the selection moved off its hit
/// meanwhile (an added layer selected in the same render, say).
pub(crate) fn due_target<'a>(due: &'a InstanceDue, selected: Option<&str>) -> Option<&'a str> {
    (selected == Some(due.hit.as_str())).then_some(due.target.as_str())
}

/// Where a pending instance pick moves the selection once the selected
/// layer's lookup answered `lookup`: `None` when the hit layer is no longer
/// selected, the lookup found no chain, or the hit layer is the pick.
pub(crate) fn pending_instance_fate(
    pending: &PendingInstance,
    selected: Option<&str>,
    lookup: &SourceLookup,
) -> Option<String> {
    let SourceLookup::Found(report) = lookup else {
        return None;
    };
    if selected != Some(pending.hit.as_str()) {
        return None;
    }
    let first = instance_target(
        report,
        pending.selected.as_deref(),
        pending.selected_id.as_deref(),
        pending.descend,
    )?;
    if pending.then_descend {
        // Inside the first pick; nothing inside it keeps the hit.
        return instance_target(report, None, Some(&first), true);
    }
    Some(first)
}
