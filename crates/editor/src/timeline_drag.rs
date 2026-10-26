//! Dragging the selected element's own timing window on the timeline: the
//! band holding the lookup frame takes grips at the window's edges; a drag
//! moves the bands and a label with the pointer, writes nothing until the
//! button comes up, and then writes the window's props as one history
//! entry. Every rule sits in a pure helper here.

use crate::element_edit::{
    ElementTargets, PropKey, WindowTargets, editable, inner_target, reason_text, window_files,
};
use crate::impact::{ImpactView, moved_impact_view};
use crate::layer_select::SourceLookup;
use crate::scope::{
    CommitGate, SceneCalls, Scope, ScopeState, chosen_scope, commit_gate, gesture_lock, scope_state,
};
use crate::timing::{WindowRole, window_role};
use crate::view::EditorView;
use celesta_react_bridge::{
    ElementOccurrences, InstanceOccurrences, LayerSourceReport, ReactChapter, SourcePos,
};
use celesta_source_edit::{EditTarget, EditValue, TimingKind, format_number};
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

/// Playhead, chapter edges and the other bands' edges; the dragged band's
/// own edges left out.
pub(crate) fn snap_targets(
    playhead: f64,
    chapters: &[ReactChapter],
    bands: &[(f64, f64)],
    dragged: usize,
) -> Vec<f64> {
    let chapter_edges = chapters
        .iter()
        .flat_map(|chapter| [chapter.from as f64, chapter.end() as f64]);
    let band_edges = bands
        .iter()
        .enumerate()
        .filter(|&(index, _)| index != dragged)
        .flat_map(|(_, &(from, to))| [from, to]);
    std::iter::once(playhead)
        .chain(chapter_edges)
        .chain(band_edges)
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
    let through = |wrappers: &[SourcePos], at: &SourcePos| wrappers.contains(at);
    let instances = element
        .instances
        .iter()
        .map(|instance| {
            let moved = |(from, to): (u64, u64)| -> Option<(u64, u64)> {
                let (from, to) = (from as i64, to as i64);
                let (from, to) = if through(&instance.wrappers, &preview.source) {
                    let ended = to == old_end;
                    match preview.grip {
                        BandGrip::Body => (from + delta, to + delta),
                        BandGrip::Start if ended => (from + delta, new_end),
                        BandGrip::Start => (from + delta, (to + delta).min(new_end)),
                        BandGrip::End if ended => (from, new_end),
                        BandGrip::End => (from, to.min(new_end)),
                    }
                } else if preview
                    .followers
                    .iter()
                    .any(|follower| through(&instance.wrappers, follower))
                    && preview.grip != BandGrip::Start
                {
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

/// The drag label's second line: the scope the write goes under.
// Design 3's scope control names a scope this way too.
fn scope_text(scope: Scope) -> &'static str {
    match scope {
        Scope::ThisScene => "Edits change this scene",
        Scope::Shared => "Edits change the shared value",
    }
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
    /// The frame a click seeks to: the band's first, as a click on a band
    /// does.
    seek: u64,
    /// The scope the write goes under, read at the press.
    scope: Option<Scope>,
    /// The button came up: the values are being written.
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
/// the scope, and the scene calls.
type BandWrite = (
    WindowTargets,
    LayerSourceReport,
    Option<Scope>,
    Option<SceneCalls>,
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
    /// drag runs, every instance of the element with those the drag moves
    /// moved.
    pub(crate) fn timeline_impact(&self) -> Option<ImpactView> {
        let Some(drag) = &self.band_drag else {
            return self.selection_impact();
        };
        Some(moved_impact_view(
            self.band_report()?,
            self.occurrences.as_ref().map(|(_, state)| state),
            self.react_chapters().unwrap_or_default(),
            u64::try_from(self.clock.frame()).unwrap_or(0),
            None,
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
            lines.extend(drag.scope.map(|scope| scope_text(scope).to_owned()));
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
        let lookup = self.source_lookup_frame as f64;
        let (Some(dragged), Some((Ok(window), report))) =
            (draggable_band(filled_bands, lookup), self.window_targets())
        else {
            return false;
        };
        // The item's start in its `Series`: from the window around it, or
        // the composition's start at the root.
        let parent = report.windows.get(1).map_or(0.0, |parent| parent.from);
        let series_start =
            (window.kind == TimingKind::SeriesSequence).then_some(grips.window.0 - parent);
        let (state, _, remembered) = self.window_scope_state(window, report);
        let scope = match &state {
            ScopeState::Options(options) => chosen_scope(remembered, options).ok(),
            ScopeState::NotShared | ScopeState::Unknown { .. } => None,
        };
        let drag = BandDrag {
            preview: DragPreview {
                source: window.source.clone(),
                window: grips.window,
                grip,
                delta: 0,
                followers: window.followers.clone(),
            },
            kind: window.kind,
            values: timing_values(window.kind, &window.targets, grips.window),
            series_start,
            start_x: x,
            moved_px: 0.0,
            snap_targets: snap_targets(
                self.clock.frame() as f64,
                self.react_chapters().unwrap_or_default(),
                bands,
                dragged,
            ),
            seek: bands[dragged].0 as u64,
            scope,
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

    /// The button came up: a click seeks to the band's first frame, a drag
    /// writes its values as one history entry, and a drag back to where it
    /// started writes nothing. True when a drag was running.
    pub(crate) fn end_band_drag(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(drag) = self.band_drag.as_ref().filter(|drag| !drag.released) else {
            return false;
        };
        match drag_end(drag.moved_px, drag.preview.delta, false) {
            DragEnd::Seek => {
                let seek = drag.seek;
                self.band_drag = None;
                self.seek_to_frame(seek, cx);
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

    /// Writes the drag's values into the window's props under the scope in
    /// effect, never held: a gate that would hold or refuse the commit, a
    /// prop the scope can't write, or a failed write leaves the notice why
    /// beside the grip and writes nothing. The drag stays, released, until
    /// the write is done.
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
        let values = band_edit(drag.kind, preview.grip, preview.delta, drag.values);
        let source = preview.source.clone();
        let (window, report, scope, calls) = match self.band_write(&source, &values) {
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
                if let Err(text) = result {
                    this.band_notice = Some(BandNotice { at, text });
                }
                this.band_drag = None;
                cx.notify();
            },
            cx,
        );
    }

    /// What writing `values` into the window written at `source` takes, or
    /// why it can't be written now: the window is not current (a reload
    /// while dragging), the scope gate holds or refuses, or the scope in
    /// effect can't write one of the props.
    fn band_write(
        &self,
        source: &SourcePos,
        values: &[(PropKey, Option<EditValue>)],
    ) -> Result<BandWrite, String> {
        let (window, report) = match self.window_targets() {
            Some((Ok(window), report)) if window.source == *source => (window, report),
            Some((Err(reason), _)) => return Err(reason.to_owned()),
            Some((Ok(_), _)) | None => {
                return Err(self
                    .element_edit
                    .unavailable(self.selected_layer.as_deref()));
            }
        };
        let keys: Vec<PropKey> = values.iter().map(|(key, _)| key.clone()).collect();
        let (state, calls, remembered) = self.window_scope_state(window, report);
        let scope = match commit_gate(&state, remembered, &window.targets, &keys) {
            CommitGate::Go(scope) => scope,
            CommitGate::Hold(note) | CommitGate::Refuse(note) => return Err(note),
        };
        if let Some(lock) = keys
            .iter()
            .find_map(|key| gesture_lock(scope, &window.targets, key, calls))
        {
            return Err(lock.text());
        }
        Ok((window.clone(), report.clone(), scope, calls.cloned()))
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
        let bands = [(30.0, 90.0), (130.0, 150.0)];
        let targets = snap_targets(45.0, &chapters, &bands, 0);
        for edge in [45.0, 0.0, 120.0, 180.0, 130.0, 150.0] {
            assert!(targets.contains(&edge), "{edge} in {targets:?}");
        }
        assert!(!targets.contains(&30.0));
        assert!(!targets.contains(&90.0));
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
