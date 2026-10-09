# Celesta implementation handoff

## Overview

Celesta is a code-first video editor. React compositions and loaded projects
are evaluated into the same Rust composition model; preview and export consume
the same renderer inputs. The GPUI application is currently preview-only.

This handoff covers the architecture, development setup, and shared constraints.
Per-task specifications, implementation details, and validation results live in
[`docs/handoff/`](docs/handoff/). Add new task notes there as
`<date>-<task>.md`; avoid duplicating their details here.

## Development setup

- Rust edition: 2024; minimum Rust version: 1.89.
- The desktop app uses `gpui-kit` (`crates/editor/Cargo.toml`).
- FFmpeg 8.1.x development libraries must be available for `ez-ffmpeg` /
  `ffmpeg-sys-next` to link against. The `ffmpeg`/`ffprobe` binaries are not
  used at runtime.
- Windows: `vcpkg install ffmpeg[x264]:x64-windows-static-md`, with
  `VCPKG_ROOT` set. Non-macOS builds enable `ez-ffmpeg`'s `static` feature.
- macOS: `brew install ffmpeg pkg-config`; desktop packaging bundles the
  required dylibs.
- Linux: `scripts/build-ffmpeg-linux.sh` builds static FFmpeg 8.1.x libraries
  with libx264. Point `PKG_CONFIG_PATH` at `<prefix>/lib/pkgconfig`; bindgen
  needs `libclang-dev`. The editor also needs `libasound2-dev`,
  `libxkbcommon-x11-dev`, and `libfontconfig-dev`.
- Linux CI uses lavapipe (`mesa-vulkan-drivers`) to run GPU tests.
  `packaging/linux/Dockerfile` builds a headless exporter image.
- Node.js >= 18 and pnpm are required for React compositions.

Build the React runtime before using `celesta-exporter --react` or the live
bridge tests:

```sh
pnpm install
pnpm --dir packages/react run codegen
pnpm run build:runtime
```

`celesta-react-bridge` spawns the compiled `packages/cli/dist/cli.js`.
`pnpm run build:runtime` builds `@celesta/cli` and every package it depends on,
in dependency order; `pnpm run test:runtime` runs their tests.
The codegen script generates `packages/react/src/generated/*.ts` from Rust
composition/project types through the `codegen` feature and ts-rs. Both
`src/generated/` and `dist/` are gitignored build output. Regenerate bindings
when their Rust types change. `.cargo/config.toml` sets the export directory
and maps large integers to TypeScript `number` for the bridge's JSON protocol.

The Celesta packages are not published to npm: user entries resolve `react`
and every `@celesta/*` package to the bundled runtime, which is `@celesta/cli`
and its dependency closure. The CLI build stages their declarations,
`@types/react`, `@types/node`, and a base tsconfig in `dist/project-types/`.
File > Set Up TypeScript copies these into a project's `.celesta/`; the folder
variant works before an entry exists. Opening a project refreshes the types
when the staged content hash changes.

`@celesta/react` is the core: the reconciler, the scene walker, primitives,
hooks, and animation helpers. The other packages build on its public API and
on `@celesta/react/internal` (render, contexts, text measurement, media probe,
host element registration), which is not for compositions. A package that
renders host elements of its own registers them with `registerHostElement()`
when its module loads, as `@celesta/character` does for `<CharacterView>` and
`<Dialogue>`; the walker rejects unregistered element types. State that the
CLI installs (the text measurer and media probe) lives in the core.
`@celesta/code` and `@celesta/voicevox` stay optional: no other package
depends on them.

## Workspace map

| Crate / package | Responsibility |
| --- | --- |
| `celesta-composition` | Renderer-independent scene types, exact rational time, transforms, animation, text styles, and audio graph types. |
| `celesta-project` | Project loading and semantic validation. |
| `celesta-evaluator` | Deterministic project-to-`Scene` and project-to-`AudioGraph` evaluation. |
| `celesta-media` | Metadata probing and exact-time video decoding through linked FFmpeg libraries. |
| `celesta-budoux` | Japanese phrase segmentation for text line breaking. |
| `celesta-renderer` | CPU reference renderer, PNG output, and shared text/path rasterization. |
| `celesta-gpu-renderer` | wgpu rendering, compositing, readback, and native preview bridge. |
| `celesta-bench` | GPU export benchmarks driven by `scripts/bench.py`; not published. |
| `celesta-exporter` | Frame-exact H.264/AAC MP4 export, including React entries. |
| `celesta-editor-core` | Read-only document, summaries, scene/audio evaluation, and timeline clock. |
| `celesta-editor-theme` | Shared desktop theme. |
| `celesta-editor` | GPUI preview app, playback, read-only panels, and export controls. |
| `celesta-remote` | Asset path resolution, remote download cache, and stylesheet font discovery. |
| `celesta-react-bridge` | Long-lived Node process and frame/component requests over stdin/stdout. |
| `packages/math` (`@celesta/math`) | Pure, React-free math utilities for compositions. |
| `packages/react` (`@celesta/react`) | Core components, hooks, animation helpers, reconciler, and scene walker. |
| `packages/shapes` (`@celesta/shapes`) | Lines, polylines, paths, circles, ellipses, and arrows. |
| `packages/layout` (`@celesta/layout`) | Layout containers and `Camera`. |
| `packages/transitions` (`@celesta/transitions`) | `Transition` and `TransitionSeries`. |
| `packages/text` (`@celesta/text`) | Text motion and box-fitted text. |
| `packages/character` (`@celesta/character`) | Characters, dialogue, subtitles, lip sync, blinking, and PSD presets. |
| `packages/media-utils` (`@celesta/media-utils`) | Media metadata helpers. |
| `packages/project` (`@celesta/project`) | Companion project data, properties, and timeline layers. |
| `packages/debug` (`@celesta/debug`) | Debug overlays. |
| `packages/code` (`@celesta/code`) | Syntax-highlighted code components. |
| `packages/voicevox` (`@celesta/voicevox`) | VOICEVOX AudioQuery lip sync. |
| `packages/cli` (`@celesta/cli`) | Entry CLI the bridge spawns; depends on every package it serves. |

Useful entry points:

- `crates/editor-core/src/document.rs`: document loading and evaluation.
- `crates/editor-core/src/clock.rs`: playback time.
- `crates/editor/src/main.rs`: application startup; view behavior is split
  across `view.rs`, `render.rs`, `playback.rs`, `preview.rs`, and the other
  editor modules.
- `crates/evaluator/src/lib.rs`: project evaluation.
- `crates/gpu-renderer/src/lib.rs`: GPU rendering and readback APIs.
- `crates/renderer/src/lib.rs`: CPU renderer and rasterizer exports.
- `packages/cli/src/cli.ts`, `packages/react/src/reconciler.ts`, and
  `packages/react/src/render.ts`: entry bundling, persistent React tree, and
  scene construction.

## Architecture boundaries

```text
Loaded project ─> Evaluator ─┐
                            ├─> Composition model ─> Renderer ─> Preview / Export
React entry ─> React bridge ┘
```

- `composition` and `project` must not depend on GPUI, React, FFmpeg, or a GPU
  backend.
- The evaluator is the project-to-scene conversion path. The editor uses the
  same evaluation and rendering model as export.
- CPU and GPU renderers consume the same evaluated `Scene`; audio remains a
  separate `AudioGraph`.
- Media probing, decoding, caching, and UI state stay outside the persisted
  project contract.
- GPU preview and audio decoding/mixing run on background workers. Requests
  carry generations, queued work coalesces to the latest request, stale
  results are discarded, and superseded mixing checks cancellation.

## React runtime constraints

The entry's default export renders a single root
`<Composition width height fps durationInFrames>`. These configuration props
must be static: mounting first renders with placeholder runtime context and
empty companion layers to discover them, discarding that pass's scene output.
Layer IDs are derived from the tree path unless an explicit `id` is supplied.

The CLI bundles entries with esbuild without type-checking user source. It
keeps `react`, its JSX runtimes, and every `@celesta/*` package external,
resolved from the CLI itself, so hooks, contexts, and registries use the same
module instances as the host. Keep React and
`react-reconciler` versions compatible; do not bump either independently.

One reconciler root persists for the lifetime of the Node process. Frame
requests update it synchronously, preserving hook state. `useCurrentFrame()`,
`useCurrentTime()`, and `useVideoConfig()` read the runtime context supplied for
the requested time. Frame requests can arrive out of order; animation should
be derived from frame/time rather than accumulated state.

An entry may export async `prepare()`, awaited once before mounting and frame
requests. Store prepared data in module state and read it synchronously during
rendering. Media preload and other workflow helpers are documented in
`docs/handoff/`.

`interpolate()` and `spring()` are pure animation functions. The spring uses
an analytical damped-oscillator response, allowing direct evaluation of any
frame; `durationInFrames` clamps late frames to the settled value. Rust easing
integrals use closed forms for the original four curves and numerical
integration for the extended catalogue.

Companion-project integration uses Rust-evaluated layers supplied in each
frame request. `<ProjectTimeline />`, `<ProjectTrack />`, and
`useProjectTrack()` consume these layers; `useProject()` reads an explicitly
provided `ProjectProvider`. Do not initiate nested Rust evaluation from a React
render: Rust is already waiting for Node's response, so that would deadlock.
Companion assets are resolved against the companion's root before joining a
scene whose asset root is the React entry's directory.

`registerComponent()` resolves named component layers into React subtrees,
preserving their evaluated transform and opacity. Unregistered components
remain unresolved and cause export to fail; editor preview shows warnings.
Component property schemas are Inspector metadata. The project property schema
also types the values an entry receives from `--props`, `--props-file`, and a
companion project's `properties`: the bridge sends them on the first stdin
line (`--properties-stdin`), and the CLI checks them after the entry's module
scope runs and before `prepare()`. See
[`docs/handoff/2026-10-08-project-property-inputs.md`](docs/handoff/2026-10-08-project-property-inputs.md).
The current Inspector is read-only.

Sequence timing, per-frame audio, standalone preview, coordinate conventions,
fonts, text helpers, shapes, subtitles, and other feature specifications are
covered by the dated handoff notes.

## Renderer and export constraints

- Single-line text is cropped vertically to visible glyphs while retaining
  advance width, including leading/trailing spaces. Multiline text retains
  its layout box. Baseline anchoring uses the first line's baseline.
- Blend modes operate on non-premultiplied color values. Groups with a
  non-normal blend mode are isolated, and their opacity applies to the
  composited group. GPU isolation uses pooled canvases bounded to the affected
  region, with a scene-sized root canvas for blending/effects.
- Consecutive layers sharing a texture can batch into an instanced draw.
  Textures are cached across frames by their inputs; unused entries are
  dropped. Text cache keys include the loaded-font count. React host child
  insertion must move existing children rather than duplicate them.
- Group clips follow the group's transform and intersect through nested
  clips. GPU clip nesting is limited to eight levels. General group masks
  are not implemented. Distance scaling is exact for uniform scales and
  approximate for non-uniform scales.
- Paths use SVG-style absolute commands with non-zero fill and stroked
  outlines. Their position is the origin; anchors are ignored. Ordinary GPU
  paths use tiled coverage; tiles exceeding 64 edges or paths exceeding the
  remaining storage budget fall back to CPU raster textures. See
  [`docs/performance/gpu-path-coverage.md`](docs/performance/gpu-path-coverage.md)
  for the coverage algorithm, comparisons, and measurements.
- Video overruns hold the final decoded frame; audio overruns produce silence.
- Export evaluates exact rational frame times and uses the shared audio graph.
  Work is staged beside the destination, cleaned on failure/cancellation, and
  atomically published without overwriting unless explicitly requested.
  H.264 yuv420p output requires non-zero even dimensions.
- `--from`/`--to` export a clamped, frame-snapped composition span. Output starts
  at its own zero time, with the audio graph shifted by the window start.
- `--preset` and `--crf` control libx264 encoding (defaults: `medium`, 18).
  `--color-conversion auto|gpu|encoder` chooses yuv420p conversion; auto uses
  GPU conversion unless the renderer is software. GPU readback returns I420
  at 1.5 bytes/pixel and uses BT.601 limited range. Its 2x2 chroma averaging
  can differ from the encoder's filtering at hard color edges.
- Remote HTTP/HTTPS assets are downloaded once into the per-user cache and
  are not revalidated. Font loading supports stylesheet `@font-face` sources
  and WOFF/WOFF2 unpacking.
- Audio preview caches decoded PCM and source peaks. The versioned persistent
  cache is capped at 1 GiB; stale/corrupt entries and cache I/O errors are
  recoverable misses. Waveforms use the same source-time mapping as playback.

## Native preview and GPUI constraints

GPUI owns the window's presentation surface. Do not create and present a
second wgpu surface for that same window.

On macOS, the native bridge renders an offscreen RGBA scene, converts it to
full-range BT.601 NV12 in GPU passes, and hands an IOSurface-backed
`CVPixelBuffer` to GPUI's Surface element after the wgpu submission completes.
CoreVideo Metal textures are retained exactly once for wgpu, and their
`CVMetalTexture` owners remain alive through external-resource drop callbacks.

Bridge failures and odd dimensions fall back to GPU RGBA readback,
RGBA-to-BGRA conversion, and `RenderImage`. Renderer-owned surfaces remain
available for separate preview windows.

Call `Window::request_animation_frame()` only during layout/prepaint/paint,
never directly from input handlers. The playback handler changes state and
calls `cx.notify()`; rendering then calls `update_playback`, which requests the
animation frame. Calling it from a mouse callback previously caused a panic
across the Objective-C boundary and terminated the app.

## Examples and validation

`packages/cli/examples/` contains focused runtime and integration fixtures;
`examples/` contains larger compositions and project fixtures.
`examples/assets/voices/001.wav` is generated from the macOS Kyoko system voice,
not a licensed VOICEROID sample.

Typical preview and export commands, run from the repository root:

```sh
cargo run -p celesta-editor -- packages/cli/examples/title.tsx
cargo run -p celesta-exporter -- --react packages/cli/examples/title.tsx output.mp4
```

Inspect `git status --short` before editing. Use targeted validation while
iterating; the Rust baseline is:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check
```

When composition/project types change, also run:

```sh
cargo test -p celesta-project -p celesta-composition --features codegen
cargo clippy -p celesta-composition -p celesta-project --all-targets --features codegen -- -D warnings
```

For React changes, regenerate and build the runtime, then run its tests:

```sh
cd packages/react
pnpm run codegen
pnpm run build
pnpm test
```

Live bridge tests require Node and built React runtime dependencies/output;
check for skipped integration tests. GPU tests need an available backend, and
native UI checks need a display and suitable platform permissions. Record
actual checks and environment limitations in the task's dated note rather
than carrying historical test counts or machine-specific results here.

## Working conventions

- Preserve unrelated or pre-existing worktree changes.
- Keep detailed task notes in `docs/handoff/`.
- Do not commit unless the user explicitly asks.
- If a commit is requested, stage only accepted files.
