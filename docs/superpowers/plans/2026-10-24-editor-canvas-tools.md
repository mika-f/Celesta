# Canvas Tools (Design 2c) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** On the canvas, rotation and scale turn about the center of the layer's box by default (a Center / Pivot toggle), the pivot is drawn as a cross that Alt-drags the anchor without moving the layer, and a Rect tool resizes a layer by its `size` props edge by edge, keeping the opposite edge in place.

**Architecture:** One pure geometry module (`crates/editor/src/canvas_geometry.rs`) maps the layer's own pixels to its parent with the composition crate's own `EvaluatedTransform::linear` / `translation` (what the renderer draws with), measures the layer's box from its outline, and solves every compensation with one rule: keep a chosen point fixed by moving `position`. The drag state is generalized from `TransformProp` to `PropKey` (sizes and anchors are not transform props), the handles come from the active tool, and the live preview and commit paths of 2a/2b are reused unchanged.

**Tech Stack:** Rust (`celesta-editor` with GPUI via gpui-kit 0.6, `celesta-composition`, `celesta-gpu-renderer`), docs in the website package.

**Spec:** `docs/superpowers/specs/2026-10-23-editor-property-editing-design.md`, section "Canvas" ("Pivot", "Resize") and the 2c lines of "Testing". Builds on 2a (`2026-10-23-editor-property-editing-engine.md`) and 2b (`2026-10-24-editor-property-sections.md`, PR #525): `PropKey`, `ElementTargets` with `table: Option<PrimitivePropertyTable>` (`size`, field bounds), `override_sites`, `commit_element`.

| Part | Plan | Spec sections |
| --- | --- | --- |
| 2a | engine (#516) | |
| 2b | Properties sections (#525) | |
| 2c | this plan | Canvas: Pivot, Resize |
| 2d | later | Inline text editing |

## Global Constraints

- One drag is one history entry, whatever number of props and files it writes (a Center rotation writes `rotation` in the component's file and `x`/`y` at the call site).
- A drag's commit goes through `commit_element` (re-resolve, "The source changed while editing", re-parse check). Nothing else writes the source.
- The live preview overrides, from the pointer's first move, every prop the handle can change for the whole drag (2a's rule: positions stay fixed, so the runtime remounts only at start and cancel). A compensating handle's set includes `x`/`y` from the first frame.
- Values are written with `format_number`; a prop whose formatted value is unchanged is not written (as `prop_values` does).
- Sizes are clamped to the table field's `min`/`max` (and whole numbers where `integer`); no drag flips or collapses a box (an analog of `MIN_SCALE_RATIO`).
- UI copy English, inspector style (radius 0, no gradients, no uppercase labels, lucide icons via `CelestaIcon`/`IconName`).
- Keep 2c out of `element_edit.rs` and `tests.rs` except where an interface forces a line: a parallel session (2b polish) is splitting those files. New tests live in the new modules' `#[cfg(test)]` blocks.
- TDD per task; red/green runs only the new tests; the crate's suite once per task; `cargo clippy -p celesta-editor --all-targets` clean (the pre-existing `large_enum_variant` on `SourceLookup` excepted).
- No `git commit --amend`; commit and test commands are separate calls; stage paths explicitly (never `examples/feature-tour/` or `docs/superpowers/.DS_Store`).

## Rulings (decided here; the spec leaves them open)

- **The pivot is the renderer's pivot**: the anchor point plus `origin` (`originX`/`originY`), as `EvaluatedTransform::translation` places it. "Pivot" mode turns about it; the cross is drawn there. 2a's `turn()` measures angles about `transform.position`, which is the pivot only when `origin` is zero — it must use the pivot.
- **One compensation rule.** A compensating drag names a fixed point — the box center (Center mode rotation/scale), the opposite edge's or corner's point (left/top resize, Shift-aspect corners), or the pivot's screen position (Alt anchor drag) — in the layer's own pixels after the change (`fixed_local'`) and in parent space before it (`fixed_parent`), builds the layer→parent map `M'` from the new props with the old position, and writes `position' = position + (fixed_parent − M'(fixed_local'))`. The spec's `c + R(Δθ)·(p − c)` and `c + (p − c)·s` are special cases (zero origin, no skew).
- **Own-pixel box** comes from the outline through the inverse maps (parent⁻¹ then layer⁻¹), so a `Text`'s box is its laid-out bounds, not its `maxWidth`. Layer-local coordinates are relative to the anchor point, as `EvaluatedTransform::homography` documents.
- **Drag props are `PropKey`s.** `TransformDrag.values`/`editable`, `drag_values`, `preview_values`, `handle_props`, `prop_changes`/`prop_values`, and `DragState::Editable` carry `PropKey` (`x`, `y`, `rotation`, `scale`, `scaleX`, `scaleY`, `anchorX`, `anchorY`, and the table's `size` props). `TransformProp` stays for the six transform props and is not grown. `prop_changes`' rule extends: position, anchors and sizes move by the same delta as the evaluated value; scales by the same ratio.
- **Compensable `x`/`y`** = both targets are `Literal` or `Missing` (directly or through a pass-through), not `Keyframe`, not locked. Center mode, left/top resize and the anchor drag need it.
- **Fallbacks.** Center mode without compensable `x`/`y`: the drag turns/scales about the pivot and the toggle reads "Pivot (x, y not editable)" (disabled-looking label, still clickable). Rect tool: needs every `size` prop of the table editable (`Literal`, `Keyframe` or `Missing`); without a `size` or with a locked size prop the Scale handles show. Left/top edges (and corners touching them) additionally need compensable `x`/`y`, else those handles are hidden. `radius` primitives: corner handles only, uniform. `Text`: left and right edge handles only, writing `maxWidth` (`Missing` inserts it). Shift on a corner keeps the box's aspect.
- **Anchor drag**: Alt + drag on the cross (or on the pivot when the cross sits on a handle) writes `anchorX`/`anchorY` as fractions of the box (unclamped: an anchor outside the box is valid) and compensates `x`/`y` so nothing moves on screen. Needs both anchor targets editable and compensable `x`/`y`; a kind-locked `anchorY="baseline"` on `Text` makes the cross inert (its tooltip gives the lock reason).
- **Toolbar**: two compact segmented toggles beside "Select layers", shown only in interactive mode: "Center" | "Pivot" and "Rect" | "Scale". Session state on `EditorView` (default Center, Rect), not saved. No keyboard shortcuts in 2c.
- **Snapping**: rotation keeps 2a's Shift 15° snap; resize has none beyond Shift-aspect.

## Review Focus

- **A layer with `origin` set** (`originX={40}`): Pivot-mode rotation turns about anchor+origin; Center-mode rotation keeps the box center fixed. Test in Task 1 (geometry) and Task 3.
- **A skewed layer** (`skewX={20}`) resized from its left edge: the right edge stays put on screen. Test in Task 5.
- **A two-file Center rotation** (rotation written on the element, `x`/`y` passed at the call site): one commit, one undo restores both files. Test in Task 3.
- **A `Text` whose laid-out box is narrower than its `maxWidth`**: dragging the right edge starts from the visible box width, not from `maxWidth`. Test in Task 5.
- **A drag through the opposite edge** (resize past zero): the size clamps at its minimum, never negative, never flips. Test in Task 5.

---

### Task 1: Canvas geometry

**Files:**
- Create: `crates/editor/src/canvas_geometry.rs` (with `#[cfg(test)] mod tests`)
- Modify: `crates/editor/src/main.rs` (`mod canvas_geometry;`)

**Interfaces:**
- Produces:

```rust
/// The layer's box in its own pixels, relative to its anchor point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LocalBox { pub(crate) min: Point, pub(crate) max: Point }
impl LocalBox {
    pub(crate) fn size(&self) -> Point;
    pub(crate) fn center(&self) -> Point;
    /// The point at fractions (`fx`, `fy`) of the box (0,0 top-left, 1,1 bottom-right).
    pub(crate) fn at(&self, fx: f64, fy: f64) -> Point;
}
/// The layer's own-pixel → parent map for `transform`: `[a, b, c, d, tx, ty]`
/// built from `EvaluatedTransform::linear` and `translation` (2D part).
pub(crate) fn layer_map(transform: &EvaluatedTransform) -> [f64; 6];
/// The pivot in parent space: where `translation` puts the turning point.
pub(crate) fn pivot(transform: &EvaluatedTransform) -> Point;
/// The box `frame.outline` encloses, in the layer's own pixels; `None` when a map collapses.
pub(crate) fn local_box(frame: &LayerFrame) -> Option<LocalBox>;
/// `position` that keeps `fixed_parent` where it is once the layer's props
/// become `after` (with `after.position` still the old position) and the fixed
/// point sits at `fixed_local` in the new own pixels.
pub(crate) fn keep_fixed(after: &EvaluatedTransform, fixed_local: Point, fixed_parent: Point) -> Point;
```

- [ ] **Step 1: Write the failing tests** (hand-computed expectations):
  - `layer_map_matches_the_renderer_layout`: a transform with position (100, 50), rotation 90, scale (2, 1), origin (0, 0) maps own (10, 0) to (100, 70); with origin (5, 0) the pivot is (105, 50) and own (5, 0) maps to (105, 50).
  - `local_box_reads_the_outline`: a frame whose outline is the scene quad of a 200×100 box at anchor (0.5, 0.5), rotated 30°, inside a parent scaled 2 → `size()` (200, 100), `center()` (0, 0); anchor (0, 0) → `min` (0, 0).
  - `keeping_the_center_through_a_rotation`: center c fixed while rotation goes 0 → 90 for a box at anchor (0, 0), size 100×50, position (10, 10): the new position puts `M'(center_local)` at the old center within 1e-9; same under a parent rotated 45° and scaled 2 (fixed points are in parent space, so the parent cancels).
  - `keeping_a_point_with_skew_and_origin`: skewX 20, originX 40: after a scale change, `M'(fixed_local)` equals `fixed_parent`.
- [ ] **Step 2: Run** `cargo test -p celesta-editor canvas_geometry` — FAIL (no module).
- [ ] **Step 3: Implement** with `EvaluatedTransform::linear()` (its 2D part) and `translation()`; invert with `celesta_gpu_renderer::invert_affine`. Do not re-derive the layout: map with the same functions the renderer uses.
- [ ] **Step 4: Run** the tests, then `cargo test -p celesta-editor` — PASS.
- [ ] **Step 5: Commit** — `feat(editor): measure a layer's box and keep a point fixed through a transform change`

---

### Task 2: Drag props keyed by `PropKey`

No behavior change: the canvas drags exactly as in 2a/2b; only the key type and the received-offset rule generalize so later tasks can drag sizes and anchors.

**Files:**
- Modify: `crates/editor/src/transform_handles.rs`, `crates/editor/src/transform_drag.rs`, `crates/editor/src/viewer.rs` (only what the types force)
- Test: `transform_handles.rs` / `transform_drag.rs` `#[cfg(test)]` blocks (move nothing out of `tests.rs`; adapt its existing drag tests only where types force it)

**Interfaces:**
- Changes:

```rust
pub(crate) fn drag_values(frame: &LayerFrame, handle: Handle, pointer: (Point, Point), shift: bool,
    previous_turn: f64, editable: &[PropKey]) -> Vec<(PropKey, f64)>;
pub(crate) fn prop_values(frame: &LayerFrame, values: &[(PropKey, f64)], received: &ReceivedProps) -> Vec<(PropKey, f64)>;
pub(crate) fn preview_values(frame: &LayerFrame, handle: Handle, editable: &[PropKey],
    values: &[(PropKey, f64)], received: &ReceivedProps) -> Vec<(PropKey, f64)>;
// TransformDrag { editable: Vec<PropKey>, values: Vec<(PropKey, f64)>, … }
// DragState::Editable(LayerFrame, Vec<PropKey>)
```
- `prop_changes`' start value for a key: transform fields for the six transform props (as today), `transform.anchor` for `anchorX`/`anchorY`, the `local_box` size for size props (Task 1). The written value moves by the same delta (position, anchors, sizes, rotation) or ratio (scales) from the received value.

- [ ] **Step 1:** Convert the existing drag tests to `PropKey` (`TransformProp::X.key()`), keeping every expectation. Add `received_offset_applies_to_anchors_and_sizes` (received `anchorX` 0.25 while the evaluated anchor is 0.5 and the drag moves it by +0.1 → writes 0.35; same for a size).
- [ ] **Step 2: Run** — FAIL to compile.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p celesta-editor` — every existing drag test passes unchanged in meaning.
- [ ] **Step 5: Commit** — `refactor(editor): key canvas drags by prop name`

---

### Task 3: Center / Pivot

**Files:**
- Modify: `crates/editor/src/transform_handles.rs` (rotation and scale with a pivot choice; `turn()` about the pivot), `crates/editor/src/transform_drag.rs` (mode and compensable check; the handle's prop set includes `x`/`y` in Center mode), `crates/editor/src/viewer.rs` (toolbar toggle), `crates/editor/src/view.rs` (state)
- Test: `transform_handles.rs` `#[cfg(test)]`; one commit/undo test where the existing commit tests live (`tests.rs`) only if no module-local harness can build `commit_files` inputs — prefer the module.

**Interfaces:**
- Produces:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum PivotMode { #[default] Center, Pivot }
/// Whether `x` and `y` can absorb a compensation: `Literal` or `Missing`
/// (directly or through a pass-through), never `Keyframe` or locked.
pub(crate) fn compensable(targets: &ElementTargets) -> bool;
// drag_values gains `pivot: PivotMode` (the mode actually used: Center only when compensable).
```
- Center rotation/scale: fixed point = box center (Task 1 `keep_fixed`); `x`/`y` join the values and the preview set from the first move. Pivot mode: about `pivot(transform)` (anchor + origin).
- Toggle: "Center" | "Pivot". When Center is chosen but the selected layer is not compensable, the toggle shows the mode actually used, "Pivot (x, y not editable)" (spec wording), while the stored choice stays Center for the next layer.

- [ ] **Step 1: Write the failing tests:** `center_rotation_keeps_the_box_center` (anchor (0,0) 100×50 Rect, parent rotated 45° and scaled 2; rotate 30° → center unchanged in scene within 1e-6; values contain rotation, x, y); `center_scale_keeps_the_box_center` (uniform and per-axis); `pivot_mode_turns_about_anchor_and_origin` (originX 40: the pivot does not move); `a_keyframed_x_falls_back_to_the_pivot` (x `Keyframe` → no x/y in the values; the mode used is Pivot); `center_mode_preview_overrides_x_and_y_from_the_first_move`; `a_two_file_center_rotation_is_one_undo` (rotation on the element file, `x`/`y` at a call site in another file).
- [ ] **Step 2: Run** — FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the crate suite — PASS; clippy clean.
- [ ] **Step 5: Commit** — `feat(editor): rotate and scale about the box center, with a Center/Pivot toggle`

---

### Task 4: The pivot cross and the anchor drag

**Files:**
- Modify: `crates/editor/src/transform_handles.rs` (`Handle::Anchor`), `crates/editor/src/transform_drag.rs` (Alt-press on the cross starts an anchor drag), `crates/editor/src/viewer.rs` (draw the cross; cursor)
- Test: module `#[cfg(test)]` blocks

**Interfaces:**
- `Handle::Anchor`: hit only with Alt held (within the handle reach of the cross); without Alt a press there is a Move. The cross is drawn for every selected layer with a frame, at the pivot (Task 1), small (handle size), in the selection color.
- Values: `anchorX`, `anchorY` (fractions of `local_box`), `x`, `y` (compensation keeping the box where it is: fixed point = any box corner, e.g. its top-left, before and after).

- [ ] **Step 1: Write the failing tests:** `an_anchor_drag_keeps_the_layer_on_screen` (rotated 30°, scaled 1.5, under a scaled parent: after moving the anchor to (0.5, 0.5) every outline corner maps to the same scene point within 1e-6); `an_anchor_drag_needs_compensable_x_y` (x keyframed → no Anchor handle); `a_baseline_text_anchor_is_inert` (`anchorY` kind-locked → no Anchor handle); `anchor_values_are_fractions_of_the_box`.
- [ ] **Step 2: Run** — FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the crate suite — PASS; clippy clean.
- [ ] **Step 5: Commit** — `feat(editor): draw the pivot and move the anchor with Alt`

---

### Task 5: The Rect tool

**Files:**
- Modify: `crates/editor/src/transform_handles.rs` (resize handles and values), `crates/editor/src/transform_drag.rs` (tool, size targets from `targets.table.size`, fallbacks), `crates/editor/src/viewer.rs` (toolbar toggle), `crates/editor/src/view.rs` (state)
- Test: module `#[cfg(test)]` blocks

**Interfaces:**
- Produces:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum ResizeTool { #[default] Rect, Scale }
/// The props that hold the box for the selected element (from `table.size`)
/// and whether the Rect tool can use them; `None` falls back to Scale.
pub(crate) enum SizeProps { Box { width: String, height: String }, Radius(String), Width(String) }
pub(crate) fn size_props(targets: &ElementTargets) -> Option<SizeProps>;
```
- Box: corner handles change both, edges one; the dragged edge follows the pointer in the layer's axes (`local_box` after the move), the opposite edge/corner is the fixed point; Shift on a corner keeps `size().x / size().y`. Radius: corners only, uniform, the opposite corner fixed. Width (`Text` `maxWidth`): left/right edges only; the opposite edge's point at the pivot's height is fixed; the new `maxWidth` = the new box width.
- Left/top edges and the corners touching them write `x`/`y` (compensation) and need compensable `x`/`y`; without it those handles are hidden. Right/bottom edges never touch `x`/`y` unless the anchor is not at the left/top (then the fixed edge moves the anchor point: compensate when compensable, else hide the handle).
- Clamp to the field's bounds (Global Constraints); minimum box 1 own pixel when the field has no `min`.
- `drag_values` gains `tool: ResizeTool` beside Task 3's `pivot: PivotMode`. The rotation stem handle stays in both tools; in the Rect tool the corner/edge handles resize, in the Scale tool they scale (2a), both honoring the pivot mode.

- [ ] **Step 1: Write the failing tests:** `right_edge_resize_at_anchor_0_rotated` (rotation 30°, width 100 → 150 along the layer's axis, x/y unchanged, the left edge stays in scene); `left_edge_resize_at_anchor_0_5_rotated` (rotation 30°: the right edge stays in scene within 1e-6, width grows, x/y compensate); `a_skewed_layer_left_edge` (Review Focus); `shift_corner_keeps_the_aspect`; `radius_corners_resize_uniformly`; `text_max_width_starts_from_the_laid_out_box` (Review Focus); `resize_through_the_opposite_edge_clamps` (Review Focus); `locked_size_falls_back_to_scale` and `no_size_falls_back_to_scale` (Group); `left_edge_hidden_without_compensable_x`; `preview_overrides_size_and_x_y_from_the_first_move`.
- [ ] **Step 2: Run** — FAIL.
- [ ] **Step 3: Implement**, then build (`cargo build -p celesta-editor`) and start `./target/debug/celesta-editor examples/feature-tour/film.tsx` for ~15 s to confirm it starts; stop only that PID.
- [ ] **Step 4: Run** the crate suite — PASS; clippy clean.
- [ ] **Step 5: Commit** — `feat(editor): resize a layer edge by edge with the Rect tool`

---

### Task 6: Docs and handoff

**Files:**
- Modify: `packages/website/content/{en,ja}/docs/01-(getting-started)/03-preview.md`, `docs/superpowers/specs/2026-10-23-editor-property-editing-design.md` (only where the Rulings settle what the spec left open: the pivot is anchor + origin, the one compensation rule, the fallback labels)
- Create: `docs/handoff/2026-10-24-canvas-tools.md`

- [ ] **Step 1:** User docs: the Center/Pivot toggle (and when it reads "Pivot (x, y not editable)"), the Rect/Scale tools (which handles each primitive gets, Shift for aspect, `maxWidth` on `Text`), the pivot cross and Alt-drag. Japanese mirrors English in plain です・ます.
- [ ] **Step 2:** Handoff: what changed per module, the Rulings, known gaps, what 2d builds on (`canvas_geometry` for mapping text metrics), and "Verify by hand": rotate a `Rect` in Center mode (it turns about its middle), switch to Pivot (about its top-left), Alt-drag the cross to the middle and rotate again; resize a `Rect` from its left edge while rotated; resize a `Circle` from a corner; drag a `Text`'s right edge to wrap it; drag a layer whose `x` is set at a component call (two files, one undo); a layer with keyframed `x` shows "Pivot (x, y not editable)".
- [ ] **Step 3: Run** `pnpm --filter @celesta/website check`.
- [ ] **Step 4: Commit** — `docs(editor): describe the canvas pivot and resize tools`
