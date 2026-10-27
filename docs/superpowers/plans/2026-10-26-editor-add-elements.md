# Adding Elements (Design 5) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The user picks a parent `Group`, chooses Rect, Ellipse, Circle, Text, Image or Video from an Add menu, draws it on the canvas, and gets an element written into the TSX with literal values only, timed from the playhead to the end of the nearest window, then selected and editable like any other element.

**Architecture:** First a rendering change: `<Video>` gains `width`/`height` through the composition model, both native renderers and the web runtime, so the spec's `<Video width>` sizes something. `crates/source-edit` gains child insertion (as the parent's last child, opening a self-closing parent) and the new element's JSX, on design 4's import editing. The GPU renderer exposes a group's children space from the layer tree alone, so a group's space is known without measuring it. The preview worker answers two new questions per frame: that space for the picked parent, and the id of the parent's last child after a reload. The editor gets an `add_element` module: pure rules (parent picking under design 3's scope, the timing windows that apply to the parent, the drawn gesture → parent-space values, the menu's enabled items, the add mode's transitions, the multi-file write) and the view glue (the Add menu in the canvas-tool row, the overlay, the write through `record_own_writes`, the selection of the added layer).

**Tech Stack:** Rust (`celesta-composition`, `celesta-renderer`, `celesta-gpu-renderer`, `celesta-source-edit` on oxc 0.154, `celesta-editor` with GPUI via gpui-kit 0.6, `celesta-react-bridge`), TypeScript in `packages/react` and `packages/web` (Vitest, `.mts`, importing `../src`), docs in the website package.

**Spec:** `docs/superpowers/specs/2026-10-23-editor-add-elements-design.md`. Builds on design 2 (property tables, `import_path`, `record_own_writes`), design 3 (`Scope`, `ScopeState`, `chosen_scope`, `ScopeChoices`, `Edit::Splice`), design 4 (`ensure_imports`, `ImportName`, `ImportLock`, `indent_unit`, `window_role`, `set_timing_span`).

## Global Constraints

- Parent: (1) the selected layer if it is a primitive `Group`; (2) otherwise the nearest `Group` in the selected layer's source chain whose JSX has a children region in the source. A chain entry qualifies only with a `layer_id`. None, or nothing selected: the items are disabled and the menu says "Select a group, or a layer inside one, to add into". The menu header names the parent ("Add to `<Group>` in `<PriceTag>`", the component from `enclosing_component`).
- Design 3's scope applies when the parent is shared (Rulings): "Shared value" adds into it; "This scene" picks the nearest qualifying parent in the scene's own code, and the header says so ("Add to `<Group>` in `<Intro>` · This scene").
- Gestures: Rect, Ellipse, Image, Video drag a box (a click makes 200 × 200; Image and Video keep the file's aspect, 400 wide); Circle drags from the center outward (a click makes radius 100); Text is a click where it starts; Image and Video open a file picker first (`import_path(entry_dir, entry_dir, chosen)`, so `src` resolves against the entry's directory and an outside file is copied into `assets/`). A press that moves less than 4 viewer pixels is a click. Sizes are in the parent's units.
- The add mode ends on Escape, after one addition, when the selection changes, when Select layers is turned off, when playback starts, and when a reload not caused by the addition lands.
- What is written: the parent's last child; wrapped in `<Sequence from durationInFrames>` from the parent's local frame at the playhead to the end of the nearest timing window around the parent (design 4's rule), no wrapper on that window's first frame; literals only:
  - Rect, Ellipse: `x`, `y`, `width`, `height`, `fill="#4F7CFF"`;
  - Circle: `x`, `y` at the center, `anchorX={0.5} anchorY={0.5}`, `radius`, `fill="#4F7CFF"`;
  - Text: `x`, `y`, `fontSize={48}`, `color="#FFFFFF"`, children `Text`;
  - Image, Video: `src`, `x`, `y`, `width`.
- A self-closing parent is opened; inserted lines are indented one level deeper than the parent's opening tag with the file's indent unit (`indent_unit`).
- Imports: `Rect`, `Text`, `Image`, `Video`, `Group`, `Sequence` from `@celesta/react`; `Ellipse`, `Circle` from `@celesta/shapes`; one `ensure_imports` call per module, so the names a module needs land in one edit (sorted lists stay sorted, a clashing binding locks with "`Rect` here is not Celesta's", a namespace import writes `C.Rect`, else one new line after the last import).
- One addition is one history entry; undo removes it. After the reload the new element is selected.
- UI copy English, inspector style; lucide icons (`square`, `circle`, `type`, `image`, `film`, `plus`).
- Editor tests call pure functions only (no GPUI test context); the write is tested with `write_all_recorded` and `EditHistory` on temp files, as `tests/element_edit.rs` does.
- Rendering changes carry before/after screenshots of the same scene in their PR (AGENTS.md), uploaded with `gh pr create --attach`.
- TDD per task; red/green runs only the new tests (`cargo test -p <crate> -- <names>`); each crate's suite once per task; Vitest in TypeScript; `cargo clippy --all-targets` clean on the touched crates (the pre-existing `large_enum_variant` on `SourceLookup` excepted); `cargo fmt --check`.
- No `git commit --amend`; commit and test commands are separate calls; stage paths explicitly.

## Rulings (decided here; the spec leaves them open or is amended)

- **`<Video>` gets a size, as a rendering change.** `LayerContent::Video` has no size and both renderers draw the decoded frame at its own size, so the spec's `<Video width>` would be ignored. `Video` gains `width`/`height: Option<f64>` in the composition model, sized exactly like `Image` (both given: both apply; one given: the frame's aspect; none: the frame's size), in the GPU renderer, the CPU renderer and the web runtime, with the property table's `size` and a Size group (`min: 1`). No `fit` (the spec writes only `width`). Correctness over a `scale` workaround; the spec is amended in Task 7.
- **Where the Add menu lives.** The canvas-tool row is the spec's viewer toolbar. It now shows whenever Select layers is on in a React preview (`shows_tool_row` drops the selection argument): with nothing selected it holds only the Add menu, with a selection the Rotate-about and Handles toggles follow. The menu still explains itself without a selection, and the canvas no longer resizes when a layer is selected. A gpui-kit `dropdown_menu` button (the `export_loudness_button` pattern) with the header and the disabled reason as `PopupMenu` labels.
- **The scope of an addition.** A parent is shared when the occurrence index's instance chains through its position have more than one outer part (the chain above it) or their ranges show in more than one scene (`is_shared`); "in the scene's own code" is not shared. The scope in effect is the selected element's: with a scope control (`ScopeState::Options`) its `chosen_scope`; without one (`NotShared`) design 3's default "This scene", so a parent shared through `{children}` is never added into unasked; a remembered choice now locked, or a parent whose sharing the index can't answer, disables the items with the lock's text or design 3's note ("Scanning scenes…", "Couldn't scan scenes: …"). Without a parent in the scene's own code under "This scene": "No group in this scene's own code to add into".
- **The timing windows that apply to the parent.** The report's windows wrap the selected element, nearest first; those written below the parent in the chain (the selected element's own `Sequence`) do not wrap the new child. They are dropped before `window_role` and `set_timing_span` (`report_at_parent`). A remaining nearest window that is `remapped` disables the items with "Can't add inside a loop or freeze frame" (design 4's text speaks of dragging). The element's own window and the scene's are alike here, both being the window around the parent, so an `Own` window goes to `set_timing_span` as `WindowRole::Scene`, which design 4 offers "Set timing" for.
- **Drawing in a transformed parent.** The press and the pointer are mapped through the inverse of the parent's children space, as `drag_values` maps a drag; the box is min/max of the two points in parent space, so the element is drawn turned with its parent, under the drawn quad. The overlay draws that parent-space box mapped back to the scene (a turned quad; a circle as a polygon; a caret mark for Text). Click sizes are parent units at the press. Every drawn side is at least 1 (an Ellipse with a zero side draws nothing). Image and Video take `width` from the drag and their height from the file's aspect, extending in the drag's direction; without a known aspect (an SVG, a failed probe) the box as dragged.
- **A Circle click makes radius 100** (the 200 × 200 click box's circle): `<Circle>` with radius 0 draws nothing, and the spec gives no click rule. Amended in Task 7.
- **A group's children space** comes from the layer tree alone (`parent_space`), so an empty group has one; `None` when the group or an ancestor turns in 3D or has a perspective, which disables the items with "Can't draw into a group turned in 3D". The worker computes it for the picked parent (`space_of`), since it measures only one layer per frame through `frame_of`.
- **A children region** is any JSX element with a closing tag, or a self-closing one (opened on insert). A parent whose only child is `{children}` or another prop (`props.children`, a destructured prop) does not qualify (the spec's non-goal); `<Group>{items.map(…)}</Group>` does, the new element going after the map.
- **An empty group can't be picked yet.** An empty `<Group />` draws no region and is in no chain, so it can't be selected; opening a self-closing parent is built and tested in source-edit and listed as a known gap in the handoff.
- **Selecting the added element.** Its layer id can't be predicted (children are numbered from the parent's path, and siblings such as masks, audio and components add any number), but the new element is the parent's last JSX child and active at the playhead, so it is the parent layer's last child in the reloaded scene (that child's first child when wrapped). The write keeps the old selection like every own write (`SelectionKeep`); from the first frame drawn with the reloaded code the request carries `added_under` (the parent's layer id, unchanged by appending a child); the answer selects that layer, replacing the kept one (whose late lookup is then ignored as no longer selected); the next lookup must have `chain[0]` at the inserted position, else the selection is cleared. No such layer: the pending selection is dropped and the kept selection stays. In a `.map()`-repeated parent this selects the new layer under the instance that was picked.
- **Image and Video aspect** for the overlay: images via `image::ImageReader::into_dimensions` (the decoders reach the editor's `image` through `celesta-gpu-renderer`'s features), videos via `FfmpegBackend::probe` on the background executor. `@celesta/shapes` missing from `primitive_properties` disables Ellipse and Circle with "`Ellipse` comes from `@celesta/shapes`, which this composition doesn't import".

## Review Focus

- **A parent `Group` rotated 30° and scaled 2, a box dragged up and to the left**: the written `x`/`y` are the parent-space minimum of the mapped press and release (not the press), `width`/`height` the parent-space distances between them along each axis (the drawn diagonal through the inverse map, so halved by the scale), and the overlay is the turned quad with those two points as opposite corners. Test in Task 4 (`a_box_dragged_up_left_in_a_rotated_scaled_parent`).
- **The selected `<Rect>` inside its own `<Sequence from={30}>` under the `Group`**: the new child's timing comes from the Group's window, not the Rect's, and on the Group's window's first frame no wrapper is written. Test in Task 4 (`the_selected_element_s_own_sequence_does_not_time_the_child`).
- **"This scene" with the nearest group inside `<PriceTag>`, called by two scenes**: the parent falls back to the nearest group in the scene's own code and the header ends "· This scene"; a group shared only through `{children}` with an unshared selected element falls back too. Test in Task 4 (`this_scene_falls_back_to_a_parent_in_the_scene_s_code`, `a_parent_shared_through_children_is_not_used_unasked`).
- **`Rect` and `Sequence` both new in a file with `import { Group, Text } from '@celesta/react'`**: one edit makes `{ Group, Rect, Sequence, Text }`; in a file importing only `react` (as `import React from 'react';`), one line `import { Rect, Sequence } from '@celesta/react';` after it, in that import's quotes and semicolon (a file without any import gets `"@celesta/react";` first, as `ensure_imports` writes). Test in Task 2 (`rect_and_sequence_both_new_land_in_one_import`).
- **The reload after an addition into a `.map()`-repeated parent with an explicit `id`**: the new layer under the picked instance is selected, and a lookup whose `chain[0]` is elsewhere clears it. Tests in Task 5 (`the_added_layer_is_the_named_parent_s_last_child`) and Task 6 (`the_added_layer_is_selected_then_confirmed`).

---

### Task 1: `<Video>` width and height (composition, both native renderers, web runtime)

**Files:**
- Modify: `crates/composition/src/model.rs` (`LayerContent::Video` and `LayerContentDef::Video` gain `width`, `height` with the `Image` arm's serde attributes)
- Modify: every `LayerContent::Video { … }` literal — `crates/composition/src/tests.rs`, `crates/evaluator/src/lib.rs`, `crates/exporter/src/project.rs`, `crates/renderer/src/{layer.rs,tests.rs}`, `crates/gpu-renderer/src/{prepare.rs,tests.rs,tests/transform_3d.rs}`, `crates/react-bridge/tests/node_integration.rs`
- Modify: `crates/gpu-renderer/src/prepare.rs` (the Video arm scales the quad to the display size like the Image arm), `crates/renderer/src/layer.rs` (the decoded frame drawn at the display size; the placeholder at it when given)
- Modify: `packages/react/src/generated/LayerContent.ts` (regenerated: `pnpm --filter @celesta/react codegen`), `packages/react/src/components.ts` (`VideoProps.width`/`height` documented as `ImageProps`'; the Video table gets `size: { width: 'width', height: 'height' }` and `Size: { width: num(undefined, { min: 1 }), height: num(undefined, { min: 1 }) }`), `packages/react/src/render.ts` (the video content carries them, validated as the image's)
- Modify: `packages/web/src/types.ts` (the video content type), `packages/web/src/scene-canvas.ts` (`w`/`h` for a video as for an image, without `fit`)
- Modify: `examples/greenscreen/composite.tsx` — the raw-footage corner's `<Video scale={0.35}>` becomes `width` at its current drawn width (the size it was scaled to), since `width` supersedes that workaround
- Test: `crates/composition/src/tests.rs`, `crates/gpu-renderer/src/tests.rs`, `crates/renderer/src/tests.rs`, `packages/web/test/scene-canvas.test.mts`, `packages/react/test/video-audio.test.mts`, `packages/react/test/property-tables.test.mts`

**Interfaces:**
- Produces: `LayerContent::Video { asset: ResolvedAsset, timing: MediaTiming, width: Option<f64>, height: Option<f64> }`; the TypeScript `LayerContent` video variant with `width?: number; height?: number`.

- [ ] **Step 1: Write the failing tests.** One fixture for every renderer: a 320 × 180 video frame (an injected decoder in Rust, as `renders_a_video_frame_from_the_injected_decoder` and `decodes_positions_and_fades_a_video_frame_on_the_gpu` do; a stand-in video with `videoWidth`/`videoHeight` on the web), so native and web assert the same sizes.
  - `a_video_size_round_trips` (composition) — `width: 160` serializes without `height`; JSON without either parses with both `None`.
  - `sizes_a_video_frame_like_an_image` (gpu-renderer, readback; renderer, pixels) — `width: 160` draws 160 × 90; `width: 160, height: 160` draws 160 × 160; `height: 90` draws 160 × 90; neither draws 320 × 180.
  - `sizes a video like an image` (`scene-canvas.test.mts`) — `drawImage` receives the same four sizes.
  - `passes a video's width and height to its layer` and `rejects a video width that is not finite and positive` ("Video width must be finite and positive") in `video-audio.test.mts`; `the Video table sizes by width and height` in `property-tables.test.mts`.
- [ ] **Step 2: Run them, expect FAIL** — `cargo test -p celesta-composition -- a_video_size_round_trips`; `cargo test -p celesta-gpu-renderer -- sizes_a_video_frame_like_an_image`; `cargo test -p celesta-renderer -- sizes_a_video_frame_like_an_image`; `pnpm --filter @celesta/react test`; `pnpm --filter @celesta/web test`
- [ ] **Step 3: Implement**, then `pnpm --filter @celesta/react codegen` and check the regenerated `LayerContent.ts` is the only generated change.
- [ ] **Step 4: Run them, expect PASS**; then `cargo test -p celesta-composition -p celesta-renderer -p celesta-gpu-renderer -p celesta-evaluator -p celesta-exporter -p celesta-react-bridge`, both package suites, clippy, fmt.
- [ ] **Step 5: Screenshots** — a still of the same frame of `examples/greenscreen/composite.tsx` (with the updated corner) from the native renderer and the web runtime, on `main` and on this branch: before, the corner footage ignores `width` and draws at the frame's size; after, it is the size `scale={0.35}` gave. Upload all four with the PR.
- [ ] **Step 6: Commit** — `feat: size a video by width and height`

### Task 2: Inserting a child and building the element (`crates/source-edit`)

**Files:**
- Create: `crates/source-edit/src/add.rs`
- Modify: `crates/source-edit/src/lib.rs`
- Test: `crates/source-edit/tests/add.rs`

**Interfaces:**
- Consumes: design 3's `Edit::Splice`; design 4's `ensure_imports(text: &str, module: &str, names: &[&str]) -> Result<(Option<Edit>, Vec<ImportName>), ImportLock>`, `ImportName`, `ImportLock`, `indent_unit(text: &str, at: u32) -> String`; `ElementRef`, `element_at`.
- Produces:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewKind { Rect, Ellipse, Circle, Text, Image, Video }
impl NewKind { pub fn tag(self) -> &'static str; /* `@celesta/react` or `@celesta/shapes` */ pub fn module(self) -> &'static str; }
pub enum NewElement { Rect { x: f64, y: f64, width: f64, height: f64 }, Ellipse { x: f64, y: f64, width: f64, height: f64 }, Circle { x: f64, y: f64, radius: f64 }, Text { x: f64, y: f64 }, Image { src: String, x: f64, y: f64, width: f64 }, Video { src: String, x: f64, y: f64, width: f64 } }
impl NewElement { pub fn kind(&self) -> NewKind; }
/// The wrapper's `from`/`durationInFrames`; an addition without one is unwrapped.
pub struct Timing { pub from: i64, pub duration: i64 }
pub enum AddLock { NoChildrenRegion, Import(ImportLock) }
impl AddLock { pub fn text(&self) -> String; }
/// Whether `element` has a children region an element can be added to.
pub fn accepts_children(text: &str, element: &ElementRef) -> bool;
/// What would refuse adding a `kind` (wrapped or not) under `parent`, checked when the menu opens.
pub fn add_lock(text: &str, parent: &ElementRef, kind: NewKind, wrapped: bool) -> Option<AddLock>;
/// The edits and where the new element's `<` is in the edited text (1-based line and column, as the runtime reports positions).
pub struct Added { pub edits: Vec<Edit>, pub line: u32, pub column: u32 }
pub fn add_child(text: &str, parent: &ElementRef, new: &NewElement, timing: Option<&Timing>) -> Result<Added, AddLock>;
```

- [ ] **Step 1: Write the failing tests** (TSX fixtures, asserting the text after `apply`, which must parse):
  - `adds_after_the_last_child` — after `<Rect … />` inside `<Group>`, one level deeper than `<Group>`.
  - `adds_into_an_empty_parent` — `<Group></Group>` → the child on its own line between the tags.
  - `opens_a_self_closing_parent` — `<Group />` → `<Group>`, the child, `</Group>`, in a file indented with tabs: the child indented with one tab more than `<Group`.
  - `indents_with_tabs_and_spaces` — two-space and tab files, each one unit deeper.
  - `writes_each_kind_s_literals` — `<Rect x={120} y={80} width={320} height={180} fill="#4F7CFF" />`; Ellipse the same attributes; `<Circle x={…} y={…} anchorX={0.5} anchorY={0.5} radius={…} fill="#4F7CFF" />`; `<Text x={…} y={…} fontSize={48} color="#FFFFFF">Text</Text>`; `<Image src="assets/a.png" x={…} y={…} width={…} />`; Video likewise.
  - `wraps_in_a_sequence` — `Timing { from: 45, duration: 75 }` → the spec's block exactly: `<Sequence from={45} durationInFrames={75}>`, the Rect one level deeper, `</Sequence>`; `None` → no wrapper.
  - `imports_from_react_and_shapes` — an Ellipse in a file with only `@celesta/react` imported adds `import { Ellipse } from '@celesta/shapes';` after the last import.
  - `rect_and_sequence_both_new_land_in_one_import` — the Review Focus case: `{ Group, Text }` → `{ Group, Rect, Sequence, Text }` in one edit; a file importing only `react` → one line `import { Rect, Sequence } from '@celesta/react';` after it, in that import's quotes and semicolon.
  - `already_imported_needs_nothing` — `Rect` and `Sequence` imported: no import edit.
  - `a_namespace_import_writes_members` — `import * as C from '@celesta/react'` → `<C.Sequence …>` and `<C.Rect …>`.
  - `a_clashing_binding_locks` — a local `const Rect = …` → `AddLock::Import`, text "`Rect` here is not Celesta's"; `add_lock` returns the same lock before any drawing, and `None` for `Text` in that file.
  - `a_children_prop_is_not_a_region` — `<Group>{children}</Group>`, `<Group>{props.children}</Group>`, a destructured `{content}` prop alone → `accepts_children` false; `a_map_expression_is_a_region` — `<Group>{items.map(…)}</Group>` → true, the child after the map.
  - `reports_the_new_element_s_position` — with an import line added above, `line`/`column` point at the new `<Rect` in the edited text (and at `<Rect`, not `<Sequence`, when wrapped).
- [ ] **Step 2: Run them, expect FAIL** — `cargo test -p celesta-source-edit -- add`
- [ ] **Step 3: Implement.** Attributes go through `write_value`; one `ensure_imports` per module with the names it needs (`tag`, and `Sequence` when wrapped); the position is computed after `apply` from the inserted offset shifted by the edits before it.
- [ ] **Step 4: Run them, expect PASS**; then `cargo test -p celesta-source-edit`.
- [ ] **Step 5: Commit** — `feat(source-edit): add an element as a parent's last child`

### Task 3: A group's children space (`crates/gpu-renderer`)

**Files:**
- Modify: `crates/gpu-renderer/src/{layer_frame.rs,lib.rs}`
- Test: `crates/gpu-renderer/src/layer_frame.rs` tests

**Interfaces:**
- Produces: `pub fn parent_space(scene: &Scene, id: &str) -> Option<[f64; 6]>` — the map from group `id`'s children space to scene pixels (its parent map composed with `Affine::from_transform` of the group), in `LayerFrame::parent`'s layout; `None` for a missing id, a non-group, or a group that itself or through an ancestor turns in 3D (`rotation_x`/`rotation_y`) or has a perspective.

- [ ] **Step 1: Write the failing tests** — `an_empty_group_has_a_parent_space` (translated `x: 100, y: 50` → `[1, 0, 0, 1, 100, 50]`); `a_nested_rotated_scaled_group_composes_its_ancestors` (an outer group translated, the inner rotated 30° and scaled 2: the composition of both); `a_group_turned_in_3d_has_none` (the group's own `rotation_x`, and an ancestor's perspective); `a_leaf_has_none`.
- [ ] **Step 2: Run them, expect FAIL** — `cargo test -p celesta-gpu-renderer -- an_empty_group_has a_nested_rotated_scaled a_group_turned_in_3d a_leaf_has_none`
- [ ] **Step 3: Implement** with the private `find`, adding the found group's own 3D check.
- [ ] **Step 4: Run them, expect PASS**; then `cargo test -p celesta-gpu-renderer`.
- [ ] **Step 5: Commit** — `feat(gpu-renderer): expose a group's children space`

### Task 4: Parent, timing, gesture and mode rules (`crates/editor`, pure)

**Files:**
- Create: `crates/editor/src/add_element.rs`
- Modify: `crates/editor/src/main.rs`
- Test: `crates/editor/src/add_element.rs` `#[cfg(test)]` (reports, occurrence indexes and chapters built by hand as in `impact.rs`'s tests)

**Interfaces:**
- Consumes: Task 2's `NewKind`, `NewElement`, `Timing`; design 3's `Scope`, `ScopeState`, `chosen_scope`, `ScopeLock::text`, `OccurrenceState`; design 4's `WindowRole`, `window_role(report, chapters, composition_end, frame)` and `set_timing_span(role: &WindowRole, chapters: &[ReactChapter], composition_end: u64, playhead: u64) -> Option<(i64, i64)>`; `OccurrenceIndex`, `scenes_of`, `outermost_chapter_at`.
- Produces:

```rust
pub(crate) struct AddParent { pub(crate) entry: LayerSource, pub(crate) index: usize /* in the chain */, pub(crate) layer_id: String, pub(crate) this_scene: bool /* moved outward by "This scene" */ }
pub(crate) enum ParentScope { Chosen(Scope), NoControl, Unknown(String) }
/// The selected element's scope for an addition: `Options` → `chosen_scope` (Err → `Unknown` with the lock's text), `NotShared` → `NoControl`, `Unknown` → its note.
pub(crate) fn parent_scope(state: &ScopeState, remembered: Option<Scope>) -> ParentScope;
/// Whether the element at `entry` is shared: the index's instance chains through it have more than one outer part, or their ranges show in more than one scene. Err is design 3's note when the index can't answer.
pub(crate) fn is_shared(occurrences: Option<&OccurrenceState>, chapters: &[ReactChapter], entry: &SourcePos) -> Result<bool, String>;
/// In the scene's own code: `is_shared` is `Ok(false)`.
pub(crate) fn in_scene_code(occurrences: Option<&OccurrenceState>, chapters: &[ReactChapter], entry: &SourcePos) -> bool;
/// Rules 1–2 and the scope; Err is the menu's reason. `accepts` reads the entry's file (`accepts_children`); `shared` is `is_shared`.
pub(crate) fn pick_parent(report: Option<&LayerSourceReport>, scope: &ParentScope, accepts: impl Fn(&LayerSource) -> bool, shared: impl Fn(&LayerSource) -> Result<bool, String>) -> Result<AddParent, String>;
/// "Add to `<Group>` in `<PriceTag>`", "Add to `<Group>`" without a component, "… · This scene" when moved.
pub(crate) fn add_header(parent: &AddParent, component: Option<&str>) -> String;
/// `report` with the windows of chain entries below `parent_index` dropped.
pub(crate) fn report_at_parent(report: &LayerSourceReport, parent_index: usize) -> LayerSourceReport;
/// The wrapper for a child of the parent at `parent_index`, from `window_role` on `report_at_parent`: `None` when no wrapper is written; Err for a remapped nearest window.
pub(crate) fn added_timing(report: &LayerSourceReport, parent_index: usize, chapters: &[ReactChapter], composition_end: u64, playhead: u64) -> Result<Option<Timing>, String>;
/// The written values for a gesture from `start` to `end` (scene pixels) in the parent's children space; `slop` is the click distance in scene pixels.
pub(crate) fn drawn_geometry(kind: NewKind, src: Option<&str>, (start, end): (Point, Point), parent_space: [f64; 6], aspect: Option<f64>, slop: f64) -> Option<NewElement>;
/// The overlay: the parent-space shape mapped back to scene pixels (four corners; a 32-point polygon for a circle; the press point for Text).
pub(crate) fn drawn_outline(kind: NewKind, (start, end): (Point, Point), parent_space: [f64; 6], aspect: Option<f64>, slop: f64) -> Vec<Point>;
/// Each menu item and its disabled reason, in menu order: `reason` (no parent, scope, timing, 3D) for all, then a missing `primitive_properties` table, then `lock_of` (`add_lock`'s text).
pub(crate) fn menu_items(reason: Option<&str>, has_table: impl Fn(&str) -> bool, lock_of: impl Fn(NewKind) -> Option<String>) -> Vec<(NewKind, Option<String>)>;
pub(crate) struct AddTool { pub(crate) kind: NewKind, pub(crate) src: Option<String>, pub(crate) aspect: Option<f64> }
pub(crate) enum AddMode { Off, Picking(NewKind) /* file dialog open */, Armed(AddTool), Drawing { tool: AddTool, start: Point, now: Point } }
pub(crate) enum AddEvent { Chosen(NewKind), Picked(Option<String>), Measured(Option<f64>), Press(Point), Move(Point), Release(Point), Escape, Ended }
/// The next mode, and the gesture to write on a release.
pub(crate) fn add_step(mode: AddMode, event: AddEvent) -> (AddMode, Option<(AddTool, Point, Point)>);
```

`is_shared`: over every element's instances whose chain contains `entry`, collect the chain after it; shared when two differ, or `scenes_of` over all their ranges gives more than one scene. `pick_parent`: the candidate by rule 1, else rule 2 (the nearest `Group` entry with a `layer_id` that `accepts`); not shared → it; shared → `Chosen(Shared)` keeps it, `Chosen(ThisScene)` and `NoControl` take the next qualifying entry outward that is not shared (`this_scene: true`), `Unknown(note)` → Err(note).

`added_timing`: `Remapped` → Err; an `Own(w)` window is passed to `set_timing_span` as `WindowRole::Scene(w)` (for a new child either kind is simply the window around the parent, while `set_timing_span` offers "Set timing" for the scene's only); `Scene` and `None` as they are. No wrapper when `from` is 0 and a window is around the parent, or `from` is 0 and the span runs to the composition's end without one.

- [ ] **Step 1: Write the failing tests:**
  - `the_selected_group_is_the_parent`; `the_nearest_group_in_the_chain_is_the_parent`; `a_group_without_a_children_region_is_skipped`; `nothing_selected_gives_the_reason` ("Select a group, or a layer inside one, to add into").
  - `is_shared_reads_the_index` — one outer part in one scene → false; two calls of `PriceTag` → true; one chain spanning two scenes → true; `Scanning` → Err "Scanning scenes…"; `Failed("x")` → Err "Couldn't scan scenes: x".
  - `this_scene_falls_back_to_a_parent_in_the_scene_s_code` — the nearest group inside a shared `PriceTag`, `Chosen(ThisScene)` → the scene's group, `this_scene` true, header "Add to `<Group>` in `<Intro>` · This scene"; `Chosen(Shared)` → the PriceTag group; no unshared group → "No group in this scene's own code to add into".
  - `a_parent_shared_through_children_is_not_used_unasked` — an unshared selected element under a shared group, `NoControl` → the fallback as "This scene"; an `Unknown` scope with a shared candidate → Err with its note; with an unshared candidate → that candidate.
  - `parent_scope_follows_the_selected_element` — `Options` with nothing remembered → `Chosen(ThisScene)`; a remembered choice now locked → `Unknown(lock text)`; `NotShared` → `NoControl`.
  - `the_header_names_the_parent_and_its_component` — "Add to `<Group>` in `<PriceTag>`"; no component → "Add to `<Group>`".
  - `the_selected_element_s_own_sequence_does_not_time_the_child` — chain `[Rect, Sequence(from 30), Group, Sequence]`, windows `[Rect's Sequence 330–390, Group's Sequence 300–420]` in chapter 300–420: at playhead 345 the timing is `from 45, duration 75`; at playhead 300 no wrapper; with parent index 0 nothing is dropped.
  - `an_own_window_around_the_parent_times_the_child` — the Group inside a hand-placed `Sequence` 330–360 in chapter 300–420, playhead 345 → `from 15, duration 15`.
  - `without_a_window_the_child_runs_to_the_chapter_end` — playhead 345, outermost chapter 300–420 → `from 345, duration 75`; playhead 0, no chapter, composition 600 → no wrapper.
  - `a_remapped_window_locks_the_addition` — "Can't add inside a loop or freeze frame".
  - `a_box_in_a_translated_rotated_scaled_parent` and `a_box_dragged_up_left_in_a_rotated_scaled_parent` — parent space translated (100, 50), rotated 30°, scaled 2: the written `x`/`y` are the parent-space minimum of the mapped press and release, `width`/`height` their parent-space distance along each axis; `drawn_outline` is that box mapped back, with the press and release as opposite corners.
  - `a_click_makes_200_by_200` (Rect, Ellipse, in parent units, at the press); `image_and_video_keep_the_aspect_400_wide` (a click → `width: 400` at the press; in an identity parent space a drag 300 right and upwards with aspect 2 → `width: 300`, `y` 150 above the press; no aspect → the box as dragged); `a_circle_drags_from_its_center` (radius = parent-space distance; a click → radius 100); `text_is_placed_where_clicked`; `every_drawn_side_is_at_least_one`.
  - `menu_items_say_why` — a reason disables all six; no `Ellipse` table → Ellipse and Circle say "`Ellipse` comes from `@celesta/shapes`, which this composition doesn't import" (Circle with its own name); a clash disables only that kind with "`Rect` here is not Celesta's".
  - `add_mode_steps` — `Chosen(Image)` → `Picking`; `Picked(None)` → `Off`; `Picked(Some)` → `Armed` with `src`; `Measured` fills `aspect` in `Armed` and `Drawing`; `Press` → `Drawing`; `Move` updates `now`; `Release` → `Off` with the gesture; `Escape` and `Ended` → `Off` from every mode with no gesture.
- [ ] **Step 2: Run them, expect FAIL** — `cargo test -p celesta-editor -- add_element::`
- [ ] **Step 3: Implement** (pure; `slop` is 4 viewer pixels in scene pixels, given by the caller).
- [ ] **Step 4: Run them, expect PASS**; then `cargo test -p celesta-editor`.
- [ ] **Step 5: Commit** — `feat(editor): rules for picking a parent and drawing an element into it`

### Task 5: The preview worker's parent space and added layer (`crates/editor`)

**Files:**
- Modify: `crates/editor/src/preview.rs` (request and result fields; computed after the render from the drawn scene), `crates/editor/src/refresh.rs` (requests fill the new fields)
- Test: `crates/editor/src/preview.rs` `#[cfg(test)]` (scenes built by hand)

**Interfaces:**
- Consumes: Task 3's `parent_space`.
- Produces:

```rust
pub(crate) struct AddedUnder { pub(crate) parent: String, pub(crate) wrapped: bool }
pub(crate) struct PreviewRequest { /* existing */ pub(crate) space_of: Option<String>, pub(crate) added_under: Option<AddedUnder> }
pub(crate) struct PreviewResult { /* existing */ pub(crate) parent_space: Option<(String, Option<[f64; 6]>)>, pub(crate) added: Option<(String, Option<String>)> /* parent, the added layer */ }
/// The added layer: `under.parent`'s last child, or that child's first child when `wrapped`.
pub(crate) fn added_layer(scene: &Scene, under: &AddedUnder) -> Option<String>;
```

- [ ] **Step 1: Write the failing tests** — `the_added_layer_is_the_named_parent_s_last_child` (a parent with an explicit id `card` among two `.map()` instances, each with a mask and three children: the last child of `card` only, not of the other instance); `a_wrapped_addition_is_the_sequence_s_child` (the last child is a sequence group → its first child's id); `a_missing_parent_adds_nothing` (`None`).
- [ ] **Step 2: Run them, expect FAIL** — `cargo test -p celesta-editor -- the_added_layer_is a_wrapped_addition a_missing_parent_adds`
- [ ] **Step 3: Implement**; both fields are answered for interactive `WholeScene` frames only, like `frame_of`.
- [ ] **Step 4: Run them, expect PASS**; then `cargo test -p celesta-editor`.
- [ ] **Step 5: Commit** — `feat(editor): report a group's space and an added layer from the preview`

### Task 6: The Add menu, the add mode, the write and the selection

**Files:**
- Modify: `crates/editor/src/add_element.rs` (view glue: the parent and its header from the lookup, the menu, the mode, the write, the pending selection), `crates/editor/src/viewer.rs` (`shows_tool_row` without the selection; the Add menu at the row's start; the add press before the double-click check; the overlay in the paint closure), `crates/editor/src/playback.rs`/`render.rs` (move, up and Escape routing; playback ends the mode), `crates/editor/src/background.rs` (the added layer, the parent space, the confirm), `crates/editor/src/refresh.rs` (the pending selection's first reloaded generation, next to `confirm_from`)
- Test: `crates/editor/src/tests/add_element.rs` (declared in `tests.rs`), `crates/editor/src/viewer.rs` tests

**Interfaces:**
- Consumes: Tasks 2, 4, 5; design 3's `ScopeChoices`, `scope_state` and the view's scene calls; `import_path`, `record_own_writes`, `write_all_recorded`, `EditHistory`, `SelectionKeep`.
- Produces:

```rust
/// The texts after adding `new` under the parent written at `parent` in `file`, and the new element's position. Err: "The source changed while editing" when no qualifying parent is at `parent` any more, else the lock's text.
pub(crate) fn add_files(texts: &Texts, file: &Path, parent: &SourcePos, new: &NewElement, timing: Option<&Timing>) -> Result<(Vec<(PathBuf, String)>, SourcePos), String>;
pub(crate) struct PendingSelect { pub(crate) position: SourcePos, pub(crate) under: AddedUnder, pub(crate) from: Option<u64> /* first generation drawn with the reloaded code */ }
pub(crate) enum AddedFate { Wait, Select(String), Drop }
/// What an answer from preview `generation` does with the pending selection.
pub(crate) fn added_fate(pending: &PendingSelect, generation: u64, added: Option<&(String, Option<String>)>) -> AddedFate;
/// Whether the selected added layer's lookup shows the inserted element (`chain[0]` at `position`).
pub(crate) fn added_confirmed(position: &SourcePos, lookup: &LookupResult) -> bool;
```

- [ ] **Step 1: Write the failing tests:**
  - `an_addition_writes_one_entry_and_undo_removes_it` — `add_files` inside `write_all_recorded` on a temp file: one `HistoryEntry`, the Rect (and its import) written; `undo` restores the file byte for byte.
  - `add_files_refuses_a_moved_parent` — the parent's line now holds a `Rect` → "The source changed while editing", nothing written.
  - `add_files_reports_the_position_after_the_import` — the position is `add_child`'s, as a `SourcePos` in `file`.
  - `the_added_layer_is_selected_then_confirmed` — `added_fate`: before `from` → `Wait`; an answer with `Some(id)` → `Select(id)`; with `None` → `Drop`; `added_confirmed` true for a lookup whose `chain[0]` is the position, false for one elsewhere (the selection is then cleared).
  - `the_tool_row_shows_with_select_layers_on` — `shows_tool_row(true, true)` true, `(true, false)` and `(false, true)` false (the old `selected` assertion goes).
- [ ] **Step 2: Run them, expect FAIL** — `cargo test -p celesta-editor -- tests::add_element:: the_tool_row_shows`
- [ ] **Step 3: Implement.** On each lookup of the selected layer the view reads the chain's files on the background executor and keeps `pick_parent`'s answer (with `parent_scope` from design 3's state and `ScopeChoices`), `add_header` (`enclosing_component`), `added_timing` at the playhead and `add_lock` per kind. While a parent is picked, requests carry `space_of`; a parent with no space disables the items with the 3D text. Image and Video: `prompt_for_paths`, then `import_path(entry_dir, entry_dir, chosen)` and the aspect on the background executor (`AddEvent::Measured`). Mouse-up runs `drawn_geometry` and `record_own_writes(vec![file], add_files…)`; `done` stores a `PendingSelect` (a refusal shows as the viewer's notice). The own write keeps the old selection (`SelectionKeep::own_write`); when the reload applies, the pending selection's `from` is the generation `confirm_from` gets; requests from then carry `added_under`; `Select(id)` calls `select_layer(Some(id))`, whose first lookup is checked with `added_confirmed` (false → `clear_layer_selection`). The overlay strokes `drawn_outline` while `Drawing`.
- [ ] **Step 4: Run them, expect PASS**; then `cargo test -p celesta-editor`, clippy, fmt.
- [ ] **Step 5: Commit** — `feat(editor): add elements by drawing them on the canvas`

### Task 7: Docs, example and handoff

**Files:**
- Modify: `packages/website/content/{en,ja}/docs/01-(getting-started)/03-preview.md` (the Add menu in the canvas-tool row, how the parent is chosen and when "This scene" moves it, each gesture and the click sizes, what is written, timing, undo); the components reference for `<Video width height>` (en/ja)
- Modify: `docs/superpowers/specs/2026-10-23-editor-add-elements-design.md` (what the build settled: `<Video>` gains `width`/`height` in every renderer; a Circle click makes radius 100; a press under 4 viewer pixels is a click; sizes in the parent's units and the element turned with a rotated parent; the viewer toolbar is the canvas-tool row, shown whenever Select layers is on; the timing windows around the parent, not the selected element; the reasons this plan adds — no group in this scene's own code, a group turned in 3D, `@celesta/shapes` not imported, inside a loop or freeze frame; only a sole `{children}` or prop child disqualifies a parent; the added element found as the parent's last child)
- Check: `examples/delta/v0.7.0/demos/EditorDemo.tsx`
- Create: `docs/handoff/2026-10-26-add-elements.md` (decisions, known gaps — an empty `<Group />` can't be selected to add into, a parent shared through `{children}` falls back under "This scene" — and "Verify by hand" with before/after screenshots of the same frame: each kind added into a scene, the TSX diff, the timing band, selected and editable)

- [ ] **Step 1: Write the docs**; `pnpm --filter @celesta/website check`.
- [ ] **Step 2: Commit** — `docs(editor): describe adding elements`

## Order

Stacked PRs main → design 3 → design 4 → design 5; within design 5:

- **Task 1** (Video size) and **Task 3** (parent space): no design 3 or design 4 task; they can run alongside designs 3 and 4, and in parallel with each other.
- **Task 2** (insertion): design 3 Task 1 (`Edit::Splice`) and design 4 Task 3 (`ensure_imports`, `ImportName`, `ImportLock`, `indent_unit`). Parallel with Tasks 1 and 3.
- **Task 4** (pure editor rules): design 3 Task 2 (`Scope`, `ScopeState`, `chosen_scope`, `ScopeLock`) and design 4 Task 1 (`WindowRole`, `window_role`, `set_timing_span`); Task 2 here.
- **Task 5** (preview worker): Task 3 here only; parallel with Tasks 2 and 4 (it touches `preview.rs`/`refresh.rs`, which Task 4 does not).
- **Task 6** (view glue): design 3 Task 3 (`ScopeChoices` and the scene calls on the view) and design 4 Task 1 (the occurrence index it changes, `window_role`); Tasks 2, 4 and 5 here.
- **Task 7** (docs): after Task 6.
