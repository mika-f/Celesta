# Mikan implementation handoff

Last updated: 2026-08-31 (React transitions, layout, media preload, and debug guides)

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
- FFmpeg 7.1+ **development libraries** must be available for `ez-ffmpeg` /
  `ffmpeg-sys-next` to link against — the `ffmpeg`/`ffprobe` binaries are no
  longer used at runtime. On Windows: `vcpkg install
  ffmpeg[x264]:x64-windows-static-md` with `VCPKG_ROOT` set (the workspace
  enables `ez-ffmpeg`'s `static` feature). macOS: `brew install ffmpeg` +
  `pkg-config`. Linux: the distro `libav{codec,format,filter,device,util}-dev`,
  `libsw{scale,resample}-dev` packages.
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
| `mikan-media` | Metadata probing and exact-time RGBA video-frame decoding via the linked FFmpeg libraries (`ez-ffmpeg`) behind `VideoFrameDecoder` (source overruns freeze on the final frame). |
| `mikan-renderer` | Deterministic CPU reference renderer, PNG output, and the shared text rasterizer. |
| `mikan-gpu-renderer` | `wgpu` renderer for images, video frames, styled text, nested transforms, opacity, offscreen readback, and renderer-owned surfaces. |
| `mikan-exporter` | Deterministic frame-exact H.264/AAC MP4 export through the shared evaluator, GPU renderer, audio graph, and the linked FFmpeg libraries (`ez-ffmpeg` `VideoWriter` for encode, `FfmpegContext` for the AAC mux). Also exports React entries via `mikan-react-bridge`. |
| `mikan-editor` | GPUI application, editor-owned document state, playback clock, GPU preview bridge, asset panel, timeline, and inspector. |
| `mikan-react-bridge` | Spawns one long-lived `@mikan/react` Node.js process per composition and requests the evaluated `Scene` (plus that frame's `<Audio>` clips) for each exact frame time over stdin/stdout JSON, or resolves individual registered components for the editor preview. |
| `packages/react` (`@mikan/react`, Node.js/TypeScript) | Declarative `Composition`/`Sequence`/`Group`/`Image`/`Rect`/`Text`/`Video`/`Audio` components rendered through a real `react-reconciler` host (hooks, including `useCurrentFrame`/`useVideoConfig`, work); `useProject`/`<ProjectTimeline />` embed a companion project's Rust-evaluated layers. The `mikan-react-render` CLI bundles a JSX/TSX entry with esbuild and emits `Scene`-shaped JSON plus per-frame audio declarations. |

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
- `ExportOptions.range` (CLI `--from`/`--to` timecodes; editor In/Out markers,
  keys `i`/`o`/`shift-x`, shown as a band on the scrubber) exports only a
  composition-time span. It is clamped to the composition and snapped to frame
  boundaries, and the encoded output starts at its own 00:00 — video renders
  `start_frame..start_frame+frames`, and the audio graph is shifted earlier by
  the window start before mixing over the window length (the mixer is
  unchanged). The whole-composition path is untouched when `range` is `None`.
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
- A selected audio asset can instead be added as a Dialogue clip when the
  project already defines at least one character. The first project character
  is the initial speaker, probed audio duration supplies the initial clip
  length, and a compatible unlocked Dialogue track is reused or created.
  Selecting the resulting clip exposes its text and all project characters in
  the Inspector; text and speaker edits are undoable and refresh the shared
  preview immediately. Changing speaker clears the old expression override so
  a character-specific expression name cannot leak into the new speaker.
- A selected image asset that is not already assigned to a character exposes a
  Character action. It creates an undoable project character with that image as
  its `default` portrait expression, positions the portrait on the right side,
  and supplies resolution-relative bottom-centered white subtitles with a black
  outline. This is intentionally a usable default; fine-grained character and
  subtitle styling remains future Inspector work.
- The Inspector lists project characters and supports inline, IME-aware name
  editing through the existing `TextInput`. Names are trimmed, cannot be empty,
  participate in undo/redo, and refresh React-aware preview state immediately.
- Selecting an image asset exposes Add expression on each character. The image
  filename becomes a unique expression id, registration is undoable, and adding
  the same image again is an idempotent no-op. A selected Dialogue clip exposes
  Default plus the speaker's non-default expressions; switching expression is
  validated against that character, respects track locking, and is undoable.
- Selecting an image asset also exposes `Mouth: a/i/u/e/o` plus optional
  `Mouth: closed` actions for each character portrait. These configure
  `PortraitDefinition::lip_sync` as transparent overlay assets, independently
  of the selected facial expression. Without a closed asset, silent frames
  simply retain the original portrait without a mouth overlay; Clear closed
  mouth removes only that optional assignment.
  Selecting an audio-backed Dialogue clip for a configured character exposes
  Generate/Regenerate from voice and Clear controls. Generation maps the
  editor's cached clip-local waveform to exact project frames with an adaptive
  relative noise gate and hysteresis, and maps the Dialogue text's hiragana,
  katakana, or Latin vowel sequence across voiced frames. Small kana replace
  the preceding vowel and `ー` repeats it. It then stores a compact sequence of
  `LipSyncCue { time, shape }` values on the Dialogue item. The evaluator adds
  the selected mouth image between portrait and subtitle layers, so GPUI
  preview, React `<ProjectTimeline />`/`<ProjectTrack />`, CPU/GPU rendering,
  and MP4 export share identical results without making the evaluator decode
  audio. Mouth configuration and cue generation are undoable; project
  validation checks image kinds, cue ordering/range, and the required voice
  asset. Cascading mouth/voice asset removal clears dependent LipSync state.
- Unreferenced characters delete immediately. Characters used by Dialogue clips
  show a warning with the affected clips; confirmed deletion removes the
  character and those Dialogue items as one undoable mutation. A referencing
  locked track rejects the entire operation before any project state changes.
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
  prints one `{"config": ..., "componentSchemas": ...}` startup line, then
  answers one request per line — `{"time": ...}` frame requests with
  `{"scene": ..., "audio": [...]}` and component-resolution requests with
  `{"components": [...]}` — or `{"error": ...}`. esbuild transpiles the user's entry
  file directly (TS or TSX) without type-checking it; `@mikan/react`'s own
  source is type-checked by `pnpm run build`.
- `mikan-react-bridge` spawns and owns this Node process for the lifetime of
  an export or preview, mirroring `mikan-media`'s one-process-per-composition
  sequential decoding session rather than spawning Node per frame.
- An entry can additionally export an async `prepare()`. `cli.ts`'s `main()`
  awaits it exactly once, before mounting the composition and before the
  first frame request — the one point in the pipeline where async work (e.g.
  fetching remote data) is allowed, since `renderAt()` itself and the Rust
  side's request/response loop are both fully synchronous. Data fetched in
  `prepare()` should be stashed in module-level state and read synchronously
  by the rendered components, so it is fetched once per export/preview
  session rather than once per frame (see `examples/homepage-demo.tsx`).
- `mikan-exporter --react <entry> <output.mp4>` renders every frame of the
  composition through the same `GpuRenderer` used for projects and encodes it
  with FFmpeg. When the composition has no audio (no `<Audio>` in the entry,
  no companion project, or a companion project with no audio of its own), the
  encoded video is still published directly without FFmpeg's separate mux
  stage; see "`<Audio>` component and React export audio mixdown" below for
  the case where it does.
- React entries integrate with the GPUI editor preview through the
  project's `react_entry`: component clips resolve against the entry's
  registered components, unresolved ones surface as a warning overlay
  instead of failing the whole preview — see "Editor React preview" below.

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
    `visual_only_project()` clones the project and drops `Audio` timeline
    items for *visual* evaluation — `Evaluator::visual_layer` already
    evaluates those to no layer, so this is a cheap explicit skip rather than
    a behavior change. This filtering is specific to the visual path: the
    companion project's own audio still plays in the exported MP4, evaluated
    unfiltered — see "`<Audio>` component and React export audio mixdown"
    below. Every other content kind (`Video`,
    `Image`, `Text`, `Dialogue`, `Component`) is kept — before constructing
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
   used that name) provides `linear`/`easeIn`/`easeOut`/`easeInOut` plus the
   usual sine/quad/cubic/quart/quint/expo/circ/back/elastic/bounce families
   (expanded 2026-08-26 alongside the `<Sequence>` work).
  `mikan_composition::Easing` (the project.json-facing enum a `Keyframe`'s
  own `easing` field uses, applied by `crates/composition/src/animation.rs`'s
  `apply_easing`/`easing_integral`) was widened to the same easings.net
  catalogue on 2026-08-28, formula-for-formula matching `Easings` above so a
  named curve looks the same whether it drives a project keyframe or an
  `interpolate()` call. `apply_easing` has a hand-derived closed-form
  antiderivative only for the original four curves (`linear`/`ease-in`/
  `ease-out`/`ease-in-out`) that `easing_integral` needs for
  `integrate_f64` (animated-playback-rate integration); every other curve
  falls back to a composite-Simpson's-rule numeric integral over
  `apply_easing` itself rather than a hand-verified closed form for
  trig/exponential/piecewise curves like elastic and bounce.
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
  for this) and falls back to `defaultValue` when the key is absent. A
  property the editor renamed or retyped silently falls back rather than
  erroring. This is the design doc's "React API の簡易形"; the
  schema-based `defineProjectProperties`/generated-Inspector-fields version
  it also describes as the eventual goal now exists too (see "Project
  Property Schema" below) — `Property Value = project.json` and
  `Property schema = TypeScript source` remain two different things, and
  the hook itself still takes its own `defaultValue` argument rather than
  consulting the declared one. Verified manually: a project with
  `{"title": "Chapter 3", "episode": 3}` in `properties`, read back through
  `useProjectProperty` inside a `<ProjectProvider>`, including a missing key
  correctly falling back to its default.
- **Component registry** (`src/registry.ts`, resolved by
  `<ProjectTimeline />` in `src/project-runtime.ts`). A project.json
  `TimelineContent::Component { component, props }` item has no meaning to
  `mikan-evaluator` — Rust has no registry, so it always evaluates that
  content to `LayerContent::MissingComponent { component, props }` (see
  `crates/evaluator/src/lib.rs`), and `visual_only_project()`
  (`mikan-exporter`) now keeps `component` items through its filter
  (previously dropped, alongside `dialogue`, in the earlier v1 pass — see
  "`dialogue` timeline content" below for when that changed too).
  `registerComponent(name, Component)`
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
  - Schema-based Project Properties
    (`defineProjectProperties`, GUI Inspector generation) is now built too —
    see "Project Property Schema" below. An `AudioGraph`
    source for React entries is done, see "`<Audio>` component and React
    export audio mixdown" below.
- **Component Property Schema** (`src/registry.ts`), now including GPUI
  editor integration. `registerComponent(name, component, schema?)` takes
  an optional third argument, a `ComponentPropertySchema<Props>` — a
  `{[K in keyof Props]: ComponentPropertyField}` record where each field is
  one of `{type: 'string'|'boolean'|'color', label?, defaultValue}`,
  `{type: 'number', label?, defaultValue, min?, max?, step?}`, or
  `{type: 'select', label?, defaultValue, options}`. It is pure metadata:
  neither `<ProjectTimeline />`'s resolution nor `GpuRenderer` reads it, and
  a component registered without one resolves and renders exactly as
  before. `getComponentSchema(name)` (also exported from `@mikan/react`,
  alongside the `ComponentPropertyField`/`ComponentPropertySchema` types)
  looks it up, returning `undefined` for both an unregistered name and a
  registered one with no schema — callers that need to tell those apart
  check `resolveComponent(name)` (internal, not exported) first.
  `listComponentSchemas()` collects every registered component's schema,
  keyed by name (skipping schema-less registrations); `cli.ts` sends this
  once, alongside `config`, in the startup `Ready` message — all
  `registerComponent()` calls have already run by module-scope time, so
  nothing is missing.
  - **GPUI editor integration.** `mikan-project`'s `ProjectSettings` gained
    an optional `react_entry: Option<String>` field (relative to the
    project file, like an asset path) — project.json's only pointer to
    which `.tsx` entry a `TimelineContent::Component` item's `component`
    name resolves against; nothing in `mikan-project`/`mikan-evaluator`
    reads it, it exists purely so the editor knows which Node process to
    query. `mikan-react-bridge` gained `ComponentPropertyField`/
    `ComponentPropertySchema` Rust types (a real enum mirroring the
    TypeScript shape field-for-field, `#[serde(tag = "type", rename_all =
    "camelCase", rename_all_fields = "camelCase")]` — the same
    `rename_all_fields` gotcha noted above applies here too) and
    `ReactCompositionMetadata::component_schemas: BTreeMap<String,
    ComponentPropertySchema>`, populated by parsing the `Ready` message's
    new `componentSchemas` field during `ReactBridge::spawn`'s handshake —
    no new request/response round trip needed, since this rides the
    existing startup message.
  - `mikan-editor` (previously fully Node-independent — confirmed by
    grepping for zero `Command::new`/`ReactBridge` references before this
    change) now depends on `mikan-react-bridge`. `EditorDocument` gained
    `react_entry()`/`react_entry_absolute_path()`/`set_react_entry()`/
    `clear_react_entry()` (mirroring `serialized_asset_path`'s
    relative-path-under-project-root convention) and
    `set_component_prop(clip_id, key, value)` (mutating a
    `TimelineContent::Component`'s `props` map, erroring
    `UnsupportedComponentProp` for any other clip kind) — all through the
    same `record_mutation`-based undo/redo path every other editor mutation
    uses. `ClipSummary` gained a `component: Option<ComponentClipSummary>`
    (name + current props) for Component clips.
  - A new `ComponentSchemaWorker` (in `main.rs`, following the exact
    `MediaProbeWorker`/`PreviewWorker` pattern: request/result `mpsc`
    channels into a dedicated thread, polled from `poll_background_work`)
    spawns `ReactBridge::spawn(node, cli_script, entry)` — the same `node`
    on `PATH` and workspace-relative `packages/react/dist/cli.js` that
    `mikan-exporter --react` resolves — purely to read
    `metadata().component_schemas` off the handshake, then drops the
    connection; the editor's own preview never renders React content, so
    nothing needs the process to stay open. This runs on its own thread
    because `ReactBridge::spawn` blocks on the child's first stdout line.
    `EditorView::refresh_component_schemas` re-queries on project load and
    whenever `react_entry` changes; results are cached by entry path rather
    than re-fetched per clip selection (spawning Node is comparatively
    slow, and the schemas cannot change without the entry file changing).
  - Inspector UI: a "React Entry" row (path or "Not set") plus Set…/Clear
    buttons (`cx.prompt_for_paths`, the same native-picker pattern
    `request_relink_asset` uses) always shows, alongside loading/error
    state for the schema fetch. Selecting a `ClipKind::Component` clip adds
    a "Component" section rendering one editable row per schema field:
    boolean as a toggle button, number as −/+ steppers respecting
    `min`/`max`/`step`, select as a cycle-through button, and string/color
    through a single shared inline `TextInput` entity (`
    component_prop_input`, generalizing track rename's one-`TextInput`
    pattern to "whichever field is being edited" via a small
    `ComponentPropEdit { clip_id, key }` state). A component with no
    matching schema (not found in `component_schemas`, or `react_entry`
    unset, or still loading) shows an explanatory hint instead of empty
    rows.
  - Verified: new `mikan-editor` unit tests
    (`react_entry_path_is_stored_relative_and_resolves_back_to_absolute`,
    `react_entry_and_component_props_are_undoable`) cover the document
    layer end to end including undo; a new `mikan-react-bridge` integration
    test (`reports_a_registered_components_property_schema_when_node_is_available`)
    spawns the real Node runtime against
    `packages/react/examples/with-registered-component.tsx` (which now
    declares a `bossIntroductionSchema`) and asserts
    `bridge.metadata().component_schemas` carries the exact declared
    fields; two new `mikan-react-bridge` unit tests cover `Ready`-message
    deserialization (with and without `componentSchemas` present). The
    full editor UI (native window, click-driven schema fetch and prop
    editing) could not be exercised interactively in this environment — the
    same limitation HANDOFF.md already notes for pointer-drag testing
    applies here too (GPUI's Linux backend reports "neither DISPLAY nor
    WAYLAND_DISPLAY is set" even with a running `Xvfb` in this sandbox) —
    so this integration is verified by the document-layer/protocol tests
    above plus `cargo build`/`clippy` on the real `main.rs`, not by a
    manual click-through.
- **Project Property Schema** (`src/properties.ts`, new; 2026-08-26). The
  last piece of the design doc: a React entry can now declare its
  GUI-editable project properties, and the GPUI Inspector renders one
  editable row per declared field writing into the project's
  `properties` map (the values `useProjectProperty()` reads). The field
  shape is deliberately shared with component schemas —
  `ProjectPropertyField` is an alias of `ComponentPropertyField`
  (string/number/boolean/color/select with labels, defaults, and display
  hints); the two concepts differ only in where their values live
  (project-level vs per timeline item).
  - `defineProjectProperties(schema)` (`src/properties.ts`, exported from
    `@mikan/react`) stores a process-global schema — call at module scope,
    like `registerComponent()`. `listProjectProperties()` returns it (or
    `undefined` when never called); `cli.ts` adds it to the startup `Ready`
    message as `propertySchema` — `null` when undeclared, `{}` for a
    declared-but-empty schema, riding the existing handshake like
    `componentSchemas` does. It plays no part in rendering or evaluation;
    values themselves live in project.json.
  - Rust mirror: `ReactCompositionMetadata::project_property_schema:
    Option<BTreeMap<String, ComponentPropertyField>>` (reusing the existing
    enum rather than duplicating it), parsed during `ReactBridge::spawn`'s
    handshake with `#[serde(default)]` so older CLI builds stay compatible.
  - Editor: the `ComponentSchemaWorker` (which already spawns Node once to
    read the handshake) now also carries `project_property_schema` back in
    its result — no extra spawn. `EditorDocument` gains
    `project_properties()` plus `set_project_property(key, value)`/
    `remove_project_property(key)` through the usual snapshot undo path
    (identical-value writes and absent-key removals record no entry). The
    Inspector shows a "Project Properties" section whenever a React Entry is
    set: per-field rows reusing the exact widget set from component props
    (boolean toggle, number −/+ steppers respecting min/max/step, select
    cycle, string/color through the shared inline `TextInput`), unset fields
    falling back to their declared defaults — the same rule
    `useProjectProperty` applies when reading. The single inline-input state
    was generalized from clip-scoped `ComponentPropEdit` to a
    `PropertyEdit { target: PropertyEditTarget, key }` whose target is either
    the project map or a specific component clip, so both sections share one
    commit/cancel path.
  - Note the value flow: editor edits write into `project.properties`; a
    React entry reads them via its own `loadProject()` of the saved file (or
    `<ProjectTimeline />`'s pre-evaluated layers) — the schema travels only
    to the Inspector, not into rendering.
  - Verified: two new `mikan-react-bridge` unit tests (`Ready` parsing with
    all five field types present / absent-or-null defaulting to None); a new
    integration test (`reports_a_declared_project_property_schema_when_node_
    is_available`, against a new `packages/react/examples/with-properties.tsx`
    declaring all five field types over an embedded `loadProjectFromString()`
    project) asserting the metadata round-trips exactly, that an entry
    without `defineProjectProperties()` yields `None`, and that the entry's
    own `useProjectProperty()` reads its embedded project's value; a new
    `mikan-editor` document test covering set/remove/undo semantics. The
    interactive Inspector click-through remains untested for the same
    environment reason noted above; UI wiring is covered by clippy on the
    real `main.rs`.
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
- **`dialogue` timeline content in `<ProjectTimeline />`/`<ProjectTrack />`**
  (`crates/exporter/src/lib.rs`'s `visual_only_project`). Turned out to need
  no TypeScript changes at all — the only thing standing between a
  `TimelineContent::Dialogue` item and `<ProjectTimeline />` was the v1
  filter dropping it before evaluation. `Evaluator::dialogue` (called from
  `visual_layer`, `crates/evaluator/src/lib.rs`) already expands a dialogue
  item into a plain `LayerContent::Group` of up to two sublayers — the
  character's portrait `Image` (if the `Character` has one, picking
  `expression` or falling back to `default_expression` from
  `PortraitDefinition.expressions`) and a subtitle `Text` (if the character
  has a `SubtitleDefinition`, or a default-styled one otherwise) — there is
  no dedicated `LayerContent::Dialogue` variant, so the resolved layer looks
  identical to any other author-placed `Group` by the time it reaches
  Node. `visual_only_project()` now only drops `Audio` items (and only as a
  cheap explicit skip — `Evaluator::visual_layer` already evaluates those to
  no layer on its own); every other content kind, including `Dialogue`,
  passes through unfiltered.
  Verified: a new `mikan-exporter` unit test
  (`visual_only_project_keeps_dialogue_and_component_but_drops_audio`)
  constructs a project with one `dialogue` item (with `audio` set) and one
  bare `audio` item, asserting the filter keeps only the former and that
  evaluating it produces the expected portrait+subtitle `Group`. End to end,
  `mikan-exporter --react packages/react/examples/with-project.tsx
  --project <a project with one dialogue track item>` produced an MP4 whose
  frame shows the character portrait, the subtitle text, and the entry's own
  `<Text>` overlay all composited together, confirming `<ProjectTimeline />`
  needed no dialogue-specific handling on the TypeScript side — the generic
  `Group`/`Image`/`Text` rendering (already exercised by the Component
  registry's resolved-subtree case) covers it.
- **`<Video>` component** (`src/components.ts`, `src/render.ts`). Props:
  `src` (matching `<Image>`), `startFrom?: number` (seconds into the source
  file playback begins at, default 0 — the React counterpart of a project
  clip's trim-in point), `playbackRate?: number` (default 1). A React
  `<Video>` plays synced to its enclosing sequence chain's own clock from
  local frame 0 (originally only the whole composition's clock; sequences
  added 2026-08-26 — see below): `buildLayer`'s
  new `video` branch (`render.ts`) is passed the current composition
  `Time` (threaded through `walkChildren`/`walkNode`, which previously
  didn't need it) and uses it directly as `MediaTiming.localTime`,
  computing the only field `GpuRenderer` actually reads to decode —
  `sourceTimeSeconds` — as `startFrom + localTimeSeconds * playbackRate`,
  the same formula `mikan-evaluator::visual_layer` uses for a project
  `TimelineContent::Video` at a constant (non-animated) playback rate.
  `secondsToTime`/`secondsFromTime` (new small helpers) convert between a
  plain seconds number and the generated `Time { value, timescale }` shape,
  using a microsecond timescale so trimming isn't visibly quantized.
  `mikan-exporter`'s `render_react_video` now attaches a video decoder
  (`FfmpegBackend::with_sequential_video`) unconditionally rather than only
  when a companion project is present — previously a React-only export
  (no `--project`) had no decoder at all, so any `LayerContent::Video`
  layer would have hit `GpuRenderError::MissingVideoDecoder`.
  Verified: a new `mikan-react-bridge` integration test
  (`computes_video_timing_from_the_composition_clock_when_node_is_available`,
  against a new `packages/react/examples/with-video.tsx` declaring
  `startFrom={1} playbackRate={2}`) asserts `sourceTimeSeconds` at frame
  15/30 (0.5s in) comes out to `1 + 0.5 * 2 = 2.0`. End to end, a real
  `mikan-exporter --react` export against a synthetic `ffmpeg testsrc`
  clip produced an MP4 whose frames actually changed over time (confirming
  real sequential decoding, not a frozen first frame) and correctly seeked
  when `startFrom` was set to a later point in the source, with a `<Text>`
  sibling compositing on top as expected.
- **`<Audio>` component and React export audio mixdown** (2026-08-26;
  `packages/react/src/components.ts`, `src/render.ts`, `src/cli.ts`,
  `crates/react-bridge/src/lib.rs`, `crates/exporter/src/lib.rs`). Closes the
  "no audio graph for React entries" gap noted above and in "Recommended next
  work". Props (`AudioProps`): `src`, `startFrom?: number` (default 0,
  matching `<Video>`), `playbackRate?: number` (default 1), `volume?: number`
  (default 1), `muted?: boolean` (default false). Like `<Video>`, `<Audio>`
  always plays synced to the whole composition's own clock from frame 0 —
  there is no `<Sequence>`-style range offset — and `volume`/`muted` are
  plain static values in this scope, not `Animatable`; per-frame-varying
  volume automation for React-declared audio is not implemented.
  - **Collection, not per-frame evaluation.** `<Audio>` elements produce no
    `LayerContent` variant (`mikan_composition::Scene`/`LayerContent` stay
    render-only, as intended — audio remains a separate top-level
    `AudioGraph`): `render.ts`'s `walkNode` recognizes the `'audio'` host
    type (added to `HOST_TYPES`) but returns no layer for it. Because
    `<Audio>` declarations are static per this scope, they are gathered by
    one dedicated evaluation rather than a per-frame protocol addition:
    `mount()`'s returned `MountedComposition` gained `collectAudioClips()`,
    which performs one additional real `renderAt`-shaped render (at time
    zero, with the entry's real `CompositionConfig` — unlike the bootstrap
    pass, which uses a placeholder config so it can run before config is
    known) and then walks the resulting instance tree recursively
    (`collectAudioNodes`, following `node.children` regardless of type, so
    `<Audio>` nested inside `<Group>` is found too) collecting every
    `audio`-typed node into a flat `AudioClipDescriptor[]`
    (`{ src, startFrom, playbackRate, volume, muted }`, defaults already
    resolved). `cli.ts` calls this once after `mount()` and includes the
    result as a new `audioClips` field in the existing one-time `Ready` JSON
    message, alongside `config`/`componentSchemas` — the same "piggyback on
    the startup handshake instead of a new request/response round trip"
    precedent `componentSchemas` already established. **Known limitation**:
    since this is a single tree walk and not something re-evaluated per
    requested frame, an `<Audio>` that only conditionally renders for part of
    the composition (based on `useCurrentFrame()`, for example) is not
    supported — it will either always or never appear in the collected list
    depending on what it evaluates to at the time this one walk runs.
    (Superseded 2026-08-26: audio moved to per-frame collection and this
    limitation is gone — see "`<Sequence>`, per-frame audio collection, and
    editor React preview" below; the `audioClips` `Ready` field no longer
    exists.)
  - **Rust-side parsing** (`crates/react-bridge/src/lib.rs`).
    `ReactAudioClipDescriptor { src, start_from, playback_rate, volume,
    muted }` is a real struct (not opaque JSON) mirroring `AudioClipDescriptor`
    field-for-field, `#[serde(rename_all = "camelCase")]`.
    `ReactCompositionMetadata` gained `audio_clips: Vec<ReactAudioClipDescriptor>`,
    parsed from the `Ready` message's new `audioClips` field
    (`#[serde(default, rename = "audioClips")]`, defaulting to empty exactly
    like `componentSchemas` does) during `ReactBridge::spawn`'s handshake —
    no new accessor beyond the existing `metadata()`. Two new unit tests
    cover `Ready`-message deserialization with and without `audioClips`
    present, mirroring the existing `componentSchemas` tests.
  - **`mikan-exporter`'s audio graph construction**
    (`crates/exporter/src/lib.rs`'s new `build_audio_graph`). Builds one
    `mikan_composition::AudioGraph` per React export: React-declared clips
    (from `metadata.audio_clips`, always) plus, when a companion project is
    given, that project's own complete `Evaluator::audio_graph()` — called on
    the project *unfiltered* (unlike the visual path's
    `visual_only_project()`, which drops `TimelineContent::Audio` items
    because they contribute nothing visually; the audio mixdown needs them to
    actually play, so it deliberately does not reuse that filter). The
    companion project supplies `sample_rate`/`master_volume` when present,
    since its `AudioGraph` already carries authoritative values; otherwise a
    new `DEFAULT_REACT_AUDIO_SAMPLE_RATE = 48_000` constant is used — there is
    no project to source a sample rate from, and every checked-in example
    project's `sampleRate` is already 48000, so this matches that existing
    convention. Each React-declared clip becomes an `AudioClip` with `range`
    spanning the full composition duration from `Time::ZERO` (`<Audio>` has
    no project-relative range the way a project clip does),
    `playback_rate`/`volume` wrapped as `Animatable::Static` (matching the
    static-only scope above), and `source_start` built from `startFrom`
    seconds via a `seconds_to_time` helper using the same
    microsecond-timescale convention `render.ts`'s `secondsToTime` already
    uses for `<Video>`. **Two asset roots, resolved the same way the visual
    path already does it**: a React-declared clip's `src` is resolved
    relative to the entry's own directory, a companion project's clip asset
    relative to `project_asset_root` — both go through the existing
    `absolutize_asset` helper (previously used only for visual layers/fonts)
    before being added to the graph, so `mix_audio_graph_cancellable`'s
    single `asset_root` parameter never actually needs to resolve a relative
    path itself.
  - **Two-stage export only when there is audio to mix.** `export_react_entry_impl`
    now spawns the `ReactBridge` and builds this `AudioGraph` before deciding
    how to render: if `graph.clips` is empty (no `<Audio>`, no companion
    project, or a companion project with no audio of its own), the export
    keeps today's exact behavior — one FFmpeg process renders frames straight
    to the published output, `-an`, no mux stage, so silent exports are
    unaffected and unregressed. If the graph has clips, video renders instead
    into a temporary workspace file (mirroring plain project export's own
    `tempdir_in`/`video.mp4` staging), then `mix_audio_graph_cancellable` (the
    same function plain project export already uses) mixes the graph, then
    the existing `mux_audio` (also shared with plain project export, unchanged)
    muxes video and audio into the real published output over a second FFmpeg
    process. `ReactVideoRequest`/`render_react_video` were refactored to take
    the already-spawned `&mut ReactBridge` and `&ReactCompositionMetadata` as
    parameters (previously `render_react_video` spawned the bridge itself),
    since the caller now needs the metadata before deciding which rendering
    path to take.
  - Verified: two new `mikan-react-bridge` unit tests
    (`deserializes_audio_clips_from_the_ready_message`,
    `ready_message_without_audio_clips_defaults_to_empty`) cover the
    `Ready`-message parsing; a new integration test
    (`crates/react-bridge/tests/node_integration.rs`,
    `collects_audio_clips_from_the_ready_message_when_node_is_available`,
    against a new `packages/react/examples/with-audio.tsx` declaring
    `startFrom={1} playbackRate={2} volume={0.5} muted={false}`) asserts the
    collected `audio_clips` metadata exactly, and that the `<Audio>` element
    contributes no layer to the rendered scene (only the entry's sibling
    `<Text>` layer appears). End to end, a real `mikan-exporter --react`
    export of an entry with `<Audio src="<absolute path to
    examples/assets/voices/001.wav>" />` (default `startFrom`/`playbackRate`/
    `volume`/`muted`) against the real Node/FFmpeg toolchain produced an MP4
    with both an `h264` video stream and an `aac` audio stream at 48 kHz
    stereo (confirmed with `ffprobe`), while re-exporting the pre-existing
    `packages/react/examples/title.tsx` (no `<Audio>`, no companion project)
    still produced a video-only MP4 with no `mixing audio`/`muxing MP4`
    progress stages, confirming the silent path is unregressed.

### `<Sequence>`, per-frame audio collection, and editor React preview (2026-08-26)

Three related gaps closed in one pass; they share one protocol change.

- **`<Sequence>` component** (`packages/react/src/components.ts`,
  `src/render.ts`). Props: `from?: number` (frame offset in the enclosing
  timeline's own frame numbering, default 0), `durationInFrames?: number`
  (default: run until the enclosing window ends), plus the common
  transform/opacity props. It is a real function component: it reads the
  enclosing runtime context and re-provides a shifted
  `CompositionRuntimeContext` around its children, so hooks called from a
  component *inside* a sequence see `frame - from` (JSX children evaluate
  where they are written, so hooks written inline in the parent still see the
  parent's clock — the same rule as every other React host, called out
  because it surprises people). The `'sequence'` host node itself always
  stays in the instance tree regardless of time: whether the window contains
  the currently rendered time is decided by render.ts's walker per requested
  frame (`childSequenceContext`), not at React render time. Inactive → no
  layers and no audio collected for that frame. Active → children render
  inside a group layer carrying the sequence's own x/y/opacity, and local
  time is shifted (`<Video>`/`<Audio>` inside play synced to the sequence's
  own clock). Nested sequences compose (origins add, audible windows
  intersect). This is what replaced the "no `<Sequence>`-style range offset"
  caveat on `<Video>`/`<Audio>`.
- **Per-frame audio collection** (`src/render.ts`, `src/cli.ts`,
  `crates/react-bridge/src/lib.rs`, `crates/exporter/src/lib.rs`). Frame
  responses are now `{scene, audio}` where `audio` lists every `<Audio>`
  element that rendered into *that* tree, each as
  `{src, sourceStart, playbackRate, volume, muted, start, duration}`:
  composition-space audible window (`start`/`duration`, intersected through
  any enclosing sequences and never before the clip's local zero),
  `sourceStart` adjusted so head-clipping keeps the source clock continuous,
  and `playbackRate`/`volume` as `number | KeyframeAnimation` — the
  TypeScript mirror of the project format's `Animatable<f64>` (this closes
  the "static-only volume" caveat too). When a window clips a clip's head,
  animation keyframes shift earlier by the clipped amount so curves stay
  aligned with what is actually heard (exact for static rates; documented
  approximation for keyframed rates). Because collection rides the same walk
  as layers, an `<Audio>` behind an ordinary React conditional or hook now
  contributes sound on exactly the frames where it renders — the old single
  `Ready`-message collection (`collectAudioClips`, `audioClips`) is gone.
  Rust mirrors this: `ReactAudioClipDescriptor` carries the new shape,
  `ReactBridge::evaluate_at` returns `FrameEvaluation { scene, audio }`
  (the `scene_at*` wrappers still return just the `Scene`),
  `render_react_video` accumulates reports across frames, and
  `merge_react_audio_clips` collapses bit-identical per-frame reports while
  preserving multiplicity (two identical clips playing at once stay two).
  **Export flow reorder**: frames stream into the staged output file first,
  the graph is built afterwards; empty graph → staged file published
  directly (silent exports keep their no-mix/no-mux path, verified against
  `title.tsx`), non-empty → mix + mux into a second temp then publish.
  **Bug fixed here**: `export_react_entry_impl` canonicalizes the entry's
  and companion project's asset roots before absolutizing relative asset
  paths — a relative CLI entry path produced a still-relative "absolute"
  path that the audio mixer joined twice.
- **Editor React preview** (`src/render.ts`'s new `createResolver()`,
  `src/cli.ts`, `crates/react-bridge/src/lib.rs`'s
  `resolve_components`, `crates/editor/src/main.rs`). A new protocol request
  `{"components": [{component, props}], "runtime": {width, height, fps,
  durationInFrames, time}}` answers
  `{"components": [layers|null, ...]}`: each registered name renders against
  a second persistent reconciler root dedicated to resolution (hook state in
  resolved components survives across calls; unresolved names yield null).
  The `runtime` object — added 2026-08-26; older bridges omit it and the
  components then see frame-0 placeholders — carries the entry's own
  `<Composition>` facts (the same ones the handshake metadata was read from)
  plus the exact preview playhead, so resolved components'
  `useCurrentFrame()`/`useVideoConfig()` match what an export renders at
  that frame (`ReactBridge::resolve_components` takes the `Time` parameter
  and derives the rest from its own metadata; the editor passes
  `scene.time`). `<ProjectTimeline />`/`useProjectTrack()` inside a
  *resolved* component still see empty content — threading the real
  per-frame project layers into resolution requests is future work. The GPUI
  preview worker
  owns an optional `ReactPreviewBridge` keyed by (node, cli script, entry) —
  respawned when `react_entry` changes, spawn/resolution failures remembered
  per context so a broken setup does not restart Node every frame. Each
  preview request collects the evaluated scene's `missingComponent` layers
  recursively, resolves them, splices resolved layers back inside their
  original layer shells (same placement semantics as the exporter's
  `rawTransform` wrapper), strips unresolved ones, and returns warnings;
  `EditorView` renders those as an amber overlay in the preview panel. With
  no `react_entry` set, component clips are hidden with an explanatory
  warning instead of failing the whole GPU render (`GpuRenderer` still
  errors on `missingComponent` content — the honest export outcome).
  **Bug fixed here**: `EditorDocument::load` now canonicalizes the project
  path before deriving the asset root, so paths serialized relative to the
  root resolve back to exactly the same absolute path (on macOS `/var` is a
  symlink to `/private/var`, which broke the `react_entry` round trip).

Verified: new integration tests (`shifts_media_inside_sequences_...`,
`collects_conditionally_rendered_audio_with_keyframed_volume_...`,
`reports_audio_clips_per_frame_...`,
`resolves_individual_components_through_the_bridge_...`,
`resolves_components_against_the_requested_time_...` in
`crates/react-bridge/tests/node_integration.rs` against new examples
`with-sequence.tsx`, `with-conditional-audio.tsx`, and
`with-frame-component.tsx` — the last one registers a component rendering
`useCurrentFrame()`/`useVideoConfig()` and asserts two resolve calls on the
same root report "frame 15 of 640 at 30fps" then "frame 7 of ...", proving
resolved hooks follow the requested time); exporter unit tests
for report merging and graph construction; end-to-end
`mikan-exporter --react packages/react/examples/with-sequence.tsx out.mp4`
produced h264+aac (ffprobe) whose extracted frame 45 shows only the
sequence-shifted testsrc video and frame 75 shows "frame 15 inside" beside
it, while `title.tsx` stayed video-only with no mix/mux progress stages.

### `@mikan/react` `x`/`y` place the top-left corner (2026-08-30)

`extractTransform` in `packages/react/src/render.ts` now defaults `anchor` to
`{ x: 0, y: 0 }` instead of `{ x: 0.5, y: 0.5 }`, so a component's `x`/`y`
address its top-left corner (CSS/canvas/Remotion convention) rather than its
centre. Nothing changed in the Rust renderers or the `Scene` /
`EvaluatedTransform` contract — the renderers already position a layer as
`position - size * anchor` and pivot `scale`/`rotation` about the same anchor;
only the React default the bridge emits moved.

- The `anchor` a layer carries is still both the positioning reference **and**
  the `scale`/`rotation` pivot, so `<Text anchorX={0.5} anchorY={0.5}>` brings
  back the old behaviour (centre-anchored, spins/scales about the centre) and
  `x`/`y` then address that centre. `<Group>` has no size and ignores
  `anchor`: its children are placed relative to its `x`/`y` and it scales
  about that point regardless.
- `CommonProps` in `src/components.ts` documents `x`/`y`/`anchorX`/`anchorY`
  accordingly.
- Examples updated to keep their rendered output: `title.tsx`,
  `with-sequence.tsx`, `with-properties.tsx`, `with-project.tsx` add
  `anchorX={0.5} anchorY={0.5}` to their centred captions; `with-rect.tsx`
  and `homepage-demo.tsx`'s full-frame background `<Rect>` switch to `x={0}
  y={0}`; `homepage-demo.tsx`'s card captions (centred on each card's local
  origin) and its animated card `<Rect>` add the 0.5 anchors;
  `character-lipsync-demo.tsx` likewise.
- The `mikan-react-bridge` integration tests assert layer structure/content,
  not transform values, so they are unaffected.

### Character portraits, PSD presets, and automatic lip sync (2026-08-29)

`@mikan/react` has `<Character>` / `<CharacterView>` (`src/components.ts`,
`src/render.ts`): a `<Character>` inside `<Assets>` declares a reusable
portrait via a ref, and `<CharacterView character={ref}>` renders it. A
portrait is either `{ type: 'image', defaultExpression, expressions,
lipSync? }` (a base image plus optional transparent mouth overlays) or
`{ type: 'psd', src, layers?, lipSync? }`. The PSD path evaluates to a
single `LayerContent::Psd`; `mikan_renderer::rasterize_psd` composites it.

- **PSD layer visibility.** Real multi-outfit / multi-expression "tachie"
  PSDs save every folder hidden, so rendering from the PSD's own saved
  visibility composes nothing but force-enabled layers. `LayerContent::Psd`
  now carries `visible_layers` (`crates/composition/src/model.rs`) alongside
  the existing `enabled_layers` / `disabled_layers`. `rasterize_psd`
  (`crates/renderer/src/lib.rs`) resolves each leaf layer as: `disabled`
  hides, then `enabled` shows (ignoring saved/ancestor visibility — this is
  the current lip-sync mouth), then when `visible_layers` is non-empty
  exactly those paths compose (a preset; saved visibility ignored),
  otherwise the PSD's saved visibility drives it (`layer.visible()` plus
  every ancestor group's `visible()`). Layers are placed at their real PSD
  coordinates (via `psd::PsdLayer::rgba`, which returns canvas-sized
  pixels). Blend modes and clipping masks are not reproduced (plain alpha).
  **Group opacity is deliberately not applied**: the `psd` crate (`0.3.5`)
  reads a folder's opacity from the wrong ("bounding section") record and
  reports `0` for every folder in real PSDTool files — applying it made
  presets compose nothing. Per-layer opacity is read correctly and applied.
  PSDTool's `*` (radio) / `!` (force-on) name conventions are **not**
  interpreted: a preset is a resolved layer list, so a real PSDTool file
  (all folders hidden, or `*` radio siblings all left visible) needs one.
  GPU path mirrors all this in `crates/gpu-renderer/src/lib.rs`'s
  `load_psd`; the cache key includes `visible_layers`.
- **PSDTool presets** (`src/psd-preset.ts`). `resolveVisibleLayers(state)`
  parses a PSDTool layer-state string (the "copy layer state" output — both
  the `/`-prefixed "all layer" form and the compact form), percent-decoding
  segments and dropping the `\N` sibling-dedup suffix (duplicate sibling
  names are not disambiguated — a known limitation, since the Rust side
  builds paths from bare names). `parsePfv(text)` reads a `.pfv` favorites
  file (`[PSDToolFavorites-v1]` header, `//tree/path` blocks with a
  possibly multi-line state, blank-line separated). `loadPsdPreset({ src,
  favorite? })` reads a `.pfv` from disk and resolves one favorite — call
  it from an entry's `prepare()`. The portrait's `layers` prop accepts the
  resolved `string[]`, or a raw state string (`render.ts` parses it
  synchronously, no disk IO).
- **Automatic lip sync** (`src/lipsync.ts`). TypeScript port of the
  editor's `lip_sync_cues_from_waveform` / `vowel_shapes`
  (`crates/editor/src/lib.rs`). `decodeWav(bytes)` parses uncompressed
  PCM/IEEE-float WAV (8/16/24/32-bit int, 32/64-bit float, any channel
  count, `WAVE_FORMAT_EXTENSIBLE`) to mono; `buildEnvelope` takes a
  per-hop (default 100 Hz, fps-independent) peak envelope; `lipSyncTimeline`
  applies the same adaptive gate (`open = max(peak*0.18, 0.015)`,
  `close = open*0.6`), hysteresis, and even vowel distribution across
  voiced hops. `loadLipSync({ src, text, hopHz? })` (call in `prepare()`)
  returns a `LipSyncTrack` with `mouthAtSeconds` / `mouthAtFrame`.
  `useLipSync(track)` reads `useCurrentTime()`; `<CharacterView
  lipSync={track}>` drives its own mouth when no `mouth` prop is given
  (`CharacterView` calls the hook internally — always, tolerating a missing
  track). Only WAV is supported; other formats would need a decoder.
- **Entry-directory resolution** (`src/entry-dir.ts`, `src/cli.ts`).
  `cli.ts` sets `MIKAN_REACT_ENTRY_DIR` before running `prepare()`, so
  `loadLipSync` / `loadPsdPreset` resolve relative paths against the entry
  file's directory — matching how the Rust renderer resolves a relative
  `<Audio>` / `<Image>` `src`.
- **Test fixture** (`examples/assets/lipsync-fixture.psd` / `.pfv`, generated
  by `packages/react/scripts/make-lipsync-fixture.mjs` — `ag-psd` is a
  devDependency used only by that script). A 240x320 PSD with every folder
  saved hidden and a `face/mouth` group of six small vowel shapes at their
  true positions, small enough for fast unit-test pixel assertions.
- **Demo assets** (`examples/assets/illust/`): redistribution-permitted BOOTH
  SD tachie PSDs (Kotonoha sisters / 紲星あかり / 結月ゆかり), each with a
  sibling `.txt` naming its BOOTH source. The demo uses
  `琴葉姉妹_SD立ち絵.psd` (3292x2400, 135 layers, `*`/`!` PSDTool
  conventions) plus a hand-authored `琴葉茜.pfv` favorite (`茜 メイド 通常`)
  selecting one sister's maid pose. These `.psd`s are large (16–19 MB each);
  renderer unit tests use the small `lipsync-fixture.psd`, not these.
  `examples/assets/voices/character-lipsync-demo.wav` (+ `.txt` transcript)
  is the committed demo narration.
- **Verified**: `crates/renderer` `rasterize_psd` unit tests (no preset →
  fully transparent; preset + a force-enabled hidden mouth → the pose plus
  the mouth at real coordinates; `disabled` beats `enabled`);
  `packages/react/test/*.test.mjs` (`node --test`, run via `pnpm test`) for
  `vowelShapes` / `parsePfv` (incl. the real `琴葉茜.pfv`) /
  `resolveVisibleLayers` / `decodeWav` / `lipSyncTimeline`; new
  `crates/react-bridge/tests/node_integration.rs` cases
  (`with-psd-preset.tsx`, `with-lip-sync.tsx`) asserting the scene's
  `visibleLayers` and per-frame mouth selection; end-to-end
  `mikan-exporter --react packages/react/examples/character-lipsync-demo.tsx`
  produced an MP4 whose frames show 琴葉茜 fully composited (maid outfit,
  twintails, gentle eyes) with the あいうえお mouth tracking the narration.

### `Rect` primitive and a Remotion-homepage-style demo composition (2026-08-28)

Requested as a reproduction of Remotion's homepage "Interactive Demo"
(`packages/promo-pages/.../homepage/Demo/Comp.tsx` upstream). Scoped down
after confirming with the user: visuals/animation only (no browser-side
interactive Player — Mikan has none), fixed mock data instead of the
original's live GitHub-trending/weather fetches, and Mikan-native
replacements for Remotion-only packages (`@remotion/animated-emoji`,
`@remotion/media`).

- **`LayerContent::Rect`** (`crates/composition/src/model.rs`): a flat-shaded
  rectangle, `{ width, height, fill: Option<Paint>, stroke: Option<Stroke>,
  corner_radius }`, reusing the existing `Paint`/`Stroke` types `TextStyle`
  already has rather than introducing a new color type. Unlike
  `Image`/`Video`, a rect has no natural size, hence the explicit
  `width`/`height`. This closes a real gap: before this, Mikan had no way to
  draw a flat background or border at all, in a project or a React entry.
- **Shared rasterizer** (`crates/renderer/src/lib.rs`): `pub fn
  rasterize_rect(width, height, corner_radius, fill: Option<&Paint>, stroke:
  Option<&Stroke>) -> Result<RasterizedText, RenderError>` mirrors
  `TextRasterizer::rasterize`'s shape exactly (same `RasterizedText`
  width/height/pixels output) so it composites through the exact same
  `render_image` path text does, and so `mikan-gpu-renderer` can call it
  directly without its own color-parsing code (it already depends on
  `mikan-renderer` for `TextRasterizer`; this is the same precedent). Uses
  Inigo Quilez's rounded-box signed-distance function, anti-aliased over a
  ~1px edge via `smoothstep`-style clamping; the stroke band is a second SDF
  evaluation against the fill rect shrunk by the stroke width. `mikan-editor`
  needed no changes — its `LayerContent` matches already have wildcard `_ =>`
  arms. `mikan-exporter`'s `absolutize_layer_content` needed one match arm
  added (a rect has no asset to absolutize, so it's a no-op alongside
  `Text`/`MissingComponent`).
- **`<Rect>` in `@mikan/react`** (`src/components.ts`, `src/render.ts`):
  `{width, height, fill?: string, stroke?: string, strokeWidth?, cornerRadius?}`
  — plain hex color strings rather than requiring authors to build `Paint`/
  `Stroke` JSON objects by hand, converted in `render.ts`'s new `'rect'`
  branch of `buildLayer` (added to `HOST_TYPES` alongside the others).
- Verified: a new `mikan-renderer` unit test
  (`renders_a_filled_rounded_rect_with_a_stroke`) asserts the fill color at
  the rect's center and that a corner-radius-excluded pixel is neither the
  fill nor the stroke color; a new `mikan-react-bridge` integration test
  (`evaluates_a_rect_with_fill_stroke_and_corner_radius_when_node_is_available`,
  against a new `packages/react/examples/with-rect.tsx`) asserts the
  evaluated `LayerContent::Rect` fields round-trip exactly through the Node
  bridge. `cargo test -p mikan-project -p mikan-composition --features
  codegen` regenerated `packages/react/src/generated/LayerContent.ts` with
  the new `rect` variant; `pnpm run build` type-checks clean against it.
- **`packages/react/examples/homepage-demo.tsx`**: a 640x360/30fps/120-frame
  composition reproducing the four-card layout (GitHub trending, weather,
  current country, an emoji picker) using `<Rect>` for each card's
  background/border, `spring()`-based staggered entrance per card,
  `interpolate()` for the weather card's count-up, and a plain emoji-glyph
  `<Text>` (cycled every 40 frames) in place of the original's Lottie
  animated emoji. `<Audio src="../../../examples/assets/voices/001.wav">`
  (the repo's existing VOICEROID voice fixture) stands in for
  `@remotion/media`'s reaction sound. Verified end to end: `cargo run -p
  mikan-exporter -- --react packages/react/examples/homepage-demo.tsx
  out.mp4` produced an h264+aac MP4 (ffprobe-confirmed); extracted frames at
  10/60/100 were inspected and show the expected staggered card entrance,
  the weather card's temperature counting up to a settled `24°C`, and the
  emoji swapping across its three glyphs. **Known limitation, not introduced
  by this work**: this environment's text rasterizer (`cosmic-text`/`swash`)
  has no color/COLR emoji font support, so emoji glyphs render as their
  monochrome fallback outline (a flame silhouette, a plain flag shape, a
  bare circle for the "pleading face") rather than full-color glyphs — a
  pre-existing constraint of the shared text rasterizer, not of the new
  `Rect` work.

### Pipelined GPU readback for export (2026-08-28)

Requested as a follow-up: "書き出し速度を速くしたい" (make export faster).
Profiled first rather than guessing — a temporary `MIKAN_EXPORT_PROFILE=1`
instrumentation (not committed) around `render_react_video`'s per-frame loop
showed, for both a tiny 640x360 `<Rect>`-heavy scene
(`homepage-demo.tsx`) and a 1920x1080 single-`<Text>` scene (`title.tsx`),
that **`GpuRenderer::render()` dominated at ~3.4-4.4ms/frame regardless of
resolution or scene complexity** — Node IPC was ~0.4-0.5ms/frame and the
FFmpeg pipe write scaled with resolution as expected, but the render cost
barely moved between the two very different scenes. That pointed at fixed
per-frame overhead rather than actual rendering work.

- **Root cause** (`crates/gpu-renderer/src/lib.rs`'s old `render()`): every
  single frame allocated a brand-new offscreen texture and readback buffer,
  submitted GPU work, then called `device.poll(wgpu::PollType::
  wait_indefinitely())` and blocked immediately for the CPU↔GPU round trip
  before doing anything else. Frame N+1 could not start until frame N's GPU
  work, readback, *and* the FFmpeg pipe write had all fully completed — a
  fully serial submit→wait→copy→write chain with no overlap between Node
  evaluation, GPU rendering, and FFmpeg encoding.
- **Fix**: `GpuRenderer::submit(&mut self, scene) -> Result<Option<GpuFrame>,
  GpuRenderError>` and `GpuRenderer::drain(&mut self) -> Result<Vec<GpuFrame>,
  GpuRenderError>`, alongside (not replacing) the existing synchronous
  `render()` used by the editor's one-off preview rendering and existing
  tests. `submit` encodes and submits a frame's GPU work and kicks off its
  `map_async` readback *without blocking*, reusing a ring of
  `PIPELINE_DEPTH = 3` texture/readback-buffer pairs (`ReadbackSlot`) instead
  of allocating fresh ones every call. Once `PIPELINE_DEPTH` frames are
  in flight, `submit` blocks to reclaim the *oldest* one (FIFO, so output
  order is always preserved) before submitting the new one, returning that
  reclaimed frame's pixels as `Some(GpuFrame)` (or `None` while the pipeline
  is still filling up). By the time a slot is reclaimed, the GPU has usually
  already finished it while the caller was busy with other frames' Node IPC/
  evaluation/FFmpeg writes, so the wait is short or free instead of a full
  submit-to-complete stall every frame. `drain()` blocks to collect whatever
  is still in flight, in order, once the caller is done submitting — needed
  since the last `PIPELINE_DEPTH - 1` frames never get returned by `submit`
  itself.
- **`mikan-exporter`**: `render_video` (plain project export) and
  `render_react_video` (React entry export) both now call `submit` per frame
  (writing its `Some(GpuFrame)` immediately when present) and `drain` after
  the loop (writing the remaining frames). A new `write_frame` helper
  de-duplicates the FFmpeg-stdin write shared by both call sites. Both
  export paths funnel through `GpuRenderer`, so both benefit from the same
  change without duplicating the pipelining logic.
- Verified: a new `mikan-gpu-renderer` unit test
  (`submit_and_drain_return_frames_in_submission_order_with_correct_content`)
  submits five distinctly-colored scenes (more than `PIPELINE_DEPTH`, so it
  exercises both the reclaim-while-submitting path and the final `drain`)
  and asserts every frame comes back in submission order with the exact
  color it was given — the concrete regression this pipelining could have
  introduced (slot mixups) is a shuffled or wrong-content frame, and this
  test is the direct check for that. Full workspace suite still green (125
  tests, up from 124). End to end: a release-build `mikan-exporter --react`
  A/B (git-stashed old code vs. new, `/usr/bin/time -p`, steady-state median
  of 3 runs) showed real time dropping from ~0.71s to ~0.58s for
  `homepage-demo.tsx` and ~0.84s to ~0.68s for `title.tsx` (~18-19%,
  meaningful but bounded by these exports' short frame counts and Node's own
  per-frame IPC being fast for such small compositions already; the fixed
  ~3-4ms/frame that was being fully serialized on every frame is now mostly
  hidden). Extracted a frame from both the old and new output of each
  composition and confirmed byte-identical raw pixel content
  (`ffmpeg -f rawvideo` + `cmp`), confirming the pipelining changed timing,
  not output. A plain (non-React) project export
  (`cargo run -p mikan-exporter -- examples/editor-demo.mikan.json`, 600
  frames) was also re-run end to end and still produces a valid h264+aac MP4.
- **Not done, deliberately scoped out for this pass**: overlapping the
  FFmpeg pipe *write* itself (currently still a blocking call on the same
  thread right after `submit`/`drain` return pixels) and overlapping Node
  IPC evaluation of frame N+1 with frame N's GPU work are both still
  possible further wins — the profiling data suggests they matter more for
  heavier/longer compositions than the ones measured here. `render_video`'s
  `mix_audio_graph_cancellable`/FFmpeg audio-mux stages, and `mikan-media`'s
  video decode path, were not profiled or touched in this pass.

### Color emoji glyph rendering (2026-08-28)

Follow-up to the earlier "known limitation" note on the homepage-demo
composition (an emoji rendered as a flat monochrome silhouette instead of
full color). Investigated rather than accepted as a hard crate limitation —
it turned out to be fixable in `mikan-renderer` without touching
dependencies at all.

- **Root cause**: `cosmic-text`/`swash` (already pinned at `0.18.2`/`0.2.10`)
  already decode color glyphs — COLR, `sbix` (how Apple Color Emoji stores
  its glyphs), CBDT/CBLC — into real per-pixel RGBA (`swash::scale::image::
  Content::Color`, surfaced through `cosmic_text::Buffer::draw`'s
  callback). `TextRasterizer::rasterize`'s draw callback
  (`crates/renderer/src/lib.rs`) was just discarding that: it only ever
  extracted `color.a()` into a single-channel coverage mask, later
  recombined with one flat `style.fill` `Paint::Solid` color — a
  representation that is exactly right for ordinary uniformly-colored text
  but structurally cannot hold an emoji's multiple hues. Font fallback to a
  system color-emoji font (`Apple Color Emoji` is already in cosmic-text's
  built-in macOS `common_fallback()` list, `src/font/fallback/macos.rs`) was
  never the missing piece.
- **Fix**: the draw callback now also accumulates a second, parallel RGBA
  buffer (`glyph_pixels`) by alpha-blending each pixel's *actual* `color`
  from cosmic-text (reusing the existing `blend` helper), alongside the
  existing coverage-only `mask` (still needed as-is for the stroke's
  dilation, which only cares about the glyph's silhouette). `buffer.draw`'s
  base color argument changed from hardcoded white to the real resolved
  `fill` color — for an ordinary glyph (`Content::Mask`), cosmic-text
  returns `(fill_rgb, coverage_alpha)` per pixel, so `glyph_pixels`
  reproduces flat-fill text exactly; for a color glyph (`Content::Color`),
  it returns the glyph's own real RGBA, captured as-is. A new
  `composite_rgba` helper (mirroring `composite_mask`, but blending a
  buffer's own per-pixel color instead of one uniform color weighted by a
  mask) replaces the old `composite_mask(&mask, fill, ...)` call for the
  fill step; the stroke step is untouched. No GPU renderer changes were
  needed — it already consumes whatever RGBA `TextRasterizer` produces via
  the same `RasterizedText` type, same as the `Rect` work earlier in this
  document.
- Verified: a new `mikan-renderer` unit test
  (`rasterizes_color_emoji_glyphs_when_a_color_font_is_available`)
  rasterizes 🔥 and asserts its opaque pixels contain more than one distinct
  RGB color — the direct check that would fail if this regressed back to
  flat-fill mask compositing (skips gracefully, matching this codebase's
  existing pattern for GPU-adapter/ffmpeg-unavailable tests, on a machine
  with no color-emoji font at all, though it did *not* skip when run here).
  Existing `renders_text_to_a_png`/`centers_visible_single_line_text_on_its_
  transform` tests still pass unchanged, confirming ordinary flat-fill text
  is pixel-for-pixel unaffected. End to end: re-exporting
  `packages/react/examples/homepage-demo.tsx` now shows the 🔥/🥳/🥲 emoji
  in full color (previously flame/flag/circle silhouettes only), and
  re-exporting `examples/voiceroid.mikan.json` (stroke+fill subtitle text)
  confirmed the stroke/fill combination used by VOICEROID-style subtitles is
  visually unaffected.

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
- `packages/react/examples/with-video.tsx`: a `<Video src="./clip.mp4"
  startFrom={1} playbackRate={2} />`, used by `mikan-react-bridge`'s video
  timing integration test (no actual `clip.mp4` needed there — Node
  evaluates the layer tree without decoding).
- `packages/react/examples/with-audio.tsx`: a `<Text>` alongside `<Audio
  src="./voice.wav" startFrom={1} playbackRate={2} volume={0.5}
  muted={false} />`, used by `mikan-react-bridge`'s per-frame audio
  integration test (no actual `voice.wav` needed there, for the same
  reason).
- `packages/react/examples/with-sequence.tsx`: a `<Sequence>`-shifted
  `<Video>`/`<Audio>` pair starting at frame 30 plus a caption component
  inside a second sequence (frames 60–89) reading its shifted
  `useCurrentFrame()`, used by the sequence-timing integration test.
- `packages/react/examples/with-conditional-audio.tsx`: an `<Audio>` behind
  `{frame >= 15 && ...}` with a keyframed volume animation, used by the
  conditional-rendering audio integration test.
- `packages/react/examples/with-properties.tsx`: declares a five-field
  `defineProjectProperties()` schema (one per field type) and reads
  `properties.title` back through `useProjectProperty()` over an embedded
  `loadProjectFromString()` project, used by the project-property-schema
  integration test.
- `packages/react/examples/with-frame-component.tsx`: registers a
  `FrameCaption` component rendering `useCurrentFrame()`/`useVideoConfig()`,
  used by the integration test asserting editor-path component resolution
  sees the requested time.
- `packages/react/examples/with-rect.tsx`: a single filled, stroked,
  rounded `<Rect>`, used by `mikan-react-bridge`'s rect-evaluation
  integration test.
- `packages/react/examples/homepage-demo.tsx`: the Remotion-homepage-style
  four-card demo composition described above; not tied to a Rust test, run
  manually via `mikan-exporter --react`.
- `packages/react/examples/with-media-info.tsx`: calls `preloadMedia()` from
  `prepare()`, derives the composition duration with
  `mediaDurationInFrames()`, and displays the probed audio sample rate.
- `packages/react/examples/with-debug.tsx`: registers a `DebugCard` component
  using `DebugOverlay` and `DebugBounds`; the guides appear in editor
  component resolution but are absent from normal composition evaluation.

### React workflow helpers (2026-08-31)

- `Transition` wraps children in the existing `Group` transform and derives a
  clamped fade, slide, or scale entrance/exit from `useCurrentFrame()`. It
  follows an enclosing `Sequence`'s local clock and duration; no renderer
  primitive or dependency was added.
- `SafeArea` provides reduced layout bounds through React context. Nested
  `Center` and `Fit` consume those bounds, while `Stack` and `Grid` place child
  origins at explicit spacing/cell sizes. This stays deterministic because it
  does not attempt DOM-style child measurement.
- `preloadMedia(src)` is intended for an entry's async `prepare()`. The Node
  CLI sends startup probe requests over the existing bridge pipe, and
  `mikan-react-bridge` answers with `mikan-media::FfmpegBackend::probe`
  metadata. Results are cached per absolute path in the entry process and
  include duration, video dimensions/frame rate, and audio stream facts.
  `mediaDurationInFrames()` rounds a known duration up to the requested
  composition frame rate. This path continues to use linked FFmpeg libraries,
  not `ffprobe` or an npm media parser.
- Component-resolution requests now mark `CompositionRuntimeContext` as a
  preview runtime. `useIsPreview()` exposes that fact; `DebugOverlay` draws
  canvas safe-area/center/frame guides and `DebugBounds` adds bounds, local
  origin, and an optional label. Both omit guide layers from normal React
  evaluation/export. Live Node integration coverage checks both the preview
  guides and their export omission.

### Standalone React composition preview in the editor (2026-09-07)

`mikan-editor <entry>.tsx` (also `.ts`/`.jsx`/`.js`/`.mjs`/`.cjs`; a
`*.mikan.json` is still always a project) opens a standalone React composition
in a **preview-only** mode: real-time GPU preview following the playhead,
`<Audio>` playback, auto-reload on source edits, and MP4 export — but no
timeline/inspector/asset editing (the composition is code, edited in the
`.tsx`). This is distinct from the existing `project.json` + `react_entry`
component-resolution path, which is unchanged.

- **Synthetic project.** `EditorDocument::react_preview(entry, width, height,
  frame_rate, sample_rate, duration_in_frames)` (`crates/editor/src/lib.rs`)
  builds an in-memory `Project` with no tracks, `settings.duration` =
  `Time::frames(duration_in_frames, frame_rate)`, and `react_entry` = the
  entry filename. Composition facts come from a one-shot `ReactBridge::spawn`
  handshake in `EditorView::open`. This synthetic project drives the existing
  clock, scrubber, transport, dimension/duration labels, and window title
  unchanged — only the scene source and a handful of gates differ, avoiding an
  enum through ~90 `self.document.*` call sites. It is never saved or mutated;
  `EditorView::is_effectively_dirty()` keeps the title/edited-state/close-guard
  quiet, and `save`/`save as`/`import assets` early-return in this mode.
- **Visual.** `EditorView.react_preview: Option<ReactPreview>` flags the mode.
  `PreviewRequest` gained `react_mode: ReactPreviewMode` (`WholeScene` vs
  `ResolveComponents`) and `react_reload: u64`. The preview worker, in
  `WholeScene` mode, calls `bridge.scene_at_with_project(scene.time, None)` and
  renders that whole scene (`render_whole_react_scene` in `main.rs`), sharing
  the respawnable `ReactPreviewBridge` (now via `ensure_react_bridge`, factored
  out of `resolve_preview_components`). Bridge errors surface as
  `preview_warnings` / `preview_error`; the GPU + native-presentation path is
  untouched.
- **Audio.** New `mikan_react_bridge::ReactBridge::collect_audio_graph(
  sample_rate, master_volume, entry_dir)` sweeps every composition frame via
  `evaluate_at`, merges the per-frame `<Audio>` reports
  (`merge_react_audio_clips`, now `pub`), and builds an `AudioGraph`
  (`react_audio_clips`, `pub`, resolves relative `src` against `entry_dir`).
  A dedicated `ReactAudioWorker` thread runs the sweep (transient bridge) and
  its result is forwarded into the **existing** `AudioMixWorker` →
  `AudioPreview` → rodio pipeline. `refresh_audio_preview` branches on
  `react_preview`. `mikan-exporter`'s own `build_audio_graph` was left as-is
  (small duplication of the clip-construction loop; deliberately not
  refactored to keep the change contained). Default sample rate 48 kHz
  (`REACT_PREVIEW_SAMPLE_RATE`), matching the exporter.
- **Auto-reload.** `watch_react_entry` starts one `cx.spawn` loop (only in
  preview mode) that every ~800 ms diffs `newest_source_mtime(entry_dir)`
  (recursive, skips `node_modules`/`dist`/`.tmp`/`.git`) against the stored
  baseline. On a change it calls `request_react_reload`, which re-reads
  `<Composition>` metadata on a background thread, rebuilds the synthetic
  document + clock (current frame preserved), bumps `react_reload_generation`
  (→ preview worker drops its `ReactPreviewBridge` and respawns Node against
  the re-bundle), and refreshes preview + audio. A **Reload composition**
  button in the right-hand info panel runs the same path. **Limitation**: the
  per-frame `collect_audio_graph` sweep is O(frames) Node round trips, so a
  long composition's audio preview takes a while to (re)build; it runs off the
  UI thread.
- **Export.** `ExportRequest.source: ExportSource` is now `Project(Box<Project>)`
  or `ReactEntry { entry, node, cli_script }`; the export worker branches to
  `Exporter::export_react_entry_cancellable`. `start_export` picks the variant
  from `react_preview` (no export range for React).
- **UI.** `render()` swaps the asset + inspector panels for a compact
  `react_preview_panel` (entry path, size, fps, duration, renderer, Reload
  button, warnings) when `is_react_preview()`; the toolbar and the (empty)
  timeline/scrubber are reused as-is.
- Verified: `mikan-react-bridge` unit tests
  (`merge_react_audio_clips_collapses_identical_per_frame_reports`,
  `react_audio_clips_resolve_relative_paths_and_carry_ranges`); a
  `mikan-editor` document test
  (`react_preview_builds_a_synthetic_document_from_composition_facts`); a live
  `node_integration` test
  (`collect_audio_graph_sweeps_every_frame_into_one_graph_when_node_is_available`,
  against `with-audio.tsx` and `with-conditional-audio.tsx`). `cargo build
  --workspace` and `cargo clippy --workspace --all-targets -D warnings` are
  clean. The GUI itself (opening a `.tsx`, playback, reload, export) could not
  be exercised in this environment — GPUI needs a display, and this Windows
  worktree's Node integration harness hits a pre-existing `EISDIR 'G:'` path
  bug that also blocks a real editor→bridge spawn here.

## Validation baseline

At this handoff, the Rust workspace has 150 passing tests and
`packages/react` has 15 passing Node tests. This slice adds live bridge tests
for media metadata preload and preview-only debug guides. The earlier baseline
had 126 workspace tests (125 from the previous handoff, plus a
`mikan-renderer` color-emoji rasterization test). Before that, 125 (124 from
the handoff before that, plus a `mikan-gpu-renderer`
test for `submit`/`drain` pipelining order/correctness). Before that, 124
(122 from the handoff before that, plus a `mikan-renderer` `Rect`
rasterization test and a `mikan-react-bridge` `<Rect>` evaluation
integration test). Before that, 122 (110 from the
handoff before that, a live-FFmpeg `mikan-media` test freezing
on the last frame past a source's end, three dialogue-authoring editor
tests, and two character-creation editor tests, two character-name tests,
two safe character-deletion tests, and two character-expression tests). The
media test skips itself when ffmpeg/ffprobe are missing, or locates them via
`MIKAN_FFMPEG_DIR`. The last checks were:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check
cargo test -p mikan-project -p mikan-composition --features codegen
cargo clippy -p mikan-composition -p mikan-project --all-targets --features codegen -- -D warnings
cd packages/react && pnpm run codegen && pnpm run build && pnpm test
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

This session's sandbox had `node`/`pnpm` on `PATH` but not `ffmpeg`/`ffprobe`
or an ALSA dev package, so `pnpm install && pnpm run codegen && pnpm run
build` in `packages/react` and the full `mikan-react-bridge` integration
suite (real Node, no skip) were run for real; `libasound2-dev`, `ffmpeg`, and
`mesa-vulkan-drivers`/`libegl1` (software Vulkan, for `mikan-gpu-renderer`'s
wgpu backend) were installed via `apt-get` to unblock `mikan-editor`'s build
and a real `mikan-exporter --react` run respectively — neither is a repo
change, just sandbox setup, and is not guaranteed present in a future
session. With that in place, a real `mikan-exporter --react` export of an
entry declaring `<Audio src="<absolute path to
examples/assets/voices/001.wav>" />` alongside a `<Text>` was run end to end
(see "`<Audio>` component and React export audio mixdown" above for the
`ffprobe`-confirmed result), and re-exporting `packages/react/examples/
title.tsx` (no audio) was re-verified to still skip the mux stage.

There are future-incompatibility warnings in transitive dependencies
`block 0.1.6` and `proc-macro-error2 2.0.1`; these are not current Mikan lint or
test failures.

With the rustfmt installed in this 2026-08-31 macOS environment, the full
`cargo fmt --all -- --check` reports formatting-only differences in pre-existing
`crates/composition/src/animation.rs`, media integration/build files, and
`crates/renderer/build.rs`. Those files were clean at task start and were not
reformatted. Every Rust file changed by this slice passes an individual
`rustfmt --edition 2024 --check`; workspace tests and `clippy -D warnings` pass.

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

The first GUI dialogue-authoring slice is also complete: an imported portrait
can create a ready-to-use character, and an imported voice can be inserted as a
Dialogue clip with editable text/speaker, and character names can be changed
inline in the Inspector. Characters can also be safely removed with reference
confirmation and undo. Selected images can now be registered as additional
portrait expressions and chosen per Dialogue clip. Fine-grained
portrait/subtitle styling is the next VOICEROID-specific editor gap; caption
file/transcript import remains a later workflow gap compared with Remotion's
broader ecosystem. Small React fade/slide/scale transitions are now available;
cross-composition transition orchestration remains deliberately out of scope.

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
   design doc's "React API の簡易形") and the schema-based
   `defineProjectProperties` (generating Inspector fields, see "Project
   Property Schema" above) are both done.
5. ~~Component registry~~ — `registerComponent()`/`<ProjectTimeline />`
   resolving `TimelineContent::Component`'s `component`/`props` to a
   registered React component is done, see "Component registry" above.
6. ~~Property schema / Inspector metadata for GUI-editable component
   props~~ — both the TypeScript-side declaration API
   (`registerComponent(name, component, schema)`, `getComponentSchema()`)
   and the GPUI editor integration (the editor spawning Node to read a
   `react_entry`'s registered schemas and rendering editable Inspector
   fields for a selected Component clip) are done, see "Component Property
   Schema" above.
7. ~~`dialogue` timeline content in `<ProjectTimeline />`~~ — done, see
   "`dialogue` timeline content" above; `visual_only_project()` no longer
   drops it.

Separately, still open from the original slice:

- ~~A `<Video>` component in `packages/react`~~ — done, see "`<Video>`
  component" above.
- ~~An `AudioGraph` source for React entries so `mikan-exporter --react` can
  mux audio instead of always publishing a silent MP4~~ — done, see
  "`<Audio>` component and React export audio mixdown" above; a composition
  with no audio still publishes a silent MP4 exactly as before.
- ~~`<Sequence>`-style range placement for React media and layers~~,
  ~~per-frame (conditionally rendered / keyframed) `<Audio>` support~~, and
  ~~React content in the GPUI editor preview with unresolved-component
  warnings~~ — all done, see "`<Sequence>`, per-frame audio collection, and
  editor React preview" above. Resolved components' hooks now follow the
  requested preview time (2026-08-26, see "Editor React preview" above);
  `<ProjectTimeline />`/`<ProjectTrack />` inside a *resolved* component
  still see empty project content there — threading real per-frame layers
  into resolution requests is future work. Media decoding past a source's
  end no longer fails the frame either (2026-08-26): `mikan-media` clamps
  requested source times to the probed final frame's presentation time
  (container/stream duration minus one nominal frame period — FFmpeg's input
  seek only emits frames at or after the target) and sequential sessions
  freeze on their last decoded frame at clean EOF, so a `Video` clip or
  React `<Video>` whose playback outruns its file shows a held last frame in
  both export and editor preview instead of erroring. Sources without a
  usable duration/frame rate keep the old error; audio has always degraded
  to silence past its source's end (the mixer skips exhausted sources).

Before starting new work here, confirm scope with the user rather than
assuming the full design doc.

Do not optimize preview presentation by letting GPUI and wgpu both present to
the same window surface.

## Working conventions

- Preserve unrelated or pre-existing worktree changes.
- Use targeted validation while iterating, then the full baseline above.
- Do not commit unless the user explicitly asks.
- If a commit is requested, stage only accepted files.
