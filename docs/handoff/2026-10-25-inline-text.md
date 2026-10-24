# Inline text editing (2026-10-25)

Double-clicking a selected `Text` whose `text` row is editable edits its
text in place on the canvas, in its own font, size, runs, wrapping, and
transform (flat, rotated, mirrored, or tilted in 3D), with a caret,
selection, clipboard, and IME composition (Japanese input), and commits it
through the `text` row as one history entry. While typing, the preview
worker re-renders the last evaluated scene with the layer's text replaced,
without running the code, and returns the layer's text metrics; the caret
geometry is computed in layout coordinates and mapped onto the box the
renderer drew the text in.
Design: `docs/superpowers/specs/2026-10-23-editor-property-editing-design.md`
(section "Canvas": "Inline text editing"). This is part 2d; it builds on 2b
(`docs/handoff/2026-10-24-property-sections.md`: the `text` row,
`commit_element_row`, `HeldCommits`, `submit_requested`, the released
scrub's override) and 2c (`docs/handoff/2026-10-24-canvas-tools.md`: the
hit regions, the tool row, `DragState`).
Plan: `docs/superpowers/plans/2026-10-25-editor-inline-text.md`.
User docs: `packages/website/content/{en,ja}/docs/01-(getting-started)/03-preview.md`
("Edit text on the canvas", and the Undo paragraph).

## What changed

- Text box and metrics in the renderer (`crates/gpu-renderer/src/text.rs`,
  `texture.rs`, `layer.rs`, `prepare.rs`, `hit.rs`, `renderer.rs`, `lib.rs`):
  `HitRegion::text_box: Option<[Point; 4]>` is a drawn text's layout box
  (the layout width by `lines · line_height`, from the first line's top, in
  scene pixels, in the order of `corners`), mapped through the layer's planes
  like `anchor_box`, so a text in 3D or under a perspective gets its projected
  quad; a corner behind the viewer gives `None` without dropping the region.
  The box is stored on the texture (`DecodedImage::text_box` /
  `TextureData::text_box`, layout units with the texture's `raster_scale`),
  so a cached raster, which skips measuring, keeps it.
  `PreparedLayer::text_box_corners` places it with `quad_corners_between`.
  `GpuRenderer::measure_text` wraps the rasterizer's measure;
  `GpuRenderer::text_layout(scene, layer) -> Option<TextLayout>` measures a
  text found through groups and masks (`find_layer`). `TextLayout { text,
  metrics, max_width, align }`; `TextMetrics` and `GlyphMetrics` are
  re-exported. `TextSize` carries `lines` and `line_height`; `PendingText`
  keeps the size it measured, so `measured_bounds` no longer measures again.
- The held scene and the live override (`crates/editor/src/preview.rs`,
  `refresh.rs`, `background.rs`, `view.rs`): `PreviewRequest` gains
  `text_override: Option<TextOverride { layer, original, text }>` and
  `reuse_scene: bool`; `PreviewResult` gains `text_layout`. The worker keeps
  `held: Option<(HeldKey, Scene)>`, the last successful evaluation, keyed by
  `HeldKey { time, react_reload, react }`; `scene_source` reuses it only when
  the request asks (`reuse_scene`, one-shot, set by a keystroke) and the key
  matches, so a keystroke after a seek or a reload evaluates. The override is
  applied to a clone (`replace_text`): text set, `visible_characters`
  cleared, `color_runs` and `font_runs` carried by `shift_runs` (common
  code-point prefix and suffix of `original` and the edited text; clamped to
  the edited length so a run never becomes invalid). After the render the
  worker measures the override layer (`text_layout`); `EditorView.text_layout`
  holds it.
- The edit model and its geometry (`crates/editor/src/inline_text.rs`, new):
  `TextEdit` (text, anchor, head, `upstream` affinity, marked text as the
  replaced range plus the composition, a private `goal_x` for vertical
  moves), code-point indices with UTF-16 conversions for GPUI; grapheme
  moves and deletions (`unicode-segmentation`); `insert`, `replace_utf16`,
  `mark_utf16` normalize `\r\n`/`\r`; `unmark` accepts the composition.
  `caret`, `selection_quads`, `index_at` work on a `TextLayout` in layout
  coordinates (snap to graphemes, interpolate inside a ligature, RTL leading
  edge on the right, wrap affinity, an empty line at its alignment's x).
  `TextMap::fit(layout, text_box)` is the homography from the layout
  rectangle to the box (Heckbert's square-to-quad, with the inverse from the
  adjugate); it refuses a zero-size rectangle or a box that is not convex
  ("The text is edge-on to the view", `EDGE_ON`); `to_layout` returns `None`
  beyond the plane's horizon; `contains` tests the layout rectangle.
- The view glue (`crates/editor/src/inline_text_view.rs`, new; hooks in
  `viewer.rs`, `render.rs`, `main.rs`, `playback.rs`, `lifecycle.rs`,
  `export_control.rs`, `element_rows.rs`, `transform_drag.rs`):
  `InlineTextSession { layer, edit, original, layout, map, selecting, moved }`.
  Pure: `is_text_layer`, `inline_entry` → `InlineEntry { Begin(String),
  Wait, Refuse(String) }`, `edit_end(EditEvent) -> EditEnd { Keep, Commit {
  refocus_root }, Cancel { reason, refocus_root } }`, `press_inside`,
  `pressed_index`, `live_override`, `cancel_session`,
  `inline_commit_value`, `committed_override`, `KeptOverride { Writing,
  UntilReload(u64), Dropped }` with `kept_override_after_write` and
  `kept_override_on_reload`, `held_commit_override`;
  `transform_drag::editing_drag_state` hides the handles and the cross. View:
  `EditorView` gains `inline_text`, `inline_text_focus`, `inline_pending`
  (a double-click waiting for its lookup), `inline_note` (a refusal beside
  the layer), `inline_notice` (a cancel reason in the viewer's notice),
  `inline_end_due` (a seek or play, settled in `settle_row_requests`),
  `inline_refocus`, `kept_override`. The canvas area, while editing, tracks
  `inline_text_focus` with key context `"Input CanvasText"`, handles the
  Input actions, is the window's input handler
  (`impl EntityInputHandler for EditorView`), and paints the caret, the
  selection, and the marked text's underline (`paint_inline_text`).
  `settle_inline_text` (in render) resolves a waiting entry, hands the focus
  back to the root, and tells the IME the text moved. `refit_inline_text`
  (next to the `text_layout` assignment) takes the frame's layout and fits
  the map to its `text_box`.
- The commit (`inline_text_view.rs`, `element_rows.rs`, `refresh.rs`,
  `edit_history.rs`, `lifecycle.rs`, `main.rs`): `commit_inline_text` writes
  through `commit_element_row(children, value)` (now `pub(crate)`), whose
  `done` calls `inline_commit_written`. `apply_react_reload` releases the
  kept override before it clears or keeps the selection and requests the
  confirming frame; a failed reload drops it. `drop_held_rows`,
  `forget_held_rows`, and `drop_row_commit` drop it with the held commit.
  `record_own_writes` calls `done` before requesting the reload, so
  `react_reloads.next()` inside `done` is the number of the reload that reads
  the write. In `lookup_allowed(transform_drag, scrub_pressed, text_edit)`,
  `text_edit` is now a running edit rather than any override, so frames
  drawing a kept override look the layer up. Quit goes through
  `lifecycle::quit_after_inline_commits`; Close and Reload go through
  `EditorView::after_inline_commit`, which runs the window removal or the
  reload once the commit is written. All three poll every 20 ms and stop
  waiting after 5 s (`commit_wait_continues`, `COMMIT_WAIT_LIMIT`).
  A commit that is not written (its write failed, or it was dropped while
  held) sets `inline_notice` to the reason: `inline_commit_written` on an
  error, `drop_held_rows` when it drops the held commit's override. The
  notice is cleared on a person's selection request (`select_layer`), no
  longer in `change_selection`, so a waiting selection change released by
  the drop keeps it.

## Decisions

These differ from, or settle what the spec left open. Where they differ
from the plan's Rulings, these supersede them:

- **The box comes from the renderer, not from `canvas_geometry`.** 2c's
  handoff expected 2d to map metrics with `layer_map` and `anchor_box`. A
  single-line text's anchor box is trimmed to its ink, so it doesn't give the
  line top, and a 3D text needs its projected quad. The renderer records
  `text_box` instead, and one homography (`TextMap`) serves flat, rotated,
  mirrored (`scaleX={-1}` reverses the corner order; the map still holds),
  and tilted text. The box stays the exact layout rectangle: cosmic-text
  draws each line's glyphs on a baseline truncated to a whole row, so the ink
  sits up to about 2 px off the box (the single-line test tolerates ±2 px);
  flooring the box broke the tilted and multi-line invariants.
- **Entry.** Only a double-click on the selected layer; `is_text_layer`
  decides from the report once the lookup answered, else from the frame's
  `text_box`, so a double-click on a pending `Rect` falls through as an
  ordinary press. Pending (`SourceLookup::Pending`, or Found with targets
  still resolving) waits, and the waiting entry is settled in render, which
  has the window to focus with. Refusal order: reloading, no targets, a
  locked `children` row's reason, no writable row, no `text_box`, a box that
  doesn't fit. The frame shown at double-click measured no layout (the
  worker measures only with an override), so "edge-on" is checked on the box
  alone (`convex`) then.
- **The edit starts from the received `children`**, after
  `submit_edited_rows`, and **beginning pauses playback** (its next frame
  would end the edit; a double-click while playing means "edit this").
- **No entry while a committed text's override is kept**
  (`kept_override != Dropped` → `Wait`, then "The code is reloading" during
  the reload, then the reloaded `children`). So `begin_inline_text` never
  overwrites another commit's override.
- **Presses before the first edited frame**: with no map yet, a press that
  hits the edited layer is inside (no caret change), so a triple-click's
  third press neither commits nor moves the caret; anything else is outside.
- **Focus.** Every end but a blur sets `inline_refocus`; the root takes the
  focus back only while `inline_text_focus` still has it. In gpui-pre a focus
  id no element tracks falls back above the `CelestaEditor` context, which
  would leave every editor shortcut dead; the guard keeps a blur's focus on
  the control the user pressed.
- **Ending table** (`edit_end`): Enter, an outside press, a selection
  change, toggling interactive mode, Seek, Play, and the project commands
  commit with refocus; a blur commits without; Escape and a cleared
  selection cancel, the latter with "The source changed while editing" when
  the file changed outside (`clear_layer_selection(outside_change)`). Seek
  and Play happen where there is no context, so they end the edit on the
  next render, before the rows submit their typed text.
- **Commit.** `inline_commit_value`: unchanged or empty writes nothing (the
  row is required). A still-marked composition is accepted first. The
  commit always requests a (held-scene) frame: with no edit running it
  carries `source_of`, which a commit held behind stale targets needs to
  drain.
- **Lookups during a commit.** `lookup_allowed` is gated on a running edit,
  not on any override, so the commit frame and frames drawing a kept
  override look the layer up. This relaxes the held scene's rule that a held
  frame calls no bridge method: the commit frame redraws the held scene and
  calls `layer_source`, which is safe because the runtime's last evaluation
  is the held scene. Keystroke frames still ask nothing.
- **How long the committed text is drawn**, as the released scrub: kept on
  every request while the commit is held or written (`Writing`); after the
  write, dropped on an error or when the targets are still current (no
  reload comes), else `UntilReload(next)`, released by the first applied
  reload numbered at or after `next`, whatever it does to the selection (an
  outside save's reload may supersede the own one, which then never
  applies). `drop_held_rows`, `forget_held_rows`, and `drop_row_commit`
  drop it only when its commit is still held; a user's selection change
  during the write keeps it, so the old text does not flash during the
  reload (the scrub, by contrast, is dropped with the selection).
- **Close, Quit, and Reload wait for the write**: Close commits, then
  removes the window at once or after polling `inline_commit_writing`; Quit
  ends every window's edit and quits once none is writing. Reload waits the
  same way: reloading at once would clear the selection (the commit's own
  write has not marked the reload as its own yet) and with it forget a
  commit still held, unwritten. The window keeps rendering meanwhile, so a
  held commit can drain. The wait stops after 5 s, so Quit always quits;
  past it, the window closes, the app quits, or the reload runs without the
  write, and a commit still held then is lost as it was before the wait.
  A deferred Reload comes after the own write's reload, so it reloads again
  and clears the selection, as an explicit Reload does.
- **A commit whose write fails, or which is dropped while held, says why on
  the canvas**, in the viewer's notice, as well as in its `text` row: the
  text reverts, and the Properties tab may not be shown. (A held commit
  forgotten with the selection is not covered; see Known gaps.)
- **A failed refit keeps the previous map**: an emptied text without
  `maxWidth` is 0 wide and can't be fitted; the caret stays where the last
  character was. Only entry refuses a degenerate box.
- **Model details**: unmarking accepts the composition (NSTextInputClient's
  `unmarkText`); composing over a selection keeps the selected text until the
  composition is accepted (an emptied composition selects it again);
  `insert`, deletions, moves, `place`, and `select_all` accept a composition
  first; Up from the first line goes to the start and Down from the last to
  the end; vertical moves keep a goal column, cleared by any other change.
- **Painting** happens only while the layout is for the text the edit shows;
  `bounds_for_range` answers from the lagging layout, clamped, so the
  candidate window stays by the text.

## Known gaps

- No undo inside an edit: Cmd+Z and Cmd+Shift+Z do nothing until the commit,
  which is then one history entry.
- No word moves or word deletion (Option/Ctrl+arrows, Option+Backspace),
  and no double-click word selection inside the text.
- `visibleCharacters` is shown in full while typing; the reveal comes back
  with the commit's reload (or on Escape).
- Runs are carried by the session's common prefix and suffix only: edits at
  two places merge the changed span, and a run between them stretches over
  it until the commit's reload. If a component renders text that differs
  from the `children` it received, its runs index the drawn text, not
  `original`, and may land on the wrong characters (still valid, clamped).
- A double-click while another layer's committed text is kept waits with no
  note until the reload.
- The window's own close button bypasses `CloseWindow`, so it does not
  commit (this predates 2d).
- Export, Open, and New commit but do not wait for the write: an export
  right after an edit may read the file before it. A write that had already
  started when Open or New replaces the contents still completes, but its
  history entry and reload belong to the old project and are dropped; a
  commit still held then dies with the old view in `replace_contents`,
  unwritten.
- A canvas commit still held when the selection is cleared without waiting
  (a reload that does not keep it, such as one for an outside change or
  another composition, or the layer disappearing) is forgotten by
  `forget_held_rows` with no viewer notice: the text silently reverts. In
  particular, a held commit's "Not saved: the code failed to reload" shows
  only when a kept reload (an own write's) fails; an outside change's reload
  has already forgotten it.
- Close, Quit, and Reload stop waiting after 5 s: a commit still held then
  (one that neither drains nor is dropped; no known path does that) is lost.
- A click just past the last glyph of a single-line text without `maxWidth`
  is outside its layout box and ends the edit.
- A pending `Text` with no `text_box` (edge-on) acts as a plain press, with
  no note, until its lookup answers.
- While the layout lags, Down past the last line goes to the edit's end but
  End to the lagging layout's line end. A selection over only a hard break or
  an empty line has zero width (no sliver).
- `HeldKey` omits `react_mode`, `prop_overrides`, and the request's scene
  mode; harmless while only keystrokes set `reuse_scene`, cheap to add.
- The worker and refresh glue (hold stays un-overridden, a failed
  evaluation clears the hold, reuse consumed once, override on every
  request) and the commit frame's lookup have no unit test (no GPUI test
  context); the hand checks cover them. A synchronous commit failure sends
  two preview requests.
- Untested geometry corners: an RTL ligature, RTL selection quads, a mark
  over another range; no renderer test for a stroked text's box or a
  windowed multi-line text. `index_at` is O(boundaries × glyphs).
- Code nits: the `children` key is spelled three ways; the `text_box`
  lookup by layer is duplicated; `ime_bounds` folds what `points_box` does;
  `ElementRows::held` became `pub(crate)`.

## What design 3 builds on

- The commit is the `text` row's commit: when design 3 adds an edit scope
  for shared elements, the canvas edit inherits it with no change here.
- `TextOverride` is by layer id, so a live edit of a shared element shows on
  the edited layer only; a scope that changes every instance may want the
  override on each of them.

## Verify by hand

In the editor, with **Select layers** on, on a scene of your own:

- Double-click a selected `<Text>Sale</Text>`: a steady 1px accent caret
  appears at the end; the handles and the cross go and the outline stays.
  Type: the text redraws live in its own font and runs. Double-click an
  unselected text: the first press selects it and the edit begins once its
  source is read. Double-click an unselected `Rect`: the first press selects
  it and the second acts as an ordinary press, with no note.
- Right after the double-click, type `k`, `v`, and Space: they are typed
  into the text (no pause, no mode switch, no play). The arrows, I and O
  don't seek or mark either.
- Keys: arrows, Shift+arrows, Home/End, Shift+Home/End; Cmd+A, Cmd+C,
  Cmd+X, Cmd+V (paste multi-line text: its line breaks become `\n`);
  Shift+Enter breaks the line; Tab and Cmd+Z do nothing; Up and Down keep
  the column across a short line.
- Japanese input through the IME: the composition is drawn underlined and
  the candidate window opens at the caret; drag-select with the IME active
  and the candidate window follows. Escape while composing: the first goes
  to the IME and cancels the composition, the second ends the edit and the
  scene shows the original text. Compose, then click outside: the
  composition shown is written.
- A rotated text, a mirrored one (`scaleX={-1}`), and one tilted in 3D
  under a perspective: the caret and the selection slant with the glyphs and
  sit on the line box (top to bottom of the line), not on the ink; clicks
  land on the glyph clicked.
- A wrapped text with `maxWidth`: End on a wrapped line puts the caret at
  that line's end, not at the next line's start; typing reflows live.
- A single line with anchor (0.5, 0.5): the caret spans the line box.
- A text with color runs and `visibleCharacters`: typing inside and after a
  colored run keeps its color on its own characters, the whole text shows
  while editing, and the reveal comes back after the commit's reload.
- A press inside the text places the caret, a drag selects, Shift-click
  extends. A fast triple-click: the third press neither ends the edit nor
  moves the caret. A press just past the last glyph of a single line without
  `maxWidth` ends the edit; judge whether that feels right.
- Enter (or Cmd+Enter): the new text stays on the canvas with no flash of
  the old one until the reload draws it; Space and the arrows work again
  (the focus is back on the editor), and Cmd+Z restores the old text in one
  step. Escape: nothing is written. Empty the text or leave it unchanged,
  then Enter: nothing is written and no history entry is added.
- Click a Properties field (the Text row, another row), the layer list, or
  the tool row while editing: the edit is written, and the focus stays in the
  control clicked. The Text row then shows the written value.
- Type, then click empty canvas or another layer: the edit is written and
  the canvas keeps the new text while the code reloads.
- A pass-through label (`<Label text="Sale" />`): the live text shows on
  the layer, the commit writes the call site, and one Cmd+Z restores it.
- A frame sized to its text (a label's background): it keeps its old size
  while typing and follows after the commit's reload.
- Double-click a locked text (`<Text>{a + b}</Text>`, `<Span>` children):
  the reason shows beside it in the drag label's style until the next press.
  Double-click during a reload: "The code is reloading".
- Commit, then double-click the same text at once: it waits, shows "The code
  is reloading" during the reload, and a double-click afterwards edits the
  new text.
- Double-click a text while playing: playback pauses and the edit begins.
- Edit the file in an external editor while editing: the edit ends and the
  notice says "The source changed while editing".
- Seek by clicking the timeline, a menu's Next Frame, or play: the edit is
  written, the frame at the new time shows the new text, and the shortcuts
  work afterwards.
- File > Open, Reload, Export while editing: the edit is committed first.
  Edit, then Cmd+Shift+E right away: check whether the export has the new
  text (see Known gaps). Edit, then Cmd+W or Cmd+Q right away: the file has
  the new text after the window closes.
- Commit held, then Cmd+R: type in the Properties Text row without
  pressing Enter, double-click the canvas text (the row's value is written
  first and the edit begins), type, and press Enter once the row's reload
  has started (the commit is held for the targets that reload resolves);
  press Cmd+R at once. The reload runs only after the canvas commit is
  written: the file has the new text, the canvas shows it after the reload,
  and one Cmd+Z restores it.
- A commit that is not written: hide the Properties tab, make the entry
  file read-only, edit the canvas text, and press Enter: the text reverts
  and the viewer's notice gives the write's error until the next click.
- Type in the Properties Text row, double-click the canvas text, type, and
  press Enter quickly: both writes land, the row's first. Then again, but
  wait while still editing until the row's reload finishes before typing
  and pressing Enter: the edit is written (not left "Saving…"), and Cmd+W
  closes promptly.
