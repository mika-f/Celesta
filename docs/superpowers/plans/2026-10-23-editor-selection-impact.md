# Selection and Impact Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When a layer is selected in the React preview, the Properties tab and the timeline show what it is, when it is on screen, which scenes an edit to it would change, and why it can't move; the selection no longer blanks after the editor's own write.

**Architecture:** The runtime (`packages/react`) marks timing wrappers in the source chain, reports each layer's timing windows, and answers a chunked whole-timeline occurrence sweep. The bridge (`crates/react-bridge`) parses these into `LayerSourceReport.windows`/`span` and an `OccurrenceIndex`. The editor runs the sweep on its own worker per reload, derives an `Impact` for the selection, and draws it in a header, an impact line, and timeline bands.

**Tech Stack:** TypeScript + React reconciler (Vitest), Rust (serde, GPUI via gpui-kit), Node bridge over stdin/stdout JSON lines.

**Spec:** `docs/superpowers/specs/2026-10-23-editor-selection-impact-design.md` (part 1 of `docs/superpowers/specs/2026-10-23-editor-element-editing-overview-design.md`)

## Global Constraints

- React compositions in the standalone preview only, `ReactPreviewMode::WholeScene`; `.celesta` JSON projects are untouched.
- UI copy is English and follows the inspector style: radius 0, no gradients, no uppercase labels, lucide icons, gpui-kit components.
- Impact is counted in scenes, not instances. A scene is a chapter; nested chapters (a part inside a scene) count as their outermost chapter. Without chapters the whole timeline is one scene.
- Tests: Vitest in TypeScript under `packages/*/test/*.test.mts` importing `../src`; Rust unit tests in `crates/*`.
- Export output must not change: scenes rendered with and without source locations stay identical.
- The PR carries screenshots of the same scene before and after (AGENTS.md), and checks `examples/delta/v0.7.0/demos/EditorDemo.tsx`, updating it if it shows the old Properties tab.
- No `git commit --amend`; one commit per task.

## Decisions made while planning (also written back into the spec)

- **Chunked sweep instead of killing the process.** The spec cancels a running sweep by killing its bridge process. `ReactBridge` owns its `Child` and exposes no handle another thread could kill, so the sweep runs as `collectOccurrences { from, frames }` requests of 240 frames; the worker abandons a sweep between chunks when a newer request is queued.
- **No per-fiber chain cache in the sweep.** Fibers alternate between renders and `committedFibers` exists to pick the live one; caching chains by fiber risks stale chains. The sweep computes the committed set once per frame and walks chains from it.
- **Repeated places in one scene.** The impact line for a non-shared element says how many places it changes when the element renders more than once in the current scene ("Changes 2 places in this scene"), so copies in the same scene are not silent either.

## Review Focus

- **A composition without chapters.** Nothing is "shared"; "Appears in" shows timecode ranges instead of scene names; the impact line still counts places. Test in Task 6.
- **An element the sweep did not record** (no source position for it, or the index from an older reload). The header falls back to the report's `span` instead of claiming "this scene only". Test in Task 6.
- **The sweep fails on some frame** (the entry throws at frame 400). The header shows "Couldn't scan scenes: …" and everything else keeps working. Test in Task 5.
- **A reload during a sweep.** The old sweep stops at its next chunk and its result is never shown. Test in Task 5.
- **Nested chapters** (a scene and parts listed inside it). An element in a part of "Price" is not "shared" between "Price" and the part. Test in Task 4.

---

### Task 1: Wrapper entries in the source chain

**Files:**
- Modify: `packages/react/src/source-chain.ts:9-18,83-86,154-184`
- Modify: `packages/react/src/components.ts:1074`
- Test: `packages/react/test/layer-source.test.mts`

**Interfaces:**
- Produces: `LayerSourceEntry.kind: 'primitive' | 'component' | 'wrapper'`. `'wrapper'` for every component passed to `markTransparent` (`Composition`, `Sequence`, `FreezeFrame`, `Loop`, `MotionBlur`, `Trail`, `FrameChildren`). Primitive is checked first.

- [ ] **Step 1: Write the failing tests**

```ts
test('timing wrappers written in the source are wrapper entries', () => {
  // <Sequence><Loop><FreezeFrame><Rect/></FreezeFrame></Loop></Sequence> inside <Scene>
  // assert kinds innermost first: ['primitive', 'wrapper' (FreezeFrame), 'wrapper' (Loop), 'wrapper' (Sequence), 'component' (Scene), 'wrapper' (Composition)]
});

test('a user component named Sequence is a component entry', () => {
  // function Sequence() { return rectAt(5); } — assert its entry kind === 'component'
});
```

Build the trees with `jsxDEV(…, at('scene.tsx', line, col))` like the existing tests. Existing assertions on the `Composition` entry's kind change from `'component'` to `'wrapper'`; update them.

- [ ] **Step 2: Run them to verify they fail**

Run: `pnpm --filter @celesta/react exec vitest run test/layer-source.test.mts`
Expected: FAIL — kinds read `'component'`.

- [ ] **Step 3: Implement**

Add `'wrapper'` to `LayerSourceEntry.kind`; in `sourceFibers` set `kind` to `'primitive'` if `isPrimitive(type)`, else `'wrapper'` if `transparent.has(unwrap(type))`, else `'component'`. Add `Loop` to the `markTransparent` list in `components.ts:1074`. Update the doc comment on `markTransparent` to say wrappers are also reported as `'wrapper'` entries.

- [ ] **Step 4: Run the package tests and typecheck**

Run: `pnpm --filter @celesta/react test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add packages/react/src/source-chain.ts packages/react/src/components.ts packages/react/test/layer-source.test.mts
git commit -m "feat(react): report timing wrappers as wrapper entries in source chains"
```

---

### Task 2: Timing windows and span in the layer source report

**Files:**
- Modify: `packages/react/src/source-chain.ts` (report types, `layerSourceReport`, new window walk)
- Modify: `packages/react/src/jsx-dev-runtime.ts`
- Modify: `packages/react/src/components.ts:1073-1074` (registrations)
- Modify: `packages/react/src/series.ts:82-90`
- Test: `packages/react/test/layer-source.test.mts`

**Interfaces:**
- Consumes: Task 1's `'wrapper'` kind.
- Produces (TypeScript, exported from `source-chain.ts`):

```ts
export interface SourcePos { file: string; line: number; column: number }
export interface TimingWindow {
  kind: 'Sequence' | 'FreezeFrame' | 'Loop';
  from: number; // composition frames
  to: number;   // exclusive
  source?: SourcePos;
  remapped?: true;
}
// LayerSourceReport gains, when the chain is non-empty:
windows: TimingWindow[]; // nearest first
span: { from: number; to: number };
```

- `markWindow(component: unknown, kind: 'Sequence' | 'FreezeFrame' | 'Loop' | 'LoopCycle'): void` in `source-chain.ts`, called in `components.ts` for `Sequence`, `FreezeFrame`, `Loop`, `LoopCycle`.
- `copySourcePosition(from: object, to: object): void` in `jsx-dev-runtime.ts`: gives `to` the position recorded for `from`, if any.

- [ ] **Step 1: Write the failing tests**

Use a composition of `durationInFrames: 100`. Assert exact values:

```ts
test('windows lists nested Sequences in composition frames, nearest first', () => {
  // <Sequence from={10} durationInFrames={50}> <Sequence from={5} durationInFrames={20}> <Rect/>
  // at frame 20: windows = [{kind:'Sequence', from:15, to:35, source: inner pos}, {kind:'Sequence', from:10, to:60, source: outer pos}]
  // span = {from:15, to:35}
});
test('a Series item window points at the Series.Sequence', () => {
  // <Series><Series.Sequence durationInFrames={30}/><Series.Sequence durationInFrames={40}><Rect/></…>
  // at frame 40: windows[0] = {kind:'Sequence', from:30, to:70, source: position of the second <Series.Sequence>}
});
test('a Scenes scene window has no source', () => {
  // defineScenes([{id:'a', durationInFrames:30, Scene: A}, {id:'b', durationInFrames:40, Scene: B}]); <Scenes>
  // Rect in B at frame 35: windows[0] = {kind:'Sequence', from:30, to:70}, 'source' in windows[0] === false
});
test('a Loop reports its outer window, remapped, and windows inside it inherit it', () => {
  // <Loop from={10} durationInFrames={20} times={3}><Sequence from={5}><Rect/>
  // at frame 37: windows = [{kind:'Sequence', from:10, to:70, remapped:true, source: inner pos}, {kind:'Loop', from:10, to:70, remapped:true, source: Loop pos}]
});
test('a FreezeFrame window and windows inside it take the enclosing window', () => {
  // <Sequence from={10} durationInFrames={50}><FreezeFrame frame={80}><Sequence from={70}><Rect/>
  // at frame 20: windows = [{Sequence,10,60,remapped}, {FreezeFrame,10,60,remapped}, {Sequence,10,60, source outer}]
});
test('an element without wrappers spans the composition', () => {
  // windows = [], span = {from:0, to:100}
});
```

The existing check that export scenes are unchanged with source locations (`packages/cli/test/layer-source.test.mts`) must keep passing; update it only where it asserts the report's shape.

- [ ] **Step 2: Run them to verify they fail**

Run: `pnpm --filter @celesta/react exec vitest run test/layer-source.test.mts`
Expected: FAIL — `windows` undefined.

- [ ] **Step 3: Implement the walk in `source-chain.ts`**

Collect window fibers from the element up to the root (same committed walk as `framesAbove`), then resolve them outermost first:

```text
rootFrame, compDuration = frame and durationInFrames of the outermost CompositionRuntimeContext provider
enclosing = { from: 0, to: compDuration }; remapped = false
for each window fiber, outermost first:
  kind = markWindow kind of its type
  skip a Sequence whose committed parent is a Loop or LoopCycle, and every LoopCycle
  value = the CompositionRuntimeContext provider value directly below it
          (for a Loop: below its Sequence child)
  Sequence, not remapped: from = enclosing.from + props.from (finite, else 0); to = from + value.durationInFrames
  Loop:                   same from/to as a Sequence; remapped = true from here down
  FreezeFrame:            from/to = enclosing; remapped = true from here down
  any window while remapped (other than the Loop itself): from/to = enclosing
  source = sourceOfProps(fiber props) (Loop: the Loop fiber's props)
  push { kind, from, to, source?, remapped? }; if not remapped, enclosing = { from, to }
windows = reversed (nearest first); span = intersection of all windows and [0, compDuration]
```

Register with `markWindow` beside the existing `markPrimitive`/`markTransparent` calls in `components.ts` (export `LoopCycle` only internally). In `SeriesRoot`, call `copySourcePosition(item.props, sequenceProps)` on the props object passed to each `Sequence`.

- [ ] **Step 4: Run the package tests**

Run: `pnpm --filter @celesta/react test && pnpm --filter @celesta/cli test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add packages/react/src packages/react/test packages/cli/test
git commit -m "feat(react): report timing windows and the span of a selected layer"
```

---

### Task 3: Occurrence sweep in the runtime and CLI

**Files:**
- Modify: `packages/react/src/source-chain.ts` (new `occurrenceKeys`)
- Modify: `packages/react/src/render.ts:265-285,1739-1763` (`MountedComposition.collectOccurrences`, shared layer walk)
- Modify: `packages/cli/src/cli.ts:99-152,323-341`
- Test: `packages/react/test/occurrences.test.mts` (new), `packages/cli/test/layer-source.test.mts`

**Interfaces:**
- Consumes: Task 2's `SourcePos`; Task 1's `'wrapper'` kind.
- Produces:

```ts
// source-chain.ts
export function occurrenceKeys(
  layerNodes: ReadonlyMap<string, HostNode>,
  resolveFile: (fileName: string) => string,
): Array<{ element: SourcePos; chain: SourcePos[] }>; // one per distinct host node with a position; chain skips 'wrapper' entries, innermost first

// render.ts
export interface OccurrenceIndex {
  elements: Array<{
    element: SourcePos;
    instances: Array<{ chain: SourcePos[]; ranges: Array<[number, number]>; maxPerFrame: number }>;
  }>;
}
// MountedComposition
collectOccurrences(from: number, frames: number, resolveFile: (fileName: string) => string, options?: { preview?: boolean }): OccurrenceIndex;

// cli.ts request / reply
{ collectOccurrences: { from: number; frames: number; preview?: boolean } }
{ collectedOccurrences: OccurrenceIndex }
```

Ranges are `[from, to)` composition frames, only within `[from, from + frames)` clipped to the composition; adjacent frames are joined.

- [ ] **Step 1: Write the failing tests** (`occurrences.test.mts`)

```ts
test('one component used in two scenes is one element with two instances', () => {
  // <Sequence from={0} durationInFrames={30}><Card/></Sequence><Sequence from={30} durationInFrames={30}><Card/></Sequence>, Card writes <Rect>
  // collectOccurrences(0, 100): one element (Rect position); instances[].ranges = [[0,30]] and [[30,60]]
});
test('one Scene component in two defineScenes entries is one instance with ranges in both', () => {
  // ranges = [[0,30],[60,90]]
});
test('.map() copies in one frame raise maxPerFrame', () => {
  // [1,2,3].map(i => <Rect key={i}/>) → maxPerFrame === 3, one instance
});
test('a conditionally rendered element stops where the condition does', () => {
  // frame < 40 && <Rect/> → ranges [[0,40]]
});
test('a chunk reports only its own frames', () => {
  // collectOccurrences(20, 10): ranges clipped to [[20,30]]
});
test('layers without a source position are skipped', () => {
  // a Rect built with createElement (no jsxDEV) → elements === []
});
```

Add one CLI test that sends `{ collectOccurrences: { from: 0, frames: 10 } }` and reads `collectedOccurrences`.

- [ ] **Step 2: Run them to verify they fail**

Run: `pnpm --filter @celesta/react exec vitest run test/occurrences.test.mts`
Expected: FAIL — `collectOccurrences` is not a function.

- [ ] **Step 3: Implement**

Move the body of `layerSource` that settles and walks the tree into `walkLayerNodes(time, preview): { instance, layerNodes }` and use it from both `layerSource` and `collectOccurrences`. In `collectOccurrences`, for each frame call `occurrenceKeys` and record, per element key and instance key (positions joined into a string key), the frame and the per-frame count. `occurrenceKeys` computes `committedFibers` once from the first node and reuses it for every node. Like `layerSource`, it discards the scene and leaves the `<Static>` cache untouched. In `cli.ts`, validate integer `from`/`frames` like `collectSubtitles`, resolve files the same way as `layerSource`.

- [ ] **Step 4: Run the tests**

Run: `pnpm --filter @celesta/react test && pnpm --filter @celesta/cli test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add packages/react/src packages/react/test packages/cli/src packages/cli/test
git commit -m "feat(react): sweep the timeline for where each element renders"
```

---

### Task 4: Bridge types, occurrence index, and lookups

**Files:**
- Modify: `crates/react-bridge/src/types.rs:33-107`
- Modify: `crates/react-bridge/src/protocol.rs:94-244`
- Modify: `crates/react-bridge/src/bridge.rs` (new `collect_occurrences`)
- Create: `crates/react-bridge/src/occurrences.rs`
- Modify: `crates/react-bridge/src/lib.rs` (module and re-exports)
- Modify: `crates/editor/src/layer_select.rs:124-129` (exhaustive match)
- Test: `crates/react-bridge/src/tests.rs`, `crates/react-bridge/src/occurrences.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: Tasks 1–3's JSON shapes.
- Produces:

```rust
pub enum ElementKind { Primitive, Component, Wrapper }          // serde "wrapper"
#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize)]
pub struct SourcePos { pub file: PathBuf, pub line: u32, pub column: u32 }
impl From<&LayerSource> for SourcePos;
pub struct TimingWindow { pub kind: String, pub from: f64, pub to: f64, pub source: Option<SourcePos>, pub remapped: bool }
pub struct FrameSpan { pub from: f64, pub to: f64 }
// LayerSourceReport gains:
pub windows: Vec<TimingWindow>,   // serde default: empty
pub span: Option<FrameSpan>,      // None from older runtimes

pub struct OccurrenceIndex { pub elements: Vec<ElementOccurrences> }
pub struct ElementOccurrences { pub element: SourcePos, pub instances: Vec<InstanceOccurrences> }
pub struct InstanceOccurrences { pub chain: Vec<SourcePos>, pub ranges: Vec<(u64, u64)>, pub max_per_frame: u32 }
impl OccurrenceIndex {
    pub fn merge(&mut self, chunk: OccurrenceIndex);
    pub fn instance_for(&self, report: &LayerSourceReport) -> Option<(&ElementOccurrences, &InstanceOccurrences)>;
}
pub fn scenes_of<'a>(ranges: &[(u64, u64)], chapters: &'a [ReactChapter]) -> Vec<&'a ReactChapter>;
pub fn outermost_chapter_at(chapters: &[ReactChapter], frame: u64) -> Option<&ReactChapter>;
impl ReactBridge { pub fn collect_occurrences(&mut self, from: u64, frames: u64) -> Result<OccurrenceIndex, ReactBridgeError>; }
```

- `instance_for` matches the report's chain without `Wrapper` entries, as `SourcePos`es, against `InstanceOccurrences::chain`; the element is the first such entry.
- `scenes_of` returns outermost chapters (not contained in another chapter's `[from, end)`) that overlap any range, in chapter order, each once. Length-0 chapters never match.
- `merge` appends unknown elements and instances, joins a range that starts where an existing one ends, and keeps the larger `max_per_frame`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn layer_source_reply_reads_windows_span_and_wrapper_kind() // JSON with kind "wrapper", windows, span → parsed values
#[test] fn layer_source_reply_from_an_older_runtime_has_no_windows()  // no windows/span keys → windows empty, span None
#[test] fn merge_joins_ranges_across_chunks()        // [(0,240)] + [(240,300)] → [(0,300)]; max_per_frame max(1,3) = 3
#[test] fn instance_for_skips_wrapper_entries()       // report chain [Rect prim, Sequence wrapper, Card comp] ↔ instance chain [Rect, Card]
#[test] fn scenes_of_counts_a_part_as_its_scene()     // chapters price[30,90), price-intro[30,45); ranges [(32,40)] → [price]
#[test] fn scenes_of_ignores_touching_edges()         // chapters a[0,30), b[30,60); ranges [(0,30)] → [a]
#[test] fn outermost_chapter_at_prefers_the_scene()   // frame 35 with the two chapters above → price
#[test] fn collected_occurrences_response_parses()    // protocol: {"collectedOccurrences": {...}} → Response::CollectedOccurrences
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p celesta-react-bridge`
Expected: FAIL — unknown types.

- [ ] **Step 3: Implement**

Add the types and serde (camelCase, `#[serde(default)]` on the new report fields, `windows` mapped in `LayerSourceReply` → `LayerSourceReport`). Add `RawResponse.collected_occurrences` and `Response::CollectedOccurrences`, and extend the "response has none of …" message. `collect_occurrences` sends `{"collectOccurrences": {"from", "frames", "preview": self.preview}}`. In `layer_select.rs`, `ElementKind::Wrapper => None`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p celesta-react-bridge && cargo test -p celesta-editor`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/react-bridge crates/editor/src/layer_select.rs
git commit -m "feat(react-bridge): parse timing windows and the occurrence index"
```

---

### Task 5: Occurrence worker in the editor

**Files:**
- Create: `crates/editor/src/occurrences.rs`
- Modify: `crates/editor/src/main.rs` (module), `crates/editor/src/view.rs` (worker and state fields), `crates/editor/src/refresh.rs:488-505` (request on reload), `crates/editor/src/background.rs` (poll)

**Interfaces:**
- Consumes: Task 4's `OccurrenceIndex`, `ReactBridge::collect_occurrences`.
- Produces:

```rust
pub(crate) const CHUNK_FRAMES: u64 = 240;
pub(crate) struct OccurrenceRequest { pub generation: u64, pub node: PathBuf, pub cli_script: PathBuf, pub entry: PathBuf,
    pub properties: PropertyInputs, pub composition_id: String, pub duration_in_frames: u64 }
pub(crate) struct OccurrenceResult { pub generation: u64, pub index: Result<OccurrenceIndex, String> }
pub(crate) struct OccurrenceWorker { pub requests: mpsc::Sender<OccurrenceRequest>, pub results: mpsc::Receiver<OccurrenceResult> }
impl OccurrenceWorker { pub fn spawn() -> Result<Self, Box<dyn Error>>; pub fn request(&self, r: OccurrenceRequest) -> Result<(), String>; }
pub(crate) enum OccurrenceState { Scanning, Ready(OccurrenceIndex), Failed(String) }
/// None when `cancelled()` turned true between chunks.
pub(crate) fn sweep(duration: u64, chunk: impl FnMut(u64, u64) -> Result<OccurrenceIndex, String>, cancelled: impl FnMut() -> bool) -> Option<Result<OccurrenceIndex, String>>;
// EditorView
pub(crate) occurrences: Option<(u64, OccurrenceState)>, // tagged with react_reload_generation
pub(crate) fn refresh_occurrences(&mut self);
```

The worker is shaped like `ReactAudioWorker` (`react_audio.rs:44-120`): own thread, `take_latest`, a new bridge per request spawned with `SpawnOptions { composition_id, source_locations: true }` and `set_preview(true)`. The thread keeps `pending: Option<OccurrenceRequest>`; `cancelled` is a closure that calls `request_rx.try_recv()` and, when it yields a request, stores it in `pending` and returns true. The loop takes `pending` before blocking on `recv()`.

- [ ] **Step 1: Write the failing tests** (in `occurrences.rs`)

```rust
#[test] fn sweep_asks_for_every_chunk_and_merges() // duration 500 → chunk calls (0,240),(240,240),(480,20); merged ranges joined
#[test] fn sweep_stops_between_chunks_when_cancelled() // cancelled after first chunk → None, one chunk call
#[test] fn sweep_reports_the_first_chunk_error()    // second chunk Err("frame 400: boom") → Some(Err("frame 400: boom"))
#[test] fn a_result_for_an_older_reload_is_dropped() // view helper: accept_occurrences(current=3, result.generation=2) == false
```

Put the generation check in a free function `accepts(current: u64, result: u64) -> bool` used by `background.rs`.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p celesta-editor occurrences`
Expected: FAIL.

- [ ] **Step 3: Implement**

`refresh_occurrences` sets `occurrences = Some((react_reload_generation, Scanning))` and requests a sweep when `is_react_preview()`; it is called next to `refresh_audio_preview()` in the reload path (`refresh.rs:504`). `poll_background_work` stores `Ready`/`Failed` for the current generation and calls `cx.notify()` through its existing caller.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p celesta-editor`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/editor/src
git commit -m "feat(editor): sweep the React entry for occurrences on every reload"
```

---

### Task 6: Impact of the selection

**Files:**
- Create: `crates/editor/src/impact.rs`
- Modify: `crates/editor/src/main.rs`

**Interfaces:**
- Consumes: Task 4's `OccurrenceIndex`, `scenes_of`, `outermost_chapter_at`, `LayerSourceReport.span`; Task 5's `OccurrenceState`.
- Produces:

```rust
pub(crate) struct Impact {
    /// Outermost chapters this instance's occurrences overlap (names).
    pub appears_in: Vec<String>,
    /// The ids of `appears_in`, in the same order.
    pub appears_in_ids: Vec<String>,
    /// This instance's ranges, for "Appears in" without chapters.
    pub own_ranges: Vec<(u64, u64)>,
    /// Outermost chapters any instance overlaps, minus the current one.
    pub other_scenes: Vec<String>,
    pub instance_count: usize,
    /// Copies in the current scene: Σ max_per_frame of instances overlapping it.
    pub places_in_scene: u32,
    /// The innermost component position of each instance's chain, for the hover list.
    pub call_sites: Vec<SourcePos>,
    /// Filled bands: this instance's ranges inside the current scene, each
    /// with its instance's index into `call_sites`.
    pub own_bands: Vec<((u64, u64), usize)>,
    /// Outlined bands: every other occurrence of the element, each with its
    /// instance's index into `call_sites`.
    pub other_bands: Vec<((u64, u64), usize)>,
}
pub(crate) enum ImpactView {
    /// No index yet (scanning, failed, or the element is not in it): the report's span only.
    SpanOnly { span: Option<(f64, f64)>, note: String },
    Known(Impact),
}
pub(crate) fn impact_view(report: &LayerSourceReport, occurrences: Option<&OccurrenceState>, chapters: &[ReactChapter], frame: u64) -> ImpactView;
/// The impact line's text and whether it uses the warning color.
pub(crate) fn impact_line(view: &ImpactView) -> (String, bool);
```

Copy, exactly:

| Case | `impact_line` |
| --- | --- |
| `Known`, `other_scenes` empty, `places_in_scene` ≤ 1 | `("Changes this scene only", false)` |
| `Known`, `other_scenes` empty, `places_in_scene` = n > 1 | `("Changes {n} places in this scene", false)` |
| `Known`, `other_scenes` non-empty | `("Also changes {names joined by \", \"}", true)` |
| `SpanOnly` | `(note, false)` |

`SpanOnly` notes: `"Scanning scenes…"` (Scanning or none), `"Couldn't scan scenes: {message}"` (Failed), `"Not found in the scene scan"` (Ready but `instance_for` is `None`).

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn shared_across_scenes_names_the_others()       // instances in intro and price, current price → other_scenes ["Intro"], line ("Also changes Intro", true)
#[test] fn one_scene_component_reused_by_scenes_is_shared() // one instance, ranges in a and b, current a → other_scenes ["B"]
#[test] fn map_copies_count_places_in_this_scene()       // max_per_frame 3 → ("Changes 3 places in this scene", false)
#[test] fn without_chapters_nothing_is_shared()          // chapters [] → appears_in [], own_ranges set, line ("Changes this scene only", false)
#[test] fn missing_from_the_index_falls_back_to_the_span() // Ready index without the element → SpanOnly { span: Some((15.0, 35.0)), note: "Not found in the scene scan" }
#[test] fn scanning_and_failed_notes()                   // Scanning → "Scanning scenes…"; Failed("boom") → "Couldn't scan scenes: boom"
#[test] fn bands_split_own_scene_from_the_rest()          // own ranges [(0,30),(60,90)], current chapter [0,30) → own_bands [(0,30)], other_bands includes (60,90)
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p celesta-editor impact`
Expected: FAIL.

- [ ] **Step 3: Implement `impact_view` and `impact_line`**

Current scene = `outermost_chapter_at(chapters, frame)`; without chapters every range is "in the current scene".

- [ ] **Step 4: Run the tests**

Run: `cargo test -p celesta-editor impact`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/editor/src/impact.rs crates/editor/src/main.rs
git commit -m "feat(editor): derive which scenes an edit of the selection changes"
```

---

### Task 7: Selection header, impact line, and locked summary

**Files:**
- Modify: `crates/editor/src/layer_panel.rs:17-98`
- Modify: `crates/editor/src/panels.rs:800-840`
- Modify: `crates/editor/src/icons.rs` (new icons)
- Test: `crates/editor/src/layer_panel.rs` (`#[cfg(test)]`) or `crates/editor/src/tests.rs`

**Interfaces:**
- Consumes: Task 6's `impact_view`, `impact_line`; `transform_edit::reason_text`, `TransformTargets`, `TransformProp::label`.
- Produces:

```rust
pub(crate) fn primitive_icon(name: &str) -> CelestaIcon;  // Rect→Square, Ellipse|Circle→Circle, Text→Type, Image→Image, Video→Film, Group|Stack→Group, other @celesta/shapes→Shapes
/// "in <Card>": the nearest Component entry after the first, wrappers skipped.
pub(crate) fn owner_label(report: &LayerSourceReport) -> Option<String>;
/// One line per lock reason: "X, Y — Written as an expression: `W / 2`", in target order; editable props omitted.
pub(crate) fn locked_summary(targets: &TransformTargets) -> Vec<String>;
```

New `CelestaIcon` variants with Lucide SVG bodies: `Square`, `Circle`, `Type`, `Image`, `Film`, `Group`, `Shapes`, `ChevronRight` (for the disclosure; use `IconName` instead where gpui-kit already ships the glyph).

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn primitive_icons() // "Rect"→Square, "Circle"→Circle, "Star"→Shapes, "Group"→Group
#[test] fn owner_label_skips_wrappers() // chain [Rect prim, Sequence wrapper, PriceTag comp] → Some("in <PriceTag>"); [Rect, Composition wrapper] → None
#[test] fn locked_summary_groups_props_by_reason() // X,Y Expression "W / 2", Rotation Literal → ["X, Y — Written as an expression: `W / 2`"]
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p celesta-editor -- primitive_icons owner_label locked_summary`
Expected: FAIL.

- [ ] **Step 3: Implement the functions and the header**

`properties_panel` order becomes: header, impact line, `layer_edit_section` (Transform), `property_section`, then the Source chain. The header block (above the Layer section):

```text
[icon] Rect                       in <PriceTag>
       components/PriceTag.tsx:42  (opens the file, like source_row)
       Appears in  Price            ← scene names, or timecode ranges via format_timecode without chapters
       Shared      also in Intro, Outro (3 instances)   ← warning color; only when other_scenes is non-empty; hover lists call_sites
       Can't move  X, Y — Written as an expression: `W / 2`   ← one row per locked_summary line
```

While `ImpactView::SpanOnly`, the "Appears in" row shows its note instead. The impact line sits directly above the Transform group, in the warning color when `impact_line` says so. The existing "Source" and "Called from" rows move into a "Source chain" disclosure, collapsed by default (a `bool` on the view, not persisted).

- [ ] **Step 4: Run the tests**

Run: `cargo test -p celesta-editor`
Expected: PASS.

- [ ] **Step 5: Check it in the app**

Run the editor on a React entry whose component appears in two chapters (for example `examples/reel`); select an element of a shared component and one written directly in a scene. Take screenshots of the Properties tab before (main) and after.

- [ ] **Step 6: Commit**

```bash
git add crates/editor/src
git commit -m "feat(editor): show what the selection is and which scenes an edit changes"
```

---

### Task 8: Timeline highlight

**Files:**
- Modify: `crates/editor/src/timeline.rs:1009-1081` (`react_composition_row`), `:1104-` (`chapter_row`)
- Test: `crates/editor/src/timeline.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: Task 6's `ImpactView` (`own_bands`, `other_bands`, `SpanOnly.span`).
- Produces:

```rust
/// Left and width of each `[from, to)` frame range as lane fractions, dropping
/// ranges outside the visible window.
fn band_geometry(ranges: &[(u64, u64)], view: LaneView) -> Vec<(f32, f32)>;
```

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn band_geometry_places_ranges_on_the_lane()   // end_frame 100, zoom 1, view_start 0; (25,50) → (0.25, 0.25)
#[test] fn band_geometry_drops_ranges_outside_the_view() // zoom 4, view_start 0.5; (0,10) dropped; (40,60) partly visible kept
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p celesta-editor band_geometry`
Expected: FAIL.

- [ ] **Step 3: Implement**

On the composition lane, above the composition block and below the playhead: `own_bands` (or `SpanOnly.span` rounded outward) as filled accent bands, `other_bands` as 1 px accent outlines. Each band has a tooltip (`{scene name or timecode range} · frames {from}–{to}` plus the call site) and a click seeks to its `from` (`seek_frame`, pausing like `seek_chapter`). On the chapter row, chapters in `appears_in` get a 2 px accent bottom border. Bands clear with the selection.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p celesta-editor`
Expected: PASS.

- [ ] **Step 5: Check it in the app**

Same entry as Task 7: both bands visible, the outlined one in the other scene, a click on it seeks there. Screenshot the timeline.

- [ ] **Step 6: Commit**

```bash
git add crates/editor/src/timeline.rs
git commit -m "feat(editor): highlight where the selected element appears on the timeline"
```

---

### Task 9: Keep the selection across the editor's own writes; docs

**Files:**
- Modify: `crates/editor/src/transform_edit.rs:251-388` (`TransformEdit::expire`)
- Modify: `crates/editor/src/refresh.rs:488-503` (keep branch)
- Modify: `crates/editor/src/background.rs:43-57` (`LookupFate::Show`)
- Modify: `crates/editor/src/transform_drag.rs:262-271` (keep the released override)
- Modify: `crates/editor/src/view.rs` (`source_lookup_stale: bool`)
- Modify: `packages/website/content/en/docs/01-(getting-started)/03-preview.md`, `packages/website/content/ja/docs/01-(getting-started)/03-preview.md`
- Create: `docs/handoff/2026-10-23-selection-impact.md`
- Test: `crates/editor/src/tests.rs`

**Interfaces:**
- Consumes: `SelectionKeep`, `LookupFate`, `TransformEdit`.
- Produces:

```rust
impl TransformEdit {
    /// The code reloaded after an own write: keep showing `resolved` (not
    /// current), and resolve the next lookup answer even if it equals the last.
    pub(crate) fn expire(&mut self);
}
pub(crate) source_lookup_stale: bool, // on EditorView
```

Behavior:
- Keep branch of a reload (`refresh.rs:490`): leave `source_lookup` as it is, set `source_lookup_stale = true`, call `transform.expire()` instead of `reset()`, and drop a released `transform_drag` before `refresh_preview()`.
- `LookupFate::Show`: set the new lookup and `source_lookup_stale = false`.
- `TransformEdit::finish` after `expire` clears `stale` (a lookup answered by the old code is already filtered by `LookupFate::Stale`).
- On a successful commit, `transform_drag` stays (released) until the kept reload, so a redraw in between still shows the dragged position.
- The header shows a small spinner while `source_lookup_stale`; Transform fields are read-only because `resolved()` reports not current.
- Outside reloads clear the selection as today.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn expire_keeps_the_resolution_visible_but_not_current() // after finish + invalidate + expire: resolved(Some(id)) == Some((_, false)); begin(same report) returns Some(_)
#[test] fn finish_after_expire_is_current_again()               // expire, begin, finish → resolved(..).1 == true
#[test] fn an_own_write_reload_keeps_the_lookup()               // view-level, using the SelectionKeep flow as in tests.rs:1435: after the keep branch, source_lookup still Found and source_lookup_stale
#[test] fn an_outside_reload_clears_the_selection()             // existing behavior pinned
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p celesta-editor -- expire own_write outside_reload`
Expected: FAIL.

- [ ] **Step 3: Implement the behavior above**

- [ ] **Step 4: Run all tests**

Run: `cargo test -p celesta-editor && cargo test -p celesta-react-bridge && pnpm --filter @celesta/react test && pnpm --filter @celesta/cli test`
Expected: PASS.

- [ ] **Step 5: Check it in the app**

Drag a layer and release: the Properties tab keeps its rows (read-only with a spinner) and the layer does not jump back before the reload.

- [ ] **Step 6: Write the docs**

- `03-preview.md` (en and ja): the header, "Appears in" / "Shared", the impact line, the timeline bands, "Can't move".
- The handoff note: what changed, the chunked sweep and its cost (one tree walk per frame per reload, on its own process), the decisions listed at the top of this plan, and the manual checks with their results.
- Check `examples/delta/v0.7.0/demos/EditorDemo.tsx`; update it in this commit if it draws the old Properties tab.

- [ ] **Step 7: Commit**

```bash
git add crates/editor/src packages/website/content docs/handoff examples
git commit -m "feat(editor): keep the selection visible across the editor's own writes"
```
