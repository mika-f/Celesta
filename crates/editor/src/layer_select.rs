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

/// Where the selected layer's source lookup stands.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SourceLookup {
    Pending,
    NotInFrame,
    NoPosition,
    Found(LayerSourceReport),
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
        Ok(Some(report)) => SourceLookup::Found(report),
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

/// The layer a chain entry emitted, when it is a primitive (a `<Group>` or
/// `<Stack>`) the user can select as a parent.
pub(crate) fn parent_layer_id(entry: &LayerSource) -> Option<&str> {
    match entry.kind {
        ElementKind::Primitive => entry.layer_id.as_deref(),
        ElementKind::Wrapper => None,
        ElementKind::Component => None,
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
