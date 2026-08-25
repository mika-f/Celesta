# Mikan

Mikan is a code-first video editor designed around a shared project and
composition model. The GUI editor owns project data, while React code may read
that data and add compositions. Preview and export will ultimately use the same
Rust renderer.

The repository currently contains the first foundation:

- `mikan-composition`: exact rational time and shared visual primitives.
- `mikan-editor`: the first GPUI editor shell with project loading, an asset
  browser, frame-accurate playback controls, GPU-rendered preview, inspector,
  and timeline overview.
- `mikan-project`: the version 0 project format, JSON loading, semantic
  validation, and timeline duration calculation.
- `mikan-evaluator`: deterministic conversion from a project to a scene at a
  given time and to the complete audio graph.
- `mikan-exporter`: frame-exact H.264/AAC MP4 export using the shared evaluator,
  GPU renderer, audio graph, and FFmpeg process boundary.
- `mikan-media`: FFprobe metadata parsing, FFmpeg-backed exact-time RGBA video
  decoding, and project-rate stereo audio decoding/mixing behind replaceable
  process boundaries.
- `mikan-renderer`: a deterministic CPU reference renderer, PNG encoder, and
  shared text rasterizer used to lock down composition behavior.
- `mikan-gpu-renderer`: the `wgpu` production-renderer foundation with
  offscreen image/video/text composition, nested transforms, opacity, painter
  ordering, and RGBA readback.
- `mikan-react-bridge`: spawns the `@mikan/react` Node.js runtime as one
  long-lived process per composition and requests the evaluated `Scene` for
  each exact frame time over a JSON stdin/stdout pipe.
- `packages/react` (`@mikan/react`, TypeScript, managed with pnpm): declarative
  `Composition`, `Group`, `Image`, and `Text` components for authoring a React
  entry, evaluated through a real `react-reconciler` host, plus the
  `mikan-react-render` CLI that bundles an entry with esbuild and evaluates it
  on request. Because the reconciler drives real React rendering, ordinary
  hooks work: `useState`/`useEffect` and this package's own
  `useCurrentFrame()`, `useCurrentTime()`, and `useVideoConfig()`.
  `interpolate()` and `spring()` (plus a small `Easings` curve set) turn a
  frame number into an animated value — `spring()` is a damped harmonic
  oscillator's analytic step response, not a physics simulation stepped
  frame by frame, so it evaluates any single frame directly rather than
  needing the frames before it. A loaded
  `.mikan.json` project can also be read into a React entry: `loadProject()`
  plus `<ProjectProvider>`/`useProject()` expose it as plain data,
  `useProjectProperty(key, defaultValue)` reads its editor-set
  `properties` (falling back to `defaultValue` when the key is absent —
  there is no schema yet), and `<ProjectTimeline />` embeds its
  `video`/`image`/`text`/`component` timeline content — evaluated by
  `mikan-evaluator` (Rust), not reimplemented in TypeScript — alongside the
  entry's own React-authored content. `component` items
  (`registerComponent(name, Component)`) resolve to a real rendered
  subtree positioned at the project-evaluated transform; an unregistered
  name is left for `GpuRenderer` to reject rather than silently dropped.
  `mikan-composition` and
  `mikan-project`'s public types carry `ts-rs` bindings (behind the `codegen`
  cargo feature) that `pnpm run codegen` regenerates into
  `packages/react/src/generated`; the package's own `Scene`/`Layer`/...,
  `Project`/`Track`/... types are built on top of those generated types
  rather than hand-mirrored.
- `examples/minimal.mikan.json`: the smallest valid project.
- `examples/voiceroid.mikan.json`: a small dialogue-oriented project example.
  It includes a tiny PPM portrait placeholder so the visual dialogue path can
  be previewed without downloading assets and a generated Japanese system-voice
  WAV fixture for exercising synchronized audio preview.

## Validate the foundation

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Render the standalone smoke-test frame:

```sh
cargo run -p mikan-renderer --example hello -- hello.png
```

Launch the editor with the empty example, or pass a project path:

```sh
cargo run -p mikan-editor
cargo run -p mikan-editor -- examples/voiceroid.mikan.json
```

Export a project to MP4 without overwriting an existing file:

```sh
cargo run -p mikan-exporter -- examples/editor-demo.mikan.json output.mp4
cargo run -p mikan-exporter -- --overwrite examples/editor-demo.mikan.json output.mp4
```

Export a React composition entry instead of a project (requires a one-time
`packages/react` setup: install, generate the TypeScript bindings for
`mikan-composition`/`mikan-project`'s types, then build):

```sh
cd packages/react && pnpm install && pnpm run codegen && pnpm run build && cd ../..
cargo run -p mikan-exporter -- --react packages/react/examples/title.tsx output.mp4
```

Add `--project <project.mikan.json>` to also evaluate a companion project and
give the entry's `<ProjectTimeline />` its `video`/`image`/`text` layers:

```sh
cargo run -p mikan-exporter -- --react packages/react/examples/with-project.tsx --project examples/editor-demo.mikan.json output.mp4
```

The exporter renders the exact rational project frame times through the shared
evaluator and `GpuRenderer`, mixes the complete shared `AudioGraph`, and muxes
H.264 video with AAC audio through FFmpeg. H.264 4:2:0 output currently requires
non-zero even project dimensions. Work is staged beside the destination and is
removed on failure; a completed file is published atomically, with no-clobber
behavior unless `--overwrite` is present.

With no project argument, the editor opens `examples/editor-demo.mikan.json` so
the play/pause and single-frame controls can be exercised immediately. The
playhead is stored as an integer frame in the project's exact rational frame
rate; each new frame re-evaluates the shared scene and refreshes the GPU
preview. Timeline clips use their actual project start and duration, and the
orange ruler supports click-and-drag frame scrubbing. Selecting a clip outlines
it and shows its type, start, and duration in the inspector. Drag the body of a
selected clip to move it; drag either white edge handle to trim it. Edits are
stored as exact project frames with coalesced undo/redo history. Grabbing any
part of the clip body preserves its pointer offset; only the explicit white
handles enter trim mode. Command-S and
Command-Shift-S save atomically, and dirty documents are guarded when closing.
Assets and Inspector content scroll independently when their rows overflow.
Use Command-I or the Assets-panel Import button to select multiple local media
files. Select an imported asset and choose Add (or Command-Return) to place a
clip at the playhead using its probed source duration, with a five-second
fallback while metadata is unavailable. Compatible tracks are reused or
created automatically. Asset rows can also be dragged to an exact timeline
frame: compatible tracks highlight green, while incompatible or locked targets
show a rejection state. Clicking a track selects it as the Add target; clicking
it again returns Add to automatic track selection. The Timeline header creates
empty Video, Audio, Overlay, or Dialogue tracks; track arrows reorder them, and
dragging a clip vertically moves it between compatible unlocked tracks. Track
rows scroll below the fixed ruler when they overflow. Selecting a track exposes
Enable/Disable, Lock/Unlock, Rename, and Delete controls in the Inspector;
non-empty deletion requires confirmation and locked tracks protect all edits
until unlocked. Rename is a native GPUI text field with IME composition,
selection, grapheme-aware editing, and clipboard shortcuts. Delete the selected
timeline clip with the Timeline button, Backspace, or Forward Delete. Import
batches, insertion, and deletion
all participate in undo/redo, and locked tracks reject destructive clip edits.
Selected assets can be relinked only to the same media kind. Missing local files
are called out in the Assets panel. Removing a referenced asset requires an
explicit confirmation listing its consumers; the editor then updates dependent
clips, Dialogue audio, and character expressions atomically so undo restores the
entire operation.

Use the toolbar Export button or Command-Shift-E to choose an MP4 destination.
The editor snapshots the current project and runs `mikan-exporter` on a
dedicated worker, so preview and editing remain responsive while frame progress
is displayed. Cancel Export stops rendering or audio mixing at its next
cancellation checkpoint and removes staged output. The native save panel owns
explicit overwrite confirmation; export failures remain recoverable in the
toolbar.

Audio clips from the shared `AudioGraph` are decoded by FFmpeg, mixed at the
project sample rate, and played through the system output device in sync with
the editor transport. Timeline/source offsets, playback-rate and volume
animation, mute state, and overlapping clips are applied during mixing. GPU
preview work and audio decoding/mixing run on dedicated workers; rapid playhead
or document changes coalesce queued requests, discard stale preview results,
and cancel superseded audio mixing. Audio/dialogue clips display downsampled
peak waveforms in the timeline. Each waveform follows its source-range
start/duration and integrated playback-rate curve, so trimmed, sped-up, and
animated-rate clips remain aligned with the audio that is actually mixed.
Track headers show live source- and volume-aware level meters. Track Mute/Solo
and the toolbar's draggable, keyboard-accessible 0–200% master-volume slider are
persisted in the project, feed the shared audio graph, and participate in
undo/redo. Selecting a Video, Audio, or audio-backed Dialogue clip exposes
0–200% clip volume plus playhead-relative Add/Update/Remove keyframe and Flatten
controls in the Inspector; edits immediately update mixing and the displayed
meter envelope.
FFprobe metadata and decoded PCM stay in editor-only
caches: project JSON remains source-authored, while repeated edits can remix
cached samples without launching FFmpeg for every asset again. Decoded PCM and
source waveform peaks also use a versioned on-disk cache across editor sessions.
Entries are keyed by canonical file identity, size, modification time, sample
rate, and channel count; invalid or corrupt entries fall back to FFmpeg without
blocking playback. The cache is capped at 1 GiB; successful reads refresh
recency and saving a new entry evicts least-recently-used files until the cache
is within the limit.

The CPU renderer currently decodes local PNG, JPEG, WebP, and PNM image assets.
Construct it with `CpuRenderer::with_asset_root` to resolve project-relative
paths. Font assets in the evaluated scene are registered before shaping;
installed system fonts provide fallback for glyphs not covered by the project.
Attach `FfmpegBackend` through `CpuRenderer::with_video_decoder` to decode local
video frames. The backend expects `ffmpeg` and `ffprobe` on `PATH` by default,
or accepts explicit executable paths. Remote URL assets remain a future boundary.

`GpuRenderer` accepts the same evaluated `Scene`. Local images and injected
video frames are uploaded as GPU textures; position, anchor, scale, rotation,
group transforms, opacity, and painter order are applied by its WGSL pipeline.
Text uses the same font loading, shaping, fill, stroke, alignment, and line
layout rasterizer as the CPU reference renderer before GPU composition.

For a renderer-owned preview window, create the `wgpu::Surface` from the
window, initialize with `GpuRenderer::request_for_surface`, and call
`configure_surface` whenever the drawable size changes. `render_to_surface`
submits and presents without a CPU readback. Its `PreviewFrameStatus`
distinguishes successful, occluded, timed-out, outdated, and lost frames so the
UI event loop can recover correctly.

The GPUI editor keeps presentation ownership with GPUI. On macOS, a dedicated
worker renders offscreen, converts RGBA into IOSurface-backed NV12 CoreVideo
planes on the GPU, and gives the resulting `CVPixelBuffer` to GPUI's native
Surface element. Native bridge failure and odd project dimensions fall back to
the previous GPU readback and `RenderImage` path without changing project
evaluation or renderer inputs.
GPUI's runtime-shader feature is enabled on macOS so a separate downloadable
Xcode Metal Toolchain component is not required for local development builds.

The v0 format deliberately does not persist probed media metadata. Width,
duration, codecs, and similar facts belong to a separate editor cache so that
the project file does not become stale.

## Architecture boundaries

```text
project.json ─┐
              ├─> Composition model ─> Renderer ─> Export
React entry ──┘             ▲
                            │
                       GPUI editor
```

The composition and project crates do not depend on React, GPUI, FFmpeg, or a
GPU backend. Those integrations can evolve without changing the serialized
project contract.

A React entry evaluates directly to the same `Scene` JSON that the evaluator
produces from a project, so both sources feed the identical renderer input.
`mikan-react-bridge` spawns the `@mikan/react` Node.js CLI as one long-lived
process per composition (mirroring `mikan-media`'s sequential video decoding
session) and exchanges one JSON request/response pair per exact frame time
over its stdio pipe, rather than spawning Node once per frame. React entries
do not yet integrate with the GPUI editor's timeline/track model, project
persistence, or audio graph; `mikan-exporter --react` renders a React entry
straight to a silent MP4.

`packages/react` can also read a `.mikan.json` project file directly, through
`loadProject()`/`loadProjectFromString()` and the generated `Project` type.
`<ProjectProvider project={...}>` and `useProject()` expose that data as
plain React context. `<ProjectTimeline />` goes further and embeds the
project's own evaluated visual content (`video`/`image`/`text` timeline
items only in this first pass; `audio`, `dialogue`, and `component` items
are dropped): `mikan-exporter --react <entry> --project <project.mikan.json>`
evaluates the project once per frame through the same `mikan-evaluator` a
plain project export uses, and embeds the resulting layers directly in that
frame's request to Node — `<ProjectTimeline />` cannot ask Rust to evaluate
mid-render, since this process is synchronously blocked on that very
request's response, so a call the other way would deadlock. Because the
project's assets may live in a different directory than the React entry,
and `GpuRenderer` resolves relative asset paths against a single
`asset_root`, those evaluated layers' relative paths (and font asset paths)
are rewritten to absolute before being sent, rather than adding a second
asset root to the renderer.
