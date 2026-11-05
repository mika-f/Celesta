# Celesta implementation handoff

## Overview

Celesta is a code-first video editor. React compositions and loaded projects
are evaluated into the same Rust composition model; preview and export consume
the same renderer inputs. The GPUI application previews and exports; for React
compositions it also edits layer props on the canvas and in the Properties tab
by writing them back into the source (`celesta-source-edit`).

This handoff covers the architecture, development setup, and shared constraints.
Per-task specifications, implementation details, and validation results live in
[`docs/handoff/`](docs/handoff/). Add new task notes there as
`<date>-<task>.md`; avoid duplicating their details here.

## Development setup

- Rust edition: 2024; minimum Rust version: 1.89.
- The desktop app uses `gpui-kit` (`crates/editor/Cargo.toml`).
- FFmpeg 8.1.x development libraries must be available for `ez-ffmpeg` /
  `ffmpeg-sys-next` to link against. They are needed only for the codec
  plugin, `plugins/ffmpeg` (`celesta-codec-ffmpeg`), and the plugins
  workspace's tests; the root workspace links no FFmpeg. Celesta's own crates
  read and write media through the plugin's process, and their tests use the
  lossless test codec (`celesta-codec-test`) instead (see
  [`docs/handoff/2026-11-05-codec-plugin.md`](docs/handoff/2026-11-05-codec-plugin.md)).
  The `ffmpeg`/`ffprobe` binaries are not used at runtime.
- Windows: `vcpkg install ffmpeg[x264,vpx,opus]:x64-windows-static-md`, with
  `VCPKG_ROOT` set. Non-macOS builds enable `ez-ffmpeg`'s `static` feature.
- macOS: `brew install ffmpeg@8 pkg-config`, then
  `export PKG_CONFIG_PATH="$(brew --prefix ffmpeg@8)/lib/pkgconfig"` (the
  formula is keg-only); desktop packaging bundles the required dylibs.
- Linux: `scripts/build-ffmpeg-linux.sh` builds static FFmpeg 8.1.x libraries
  with libx264, libvpx, and libopus. Point `PKG_CONFIG_PATH` at
  `<prefix>/lib/pkgconfig`; bindgen
  needs `libclang-dev`. The editor also needs `libasound2-dev`,
  `libxkbcommon-x11-dev`, and `libfontconfig-dev`.
- Linux CI uses lavapipe (`mesa-vulkan-drivers`) to run GPU tests.
  `packaging/linux/Dockerfile` builds a headless exporter image.
- Node.js and pnpm are required for React compositions. The runtime packages
  declare Node.js >= 18; development and CI use the versions pinned in
  `mise.toml` (Node.js 26, pnpm 12).

Build the React runtime before using `celesta-exporter --react` or the live
bridge tests:

```sh
pnpm install
pnpm run build:runtime
```

`celesta-react-bridge` spawns the compiled `packages/cli/dist/cli.js`.
`pnpm run build:runtime` runs Turborepo's `build` for `@celesta.studio/cli` and every
package it depends on, in dependency order; `pnpm run test:runtime` runs their
tests. `@celesta.studio/react`'s build first runs its `codegen` script, which generates
`packages/react/src/generated/*.ts` from Rust composition/project types through
the `codegen` feature and ts-rs. Both `src/generated/` and `dist/` are
gitignored build output. Turborepo caches `codegen` by the Cargo manifests,
lockfile, `.cargo/config.toml`, and the `composition` and `project` crates, so
`build:runtime` regenerates the bindings only when those change; a package's
own `pnpm run build` skips codegen. `.cargo/config.toml` sets the export
directory and maps large integers to TypeScript `number` for the bridge's JSON
protocol.

The Celesta packages are not published to npm: user entries resolve `react`
and every `@celesta.studio/*` package to the bundled runtime, which is `@celesta.studio/cli`
and its dependency closure. The CLI build stages their declarations,
`@types/react`, `@types/node`, and a base tsconfig in `dist/project-types/`.
File > Set Up TypeScript copies these into a project's `.celesta/`; the folder
variant works before an entry exists. Opening a project refreshes the types
when the staged content hash changes.
The packages were named `@celesta/*` before they took the `@celesta.studio`
npm scope; the CLI, `@celesta.studio/web`, the staged tsconfig, and
`celesta-source-edit` still accept the old names, and the editor and the
exporter warn about each file that uses them. See
[`docs/handoff/2026-11-04-celesta-studio-scope.md`](docs/handoff/2026-11-04-celesta-studio-scope.md).

`@celesta.studio/react` is the core: the reconciler, the scene walker, primitives,
hooks, and animation helpers. The other packages build on its public API and
on `@celesta.studio/react/internal` (render, contexts, text measurement, media probe,
host element registration), which is not for compositions. A package that
renders host elements of its own registers them with `registerHostElement()`
when its module loads, as `@celesta.studio/character` does for `<CharacterView>` and
`<Dialogue>`; the walker rejects unregistered element types. State that the
CLI installs (the text measurer and media probe) lives in the core.
Ready-made, high-level components that compose the primitives (such as
`<FrameChrome>`) go in `@celesta.studio/components` rather than the core, and use
only the public APIs of the core and the other `@celesta.studio/*` packages, never
`@celesta.studio/react/internal`.
`@celesta.studio/code`, `@celesta.studio/voicevox`, and `@celesta.studio/aivis-speech` stay optional imports for
compositions: no package a composition imports depends on them, though
`@celesta.studio/cli` includes them in the runtime.

## Workspace map

| Crate / package | Responsibility |
| --- | --- |
| `celesta-composition` | Renderer-independent scene types, exact rational time, transforms, animation, text styles, and audio graph types. |
| `celesta-project` | Project loading and semantic validation. |
| `celesta-evaluator` | Deterministic project-to-`Scene` and project-to-`AudioGraph` evaluation. |
| `celesta-media` | Probing, exact-time video frames, and audio decoding through a codec plugin (`MediaBackend`, `CodecProvider`, the codec protocol), plus the audio mix, ducking, time stretching, and loudness. Links no codec library. |
| `celesta-codec-ffmpeg` | `plugins/ffmpeg`, the FFmpeg codec plugin: a program serving the codec protocol (probe, decode, encode, mux, HDR tone mapping), and `FfmpegCodec`, through which its own tests drive the exporter. The only crate that links FFmpeg. A member of the plugins workspace, `plugins/Cargo.toml`, with Celesta's version; built into a plugin directory by `scripts/plugins/build.mjs`. |
| `celesta-codec-test` | The lossless test codec the core's tests read and write media with, in-process (`install`) or as the program `celesta-codec-test` in a temporary plugin folder (`TestCodecPlugin`); not published. |
| `celesta-budoux` | Japanese phrase segmentation for text line breaking. |
| `celesta-renderer` | CPU reference renderer, PNG output, and shared text/path rasterization. |
| `celesta-gpu-renderer` | wgpu rendering, compositing, readback, and native preview bridge. |
| `celesta-bench` | GPU export benchmarks driven by `scripts/bench.py`; not published. |
| `celesta-exporter` | Frame-exact H.264/AAC MP4 export, including React entries. |
| `celesta-mcp` | `Celesta-mcp`, the MCP server agents use to inspect, render, and export. |
| `celesta-editor-core` | Read-only document, summaries, scene/audio evaluation, and timeline clock. |
| `celesta-editor-theme` | Shared desktop theme. |
| `celesta-editor` | GPUI app: preview, playback, panels, canvas and Properties editing, and export controls. |
| `celesta-source-edit` | Locates JSX elements in user TSX source and applies byte-span edits for the editor's write-back. |
| `celesta-remote` | Asset path resolution, remote download cache, and stylesheet font discovery. |
| `celesta-react-bridge` | Long-lived Node process and frame/component requests over stdin/stdout. |
| `celesta-plugin` | Content plugins' 2D meshes and 3D scenes, the `ContentProvider` interface, and the plugin protocol's framing and `hello` (`serve_requests`, which the codec role uses too); depends on serde only. See [`docs/handoff/2026-11-01-plugin-system.md`](docs/handoff/2026-11-01-plugin-system.md) and [`docs/handoff/2026-11-03-plugin-scene-content.md`](docs/handoff/2026-11-03-plugin-scene-content.md). |
| `celesta-plugin-host` | Finds plugin directories, decides which may run (the user's settings: trusted projects, allowed plugins), runs each content plugin in its own process for the renderer, and chooses the codec plugin (`InstalledPlugins::codec`). |
| `celesta-plugin-flag` | `examples/plugins/flag`, a sample plugin program; with `beep-voice` beside it, built by `scripts/plugins/build.mjs`. |
| `celesta-plugin-turntable` | `examples/plugins/turntable`, a sample plugin program that returns a 3D scene; built by `scripts/plugins/build.mjs`. |
| `celesta-plugin-vrm` | `plugins/vrm`, the VRM avatar plugin's program, with `<Vrm>` in `plugins/vrm/js` (`celesta-plugin-vrm-js`). A member of the first-party plugins' workspace, `plugins/Cargo.toml`, which the root workspace excludes; built into a plugin directory by `scripts/plugins/build.mjs`. See [`docs/handoff/2026-11-03-plugin-vrm.md`](docs/handoff/2026-11-03-plugin-vrm.md). |
| `packages/math` (`@celesta.studio/math`) | Pure, React-free math utilities for compositions. |
| `packages/react` (`@celesta.studio/react`) | Core components, hooks, animation helpers, reconciler, and scene walker. |
| `packages/shapes` (`@celesta.studio/shapes`) | Lines, polylines, paths, circles, ellipses, arrows, triangles, polygons, stars, pies, and hearts, plus generators for their path commands. |
| `packages/lottie` (`@celesta.studio/lottie`) | `<Lottie>`: Lottie JSON animations drawn as paths, groups, and masks; see [`docs/handoff/2026-10-19-lottie.md`](docs/handoff/2026-10-19-lottie.md). |
| `packages/shader` (`@celesta.studio/shader`) | `defineShader`: custom WGSL filters for a layer's `shader` prop. |
| `packages/layout` (`@celesta.studio/layout`) | Layout containers and `Camera`. |
| `packages/components` (`@celesta.studio/components`) | High-level components built on the core's public API, such as `FrameChrome`, `PatternBackground`, `Panel`, `Badge`, `Counter`, `StatCard`, `BarChart`, `Callout`, `MediaFrame`, and `CaptionTrack`. |
| `packages/transitions` (`@celesta.studio/transitions`) | `Transition` and `TransitionSeries`. |
| `packages/text` (`@celesta.studio/text`) | Text motion, box-fitted text, emphasis by grapheme range, and timed frame ranges shared by captions and dialogue subtitles. |
| `packages/character` (`@celesta.studio/character`) | Characters, dialogue, subtitles, lip sync, blinking, and PSD presets. |
| `packages/media-utils` (`@celesta.studio/media-utils`) | Media metadata helpers. |
| `packages/project` (`@celesta.studio/project`) | Companion project data, properties, and timeline layers. |
| `packages/debug` (`@celesta.studio/debug`) | Debug overlays. |
| `packages/code` (`@celesta.studio/code`) | Syntax-highlighted code components. |
| `packages/voicevox` (`@celesta.studio/voicevox`) | VOICEVOX AudioQuery lip sync. |
| `packages/aivis-speech` (`@celesta.studio/aivis-speech`) | AivisSpeech AudioQuery and WAV lip sync. |
| `packages/cli` (`@celesta.studio/cli`) | Entry CLI the bridge spawns; depends on every package it serves. |
| `packages/exporter` (`@celesta.studio/exporter`) | Node.js API that runs `celesta-exporter --json` for scripts; not part of the runtime. |
| `packages/web` (`@celesta.studio/web`) | Browser evaluator, Canvas preview, and WebCodecs MP4 export; see [`packages/web/README.md`](packages/web/README.md). |
| `packages/website` (`@celesta.studio/website`) | Official website, English and Japanese user guide, and playground. |
| `scripts/plugins` (`celesta-plugin-scripts`) | The plugin build script, `build.mjs`, which makes the plugin directory Celesta loads from a plugin's folder, and its helpers `platform.mjs`, `cargo.mjs`, `manifest.mjs`, and `licenses.mjs`; `bump-version.mjs`, with its helper `version.mjs`, which bumps Celesta's version and the plugins that follow it; `affected.mjs`, with its helpers `inputs.mjs` and `lockfile.mjs`, which lists the first-party plugins a change affects, from `plugins/Cargo.lock` or a plugin's own lockfile, the pnpm graph, and `turbo.json`, for CI's `Plugins (<OS>)` jobs; and `libtest.mjs`, which finds the skipped tests in a `cargo test -- --show-output` log for `scripts/report-skipped-tests.mjs`; not published. |

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
- No Celesta crate but `celesta-codec-ffmpeg` links FFmpeg (or any codec
  library), even optionally: media goes through `celesta_media::MediaBackend`
  and the codec plugin's process, so the editor, exporter, and MCP server
  stay MIT/Apache-2.0 when packages ship a GPL FFmpeg. The core's tests use
  `celesta-codec-test`; only `celesta-codec-ffmpeg`'s own tests link FFmpeg.
- GPU preview and audio decoding/mixing run on background workers. Requests
  carry generations, queued work coalesces to the latest request, stale
  results are discarded, and superseded mixing checks cancellation.

## React runtime constraints

The entry's default export renders a single root
`<Composition width height fps durationInFrames>`. These configuration props
must be static: mounting first renders with placeholder runtime context and
empty companion layers to discover them, discarding that pass's scene output.
Layer IDs are derived from the tree path unless an explicit `id` is supplied.

The editor's preview bridge passes `--source-locations`, which bundles with the
JSX dev runtime so a `layerSource` request can answer where a layer was
written; see [Open a layer's source](docs/handoff/2026-10-14-open-layer-source.md).

The CLI bundles entries with esbuild without type-checking user source. It
keeps `react`, its JSX runtimes, and every `@celesta.studio/*` package external,
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
The editor's Properties tab and canvas tools edit props by rewriting the
source; see [`docs/handoff/2026-10-23-property-editing-engine.md`](docs/handoff/2026-10-23-property-editing-engine.md)
and the notes it links.

Sequence timing, per-frame audio, standalone preview, coordinate conventions,
fonts, text helpers, shapes, subtitles, and other feature specifications are
covered by the dated handoff notes.

## Renderer and export constraints

- Single-line text is cropped vertically to visible glyphs while retaining
  advance width, including leading/trailing spaces. Multiline text retains
  its layout box. Baseline anchoring uses the first line's baseline.
- Blend modes operate on non-premultiplied color values. Groups with a
  non-normal blend mode or an opacity below 1 are isolated, and their opacity
  applies to the composited group, as CSS `opacity` does; a group whose only
  child is one non-group, non-instances shape blended normally passes its
  opacity down instead (`Layer::isolates_group`, mirrored in `@celesta.studio/web`).
  GPU isolation uses pooled canvases bounded to the affected
  region, with a scene-sized root canvas for blending/effects.
- Consecutive layers sharing a texture can batch into an instanced draw.
  Textures are cached across frames by their inputs; unused entries are
  dropped. Text cache keys include the rasterizer's `fonts_revision()`. React host child
  insertion must move existing children rather than duplicate them.
- Group clips follow the group's transform and intersect through nested
  clips. GPU clip nesting is limited to eight levels. Distance scaling is
  exact for uniform scales and approximate for non-uniform scales.
- `<Group mask>` / `<Mask>` show a group's children where another subtree
  draws, by alpha or luminance, optionally inverted. Masked groups are
  always isolated, in the GPU and CPU renderers and in `@celesta.studio/web`.
- Layers and groups take `skewX`/`skewY`, `rotationX`/`rotationY`,
  `perspective`, and `backfaceVisibility`. Skew and orthographic 3D rotation
  are affine; a layer with both a perspective and a 3D rotation draws through
  a plane canvas and is projected onto its parent, in the GPU renderer and in
  `@celesta.studio/web`. The CPU reference renderer refuses skew and 3D rotation. See
  [`docs/handoff/2026-10-16-skew-3d-transform.md`](docs/handoff/2026-10-16-skew-3d-transform.md).
  A plane too large for one canvas at the scale its nearest part needs is
  drawn in bands of equal depth, and a GPU text layer too large for one
  texture is rasterized only where it shows; see
  [`docs/handoff/2026-10-16-plane-density-bands.md`](docs/handoff/2026-10-16-plane-density-bands.md).
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
  `--bit-depth 10` encodes yuv420p10le (High 10): the GPU converts to 10-bit
  planes (`ReadbackFormat::Yuv420p10`), or reads back 16-bit RGBA
  (`ReadbackFormat::Rgba16`, rgba64le) for the encoder to convert.
  `--color-conversion auto|gpu|encoder` chooses yuv420p conversion; auto uses
  GPU conversion unless the renderer is software. GPU readback returns I420
  at 1.5 bytes/pixel and uses BT.601 limited range. Its 2x2 chroma averaging
  can differ from the encoder's filtering at hard color edges.
- GPU canvases are `Rgba16Float` where the adapter can render and blend it
  (`canvas_format`), and every readback (RGBA, I420, and preview targets) is
  rounded to 8 bits once, with an ordered dither, so gentle gradients do not
  band. GPU gradients are not rounded to whole codes as the CPU renderer's
  are. See
  [`docs/handoff/2026-10-12-gradient-dither.md`](docs/handoff/2026-10-12-gradient-dither.md).
- HDR (PQ, HLG) and BT.2020 video is decoded to 16-bit RGB and tone mapped
  to 8-bit sRGB in `celesta-media`, matching Chrome's canvas drawing, so the
  renderers only ever see SDR frames. See
  [`docs/handoff/2026-10-16-hdr-video-import.md`](docs/handoff/2026-10-16-hdr-video-import.md).
- Remote HTTP/HTTPS assets are downloaded once into the per-user cache and
  are not revalidated. Font loading supports stylesheet `@font-face` sources
  and WOFF/WOFF2 unpacking.
- Audio preview caches decoded PCM and source peaks. The versioned persistent
  cache is capped at 1 GiB; stale/corrupt entries and cache I/O errors are
  recoverable misses. Waveforms use the same source-time mapping as playback.
- Video exports measure the mix's integrated loudness and true peak, and
  `--loudness` brings the whole mix to a target with a true-peak limiter.
  See
  [`docs/handoff/2026-10-22-export-loudness.md`](docs/handoff/2026-10-22-export-loudness.md).

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
node scripts/plugins/build.mjs plugins/ffmpeg
export CELESTA_PLUGIN_PATH="$PWD/plugins/dist"   # PowerShell: $env:CELESTA_PLUGIN_PATH = "$PWD\plugins\dist"
cargo run -p celesta-editor -- packages/cli/examples/title.tsx
cargo run -p celesta-exporter -- --react packages/cli/examples/title.tsx output.mp4
```

The editor and exporter find the codec plugin through `CELESTA_PLUGIN_PATH`,
which must be an absolute path and is set by hand in each shell; rebuild the
plugin after changing `plugins/ffmpeg` or `celesta-media`'s codec protocol.
A `celesta-codec-ffmpeg` program beside Celesta's executables serves only
`cargo install`. Delete any `celesta-codec-ffmpeg` left in
`target/<profile>` from when the codec was a root crate: Celesta would use
it and hide a missing `CELESTA_PLUGIN_PATH`. Tests whose `celesta-exporter` or
`celesta-mcp` child exports video give it the test codec's program instead, so
they need
`celesta-codec-test` built, which `cargo test --workspace` does; otherwise run
`cargo build -p celesta-codec-test` first.

Inspect `git status --short` before editing. Use targeted validation while
iterating; the Rust baseline is:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check
```

The root `cargo test --workspace` leaves out the first-party plugins'
workspace; after changing it, a plugin, or the plugin build script, run
`cargo test --manifest-path plugins/Cargo.toml --workspace --locked`,
`cargo clippy --manifest-path plugins/Cargo.toml --workspace --all-targets --locked -- -D warnings`,
`cargo fmt --check` in `plugins/`, and `pnpm --dir scripts/plugins test`.
The plugins workspace's `--workspace` checks build the codec plugin, so they
need FFmpeg on `PKG_CONFIG_PATH` (or `VCPKG_ROOT` on Windows).
On a pull request, CI's `Plugins (<OS>)` jobs run these checks with `-p` for
the affected plugins only; a push to `main` and a manual run test every
plugin. The scripts' own tests run in the root `Test` job. To see which
plugins a branch affects, commit and run
`node scripts/plugins/affected.mjs --base "$(git merge-base origin/main HEAD)"`.
A plugin's inputs are its folder, its lockfile's local packages, its `js/`
package's workspace dependencies with the `$TURBO_ROOT$/` paths `turbo.json`
gives their tasks, and its `licenses` files, so a module's `js/package.json`
must declare every workspace package its build uses.

For a release, run `node scripts/plugins/bump-version.mjs X.Y.Z`, then set
the version in each `packages/*/package.json` by hand.

When composition/project types change, also run:

```sh
cargo test -p celesta-project -p celesta-composition --features codegen
cargo clippy -p celesta-composition -p celesta-project --all-targets --features codegen -- -D warnings
```

For React changes, build the runtime (which regenerates the bindings when the
Rust types changed), then run its tests:

```sh
pnpm run build:runtime
pnpm --dir packages/react test
```

Live bridge tests require Node and built React runtime dependencies/output;
check for skipped integration tests. GPU tests need an available backend, and
native UI checks need a display and suitable platform permissions. Record
actual checks and environment limitations in the task's dated note rather
than carrying historical test counts or machine-specific results here.

## Working conventions

- Preserve unrelated or pre-existing worktree changes.
- Keep detailed task notes in `docs/handoff/`.
- For a clippy finding, check its cause and how the code is used, and weigh
  the suggested change against keeping the code (see
  `.agents/skills/rust-best-practices`). To keep the code, put
  `#[expect(clippy::…, reason = "…")]` on the item. Unlike `allow`, it fails
  under `-D warnings` once the lint stops firing, but nothing flags a reason
  that has gone stale.
- Do not pass CI with `-A` on the command line or a `clippy.toml` threshold.
- Do not commit unless the user explicitly asks.
- If a commit is requested, stage only accepted files.
