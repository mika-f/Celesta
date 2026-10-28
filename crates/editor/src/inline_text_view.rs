//! Editing a text layer in place on the canvas: a double-click enters an
//! edit, the keys and the IME change it while the held scene is drawn with
//! it, a press places the caret, and every way an edit ends goes through
//! [`edit_end`]. The caret, the selection and the marked text are painted
//! over the frame.

use crate::element_edit::{
    ElementTargets, PropKey, edit_value, editable, inner_target, reason_text,
};
use crate::element_rows::{HeldCommits, value_text};
use crate::inline_text::{
    EDGE_ON, TextEdit, TextMap, caret, convex, from_utf16, index_at, selection_quads, to_utf16,
};
use crate::layer_select::{SourceLookup, letterbox, scene_point, to_scene};
use crate::preview::TextOverride;
use crate::scope::{CommitGate, Scope};
use crate::view::EditorView;
use celesta_composition::Point;
use celesta_editor_theme as theme;
use celesta_gpu_renderer::{TextLayout, hit_test};
use celesta_react_bridge::LayerSourceReport;
use celesta_source_edit::{EditTarget, EditValue};
use gpui_kit::base::actions::{SelectDown, SelectLeft, SelectRight, SelectUp};
use gpui_kit::component::input::{
    Backspace, Copy, Cut, Delete, Enter, Escape, IndentInline, MoveDown, MoveEnd, MoveHome,
    MoveLeft, MoveRight, MoveUp, OutdentInline, Paste, Redo, SelectAll, SelectToEndOfLine,
    SelectToStartOfLine, Undo,
};
use gpui_kit::{
    Bounds, ClipboardItem, Context, Div, EntityInputHandler, InteractiveElement as _,
    MouseDownEvent, MouseMoveEvent, PathBuilder, Pixels, UTF16Selection, Window, point, px,
};
use std::ops::Range;

/// Why a double-click while the code reloads does not edit.
const RELOADING: &str = "The code is reloading";
/// Why a double-click on a text the shown frame recorded no box for (or one
/// with a corner behind the viewer) does not edit.
const NOT_DRAWN: &str = "The text isn't drawn in this frame";
/// Why a double-click on a text without a writable `text` row does not edit
/// when nothing more precise is known.
const NOT_EDITABLE: &str = "The text can't be edited here";
/// Why an edit ended when the file changed outside the editor.
const SOURCE_CHANGED: &str = "The source changed while editing";

/// A text being edited on the canvas.
#[derive(Debug)]
pub(crate) struct InlineTextSession {
    pub(crate) layer: String,
    pub(crate) edit: TextEdit,
    /// The text the edit started from: the `children` the code passed.
    pub(crate) original: String,
    /// The edited layer's layout as the newest frame drew it; it lags the
    /// edit until that edit's frame arrives.
    pub(crate) layout: Option<TextLayout>,
    /// From `layout` to the box the newest frame drew the text in.
    pub(crate) map: Option<TextMap>,
    /// A press in the text is held: moving the pointer extends the
    /// selection.
    pub(crate) selecting: bool,
    /// The layout or the map changed since the IME was told where the text
    /// is.
    pub(crate) moved: bool,
    /// The edit scope the control showed when the edit started: the text
    /// is written under it or refused (see `EditorView::commit_gesture_row`).
    pub(crate) scope: Option<Scope>,
}

impl InlineTextSession {
    pub(crate) fn new(layer: String, original: String) -> Self {
        Self {
            layer,
            edit: TextEdit::new(original.clone()),
            original,
            layout: None,
            map: None,
            selecting: false,
            moved: false,
            scope: None,
        }
    }
}

/// What a double-click on a `Text` does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum InlineEntry {
    /// Edit, starting from this text.
    Begin(String),
    /// The layer's source is still being looked up (or read): decide once
    /// it answers.
    Wait,
    /// Not editable, for this reason.
    Refuse(String),
}

/// The text the code passed to a `Text` as its `children`: what an edit
/// starts from. Empty when it passed none.
fn received_text(report: &LayerSourceReport) -> String {
    report
        .props
        .get("children")
        .map(|value| value_text(&edit_value(value)))
        .unwrap_or_default()
}

/// Whether a double-clicked layer is a `Text` for an inline edit: its
/// report says so once the lookup answers with one; until then (and
/// without one) the frame's `HitRegion::text_box` does, which the renderer
/// records for every drawn text. Anything else is no entry, and the press
/// goes on as usual.
pub(crate) fn is_text_layer(lookup: &SourceLookup, drawn_text: bool) -> bool {
    match lookup {
        SourceLookup::Found(report) => report
            .chain
            .first()
            .is_some_and(|element| element.name == "Text"),
        SourceLookup::Pending
        | SourceLookup::NotInFrame
        | SourceLookup::NoPosition
        | SourceLookup::Failed(_) => drawn_text,
    }
}

/// A double-click on a `Text`: `text_box` is its `HitRegion::text_box` in
/// the frame shown, and `layout` its layout when that frame measured one.
/// `targets` are the layer's current targets; without them,
/// `lookup_pending` says they are still coming and `unavailable` why there
/// are none.
pub(crate) fn inline_entry(
    targets: Option<(&ElementTargets, &LayerSourceReport)>,
    lookup_pending: bool,
    unavailable: Option<&str>,
    reloading: bool,
    text_box: Option<[Point; 4]>,
    layout: Option<&TextLayout>,
) -> InlineEntry {
    if reloading {
        return InlineEntry::Refuse(RELOADING.to_owned());
    }
    let Some((targets, report)) = targets else {
        return if lookup_pending {
            InlineEntry::Wait
        } else {
            InlineEntry::Refuse(unavailable.unwrap_or(NOT_EDITABLE).to_owned())
        };
    };
    match targets.target(&PropKey::Prop("children".to_owned())) {
        Some(target) => {
            if let EditTarget::Locked(reason) = inner_target(target) {
                return InlineEntry::Refuse(reason_text(reason));
            }
            if !editable(target) {
                return InlineEntry::Refuse(NOT_EDITABLE.to_owned());
            }
        }
        None => return InlineEntry::Refuse(NOT_EDITABLE.to_owned()),
    }
    let Some(text_box) = text_box else {
        return InlineEntry::Refuse(NOT_DRAWN.to_owned());
    };
    // Without a layout yet (the frame measured none), the box alone says
    // whether the text faces the viewer.
    let faces = match layout {
        Some(layout) => TextMap::fit(layout, text_box).is_ok(),
        None => convex(text_box),
    };
    if !faces {
        return InlineEntry::Refuse(EDGE_ON.to_owned());
    }
    InlineEntry::Begin(received_text(report))
}

/// Something that may end an edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EditEvent {
    Enter,
    Escape,
    /// The editing element lost the focus (a click in the Properties tab,
    /// the layer list, the tool row).
    Blur,
    /// A press on the canvas, inside the text's layout box or not.
    CanvasPress {
        inside: bool,
    },
    /// Another layer is selected, or the selection is cleared, on request.
    SelectionChange,
    /// The selection was cleared by a reload or the layer disappearing;
    /// `outside_change` when the file changed outside the editor.
    SelectionCleared {
        outside_change: bool,
    },
    ToggleInteractive,
    Seek,
    Play,
    /// New, Open, Reload, Close or Export.
    ProjectCommand,
}

/// How an edit ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EditEnd {
    Keep,
    /// Written (when changed); `refocus_root` hands the focus back to the
    /// editor, so Cmd+Z then undoes the commit.
    Commit {
        refocus_root: bool,
    },
    /// Not written; `reason` goes to the viewer's notice.
    Cancel {
        reason: Option<&'static str>,
        refocus_root: bool,
    },
}

/// What `event` does to a running edit. Only a blur leaves the focus where
/// it went: every other event comes with the focus still on the edit (a
/// press elsewhere blurs it first), whose element goes with it.
pub(crate) fn edit_end(event: EditEvent) -> EditEnd {
    match event {
        EditEvent::CanvasPress { inside: true } => EditEnd::Keep,
        EditEvent::Blur => EditEnd::Commit {
            refocus_root: false,
        },
        EditEvent::Escape => EditEnd::Cancel {
            reason: None,
            refocus_root: true,
        },
        EditEvent::SelectionCleared { outside_change } => EditEnd::Cancel {
            reason: outside_change.then_some(SOURCE_CHANGED),
            refocus_root: true,
        },
        EditEvent::Enter
        | EditEvent::CanvasPress { inside: false }
        | EditEvent::SelectionChange
        | EditEvent::ToggleInteractive
        | EditEvent::Seek
        | EditEvent::Play
        | EditEvent::ProjectCommand => EditEnd::Commit { refocus_root: true },
    }
}

/// Whether a canvas press while editing is in the text: inside the layout
/// box (`in_box`) once the frame drawn with the edit has fitted the map;
/// before that, when it hits the edited layer, so a triple-click's third
/// press neither commits nor moves the caret.
pub(crate) fn press_inside(in_box: Option<bool>, hits_layer: bool) -> bool {
    in_box.unwrap_or(hits_layer)
}

/// Why a commit that `event` ends an edit with is refused, when the edit
/// stays open instead of losing its text: the gesture's `gate` (`None`
/// while the commit would wait its turn) refuses a text that `writes`
/// (a gesture is never held for the scan: a hold is one more refusal),
/// and the person ended the edit (Enter, a blur, a press outside the
/// text), so it can be committed again under the scope the control shows
/// now. When the selection, the playhead or the project moves on, the
/// edit ends whatever the gate says.
pub(crate) fn inline_commit_refusal(
    event: EditEvent,
    writes: bool,
    gate: Option<&CommitGate>,
) -> Option<String> {
    let ended_by_the_person = matches!(
        event,
        EditEvent::Enter | EditEvent::Blur | EditEvent::CanvasPress { inside: false }
    );
    match gate {
        Some(CommitGate::Refuse(refusal) | CommitGate::Hold(refusal))
            if writes && ended_by_the_person =>
        {
            Some(refusal.clone())
        }
        _ => None,
    }
}

/// What a committed edit of `original` writes: `edited` as the `text`
/// row's value; nothing when it is unchanged, or empty (the row is
/// required, so an emptied text shows the source again).
pub(crate) fn inline_commit_value(original: &str, edited: &str) -> Option<EditValue> {
    (edited != original && !edited.is_empty()).then(|| EditValue::String(edited.to_owned()))
}

/// Where the override that draws a committed text stands, as a released
/// scrub's does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum KeptOverride {
    /// The commit waits its turn or is being written.
    Writing,
    /// Written: kept until a reload numbered this or later draws it.
    UntilReload(u64),
    /// None is kept.
    #[default]
    Dropped,
}

/// Where the override stands once its commit's write finished: dropped on
/// an error (the file holds the old text) and when the targets are still
/// current (the text came out as it was, so no reload comes); otherwise
/// kept for the reload that reads the write, which is numbered
/// `next_reload` or later.
pub(crate) fn kept_override_after_write(
    result: &Result<(), String>,
    targets_current: bool,
    next_reload: u64,
) -> KeptOverride {
    match result {
        Ok(()) if !targets_current => KeptOverride::UntilReload(next_reload),
        Ok(()) | Err(_) => KeptOverride::Dropped,
    }
}

/// Whether the override is still kept once reload `sequence` applies: any
/// reload numbered `from` or later was requested after the write and read
/// the written file, whether it keeps the selection or not (an outside
/// change's may supersede the own write's, which then never applies).
pub(crate) fn kept_override_on_reload(kept: &KeptOverride, sequence: u64) -> bool {
    match kept {
        KeptOverride::Writing => true,
        KeptOverride::UntilReload(from) => sequence < *from,
        KeptOverride::Dropped => false,
    }
}

/// The override that draws `session`'s text once committed: what it shows,
/// a composition still marked accepted.
pub(crate) fn committed_override(session: &mut InlineTextSession) -> TextOverride {
    session.edit.unmark();
    TextOverride {
        layer: session.layer.clone(),
        original: session.original.clone(),
        text: session.edit.text().to_owned(),
    }
}

/// Whether the override waits on a commit `held` (not yet being written):
/// forgetting or dropping the held commits then drops it, as nothing will
/// write its text.
pub(crate) fn held_commit_override(
    kept: &KeptOverride,
    text_override: Option<&TextOverride>,
    held: &HeldCommits,
) -> bool {
    *kept == KeptOverride::Writing
        && text_override.is_some_and(|kept| held.contains(&kept.layer, &children_key()))
}

/// The `text` row's key.
fn children_key() -> PropKey {
    PropKey::Prop("children".to_owned())
}

/// The caret a press at `position` (window pixels) puts in the text, when
/// it is inside the layout box `map` was fitted for: the viewer's frame
/// fitted into `bounds`, the scene point mapped back into the layout.
pub(crate) fn pressed_index(
    bounds: Bounds<Pixels>,
    frame_size: (f32, f32),
    position: gpui_kit::Point<Pixels>,
    layout: &TextLayout,
    map: &TextMap,
) -> Option<(usize, bool)> {
    let scene = to_scene(bounds, frame_size, position)?;
    let local = map.to_layout(scene).filter(|local| map.contains(*local))?;
    Some(index_at(layout, local))
}

/// The override that draws `session`'s layer with what the edit shows.
pub(crate) fn live_override(session: &InlineTextSession) -> TextOverride {
    TextOverride {
        layer: session.layer.clone(),
        original: session.original.clone(),
        text: session.edit.display_text(),
    }
}

/// Ends `session` without writing: it goes, and so does its layer's
/// override, so the next frame draws the code's own text. True when there
/// was one.
pub(crate) fn cancel_session(
    session: &mut Option<InlineTextSession>,
    text_override: &mut Option<TextOverride>,
) -> bool {
    let Some(ended) = session.take() else {
        return false;
    };
    if text_override
        .as_ref()
        .is_some_and(|live| live.layer == ended.layer)
    {
        *text_override = None;
    }
    true
}

/// What is painted over an edited text, in scene pixels.
pub(crate) struct InlinePaint {
    /// The caret, top to bottom; none while a selection shows.
    caret: Option<[Point; 2]>,
    /// One quad per line the selection covers.
    selection: Vec<[Point; 4]>,
    /// The bottom edge of each line the marked text covers.
    underline: Vec<[Point; 2]>,
}

fn inline_paint(layout: &TextLayout, map: &TextMap, edit: &TextEdit) -> InlinePaint {
    let scene = |p: Point| map.to_scene(p);
    let selection = edit.selection();
    let (head, upstream) = edit.head();
    let underline = edit
        .marked()
        .map(|marked| {
            selection_quads(layout, marked)
                .into_iter()
                .map(|[_, _, right, left]| [scene(left), scene(right)])
                .collect()
        })
        .unwrap_or_default();
    InlinePaint {
        caret: selection
            .is_empty()
            .then(|| caret(layout, head, upstream).map(scene)),
        selection: selection_quads(layout, selection)
            .into_iter()
            .map(|quad| quad.map(scene))
            .collect(),
        underline,
    }
}

/// Paints `paint` on the frame fitted into `bounds`: the selection in the
/// accent at 30%, the caret and the marked text's underline as 1px accent
/// strokes, slanting with the text.
pub(crate) fn paint_inline_text(
    bounds: Bounds<Pixels>,
    frame_size: (f32, f32),
    paint: &InlinePaint,
    window: &mut Window,
) {
    let Some((left, top, scale)) = letterbox(bounds, frame_size) else {
        return;
    };
    let on_viewer = |p: Point| point(px(left + p.x as f32 * scale), px(top + p.y as f32 * scale));
    let accent = theme::accent();
    for quad in &paint.selection {
        let mut path = PathBuilder::fill();
        path.move_to(on_viewer(quad[0]));
        for corner in &quad[1..] {
            path.line_to(on_viewer(*corner));
        }
        path.close();
        if let Ok(path) = path.build() {
            window.paint_path(path, accent.opacity(0.3));
        }
    }
    for [from, to] in paint.underline.iter().chain(&paint.caret) {
        let mut path = PathBuilder::stroke(px(1.0));
        path.move_to(on_viewer(*from));
        path.line_to(on_viewer(*to));
        if let Ok(path) = path.build() {
            window.paint_path(path, accent);
        }
    }
}

/// Where the IME places its candidate window for `range` (code points):
/// the caret at its start when it is empty, else the first line it covers,
/// in window pixels on the frame fitted into `bounds`.
fn ime_bounds(
    layout: &TextLayout,
    map: &TextMap,
    range: Range<usize>,
    bounds: Bounds<Pixels>,
    frame_size: (f32, f32),
) -> Option<Bounds<Pixels>> {
    let (left, top, scale) = letterbox(bounds, frame_size)?;
    let start = range.start;
    let corners: Vec<Point> = match selection_quads(layout, range).first() {
        Some(quad) => quad.to_vec(),
        None => caret(layout, start, false).to_vec(),
    };
    let on_viewer = corners.into_iter().map(|p| {
        let p = map.to_scene(p);
        (left + p.x as f32 * scale, top + p.y as f32 * scale)
    });
    let (min, max) = on_viewer.fold(
        (
            (f32::INFINITY, f32::INFINITY),
            (f32::NEG_INFINITY, f32::NEG_INFINITY),
        ),
        |((min_x, min_y), (max_x, max_y)), (x, y)| {
            ((min_x.min(x), min_y.min(y)), (max_x.max(x), max_y.max(y)))
        },
    );
    Some(Bounds::from_corners(
        point(px(min.0), px(min.1)),
        point(px(max.0), px(max.1)),
    ))
}

impl EditorView {
    /// What a double-click on the selected `layer` does, when it is a
    /// `Text` (or may be one: its lookup has not answered yet); `None`
    /// otherwise.
    pub(crate) fn inline_entry_for(&self, layer: &str) -> Option<InlineEntry> {
        let text_box = self
            .hit_regions
            .iter()
            .filter(|region| region.layer_id == layer)
            .find_map(|region| region.text_box);
        let lookup = self.source_lookup.as_ref()?;
        if !is_text_layer(lookup, text_box.is_some()) {
            return None;
        }
        // A committed text still drawn by its override goes first: until its
        // reload (or its end), the targets are those it is being written to.
        let kept = self.kept_override != KeptOverride::Dropped;
        let targets = self.element_targets().filter(|_| !kept);
        // A text edit that would be refused when committed (the scope it
        // keeps can't write it, or would not show it in this scene, or the
        // scenes are being scanned) says why.
        if targets.is_some()
            && let Some(refusal) = self.gesture_refusal(&children_key())
        {
            return Some(InlineEntry::Refuse(refusal));
        }
        let (lookup_pending, unavailable) = match lookup {
            SourceLookup::Pending => (true, None),
            SourceLookup::Found(_) if kept => (true, None),
            // Being read, or waiting for an own write's reload: the targets
            // are coming.
            SourceLookup::Found(_) => (
                targets.is_none() && !self.element_edit.failed(Some(layer)),
                targets
                    .is_none()
                    .then(|| self.element_edit.unavailable(Some(layer))),
            ),
            SourceLookup::NotInFrame => (false, Some("Not in this frame".to_owned())),
            SourceLookup::NoPosition => (false, Some("No source position".to_owned())),
            SourceLookup::Failed(error) => (false, Some(error.clone())),
        };
        // The frame measured the layer's layout only when it drew an
        // override of it (a commit waiting for its reload).
        let layout = self.text_layout.as_ref().filter(|_| {
            self.text_override
                .as_ref()
                .is_some_and(|live| live.layer == layer)
        });
        Some(inline_entry(
            targets,
            lookup_pending,
            unavailable.as_deref(),
            self.react_reloads.is_running(),
            text_box,
            layout,
        ))
    }

    /// Acts on a double-click's entry for the selected `layer`.
    pub(crate) fn enter_inline_text(
        &mut self,
        layer: String,
        entry: InlineEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match entry {
            InlineEntry::Begin(original) => self.begin_inline_text(layer, original, window, cx),
            InlineEntry::Wait => self.inline_pending = Some(layer),
            InlineEntry::Refuse(reason) => self.inline_note = Some((layer, reason)),
        }
        cx.notify();
    }

    /// Starts editing `layer`'s text from `original`, the `children` the
    /// code passed, after the Properties rows' typed text is submitted (so a
    /// value typed there first goes first). Playback pauses: its next frame
    /// would end the edit. The frame is redrawn from the held scene with the
    /// layer's own text, which measures its layout.
    pub(crate) fn begin_inline_text(
        &mut self,
        layer: String,
        original: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.submit_edited_rows(cx);
        self.pause();
        let session = InlineTextSession {
            scope: self.kept_scope(),
            ..InlineTextSession::new(layer, original)
        };
        self.text_override = Some(live_override(&session));
        self.inline_text = Some(session);
        self.inline_pending = None;
        self.inline_note = None;
        self.inline_notice = None;
        self.reuse_scene = true;
        self.refresh_preview();
        if let Some(focus) = &self.inline_text_focus {
            focus.focus(window, cx);
        }
        cx.notify();
    }

    /// The edit changed: the live frame draws what it shows, from the held
    /// scene (the code is not run); a caret move only repaints.
    pub(crate) fn inline_text_changed(&mut self, cx: &mut Context<Self>) {
        let Some(session) = &self.inline_text else {
            return;
        };
        let live = live_override(session);
        if self.text_override.as_ref() != Some(&live) {
            self.text_override = Some(live);
            self.reuse_scene = true;
            self.refresh_preview();
        }
        cx.notify();
    }

    /// Ends the edit without writing; `reason` goes to the viewer's notice.
    pub(crate) fn cancel_inline_text(&mut self, reason: Option<&str>, cx: &mut Context<Self>) {
        if self.drop_inline_text(reason) {
            self.reuse_scene = true;
            self.refresh_preview();
        }
        cx.notify();
    }

    /// Ends the edit, writing its text through the `text` row (one history
    /// entry, held while another write or a reload runs) unless it is
    /// unchanged or empty. What the edit shows is written: a composition
    /// still marked is accepted. The override keeps drawing the committed
    /// text until the reload that reads the write (see [`KeptOverride`]).
    pub(crate) fn commit_inline_text(&mut self, cx: &mut Context<Self>) {
        let Some(mut session) = self.inline_text.take() else {
            return;
        };
        let committed = committed_override(&mut session);
        self.text_layout = None;
        self.inline_notice = None;
        // The row writes for the selected layer, which every end keeps
        // until the commit is made.
        let value = inline_commit_value(&committed.original, &committed.text)
            .filter(|_| self.selected_layer.as_deref() == Some(committed.layer.as_str()));
        if let Some(value) = value {
            self.text_override = Some(committed);
            // Before the commit, which may finish at once (a failure).
            self.kept_override = KeptOverride::Writing;
            self.commit_gesture_row(&children_key(), Some(value), session.scope, cx);
        } else if self
            .text_override
            .as_ref()
            .is_some_and(|live| live.layer == committed.layer)
        {
            self.text_override = None;
        }
        // Always a frame, even when it draws what is shown: with no edit
        // running it looks the layer up, and a commit held for targets that
        // went stale while editing (an own write's reload, whose frames
        // asked nothing) waits for that answer.
        self.reuse_scene = true;
        self.refresh_preview();
        cx.notify();
    }

    /// A commit of `layer`'s `text` row finished with `result`: the
    /// override of a committed text is kept for the reload that reads it,
    /// or dropped (see [`kept_override_after_write`]). Not while a newer
    /// commit of the row waits its turn.
    pub(crate) fn inline_commit_written(
        &mut self,
        layer: Option<&str>,
        result: &Result<(), String>,
    ) {
        let ours = self.kept_override == KeptOverride::Writing
            && self
                .text_override
                .as_ref()
                .is_some_and(|kept| Some(kept.layer.as_str()) == layer);
        if !ours
            || held_commit_override(
                &self.kept_override,
                self.text_override.as_ref(),
                &self.element_rows.held,
            )
        {
            return;
        }
        // Another layer selected meanwhile says nothing about this write.
        let targets_current =
            self.selected_layer.as_deref() == layer && self.element_targets().is_some();
        // The own write's reload, if one comes, was not requested yet: it
        // gets this number.
        let next_reload = self.react_reloads.next();
        self.kept_override = kept_override_after_write(result, targets_current, next_reload);
        if self.kept_override == KeptOverride::Dropped {
            self.text_override = None;
            self.refresh_preview();
        }
        // The text reverts: the viewer says why, also when the Properties
        // tab, whose row carries the message, is not shown.
        if let Err(message) = result {
            self.inline_notice = Some(message.clone());
        }
    }

    /// Drops the override of a committed text; the caller asks for the
    /// frame. True when one was kept.
    pub(crate) fn drop_kept_override(&mut self) -> bool {
        if std::mem::take(&mut self.kept_override) == KeptOverride::Dropped {
            return false;
        }
        // A running edit's override is its own (none begins while one is
        // kept).
        if self.inline_text.is_none() {
            self.text_override = None;
        }
        true
    }

    /// The held commits are about to be forgotten or dropped: the override
    /// of a committed text among them goes too. True when it went.
    pub(crate) fn drop_held_commit_override(&mut self) -> bool {
        held_commit_override(
            &self.kept_override,
            self.text_override.as_ref(),
            &self.element_rows.held,
        ) && self.drop_kept_override()
    }

    /// Whether a committed text is waiting its turn or being written:
    /// closing or quitting waits for it.
    pub(crate) fn inline_commit_writing(&self) -> bool {
        self.kept_override == KeptOverride::Writing
    }

    /// [`cancel_inline_text`](Self::cancel_inline_text) without asking for
    /// a frame: the caller does. True when an edit ended.
    fn drop_inline_text(&mut self, reason: Option<&str>) -> bool {
        if !cancel_session(&mut self.inline_text, &mut self.text_override) {
            return false;
        }
        self.text_layout = None;
        self.inline_notice = reason.map(str::to_owned);
        true
    }

    /// Ends a running edit as [`edit_end`] decides for `event`. True when
    /// it ended.
    pub(crate) fn end_inline_text(&mut self, event: EditEvent, cx: &mut Context<Self>) -> bool {
        if self.inline_text.is_none() {
            return false;
        }
        match edit_end(event) {
            EditEnd::Keep => return false,
            EditEnd::Commit { .. } if self.keep_refused_inline_text(event, cx) => return false,
            EditEnd::Commit { refocus_root } => {
                self.inline_refocus |= refocus_root;
                self.commit_inline_text(cx);
            }
            EditEnd::Cancel {
                reason,
                refocus_root,
            } => {
                self.inline_refocus |= refocus_root;
                self.cancel_inline_text(reason, cx);
            }
        }
        true
    }

    /// A commit `event` asks for that the scope refuses (see
    /// [`inline_commit_refusal`]): the edit stays open with its text and
    /// caret, the viewer's notice says why, and the edit now keeps the
    /// scope the control shows, so committing again goes under it. True
    /// when the edit stays.
    fn keep_refused_inline_text(&mut self, event: EditEvent, cx: &mut Context<Self>) -> bool {
        let Some(session) = &self.inline_text else {
            return false;
        };
        let writes = self.selected_layer.as_deref() == Some(session.layer.as_str())
            && inline_commit_value(&session.original, &session.edit.display_text()).is_some();
        let gate = (!self.row_commit_waits())
            .then(|| self.gesture_gate_for(session.scope, &[children_key()]));
        let Some(refusal) = inline_commit_refusal(event, writes, gate.as_ref()) else {
            return false;
        };
        let scope = self.kept_scope();
        if let Some(session) = &mut self.inline_text {
            session.scope = scope;
        }
        self.inline_notice = Some(refusal);
        cx.notify();
        true
    }

    /// The selection was cleared without a request (a reload, the layer
    /// disappearing, another file): a running edit is cancelled, saying so
    /// when the file changed outside the editor. Asks for no frame.
    pub(crate) fn inline_text_selection_cleared(&mut self, outside_change: bool) {
        if self.inline_text.is_none() {
            return;
        }
        match edit_end(EditEvent::SelectionCleared { outside_change }) {
            EditEnd::Cancel {
                reason,
                refocus_root,
            } => {
                self.inline_refocus |= refocus_root;
                self.drop_inline_text(reason);
            }
            EditEnd::Keep | EditEnd::Commit { .. } => {}
        }
    }

    /// What the edit waits for until a window is at hand: a double-click
    /// waiting for its layer's source, the focus to hand back to the
    /// editor, and the IME to be told that the text moved. (A seek or play
    /// that ends it is settled with the rows' typed text, in
    /// `settle_row_requests`.)
    pub(crate) fn settle_inline_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(layer) = self.inline_pending.clone() {
            let entry = (self.selected_layer.as_ref() == Some(&layer)
                && self.inline_text.is_none())
            .then(|| self.inline_entry_for(&layer))
            .flatten();
            match entry {
                Some(InlineEntry::Wait) => {}
                Some(entry) => {
                    self.inline_pending = None;
                    self.enter_inline_text(layer, entry, window, cx);
                }
                None => self.inline_pending = None,
            }
        }
        // An edit that ended with the focus on it: the focus would stay on
        // an element no longer drawn, where the editor's keys don't reach.
        if std::mem::take(&mut self.inline_refocus)
            && self.inline_text.is_none()
            && self
                .inline_text_focus
                .as_ref()
                .is_some_and(|focus| focus.is_focused(window))
            && let Some(root) = &self.focus_handle
        {
            root.focus(window, cx);
        }
        if let Some(session) = &mut self.inline_text
            && std::mem::take(&mut session.moved)
        {
            window.invalidate_character_coordinates();
        }
    }

    /// Takes the edited layer's layout from the newest frame and fits the
    /// map to the box that frame drew it in. A frame without either keeps
    /// the previous ones, and so does a box the layout can't be fitted to
    /// (an emptied text without `maxWidth` has no width).
    pub(crate) fn refit_inline_text(&mut self) {
        let Some(session) = &mut self.inline_text else {
            return;
        };
        if let Some(layout) = &self.text_layout
            && session.layout.as_ref() != Some(layout)
        {
            session.layout = Some(layout.clone());
            session.moved = true;
        }
        let text_box = self
            .hit_regions
            .iter()
            .filter(|region| region.layer_id == session.layer)
            .find_map(|region| region.text_box);
        if let (Some(layout), Some(text_box)) = (&session.layout, text_box)
            && let Ok(map) = TextMap::fit(layout, text_box)
            && session.map.as_ref() != Some(&map)
        {
            session.map = Some(map);
            session.moved = true;
        }
    }

    /// What to paint over the edited text; nothing while its layout is for
    /// another text than the edit shows (the frame lags a keystroke).
    pub(crate) fn inline_text_paint(&self) -> Option<InlinePaint> {
        let session = self.inline_text.as_ref()?;
        let (layout, map) = (session.layout.as_ref()?, session.map.as_ref()?);
        (layout.text == session.edit.display_text())
            .then(|| inline_paint(layout, map, &session.edit))
    }

    /// A press on the canvas while editing: inside the text it places the
    /// caret (Shift extends the selection) and starts selecting; outside,
    /// it ends the edit. Before the first frame drawn with the edit, a
    /// press on the edited layer is inside and leaves the caret. True when
    /// the press is taken.
    pub(crate) fn press_inline_text(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let frame_size = self.frame_size();
        let bounds = self.viewer_bounds.get();
        let hit = bounds
            .and_then(|bounds| scene_point(bounds, frame_size, event.position))
            .and_then(|point| hit_test(&self.hit_regions, point))
            .map(str::to_owned);
        let Some(session) = &mut self.inline_text else {
            return false;
        };
        let mapped = match (bounds, &session.layout, &session.map) {
            (Some(bounds), Some(layout), Some(map)) => Some(pressed_index(
                bounds,
                frame_size,
                event.position,
                layout,
                map,
            )),
            _ => None,
        };
        let inside = press_inside(
            mapped.as_ref().map(Option::is_some),
            hit.as_deref() == Some(session.layer.as_str()),
        );
        if let Some(Some((index, upstream))) = mapped {
            session.edit.place(index, upstream, event.modifiers.shift);
            session.selecting = true;
            window.invalidate_character_coordinates();
            self.inline_text_changed(cx);
        }
        self.end_inline_text(EditEvent::CanvasPress { inside }, cx);
        inside
    }

    /// Follows a press held in the text: the selection extends to the
    /// pointer, past the text's edges too, and the IME's candidate window
    /// follows the caret.
    pub(crate) fn continue_inline_selection(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self
            .inline_text
            .as_ref()
            .is_some_and(|session| session.selecting)
            || self.release_unseen(event, cx, |this, _| {
                this.end_inline_selection();
                false
            })
        {
            return;
        }
        let frame_size = self.frame_size();
        let bounds = self.viewer_bounds.get();
        let Some(session) = self.inline_text.as_mut() else {
            return;
        };
        let (Some(bounds), Some(layout), Some(map)) = (bounds, &session.layout, &session.map)
        else {
            return;
        };
        let Some(local) =
            to_scene(bounds, frame_size, event.position).and_then(|scene| map.to_layout(scene))
        else {
            return;
        };
        let (index, upstream) = index_at(layout, local);
        if session.edit.head() != (index, upstream) {
            session.edit.place(index, upstream, true);
            window.invalidate_character_coordinates();
            cx.notify();
        }
    }

    /// The button came up: a press in the text stops selecting.
    pub(crate) fn end_inline_selection(&mut self) {
        if let Some(session) = &mut self.inline_text {
            session.selecting = false;
        }
    }

    /// Changes the edit with `change` (given the newest layout, which a
    /// line move needs), then redraws.
    fn edit_inline(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut TextEdit, Option<&TextLayout>),
    ) {
        let Some(session) = &mut self.inline_text else {
            return;
        };
        change(&mut session.edit, session.layout.as_ref());
        window.invalidate_character_coordinates();
        self.inline_text_changed(cx);
    }

    /// Copies the selection as plain text; true when there was one.
    fn copy_inline_selection(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(session) = &self.inline_text else {
            return false;
        };
        let selected = session.edit.selected_text();
        if selected.is_empty() {
            return false;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(selected));
        true
    }

    /// The edited text's element while editing: the Input bindings apply
    /// (not the editor's `!Input` shortcuts), each handled on the edit.
    /// Enter and Cmd+Enter commit, Shift+Enter breaks the line; Tab, Undo
    /// and Redo do nothing inside an edit.
    pub(crate) fn inline_text_keys(&self, area: Div, cx: &mut Context<Self>) -> Div {
        let line = |down: bool, extend: bool| {
            move |edit: &mut TextEdit, layout: Option<&TextLayout>| {
                if let Some(layout) = layout {
                    edit.move_vertical(layout, down, extend);
                }
            }
        };
        let edge = |end: bool, extend: bool| {
            move |edit: &mut TextEdit, layout: Option<&TextLayout>| {
                if let Some(layout) = layout {
                    edit.move_line_edge(layout, end, extend);
                }
            }
        };
        area.key_context("Input CanvasText")
            .on_action(cx.listener(|this, _: &Backspace, window, cx| {
                this.edit_inline(window, cx, |edit, _| edit.backspace());
            }))
            .on_action(cx.listener(|this, _: &Delete, window, cx| {
                this.edit_inline(window, cx, |edit, _| edit.delete());
            }))
            .on_action(cx.listener(|this, enter: &Enter, window, cx| {
                if enter.shift && !enter.secondary {
                    this.edit_inline(window, cx, |edit, _| edit.insert("\n"));
                } else {
                    this.end_inline_text(EditEvent::Enter, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Escape, _, cx| {
                this.end_inline_text(EditEvent::Escape, cx);
            }))
            .on_action(cx.listener(|this, _: &MoveLeft, window, cx| {
                this.edit_inline(window, cx, |edit, _| edit.move_horizontal(false, false));
            }))
            .on_action(cx.listener(|this, _: &MoveRight, window, cx| {
                this.edit_inline(window, cx, |edit, _| edit.move_horizontal(true, false));
            }))
            .on_action(cx.listener(|this, _: &SelectLeft, window, cx| {
                this.edit_inline(window, cx, |edit, _| edit.move_horizontal(false, true));
            }))
            .on_action(cx.listener(|this, _: &SelectRight, window, cx| {
                this.edit_inline(window, cx, |edit, _| edit.move_horizontal(true, true));
            }))
            .on_action(cx.listener(move |this, _: &MoveUp, window, cx| {
                this.edit_inline(window, cx, line(false, false));
            }))
            .on_action(cx.listener(move |this, _: &MoveDown, window, cx| {
                this.edit_inline(window, cx, line(true, false));
            }))
            .on_action(cx.listener(move |this, _: &SelectUp, window, cx| {
                this.edit_inline(window, cx, line(false, true));
            }))
            .on_action(cx.listener(move |this, _: &SelectDown, window, cx| {
                this.edit_inline(window, cx, line(true, true));
            }))
            .on_action(cx.listener(move |this, _: &MoveHome, window, cx| {
                this.edit_inline(window, cx, edge(false, false));
            }))
            .on_action(cx.listener(move |this, _: &MoveEnd, window, cx| {
                this.edit_inline(window, cx, edge(true, false));
            }))
            .on_action(
                cx.listener(move |this, _: &SelectToStartOfLine, window, cx| {
                    this.edit_inline(window, cx, edge(false, true));
                }),
            )
            .on_action(cx.listener(move |this, _: &SelectToEndOfLine, window, cx| {
                this.edit_inline(window, cx, edge(true, true));
            }))
            .on_action(cx.listener(|this, _: &SelectAll, window, cx| {
                this.edit_inline(window, cx, |edit, _| edit.select_all());
            }))
            .on_action(cx.listener(|this, _: &Copy, _, cx| {
                this.copy_inline_selection(cx);
            }))
            .on_action(cx.listener(|this, _: &Cut, window, cx| {
                if this.copy_inline_selection(cx) {
                    this.edit_inline(window, cx, |edit, _| edit.insert(""));
                }
            }))
            .on_action(cx.listener(|this, _: &Paste, window, cx| {
                // Plain text; the edit makes its line breaks `\n`.
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    this.edit_inline(window, cx, |edit, _| edit.insert(&text));
                }
            }))
            .on_action(cx.listener(|_, _: &Undo, _, _| {}))
            .on_action(cx.listener(|_, _: &Redo, _, _| {}))
            .on_action(cx.listener(|_, _: &IndentInline, _, _| {}))
            .on_action(cx.listener(|_, _: &OutdentInline, _, _| {}))
    }
}

/// The platform's text input (typing, IME composition, its candidate
/// window) over the edited text, in UTF-16 offsets of what the edit shows.
impl EntityInputHandler for EditorView {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let display = self.inline_text.as_ref()?.edit.display_text();
        let start = from_utf16(&display, range_utf16.start);
        let end = from_utf16(&display, range_utf16.end).max(start);
        adjusted_range.replace(to_utf16(&display, start)..to_utf16(&display, end));
        Some(display.chars().skip(start).take(end - start).collect())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let edit = &self.inline_text.as_ref()?.edit;
        let selection = edit.selection();
        let (head, _) = edit.head();
        Some(UTF16Selection {
            range: edit.selection_utf16(),
            reversed: !selection.is_empty() && head == selection.start,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.inline_text.as_ref()?.edit.marked_utf16()
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.edit_inline(window, cx, |edit, _| edit.unmark());
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_inline(window, cx, |edit, _| edit.replace_utf16(range_utf16, text));
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_inline(window, cx, |edit, _| {
            edit.mark_utf16(range_utf16, new_text, new_selected_range);
        });
    }

    /// From the newest layout, even one lagging the edit (its indices
    /// clamped to it), so the candidate window stays by the text.
    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let frame_size = self.frame_size();
        let session = self.inline_text.as_ref()?;
        let (layout, map) = (session.layout.as_ref()?, session.map.as_ref()?);
        let display = session.edit.display_text();
        let range = from_utf16(&display, range_utf16.start)..from_utf16(&display, range_utf16.end);
        ime_bounds(layout, map, range, element_bounds, frame_size)
    }

    fn character_index_for_point(
        &mut self,
        position: gpui_kit::Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let frame_size = self.frame_size();
        let bounds = self.viewer_bounds.get()?;
        let session = self.inline_text.as_ref()?;
        let (layout, map) = (session.layout.as_ref()?, session.map.as_ref()?);
        let local = map.to_layout(to_scene(bounds, frame_size, position)?)?;
        let (index, _) = index_at(layout, local);
        Some(to_utf16(&session.edit.display_text(), index))
    }
}
