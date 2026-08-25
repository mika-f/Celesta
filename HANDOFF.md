# Mikan implementation handoff

Last updated: 2026-08-25

## Goal

Mikan is a code-first video editor. The GPUI editor owns and edits project
data. Projects and React compositions are evaluated into the same Rust
composition model, and preview/export should consume the same renderer inputs.

The current milestone is a usable editor foundation for gameplay videos with
VOICEROID-style portraits, dialogue subtitles, voice assets, and ordinary
video/audio tracks.

## Repository state

- Workspace: `/Users/natsuneko/ghq/github.com/mika-f/mikan`
- Rust edition: 2024
- Minimum Rust version: 1.89
- The initial implementation is tracked on `main`; track management landed in
  commit `52b13d5` and MP4 export landed in `dfb7f1a`. Inspect
  `git status --short` for newer work before editing or staging.
- FFmpeg and FFprobe are installed and available on `PATH`.
- GPUI is pinned to crates.io version `0.2.2`.
- Node.js (>= 18) and pnpm are required for the React composition path
  (`packages/react`, `mikan-react-bridge`). Run `pnpm install && pnpm run
  codegen && pnpm run build` once in `packages/react` before using
  `mikan-exporter --react` or its tests; `mikan-react-bridge` spawns the
  compiled `dist/cli.js`, not the TypeScript sources directly. `pnpm run
  codegen` runs `cargo test -p mikan-project -p mikan-composition --features
  codegen` to (re)generate `src/generated/*.ts`, which `pnpm run build`
  requires as input; both `src/generated/` and `dist/` are gitignored build
  output, not checked in.

Before editing, run:

```sh
git status --short
cargo test --workspace
```

## Workspace map

| Crate | Responsibility |
| --- | --- |
| `mikan-composition` | Renderer-independent scene types, exact rational time, transforms, animation evaluation, text styles, and audio graph types. |
| `mikan-project` | Version 0 JSON project format, loading, semantic validation, references, and duration calculation. |
| `mikan-evaluator` | Deterministic conversion from `Project` to a visual `Scene` at a time and to the complete `AudioGraph`. |
| `mikan-media` | FFprobe metadata and FFmpeg exact-time RGBA video-frame decoding behind `VideoFrameDecoder`. |
| `mikan-renderer` | Deterministic CPU reference renderer, PNG output, and the shared text rasterizer. |
| `mikan-gpu-renderer` | `wgpu` renderer for images, video frames, styled text, nested transforms, opacity, offscreen readback, and renderer-owned surfaces. |
| `mikan-exporter` | Deterministic frame-exact H.264/AAC MP4 export through the shared evaluator, GPU renderer, audio graph, and FFmpeg. Also exports React entries via `mikan-react-bridge`. |
| `mikan-editor` | GPUI application, editor-owned document state, playback clock, GPU preview bridge, asset panel, timeline, and inspector. |
| `mikan-react-bridge` | Spawns one long-lived `@mikan/react` Node.js process per composition and requests the evaluated `Scene` for each exact frame time over stdin/stdout JSON. |
| `packages/react` (`@mikan/react`, Node.js/TypeScript) | Declarative `Composition`/`Group`/`Image`/`Text` components rendered through a real `react-reconciler` host (hooks, including `useCurrentFrame`/`useVideoConfig`, work); `useProject`/`<ProjectTimeline />` embed a companion project's Rust-evaluated layers. The `mikan-react-render` CLI bundles a JSX/TSX entry with esbuild and emits `Scene`-shaped JSON. |

Important files:

- `crates/editor/src/lib.rs`: `EditorDocument`, clip summaries,
  `TimelineClock`, frame-based clip mutations, and editor tests.
- `crates/editor/src/main.rs`: GPUI window, playback, scrubbing, selection,
  clip dragging/trimming, inspector, and preview refresh.
- `crates/project/src/lib.rs`: serialized project contract.
- `crates/evaluator/src/lib.rs`: project-to-composition behavior.
- `crates/gpu-renderer/src/lib.rs`: GPU rendering and readback APIs.
- `crates/renderer/src/lib.rs`: CPU reference behavior and text rasterizer.

## Architecture boundaries

```text
project.json ─┐
              ├─> Composition model ─> Renderer ─> Preview / Export
React entry ──┘             ▲
                            │
                       GPUI editor
```

Keep these boundaries intact:

- `composition` and `project` must not depend on GPUI, React, FFmpeg, or a GPU
  backend.
- The editor mutates the `Project`; it does not maintain a separate rendering
  truth.
- The evaluator is the only project-to-scene conversion path.
- CPU and GPU renderers consume the same evaluated `Scene`.
- Probed media facts belong in a future editor cache, not the persisted v0
  project file.

## Implemented editor behavior

- Loads a `.mikan.json` path passed on the command line.
- With no argument, loads `examples/editor-demo.mikan.json`.
- Asset list, GPU preview, inspector, transport controls, and timeline panels.
- Integer-frame playhead using the project's exact rational frame rate.
- Play/pause, previous frame, next frame, and stop-at-end behavior.
- Click-and-drag timeline scrubbing with immediate GPU preview refresh.
- Timeline clips positioned using their real project start and duration.
- Clip selection with selected clip details in the inspector.
- Drag a clip body to move it.
- Drag the left or right white edge handle to trim it.
- Clip edits enforce a minimum duration of one frame and remain inside the
  current timeline range.
- Clip-body dragging is always a move operation and preserves the pointer's
  relative grab position, including near either side. Trimming is restricted to
  the selected clip's explicit eight-pixel left/right handle hit areas, which
  show a horizontal resize cursor.
- Clip edits mutate `EditorDocument.project` and refresh the summaries,
  inspector, and evaluated GPU preview.
- Project-snapshot undo/redo history with revision-based clean/dirty tracking.
- A complete pointer drag coalesces into one undo entry.
- Stable pretty JSON serialization and atomic save-to-current-path behavior.
- Dirty state in both the toolbar and native window title.
- Recoverable save errors shown in the toolbar.
- Keyboard shortcuts: Command-S to save, Command-Z / Command-Shift-Z for
  undo/redo, Space for play/pause, and Left / Right for frame stepping.
- Save As with Command-Shift-S. Command-S opens Save As for pathless documents.
- Dirty-window close/quit guard with Save, Don't Save, and Cancel choices.
- Native macOS edited-window state synchronized with the document revision.
- Independently scrollable Assets and Inspector content with fixed panel
  headers; scroll positions persist across editor redraws.
- Single-line, unconstrained text anchors use visible glyph bounds, keeping
  centered titles pixel-aligned in both CPU and GPU renderers. Constrained and
  multiline text retain their layout boxes for alignment and line spacing.
- Audio preview evaluates the shared `AudioGraph`, decodes assets through
  FFmpeg to project-rate stereo PCM, mixes timeline/source offsets, animated
  playback rate and volume, mute state, and overlapping clips, then plays the
  result through rodio synchronized to the editor playback start.
- Audio pauses with transport, scrubbing, frame stepping, and timeline edits;
  audio decode/device failures remain recoverable toolbar errors.
- GPU preview rendering and FFmpeg audio decoding/mixing run on dedicated
  workers rather than the GPUI thread.
- On macOS, even-sized preview frames stay on the GPU: wgpu composites into an
  RGBA intermediate, converts it into the Y and CbCr planes of an
  IOSurface-backed full-range NV12 `CVPixelBuffer`, and GPUI presents that
  buffer through its native Surface element. The two libraries never present
  to the same window surface.
- Native preview initialization or conversion failure falls back to the
  existing GPU readback/`RenderImage` bridge. Odd-sized projects use that
  fallback because NV12 requires even plane dimensions.
- The native bridge receives CoreVideo's Metal textures under the get rule and
  retains them exactly once for wgpu. Each `CVMetalTexture` backing owner is
  held by wgpu's external-resource drop callback so deferred destruction cannot
  leave CoreVideo's texture cache with a dangling Metal object.
- Preview and audio requests carry monotonically increasing generations.
  Queued work is coalesced to the latest request, stale results are discarded,
  and superseded audio mixing stops at cancellation checkpoints.
- Audio and dialogue clips display downsampled peak waveforms aligned to their
  project ranges after the background mix completes.
- Audio-capable track headers expose persistent Mute and Solo controls. Mute
  affects audio without hiding visual layers; when any audible track is soloed,
  non-solo audio tracks are excluded from the shared `AudioGraph`.
- The toolbar exposes a persistent master-volume slider from 0% to 200%. Pointer
  dragging updates it in 1% increments and coalesces into one undo entry. It is
  a tab stop with Arrow keys for 5% changes, Shift-Arrow for 10%, and Home/End
  for 0%/200%. The value is evaluated into `AudioGraph` and applied at final
  mixdown.
- Audio-capable track headers display a live level meter at the current playhead
  position. Background mixing derives per-clip envelopes from source-mapped
  peaks and animated clip volume; active clips are aggregated per track without
  allocating an additional full-length PCM buffer for every track.
- The standalone `mikan-exporter` CLI renders every project frame at its exact
  rational time through `Evaluator` and `GpuRenderer`, mixes the same complete
  `AudioGraph` used by preview, then creates H.264/AAC MP4 through FFmpeg.
  Export work is staged beside the destination, cleaned after failure, and
  atomically published without clobbering an existing file unless explicitly
  requested. The current yuv420p output requires non-zero even dimensions.
- The editor toolbar and Command-Shift-E open a native MP4 destination prompt,
  snapshot the current `Project`, and invoke `mikan-exporter` on a dedicated
  worker. Frame rendering, audio mixing, and muxing progress is visible while
  the UI remains responsive. Cancellation is checked across rendering and
  audio work, terminates the active FFmpeg stream, and removes staged output;
  failures remain recoverable toolbar errors.
- Selecting a Video, Audio, or audio-backed Dialogue clip exposes its 0%-200%
  volume in the Inspector. The +/- controls adjust static volume in 5% steps,
  or upsert a keyframe at the current clip-local playhead position once
  automation is active. Add/Update, Remove, and Flatten controls author the
  `Animatable<f64>` project representation directly. Dialogue's optional
  `volume` field is backward compatible with existing v0 projects. All changes
  are undoable and immediately remix the audio preview and level envelope.
- Track Mute/Solo and master-volume changes participate in project-snapshot
  undo/redo and dirty tracking.
- The Assets panel imports multiple local video, audio, supported image, and
  font files through a native picker (`Command-I`). A batch is validated before
  mutation and becomes one undo entry; unsupported files leave the project
  unchanged.
- Imported files inside a saved project's directory use stable `./...`
  references. External files and imports into an untitled document use absolute
  paths so the first Save As does not silently change their target.
- Selecting an asset enables Add (`Command-Return`), which inserts an exact-frame
  clip at the playhead using probed source duration. A five-second fallback is
  used while metadata is unavailable. Compatible unlocked tracks are reused;
  otherwise Video, Audio, or Overlay tracks are created. New visual clips start
  centered on the canvas.
- Asset rows can be dragged directly onto a timeline track. The pointer's drop
  position determines the exact insertion frame; compatible tracks highlight
  green and incompatible or locked tracks show a red rejection state. Dropping
  into an empty timeline creates a compatible track.
- Clicking a track row explicitly selects it as the Add target. Clicking it a
  second time clears the selection and restores automatic compatible-track
  reuse/creation. Explicit targets are validated in `EditorDocument`, so Add and
  drag/drop cannot silently fall back to another track.
- The Timeline header can create empty Video, Audio, Overlay, and Dialogue
  tracks. Track headers provide Up/Down ordering controls; locked tracks reject
  reordering. Creation and ordering are project mutations with undo/redo.
- Dragging a clip body vertically highlights compatible unlocked tracks in
  green and incompatible or locked tracks in red. Releasing over a compatible
  target moves the serialized timeline item while preserving its exact range;
  simultaneous horizontal and vertical movement is one undo entry.
- Timeline track rows scroll independently below the fixed Timeline header and
  scrubber, allowing manually created tracks to remain reachable.
- Selecting a track exposes Enable/Disable, Lock/Unlock, Rename, and Delete in
  the Inspector. Locked tracks allow only Unlock; their rename, enabled state,
  ordering, clips, and deletion remain protected. Inline rename uses GPUI's
  `EntityInputHandler`, including platform IME composition, UTF-16/UTF-8 range
  conversion, grapheme-aware cursor movement and deletion, mouse/Shift
  selection, Home/End, clipboard shortcuts, and the character palette. It
  commits with Enter/Save and cancels with Escape/Cancel.
- Empty tracks delete immediately. Deleting a track with clips requires an
  explicit warning confirmation and removes the serialized items only after
  approval. The document API rejects non-empty deletion unless the caller opts
  in, so this safety does not depend solely on the UI.
- Track rename, enabled/locked toggles, and deletion participate in undo/redo.
  Track enabled state immediately refreshes both visual and audio evaluation;
  deleting a dynamic-duration track recalculates the timeline duration.
- Selected clips can be deleted from the Timeline header or with Backspace /
  Forward Delete. Insert/delete operations are undoable, derived timelines
  recalculate duration, and locked tracks reject both deletion and drag edits.
- A dedicated FFprobe worker caches duration, video dimensions, and audio-stream
  presence by asset ID for the editor session. Assets show this metadata or a
  probe-failure state without persisting probed facts in v0 JSON.
- The audio worker caches decoded PCM by path, project sample rate, and channel
  count. Subsequent edits remixes cached samples instead of invoking FFmpeg for
  every audio asset again.
- Decoded PCM and 512-bucket source peaks are persisted in a versioned binary
  cache (`~/Library/Caches/com.natsuneko.mikan/audio-v1` on macOS). Keys include
  canonical path, file identity, size, mtime, sample rate, and channel count.
  Cache corruption, staleness, and read/write failures are recoverable misses;
  the worker falls back to FFmpeg and rewrites the entry without surfacing an
  editor failure.
- The persistent cache is capped at 1 GiB. Successful reads update entry mtime
  as a last-used marker; after each store, `.pcm` sizes are totaled and the
  oldest entries are removed until within the limit. Maintenance errors remain
  non-fatal.
- Waveform peaks are cached per source asset and mapped to each audible clip,
  replacing the previous complete-mix waveform approximation.
- Clip waveforms map each timeline bucket through `sourceRange.start`, optional
  `sourceRange.duration`, and the integrated playback-rate animation before
  sampling cached source peaks. The audio graph retains source duration and the
  mixer enforces the same exclusive source-range end, keeping waveform and
  playback behavior aligned.
- Local asset paths are checked when summaries refresh. Missing files appear in
  red in the Assets panel with an explicit Relink-required diagnostic, including
  images and fonts that FFprobe does not inspect.
- Relink replaces an asset's source through a single-file native picker while
  enforcing the original video/audio/image/font kind. It participates in
  undo/redo and invalidates media/PCM cache generations.
- Unreferenced assets can be removed directly. Referenced assets show a warning
  with affected clips/characters; confirmed cascading removal deletes direct
  media clips, detaches Dialogue audio, repairs character default expressions,
  and clears invalid Dialogue expression overrides so project validation remains
  successful. The whole cascade is one undo entry.

Projects opened from the command line save back to their current path. The
built-in editor demo has no file path, so its first Command-S opens Save As.

## React composition integration

The first vertical slice from `project.json` to a video also exists for React
entries, matching the architecture diagram's "React entry" path:

- `packages/react` is a TypeScript package managed with pnpm. `@mikan/react`
  exports `Composition`, `Group`, `Image`, and `Text` components (typed props
  in `src/components.ts`; `src/scene.ts` re-exports the `Scene`/`Layer`/...
  types from `src/generated/`, ts-rs bindings generated from
  `mikan_composition`, rather than hand-mirroring the JSON shape).
- The entry's default export must render a single root `<Composition width
  height fps durationInFrames>` element. Layer ids default to a
  path-based string (for example `root.0.1`) stable across repeated renders of
  the same tree shape, or an explicit `id` prop.
- `pnpm run build` compiles `src/*.ts` to `dist/*.js` (plain CommonJS, plus
  `.d.ts`) with `tsc`; `mikan-react-bridge` and `mikan-exporter` spawn
  `dist/cli.js`, not the TypeScript sources. `dist/` is gitignored like
  `node_modules/` and `.tmp/`, so it must be rebuilt after checkout.
- `mikan-react-render` (`packages/react/src/cli.ts`, compiled to
  `dist/cli.js`) is the Node.js CLI: it bundles the given entry with esbuild
  (`jsx: automatic`, entry's own `@mikan/react` import kept external so the
  same component-marker objects are compared, not a bundled duplicate),
  writes the bundle beside the package under `.tmp/` (self-reference
  resolution needs the bundle to live inside the package directory tree),
  prints one `{"config": ...}` line with width/height/frameRate/
  durationInFrames, then answers one `{"time": ...}` request per line with
  `{"scene": ...}` or `{"error": ...}`. esbuild transpiles the user's entry
  file directly (TS or TSX) without type-checking it; `@mikan/react`'s own
  source is type-checked by `pnpm run build`.
- `mikan-react-bridge` spawns and owns this Node process for the lifetime of
  an export or preview, mirroring `mikan-media`'s one-process-per-composition
  sequential decoding session rather than spawning Node per frame.
- `mikan-exporter --react <entry> <output.mp4>` renders every frame of the
  composition through the same `GpuRenderer` used for projects and encodes it
  with FFmpeg. There is no audio graph for React entries yet, so the encoded
  video is published directly without FFmpeg's separate mux stage.
- `<Video>` is intentionally not implemented yet; only `Group`, `Image`, and
  `Text` layers are supported from React.
- React entries do not yet integrate with the GPUI editor (no project
  persistence, timeline/track placement, or preview panel); see Recommended
  next work.

### react-reconciler, hooks, and `<ProjectTimeline />`

The initial React-composition slice walked the JSX element tree directly
(calling function components itself, matching `Composition`/`Group`/`Image`/
`Text` by object identity) rather than using React's own reconciliation.
That has been replaced with a real `react-reconciler` (`^0.29.2`, pinned to
match `react@^18.3.1` — the reconciler's own peer dependency; do not bump
either independently) host in `src/reconciler.ts`. `Composition`/`Group`/
`Image`/`Text` (`src/components.ts`) are now thin wrappers around
`React.createElement('composition' | 'group' | 'image' | 'text', props)`;
the reconciler's host config (mutation mode; a plain `{type, props,
children}` tree, no real host platform) turns those into instances, and
`src/render.ts` walks that resulting instance tree (not JSX elements) into
`Layer[]`. This means ordinary React composition — conditionals, `.map()`,
context, and now hooks — works through user components exactly as it would
in any other React host.

- **The mount is persistent.** `render.ts`'s `mount(defaultExport)` creates
  one root via `reconciler.ts`'s `createRoot()` and keeps it for the whole
  process; `cli.ts` calls `mounted.renderAt(time, projectLayers)` once per
  frame request against that same root (via `HostReconciler.flushSync(() =>
  updateContainer(...))`, `LegacyRoot` mode for synchronous, un-batched
  commits). This is required for hook state to mean anything: `useState`
  persisting across frames was verified manually (a counter that only grows,
  read back down after moving the request time backward, stays at its
  high-water mark — proving state survives across `renderAt` calls on the
  same root, not just within one).
- **`useCurrentFrame()`, `useCurrentTime()`, `useVideoConfig()`**
  (`src/hooks.ts`) read a `CompositionRuntimeContext` that `render.ts`
  provides around the entry on every `renderAt` call, carrying that call's
  `time` plus the composition's own static width/height/fps/durationInFrames
  (read once, on a first bootstrap pass — see below). `useCurrentFrame()`
  computes `round(time.value / time.timescale * fps)`.
- **The bootstrap chicken-and-egg problem**: reading `<Composition>`'s own
  props requires rendering the tree once, but the tree's children may call
  `useCurrentFrame()`/`useVideoConfig()` or render `<ProjectTimeline />`
  before real values exist for any of that. `mount()`'s first pass therefore
  provides a placeholder `CompositionRuntimeContext` (zeros, `fps: 1`) and an
  empty project-layers array rather than leaving them unset (which throws) —
  its own *output* layers are discarded; only `<Composition>`'s width/
  height/fps/durationInFrames props, which must be static, are read from it.
- **`external` in `cli.ts`'s esbuild call now also covers `react` and
  `react/jsx-runtime`/`react/jsx-dev-runtime`, not just `@mikan/react`.**
  Without this, the entry's bundle gets its own copy of React with its own
  internal dispatcher slot, separate from the one this process's
  `react-reconciler` actually sets — hooks then fail at runtime with React's
  "Invalid hook call" warning (reproduced and fixed during this work). All
  of `react`, its jsx-runtime, and `@mikan/react` need to resolve to the
  exact module instances this process already loaded, which is only
  possible because the entry's bundle is written inside
  `packages/react/`'s own directory tree (Node's package self-reference
  resolution) rather than to the OS temp directory.
- **`<ProjectTimeline />` and its Rust-side counterpart.** `useProject()`
  (`src/project-runtime.ts`) reads a `Project` from `ProjectContext`, which
  the entry populates explicitly with `<ProjectProvider project={...}>`
  (typically `project={loadProject('./project.json')}`) — there is no
  implicit project loading in the CLI. `<ProjectTimeline />` is different: it
  cannot evaluate anything itself. It reads a `ProjectLayersContext` that
  `render.ts` provides per `renderAt` call, sourced from Rust, and emits
  those layers through a `rawLayers` host type that `render.ts`'s walker
  splices directly into the output `Layer[]` at that position — no
  transformation, since Rust already fully evaluated them.
  - **Why Rust evaluates instead of Node asking Rust mid-render**: the
    bridge protocol (`mikan-react-bridge`) is a synchronous one-request-per-
    line pipe where Rust always initiates and blocks on Node's response. If
    `<ProjectTimeline />` tried to ask Rust to evaluate while rendering,
    Rust would already be blocked waiting for *this* response and could
    never service that nested request — deadlock. Instead,
    `ReactBridge::scene_at_with_project(time, Option<&[Layer]>)`
    (`mikan-react-bridge`) embeds the already-evaluated layers in the
    request itself: `{"time": ..., "project": {"layers": [...]}}`
    (`project` omitted entirely when there is no companion project, via
    `skip_serializing_if`).
  - **`mikan-exporter`**: `Exporter::export_react_entry_with_project[
    _and_progress/_cancellable]` take a `CompanionProject { project,
    project_asset_root }` alongside the entry. Internally,
    `visual_only_project()` clones the project and drops every timeline item
    whose content isn't `Video`/`Image`/`Text` (v1 scope: no `audio`,
    `dialogue`, or `component` — dialogue/component are dropped because
    their evaluator-expanded output would otherwise appear even though the
    entry never asked `<ProjectTimeline />` for it; audio produces no visual
    layers anyway so dropping it is a no-op either way) before constructing
    an `Evaluator` and calling `scene_at(time)` once per frame, same as
    plain project export. When a companion project is present, the
    `GpuRenderer` also gets a sequential-video decoder attached (project
    `Video` content needs it; a React entry alone never does, since
    `<Video>` isn't implemented — see above).
  - **Two different asset roots, one renderer.** `GpuRenderer` resolves
    every relative asset path against a single `asset_root`
    (`crates/gpu-renderer/src/lib.rs`'s `local_asset_path`), which stays set
    to the React entry's own directory. A companion project's assets can
    live somewhere else entirely, so `absolutize_layers`/
    `absolutize_fonts`/`absolutize_asset` (`mikan-exporter`) rewrite the
    project-evaluated `Layer`s' (and `Scene.fonts`') relative `File` paths
    into absolute ones (joined against `project_asset_root`) before they are
    sent to Node — `local_asset_path` already left absolute paths alone, so
    this needed no `GpuRenderer` changes. Project fonts are evaluated once
    (not per frame, since they do not vary by time) and merged into every
    frame's `Scene.fonts` after Node responds.
  - **CLI**: `mikan-exporter --react <entry> --project <project.mikan.json>
    <output.mp4>` loads the project relative to its own path (its parent
    directory becomes `project_asset_root`) and requires `--react`;
    `--project` without `--react` is a usage error.
  - Verified end to end (`packages/react/examples/with-project.tsx`, a
    `<ProjectTimeline />` alongside a React-authored `<Text>`, against a
    project with one `text` timeline item): the exported frame shows both
    the React-authored text and the project-evaluated text together. Also
    covered by an integration test
    (`crates/react-bridge/tests/node_integration.rs`,
    `embeds_pre_evaluated_project_layers_into_project_timeline_when_node_is_available`)
    that calls `scene_at_with_project` directly with a synthetic `Layer` and
    asserts it comes back untouched alongside the entry's own content.
- **`interpolate()` and `spring()`** (`src/animation.ts`) are plain
  functions over a frame/time number, no reconciler or hooks involved — they
  compose with `useCurrentFrame()`/`useVideoConfig()` but do not depend on
  them. `interpolate(input, inputRange, outputRange, options?)` matches the
  usual Remotion-shaped contract: piecewise-linear by default, an optional
  per-segment `easing`, and `extrapolateLeft`/`extrapolateRight` (`'extend'`
  default, `'clamp'`, `'identity'`) for input outside the given range.
  `Easings` (plural — the generated `Easing` union type from
  `mikan_composition`, an unrelated project.json-facing concept, already
  used that name) provides `linear`/`easeIn`/`easeOut`/`easeInOut` curves.
  `spring({frame, fps, config?, from?, to?, delay?, durationInFrames?})` is
  the closed-form step response of a damped harmonic oscillator (mass-
  spring-damper solved analytically for the underdamped/critically-damped/
  overdamped cases), not a physics simulation stepped frame by frame — this
  matters because it means any single requested frame can be evaluated
  directly, with no dependency on the frames before it, which fits how this
  renderer works (each `renderAt` call is an independent frame request, not
  guaranteed to arrive in order). `durationInFrames` clamps late frames to
  the settled value rather than fitting the spring's stiffness to finish by
  exactly that frame (Remotion does the latter; not implemented here).
  Verified manually end to end: a `<Text>` combining `spring()` (scale, with
  reduced damping so the overshoot is visible) and `interpolate()`
  (position) rendered through `mikan-exporter --react` shows the text
  entering from the left, bouncing past full scale, and settling — matches
  the JSON values spot-checked in isolation via the CLI's stdin/stdout
  protocol directly (scale reaches ~1.25 at frame 10 before settling near 1
  by frame 30, position moves from 100 to 320 and clamps at the composition
  width beyond frame 60 per `extrapolateRight: 'clamp'`).
- **`useProjectProperty(key, defaultValue)`** (`src/project-runtime.ts`,
  alongside `useProject()`) reads `Project.properties`
  (`Record<string, JsonValue>`, already generated — no Rust changes needed
  for this) and falls back to `defaultValue` when the key is absent. There
  is no schema: no declared type, no validation, no Inspector generation. A
  property the editor renamed or retyped silently falls back rather than
  erroring — this is the design doc's "React API の簡易形", not the
  schema-based `defineProjectProperties`/generated-Inspector-fields version
  it also describes as the eventual goal, which is deferred (see below;
  `Property Value = project.json`/`Property schema = TypeScript source` are
  two different things, and only the former exists yet). Verified manually:
  a project with `{"title": "Chapter 3", "episode": 3}` in `properties`,
  read back through `useProjectProperty` inside a `<ProjectProvider>`,
  including a missing key correctly falling back to its default.
- **Component registry** (`src/registry.ts`, resolved by
  `<ProjectTimeline />` in `src/project-runtime.ts`). A project.json
  `TimelineContent::Component { component, props }` item has no meaning to
  `mikan-evaluator` — Rust has no registry, so it always evaluates that
  content to `LayerContent::MissingComponent { component, props }` (see
  `crates/evaluator/src/lib.rs`), and `visual_only_project()`
  (`mikan-exporter`) now keeps `component` items through its filter
  (previously dropped, alongside `dialogue`, in the earlier v1 pass —
  `dialogue` is still dropped). `registerComponent(name, Component)`
  (called at module scope in the entry, so registration happens before any
  frame renders — the registry is a plain process-global `Map`, safe
  because one `mikan-react-render` process only ever handles one entry) is
  how the entry supplies what Rust cannot. `<ProjectTimeline />` walks the
  layers Rust evaluated; for each `missingComponent` layer, it looks up
  `component` in the registry:
  - **Resolved**: renders `<RegisteredComponent {...props} />` as a real
    subtree — the registered component's own hooks/state work normally,
    since it becomes part of the same persistent reconciler tree as
    everything else — wrapped in a `group` carrying the *original*
    evaluated `transform`/`opacity` from the timeline item (a
    `rawTransform`/`rawOpacity` escape hatch added to `render.ts`'s
    `extractTransform`/`buildLayer`, recognized only on internally-constructed
    elements, not part of the public `GroupProps` type), so the resolved
    content lands exactly where the project placed it regardless of what
    the component itself renders.
  - **Unresolved** (no matching `registerComponent()` call): the
    `missingComponent` layer passes through unchanged. `GpuRenderer` then
    errors on it (`GpuRenderError::UnsupportedContent`) — this is the
    correct, honest outcome for export; the design doc's "Editor では
    Missing Component として警告表示できるようにしたい" is about the GPUI
    editor's own preview, not this export path, and remains unbuilt.
  - Verified end to end
    (`packages/react/examples/with-registered-component.tsx`, registering
    `BossIntroduction`) both ways: a project with a registered
    `component: "BossIntroduction"` item renders its resolved `<Text>`
    (`"Golem (Lv.42)"` from `props: {bossName, level}`) through
    `mikan-exporter --react --project`, and a project with an unregistered
    component name fails the export with the expected `GpuRenderError`.
    Also covered by an integration test
    (`crates/react-bridge/tests/node_integration.rs`,
    `resolves_a_registered_component_when_node_is_available`) asserting the
    resolved layer becomes a `group` wrapping the rendered `Text`, and the
    unresolved one stays a `missingComponent` layer with its original
    `component` name.
  - **Not yet built**: `dialogue` content in `<ProjectTimeline />`. A
    `<Video>` React component. An `AudioGraph` source for React entries.
    Schema-based Project Properties (`defineProjectProperties`, GUI
    Inspector generation).
- **Component Property Schema** (`src/registry.ts`). Scoped down with the
  user to the TypeScript-side declaration API only — no GPUI editor
  integration yet (that would need the editor to query a running Node
  process for registered schemas, a new kind of Node dependency for a
  currently Node-independent editor, and was explicitly deferred rather
  than decided against). `registerComponent(name, component, schema?)`
  takes an optional third argument, a `ComponentPropertySchema<Props>` — a
  `{[K in keyof Props]: ComponentPropertyField}` record where each field is
  one of `{type: 'string'|'boolean'|'color', label?, defaultValue}`,
  `{type: 'number', label?, defaultValue, min?, max?, step?}`, or
  `{type: 'select', label?, defaultValue, options}`. It is pure metadata:
  neither `<ProjectTimeline />`'s resolution nor `GpuRenderer` reads it, and
  a component registered without one resolves and renders exactly as
  before — this only future-proofs the registry call for a GUI Inspector
  that does not exist yet. `getComponentSchema(name)` (also exported from
  `@mikan/react`, alongside the `ComponentPropertyField`/
  `ComponentPropertySchema` types) looks it up, returning `undefined` for
  both an unregistered name and a registered one with no schema — callers
  that need to tell those apart check `resolveComponent(name)` (internal,
  not exported) first. `crates/react-bridge`/`mikan-evaluator` are
  untouched: `TimelineContent::Component`'s `props` stays opaque JSON to
  Rust regardless of whether the entry declared a schema for it.
  Verified: `packages/react/examples/with-registered-component.tsx` now
  declares a `bossIntroductionSchema` for `BossIntroduction`'s `bossName`/
  `level` props (`tsc` type-checks the schema against the component's
  actual props type), a manual Node smoke test against the built
  `dist/index.js` confirmed `getComponentSchema` round-trips a registered
  schema, returns `undefined` for a schema-less registration and for an
  unregistered name, and the existing
  `resolves_a_registered_component_when_node_is_available` integration
  test still passes unchanged (rendering never reads the schema).
- **Per-track access** (`useProjectTrack()`, `<ProjectTrack id="..." />`,
  `src/project-runtime.ts`). `<ProjectTimeline />` embeds every track's
  layers flattened together, which is fine for the whole-composition case
  but gives an entry no way to read or re-embed just one track — needed new
  evaluator-side surface, not just a new React component, since
  `mikan-evaluator::Evaluator::scene_at` only ever evaluated all tracks
  together.
  - `Evaluator::layers_for_track(track_id, time)`
    (`crates/evaluator/src/lib.rs`) evaluates one track's active visual
    layers at an exact time, independent of the others; both it and
    `scene_at` now share an `active_track_layers(track, time)` helper. An
    unknown `track_id`, or a track disabled at the project level, evaluates
    to an empty `Vec` rather than an error — from the caller's perspective
    both are just "nothing to show."
  - `mikan-exporter`'s `render_react_video` per-frame loop now evaluates
    every track in the filtered companion project unconditionally, into a
    `BTreeMap<String, Vec<Layer>>` (Rust cannot statically know which track
    ids an entry's JSX will reference, so this avoids any negotiation
    handshake with Node at the cost of always doing the per-track work —
    acceptable given typical track counts), applying the same
    `absolutize_layers` asset-root rewrite used for the flat `layers`
    embed. This travels alongside the existing flat layers in a new
    `ProjectFrame<'a> { layers: &'a [Layer], tracks: &'a BTreeMap<String,
    Vec<Layer>> }` struct (`crates/react-bridge/src/lib.rs`), which replaced
    `scene_at_with_project`'s old `Option<&[Layer]>` parameter with
    `Option<ProjectFrame<'_>>`.
  - On the TypeScript side, a new `ProjectTrackLayersContext` (parallel to
    the existing `ProjectLayersContext`) carries the per-track map;
    `useProjectTrack(trackId)` reads it and returns `tracks[trackId] ?? []`,
    and `<ProjectTrack id="..." />` renders that track's layers the same way
    `<ProjectTimeline />` renders the flat list — both now share a
    `renderProjectLayers(layers)` helper (including `missingComponent`
    resolution through the same registry) instead of duplicating it.
    `mount()`'s bootstrap render pass and `renderAt()` provide both
    Provider values from a single `ProjectFrame` argument (`{ layers,
    tracks }`, empty during bootstrap) so the two contexts stay consistent.
  - Verified end to end (`packages/react/examples/with-project-track.tsx`,
    a project with `titles` and `overlays` tracks): `useProjectTrack('titles')`
    correctly reported only that track's layer count, `<ProjectTrack
    id="overlays" />` rendered that track's content at its project-evaluated
    position, and neither track's actual layer content leaked into the
    other — confirmed via `mikan-exporter --react --project` producing an MP4
    with the expected on-screen text, and by a Rust integration test
    (`crates/react-bridge/tests/node_integration.rs`,
    `embeds_per_track_layers_for_use_project_track_when_node_is_available`).

### TypeScript type generation and the Project loader

`mikan-composition`'s and `mikan-project`'s public serde types carry
`#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]` plus `#[cfg_attr(feature
= "codegen", ts(export))]`, gated behind a `codegen` cargo feature so `ts-rs`
is not a dependency of default builds (editor, exporter, etc. build exactly
as before). `pnpm run codegen` in `packages/react` (or directly, `cargo test
-p mikan-project -p mikan-composition --features codegen`) runs the tests
`ts-rs`'s derive macro generates, one per exported type, each of which writes
a `.ts` file. `.cargo/config.toml` sets `TS_RS_EXPORT_DIR` so those land in
`packages/react/src/generated/` (workspace-relative, harmless when `codegen`
is off) and `TS_RS_LARGE_INT = "number"` so `Time.value` (`i64`) binds to
`number`, not `bigint` — `serde_json` has no bigint literal, so a `bigint`
binding would not round-trip through the stdin/stdout JSON protocol
`mikan-react-bridge` uses. `serde-json-impl` is enabled so `PropertyValue`
(`serde_json::Value`) binds to a `JsonValue` type under
`generated/serde_json/`. `src/generated/` is gitignored, like `dist/`, and
regenerated from Rust rather than committed.

`packages/react/src/scene.ts` now re-exports the render-output side (`Scene`,
`Layer`, `LayerContent`, `Time`, `Rational`, ...) from `src/generated/`
instead of hand-mirroring it, and `src/project.ts` adds a
`loadProject(path)` / `loadProjectFromString(json)` reader for `.mikan.json`
files, typed against the generated `Project`. This loader is read-only and
intentionally thin (`JSON.parse` plus a type assertion): the canonical
validation is `mikan-project`'s `Project::load`/`Project::from_json`, and a
project loaded this way is expected to already be valid. Nothing yet
connects a loaded `Project` into a `<Composition>` tree — there is no
`<ProjectTimeline />`, `<ProjectTrack />`, or `useProject()` — so this is
purely a typed-read building block for that future integration, not usable
from a React entry yet.

**Bug found and fixed while adding this**: `LayerContent`, `AssetLocation`,
`Asset`, `AssetSource`, `Paint`, and `TimelineContent` are `#[serde(tag =
"type", rename_all = "camelCase")]` enums with struct variants. `rename_all`
on an enum only renames *variant names* (`video`, `dialogue`, ...); it does
not rename the *fields inside* struct variants. `TimelineContent`'s
`source_range`/`playback_rate` and `LayerContent::Text`'s `max_width` were
therefore actually serialized/deserialized as snake_case in JSON, contrary
to the camelCase convention documented and used everywhere else in the
project — confirmed empirically (`serde_json::to_string` on a constructed
`TimelineContent::Video`, and deserializing a `LayerContent::Text` JSON
object with `"maxWidth"` vs `"max_width"`). This silently broke the `<Text
maxWidth>` React prop added earlier in this same session: the JSON sent
`"maxWidth"`, Rust ignored it as an unknown field, and `max_width` stayed
`None`. Fixed by adding `rename_all_fields = "camelCase"` (stable since serde
1.0.194; this workspace pins 1.0.229) alongside `rename_all` on all six
affected enums, which renames fields across every variant while `rename_all`
keeps renaming the tag values. Re-verified end to end: `mikan-exporter
--react` on an entry using `<Text maxWidth={200}>` now visibly wraps the
text. No existing test exercised these optional fields through a JSON round
trip, so nothing else needed updating, but it is worth being alert to
similar `rename_all`-without-`rename_all_fields` mistakes if more tagged
enums with struct variants are added.

## Preview integration decision

GPUI owns its macOS Metal presentation surface. Creating and presenting a
second `wgpu::Surface` for the same GPUI window would introduce conflicting
surface ownership. The editor therefore uses a single-presenter native bridge:

1. evaluates the project into a `Scene`;
2. renders the scene into an offscreen RGBA texture with `GpuRenderer`;
3. converts RGBA to full-range BT.601 NV12 in two GPU render passes;
4. renders into IOSurface-backed CoreVideo planes wrapped as wgpu textures;
5. gives the completed `CVPixelBuffer` to GPUI's Surface element for display.

The bridge waits for wgpu's command submission before handing the shared buffer
to GPUI, but performs no pixel readback or CPU color conversion. Bridge setup or
rendering failures and odd project dimensions retain the previous offscreen
`GpuFrame` readback, RGBA-to-BGRA conversion, and `Arc<RenderImage>` path.
`GpuRenderer::render_to_surface` remains useful for a separate, renderer-owned
preview window.

GPUI uses its `runtime_shaders` feature on macOS. Without it, GPUI's build
script requires the separately downloadable Xcode Metal Toolchain component.

## Critical GPUI constraint and fixed crash

Do not call `Window::request_animation_frame()` from a mouse/click handler.
GPUI 0.2.2 only permits it during request-layout, prepaint, or paint. Calling it
from the Play mouse-up callback caused a Rust panic to cross the Objective-C
event boundary and terminate the process with SIGABRT.

The correct flow is:

1. the click handler changes playback state and calls `cx.notify()`;
2. `EditorView::render` calls `update_playback`;
3. `update_playback`, now inside rendering, calls
   `window.request_animation_frame()`.

The crash was reproduced with `RUST_BACKTRACE=full`, fixed, and the VOICEROID
project was then automatically played beyond its 5-second dialogue start
without crashing.

## Example projects

- `examples/minimal.mikan.json`: smallest valid empty project.
- `examples/editor-demo.mikan.json`: self-contained 10-second title clip used
  by the default editor launch.
- `examples/voiceroid.mikan.json`: dialogue from 5 to 8 seconds, portrait,
  subtitle styling, and a voice asset reference.
- `examples/assets/akane/default.ppm`: tiny built-in portrait placeholder so
  the visual VOICEROID path works without downloads.

`examples/assets/voices/001.wav` is a 48 kHz stereo PCM spoken fixture generated
from the macOS Kyoko system voice. It is an executable example asset, not a
licensed VOICEROID voice sample.

- `packages/react/examples/title.tsx`: a five-second, single-`Text` React
  composition used by `mikan-react-bridge`'s integration test and as the
  `mikan-exporter --react` example.
- `packages/react/examples/with-project.tsx`: `<ProjectTimeline />` alongside
  a React-authored `<Text>`, used by `mikan-react-bridge`'s
  project-companion integration test and the `mikan-exporter --react
  --project` example.
- `packages/react/examples/with-registered-component.tsx`: registers a
  `BossIntroduction` component (with a `ComponentPropertySchema` declaring
  its `bossName`/`level` props) and renders `<ProjectTimeline />`, used by
  `mikan-react-bridge`'s component-registry integration test.
- `packages/react/examples/with-project-track.tsx`: reads one track via
  `useProjectTrack('titles')` and renders another whole via `<ProjectTrack
  id="overlays" />`, used by `mikan-react-bridge`'s per-track integration
  test.

## Validation baseline

At this handoff, the workspace has 88 passing tests (85 from the previous
handoff plus two `mikan-evaluator` per-track unit tests and
`mikan-react-bridge`'s new per-track integration test). The last checks
were:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check
cargo test -p mikan-project -p mikan-composition --features codegen
cargo clippy -p mikan-composition -p mikan-project --all-targets --features codegen -- -D warnings
```

The `codegen`-feature checks add no new test count (the ts-rs-generated
`export_bindings_*` tests just write files as a side effect) but are worth
running whenever composition/project types change, since that is what
regenerates `packages/react/src/generated/`.

`mikan-react-bridge`'s integration test spawns the real `mikan-react-render`
CLI and skips itself with a message if `node` is not on `PATH` or if
`packages/react/node_modules` or `packages/react/dist` do not exist yet (run
`pnpm install && pnpm run codegen && pnpm run build` there first).
`mikan-media`'s FFmpeg integration tests use the same skip-if-missing pattern
for `ffmpeg`/`ffprobe`.

The editor and VOICEROID example were also launched successfully:

```sh
cargo run -p mikan-editor
cargo run -p mikan-editor -- examples/voiceroid.mikan.json
```

A full React-entry export was also run end to end and its output frame was
inspected: `cargo run -p mikan-exporter -- --react
packages/react/examples/title.tsx output.mp4` produced a 150-frame, 1920x1080
H.264 MP4 whose first decoded frame shows the expected centered white title
text on the composition's background. A project-companion export (`--react
packages/react/examples/with-project.tsx --project <a project.json with one
text timeline item>`) was also run end to end and its output frame inspected:
both the React-authored text and the project-evaluated text appear in the
same frame. `useState` persisting across frames (a counter that only grows,
checked by moving the requested time backward and confirming the value does
not drop) was also verified manually via the CLI's stdin/stdout protocol
directly.

There are future-incompatibility warnings in transitive dependencies
`block 0.1.6` and `proc-macro-error2 2.0.1`; these are not current Mikan lint or
test failures.

The frame-based mutation model and its boundaries are unit-tested. Native
window startup is verified, but automated pointer-drag testing is not yet
available because the current environment lacks macOS Accessibility permission
for synthetic GUI input. Keep drag behavior easy to exercise manually.

## Recommended next work

The initial editor mutation, persistence, audio, background-worker, native
preview, deterministic export, editor export-control, and sequential-decoding
export milestones are complete. The minimal React-composition-to-MP4 vertical
slice (`packages/react`, `mikan-react-bridge`, `mikan-exporter --react`) is
also complete, as is Rust-to-TypeScript type generation and a read-only
`loadProject()` (see "React composition integration" above).

The user has shared a more ambitious design (see git history / conversation
for the full text) where a React entry does not just describe an independent
scene, but can read and re-embed GUI-editor-owned `project.json` content
(`<ProjectTimeline />`, `<ProjectTrack />`, `useProject()`), where
`project.json` can place a React component instance on the timeline
(`TimelineContent::Component`, which already exists in the format — see
`crates/project/src/lib.rs`), and where GUI-editable values are exposed from
React via an explicit Property Schema (`defineProjectProperties`,
`useProjectProperty()`). That design's own priority order, adjusted for what
is already done:

1. ~~Rust Project type generation~~, ~~Project loader~~, ~~React context +
   `useProject()`~~, ~~`<ProjectTimeline />`~~, ~~`useCurrentFrame()` /
   `useCurrentTime()` / `useVideoConfig()`~~ (which did mean adopting
   `react-reconciler`, as anticipated) — done, see "react-reconciler, hooks,
   and `<ProjectTimeline />`" above.
2. ~~`useProjectTrack()` / `<ProjectTrack />` (per-track access)~~ — done,
   see "Per-track access" above.
3. ~~`interpolate()` / `spring()` animation utilities~~ — done, see
   "react-reconciler, hooks, and `<ProjectTimeline />`" above.
4. ~~Project Properties~~ — `useProjectProperty(key, defaultValue)` (the
   design doc's "React API の簡易形") is done; `defineProjectProperties`
   (schema-based, generating Inspector fields) remains deferred, see below.
5. ~~Component registry~~ — `registerComponent()`/`<ProjectTimeline />`
   resolving `TimelineContent::Component`'s `component`/`props` to a
   registered React component is done, see "Component registry" above.
6. ~~Property schema / Inspector metadata for GUI-editable component
   props~~ — the TypeScript-side declaration API
   (`registerComponent(name, component, schema)`, `getComponentSchema()`)
   is done, see "Component Property Schema" above; GPUI editor integration
   (the editor actually reading a schema and rendering Inspector fields
   from it) was scoped out for now — it would be the editor's first Node.js
   dependency, and stays open for a future session.
7. `dialogue` timeline content in `<ProjectTimeline />` (dropped in the v1
   filter — see above — deliberately; `component` content is no longer
   dropped, per item 5).

Separately, still open from the original slice:

- A `<Video>` component in `packages/react`, matching `LayerContent::Video`'s
  `MediaTiming` (local time, source start, source time in seconds, playback
  rate) the way project timeline clips already do.
- An `AudioGraph` source for React entries so `mikan-exporter --react` can mux
  audio instead of always publishing a silent MP4.
- GPUI editor integration for the Component Property Schema (item 6 above):
  querying a running Node process for registered components' schemas and
  rendering Inspector fields from them.

Before starting item 7 or anything further down this list, confirm scope
with the user rather than assuming the full design doc.

Do not optimize preview presentation by letting GPUI and wgpu both present to
the same window surface.

## Working conventions

- Preserve unrelated or pre-existing worktree changes.
- Use targeted validation while iterating, then the full baseline above.
- Do not commit unless the user explicitly asks.
- If a commit is requested, stage only accepted files.
