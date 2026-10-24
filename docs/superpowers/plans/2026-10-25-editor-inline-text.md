# Inline Text Editing (Design 2d) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Double-clicking a `Text` layer whose children resolve to an editable string edits the text in place on the canvas, in its own font, size, wrapping and transform, with caret, selection, clipboard and IME (Japanese input), and commits it as one history entry.

**Architecture:** The preview worker keeps the last scene the React runtime evaluated, so a keystroke re-renders it with the edited layer's text replaced, without running the code, and returns the layer's `TextMetrics` with the frame. A pure module (`inline_text.rs`) holds the edit (text, selection, marked text) and turns the metrics into caret, selection and hit-test geometry in layout coordinates; the viewer maps them to the screen by the homography onto the text's box as the renderer drew it (flat, rotated, mirrored, or tilted in 3D), paints them, and is GPUI's input handler for the edit. Commit goes through the Properties tab's `text` row path. When an edit ends is decided by pure functions, so it is tested without a GPUI context.

**Tech Stack:** Rust (`celesta-editor` with GPUI via gpui-kit 0.6 / gpui-pre 0.3.6, `celesta-gpu-renderer`, `celesta-renderer`, `celesta-composition`), docs in the website package.

**Spec:** `docs/superpowers/specs/2026-10-23-editor-property-editing-design.md`, section "Inline text editing" and the 2d lines of "Testing". Builds on 2b (`text` row, `commit_element_row`, `HeldCommits`, `SelectionGate`, `submit_requested`, the released scrub's override) and 2c (`HitRegion::anchor_box`, the tool row).

| Part | Plan | Spec sections |
| --- | --- | --- |
| 2a | engine (#516) | |
| 2b | Properties sections (#525, #526) | |
| 2c | canvas tools (#530) | Canvas: Pivot, Resize |
| 2d | this plan | Canvas: Inline text editing |

## Global Constraints

- One commit is one history entry, through the `text` row's path (`commit_element_row` for `PropKey::Prop("children")`): re-resolve, "The source changed while editing", the re-parse check, held while a write or reload runs. Nothing else writes the source.
- Nothing is written while typing, and the React runtime is not asked anything for a keystroke (no evaluation, no layer source lookup): what the code computes from the text keeps its old size until the commit's reload (spec, decided with the user).
- Design 3 (edit scope) is not built: a shared element's text is committed exactly as the Properties tab's `text` row commits it today. Design 3 adds the scope later.
- Indices: the edit holds code-point indices, as `TextMetrics.line_starts` and `GlyphMetrics.start`/`end` do; UTF-16 only at GPUI's input handler, bytes only inside `String` operations.
- The editor tests have no GPUI test context: every lifecycle rule is a pure function over plain values, tested directly; the view only routes events to it.
- UI copy English, inspector style (radius 0 except the 2c tabs, no gradients, no uppercase labels, lucide icons via `CelestaIcon`/`IconName`).
- TDD per task; red/green runs only the new tests; each crate's suite once per task; `cargo clippy -p celesta-editor -p celesta-gpu-renderer --all-targets` clean (the pre-existing `large_enum_variant` on `SourceLookup` excepted); `cargo fmt --check`.
- No `git commit --amend`; commit and test commands are separate calls; stage paths explicitly (never `examples/feature-tour/` or `docs/superpowers/.DS_Store`).

## Rulings (decided here; the spec leaves them open)

- **The text box comes from the renderer.** A single-line text's anchor box is trimmed to its ink, so `anchor_box` does not give the line top. The renderer records each drawn text's layout box (`HitRegion::text_box`: the layout width by `lines · line_height`, from the first line's top, in scene pixels, in the order of `corners`), through whatever the layer is drawn with, so a text in 3D or under a perspective gets its projected quad. The line top and line height are stored on the text texture (in layout units, with its `raster_scale`) so a cached texture, which skips measuring, still has them.
- **Caret geometry is computed in layout coordinates** (origin at the first line's top-left, line `i` from `i · line_height`) and mapped to the scene by the homography that takes the layout rectangle to `text_box` (`TextMap`). It is exact for a flat text (an affine case), a mirrored one (`scaleX={-1}` reverses the corner order; the map still holds) and a tilted one (a plane's perspective is a homography), so 2D and 3D share one path; the inverse maps a click back. Refused on double-click only when the renderer recorded no `text_box` ("The text isn't drawn in this frame", also when a corner is behind the viewer) or the box is degenerate, of zero area or not convex ("The text is edge-on to the view").
- **The edit starts from the received `children`** (`report.props`), which is what the code passed to `<Text>`, after the Properties rows' typed text is submitted (`submit_edited_rows`), so a value typed there first is not overwritten. The live render shows it unchanged until a key is pressed.
- **The live text keeps its runs.** `color_runs` and `font_runs` index the text; the edited layer's runs are carried through the edit by the common prefix and suffix of the original and edited text (runs in the prefix kept, in the suffix shifted by the length change, a run across the changed span stretched or cut to it), so the text keeps its own fonts and colors and never gets `InvalidTextColorRun`. `visible_characters` is cleared while editing (all of the text shows); the commit's reload restores whatever the code computes.
- **Arrow keys move by grapheme cluster** (`unicode-segmentation`); caret placement and `index_at` snap to grapheme boundaries inside a shaping cluster, interpolating between them for a cluster of several graphemes (a ligature), so a combining sequence is never split. An RTL glyph's leading edge is its right edge.
- **Soft wraps have an affinity.** The index at a wrapped line's start draws at the next line's start, except after End or a click past a line's end, which draw it at the end of the line before (`upstream`); any other move clears it. An empty line's caret sits at its alignment's x (left 0, center W/2, right W of the layout width).
- **Keys.** The editing element has key context `"Input CanvasText"`: gpui-base's Input bindings apply and the editor's `!Input` shortcuts don't. Handled as actions: `Backspace`, `Delete`, `Enter`, `MoveLeft`/`Right`/`Up`/`Down`, `MoveHome`/`End`, `SelectLeft`/`Right`/`Up`/`Down` (`gpui_kit::base::actions`), `SelectToStartOfLine`/`SelectToEndOfLine` (Shift+Home/End, Shift+Cmd+Left/Right), `SelectAll`, `Copy`, `Cut`, `Paste`, `Undo`/`Redo` (swallowed: no undo inside an edit), `Escape`. Enter and Cmd+Enter commit, Shift+Enter inserts `\n`, Tab does nothing. Pasted text is plain, `\r\n` and `\r` become `\n`. The editor's bindings outside `!Input` that replace or leave the project (New, Open, Reload, Close, Export) commit the edit first; Toggle loop and Show problems leave it running.
- **Focus.** The press that begins an edit, and every canvas press while editing, calls `window.prevent_default()`, so the root's focus tracking does not take the focus back from `inline_text_focus`.
- **Caret is steady** (no blink), 1 px in the accent color, mapped as a segment so it slants with rotated text; the selection is the accent at 30% alpha; marked (IME) text is underlined 1 px at its line's bottom. While the layout lags the text (it is for an older `display_text`), nothing is painted, and `bounds_for_range` answers from the lagging layout with indices clamped to it, so the candidate window stays by the text.
- **Entry.** A double-click on a `Text` begins an edit when its `text` row is editable. When the layer's source lookup has not answered yet (the double-click's first press selected it), the entry waits for it and begins or refuses when it answers; any other press or selection change drops the waiting entry. Refused while the code reloads ("The code is reloading"). A refusal shows the reason (`reason_text`, `element_edit.unavailable`, or the ones above) in a note beside the layer, styled as the drag label, until the next press or selection change.
- **Ending.** Commits: Enter, a canvas press outside the layout box (then the press acts as usual), blur of the editing element (a click in the Properties tab, the layer list, the tool row), selecting another layer or clearing the selection, toggling Select layers, seeking, starting playback, and the project bindings above. Cancels: Escape; a selection cleared by a reload or by the layer disappearing (`clear_layer_selection` ends any edit), with "The source changed while editing" when the cause is an outside change. An own write's reload keeps the edit. Focus returns to the root only after Enter, Escape or a canvas press (so Cmd+Z then undoes the commit), never after a blur, which would take it from the control the user pressed.
- **Commit.** Unchanged text leaves without writing. An empty text leaves without writing (the `text` row is required: an emptied field shows the source again). Otherwise the row commit runs.
- **The override after a commit** follows the released scrub: it stays on every request while `(layer, children)` is held or being written; when the write completes, it is dropped on an error or when the targets are still current (no reload comes), and otherwise kept until the first reload, at or after the sequence `react_reloads.next()` gave at completion, that keeps the selection; every path that drops or forgets the held commit (`drop_held_rows`, `drop_row_commit`, `forget_held_rows`) drops it too.
- **The Properties `Text` row stays enabled** while editing: pressing it moves the focus, so the canvas edit commits on blur first and the row then shows the written value, as after any row commit.

## Review Focus

- **A single-line text with anchor (0.5, 0.5), a mirrored text, and a text tilted under a perspective**: the caret's top and bottom are the line box's, not the ink's, and the caret follows the glyphs. Test in Task 1 (renderer) and Task 3 (geometry).
- **A `Text` with color runs and `visibleCharacters`**: typing inside and after a colored run keeps its color on its own characters and renders without `InvalidTextColorRun`. Test in Task 2.
- **IME composition then Escape**: the marked text is dropped and the scene shows the original text. Test in Task 3 (edit model) and Task 4 (ending).
- **A keystroke right after a seek or a reload**: the frame is the new time's and the new code's, not the held scene's. Test in Task 2.
- **Pass-through `children`** (`<Label text="Sale" />`): the live override is by layer id, the commit writes the call site, one undo restores it. Test in Task 5.

---

### Task 1: Text metrics and the text box from the renderer

**Files:**
- Modify: `crates/gpu-renderer/src/{lib.rs,renderer.rs,hit.rs,prepare.rs,text.rs,texture.rs}` (and wherever `HitRegion`s are built for text)
- Test: `crates/gpu-renderer/src/tests.rs`

**Interfaces:**
- Produces:

```rust
pub use celesta_renderer::{GlyphMetrics, TextMetrics}; // re-exported from celesta-gpu-renderer
#[derive(Clone, Debug, PartialEq)]
pub struct TextLayout {
    pub text: String,          // the text it was laid out for
    pub metrics: TextMetrics,
    pub max_width: Option<f64>,
    pub align: TextAlign,      // style.align, Left when unset
}
impl GpuRenderer {
    /// Lays `text` out as the last rendered frame's fonts do (call after a render).
    pub fn measure_text(&mut self, text: &str, style: &TextStyle, max_width: Option<f64>) -> TextMetrics;
    /// The layout of text layer `layer` of `scene` (through groups and masks), measured
    /// as `measure_text` does; None when there is no such text layer.
    pub fn text_layout(&mut self, scene: &Scene, layer: &str) -> Option<TextLayout>;
}
// HitRegion gains, for a drawn text layer (None for every other):
/// The text's layout box — layout width by `lines · line_height`, from the first
/// line's top — in scene pixels, in the order of `corners`; projected like them.
pub text_box: Option<[Point; 4]>,
```

- [ ] **Step 1: Write the failing tests** — `measure_text_matches_the_rendered_layout` (width/height equal the text size the frame used); `text_layout_finds_a_text_inside_a_group` (and None for a rect or a missing id); `a_multi_line_text_s_box_is_its_anchor_box` (no stroke, anchor (0,0)); `a_single_line_text_s_box_is_its_line_box` (anchor (0.5, 0.5), "HHH": the box is `line_height` tall and the glyphs' ink rows lie inside it, from `ascent − cap_height` to `ascent` below its top, ±1 px); `a_cached_text_texture_keeps_its_box` (the same text rendered twice: the second frame's box equals the first's, also at a lower raster scale); `a_baseline_anchored_text_s_box_starts_an_ascent_above_its_position`; `a_mirrored_text_s_box_reverses_its_order` (`scaleX` −1); `a_tilted_text_s_box_is_projected` (a text with `rotationX` 40° under a perspective: the box's corners are the layout rectangle's corners projected as its `corners` are — a multi-line, unstroked text's box equals its projected anchor box).
- [ ] **Step 2: Run them, expect FAIL** — `cargo test -p celesta-gpu-renderer text_box measure_text text_layout`
- [ ] **Step 3: Implement** — `measure_text` wraps `text_rasterizer.measure`; the texture stores the line box (top and height, layout units) next to its baseline anchor; the box is carried through the same transform as `anchor_box_corners`. Add `celesta-renderer` to the editor's dependencies only if the re-export is not enough.
- [ ] **Step 4: Run them, expect PASS**; then `cargo test -p celesta-gpu-renderer`.
- [ ] **Step 5: Commit** — `feat(gpu-renderer): measure text and record a text's layout box`

### Task 2: The held scene and the live text override

**Files:**
- Modify: `crates/editor/src/preview.rs`, `crates/editor/src/refresh.rs` (request building, `lookup_allowed` call), `crates/editor/src/layer_select.rs` (`lookup_allowed`), `crates/editor/src/background.rs` (result)
- Test: `crates/editor/src/preview.rs` and `layer_select.rs` `#[cfg(test)]`

**Interfaces:**
- Consumes: Task 1's `GpuRenderer::text_layout`, `TextLayout`.
- Produces:

```rust
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextOverride { pub(crate) layer: String, pub(crate) original: String, pub(crate) text: String }
// PreviewRequest gains:
pub(crate) text_override: Option<TextOverride>,
/// Render the scene the runtime last evaluated instead of evaluating again.
pub(crate) reuse_scene: bool,
// PreviewResult gains: pub(crate) text_layout: Option<TextLayout>, // the overridden layer's
/// What the held scene was evaluated for: the request's time, its `react_reload`, and its React preview.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HeldKey { /* … */ }
pub(crate) enum SceneSource { Held, Evaluate }
/// Held only when asked to reuse and the held scene's key matches the request's.
pub(crate) fn scene_source(request: &PreviewRequest, held: Option<&HeldKey>) -> SceneSource;
/// Replaces layer `layer`'s text (through groups and masks): its runs carried
/// through the edit (`shift_runs`), `visible_characters` cleared. False when there is no such text layer.
pub(crate) fn replace_text(scene: &mut Scene, edit: &TextOverride) -> bool;
/// Runs over `original` moved onto `edited` by their common prefix and suffix.
pub(crate) fn shift_runs<R: Clone>(runs: &[R], span: impl Fn(&R) -> Range<usize>, with: impl Fn(&R, Range<usize>) -> R, original: &str, edited: &str) -> Vec<R>;
// layer_select.rs: lookup_allowed gains `text_edit: bool` (false while an edit or its post-commit override is active)
```

- [ ] **Step 1: Write the failing tests** — `replace_text_reaches_a_layer_inside_a_group_and_a_mask`; `replace_text_clears_visible_characters`; `shift_runs_keeps_the_prefix_and_shifts_the_suffix` (insert inside, before and after a colored run; delete across a run's end; a run emptied by the edit is dropped); `replace_text_refuses_a_missing_or_non_text_layer`; `a_reused_request_without_a_held_scene_evaluates`; `a_reused_request_for_another_frame_or_reload_evaluates` (time, `react_reload`, React preview each differing); `lookup_is_not_allowed_while_editing_text`.
- [ ] **Step 2: Run them, expect FAIL** — `cargo test -p celesta-editor replace_text shift_runs scene_source lookup_is_not_allowed`
- [ ] **Step 3: Implement** — the worker keeps the last evaluated `Scene` with its `HeldKey`; `scene_source` picks it or evaluates (and holds the new one); the override is applied to a clone, which is rendered, then the overridden layer's `text_layout` returns with the frame. `refresh_preview` copies the view's active override into every request (so `take_latest` can't drop it), sets `reuse_scene` only for a keystroke's request (Task 4 calls it), and sends no `source_of` while `lookup_allowed` is false.
- [ ] **Step 4: Run them, expect PASS**; then `cargo test -p celesta-editor`.
- [ ] **Step 5: Commit** — `feat(editor): re-render the held scene with a layer's text replaced`

### Task 3: The edit model and its geometry

Runs in parallel with Task 2, in a subagent worktree (a new module; it reads only Task 1's `TextLayout`).

**Files:**
- Create: `crates/editor/src/inline_text.rs` (with `#[cfg(test)] mod tests`)
- Modify: `crates/editor/src/main.rs` (`mod inline_text;`)

**Interfaces:**
- Consumes: `TextLayout` (Task 1; tests build layouts and boxes by hand), `layer_select::letterbox`.
- Produces:

```rust
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextEdit { /* text, anchor, head, upstream, marked: Option<(Range<usize>, String)> */ }
impl TextEdit {
    pub(crate) fn new(text: String) -> Self;            // caret at the end
    pub(crate) fn text(&self) -> &str;                  // committed text, without marked text
    pub(crate) fn display_text(&self) -> String;        // with marked text, what is rendered
    pub(crate) fn selection(&self) -> Range<usize>;     // code points, ordered
    pub(crate) fn insert(&mut self, text: &str);        // replaces the selection; normalizes \r\n, \r
    pub(crate) fn backspace(&mut self); pub(crate) fn delete(&mut self);
    pub(crate) fn move_horizontal(&mut self, forward: bool, extend: bool);
    pub(crate) fn move_vertical(&mut self, layout: &TextLayout, down: bool, extend: bool);
    pub(crate) fn move_line_edge(&mut self, layout: &TextLayout, end: bool, extend: bool);
    pub(crate) fn select_all(&mut self); pub(crate) fn selected_text(&self) -> String;
    pub(crate) fn place(&mut self, index: usize, upstream: bool, extend: bool);
    // GPUI input handler support (UTF-16 ranges over display_text):
    pub(crate) fn replace_utf16(&mut self, range: Option<Range<usize>>, text: &str);
    pub(crate) fn mark_utf16(&mut self, range: Option<Range<usize>>, text: &str, selected: Option<Range<usize>>);
    pub(crate) fn unmark(&mut self);
    pub(crate) fn marked_utf16(&self) -> Option<Range<usize>>; pub(crate) fn selection_utf16(&self) -> Range<usize>;
}
// Layout coordinates: origin at the first line's top-left, line i from i · line_height.
// Indices past the layout's text are clamped to it.
/// The caret at `index` as a segment (line top, line bottom).
pub(crate) fn caret(layout: &TextLayout, index: usize, upstream: bool) -> [Point; 2];
/// One rectangle per line the range covers.
pub(crate) fn selection_quads(layout: &TextLayout, range: Range<usize>) -> Vec<[Point; 4]>;
/// The grapheme boundary nearest to a point: (index, upstream).
pub(crate) fn index_at(layout: &TextLayout, point: Point) -> (usize, bool);
/// The homography taking the layout rectangle (layout width by lines · line_height) to `text_box`.
pub(crate) struct TextMap { /* 3×3, and its inverse */ }
impl TextMap {
    /// Err("The text is edge-on to the view") for a box of zero area or not convex.
    pub(crate) fn fit(layout: &TextLayout, text_box: [Point; 4]) -> Result<Self, String>;
    pub(crate) fn to_scene(&self, layout_point: Point) -> Point;
    pub(crate) fn to_layout(&self, scene: Point) -> Option<Point>;
}
```

- [ ] **Step 1: Write the failing tests** — edit model: `insert_replaces_the_selection`, `backspace_removes_one_grapheme` ("e\u{301}" goes at once), `shift_moves_extend_the_selection`, `paste_normalizes_line_breaks`, `marked_text_shows_in_display_text_and_unmark_keeps_text_unchanged`, `utf16_ranges_round_trip_through_astral_and_japanese_text` ("日本🎌語"), `escape_after_marking_restores` (a fresh `TextEdit::new(original)` equals the start). Geometry, with hand-built layouts: `caret_on_a_wrapped_line_start_and_upstream_end`, `caret_in_a_multi_line_text_steps_by_line_height`, `caret_before_an_rtl_cluster_is_at_its_right_edge`, `caret_inside_a_ligature_interpolates`, `caret_and_index_at_snap_to_graphemes_in_a_combining_cluster` ("e\u{301}x": no caret between `e` and the accent), `an_empty_line_s_caret_follows_alignment`, `selection_over_two_lines_gives_two_quads`, `index_at_round_trips_caret` (every boundary of a wrapped two-line text), `indices_past_a_lagging_layout_clamp`, `vertical_moves_keep_x`, `home_and_end_use_visual_lines`, `a_map_fitted_to_a_rotated_scaled_box_is_its_affine_map` (rotation 30°, scale 2: the caret's scene points against hand-computed ones), `a_map_fitted_to_a_mirrored_box_maps` (reversed corner order), `a_map_fitted_to_a_projected_box_follows_the_perspective` (a trapezoid: the corners map exactly and the rectangle's center maps to the diagonals' crossing), `to_layout_inverts_to_scene`, `fit_refuses_a_degenerate_box` (zero area, and a bow-tie).
- [ ] **Step 2: Run them, expect FAIL** — `cargo test -p celesta-editor inline_text`
- [ ] **Step 3: Implement.** Line `i` spans `line_starts[i]..line_starts[i + 1]` (the last to the text's end); its top is `i · line_height`. `TextMap::fit` solves the 4-point homography (the unit square to each quad, composed). A caret x on line `i` is the left edge of the cluster whose `start` is the index (its right edge for `rtl`), else the right edge of the cluster that ends there, else, inside a cluster, `x + width · g / n` for the index's grapheme `g` of the cluster's `n`.
- [ ] **Step 4: Run them, expect PASS**; then `cargo test -p celesta-editor`.
- [ ] **Step 5: Commit** — `feat(editor): model an inline text edit and its caret geometry`

### Task 4: Editing on the canvas, Escape, and what ends an edit

**Files:**
- Modify: `crates/editor/src/{view.rs,viewer.rs,transform_drag.rs,background.rs,main.rs}`; create `crates/editor/src/inline_text_view.rs` for the view glue if `viewer.rs` would grow past ~1800 lines
- Test: the modules' `#[cfg(test)]` blocks

**Interfaces:**
- Consumes: Tasks 2 and 3.
- Produces:

```rust
pub(crate) struct InlineTextSession { pub(crate) layer: String, pub(crate) edit: TextEdit, pub(crate) original: String, pub(crate) layout: Option<TextLayout>, pub(crate) map: Option<TextMap> }
// EditorView gains: inline_text: Option<InlineTextSession>, inline_text_focus: FocusHandle,
// inline_pending: Option<String /* layer */>, inline_note: Option<(String /* layer */, String /* reason */)>
pub(crate) enum InlineEntry { Begin(String), Wait, Refuse(String) }
/// A double-click on a `Text`: `text_box` is its `HitRegion::text_box` in the frame shown.
pub(crate) fn inline_entry(targets: Option<(&ElementTargets, &LayerSourceReport)>, lookup_pending: bool, unavailable: Option<&str>, reloading: bool, text_box: Option<[Point; 4]>, layout: Option<&TextLayout>) -> InlineEntry;
pub(crate) enum EditEvent { Enter, Escape, Blur, CanvasPress { inside: bool }, SelectionChange, SelectionCleared { outside_change: bool }, ToggleInteractive, Seek, Play, ProjectCommand }
pub(crate) enum EditEnd { Keep, Commit { refocus_root: bool }, Cancel { reason: Option<&'static str>, refocus_root: bool } }
pub(crate) fn edit_end(event: EditEvent) -> EditEnd;
/// The handles and cross shown: none while editing (the outline stays).
pub(crate) fn editing_drag_state(state: DragState, editing: bool) -> DragState;
impl EditorView {
    pub(crate) fn begin_inline_text(&mut self, layer: String, window: &mut Window, cx: &mut Context<Self>);
    pub(crate) fn inline_text_changed(&mut self, cx: &mut Context<Self>); // live request, reuse_scene
    /// Ends the edit without writing; `reason` goes to the viewer's notice.
    pub(crate) fn cancel_inline_text(&mut self, reason: Option<&str>, cx: &mut Context<Self>);
}
impl EntityInputHandler for EditorView { /* over the session's TextEdit */ }
```

- [ ] **Step 1: Write the failing tests** — `inline_entry_takes_the_received_children` (literal, const, pass-through); `inline_entry_waits_for_a_pending_lookup`; `inline_entry_refuses_a_locked_row_with_its_reason` (`<Span>` children, an expression); `inline_entry_refuses_while_the_code_reloads`; `inline_entry_refuses_a_text_that_isn_t_drawn` and `…_an_edge_on_text`; `edit_end_decides_every_event` (one assertion per `EditEvent`: Enter and a canvas press commit and refocus the root, Blur commits without refocusing, Escape cancels and refocuses, a selection cleared by an outside change cancels with "The source changed while editing", one cleared otherwise cancels without a reason, the others commit); `editing_drag_state_hides_handles_and_cross`; `a_press_in_the_layout_box_places_the_caret` (pure: screen point → `index_at` via `letterbox` and `TextMap::to_layout`, on a tilted box too).
- [ ] **Step 2: Run them, expect FAIL** — `cargo test -p celesta-editor inline_entry edit_end editing_drag_state a_press_in_the_layout_box`
- [ ] **Step 3: Implement.**
  - `select_layer_at`: `click_count >= 2` on a `Text` → `inline_entry`; `Begin` submits the Properties rows' typed text, then begins (focuses `inline_text_focus`); `Wait` sets `inline_pending`, resolved where `source_lookup` is set (`background.rs`); `Refuse` sets `inline_note`. Checked before `begin_transform_drag`. The press that begins, and every canvas press while editing, calls `window.prevent_default()`. The note and a pending entry clear on any other press and on selection change.
  - While editing, the canvas area tracks `inline_text_focus` with key context `"Input CanvasText"` and handles the actions listed in the Keys ruling; each change calls `inline_text_changed`. Escape calls `cancel_inline_text(None)`.
  - `clear_layer_selection` routes `SelectionCleared` to `edit_end` (every path that clears it: reloads, `LookupFate::Gone`, session switches); the outside-change branch of `reload_composition` marks it as such.
  - The canvas paint closure calls `window.handle_input(&inline_text_focus, ElementInputHandler::new(bounds, entity), cx)` and paints caret, selection and marked underline when `layout.text == edit.display_text()`; `bounds_for_range` answers from the session's layout either way (clamped), mapped through `map` to window coordinates; `window.invalidate_character_coordinates()` when the layout changes. `map` is refitted to each frame's `text_box`; a frame without one leaves the previous map.
  - A press inside the layout box places the caret (Shift extends) and the root's `drag_move` extends the selection; `drag_state()` goes through `editing_drag_state`; the outline stays.
- [ ] **Step 4: Run them, expect PASS**; then `cargo test -p celesta-editor`.
- [ ] **Step 5: Commit** — `feat(editor): edit a text layer in place on the canvas`

### Task 5: Committing an edit

**Files:**
- Modify: `crates/editor/src/{viewer.rs or inline_text_view.rs,element_rows.rs,playback.rs,refresh.rs,render.rs,main.rs}`
- Test: `crates/editor/src/tests/inline_text.rs` (new, declared in `tests.rs`), using the existing `single_row`/`other_prop_report`/`resolve_in`/`commit_files` helpers

**Interfaces:**
- Consumes: Task 4; `commit_element_row` (made `pub(crate)`), `settle_row_requests`, `SelectionGate`, the released scrub's override handling (`element_rows.rs`).
- Produces:

```rust
impl EditorView {
    /// Ends the edit, writing its text through the `text` row unless unchanged or empty.
    pub(crate) fn commit_inline_text(&mut self, refocus_root: bool, window: Option<&mut Window>, cx: &mut Context<Self>);
}
/// What a committed edit writes: None when unchanged or empty.
pub(crate) fn inline_commit_value(original: &str, edited: &str) -> Option<EditValue>;
/// Where the post-commit override stands.
pub(crate) enum KeptOverride { Writing, UntilReload(u64), Dropped }
pub(crate) fn kept_override_after_write(result: &Result<(), String>, targets_current: bool, next_reload: u64) -> KeptOverride;
pub(crate) fn kept_override_on_reload(kept: &KeptOverride, sequence: u64, keeps_selection: bool) -> bool; // still kept
```

- [ ] **Step 1: Write the failing tests** — `inline_commit_value_skips_unchanged_and_empty`; `an_edit_writes_one_jsx_text_literal` (and a `\n` writes `{"a\nb"}`, through `commit_files`); `a_pass_through_edit_writes_the_call_site` and `…_undoes_in_one_step` (`EditHistory`); `a_const_edit_writes_the_const`; `the_override_is_dropped_on_an_error_or_without_a_reload`; `the_override_waits_for_the_first_kept_reload_at_or_after_its_sequence` (an older reload, a reload that clears the selection, then the one that clears it); `dropping_a_held_commit_drops_its_override`; `seeking_or_playing_commits_first` (the `submit_requested` path settles the edit as a commit, through `edit_end`); `selecting_another_layer_commits_before_the_gate` (the gate sees the held commit).
- [ ] **Step 2: Run them, expect FAIL** — `cargo test -p celesta-editor inline_text`
- [ ] **Step 3: Implement.** Every `Commit` from `edit_end` calls `commit_inline_text`: Enter, a canvas press outside the layout box (then handled as usual), blur (`window.on_blur` on `inline_text_focus`), `select_layer`/`release_selection`/`toggle_interactive` (before their gate), `settle_row_requests` (seek, playback), and the project commands (New, Open, Reload, Close, Export) before they run. The override's lifetime follows `KeptOverride`, mirroring the released scrub, and `drop_held_rows`/`drop_row_commit`/`forget_held_rows` drop it.
- [ ] **Step 4: Run them, expect PASS**; then `cargo test -p celesta-editor`, clippy, fmt.
- [ ] **Step 5: Commit** — `feat(editor): commit an inline text edit`

### Task 6: Docs and handoff

**Files:**
- Modify: `packages/website/content/{en,ja}/docs/01-(getting-started)/03-preview.md` (an "Edit text on the canvas" section: double-click, Enter / Shift+Enter / Escape, what commits, IME, what is not editable and why, frames sized to the text following on commit)
- Modify: `docs/superpowers/specs/2026-10-23-editor-property-editing-design.md` ("Inline text editing": the rulings above that settle what the spec left open)
- Create: `docs/handoff/2026-10-25-inline-text.md` (decisions, known gaps: no undo inside an edit, no word moves, `visibleCharacters` shown in full while typing, runs carried by prefix and suffix only; "Verify by hand": the spec's manual list including Japanese input through the IME and its candidate window, a rotated, a mirrored and a 3D-tilted text, a wrapped `maxWidth` text, a pass-through label, Escape after composing, typing letters bound to editor shortcuts (k, v, space) right after the double-click, a double-click on an unselected text, clicking a Properties field while editing)

- [ ] **Step 1: Write the docs**; `pnpm --filter @celesta/website check`.
- [ ] **Step 2: Commit** — `docs(editor): describe inline text editing`

## Order

Task 1 → (Task 2 ∥ Task 3, Task 3 in a subagent worktree) → Task 4 → Task 5 → Task 6.
