# Timing on the Timeline (Design 4) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The selected element's start and end, and a scene's length, are changed by dragging on the timeline and written into the TSX; an element without timing of its own can be given one ("Set timing" wraps it in a `Sequence`).

**Architecture:** The runtime's occurrence index starts listing, per instance, the wrappers (`Sequence`, `Series.Sequence` items) it renders through, so a drag knows which bands it moves. The editor classifies the report's nearest timing window as the element's own or the scene's, resolves the wrapper written there as an element of its own (its chain from the wrapper outward, read unchecked) and commits through design 3's `commit_scoped`. `crates/source-edit` gains the timing kind (from the `@celesta/react` import), the `defineScenes` lists and how `<Composition durationInFrames>` is written, and JSX wrapping with import editing (reused by design 5). The timeline gets a `timeline_drag` module for band and chapter-edge drags with snapping; every rule sits in a pure helper.

**Tech Stack:** Rust (`celesta-source-edit` on oxc 0.154, `celesta-editor` with GPUI via gpui-kit 0.6, `celesta-react-bridge`), TypeScript in `packages/react` (Vitest), docs in the website package.

**Spec:** `docs/superpowers/specs/2026-10-23-editor-timeline-timing-design.md`. Builds on design 1 (timing windows, bands, occurrence index), design 2 (resolver, multi-file history, `record_own_writes`), design 3 (`Edit::Splice`, `commit_scoped`, `commit_gate`, `scope_state`, `ScopeChoices`, `SceneCalls`, `scope_in_effect`).

## Global Constraints

- A window is **the scene's** when it has no source (a `<Scenes>` scene) or its span, clipped to the composition's end and rounded to whole frames, equals the span of a chapter holding the lookup frame (a part or its outermost scene); any other window is the element's own. A `remapped` window (inside `FreezeFrame`/`Loop`) is locked: "Timing inside a loop or freeze frame can't be dragged".
- The edit table, verbatim: `<Sequence from durationInFrames>` — start edge `from += d`, `durationInFrames −= d`; end edge `durationInFrames += d`; body `from += d`. `<Series.Sequence durationInFrames offset>` — start edge `offset += d`, `durationInFrames −= d`; end edge `durationInFrames += d`; body `offset += d`.
- A missing `durationInFrames` defaults to the window's current length (dragging the end inserts it); a missing `from`/`offset` is 0. Values are whole frames; deltas are frames in the window's parent timeline.
- Nothing is written during a drag: the bands and a label (`from 30 → 45`) follow the pointer and the preview shows the playhead frame; mouse-up writes one history entry. Escape cancels. A press that moves less than 3 px is a click and seeks, as today.
- Snapping: edges snap to the playhead, chapter edges, and the other bands' edges within 6 px; Shift turns snapping off.
- Scene drags write the `defineScenes` entry's `durationInFrames` (literal or const in an array literal of object literals). Zero or several matches lock: "Can't find scene `intro` in the source". When the composition's length doesn't follow the scene list, the drag is clamped so no chapter starts at or after the composition's end, with "The composition is N frames long" beside the edge.
- "Set timing" wraps the element in `<Sequence from={…} durationInFrames={…}>`, indented like the element, the element re-indented one level, nothing else re-formatted; `Sequence` is added to the file's `@celesta/react` import or a new import after the last import; only a JSX child can be wrapped ("Wrap the element in a parent first"); tooltip "Animations inside start at the new start".
- Design 3's scope applies to a window written inside a shared component ("This scene" lifts `from`/`durationInFrames`/`offset` like any number).
- UI copy English, inspector style; cursors `ResizeLeftRight` on edges, `OperationNotAllowed` with the reason tooltip on locked ones.
- Tests call pure functions only (no GPUI test context): view paths name the pure helper that carries their rule; multi-file writes are tested through pure `*_files(texts, …)` functions and `write_all_recorded`/`EditHistory` on temp files, as `tests/element_edit.rs` does.
- TDD per task; red/green runs only the new tests (`cargo test -p <crate> -- <names>`); each crate's suite once per task; Vitest in TypeScript (`.mts`, imports `../src`); `cargo clippy -p celesta-source-edit -p celesta-editor -p celesta-react-bridge --all-targets` clean (pre-existing `large_enum_variant` on `SourceLookup` excepted); `cargo fmt --check`.
- No `git commit --amend`; commit and test commands are separate calls; stage paths explicitly.

## Rulings (decided here; the spec leaves them open)

- **Wrapper positions in the occurrence index.** `occurrenceKeys` drops `'wrapper'` entries from the chain (so `Sequence` and `Series.Sequence` positions are in no instance's chain); it now also returns them as `wrappers`, and each instance keeps the union over frames (the instance key stays the chain). Reason: the spec has the bands of every instance a drag moves follow it, and only the runtime knows which instances render through a wrapper.
- **What moves during a band drag.** Every instance of the selected element whose `wrappers` hold the dragged window's source moves (the copies a `.map()` makes are one instance); instances through a later item of the same `<Series>` shift by `d` for the end edge and the body, 0 for the start edge (offset and duration cancel). A moved range follows the grip: body `+d`; start edge `+d`, cut at the window's end; end edge cut at the new end. A range ending at the old end keeps reaching the window's end for either grip and either sign of `d` (start edge `−15` on window 30–90 gives 15–90, not 15–75): the band must not shrink from the right while its start is dragged left. Other elements' bands are not drawn, as today.
- **Grips sit on the window, not the band.** Grips are drawn at `windows[0].from`/`to`, over the band that holds the report's lookup frame (`Resolved.timeline_frame`); other bands don't drag. Reason: bands are occurrence ranges cut at the scene and merged per pixel, and the window describes the lookup frame only. The spec's "the band that contains the playhead (or the first one)" is amended to this. The grips are drawn from `windows[0]` (moved by a running drag), and only when one of the instance's filled bands in the current scene holds the lookup frame.
- **Window kind from the import.** `timing_kind` resolves the tag through the file's `@celesta/react` import: `Sequence` (or its alias, or `C.Sequence` under `import * as C`) and `Series.Sequence` (likewise through `Series`'s alias or `C.Series.Sequence`). It runs in the background job beside `sync_element_targets`, never in render. Reason: the runtime reports a `Series` item as `kind: 'Sequence'`, `ElementRef` carries no name, and text matching fails on aliases and namespaces.
- **A window is resolved as an element of its own.** `window_report` takes the report's chain from the wrapper entry written at the window's source outward; `resolve_rows` resolves its timing props in `RowRead::Unchecked` mode (`component_props: true`, `received: None`, `render_count: 1`, values not compared with a received one). Reason: the runtime reports no value for a wrapper's props, and the checked read locks every literal without one as `OverriddenByParent`. Pass-through, the call site, `enclosing`, `defaults` and `lift_lock` then come from `resolve_rows` as for any element.
- **Timing writes go through design 3's `commit_scoped`**, unchanged: `window_files` passes the window's `ElementTargets` (`PropKey::Prop("from" | "durationInFrames" | "offset")`, `component` = the `ComponentTargets` of the call of the component enclosing the wrapper, `enclosing` = that component) and a `resolve_again` closure calling `resolve_window` on the texts being written. Before it, `commit_gate(&scope_state(report, &window.targets, …), choices.get(&element), &window.targets, &keys)` decides the scope; `S` is the window's component's call, found in the selected instance's chain (an outer component's call is in it too). `calls` is the element's `SceneCalls` when the window's component is the element's, else the scene-calls job also runs for the window. The drag label's second line is `scope_in_effect`. No design 3 signature changes.
- **Grip locks and bounds.** `grip_lock`: the start edge needs both its props editable, the end edge `durationInFrames`, the body `from`/`offset` (`from={cut - 9}` locks start and body, not the end). Deltas keep `durationInFrames ≥ 1` on both edges and a `Series` item's start (`windows[0].from` minus the enclosing window's `from`, 0 at the root) `≥ 0`; `Series` throws otherwise.
- **Drag deltas are unclamped frames**: `(x − start_x) / pixels_per_frame` with `pixels_per_frame = lane_width · zoom / end_frame` (as `scrub_timeline_pixels`), not `timeline_fraction_at`, which clamps to the lane. Reason: a drag must be able to extend the last scene or a band past the lane's edge.
- **Grip hit areas.** Edges are 6 px inside the band, the rest is the body; a band narrower than 12 px splits into a start half and an end half with no body (the minimum drawn width is 2 px).
- **Snapping.** A body drag snaps whichever of its two edges is nearer a target; the dragged window's own edges (also a playhead or chapter edge sitting on them) and the bands that move with the drag are not targets.
- **Scene lists.** `scene_lists` returns each `defineScenes([...])` call whose argument is an array literal of object literals with string-literal `id`s, entries in list order; a list holding anything else (a spread, `CHAPTERS.map(…)`) is skipped. A drag moves only later entries of that list (chapters are sorted by `from` and may mix sources or overlap through offsets).
- **Scene bounds.** The written value is the chapter's visible length plus `d` (a chapter cut at the composition's end shows less than its entry, so entry plus `d` would be an unseen change). `d ≥ −visible`; every later entry's `from + d` stays at or after the list's first chapter's `from` (a later negative offset must not start before 0; the first entry's own offset is taken as 0, a bound at least as strict). When the composition doesn't follow: no later chapter starts at or after its end, and the dragged end doesn't pass it.
- **Composition follows the scenes** when `<Composition durationInFrames={X.durationInFrames}>` in the React entry file names the binding of the `defineScenes` call holding the entry: `X` a const of the same file, or a named import (alias included) whose module resolves (`.ts`, `.tsx`, `/index.ts(x)`) to the list's file and whose imported name is the list's binding. Anything else clamps. Reason: `examples/feature-tour` declares `SCENES` in `timeline.tsx` and uses it in `film.tsx`.
- **Files parse by their extension.** The scene search and the checked write of a scene list parse with `SourceType::from_path` (TSX when unknown): `examples/reel/timeline.ts` and `examples/delta/v0.8.0/timeline.ts` are `.ts`, where `<T>(x) => x` and `<number>v` are not TSX. `apply_checked` takes the path it writes.
- **The scene search** walks the React entry's folder with `newest_source_mtime`'s skips (`node_modules`, `dist`, `.tmp`, `.git`, depth 8), parses only files containing the text `defineScenes`, and runs on the background executor on every reload (generation-guarded), not behind `refresh_occurrences`, which is off with Select layers off.
- **"Set timing" values.** Nearest window the scene's (`windows[0]`): `from = playhead − windows[0].from`, `durationInFrames = windows[0].to − playhead` (the window clipped to the composition's end, both rounded). No window: the element is on the composition's own timeline, so `from = playhead` and `durationInFrames` runs to the end of the outermost chapter at the playhead, else the composition's end. A remapped or own window: not offered.
- **"Set timing" on a shared element** writes the component every scene uses, so it follows design 3's choice: enabled when the element is not shared or "Shared value" is chosen; under "This scene" disabled with "Can't be set per scene yet" (`ScopeLock::PerSceneUnsupported`); an index that can't answer disables it with its note.
- **Import editing is one shared piece** (`imports.rs`, design 5 adds several names at once): names go into an existing value import of the module (sorted lists, compared ignoring case, stay sorted; otherwise appended), as one edit; else one new import line after the last import with the new names sorted, its quotes and semicolon following the file's last import. `import type { … }` is never extended; a name bound by a type-only import, or by anything other than an import from the module, locks (`Clash`). An alias writes the alias; `import * as C` writes `C.Sequence` and needs no edit.
- **Wrap formatting.** Lines that start inside a template literal or string are not re-indented. An element that shares its line with other code (`<G><Rect /></G>`) is wrapped inline, with no line breaks added.

## Review Focus

- **A `Series` of three items, the middle one's end dragged**: the third item's bands (when it is the selected element's) move by the same delta during the drag; one write changes only the middle item's `durationInFrames`. Tests in Task 5 (`a_series_drag_moves_the_following_items`) and Task 4 (`window_files_changes_only_the_dragged_series_item`).
- **A scene drag in a literal `defineScenes` with a literal composition length**: the clamp stops the last chapter from starting at or after the end, with "The composition is N frames long"; with `feature-tour`'s two-file `SCENES` it doesn't clamp. Tests in Task 6 (`the_clamp_keeps_chapters_inside_a_literal_length`, `composition_follows_an_imported_scene_list`).
- **"Set timing" in a file indented with tabs, with `Sequence` already imported under an alias (`import { Sequence as Seq }`)**: the wrapper writes `<Seq …>`, tabs kept, the import untouched. Test in Task 7 (`set_timing_files_uses_the_alias_and_keeps_tabs`).
- **A drag at a zoom where one frame is under a pixel**: deltas stay whole frames, snapping still within 6 px, and the drag goes past the lane's edge. Test in Task 5 (`drag_frames_is_unclamped_at_every_zoom`).
- **A window inside a shared component with "This scene"**: the drag lifts `from`/`durationInFrames` (`sequenceFrom`) and writes the scene's call site, one history entry; undo restores both files. Test in Task 4 (`window_files_lifts_under_this_scene`).

---

### Task 1: Wrapper positions and window classification

**Files:**
- Modify: `packages/react/src/source-chain.ts` (`occurrenceKeys` returns `wrappers`), `packages/react/src/render.ts` (`OccurrenceIndex` instances carry `wrappers`, unioned over frames)
- Modify: `crates/react-bridge/src/occurrences.rs` (`InstanceOccurrences.wrappers`, `merge` unions them)
- Create: `crates/editor/src/timing.rs`; Modify: `crates/editor/src/main.rs`
- Test: `packages/react/test/occurrences.test.mts`, `crates/react-bridge/src/tests.rs`, `crates/editor/src/timing.rs` `#[cfg(test)]`

**Interfaces:**
- Produces:

```ts
// source-chain.ts
export function occurrenceKeys(layerNodes: ReadonlyMap<string, HostNode>, resolveFile: (fileName: string) => string): Array<{ element: SourcePos; chain: SourcePos[]; wrappers: SourcePos[]; place: string }>;
// render.ts — OccurrenceIndex instance
{ chain: SourcePos[]; wrappers: SourcePos[] /* the 'wrapper' entries' positions, innermost first, unioned over frames */; ranges: Array<[number, number]>; maxPerFrame: number }
```

```rust
// occurrences.rs
pub struct InstanceOccurrences { /* existing */ #[serde(default)] pub wrappers: Vec<SourcePos> }
// timing.rs
pub(crate) enum WindowRole<'a> { Own(&'a TimingWindow), Scene(&'a TimingWindow), Remapped, None /* no windows */ }
/// The role of `report.windows[0]` at the lookup `frame`.
pub(crate) fn window_role<'a>(report: &'a LayerSourceReport, chapters: &[ReactChapter], composition_end: u64, frame: u64) -> WindowRole<'a>;
/// `from` and `durationInFrames` for "Set timing" at `playhead`; `None` for `Own` and `Remapped`.
pub(crate) fn set_timing_span(role: &WindowRole, chapters: &[ReactChapter], composition_end: u64, playhead: u64) -> Option<(i64, i64)>;
```

- [ ] **Step 1: Write the failing runtime tests** — Vitest `occurrence instances list the wrappers they render through` (a `<Rect>` in a `<Sequence>`: `wrappers` holds the `Sequence`'s position; a `<Rect>` in the second `<Series.Sequence>` holds that item's position; the chain is unchanged); Rust `instance_wrappers_default_and_merge` (JSON without `wrappers` → empty; merging two chunks unions them, no duplicates).
- [ ] **Step 2: Run them, expect FAIL** — `pnpm --filter @celesta/react test -- occurrences`; `cargo test -p celesta-react-bridge -- instance_wrappers`
- [ ] **Step 3: Implement**; run them, expect PASS; then `pnpm --filter @celesta/react test` and `cargo test -p celesta-react-bridge`.
- [ ] **Step 4: Commit** — `feat(react): list the wrappers each occurrence instance renders through`
- [ ] **Step 5: Write the failing classification tests** (`timing.rs`):
  - `no_windows_is_none`; `a_remapped_window_is_locked` (`Remapped`, with or without a source).
  - `a_window_without_source_is_the_scene_s`.
  - `a_window_spanning_a_chapter_is_the_scene_s` — a window 300–420 with a chapter 300–420; with a part 330–390 holding the frame and a window 330–390; the last chapter cut at the composition's end (window 900–1020, chapter 900–960, composition 960) still matches; a window 299.6–420.2 matches after rounding.
  - `a_hand_placed_sequence_inside_a_chapter_is_own` — window 330–360 in chapter 300–420.
  - `set_timing_span_runs_from_the_playhead_to_the_window_end` — `Scene` window 300–420, playhead 345 → `(45, 75)`.
  - `set_timing_span_without_a_window_is_on_the_composition_s_timeline` — playhead 345, outermost chapter 300–420 → `(345, 75)`; no chapter, composition 600 → `(345, 255)`; `Own` and `Remapped` → `None`.
- [ ] **Step 6: Run them, expect FAIL** — `cargo test -p celesta-editor -- timing::`
- [ ] **Step 7: Implement**; run them, expect PASS; then `cargo test -p celesta-editor`.
- [ ] **Step 8: Commit** — `feat(editor): tell an element's own timing window from its scene's`

### Task 2: Timing kind, scene lists and the composition's length (`crates/source-edit`)

**Files:**
- Create: `crates/source-edit/src/timing.rs`, `crates/source-edit/src/scenes.rs`
- Modify: `crates/source-edit/src/lib.rs` (`parses_file`, a path-aware parse), `crates/source-edit/src/resolve.rs` and `inputs.rs` (the value-at-a-span entry), `crates/source-edit/src/pass_through.rs` (`resolve_unchecked`)
- Test: `crates/source-edit/tests/{timing,scenes}.rs`; `packages/react/test/chapters.test.mts`, `packages/react/test/layer-source.test.mts`

**Interfaces:**
- Produces:

```rust
// timing.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimingKind { Sequence, SeriesSequence }
/// The timing wrapper `element` is, through the file's `@celesta/react` import (aliases and `import * as C` included); `None` for anything else.
pub fn timing_kind(text: &str, element: &ElementRef) -> Option<TimingKind>;
/// The `<Series.Sequence>` siblings after `element` in its `<Series>`, as 1-based line and UTF-16 column (what `element_at` takes).
pub fn later_series_items(text: &str, element: &ElementRef) -> Vec<(u32, u32)>;
// pass_through.rs
/// `prop` of `element` by `resolve_through`'s rules, not compared with a received value (the runtime reports none for a wrapper's props): a literal is `Literal`, a parameter resolves at `call_site` unchecked, an absent prop is `Missing { default: None }`.
pub fn resolve_unchecked(text: &str, element: &ElementRef, prop: &str, call_site: Option<(&str, &ElementRef)>) -> EditTarget;
// scenes.rs
pub struct SceneList { pub binding: Option<String> /* `const X = defineScenes(…)` */, pub entries: Vec<SceneItem> /* list order */ }
pub struct SceneItem { pub id: String, pub duration: EditTarget, pub inputs: Vec<(String, EditTarget)> /* when `duration` is an expression */ }
/// Every `defineScenes([...])` call in `text` (parsed as `path`'s type) whose argument is an array literal of object literals with string-literal ids.
pub fn scene_lists(text: &str, path: &Path) -> Vec<SceneList>;
pub enum CompositionLength { ListDuration { module: Option<String> /* import specifier; None: this file */, name: String /* the binding's name in that module */ }, Other }
/// How `<Composition durationInFrames>` is written in `text`; `None` without a `<Composition>`.
pub fn composition_length(text: &str, path: &Path) -> Option<CompositionLength>;
// lib.rs
/// Whether `text` parses without error as `path`'s source type (TSX when the extension says nothing).
pub fn parses_file(text: &str, path: &Path) -> bool;
```

- [ ] **Step 1: Write the failing tests:**
  - `timing_kind_reads_the_import` — `<Sequence>`, `<Seq>` with `Sequence as Seq`, `<C.Sequence>` with `import * as C` → `Sequence`; `<Series.Sequence>`, `<S.Sequence>` with `Series as S`, `<C.Series.Sequence>` → `SeriesSequence`; a local `function Sequence` → `None`.
  - `later_series_items_are_the_following_siblings` — three items, the first → the second's and third's positions; the last → empty.
  - `resolve_unchecked_reads_written_props` — `from={30}` → `Literal` 30 (not `OverriddenByParent`); `const START = 30; from={START}` → the const's literal; `from={start}` with `start={45}` at the call site → `PassThrough` to 45; no `durationInFrames` → `Missing { default: None }`; `from={cut - 9}` → `Expression`.
  - `finds_scene_lists_with_ids_in_order` — a `.ts` file also holding `const id = <T>(x: T) => x;` and `<number>v`: one list, binding `SCENES`, ids `["intro", "main", "outro"]`, `intro`'s duration `Literal` 90.
  - `a_list_with_a_spread_is_skipped` — `[{ id: 'open', … }, ...CHAPTERS.map(…)]` → no list.
  - `a_const_duration_resolves_to_the_const`; `an_expression_duration_is_locked_with_its_inputs` (`BAR * 3` → `Expression`, inputs `[("BAR", Literal 30)]`).
  - `composition_length_reads_the_list_reference` — `durationInFrames={SCENES.durationInFrames}` with a same-file const → `ListDuration { module: None, name: "SCENES" }`; with `import { SCENES } from './timeline'` → `module: Some("./timeline")`; with `import { SCENES as S }` and `S.durationInFrames` → `name: "SCENES"`; `durationInFrames={300}` → `Other`; no `<Composition>` → `None`.
  - `parses_file_by_extension` — `<number>v` parses as `.ts`, not as `.tsx`.
  - Vitest: `chapter ids line up with defineScenes entries`; `series windows report the item position` (keep or add the assertion).
- [ ] **Step 2: Run them, expect FAIL** — `cargo test -p celesta-source-edit -- timing_kind later_series resolve_unchecked scene_lists a_list_with a_const_duration an_expression_duration composition_length parses_file`; `pnpm --filter @celesta/react test -- chapters layer-source`
- [ ] **Step 3: Implement.** The scene search is a `Visit` over call expressions (callee identifier `defineScenes`). A `durationInFrames` value resolves through the element-free entry of the existing rules, `resolve.rs`'s `pub(crate) fn resolve_value_at(text, value: Span) -> EditTarget` and `inputs.rs`'s `pub(crate) fn inputs_at(text, value: Span) -> Vec<(String, EditTarget)>` (a `Gather` anchored at the value instead of an element's attribute). `parse_checked` gains the source type; `parse`/`parses` keep TSX.
- [ ] **Step 4: Run them, expect PASS**; then `cargo test -p celesta-source-edit` and `pnpm --filter @celesta/react test`.
- [ ] **Step 5: Commit** — `feat(source-edit): read timing wrappers, scene lists and the composition length`

### Task 3: JSX wrapping and import editing (`crates/source-edit`)

**Files:**
- Create: `crates/source-edit/src/{insert.rs,imports.rs}`
- Modify: `crates/source-edit/src/lib.rs`
- Test: `crates/source-edit/tests/{insert,imports}.rs`

**Interfaces:**
- Consumes: design 3 Task 1's `Edit::Splice { start, end, text }`.
- Produces (design 5 reuses all of it):

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportName { Named(String) /* the local name to write: `Sequence`, or `Seq` for `Sequence as Seq` */, Namespace(String) /* the member to write: `C.Sequence` */ }
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportLock { Clash { name: String } }
/// The one edit (if any) that makes every name of `names` from `module` usable in `text`, and how to write each, in `names` order.
pub fn ensure_imports(text: &str, module: &str, names: &[&str]) -> Result<(Option<Edit>, Vec<ImportName>), ImportLock>;
/// `ensure_imports` for one name.
pub fn ensure_import(text: &str, module: &str, name: &str) -> Result<(Option<Edit>, ImportName), ImportLock>;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WrapLock { NotAChild }
/// Wraps the element at `element` in `<{tag} {attrs}>…</{tag}>`, indented like it, re-indenting it one level.
pub fn wrap_element(text: &str, element: &ElementRef, tag: &str, attrs: &[(&str, EditValue)]) -> Result<Vec<Edit>, WrapLock>;
/// The file's indent unit near `at` (a tab, or the leading spaces of one level).
pub fn indent_unit(text: &str, at: u32) -> String;
```

- [ ] **Step 1: Write the failing tests** (asserting the text after `apply`, which must parse):
  - `wraps_a_child_element_with_spaces` — `<G>\n    <Rect width={10} />\n  </G>` with `from 45`, `durationInFrames 75` → `<Sequence from={45} durationInFrames={75}>` at the element's indent, `<Rect …>` one level deeper, `</Sequence>` at the element's indent; the rest of the file unchanged.
  - `wraps_with_tabs`; `wraps_a_multi_line_element_re_indenting_every_line`; `re_indenting_skips_lines_inside_a_template_literal`.
  - `an_element_sharing_its_line_is_wrapped_inline` — `<G><Rect /></G>` → `<G><Sequence from={45} durationInFrames={75}><Rect /></Sequence></G>`.
  - `refuses_a_root_or_a_prop_value` — `return <Rect />`, `() => <Rect />`, `icon={<Rect />}` → `NotAChild`.
  - `indent_unit_reads_tabs_or_one_level_of_spaces` — 2 and 4 spaces, a tab.
  - `adds_to_an_existing_named_import_keeping_it_sorted` — `{ Rect, Text }` + `Sequence` → `{ Rect, Sequence, Text }`.
  - `adds_two_new_names_to_a_sorted_import` — `{ Rect, Text }` + `["Sequence", "Circle"]` → one edit, `{ Circle, Rect, Sequence, Text }`, names `[Named("Sequence"), Named("Circle")]`.
  - `adds_to_an_unsorted_import_at_the_end` — `{ Text, Rect }` + `Sequence` → `{ Text, Rect, Sequence }`.
  - `adds_a_new_import_after_the_last` — no `@celesta/react` import: `import { Sequence } from "@celesta/react";` after the last import, quotes and semicolon as that import's.
  - `adds_two_new_names_on_one_new_line` — `["Sequence", "Circle"]` → `import { Circle, Sequence } from '@celesta/react';`.
  - `already_imported_needs_nothing` (`None` edit); `an_aliased_import_writes_the_alias` (`Named("Seq")`, no edit); `a_namespace_import_writes_a_member` (`Namespace("C.Sequence")`, no edit).
  - `a_type_only_import_is_never_extended` — only `import type { SceneEntry } from '@celesta/react'` → a new line; `import type { Sequence }` or `import { type Sequence }` → `Clash`.
  - `a_clashing_local_binding_locks` — `const Sequence = …` or `import { Sequence } from './mine'` → `Clash { name: "Sequence" }`; with two names one clashing → `Err`, no edit.
- [ ] **Step 2: Run them, expect FAIL** — `cargo test -p celesta-source-edit -- wraps re_indenting an_element_sharing refuses_a_root indent_unit adds_ already_imported an_aliased a_namespace a_type_only a_clashing`
- [ ] **Step 3: Implement** with `Edit::Splice`. Several names into an existing list are one `Splice` from the first insertion point to the last, the text between kept verbatim.
- [ ] **Step 4: Run them, expect PASS**; then `cargo test -p celesta-source-edit`.
- [ ] **Step 5: Commit** — `feat(source-edit): wrap a JSX element and edit imports`

### Task 4: Window targets and their commit

**Files:**
- Modify: `crates/editor/src/element_edit/resolve.rs` (`RowRead`, `window_report`, `resolve_window`), `crates/editor/src/element_edit/commit.rs` (`window_files`), `crates/editor/src/element_edit.rs` (`WindowTargets`; the window job beside `sync_element_targets`: role, `resolve_window`, the scene calls of the window's component when it is not the element's)
- Test: `crates/editor/src/tests/timeline_timing.rs` (declared in `tests.rs`)

**Interfaces:**
- Consumes: Task 1 (`window_role`), Task 2 (`timing_kind`, `later_series_items`, `resolve_unchecked`); design 3 Tasks 2–3 (`ElementTargets.enclosing`/`defaults`/`lift_lock`, `commit_scoped`, `SceneCalls`, `Scope`).
- Produces:

```rust
// element_edit/resolve.rs
pub(super) enum RowRead { Received /* today's */, Unchecked /* `resolve_unchecked`, ctx `component_props: true`, `received: None`, `render_count: 1` */ }
pub(super) fn resolve_rows(read: impl Fn(&Path) -> Result<String, String>, report: &LayerSourceReport, table: Option<&PrimitivePropertyTable>, rows: impl FnOnce(&str, &ElementRef) -> Vec<String>, how: RowRead) -> Result<ElementTargets, String>;
/// `report` as the report of the wrapper written at `source`: its chain from that wrapper entry outward, no received props, one render. `None` when no entry is there.
pub(crate) fn window_report(report: &LayerSourceReport, source: &SourcePos) -> Option<LayerSourceReport>;
pub(crate) struct WindowTargets { pub(crate) source: SourcePos, pub(crate) kind: TimingKind, pub(crate) targets: ElementTargets, pub(crate) followers: Vec<SourcePos> /* later `Series` items */ }
/// The wrapper at `source` with its timing props as rows (`from`, `durationInFrames`; or `durationInFrames`, `offset`).
pub(crate) fn resolve_window(read: impl Fn(&Path) -> Result<String, String>, report: &LayerSourceReport, source: &SourcePos) -> Result<WindowTargets, String>;
// element_edit/commit.rs
/// `commit_scoped` for a window, re-resolving with `resolve_window`.
pub(crate) fn window_files(texts: &Texts, window: &WindowTargets, report: &LayerSourceReport, scope: Option<Scope>, calls: Option<&SceneCalls>, values: &[(PropKey, Option<EditValue>)]) -> Result<Vec<(PathBuf, String)>, CommitError>;
```

- [ ] **Step 1: Write the failing tests** (fixtures on temp files; history through `write_all_recorded` and `EditHistory`):
  - `window_report_starts_at_the_wrapper_entry` — chain `[Rect, Sequence (wrapper), Intro]` and the `Sequence`'s position → chain `[Sequence, Intro]`, props empty, `render_count` 1; a position not in the chain → `None`.
  - `resolve_window_reads_the_timing_props` — `<Sequence from={30}>` → `from` `Literal` 30, `durationInFrames` `Missing`; `<Series.Sequence durationInFrames={60}>` → rows `durationInFrames`, `offset`, `followers` the later items; a `<FreezeFrame>` → `Err`.
  - `window_files_writes_a_literal` — `from={30}` → `from={45}`, nothing else changed.
  - `window_files_inserts_a_missing_duration` — `durationInFrames={75}` inserted.
  - `window_files_writes_a_pass_through_at_the_call_site` — `from={start}` in `Fade`, `<Fade start={30}>` in the scene → only the scene's file changes.
  - `window_files_changes_only_the_dragged_series_item` — three items; the middle `durationInFrames={60}` → `{75}`; the others unchanged.
  - `window_files_lifts_under_this_scene` — `<Sequence from={30}>` inside `PriceTag`, used by two scenes, `Scope::ThisScene`: `from={sequenceFrom}`, `{ label, sequenceFrom = 30 }`, `sequenceFrom={45}` at the scene's call; one `HistoryEntry` with two files; `undo` restores both.
  - `window_files_refuses_a_changed_source` — the `Sequence` moved a line → `SourceChanged`, nothing written.
- [ ] **Step 2: Run them, expect FAIL** — `cargo test -p celesta-editor -- tests::timeline_timing::`
- [ ] **Step 3: Implement.** Existing `resolve_rows` callers pass `RowRead::Received`. The job runs where `sync_element_targets` resolves the selected element and stores `window_targets: Option<WindowTargets>` for an `Own` role; mouse-up (Task 5) commits inside `record_own_writes` after `commit_gate`, as the Rulings state.
- [ ] **Step 4: Run them, expect PASS**; then `cargo test -p celesta-editor`.
- [ ] **Step 5: Commit** — `feat(editor): resolve and commit an element's timing window`

### Task 5: Dragging a band

**Files:**
- Create: `crates/editor/src/timeline_drag.rs` (an `impl EditorView` like `transform_drag.rs`, plus the pure helpers)
- Modify: `crates/editor/src/timeline.rs` (`LaneView` and helpers `pub(crate)`; grips over the band holding the lookup frame; the label drawn outside the lane's `overflow_hidden`), `crates/editor/src/impact.rs` (bands from previewed occurrences while a drag runs), `crates/editor/src/playback.rs` / `render.rs` (`drag_move`/`drag_end`/Escape route the new drag)
- Test: `crates/editor/src/timeline_drag.rs` `#[cfg(test)]`

**Interfaces:**
- Consumes: Tasks 1, 2 (`TimingKind`) and 4 (`WindowTargets`, `window_files`); design 3 Task 3 (`commit_gate`) and Task 4 (`scope_in_effect`).
- Produces:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BandGrip { Start, End, Body }
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TimingValues { pub(crate) start: f64 /* `from` or `offset` */, pub(crate) duration: f64 }
pub(crate) struct DragPreview { pub(crate) source: SourcePos, pub(crate) window: (f64, f64), pub(crate) grip: BandGrip, pub(crate) delta: i64, pub(crate) followers: Vec<SourcePos> }
pub(crate) enum DragEnd { Seek, Cancel, Commit }
/// The band (index into `bands`, frames) a drag starts on: the one holding `frame`.
pub(crate) fn draggable_band(bands: &[(f64, f64)], frame: f64) -> Option<usize>;
pub(crate) fn grip_at(x: f32, left: f32, width: f32) -> BandGrip;
/// Why `grip` can't drag (design 2's reason of the first locked prop it needs).
pub(crate) fn grip_lock(kind: TimingKind, grip: BandGrip, targets: &ElementTargets) -> Option<String>;
/// The current values, a missing start 0 and a missing duration `window`'s length.
pub(crate) fn timing_values(kind: TimingKind, targets: &ElementTargets, window: (f64, f64)) -> TimingValues;
/// `delta` within the bounds: duration ≥ 1, and a `Series` item's start (`series_start`) ≥ 0.
pub(crate) fn clamp_band_delta(grip: BandGrip, delta: i64, values: TimingValues, series_start: Option<f64>) -> i64;
/// The values to commit for `grip` and `delta` (the edit table).
pub(crate) fn band_edit(kind: TimingKind, grip: BandGrip, delta: i64, values: TimingValues) -> Vec<(PropKey, Option<EditValue>)>;
/// Frames between `start_x` and `x`, not clamped to the lane.
pub(crate) fn drag_frames(start_x: f32, x: f32, pixels_per_frame: f64) -> f64;
/// Playhead, chapter edges and the other bands' edges; the dragged band's own edges left out.
pub(crate) fn snap_targets(playhead: f64, chapters: &[ReactChapter], bands: &[(f64, f64)], dragged: usize) -> Vec<f64>;
/// `raw` frames rounded, after snapping the nearest of `edges` (moved by `raw`) to `targets` within 6 px; no snapping with `shift`.
pub(crate) fn snapped_delta(edges: &[f64], raw: f64, targets: &[f64], pixels_per_frame: f64, shift: bool) -> i64;
/// `element` with every instance through `preview.source` moved by its grip, and those through a follower shifted (see Rulings).
pub(crate) fn preview_occurrences(element: &ElementOccurrences, preview: &DragPreview) -> ElementOccurrences;
/// `from 30 → 45`; both props for the start edge (`from 30 → 45 · durationInFrames 60 → 45`).
pub(crate) fn drag_label(kind: TimingKind, grip: BandGrip, before: TimingValues, after: TimingValues) -> String;
pub(crate) fn drag_end(moved_px: f32, delta: i64, escaped: bool) -> DragEnd;
```

- [ ] **Step 1: Write the failing tests:**
  - `draggable_band_holds_the_lookup_frame` — bands 0–60, 120–180, frame 130 → 1; frame 90 → `None`.
  - `grip_at_prefers_edges_and_splits_narrow_bands` — a 100 px band: x 3 → `Start`, 97 → `End`, 50 → `Body`; a 10 px band: 4 → `Start`, 6 → `End`.
  - `grip_lock_names_the_locked_prop` — `from={cut - 9}`: `Start` and `Body` locked with its expression text, `End` free; `durationInFrames={len}` unresolved: `Start` and `End` locked.
  - `timing_values_default_missing_props` — no `from`, no `durationInFrames`, window 30–90 → `{ start: 0, duration: 60 }`.
  - `band_edit_follows_the_table` — every kind × grip with `d = 15` on `{ start: 30, duration: 60 }`: `Sequence` start → `from 45`, `durationInFrames 45`; end → `durationInFrames 75`; body → `from 45`; `SeriesSequence` likewise on `offset`.
  - `band_delta_keeps_duration_and_series_start` — start edge `d = 70` on duration 60 → 59; end edge `d = −70` → −59; a `Series` item starting at 20 with body `d = −30` → −20.
  - `drag_frames_is_unclamped_at_every_zoom` — 0.25 px/frame: 10 px → 40 frames; 3 px/frame: 10 px → 3.33; an `x` past the lane's right edge still grows the delta.
  - `snapping_takes_the_nearest_edge_within_six_pixels` — 2 px/frame, a target 2 frames from the end edge → snapped; 4 frames away → raw; a body drag snaps its start edge when that one is nearer; the dragged band's edges are not in `snap_targets`.
  - `shift_turns_snapping_off`; `deltas_are_whole_frames_at_every_zoom` (0.25, 1, 7.5 px/frame).
  - `preview_moves_the_instances_through_the_window` — instances through the source: body `+15` moves every range; start `+15` on window 30–90 clips 30–90 to 45–90; end `−15` clips to 30–75; end `+15` extends a range ending at 90 to 105; an instance without the source stays.
  - `a_series_drag_moves_the_following_items` — three items, the middle one's end `+15`: the third item's instance moves `+15`, the first stays; start edge: followers stay.
  - `drag_label_names_the_changed_props` — `from 30 → 45`; `offset 0 → 15`; start edge: both props.
  - `drag_end_tells_a_click_from_a_drag` — 2 px → `Seek`; Escape → `Cancel`; delta 0 → `Cancel`; otherwise `Commit`.
- [ ] **Step 2: Run them, expect FAIL** — `cargo test -p celesta-editor -- timeline_drag::`
- [ ] **Step 3: Implement.** Grips at `windows[0]`'s edges; a locked grip shows `OperationNotAllowed` and its `grip_lock` text; the label's second line is `scope_in_effect`; mouse-up asks `commit_gate` (`Hold`/`Refuse` → the drag's notice) and writes with `window_files` inside `record_own_writes`.
- [ ] **Step 4: Run them, expect PASS**; then `cargo test -p celesta-editor`.
- [ ] **Step 5: Commit** — `feat(editor): drag an element's timing on the timeline`

### Task 6: Dragging a scene's end

**Files:**
- Create: `crates/editor/src/scene_entries.rs` (the search, edges, clamp, write)
- Modify: `crates/editor/src/source.rs` (`newest_source_mtime`'s walk shared as `source_files`), `crates/editor/src/refresh.rs` (start the search where `react_reload_generation` is bumped), `crates/editor/src/element_edit/commit.rs` (`apply_checked` takes the path; callers pass the file they write), `crates/editor/src/timeline_drag.rs` (chapter-edge drag), `crates/editor/src/timeline.rs` (`chapter_row` end-edge hit area)
- Test: `crates/editor/src/scene_entries.rs` `#[cfg(test)]`

**Interfaces:**
- Consumes: Task 2 (`scene_lists`, `composition_length`, `parses_file`), Task 5 (`drag_frames`, `snapped_delta`, `drag_end`).
- Produces:

```rust
pub(crate) fn apply_checked(path: &Path, text: &str, edits: &[Edit]) -> Result<String, CommitError>;
pub(crate) struct SceneSources { pub(crate) generation: u64, pub(crate) lists: Vec<(PathBuf, SceneList)>, pub(crate) length: Option<CompositionLength>, pub(crate) entry: PathBuf }
/// Reads every `.ts`/`.tsx` file under `dir` that contains `defineScenes` (`source_files`' skips), and the entry's composition length.
pub(crate) fn find_scene_sources(dir: &Path, entry: &Path, generation: u64) -> SceneSources;
/// Whether the composition's length is the list's `durationInFrames` (same file, or an import resolving to it).
pub(crate) fn composition_follows(entry: &Path, length: Option<&CompositionLength>, list_file: &Path, binding: Option<&str>, exists: impl Fn(&Path) -> bool) -> bool;
pub(crate) enum SceneEdge { Editable { file: PathBuf, duration: EditTarget, later: Vec<String> /* ids after it in its list */, follows: bool }, Locked(String) }
pub(crate) fn scene_edges(sources: &SceneSources, chapters: &[ReactChapter], exists: impl Fn(&Path) -> bool) -> HashMap<String, SceneEdge>;
/// `delta` within the Rulings' bounds, and the note when the composition's end stopped it.
pub(crate) fn clamp_scene_delta(chapters: &[ReactChapter], id: &str, later: &[String], delta: i64, composition_end: u64, follows: bool) -> (i64, Option<String>);
/// The entry's new text: found again in `texts`, at the same place, `durationInFrames` set to `written`.
pub(crate) fn scene_files(texts: &Texts, id: &str, edge: &SceneEdge, written: f64) -> Result<Vec<(PathBuf, String)>, CommitError>;
```

- [ ] **Step 1: Write the failing tests:**
  - `an_entry_found_once_is_editable`; `zero_or_two_entries_lock_with_the_reason` ("Can't find scene `intro` in the source"); `an_expression_duration_locks_with_its_inputs` (design 2's text).
  - `composition_follows_an_imported_scene_list` — `film.tsx` with `import { SCENES } from './timeline'` and `timeline.tsx` binding `SCENES` → true; with `SCENES as S` → true; `./timeline` resolving to another file's list → false; a literal → false.
  - `the_clamp_keeps_chapters_inside_a_literal_length` — chapters intro 0–90, main 90–210, outro 210–300, composition 300, dragging intro `+100` → `+89` with "The composition is 300 frames long"; `follows` → `+100`, no note.
  - `the_clamp_keeps_lengths_and_starts_non_negative` — intro `−120` → `−90`; a later entry starting before the list's first chapter is never produced.
  - `only_later_entries_of_the_same_list_count` — a chapter of another list after it does not bound the clamp.
  - `a_clipped_chapter_writes_its_visible_length` — entry 200, visible 150, `d = −10` → written 140.
  - `scene_files_writes_the_entry` (`.ts` file, `durationInFrames: 90` → `100`); `scene_files_refuses_a_changed_source` (the entry moved → `SourceChanged`).
  - `the_search_skips_ignored_folders_and_files_without_define_scenes` — temp dir with `node_modules/x.ts`, `dist/y.ts`, a `.ts` without the text: none parsed.
- [ ] **Step 2: Run them, expect FAIL** — `cargo test -p celesta-editor -- scene_entries::`
- [ ] **Step 3: Implement.** `find_scene_sources` runs on `cx.background_executor()` per reload; a result for an older generation is dropped. The chapter drag reuses Task 5's helpers; the edge carries the not-allowed cursor and reason when `Locked`; the clamp note is drawn beside the edge; mouse-up writes `scene_files` inside `record_own_writes`.
- [ ] **Step 4: Run them, expect PASS**; then `cargo test -p celesta-editor`.
- [ ] **Step 5: Commit** — `feat(editor): drag a scene's end on the chapter row`

### Task 7: "Set timing"

**Files:**
- Modify: `crates/editor/src/timing.rs` (`set_timing_offer`, `set_timing_files`), `crates/editor/src/element_edit.rs` (the dry `wrap_element` in the background job → `wrap_lock`), `crates/editor/src/timeline.rs` (the action on the band), `crates/editor/src/timeline_drag.rs` or `element_edit.rs` (`set_timing`)
- Test: `crates/editor/src/tests/timeline_timing.rs`

**Interfaces:**
- Consumes: Tasks 1, 3 and 4; design 3 Task 2 (`ScopeState`, `chosen_scope`, `ScopeLock::PerSceneUnsupported`).
- Produces:

```rust
pub(crate) enum SetTimingOffer { Hidden, Disabled(String), Enabled { from: i64, duration: i64 } }
/// Hidden for an own or remapped window; disabled by `wrap_lock` ("Wrap the element in a parent first") or the scope (see Rulings).
pub(crate) fn set_timing_offer(role: &WindowRole, span: Option<(i64, i64)>, wrap_lock: Option<&WrapLock>, scope: &ScopeState, remembered: Option<Scope>) -> SetTimingOffer;
/// The element at `report`'s position found again in `texts`, wrapped and `Sequence` imported, in one checked text.
pub(crate) fn set_timing_files(texts: &Texts, report: &LayerSourceReport, expected: &ElementRef, from: i64, duration: i64) -> Result<Vec<(PathBuf, String)>, CommitError>;
pub(crate) fn set_timing(&mut self, cx: &mut Context<Self>);
```

- [ ] **Step 1: Write the failing tests:**
  - `set_timing_files_wraps_and_imports_in_one_entry` — `<Rect … />` in a scene, `(45, 75)`: the spec's `<Sequence from={45} durationInFrames={75}>` block, `Sequence` added to `{ Rect }`; one `HistoryEntry`; `undo` restores.
  - `set_timing_files_uses_the_alias_and_keeps_tabs` — tabs, `import { Rect, Sequence as Seq }` → `<Seq from={45} durationInFrames={75}>`, tabs kept, the import unchanged.
  - `set_timing_files_refuses_a_changed_source` → `SourceChanged`.
  - `set_timing_offer_follows_the_role` — `Own`/`Remapped` → `Hidden`; `Scene`/`None` → `Enabled` with the span; `WrapLock::NotAChild` → `Disabled("Wrap the element in a parent first")`.
  - `set_timing_offer_follows_the_scope` — `NotShared` → `Enabled`; `Options` with "Shared value" chosen → `Enabled`; with "This scene" → `Disabled("Can't be set per scene yet")`; `Unknown { note }` → `Disabled(note)`.
- [ ] **Step 2: Run them, expect FAIL** — `cargo test -p celesta-editor -- set_timing`
- [ ] **Step 3: Implement** (the action's tooltip "Animations inside start at the new start"; the write inside `record_own_writes`).
- [ ] **Step 4: Run them, expect PASS**; then `cargo test -p celesta-editor`, clippy, fmt.
- [ ] **Step 5: Commit** — `feat(editor): give an element its own timing`

### Task 8: Docs, example and handoff

**Files:**
- Modify: `packages/website/content/{en,ja}/docs/01-(getting-started)/03-preview.md` (dragging bands and chapters, snapping, "Set timing" and what it means for animations, "Set timing" on a shared element, locks)
- Modify: `docs/superpowers/specs/2026-10-23-editor-timeline-timing-design.md` (only what the build settled: grips on `windows[0]` and the band holding the lookup frame; window kind from the import; the "scene's window" comparison clipped and rounded; scene lists with a spread are skipped and only later entries move; the scene bounds and the visible-length write; the composition-follows rule across files; "Set timing" values without a window and on a shared element)
- Check: `examples/delta/v0.7.0/demos/EditorDemo.tsx`
- Create: `docs/handoff/2026-10-26-timeline-timing.md` (decisions, known gaps — lists with spreads lock as not found, a `.map()` of `Series` items moves as one — and "Verify by hand" with before/after screenshots of the same frame)

- [ ] **Step 1: Write the docs**; `pnpm --filter @celesta/website check`.
- [ ] **Step 2: Commit** — `docs(editor): describe timing on the timeline`

## Order

Task 1 ∥ Task 2 ∥ Task 3 → Task 4 → Task 5 → Task 6 → Task 7 → Task 8.

| Task | Design 3 | This plan |
| --- | --- | --- |
| 1 | none — runs alongside design 3 | — |
| 2 | none — runs alongside design 3 | — |
| 3 | Task 1's `Edit::Splice` (its first commit) | — (unblocks design 5) |
| 4 | Tasks 2–3 (`ElementTargets` fields, `commit_scoped`, `SceneCalls`) | 1, 2 |
| 5 | Task 3 (`commit_gate`), Task 4 (`scope_in_effect`) | 4 |
| 6 | Task 3 (`apply_checked`'s callers include `commit_scoped`) | 2, 5 |
| 7 | Task 2 (`ScopeState`, `chosen_scope`) | 1, 3, 4 |
| 8 | — | all |

Tasks 5–7 share `timeline.rs` and `timeline_drag.rs`, so they run in sequence.
