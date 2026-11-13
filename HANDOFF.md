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
  `ffmpeg-sys-next` to link against. They are needed only for the FFmpeg
  media plugin, `plugins/ffmpeg` (`celesta-codec-ffmpeg`), and its tests, which
  the per-build plugin checks include. On macOS the
  AVFoundation plugin's tests (`plugins/avfoundation`) need them too: they
  read its files back with the FFmpeg plugin and libavformat. The root
  workspace and the VRM plugin link no FFmpeg. Celesta's own crates
  read and write media through the plugin's process, and their tests use the
  lossless test codec (`celesta-codec-test`) instead (see
  [`docs/handoff/2026-11-05-codec-plugin.md`](docs/handoff/2026-11-05-codec-plugin.md),
  [`docs/handoff/2026-11-08-media-decoders.md`](docs/handoff/2026-11-08-media-decoders.md),
  and [`docs/handoff/2026-11-08-media-encoders.md`](docs/handoff/2026-11-08-media-encoders.md)).
  The `ffmpeg`/`ffprobe` binaries are not used at runtime.
- Windows: the standard distribution uses
  `ffmpeg[zlib,opus,vpx]:x64-windows-static-md-release`, and the optional
  x264 build `ffmpeg[x264]` lives in its own vcpkg root; `VCPKG_ROOT` selects
  standard and `FFMPEG_X264_VCPKG_ROOT` the x264 build. CI downloads both as
  the pinned NuGet packages `Celesta.FFmpeg.Windows.Standard` and
  `Celesta.FFmpeg.Windows.X264` (`.github/actions/setup-ffmpeg`), which
  `.github/workflows/ffmpeg-windows.yml` builds with the vcpkg release it
  names and publishes to GitHub Packages. Non-macOS builds enable `ez-ffmpeg`'s `static` feature.
  Also needed there:
  - Git for Windows with `git config --global core.autocrlf false` before
    cloning (the repository has no `.gitattributes`, and CRLF files fail
    CI's `cargo fmt --check` on Linux);
  - Visual Studio 2022 Build Tools, "Desktop development with C++",
    including a Windows 11 SDK (10.0.22621 or later), for MSVC;
  - `rustup toolchain install 1.99.1 --component clippy,rustfmt` and
    `rustup override set 1.99.1` in the checkout (`mise.toml` pins Rust
    through `mr-boxington`, which is macOS-only);
  - Node.js and pnpm at `mise.toml`'s versions, through `mise` with
    `MISE_DISABLE_TOOLS=rust,mr-boxington` (as CI sets it) or the official
    installers;
  - LLVM (`winget install LLVM.LLVM`) for `ffmpeg-sys-next`'s bindgen, with
    `LIBCLANG_PATH` set to `C:\Program Files\LLVM\bin` if `libclang` is not
    found;
  - a GPU with a current driver: the media plugins' export tests skip
    without one.
- macOS: `brew install ffmpeg@8 pkg-config`, then
  `export PKG_CONFIG_PATH="$(brew --prefix ffmpeg@8)/lib/pkgconfig"` (the
  formula is keg-only) for local use, including its GPL build. Distribution
  builds instead use independent prefixes from `scripts/build-ffmpeg-linux.sh`
  with `FFMPEG_VARIANT=standard|x264` (standard links Homebrew's libopus and
  libvpx); desktop packages bundle the standard dylibs and the x264 download
  carries its own.
- The AVFoundation media plugin, `plugins/avfoundation`
  (`celesta-codec-avfoundation`), builds on macOS only and links only
  Apple's frameworks (no FFmpeg; `celesta-codec-ffmpeg` is a macOS-only
  dev-dependency of its tests). Its `plugin.json` has `"os": ["macos"]`, so
  `scripts/plugins/build.mts` skips it elsewhere (`avfoundation: skipped, it
  is built for macos only`); on Linux and Windows its crate builds a stub
  program that exits with an error, so the plugins workspace still builds
  on both. The macOS package puts it in `Contents/Resources/PlugIns` beside
  `ffmpeg` (`scripts/package-macos.py`, no `dylibbundler`); see
  [`docs/handoff/2026-11-09-media-avfoundation.md`](docs/handoff/2026-11-09-media-avfoundation.md).
- The Media Foundation media plugin, `plugins/mediafoundation`
  (`celesta-codec-mediafoundation`), is its Windows twin: it builds on
  Windows only and links only Windows' system libraries (Media Foundation
  and COM, through the `windows` crate; `celesta-codec-ffmpeg` is a
  Windows-only dev-dependency of its tests). Its `plugin.json` has
  `"os": ["windows"]`, so `build.mts` skips it elsewhere (`mediafoundation:
  skipped, it is built for windows only`); on macOS and Linux its crate
  builds a stub program that exits with an error. It has no muxer: its encoders write
  into another plugin's (`ffmpeg/mp4`). The Windows package puts it in
  `plugins\mediafoundation` beside `plugins\ffmpeg`
  (`scripts/package-windows.ps1`, with its own copies of the MSVC runtime
  DLLs); see
  [`docs/handoff/2026-11-10-media-windows.md`](docs/handoff/2026-11-10-media-windows.md).
- CI's Plugins job runs Clippy on every system, so each OS-native plugin is
  linted on its own system (rustfmt, Linux only, formats their gated code
  too).
- Linux: `scripts/build-ffmpeg-linux.sh` builds static FFmpeg 8.1.x libraries
  with `FFMPEG_VARIANT=standard|x264`, defaulting to standard (LGPL with zlib,
  libopus, and libvpx, which link dynamically from the distribution's
  packages; x264 is GPL with libx264).
  Prefixes default to `/opt/ffmpeg8-<variant>` and must stay separate. Point `PKG_CONFIG_PATH` at
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
when the staged content hash changes. Both use `celesta_plugin_host::project_root`
and never write above it; see
[`docs/handoff/2026-11-06-project-types-root.md`](docs/handoff/2026-11-06-project-types-root.md).
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
| `celesta-media` | Media through plugins: probing, exact-time video frames, and audio decoding (`MediaBackend`, decoder routing), the component model (`MediaRegistry` with its `describe` cache, `MediaProvider` and the `Decoder`, `VideoEncoder`, `AudioEncoder`, and `Muxer` traits, `MediaProcess`, plugin protocol 2's messages), pipelined export requests (`Pipeline`, `PIPE_WINDOW`, and the ring of frame slots an export process gets, `ProviderUse::Export { ring }` sized by `ring_layout`), the settings schema and its validation, and audio resampling; plus the audio mix, ducking, time stretching, and loudness. Links no codec library. |
| `celesta-media-image` | The built-in image components, registered in every exporter registry by `celesta_exporter::with_builtin` (plugin id `builtin`, `MediaOrigin::BuiltIn`, no folder or process): the video encoders `builtin/png` (`compression`, `depth`, `transparent`), `builtin/jpeg` (`quality`), and `builtin/webp` (`lossless`, `quality`, `transparent`), and the muxers `builtin/image` (one file) and `builtin/imageSequence` (`sequence: true`, one file per frame from a `frame-%06d.<ext>` pattern), which serve still exports only; see [`docs/handoff/2026-11-09-media-stills.md`](docs/handoff/2026-11-09-media-stills.md). |
| `celesta-codec-ffmpeg` | Shared FFmpeg implementation and the standard process in `plugins/ffmpeg`: decoder `ffmpeg/ffmpeg`, ProRes, VP9, AAC, Opus, PCM, MP4/MOV/WebM muxers. App distributions bundle its LGPL build (zlib, libopus, libvpx). The optional native build and process `celesta-codec-ffmpeg-x264` (`plugins/ffmpeg-x264`) provides `ffmpeg-x264/libx264` and is a separate download; `Profile::components` in `plugins/ffmpeg/src/lib.rs` assigns each component to one of the two programs, taking each encoder's from its exhaustive `VideoCodec::profile` / `AudioCodec::profile`. Local builds may use a system GPL FFmpeg, while distribution validation explicitly checks licenses and opens encoders. |
| `celesta-codec-avfoundation` | `plugins/avfoundation`, the macOS media plugin: a program serving protocol 2 with VideoToolbox's video encoders `h264`, `hevc`, and `prores`, AudioToolbox's `aac`, AVAssetWriter's muxers `mp4` and `mov`, and an AVAssetReader decoder `avfoundation/avfoundation` (`mp4`, `mov`, `m4a`), through `objc2` bindings; `AvfCodec`, through which its own tests drive the exporter. MIT or Apache-2.0; `plugin.json`'s `"os": ["macos"]`. FFmpeg ranks before it among bundled plugins, so its muxers, AAC, HEVC, and ProRes serve exports only when named; its H.264, though, is the default MP4 encoder when it is available, since the standard `ffmpeg` has no `avc1` encoder (`default_encoder` in `crates/exporter/src/resolve.rs`) and the optional `ffmpeg-x264` download ranks after bundled plugins. A member of the plugins workspace with Celesta's version. |
| `celesta-codec-mediafoundation` | `plugins/mediafoundation`, the Windows media plugin: a program serving protocol 2 with Media Foundation's encoder MFTs `h264` and `hevc` (hardware first, with a `hardware` setting; RGB converted to BT.709 YUV in the plugin) and `aac` (Microsoft's AAC encoder, with a 1024-frame lead-in as its delay), and an `IMFSourceReader` decoder `mediafoundation/mediafoundation` (`mp4`, `mov`, `m4a`) that applies MP4 and MOV edit lists itself, through the `windows` crate; no muxer, so its encoders write into another plugin's (`ffmpeg/mp4`); `MfCodec`, through which its own tests drive the exporter. MIT or Apache-2.0; `plugin.json`'s `"os": ["windows"]`. FFmpeg ranks before it among bundled plugins, so its muxers, AAC, HEVC, and ProRes serve exports only when named; its H.264, though, is the default MP4 encoder when it is available, since the standard `ffmpeg` has no `avc1` encoder (`default_encoder` in `crates/exporter/src/resolve.rs`) and the optional `ffmpeg-x264` download ranks after bundled plugins. A member of the plugins workspace with Celesta's version. |
| `celesta-codec-test` | The lossless test media plugin the core's tests read and write media with, in-process (`install`) or as the program `celesta-codec-test` in a temporary plugin folder (`TestCodecPlugin`): a decoder, `celesta-test-codec/test`, which lists no extensions and reads test codec and WAV files of any name, a lossless video encoder, a PCM audio encoder, and its own muxer; not published. |
| `celesta-budoux` | Japanese phrase segmentation for text line breaking. |
| `celesta-renderer` | CPU reference renderer, PNG output, and shared text/path rasterization. |
| `celesta-gpu-renderer` | wgpu rendering, compositing, readback, and native preview bridge. |
| `celesta-bench` | GPU export benchmarks driven by `scripts/bench.py`; not published. |
| `celesta-exporter` | Frame-exact export through media components (MP4, MOV, WebM, and whatever muxers plugins add), including React entries: `resolve_output`, the interleaver, `--list-components` and `--describe`, stills, sequences, and contact sheets through the built-in image components (`still_output.rs`, `with_builtin`, `serves_video`), and export presets (`presets.rs`: the user's and the project's `export-presets.json`, `PresetLookup`, `--preset`/`--preset-file`, and the project's `default`; see [`docs/handoff/2026-11-09-export-presets-dialog.md`](docs/handoff/2026-11-09-export-presets-dialog.md)). |
| `celesta-mcp` | `Celesta-mcp`, the MCP server agents use to inspect, render, and export. |
| `celesta-editor-core` | Read-only document, summaries, scene/audio evaluation, and timeline clock. |
| `celesta-editor-theme` | Shared desktop theme. |
| `celesta-editor` | GPUI app: preview, playback, panels, canvas and Properties editing, and export controls, including the export page (`export_page/`: its model over the registry and the presets files, `wording.rs` for the warnings in the page's words, and `view/`: the rows in `view.rs`, the encoder fields in `fields.rs`, the Select lists in `choice.rs`, the row layout in `layout.rs`, and "Save as preset…" in `save.rs`), shown in place of Assets by `export_control.rs`. |
| `celesta-source-edit` | Locates JSX elements in user TSX source and applies byte-span edits for the editor's write-back. |
| `celesta-remote` | Asset path resolution, remote download cache, and stylesheet font discovery. |
| `celesta-react-bridge` | Long-lived Node process and frame/component requests over stdin/stdout. |
| `celesta-plugin` | Content plugins' 2D meshes and 3D scenes, the `ContentProvider` interface, and the plugin protocol's framing and `hello` (`serve_requests`, which media plugins use too, and `protocol_for_roles`: 1 for content, 2 for media), and the memory a media process shares with Celesta (`shared::SharedMemory`: one slot for decoding, or a `Ring` of slots for an export's frames, laid out by `RingLayout`); depends on no Celesta crate (serde, serde_json, thiserror, and libc). See [`docs/handoff/2026-11-01-plugin-system.md`](docs/handoff/2026-11-01-plugin-system.md) and [`docs/handoff/2026-11-03-plugin-scene-content.md`](docs/handoff/2026-11-03-plugin-scene-content.md). |
| `celesta-plugin-host` | Finds plugin directories, decides which may run (the user's settings: trusted projects, allowed plugins), runs each content plugin in its own process for the renderer (`PluginChild`, which media processes use too; its send window keeps several requests in flight, answered in order, each with its own deadline, and `request` is the window of one), and lists the media plugins and their components (`InstalledPlugins::media`, with `MediaOrigin`). |
| `celesta-plugin-flag` | `examples/plugins/flag`, a sample plugin program; with `beep-voice` beside it, built by `scripts/plugins/build.mts`. |
| `celesta-plugin-turntable` | `examples/plugins/turntable`, a sample plugin program that returns a 3D scene; built by `scripts/plugins/build.mts`. |
| `celesta-plugin-vrm` | `plugins/vrm`, the VRM avatar plugin's program, with `<Vrm>` in `plugins/vrm/js` (`celesta-plugin-vrm-js`). A member of the first-party plugins' workspace, `plugins/Cargo.toml`, which the root workspace excludes; built into a plugin directory by `scripts/plugins/build.mts`. See [`docs/handoff/2026-11-03-plugin-vrm.md`](docs/handoff/2026-11-03-plugin-vrm.md). |
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
| `packages/exporter` (`@celesta.studio/exporter`) | Node.js API that runs `celesta-exporter --json` for scripts (`exportVideo` with `preset`, `presetFile`, `muxer`, `videoEncoder`, `audioEncoder`; `exportStills` with `muxer` and `videoEncoder`; `listComponents`, `describeComponent`); not part of the runtime. |
| `packages/web` (`@celesta.studio/web`) | Browser evaluator, Canvas preview, and WebCodecs MP4 export; see [`packages/web/README.md`](packages/web/README.md). |
| `packages/website` (`@celesta.studio/website`) | Official website, English and Japanese user guide, and playground. |
| `scripts/plugins` (`celesta-plugin-scripts`) | The plugin build script, `build.mts`, which makes the plugin directory Celesta loads from a plugin's folder, and its helpers `platform.mts`, `cargo.mts`, `manifest.mts`, and `licenses.mts`; `bump-version.mts`, with its helper `version.mts`, which bumps Celesta's version and the plugins that follow it; `affected.mts`, with its helpers `inputs.mts` and `lockfile.mts`, which lists the first-party plugins a change affects, from `plugins/Cargo.lock` or a plugin's own lockfile, the pnpm graph, and `turbo.json`, for CI's `Plugins (<OS>)` jobs; and `libtest.mts`, which finds the skipped tests in a `cargo test -- --show-output` log for `scripts/report-skipped-tests.mts`. They are TypeScript, which Node runs as it is, stripping the types; not published. |

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
  library; `celesta-codec-avfoundation` and `celesta-codec-mediafoundation`
  use only the operating system's frameworks, and link FFmpeg only in their
  tests), even optionally: media
  goes through `celesta_media::MediaBackend` and the media plugins'
  processes, so the editor, exporter, and MCP server stay MIT/Apache-2.0
  when packages ship a GPL FFmpeg. The core's tests use
  `celesta-codec-test`; only the media plugins' own tests link FFmpeg.
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
- Exports write through media components (`resolve_output`): the output's
  extension picks the muxer, its default codecs the encoders, and
  `--muxer`, `--video-encoder`, `--audio-encoder`, `--video-option`, and
  `--audio-option` override them. `--preset` takes an export preset from
  `--preset-file`, the project root's `export-presets.json`, or the user's
  (beside `settings.json`), and a project's `default` preset applies when no
  preset and no component is named; a stale preset warns and falls back,
  while an explicit value stays strict (`invalid_option`), and a broken
  presets file the lookup has to find a name in is `invalid_presets` (a
  broken user file only warns under a project preset). A named or preset
  muxer that does not list the output's extension writes it anyway, with a
  warning. The editor's Export (the title bar, File → Export…) opens the
  export page, which reads the same files and exports an explicit choice. libx264's `preset` and
  `crf` default to `medium` and 18; `bitDepth=10` encodes yuv420p10le
  (High 10). Celesta offers the encoder `yuv420p`, `yuv420p10`, `nv12`, and
  `p010`, which the GPU converts (`ReadbackFormat::Yuv420p`, `Yuv420p10`,
  `Nv12`, and `P010`), unless the renderer is software, then `rgba16`
  (`ReadbackFormat::Rgba16Straight`, straight-alpha rgba64le) and `rgba8`,
  which the encoder converts. GPU readback returns I420 or NV12 at 1.5
  bytes/pixel, or their 10-bit forms (`yuv420p10`, `p010`) at 3, in BT.709
  limited range. Its 2x2 chroma averaging can differ from the encoder's
  filtering at hard color edges. Every encoder is given `Color` (BT.709
  limited) and every container is tagged from it (H.264 VUI, MP4 and MOV
  `colr`, WebM `Colour`). See
  [`docs/handoff/2026-11-08-media-bt709.md`](docs/handoff/2026-11-08-media-bt709.md).
- Stills (`--frame`, `--frames`, `--every`, `--contact-sheet`) write through
  the built-in image components, named by the output's extension rather
  than ranked: `builtin/image` for one file, `builtin/imageSequence` for
  several, and `builtin/png`, `builtin/jpeg`, or `builtin/webp`. A plugin's
  image muxer or encoder is used only when named (`--muxer`,
  `--video-encoder`), and `--video-option` sets their settings (`--quality`
  is the encoder's `quality`). Stills read no presets file, and a video
  export never uses a built-in or `sequence: true` muxer. With default
  settings the files are byte-identical to those before P4. A sequence is
  staged in a hidden `.celesta-*` folder beside the output and each frame is
  published as soon as the muxer writes it, so a failed or cancelled
  sequence keeps the frames before it; one file is published when the
  muxer finishes. `transparent=true` (PNG, WebP) keeps alpha in straight
  `rgba8`, read back as `Rgba16Straight` and narrowed on the CPU, and
  `depth=16` writes 16-bit PNG from `rgba16`. See
  [`docs/handoff/2026-11-09-media-stills.md`](docs/handoff/2026-11-09-media-stills.md).
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
node scripts/plugins/build.mts plugins/ffmpeg plugins/avfoundation plugins/mediafoundation
export CELESTA_PLUGIN_PATH="$PWD/plugins/dist"   # PowerShell: $env:CELESTA_PLUGIN_PATH = "$PWD\plugins\dist"
cargo run -p celesta-editor -- packages/cli/examples/title.tsx
cargo run -p celesta-exporter -- --react packages/cli/examples/title.tsx output.mov
```

The editor and exporter find the media plugin through `CELESTA_PLUGIN_PATH`,
which must be an absolute path and is set by hand in each shell; rebuild the
plugin after changing `plugins/ffmpeg` or `celesta-media`'s media protocol.
Media plugin programs named `celesta-codec-<name>` (such as
`celesta-codec-ffmpeg`) beside Celesta's executables serve only installed
programs, such as `cargo install`'s; a source build never uses a
media plugin beside an executable in a Cargo build folder such as
`target/<profile>`. Tests whose `celesta-exporter` or `celesta-mcp` child
exports video give it the test codec's program instead, so they need
`celesta-codec-test` built, which `cargo test --workspace` does; otherwise
run `cargo build -p celesta-codec-test` first.

To see how long an export's audio mix takes next to its render, set
`CELESTA_EXPORT_TIMING=1`; the export then prints one line on stderr:

```text
celesta: timing: render 4.50 s for 300 frames; mix 1.25 s, done at frame 87; staged peak 12.0 MB; spilled no; frames ring 3 deep
```

That is the render's and the mix's times, how many frames the render had sent
when it took the mix (`done after the render` when it had sent every frame
first; `mix none` without a mix), the most video packet bytes the interleaver
held in memory at once while they waited for the muxer (in megabytes of 10^6
bytes; the audio packets are not counted), whether it spilled them to a file, and how the video encoder's
process took the frames and how many at a time: through its ring of shared
memory (`ring`), in each request's payload (`pipe`), or one at a time in
Celesta's own process (`in process`).

An existing checkout from before the codec moved to `plugins/ffmpeg` may
still have `celesta-codec-ffmpeg` in `target/<profile>`. Celesta ignores it
there, so video and audio fail with `unsupported_output` or `no_decoder`
and their hint until you build `plugins/dist` (`node scripts/plugins/build.mts
plugins/ffmpeg`) and set `CELESTA_PLUGIN_PATH` to its absolute path, as
above. The old program can be deleted.

A `plugins/dist/ffmpeg` staged before the `media` role (its manifest says
`"roles": ["codec"]`) is ignored too; build it again.

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
the per-package runner with independent native builds:

```sh
node scripts/plugins/run-cargo.mts plugins test -p celesta-codec-ffmpeg -p celesta-codec-ffmpeg-x264 -p celesta-codec-avfoundation -p celesta-codec-mediafoundation -p celesta-plugin-vrm --locked --no-fail-fast
node scripts/plugins/run-cargo.mts plugins clippy -p celesta-codec-ffmpeg -p celesta-codec-ffmpeg-x264 -p celesta-codec-avfoundation -p celesta-codec-mediafoundation -p celesta-plugin-vrm --all-targets --locked -- -D warnings
cargo fmt --manifest-path plugins/Cargo.toml --all -- --check
pnpm --dir scripts/plugins test
```

`PKG_CONFIG_PATH` selects standard; `FFMPEG_X264_PKG_CONFIG_PATH` selects the
optional x264 Unix build (`FFMPEG_X264_VCPKG_ROOT` on Windows). Both FFmpeg
packages are listed once, in `scripts/plugins/ffmpeg.mts`. Cargo features
unify within one invocation, so do not run a single `cargo test --workspace`
against both FFmpeg builds. `--distribution` on the
plugin build script checks licenses and encoder opens on the target platform;
it refuses a cross-compiled distribution it cannot execute locally.
The per-build checks need FFmpeg on `PKG_CONFIG_PATH` (or `VCPKG_ROOT` on Windows).
On a pull request, CI's `Plugins (<OS>)` jobs run these checks with `-p` for
the affected plugins only; a push to `main` and a manual run test every
plugin. The scripts' own tests run in the root `Test` job. To see which
plugins a branch affects, commit and run
`node scripts/plugins/affected.mts --base "$(git merge-base origin/main HEAD)"`.
A plugin's inputs are its folder, its lockfile's local packages, its `js/`
package's workspace dependencies with the `$TURBO_ROOT$/` paths `turbo.json`
gives their tasks, and its `licenses` files, so a module's `js/package.json`
must declare every workspace package its build uses.

For a release, run `node scripts/plugins/bump-version.mts X.Y.Z`, then set
the version in each `packages/*/package.json` and in `templates/package.json`
by hand.

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
native UI checks need a display and suitable platform permissions. The Media
Foundation plugin's tests skip, through one gate (`src/mf/gate.rs`), when
Windows lacks Media Foundation or a component;
`CELESTA_REQUIRE_MEDIA_FOUNDATION=1` makes that a failure, as
`CELESTA_REQUIRE_GPU=1` does a missing adapter. Set it on a Windows machine
known to have them; CI does not set it until Media Foundation on
`windows-latest` is proven. Record actual checks and environment limitations
in the task's dated note rather than carrying historical test counts or
machine-specific results here.

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
