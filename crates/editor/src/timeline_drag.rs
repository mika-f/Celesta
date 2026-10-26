//! Dragging the selected element's own timing window on the timeline: the
//! band holding the lookup frame takes grips at the window's edges; a drag
//! moves the bands and a label with the pointer, writes nothing until the
//! button comes up, and then writes the window's props as one history
//! entry. Every rule sits in a pure helper here. A chapter's end drags the
//! same way on the chapter row (its rules in `scene_entries`). An element
//! without a window of its own is offered "Set timing" there instead (its
//! rules in `timing`).

use crate::element_edit::{
    CommitError, ElementTargets, PropKey, WindowTargets, editable, inner_target, reason_text,
    window_files,
};
use crate::impact::{ImpactFilter, ImpactView, moved_impact_view};
use crate::layer_select::SourceLookup;
use crate::scene_entries::{
    SceneEdge, clamp_scene_delta, preview_chapters, scene_files, scene_snap_targets,
    writes_nothing, written_length,
};
use crate::scope::{
    CommitGate, SceneCalls, Scope, ScopeState, commit_scope, gesture_gate, impact_filter,
    scope_in_effect, scope_state,
};
use crate::timing::{
    SetTimingOffer, WindowRole, set_timing_files, set_timing_offer, set_timing_span, window_role,
};
use crate::view::EditorView;
use celesta_react_bridge::{
    ElementOccurrences, InstanceOccurrences, LayerSourceReport, ReactChapter, SourcePos,
};
use celesta_source_edit::{EditTarget, EditValue, LockReason, TimingKind, format_number};
use gpui_kit::{Context, MouseMoveEvent};

/// What part of the window a drag holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BandGrip {
    Start,
    End,
    Body,
}

/// A window's timing values in frames.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TimingValues {
    /// `from` of a `Sequence`, `offset` of a `Series` item.
    pub(crate) start: f64,
    pub(crate) duration: f64,
}

/// What a running drag moves, for the bands drawn meanwhile.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DragPreview {
    /// Where the dragged wrapper is written.
    pub(crate) source: SourcePos,
    /// The window's frames when the drag started.
    pub(crate) window: (f64, f64),
    pub(crate) grip: BandGrip,
    /// Whole frames.
    pub(crate) delta: i64,
    /// The `Series` items after the dragged one.
    pub(crate) followers: Vec<SourcePos>,
}

/// What the button coming up (or Escape) does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DragEnd {
    Seek,
    Cancel,
    Commit,
}

/// How far inside a band its edge grips reach, in pixels.
const EDGE_GRIP: f32 = 6.0;

/// Edges snap to a target this many pixels away or nearer.
const SNAP_DISTANCE: f64 = 6.0;

/// A press that moves less than this many pixels is a click.
const CLICK_DISTANCE: f32 = 3.0;

const DURATION: &str = "durationInFrames";

/// The prop a window's start is written in.
fn start_prop(kind: TimingKind) -> &'static str {
    match kind {
        TimingKind::Sequence => "from",
        TimingKind::SeriesSequence => "offset",
    }
}

/// The props `grip` writes, in the order they are written.
fn grip_props(kind: TimingKind, grip: BandGrip) -> Vec<&'static str> {
    match grip {
        BandGrip::Start => vec![start_prop(kind), DURATION],
        BandGrip::End => vec![DURATION],
        BandGrip::Body => vec![start_prop(kind)],
    }
}

/// The band (index into `bands`, frames) a drag starts on: the one holding
/// `frame`.
pub(crate) fn draggable_band(bands: &[(f64, f64)], frame: f64) -> Option<usize> {
    bands
        .iter()
        .position(|&(from, to)| from <= frame && frame < to)
}

/// How far an edge grip of a band `width` pixels wide reaches inside it.
pub(crate) fn edge_width(width: f32) -> f32 {
    EDGE_GRIP.min(width / 2.0)
}

/// The grip at window x `x` on a band `width` pixels wide from `left`: the
/// edges reach [`EDGE_GRIP`] inside it, the rest is the body; a band
/// narrower than two edges splits into a start half and an end half.
pub(crate) fn grip_at(x: f32, left: f32, width: f32) -> BandGrip {
    let edge = edge_width(width);
    let local = x - left;
    if local < edge {
        BandGrip::Start
    } else if local >= width - edge {
        BandGrip::End
    } else {
        BandGrip::Body
    }
}

/// Why `grip` can't drag (design 2's reason of the first locked prop it
/// needs).
pub(crate) fn grip_lock(
    kind: TimingKind,
    grip: BandGrip,
    targets: &ElementTargets,
) -> Option<String> {
    grip_props(kind, grip).into_iter().find_map(|name| {
        let target = targets.target(&PropKey::Prop(name.to_owned()))?;
        match inner_target(target) {
            _ if editable(target) => None,
            EditTarget::Locked(reason) => Some(reason_text(reason)),
            _ => Some(format!("Can't write `{name}`")),
        }
    })
}

/// The number `targets` holds for `name`, when it holds one.
fn number_of(targets: &ElementTargets, name: &str) -> Option<f64> {
    match inner_target(targets.target(&PropKey::Prop(name.to_owned()))?) {
        EditTarget::Literal {
            value: EditValue::Number(value),
            ..
        } => Some(*value),
        EditTarget::Keyframe { value, .. } => Some(*value),
        _ => None,
    }
}

/// The current values, a missing start 0 and a missing duration `window`'s
/// length. A locked prop reads as a missing one: the grips that need it
/// don't drag.
pub(crate) fn timing_values(
    kind: TimingKind,
    targets: &ElementTargets,
    window: (f64, f64),
) -> TimingValues {
    TimingValues {
        start: number_of(targets, start_prop(kind)).unwrap_or(0.0),
        duration: number_of(targets, DURATION).unwrap_or((window.1 - window.0).round()),
    }
}

/// `delta` within the bounds: duration ≥ 1, and a `Series` item's start
/// (`series_start`) ≥ 0.
pub(crate) fn clamp_band_delta(
    grip: BandGrip,
    delta: i64,
    values: TimingValues,
    series_start: Option<f64>,
) -> i64 {
    // The most a start may move back, and the most a duration may give.
    let back = series_start.map(|start| -(start.round() as i64));
    let spare = values.duration.round() as i64 - 1;
    match grip {
        BandGrip::Start => {
            let delta = delta.min(spare);
            back.map_or(delta, |back| delta.max(back))
        }
        BandGrip::End => delta.max(-spare),
        BandGrip::Body => back.map_or(delta, |back| delta.max(back)),
    }
}

/// The values to commit for `grip` and `delta` (the edit table).
pub(crate) fn band_edit(
    kind: TimingKind,
    grip: BandGrip,
    delta: i64,
    values: TimingValues,
) -> Vec<(PropKey, Option<EditValue>)> {
    let after = moved_values(grip, delta, values);
    grip_props(kind, grip)
        .into_iter()
        .map(|name| {
            let value = if name == DURATION {
                after.duration
            } else {
                after.start
            };
            (
                PropKey::Prop(name.to_owned()),
                Some(EditValue::Number(value)),
            )
        })
        .collect()
}

/// What a drag of `grip` by `delta` writes into the window `targets`
/// describes: [`band_edit`], and for a body drag of a `Sequence` without
/// `durationInFrames`, that prop at the length the window shows. The
/// runtime keeps such a window's end at its parent's end, so moving its
/// start alone would change its length against the preview.
pub(crate) fn band_writes(
    kind: TimingKind,
    grip: BandGrip,
    delta: i64,
    values: TimingValues,
    targets: &ElementTargets,
) -> Vec<(PropKey, Option<EditValue>)> {
    let mut writes = band_edit(kind, grip, delta, values);
    let duration = PropKey::Prop(DURATION.to_owned());
    let missing = targets
        .target(&duration)
        .is_some_and(|target| matches!(inner_target(target), EditTarget::Missing { .. }));
    if kind == TimingKind::Sequence && grip == BandGrip::Body && missing {
        writes.push((duration, Some(EditValue::Number(values.duration))));
    }
    writes
}

/// `values` after `grip` moved `delta` frames.
pub(crate) fn moved_values(grip: BandGrip, delta: i64, values: TimingValues) -> TimingValues {
    let delta = delta as f64;
    match grip {
        BandGrip::Start => TimingValues {
            start: values.start + delta,
            duration: values.duration - delta,
        },
        BandGrip::End => TimingValues {
            duration: values.duration + delta,
            ..values
        },
        BandGrip::Body => TimingValues {
            start: values.start + delta,
            ..values
        },
    }
}

/// The window's edges `grip` moves, in frames.
pub(crate) fn grip_edges(grip: BandGrip, window: (f64, f64)) -> Vec<f64> {
    match grip {
        BandGrip::Start => vec![window.0],
        BandGrip::End => vec![window.1],
        BandGrip::Body => vec![window.0, window.1],
    }
}

/// `window` after `grip` moved `delta` frames.
pub(crate) fn moved_window(grip: BandGrip, delta: i64, window: (f64, f64)) -> (f64, f64) {
    let delta = delta as f64;
    match grip {
        BandGrip::Start => (window.0 + delta, window.1),
        BandGrip::End => (window.0, window.1 + delta),
        BandGrip::Body => (window.0 + delta, window.1 + delta),
    }
}

/// Frames between `start_x` and `x`, not clamped to the lane.
pub(crate) fn drag_frames(start_x: f32, x: f32, pixels_per_frame: f64) -> f64 {
    f64::from(x - start_x) / pixels_per_frame.max(f64::MIN_POSITIVE)
}

/// Playhead, chapter edges and the edges of the bands that stay still
/// (`still`, see [`still_ranges`]); the dragged `window`'s own edges left
/// out, which moving onto would change nothing.
pub(crate) fn snap_targets(
    playhead: f64,
    chapters: &[ReactChapter],
    still: &[(f64, f64)],
    window: (f64, f64),
) -> Vec<f64> {
    let chapter_edges = chapters
        .iter()
        .flat_map(|chapter| [chapter.from as f64, chapter.end() as f64]);
    let band_edges = still.iter().flat_map(|&(from, to)| [from, to]);
    std::iter::once(playhead)
        .chain(chapter_edges)
        .chain(band_edges)
        .filter(|&target| target != window.0 && target != window.1)
        .collect()
}

/// Whether `instance` moves with `preview`: it renders through the dragged
/// window, or, for the end edge and the body, through a later item of the
/// same `Series`.
fn moves(instance: &InstanceOccurrences, preview: &DragPreview) -> bool {
    instance.wrappers.contains(&preview.source)
        || preview.grip != BandGrip::Start
            && preview
                .followers
                .iter()
                .any(|follower| instance.wrappers.contains(follower))
}

/// The ranges (frames) of the instances of `element` that `filter` draws
/// and a drag of `preview.grip` leaves where they are: what its edges snap
/// to beside the playhead and the chapters.
pub(crate) fn still_ranges(
    element: &ElementOccurrences,
    preview: &DragPreview,
    filter: &ImpactFilter,
) -> Vec<(f64, f64)> {
    element
        .instances
        .iter()
        .filter(|instance| filter.changes(instance) && !moves(instance, preview))
        .flat_map(|instance| &instance.ranges)
        .map(|&(from, to)| (from as f64, to as f64))
        .collect()
}

/// `raw` frames rounded, after snapping the nearest of `edges` (moved by
/// `raw`) to `targets` within 6 px; no snapping with `shift`.
pub(crate) fn snapped_delta(
    edges: &[f64],
    raw: f64,
    targets: &[f64],
    pixels_per_frame: f64,
    shift: bool,
) -> i64 {
    let nearest = edges
        .iter()
        .flat_map(|edge| targets.iter().map(move |target| target - edge))
        .filter(|_| !shift)
        .map(|delta| (delta, (delta - raw).abs() * pixels_per_frame))
        .filter(|&(_, distance)| distance <= SNAP_DISTANCE)
        .min_by(|a, b| a.1.total_cmp(&b.1));
    nearest.map_or(raw, |(delta, _)| delta).round() as i64
}

/// `element` with every instance through `preview.source` moved by its
/// grip, and those through a follower shifted. A moved range follows the
/// grip: the body moves it; the start edge moves it, cut at the window's
/// end; the end edge cuts it at the new end. A range that ran to the old
/// end runs to the new one: it shows as long as the window does. Instances
/// through a later item of the `Series` move with the end and the body; for
/// the start edge, offset and duration cancel.
pub(crate) fn preview_occurrences(
    element: &ElementOccurrences,
    preview: &DragPreview,
) -> ElementOccurrences {
    let delta = preview.delta;
    let old_end = preview.window.1.round() as i64;
    let new_end = moved_window(preview.grip, delta, preview.window).1.round() as i64;
    let instances = element
        .instances
        .iter()
        .map(|instance| {
            let moved = |(from, to): (u64, u64)| -> Option<(u64, u64)> {
                let (from, to) = (from as i64, to as i64);
                let (from, to) = if instance.wrappers.contains(&preview.source) {
                    let ended = to == old_end;
                    match preview.grip {
                        BandGrip::Body => (from + delta, to + delta),
                        BandGrip::Start if ended => (from + delta, new_end),
                        BandGrip::Start => (from + delta, (to + delta).min(new_end)),
                        BandGrip::End if ended => (from, new_end),
                        BandGrip::End => (from, to.min(new_end)),
                    }
                } else if moves(instance, preview) {
                    (from + delta, to + delta)
                } else {
                    (from, to)
                };
                let (from, to) = (from.max(0), to.max(0));
                (from < to).then_some((from as u64, to as u64))
            };
            InstanceOccurrences {
                ranges: instance.ranges.iter().copied().filter_map(moved).collect(),
                ..instance.clone()
            }
        })
        .collect();
    ElementOccurrences {
        element: element.element.clone(),
        instances,
    }
}

/// `from 30 → 45`; both props for the start edge (`from 30 → 45 ·
/// durationInFrames 60 → 45`).
pub(crate) fn drag_label(
    kind: TimingKind,
    grip: BandGrip,
    before: TimingValues,
    after: TimingValues,
) -> String {
    grip_props(kind, grip)
        .into_iter()
        .map(|name| {
            let (was, now) = if name == DURATION {
                (before.duration, after.duration)
            } else {
                (before.start, after.start)
            };
            format!("{name} {} → {}", format_number(was), format_number(now))
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

/// What the end of a drag that moved the pointer `moved_px` at most and
/// the grip `delta` frames does: Escape cancels; a press that barely moved
/// is a click and seeks; a drag back to where it started writes nothing.
pub(crate) fn drag_end(moved_px: f32, delta: i64, escaped: bool) -> DragEnd {
    if escaped {
        DragEnd::Cancel
    } else if moved_px < CLICK_DISTANCE {
        DragEnd::Seek
    } else if delta == 0 {
        DragEnd::Cancel
    } else {
        DragEnd::Commit
    }
}

/// The notice of a released drag that nothing was written for: the scope
/// gate held or refused it, or the window was not current.
pub(crate) fn not_written(note: &str) -> String {
    format!("Not written: {note}")
}

/// A drag of the selected element's own timing window on the timeline.
#[derive(Debug)]
pub(crate) struct BandDrag {
    pub(crate) preview: DragPreview,
    kind: TimingKind,
    /// The window's values when the drag started.
    values: TimingValues,
    /// A `Series` item's start in its `Series`; `None` for a `Sequence`.
    series_start: Option<f64>,
    /// Window x of the press.
    start_x: f32,
    /// The farthest the pointer went from `start_x`, in pixels.
    moved_px: f32,
    /// What the moved edges snap to, read at the press.
    snap_targets: Vec<f64>,
    /// The frame a click seeks to: the first of the drawn band holding the
    /// pressed frame, as a click on that band does; `None` when no band
    /// holds it.
    seek: Option<u64>,
    /// The edit scope the window's scope options showed at the press (see
    /// [`commit_scope`]): the drag is written under it or refused, and the
    /// label's second line says it.
    scope: Option<Scope>,
    /// The instances the bands show while the drag runs: those an edit
    /// under `scope` changes (see [`impact_filter`]).
    filter: ImpactFilter,
    /// The button came up: the values are being written and, after a
    /// successful write, drawn until its reload, so the bands do not flash
    /// back.
    released: bool,
}

/// Why the last band drag wrote nothing, beside the frame `at`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BandNotice {
    pub(crate) at: f64,
    pub(crate) text: String,
}

/// The grips over the band holding the lookup frame.
#[derive(Debug)]
pub(crate) struct BandGrips {
    /// The window's frames as drawn: moved while a drag runs.
    pub(crate) window: (f64, f64),
    /// Why the start edge, the end edge and the body don't drag.
    locks: [Option<String>; 3],
    /// A running drag's label lines, and the frame it sits at.
    pub(crate) label: Option<(f64, Vec<String>)>,
}

impl BandGrips {
    /// Why `grip` doesn't drag.
    pub(crate) fn lock(&self, grip: BandGrip) -> Option<&str> {
        let index = match grip {
            BandGrip::Start => 0,
            BandGrip::End => 1,
            BandGrip::Body => 2,
        };
        self.locks[index].as_deref()
    }
}

/// The frame of the edge `grip` holds in `window`; the start for the body.
fn grip_frame(grip: BandGrip, window: (f64, f64)) -> f64 {
    match grip {
        BandGrip::End => window.1,
        BandGrip::Start | BandGrip::Body => window.0,
    }
}

/// What a write of the window takes: its targets, the element's report,
/// the scope, the scene calls, and the values.
type BandWrite = (
    WindowTargets,
    LayerSourceReport,
    Option<Scope>,
    Option<SceneCalls>,
    Vec<(PropKey, Option<EditValue>)>,
);

impl EditorView {
    /// The selected layer's report while its source is found.
    fn band_report(&self) -> Option<&LayerSourceReport> {
        self.selected_layer.as_ref()?;
        match &self.source_lookup {
            Some(SourceLookup::Found(report)) if !report.chain.is_empty() => Some(report),
            _ => None,
        }
    }

    /// The impact the timeline draws: the selection's, and while a band
    /// drag runs, the instances its scope changes with those the drag
    /// moves moved.
    pub(crate) fn timeline_impact(&self) -> Option<ImpactView> {
        let Some(drag) = &self.band_drag else {
            return self.selection_impact();
        };
        Some(moved_impact_view(
            self.band_report()?,
            self.occurrences.as_ref().map(|(_, state)| state),
            self.react_chapters().unwrap_or_default(),
            u64::try_from(self.clock.frame()).unwrap_or(0),
            &drag.filter,
            Some(&drag.preview),
        ))
    }

    /// The grips of the selected element's own timing window, at its
    /// edges, when one of `filled` (this instance's bands in the current
    /// scene, frames) holds the lookup frame; while a drag runs, where it
    /// moved them. A loop's or a freeze frame's window, and one that
    /// doesn't resolve, has every grip locked with why. `None` for a
    /// scene's window, or none.
    pub(crate) fn band_grips(&self, filled: &[(f64, f64)]) -> Option<BandGrips> {
        if let Some(drag) = &self.band_drag {
            let preview = &drag.preview;
            let after = moved_values(preview.grip, preview.delta, drag.values);
            let window = moved_window(preview.grip, preview.delta, preview.window);
            let mut lines = vec![drag_label(drag.kind, preview.grip, drag.values, after)];
            lines.extend(drag.scope.map(|scope| scope_in_effect(scope).to_owned()));
            return Some(BandGrips {
                window,
                locks: Default::default(),
                label: Some((grip_frame(preview.grip, window), lines)),
            });
        }
        let report = self.band_report()?;
        let frame = self.source_lookup_frame;
        let window = match window_role(
            report,
            self.react_chapters().unwrap_or_default(),
            u64::try_from(self.clock.end_frame()).unwrap_or(0),
            u64::try_from(frame).unwrap_or(0),
        ) {
            WindowRole::Own(window) => window,
            WindowRole::Remapped => report.windows.first()?,
            WindowRole::Scene(_) | WindowRole::None => return None,
        };
        draggable_band(filled, frame as f64)?;
        let grips = [BandGrip::Start, BandGrip::End, BandGrip::Body];
        let locks = match self.window_targets() {
            Some((Ok(targets), _)) => {
                grips.map(|grip| grip_lock(targets.kind, grip, &targets.targets))
            }
            Some((Err(reason), _)) => grips.map(|_| Some(reason.to_owned())),
            None => {
                let reason = self
                    .element_edit
                    .unavailable(self.selected_layer.as_deref());
                grips.map(|_| Some(reason.clone()))
            }
        };
        Some(BandGrips {
            window: (window.from, window.to),
            locks,
            label: None,
        })
    }

    /// Whether a band drag runs with the button down.
    pub(crate) fn band_drag_active(&self) -> bool {
        self.band_drag.as_ref().is_some_and(|drag| !drag.released)
    }

    /// The scope state of writing `window` around the element `report`
    /// describes, with the playhead where it is now; the scene calls that
    /// count; and the choice made for the element.
    fn window_scope_state(
        &self,
        window: &WindowTargets,
        report: &LayerSourceReport,
    ) -> (ScopeState, Option<&SceneCalls>, Option<Scope>) {
        let calls = self.window_scene_calls(window, report);
        let state = scope_state(
            report,
            &window.targets,
            self.occurrences.as_ref().map(|(_, state)| state),
            calls,
            self.react_chapters().unwrap_or_default(),
            u64::try_from(self.clock.frame()).unwrap_or(0),
        );
        let remembered = report
            .chain
            .first()
            .and_then(|element| self.scope_choices.get(&SourcePos::from(element)));
        (state, calls, remembered)
    }

    /// Starts a drag when window x `x` is on a grip that drags. `bands` are
    /// the bands drawn (frames), the first `filled` of them this instance's
    /// in the current scene. True when one started; a press on a locked
    /// grip starts none, and a click on the band below seeks as before.
    pub(crate) fn begin_band_drag(&mut self, x: f32, bands: &[(f64, f64)], filled: usize) -> bool {
        if self.band_drag.is_some() {
            return false;
        }
        let filled_bands = &bands[..filled.min(bands.len())];
        let Some(grips) = self.band_grips(filled_bands) else {
            return false;
        };
        // As drawn: at least 2 px wide.
        let left = self.frame_x(grips.window.0);
        let grip = grip_at(x, left, (self.frame_x(grips.window.1) - left).max(2.0));
        if grips.lock(grip).is_some() {
            return false;
        }
        let Some((Ok(window), report)) = self.window_targets() else {
            return false;
        };
        // The frame under the press, for a click: the band holding it
        // seeks as a click on it does.
        let pressed = grips.window.0 + f64::from(x - left) / self.pixels_per_frame();
        let seek = draggable_band(bands, pressed).map(|band| bands[band].0 as u64);
        // The item's start in its `Series`: from the window around it, or
        // the composition's start at the root.
        let parent = report.windows.get(1).map_or(0.0, |parent| parent.from);
        let series_start =
            (window.kind == TimingKind::SeriesSequence).then_some(grips.window.0 - parent);
        let (state, calls, remembered) = self.window_scope_state(window, report);
        let scope = commit_scope(remembered, &state);
        let filter = impact_filter(scope, &window.targets, calls);
        let preview = DragPreview {
            source: window.source.clone(),
            window: grips.window,
            grip,
            delta: 0,
            followers: window.followers.clone(),
        };
        let still = self
            .indexed_element(report)
            .map(|element| still_ranges(element, &preview, &filter))
            .unwrap_or_default();
        let drag = BandDrag {
            kind: window.kind,
            values: timing_values(window.kind, &window.targets, grips.window),
            series_start,
            start_x: x,
            moved_px: 0.0,
            snap_targets: snap_targets(
                self.clock.frame() as f64,
                self.react_chapters().unwrap_or_default(),
                &still,
                grips.window,
            ),
            seek,
            scope,
            filter,
            preview,
            released: false,
        };
        self.pause();
        self.band_notice = None;
        self.band_drag = Some(drag);
        true
    }

    /// Follows the pointer: moves the grip by whole frames, snapped unless
    /// Shift is held, past the lane's edges too. Writes nothing.
    pub(crate) fn continue_band_drag(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if !self.band_drag_active() {
            return;
        }
        // The button came up where the view did not see it: end there.
        if !event.dragging() {
            if self.end_band_drag(cx) {
                cx.notify();
            }
            return;
        }
        let pixels_per_frame = self.pixels_per_frame();
        let x = f32::from(event.position.x);
        let Some(drag) = self.band_drag.as_mut() else {
            return;
        };
        drag.moved_px = drag.moved_px.max((x - drag.start_x).abs());
        let grip = drag.preview.grip;
        let raw = drag_frames(drag.start_x, x, pixels_per_frame);
        let edges = grip_edges(grip, drag.preview.window);
        let delta = snapped_delta(
            &edges,
            raw,
            &drag.snap_targets,
            pixels_per_frame,
            event.modifiers.shift,
        );
        let delta = clamp_band_delta(grip, delta, drag.values, drag.series_start);
        if delta != drag.preview.delta {
            drag.preview.delta = delta;
            cx.notify();
        }
    }

    /// The button came up: a click seeks to the first frame of the band it
    /// pressed, a drag writes its values as one history entry, and a drag
    /// back to where it started writes nothing. True when a drag was
    /// running.
    pub(crate) fn end_band_drag(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(drag) = self.band_drag.as_ref().filter(|drag| !drag.released) else {
            return false;
        };
        match drag_end(drag.moved_px, drag.preview.delta, false) {
            DragEnd::Seek => {
                let seek = drag.seek;
                self.band_drag = None;
                if let Some(frame) = seek {
                    self.seek_to_frame(frame, cx);
                }
            }
            DragEnd::Cancel => self.band_drag = None,
            DragEnd::Commit => self.commit_band_drag(cx),
        }
        true
    }

    /// Escape during a band drag: drops it, writing nothing. True when a
    /// pressed drag was cancelled.
    pub(crate) fn escape_band_drag(&mut self) -> bool {
        let Some(drag) = self.band_drag.as_ref().filter(|drag| !drag.released) else {
            return false;
        };
        if drag_end(drag.moved_px, drag.preview.delta, true) == DragEnd::Cancel {
            self.band_drag = None;
        }
        true
    }

    /// Writes the drag's values into the window's props under the scope it
    /// started with, never held and never under another: a gate that holds
    /// or refuses it, or a failed write, leaves the notice why beside the
    /// grip. The released drag stays drawn while the write runs and, after
    /// a successful one, until its reload (see `drop_released_band_drag`).
    fn commit_band_drag(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = self.band_drag.as_mut() else {
            return;
        };
        drag.released = true;
        let preview = &drag.preview;
        let at = grip_frame(
            preview.grip,
            moved_window(preview.grip, preview.delta, preview.window),
        );
        let (window, report, scope, calls, values) = match self.band_write() {
            Ok(write) => write,
            Err(text) => {
                self.band_notice = Some(BandNotice { at, text });
                self.band_drag = None;
                return;
            }
        };
        self.record_own_writes(
            window.targets.files.clone(),
            move |texts| {
                window_files(texts, &window, &report, scope, calls.as_ref(), &values)
                    .map_err(|error| error.to_string())
            },
            move |this, result, cx| {
                // A window still current after a success means nothing was
                // written (the text came out the same): no reload comes to
                // drop the drag.
                if let Err(text) = &result {
                    this.band_notice = Some(BandNotice {
                        at,
                        text: text.clone(),
                    });
                }
                if result.is_err() || this.window_targets().is_some() {
                    this.band_drag = None;
                }
                cx.notify();
            },
            cx,
        );
    }

    /// What writing the released drag takes, or why it can't be written
    /// now: the window is not current (a reload while dragging), or
    /// [`gesture_gate`] holds or refuses it under the scope it started with
    /// (a prop that scope can't write, the scenes scanned meanwhile).
    fn band_write(&self) -> Result<BandWrite, String> {
        let drag = self.band_drag.as_ref().ok_or_else(String::new)?;
        let preview = &drag.preview;
        let (window, report) = match self.window_targets() {
            Some((Ok(window), report)) if window.source == preview.source => (window, report),
            Some((Err(reason), _)) => return Err(not_written(reason)),
            Some((Ok(_), _)) | None => {
                let reason = self
                    .element_edit
                    .unavailable(self.selected_layer.as_deref());
                return Err(not_written(&reason));
            }
        };
        let values = band_writes(
            drag.kind,
            preview.grip,
            preview.delta,
            drag.values,
            &window.targets,
        );
        let keys: Vec<PropKey> = values.iter().map(|(key, _)| key.clone()).collect();
        let (state, calls, _) = self.window_scope_state(window, report);
        let scope = match gesture_gate(drag.scope, &state, &window.targets, &keys, calls) {
            CommitGate::Go(scope) => scope,
            CommitGate::Hold(note) | CommitGate::Refuse(note) => return Err(not_written(&note)),
        };
        Ok((
            window.clone(),
            report.clone(),
            scope,
            calls.cloned(),
            values,
        ))
    }

    /// The reload after a band drag's write draws its values: the released
    /// drag goes (a pressed one goes on).
    pub(crate) fn drop_released_band_drag(&mut self) {
        if !self.band_drag_active() {
            self.band_drag = None;
        }
    }

    /// The "Set timing" offer for the selected element at its lookup frame
    /// (see [`set_timing_offer`]), under the scope the Properties panel
    /// shows for it. Hidden while its targets are being read (again, after
    /// a write too); disabled when they can't be read, and when its file is
    /// outside the project. `None` without a found source.
    pub(crate) fn set_timing_action(&self) -> Option<SetTimingOffer> {
        let report = self.band_report()?;
        let chapters = self.react_chapters().unwrap_or_default();
        let end = u64::try_from(self.clock.end_frame()).unwrap_or(0);
        let frame = u64::try_from(self.source_lookup_frame).unwrap_or(0);
        let role = window_role(report, chapters, end, frame);
        if matches!(role, WindowRole::Own(_) | WindowRole::Remapped) {
            return Some(SetTimingOffer::Hidden);
        }
        let selected = self.selected_layer.as_deref();
        let unavailable = || SetTimingOffer::Disabled(self.element_edit.unavailable(selected));
        let Some((targets, _)) = self.element_targets() else {
            // A resolution or a reload on its way answers; a failed one
            // says why.
            if !self.element_edit.failed(selected) {
                return Some(SetTimingOffer::Hidden);
            }
            return Some(unavailable());
        };
        if targets.outside.contains(&targets.file) {
            return Some(SetTimingOffer::Disabled(reason_text(
                &LockReason::OutsideProject,
            )));
        }
        let (Some((resolved, _)), Some(context)) = (
            self.element_edit.resolved(selected),
            self.shown_scope_context(),
        ) else {
            return Some(unavailable());
        };
        Some(set_timing_offer(
            &role,
            set_timing_span(&role, chapters, end, frame),
            resolved.wrap_lock.as_ref(),
            resolved.import_lock.as_ref(),
            &context.state,
            self.remembered_scope(),
        ))
    }

    /// Wraps the selected element in a `<Sequence>` from the playhead (see
    /// [`set_timing_files`]) as one history entry, then reloads. Playback
    /// pauses first, as a band drag's press does. An offer no longer
    /// enabled, or a write that fails, leaves the notice why where the band
    /// drag leaves its own.
    pub(crate) fn set_timing(&mut self, cx: &mut Context<Self>) {
        self.pause();
        let at = self.source_lookup_frame as f64;
        let (from, duration) = match self.set_timing_action() {
            Some(SetTimingOffer::Enabled { from, duration }) => (from, duration),
            Some(SetTimingOffer::Disabled(reason)) => {
                self.band_notice = Some(BandNotice {
                    at,
                    text: not_written(&reason),
                });
                return;
            }
            Some(SetTimingOffer::Hidden) | None => return,
        };
        let Some((targets, report)) = self.element_targets() else {
            return;
        };
        let (file, expected, report) = (targets.file.clone(), targets.element, report.clone());
        self.band_notice = None;
        self.record_own_writes(
            vec![file],
            move |texts| {
                set_timing_files(texts, &report, &expected, from, duration)
                    .map_err(|error| error.to_string())
            },
            move |this, result, cx| {
                if let Err(text) = result {
                    this.band_notice = Some(BandNotice {
                        at,
                        text: not_written(&text),
                    });
                }
                cx.notify();
            },
            cx,
        );
    }
}

/// A drag of a chapter's end on the chapter row: the `defineScenes` entry's
/// `durationInFrames`. Nothing is written until the button comes up.
#[derive(Debug)]
pub(crate) struct SceneDrag {
    pub(crate) id: String,
    /// The edge as found at the press (editable): the write needs it
    /// unchanged.
    edge: SceneEdge,
    /// The id of its list's first entry, and those after it.
    first: String,
    later: Vec<String>,
    /// The composition's length is the list's.
    follows: bool,
    /// The chapters and the composition's end at the press.
    chapters: Vec<ReactChapter>,
    composition_end: u64,
    /// The chapter's first frame, which a click seeks to, and its end and
    /// length as drawn at the press.
    from: u64,
    end: f64,
    length: f64,
    /// Window x of the press.
    start_x: f32,
    /// The farthest the pointer went from `start_x`, in pixels.
    moved_px: f32,
    /// What the end snaps to, read at the press.
    snap_targets: Vec<f64>,
    /// Whole frames, within the bounds.
    pub(crate) delta: i64,
    /// Why the bounds stopped the end, while they do.
    note: Option<String>,
    /// The button came up: drawn while the write runs and, after a
    /// successful one, until its reload.
    released: bool,
}

impl EditorView {
    /// Whether a scene drag runs with the button down.
    pub(crate) fn scene_drag_active(&self) -> bool {
        self.scene_drag.as_ref().is_some_and(|drag| !drag.released)
    }

    /// `chapters` as the chapter row draws them: moved by a running (or
    /// released) scene drag.
    pub(crate) fn drawn_chapters(&self, chapters: &[ReactChapter]) -> Vec<ReactChapter> {
        match &self.scene_drag {
            Some(drag) => preview_chapters(chapters, &drag.id, &drag.later, drag.delta),
            None => chapters.to_vec(),
        }
    }

    /// The chapter row's label: a running drag's (`durationInFrames 90 →
    /// 100`, and why the bounds stop it), or why the last one wrote
    /// nothing; the frame it sits at, and whether it is a notice.
    pub(crate) fn scene_label(&self) -> Option<(f64, String, bool)> {
        if let Some(drag) = self.scene_drag.as_ref() {
            let before = TimingValues {
                start: 0.0,
                duration: drag.length,
            };
            let after = moved_values(BandGrip::End, drag.delta, before);
            let mut lines = vec![drag_label(
                TimingKind::Sequence,
                BandGrip::End,
                before,
                after,
            )];
            lines.extend(drag.note.clone());
            return Some((drag.end + drag.delta as f64, lines.join(" · "), false));
        }
        let notice = self.scene_notice.as_ref()?;
        Some((notice.at, notice.text.clone(), true))
    }

    /// Starts a drag of chapter `id`'s end, pressed at window x `x`, when
    /// it drags. True when one started.
    pub(crate) fn begin_scene_drag(&mut self, id: &str, x: f32) -> bool {
        if self.scene_drag.is_some() {
            return false;
        }
        let edge = self.scene_edge(id);
        let SceneEdge::Editable {
            first,
            later,
            follows,
            ..
        } = &edge
        else {
            return false;
        };
        let Some(chapters) = self.react_chapters() else {
            return false;
        };
        let Some(chapter) = chapters.iter().find(|chapter| chapter.id == id) else {
            return false;
        };
        let composition_end = u64::try_from(self.clock.end_frame()).unwrap_or(0);
        let length = written_length(chapters, id, 0, composition_end);
        let drag = SceneDrag {
            id: id.to_owned(),
            first: first.clone(),
            later: later.clone(),
            follows: *follows,
            snap_targets: scene_snap_targets(self.clock.frame() as f64, chapters, id, later),
            chapters: chapters.to_vec(),
            composition_end,
            from: chapter.from,
            end: chapter.from as f64 + length,
            length,
            start_x: x,
            moved_px: 0.0,
            delta: 0,
            note: None,
            released: false,
            edge,
        };
        self.pause();
        self.scene_notice = None;
        self.scene_drag = Some(drag);
        true
    }

    /// Follows the pointer: moves the end by whole frames, snapped unless
    /// Shift is held, within the bounds. Writes nothing.
    pub(crate) fn continue_scene_drag(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if !self.scene_drag_active() {
            return;
        }
        // The button came up where the view did not see it: end there.
        if !event.dragging() {
            if self.end_scene_drag(cx) {
                cx.notify();
            }
            return;
        }
        let pixels_per_frame = self.pixels_per_frame();
        let x = f32::from(event.position.x);
        let Some(drag) = self.scene_drag.as_mut() else {
            return;
        };
        drag.moved_px = drag.moved_px.max((x - drag.start_x).abs());
        let raw = drag_frames(drag.start_x, x, pixels_per_frame);
        let delta = snapped_delta(
            &[drag.end],
            raw,
            &drag.snap_targets,
            pixels_per_frame,
            event.modifiers.shift,
        );
        let (delta, note) = clamp_scene_delta(
            &drag.chapters,
            &drag.id,
            &drag.first,
            &drag.later,
            delta,
            drag.composition_end,
            drag.follows,
        );
        if delta != drag.delta || note != drag.note {
            drag.delta = delta;
            drag.note = note;
            cx.notify();
        }
    }

    /// The button came up: a click seeks to the chapter's start, as a
    /// click on it does; a drag writes the entry's length as one history
    /// entry; a drag back to where it started writes nothing. True when a
    /// drag was running.
    pub(crate) fn end_scene_drag(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(drag) = self.scene_drag.as_ref().filter(|drag| !drag.released) else {
            return false;
        };
        match drag_end(drag.moved_px, drag.delta, false) {
            DragEnd::Seek => {
                let from = drag.from;
                self.scene_drag = None;
                self.seek_to_frame(from, cx);
            }
            DragEnd::Cancel => self.scene_drag = None,
            DragEnd::Commit => self.commit_scene_drag(cx),
        }
        true
    }

    /// Escape during a scene drag: drops it, writing nothing. True when a
    /// pressed drag was cancelled.
    pub(crate) fn escape_scene_drag(&mut self) -> bool {
        let Some(drag) = self.scene_drag.as_ref().filter(|drag| !drag.released) else {
            return false;
        };
        if drag_end(drag.moved_px, drag.delta, true) == DragEnd::Cancel {
            self.scene_drag = None;
        }
        true
    }

    /// Writes the entry's new length: a source no longer as it was at the
    /// press, or a failed write, leaves the notice why beside the end. The
    /// released drag stays drawn while the write runs and, after a
    /// successful one, until its reload (see `drop_released_scene_drag`).
    fn commit_scene_drag(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = self.scene_drag.as_mut() else {
            return;
        };
        drag.released = true;
        let at = drag.end + drag.delta as f64;
        let id = drag.id.clone();
        let written = written_length(&drag.chapters, &id, drag.delta, drag.composition_end);
        let edge = match self.scene_write() {
            Ok(edge) => edge,
            Err(text) => {
                self.scene_notice = Some(BandNotice { at, text });
                self.scene_drag = None;
                return;
            }
        };
        let SceneEdge::Editable { file, .. } = &edge else {
            self.scene_drag = None;
            return;
        };
        // The entry already holds it: no reload would come to drop the
        // drag.
        if writes_nothing(&edge, written) {
            self.scene_drag = None;
            return;
        }
        self.record_own_writes(
            vec![file.clone()],
            move |texts| scene_files(texts, &id, &edge, written).map_err(|error| error.to_string()),
            move |this, result, cx| {
                if let Err(text) = result {
                    this.scene_notice = Some(BandNotice { at, text });
                    this.scene_drag = None;
                }
                cx.notify();
            },
            cx,
        );
    }

    /// The edge the released drag writes, or why it can't be written now:
    /// the search for the current reload found it locked, or not as it was
    /// at the press.
    fn scene_write(&self) -> Result<SceneEdge, String> {
        let drag = self.scene_drag.as_ref().ok_or_else(String::new)?;
        match self.scene_edge(&drag.id) {
            SceneEdge::Locked(reason) => Err(not_written(&reason)),
            edge if edge == drag.edge => Ok(edge),
            SceneEdge::Editable { .. } => Err(not_written(&CommitError::SourceChanged.to_string())),
        }
    }

    /// The reload after a scene drag's write draws its length: the released
    /// drag goes (a pressed one goes on).
    pub(crate) fn drop_released_scene_drag(&mut self) {
        if !self.scene_drag_active() {
            self.scene_drag = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element_edit::Target;
    use celesta_source_edit::{ElementRef, LiteralForm, LockReason, Span};
    use std::path::PathBuf;

    fn literal(value: f64) -> EditTarget {
        EditTarget::Literal {
            span: Span { start: 0, end: 1 },
            value: EditValue::Number(value),
            form: LiteralForm::Number,
        }
    }

    fn missing() -> EditTarget {
        EditTarget::Missing {
            insert_at: 0,
            default: None,
        }
    }

    /// A wrapper's targets with `rows` (name, target).
    fn targets(rows: Vec<(&str, EditTarget)>) -> ElementTargets {
        let file = PathBuf::from("/project/scene.tsx");
        ElementTargets {
            file: file.clone(),
            files: vec![file.clone()],
            element: ElementRef {
                start: 0,
                end_of_attributes: 0,
                children: None,
            },
            targets: rows
                .into_iter()
                .map(|(name, target)| {
                    (
                        PropKey::Prop(name.to_owned()),
                        Target {
                            file: file.clone(),
                            path: name.to_owned(),
                            target,
                            call_site: None,
                            inputs: Vec::new(),
                        },
                    )
                })
                .collect(),
            table: None,
            component: None,
            enclosing: None,
            defaults: Vec::new(),
            lift_lock: None,
            outside: Vec::new(),
        }
    }

    fn pos(line: u32) -> SourcePos {
        SourcePos {
            file: "/project/scene.tsx".into(),
            line,
            column: 5,
        }
    }

    fn number(key: &str, value: f64) -> (PropKey, Option<EditValue>) {
        (
            PropKey::Prop(key.to_owned()),
            Some(EditValue::Number(value)),
        )
    }

    fn values(start: f64, duration: f64) -> TimingValues {
        TimingValues { start, duration }
    }

    #[test]
    fn draggable_band_holds_the_lookup_frame() {
        let bands = [(0.0, 60.0), (120.0, 180.0)];
        assert_eq!(draggable_band(&bands, 130.0), Some(1));
        assert_eq!(draggable_band(&bands, 0.0), Some(0));
        assert_eq!(draggable_band(&bands, 90.0), None);
        // `to` is the frame after the last.
        assert_eq!(draggable_band(&bands, 60.0), None);
    }

    #[test]
    fn grip_at_prefers_edges_and_splits_narrow_bands() {
        assert_eq!(grip_at(3.0, 0.0, 100.0), BandGrip::Start);
        assert_eq!(grip_at(97.0, 0.0, 100.0), BandGrip::End);
        assert_eq!(grip_at(50.0, 0.0, 100.0), BandGrip::Body);
        // Measured from the band's left.
        assert_eq!(grip_at(250.0, 200.0, 100.0), BandGrip::Body);
        assert_eq!(grip_at(203.0, 200.0, 100.0), BandGrip::Start);

        assert_eq!(grip_at(4.0, 0.0, 10.0), BandGrip::Start);
        assert_eq!(grip_at(6.0, 0.0, 10.0), BandGrip::End);
    }

    #[test]
    fn grip_lock_names_the_locked_prop() {
        let expression = EditTarget::Locked(LockReason::Expression {
            source: "cut - 9".to_owned(),
        });
        let cut = targets(vec![
            ("from", expression),
            ("durationInFrames", literal(60.0)),
        ]);
        let reason = "Written as an expression: cut - 9";
        let lock = |grip| grip_lock(TimingKind::Sequence, grip, &cut);
        assert_eq!(lock(BandGrip::Start).as_deref(), Some(reason));
        assert_eq!(lock(BandGrip::Body).as_deref(), Some(reason));
        assert_eq!(lock(BandGrip::End), None);

        let unresolved = EditTarget::Locked(LockReason::UnresolvedIdentifier {
            name: "len".to_owned(),
        });
        let len = targets(vec![
            ("from", literal(30.0)),
            ("durationInFrames", unresolved),
        ]);
        let reason = "Comes from len, which can't be traced";
        let lock = |grip| grip_lock(TimingKind::Sequence, grip, &len);
        assert_eq!(lock(BandGrip::Start).as_deref(), Some(reason));
        assert_eq!(lock(BandGrip::End).as_deref(), Some(reason));
        assert_eq!(lock(BandGrip::Body), None);

        // A `Series` item's start is its `offset`.
        let item = targets(vec![
            ("durationInFrames", literal(60.0)),
            (
                "offset",
                EditTarget::Locked(LockReason::Expression {
                    source: "gap".to_owned(),
                }),
            ),
        ]);
        let lock = |grip| grip_lock(TimingKind::SeriesSequence, grip, &item);
        assert_eq!(
            lock(BandGrip::Body).as_deref(),
            Some("Written as an expression: gap")
        );
        assert_eq!(lock(BandGrip::End), None);

        // Missing props are inserted.
        let none = targets(vec![("from", missing()), ("durationInFrames", missing())]);
        for grip in [BandGrip::Start, BandGrip::End, BandGrip::Body] {
            assert_eq!(grip_lock(TimingKind::Sequence, grip, &none), None);
        }
    }

    #[test]
    fn timing_values_default_missing_props() {
        let none = targets(vec![("from", missing()), ("durationInFrames", missing())]);
        assert_eq!(
            timing_values(TimingKind::Sequence, &none, (30.0, 90.0)),
            values(0.0, 60.0)
        );

        let written = targets(vec![
            ("from", literal(30.0)),
            ("durationInFrames", literal(45.0)),
        ]);
        assert_eq!(
            timing_values(TimingKind::Sequence, &written, (30.0, 90.0)),
            values(30.0, 45.0)
        );

        let item = targets(vec![
            ("durationInFrames", literal(60.0)),
            ("offset", literal(-10.0)),
        ]);
        assert_eq!(
            timing_values(TimingKind::SeriesSequence, &item, (50.0, 110.0)),
            values(-10.0, 60.0)
        );

        // A locked prop reads as a missing one: its grips don't drag.
        let locked = targets(vec![
            ("from", EditTarget::Locked(LockReason::OverriddenByParent)),
            ("durationInFrames", literal(60.0)),
        ]);
        assert_eq!(
            timing_values(TimingKind::Sequence, &locked, (30.0, 90.0)),
            values(0.0, 60.0)
        );
    }

    #[test]
    fn band_edit_follows_the_table() {
        let now = values(30.0, 60.0);
        let edit = |kind, grip| band_edit(kind, grip, 15, now);
        assert_eq!(
            edit(TimingKind::Sequence, BandGrip::Start),
            [number("from", 45.0), number("durationInFrames", 45.0)]
        );
        assert_eq!(
            edit(TimingKind::Sequence, BandGrip::End),
            [number("durationInFrames", 75.0)]
        );
        assert_eq!(
            edit(TimingKind::Sequence, BandGrip::Body),
            [number("from", 45.0)]
        );
        assert_eq!(
            edit(TimingKind::SeriesSequence, BandGrip::Start),
            [number("offset", 45.0), number("durationInFrames", 45.0)]
        );
        assert_eq!(
            edit(TimingKind::SeriesSequence, BandGrip::End),
            [number("durationInFrames", 75.0)]
        );
        assert_eq!(
            edit(TimingKind::SeriesSequence, BandGrip::Body),
            [number("offset", 45.0)]
        );
    }

    #[test]
    fn a_body_drag_keeps_a_missing_duration() {
        // `<Sequence from={30}>` showing 30–90: its end is its parent's.
        let open = targets(vec![
            ("from", literal(30.0)),
            ("durationInFrames", missing()),
        ]);
        let now = timing_values(TimingKind::Sequence, &open, (30.0, 90.0));
        assert_eq!(
            band_writes(TimingKind::Sequence, BandGrip::Body, 15, now, &open),
            [number("from", 45.0), number("durationInFrames", 60.0)]
        );
        // The edges write the duration anyway, or don't need it.
        assert_eq!(
            band_writes(TimingKind::Sequence, BandGrip::End, 15, now, &open),
            [number("durationInFrames", 75.0)]
        );
        assert_eq!(
            band_writes(TimingKind::Sequence, BandGrip::Start, 15, now, &open),
            [number("from", 45.0), number("durationInFrames", 45.0)]
        );

        // A written duration is left as it is.
        let closed = targets(vec![
            ("from", literal(30.0)),
            ("durationInFrames", literal(60.0)),
        ]);
        assert_eq!(
            band_writes(TimingKind::Sequence, BandGrip::Body, 15, now, &closed),
            [number("from", 45.0)]
        );
    }

    #[test]
    fn a_notice_at_the_release_says_nothing_was_written() {
        assert_eq!(
            not_written("Scanning scenes…"),
            "Not written: Scanning scenes…"
        );
    }

    #[test]
    fn band_delta_keeps_duration_and_series_start() {
        let now = values(30.0, 60.0);
        assert_eq!(clamp_band_delta(BandGrip::Start, 70, now, None), 59);
        assert_eq!(clamp_band_delta(BandGrip::End, -70, now, None), -59);
        // A `Sequence` may start before its parent.
        assert_eq!(clamp_band_delta(BandGrip::Body, -100, now, None), -100);
        assert_eq!(clamp_band_delta(BandGrip::End, 500, now, None), 500);

        let item = values(0.0, 60.0);
        assert_eq!(clamp_band_delta(BandGrip::Body, -30, item, Some(20.0)), -20);
        assert_eq!(
            clamp_band_delta(BandGrip::Start, -30, item, Some(20.0)),
            -20
        );
        assert_eq!(clamp_band_delta(BandGrip::Body, 30, item, Some(20.0)), 30);
        // The end leaves the item's start where it is.
        assert_eq!(clamp_band_delta(BandGrip::End, -30, item, Some(20.0)), -30);
    }

    #[test]
    fn drag_frames_is_unclamped_at_every_zoom() {
        assert_eq!(drag_frames(100.0, 110.0, 0.25), 40.0);
        assert!((drag_frames(100.0, 110.0, 3.0) - 10.0 / 3.0).abs() < 1e-9);
        assert_eq!(drag_frames(100.0, 90.0, 1.0), -10.0);
        // Past the lane's right edge (1000 px wide from 0), the delta keeps
        // growing.
        assert_eq!(drag_frames(900.0, 1200.0, 1.0), 300.0);
        assert_eq!(drag_frames(900.0, 1400.0, 1.0), 500.0);
    }

    fn chapter(id: &str, from: u64, duration_in_frames: u64) -> ReactChapter {
        ReactChapter {
            id: id.to_owned(),
            name: id.to_owned(),
            from,
            duration_in_frames,
        }
    }

    #[test]
    fn snapping_takes_the_nearest_edge_within_six_pixels() {
        // The end edge at 90, 2 px a frame.
        assert_eq!(snapped_delta(&[90.0], 10.0, &[102.0], 2.0, false), 12);
        assert_eq!(snapped_delta(&[90.0], 10.0, &[104.0], 2.0, false), 10);
        // A body drag snaps whichever edge is nearer its target.
        assert_eq!(
            snapped_delta(&[30.0, 90.0], 10.0, &[41.0, 103.0], 2.0, false),
            11
        );
        assert_eq!(
            snapped_delta(&[30.0, 90.0], 10.0, &[43.0, 101.0], 2.0, false),
            11
        );

        let chapters = [chapter("intro", 0, 120), chapter("outro", 120, 60)];
        let still = [(130.0, 150.0)];
        let targets = snap_targets(45.0, &chapters, &still, (30.0, 90.0));
        for edge in [45.0, 0.0, 120.0, 180.0, 130.0, 150.0] {
            assert!(targets.contains(&edge), "{edge} in {targets:?}");
        }
        assert!(!targets.contains(&30.0));
        assert!(!targets.contains(&90.0));
    }

    #[test]
    fn snap_targets_leave_out_the_window_and_what_moves() {
        // The playhead and a chapter edge on the window's own edges.
        let targets = snap_targets(30.0, &[chapter("intro", 0, 90)], &[], (30.0, 90.0));
        assert_eq!(targets, [0.0]);

        // Three items of a `Series`, the middle one dragged; a fourth
        // instance elsewhere.
        let items = element(vec![
            instance(10, vec![(0, 60)], vec![pos(4)]),
            instance(20, vec![(60, 120)], vec![pos(8)]),
            instance(30, vec![(120, 180)], vec![pos(12)]),
            instance(40, vec![(300, 330)], vec![]),
        ]);
        let dragged = |grip| DragPreview {
            source: pos(8),
            window: (60.0, 120.0),
            grip,
            delta: 0,
            followers: vec![pos(12)],
        };
        let all = ImpactFilter::All;
        assert_eq!(
            still_ranges(&items, &dragged(BandGrip::End), &all),
            [(0.0, 60.0), (300.0, 330.0)]
        );
        assert_eq!(
            still_ranges(&items, &dragged(BandGrip::Body), &all),
            [(0.0, 60.0), (300.0, 330.0)]
        );
        // The start edge leaves the later items where they are.
        assert_eq!(
            still_ranges(&items, &dragged(BandGrip::Start), &all),
            [(0.0, 60.0), (120.0, 180.0), (300.0, 330.0)]
        );
        // Instances the bands don't show are no targets.
        let through = ImpactFilter::Through(pos(40));
        assert_eq!(
            still_ranges(&items, &dragged(BandGrip::End), &through),
            [(300.0, 330.0)]
        );
    }

    #[test]
    fn shift_turns_snapping_off() {
        assert_eq!(snapped_delta(&[90.0], 10.0, &[102.0], 2.0, true), 10);
        assert_eq!(snapped_delta(&[90.0], 10.4, &[90.0], 2.0, true), 10);
    }

    #[test]
    fn deltas_are_whole_frames_at_every_zoom() {
        for pixels_per_frame in [0.25, 1.0, 7.5] {
            for x in [1.0_f32, 7.0, 13.0, 333.0] {
                let raw = drag_frames(0.0, x, pixels_per_frame);
                let delta = snapped_delta(&[90.0], raw, &[], pixels_per_frame, false);
                assert_eq!(delta, raw.round() as i64);
            }
        }
        // A target 20 frames away is within 6 px at 0.25 px a frame.
        assert_eq!(snapped_delta(&[90.0], 0.0, &[110.0], 0.25, false), 20);
        assert_eq!(snapped_delta(&[90.0], 0.0, &[110.0], 1.0, false), 0);
        // A fractional target lands on a whole frame.
        assert_eq!(snapped_delta(&[90.0], 0.0, &[90.6], 7.5, false), 1);
    }

    fn instance(
        chain_line: u32,
        ranges: Vec<(u64, u64)>,
        wrappers: Vec<SourcePos>,
    ) -> InstanceOccurrences {
        InstanceOccurrences {
            chain: vec![pos(100), pos(chain_line)],
            ranges,
            max_per_frame: 1,
            wrappers,
        }
    }

    fn element(instances: Vec<InstanceOccurrences>) -> ElementOccurrences {
        ElementOccurrences {
            element: pos(100),
            instances,
        }
    }

    fn preview(grip: BandGrip, delta: i64, followers: Vec<SourcePos>) -> DragPreview {
        DragPreview {
            source: pos(4),
            window: (30.0, 90.0),
            grip,
            delta,
            followers,
        }
    }

    fn ranges(element: &ElementOccurrences) -> Vec<Vec<(u64, u64)>> {
        element
            .instances
            .iter()
            .map(|instance| instance.ranges.clone())
            .collect()
    }

    #[test]
    fn preview_moves_the_instances_through_the_window() {
        let through = vec![pos(4), pos(2)];
        let shown = element(vec![
            instance(10, vec![(30, 90)], through.clone()),
            instance(20, vec![(40, 60), (70, 90)], through.clone()),
            instance(30, vec![(200, 260)], vec![pos(8)]),
        ]);
        let moved =
            |grip, delta| ranges(&preview_occurrences(&shown, &preview(grip, delta, vec![])));

        assert_eq!(
            moved(BandGrip::Body, 15),
            [vec![(45, 105)], vec![(55, 75), (85, 105)], vec![(200, 260)]]
        );
        assert_eq!(
            moved(BandGrip::Start, 15),
            [vec![(45, 90)], vec![(55, 75), (85, 90)], vec![(200, 260)]]
        );
        // Pulled back, a range that ran to the window's end still does: the
        // end edge stays where it is.
        assert_eq!(
            moved(BandGrip::Start, -15),
            [vec![(15, 90)], vec![(25, 45), (55, 90)], vec![(200, 260)]]
        );
        assert_eq!(
            moved(BandGrip::End, -15),
            [vec![(30, 75)], vec![(40, 60), (70, 75)], vec![(200, 260)]]
        );
        assert_eq!(
            moved(BandGrip::End, 15),
            [vec![(30, 105)], vec![(40, 60), (70, 105)], vec![(200, 260)]]
        );
        // A range the new end cuts away is gone.
        assert_eq!(moved(BandGrip::End, -35)[1], vec![(40, 55)]);
        // Nothing moves before frame 0.
        assert_eq!(moved(BandGrip::Body, -50)[0], vec![(0, 40)]);
        // The chains, the copies and the wrappers stay.
        let kept = preview_occurrences(&shown, &preview(BandGrip::Body, 15, vec![]));
        assert_eq!(kept.element, shown.element);
        assert_eq!(kept.instances[1].chain, shown.instances[1].chain);
        assert_eq!(kept.instances[1].wrappers, through);
    }

    #[test]
    fn a_series_drag_moves_the_following_items() {
        // Three items of 60 frames at 4, 8 and 12; the middle one is dragged.
        let at = |line| SourcePos {
            column: 5,
            ..pos(line)
        };
        let items = element(vec![
            instance(10, vec![(0, 60)], vec![at(4)]),
            instance(20, vec![(60, 120)], vec![at(8)]),
            instance(30, vec![(120, 180)], vec![at(12)]),
        ]);
        let dragged = |grip, delta| DragPreview {
            source: at(8),
            window: (60.0, 120.0),
            grip,
            delta,
            followers: vec![at(12)],
        };
        let moved = |grip, delta| ranges(&preview_occurrences(&items, &dragged(grip, delta)));

        assert_eq!(
            moved(BandGrip::End, 15),
            [vec![(0, 60)], vec![(60, 135)], vec![(135, 195)]]
        );
        assert_eq!(
            moved(BandGrip::Body, 15),
            [vec![(0, 60)], vec![(75, 135)], vec![(135, 195)]]
        );
        // The start edge's offset and duration cancel: the next item stays.
        assert_eq!(
            moved(BandGrip::Start, 15),
            [vec![(0, 60)], vec![(75, 120)], vec![(120, 180)]]
        );
    }

    #[test]
    fn drag_label_names_the_changed_props() {
        let before = values(30.0, 60.0);
        assert_eq!(
            drag_label(
                TimingKind::Sequence,
                BandGrip::Body,
                before,
                values(45.0, 60.0)
            ),
            "from 30 → 45"
        );
        assert_eq!(
            drag_label(
                TimingKind::Sequence,
                BandGrip::End,
                before,
                values(30.0, 75.0)
            ),
            "durationInFrames 60 → 75"
        );
        assert_eq!(
            drag_label(
                TimingKind::SeriesSequence,
                BandGrip::Body,
                values(0.0, 60.0),
                values(15.0, 60.0)
            ),
            "offset 0 → 15"
        );
        assert_eq!(
            drag_label(
                TimingKind::Sequence,
                BandGrip::Start,
                before,
                values(45.0, 45.0)
            ),
            "from 30 → 45 · durationInFrames 60 → 45"
        );
    }

    #[test]
    fn drag_end_tells_a_click_from_a_drag() {
        assert_eq!(drag_end(2.0, 0, false), DragEnd::Seek);
        assert_eq!(drag_end(2.9, 1, false), DragEnd::Seek);
        assert_eq!(drag_end(40.0, 15, true), DragEnd::Cancel);
        assert_eq!(drag_end(2.0, 0, true), DragEnd::Cancel);
        assert_eq!(drag_end(40.0, 0, false), DragEnd::Cancel);
        assert_eq!(drag_end(3.0, 1, false), DragEnd::Commit);
        assert_eq!(drag_end(40.0, -15, false), DragEnd::Commit);
    }
}
