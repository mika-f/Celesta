# Celesta implementation handoff

## Goal

Celesta is a code-first video editor. Projects (`.celesta.json`) and React
compositions are authored as source files and evaluated into the same Rust
composition model; preview and export consume the same renderer inputs.

## Dated notes

Per-task notes live in `docs/handoff/<date>-<task>.md`. Newest first:

- [`2026-10-05-exporter-json-results`](docs/handoff/2026-10-05-exporter-json-results.md): Exporter `--json` results
- [`2026-10-05-subtitle-render-prop`](docs/handoff/2026-10-05-subtitle-render-prop.md): Drawing subtitles with a render prop
- [`2026-10-05-circles-ellipses-arrows`](docs/handoff/2026-10-05-circles-ellipses-arrows.md): Circles, ellipses, and arrows
- [`2026-10-05-performance-check`](docs/handoff/2026-10-05-performance-check.md): Performance check per pull request
- [`2026-10-04-fit-text-to-box`](docs/handoff/2026-10-04-fit-text-to-box.md): Fitting text into a box
- [`2026-10-03-phrase-line-breaking`](docs/handoff/2026-10-03-phrase-line-breaking.md): Phrase line breaking
- [`2026-10-03-gpu-blur-paired-taps`](docs/handoff/2026-10-03-gpu-blur-paired-taps.md): GPU blur pairs its taps
- [`2026-10-02-celesta-math`](docs/handoff/2026-10-02-celesta-math.md): `@celesta/math`
- [`2026-09-25-preview-only-gpui-app`](docs/handoff/2026-09-25-preview-only-gpui-app.md): Preview-only GPUI app
- [`2026-09-25-react-font-local-files`](docs/handoff/2026-09-25-react-font-local-files.md): React `<Font>` loads local font files
- [`2026-09-07-standalone-react-preview`](docs/handoff/2026-09-07-standalone-react-preview.md): Standalone React composition preview in the editor
- [`2026-08-31-react-workflow-helpers`](docs/handoff/2026-08-31-react-workflow-helpers.md): React workflow helpers
- [`2026-08-30-react-xy-top-left`](docs/handoff/2026-08-30-react-xy-top-left.md): `@celesta/react` `x`/`y` place the top-left corner
- [`2026-08-29-portraits-psd-lipsync`](docs/handoff/2026-08-29-portraits-psd-lipsync.md): Character portraits, PSD presets, and automatic lip sync
- [`2026-08-28-rect-demo-composition`](docs/handoff/2026-08-28-rect-demo-composition.md): `Rect` primitive and a Remotion-homepage-style demo composition
- [`2026-08-28-pipelined-gpu-readback`](docs/handoff/2026-08-28-pipelined-gpu-readback.md): Pipelined GPU readback for export
- [`2026-08-28-color-emoji-glyphs`](docs/handoff/2026-08-28-color-emoji-glyphs.md): Color emoji glyph rendering
- [`2026-08-26-sequence-audio-react-preview`](docs/handoff/2026-08-26-sequence-audio-react-preview.md): `<Sequence>`, per-frame audio collection, and editor React preview

Since the preview-only change (2026-09-25), the "Implemented editor behavior"
notes below describe the editing features as they existed before it; treat them
as history. Add new notes as `docs/handoff/<date>-<task>.md` and list them above.

The current milestone is a usable editor foundation for gameplay videos with
VOICEROID-style portraits, dialogue subtitles, voice assets, and ordinary
video/audio tracks.

## Repository state

- Workspace: `/Users/natsuneko/ghq/github.com/mika-f/celesta`
- Rust edition: 2024
- Minimum Rust version: 1.89
- The initial implementation is tracked on `main`; track management landed in
  commit `52b13d5` and MP4 export landed in `dfb7f1a`. Inspect
  `git status --short` for newer work before editing or staging.
- FFmpeg 8.1.x **development libraries** must be available for `ez-ffmpeg` /
  `ffmpeg-sys-next` to link against — the `ffmpeg`/`ffprobe` binaries are no
  longer used at runtime. On Windows: `vcpkg install
  ffmpeg[x264]:x64-windows-static-md` with `VCPKG_ROOT` set (the workspace
  enables `ez-ffmpeg`'s `static` feature). macOS: `brew install ffmpeg` +
  `pkg-config`. Linux: `scripts/build-ffmpeg-linux.sh` builds static FFmpeg
  8.1.x libraries with libx264 into a prefix (Ubuntu 24.04 ships 6.1); point
  `PKG_CONFIG_PATH` at `<prefix>/lib/pkgconfig`; bindgen needs `libclang-dev`.
  The editor also needs `libasound2-dev`, `libxkbcommon-x11-dev`, and
  `libfontconfig-dev`. CI runs Linux on ubuntu-latest with lavapipe
  (`mesa-vulkan-drivers`), so the GPU tests run there too.
  `packaging/linux/Dockerfile` builds a headless exporter image.
- GPUI is pinned to crates.io version `0.2.2`.
- Node.js (>= 18) and pnpm are required for the React composition path
  (`packages/react`, `celesta-react-bridge`). Run `pnpm install && pnpm run
  codegen && pnpm run build` once in `packages/react` before using
  `celesta-exporter --react` or its tests; `celesta-react-bridge` spawns the
  compiled `dist/cli.js`, not the TypeScript sources directly. `pnpm run
  codegen` runs `cargo test -p celesta-project -p celesta-composition --features
  codegen` to (re)generate `src/generated/*.ts`, which `pnpm run build`
  requires as input; both `src/generated/` and `dist/` are gitignored build
  output, not checked in.
- `@celesta/react` is not published to npm: user entries always resolve
  `react` and `@celesta/react` to the bundled runtime. `pnpm run build` also
  stages `dist/project-types/` (`scripts/stage-project-types.mjs`): the
  package's declarations plus `@types/react`/`@types/node` and a base
  tsconfig. File > Set Up TypeScript copies it into a project's `.celesta/`
  (`celesta_react_bridge::set_up_project_types`); File > Set Up TypeScript
  in Folder… installs it into a picked folder as-is, for a project with no
  entry yet (`set_up_project_types_in`). Opening a project
  re-copies it when its `version.json` content hash differs
  (`refresh_project_types`).
- `@celesta/code` stays separate from the React module and its dependencies.
  The React build compiles it after the core declarations and stages its types
  in `project-types`. Desktop packaging includes Code and its tokenizer
  dependencies; the CLI bundles it on import while sharing the core externals.
  File > Set Up TypeScript supplies its import mapping without a project install.

Before editing, run:

```sh
git status --short
cargo test --workspace
```

## Workspace map

| Crate | Responsibility |
| --- | --- |
| `celesta-composition` | Renderer-independent scene types, exact rational time, transforms, animation evaluation, text styles, and audio graph types. |
| `celesta-project` | Version 0 JSON project format, loading, semantic validation, references, and duration calculation. |
| `celesta-evaluator` | Deterministic conversion from `Project` to a visual `Scene` at a time and to the complete `AudioGraph`. |
| `celesta-media` | Metadata probing and exact-time RGBA video-frame decoding via the linked FFmpeg libraries (`ez-ffmpeg`) behind `VideoFrameDecoder` (source overruns freeze on the final frame). |
| `celesta-budoux` | Rust port of google/budoux with its Japanese model: splits text into phrases for `lineBreak: 'phrase'`. |
| `celesta-renderer` | Deterministic CPU reference renderer, PNG output, and the shared text rasterizer. |
| `celesta-gpu-renderer` | `wgpu` renderer for images, video frames, styled text, nested transforms, opacity, offscreen readback, and renderer-owned surfaces. |
| `celesta-bench` | Benchmark workloads for the GPU renderer's export path, driven by `scripts/bench.py` locally (time) and in CI (Cachegrind instruction counts on lavapipe). Not published. |
| `celesta-exporter` | Deterministic frame-exact H.264/AAC MP4 export through the shared evaluator, GPU renderer, audio graph, and the linked FFmpeg libraries (`ez-ffmpeg` `VideoWriter` for encode, `FfmpegContext` for the AAC mux). Also exports React entries via `celesta-react-bridge`. |
| `celesta-editor` | Preview-only GPUI application: File menu open/reload, playback clock, GPU preview bridge, read-only asset list, timeline, and inspector, and MP4 export. |
| `celesta-remote` | Resolves asset locations to local files: relative paths join the asset root, and `http`/`https` URLs download once into a per-user cache (`<cache dir>/remote-v1/<url hash>/<file name>`, never revalidated). Used by both renderers, the audio mixer, the editor's waveforms, and React `preloadMedia()` probes. Also reads `@font-face` stylesheets (`stylesheet_font_faces`), e.g. Google Fonts CSS links; `TextRasterizer::load_fonts` loads each face, unpacks WOFF/WOFF2 with `wuff`, and registers the CSS `font-family` as an extra family name. |
| `celesta-react-bridge` | Spawns one long-lived `@celesta/react` Node.js process per composition and requests the evaluated `Scene` (plus that frame's `<Audio>` clips) for each exact frame time over stdin/stdout JSON, or resolves individual registered components for the editor preview. |
| `packages/math` (`@celesta/math`, TypeScript) | Pure, React-free math for compositions: seeded `random` helpers, `noise`/`noise2D`/`noise3D`/`fbm*`, scalar shaping (`clamp`, `lerp`, `remap`, `smoothstep`, ...), waves, angles, and `{x, y}` point helpers. `@celesta/react` depends on it (`workspace:*`) and builds it through a TypeScript project reference. |
| `packages/react` (`@celesta/react`, Node.js/TypeScript) | Declarative `Composition`/`Sequence`/`Group`/`Image`/`Rect`/`Path`/`Text`/`Video`/`Audio` components rendered through a real `react-reconciler` host (hooks, including `useCurrentFrame`/`useVideoConfig`, work); `useProject`/`<ProjectTimeline />` embed a companion project's Rust-evaluated layers. The `celesta-react-render` CLI bundles a JSX/TSX entry with esbuild and emits `Scene`-shaped JSON plus per-frame audio declarations. |

Important files:

- `crates/editor-core/src/lib.rs`: read-only `EditorDocument`, clip
  summaries, `TimelineClock`, and tests.
- `crates/editor/src/main.rs`: GPUI window, menus, open/reload, playback,
  scrubbing, selection, read-only inspector, preview refresh, and export.
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

- Loads a `.celesta.json` path passed on the command line.
- With no argument, loads `examples/editor-demo.celesta.json`.
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
- Single-line text is cropped vertically to its visible glyphs, so
  `anchorY` 0.5 centers the letters, but keeps its advance width, so leading
  and trailing spaces take up room (issue #21). Multiline text retains its
  layout box. A Text layer with `baselineAnchor` (React `anchorY="baseline"`)
  is anchored vertically on its first line's baseline, which
  `RasterizedText::baseline` reports, so separate runs share a baseline.
- Layers carry a `blendMode` (`BlendMode`: normal, multiply, screen, overlay,
  add, difference; issue #27), set by React's `blendMode` prop or a project
  item's `blendMode`. The formulas live in `BlendMode::blend_channel` and are
  applied to non-premultiplied 8-bit values. A `Group` with a mode other than
  normal is isolated: its children composite onto a transparent layer, and
  its opacity applies to that layer as a whole. The CPU renderer does this with
  a scene-sized `RgbaFrame`. The GPU renderer keeps its single-pass path for
  scenes without blending or effects. A scene with them draws onto a
  scene-sized root canvas (premultiplied alpha). Each isolated group or
  effect draws onto a pooled canvas that covers only its bounds
  (`CanvasRegion`, sized in 128 px steps and kept inside the scene); every
  instance carries the origin of the canvas it draws on. `Compositor`
  composites each group's canvas before its parent's draws, so a canvas is
  drawn in as few passes as possible: a pass loads and stores its whole
  target. Only a blended draw ends a pass, because it copies the area under
  it to a backdrop texture (`fs_blend` in `layer.wgsl`) first. Finally the
  root canvas is copied onto the target.
- `Rect` fills and strokes, flat or gradient, are shaded in `layer.wgsl`
  (`rect_texel`, `paint_color`) and never rasterized into textures, so an
  animated gradient costs no more than a static one. Gradients are passed
  in a `paints` storage buffer next to the clips (`encode_paint`), and the
  shader matches `celesta_renderer::rasterize_rect` within one code value.
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
- The standalone `celesta-exporter` CLI renders every project frame at its exact
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
- Dense geometry (2026-09-28, issue #29): `GpuRenderer` no longer rasterizes
  rects on the CPU. `layer.wgsl`'s `rect_color` shades them from the same
  rounded-box SDF `rasterize_rect` uses, snapped to the texel the rasterized
  texture would have been sampled at, so output matches it (within one code
  value where f32 and f64 round a stroke blend differently). Every layer is
  one instance in a per-frame vertex buffer (`LayerInstance`, reused across
  frames), texture bind groups are created once per texture, and consecutive
  layers sampling the same texture are one instanced draw — all rects share
  a 1x1 placeholder — so the AFTERIMAGE ribbons (3,000+ rects whose size
  changes every frame) cost one draw instead of 3,000 textures, buffers,
  bind groups, and draws. `cargo run --release -p celesta-gpu-renderer
  --example dense-geometry-bench` times that scene; the CLI progress line
  shows fps, elapsed time, and ETA. The same issue's "slows down over a long
  export" was the React host config: `appendChild`/`insertBefore` did not
  remove a child React was *moving* (a keyed list re-sorted between frames),
  so every move left a duplicate and a depth-sorted list grew each frame
  until it unmounted (also over-drawing translucent layers). They now move
  the child like the DOM does (`packages/react/test/reconciler.test.mjs`).
- Group clip (2026-09-29, issue #24): `LayerContent::Group` has an optional
  `clip` (`celesta_composition::Clip`: `x`, `y`, `width`, `height`,
  `cornerRadius`; `x`/`y` is the top-left corner) in the group's own
  coordinates, so it follows the group's transform. React sets it with
  `<Group clip={{ x, y, width, height, cornerRadius }}>` (`x`/`y`/
  `cornerRadius` default to 0). Nested clips intersect, and a clip without
  area hides its children. The CPU renderer keeps a chain of `ClipRegion`s in
  `ParentState` and scales each pixel by its coverage (the same rounded-box
  SDF as `rasterize_rect`; it has no rotation). On the GPU each clipped group
  pushes a `ClipEntry` (inverse group transform, half size, radius, parent)
  into a per-frame storage buffer, bind group 2 (group 1 is the blend
  backdrop, so the plain pipelines leave it unset); a layer's instance carries
  the index of its innermost clip in its eighth vec4 (`clip.x`), and
  `layer.wgsl`'s `clip_coverage` walks the parent chain and scales the alpha.
  Batching and draw counts are unchanged. A clipped group that also has a
  blend mode is isolated as usual: its children carry the clip, the group's
  canvas does not. Nesting deeper than `MAX_CLIP_DEPTH` (8) is
  `GpuRenderError::ClipsNestedTooDeep`. Both renderers turn local distance
  into pixels with `sqrt(|det|)` of the group's transform: exact for a
  uniform scale, approximate for a non-uniform one. The general mask
  (`<Group mask>`) from the issue is not implemented. `examples/celesta-reel.tsx`
  now clips its scrolling timeline instead of covering the label column with
  an opaque rect; it renders the same pixels on a flat background.
- Paths (2026-10-01, issue #31): `LayerContent::Path` holds absolute
  `PathCommand`s (`moveTo`/`lineTo`/`quadTo`/`cubicTo`/`close`, SVG
  semantics) in the layer's own coordinates, an optional `fill` (non-zero)
  and `stroke` (`Paint` + width, drawn over the fill), `lineCap`
  (butt/round/square), `lineJoin` (miter/round/bevel) and `miterLimit`
  (SVG's ratio, default 4). The layer position is the origin and the anchor
  is ignored, like `Group`. `celesta_renderer::rasterize_path` strokes it
  with tiny-skia in local coordinates, transforms the outline to output
  pixels (so scaling stays sharp and a non-uniform scale stretches the
  stroke), and rasterizes anti-aliased coverage masks over the outline's
  bounds cut to the frame; paint is applied per pixel with the same
  `ResolvedPaint::color_at` rects use. Both renderers composite the result
  unscaled at its pixel corner. (Until issue #116, below, the GPU renderer
  uploaded this rasterization too, so a lone path was identical on CPU and
  GPU.) The GPU batched consecutive path
  layers that composite plainly through the same clip (across plain groups)
  into one `PreparedItem::Paths`, rasterized after the frame is prepared by
  `rasterize_paths`: coverage per path in parallel (each over its own
  bounds, so batching never changes a path's coverage — tiny-skia's edge
  clipper does when a path is clipped to bands), then composited in order
  into an f32 premultiplied buffer in parallel bands of rows, with each
  path's cumulative opacity painted in. A batch differs from layer-by-layer
  compositing by at most a code value or two. React: `<Path points closed |
  commands, stroke, strokeWidth, fill, cap, join, miterLimit>` (host type
  `path`); `Line`/`Polyline` are now `Path`s (one layer instead of a `Rect`
  per segment, so a translucent polyline no longer darkens at its joints;
  `Polyline` gained `closed` and `join`). Benchmarks on an Apple M4, 1080p,
  the three 9-strand ribbons of `dense-geometry-bench` (3,029 rect layers,
  or 217 path layers with strands cut into 3 depth bands): GPU-only, rects
  1.4 ms/frame and paths 6.8 ms/frame — the CPU rasterization dominates and
  the GPU-shaded rects of issue #29 remain cheaper per frame (before
  batching and threading paths took 33.7 ms). End to end through React
  (`celesta-exporter --react`, 150 frames, `--preset ultrafast`, scratch
  entries equivalent to the bench), rects exported at 55.7 fps (3.37 s,
  1.0 MB of scene JSON per frame for 3,028 layers) and paths at 91.7 fps
  (1.77 s, 245 KB for 165 layers): reconciling and serializing thousands of
  layers costs more than the rasterization saves. A depth-banded path ribbon
  quantizes opacity/width to its bands where the rects vary per segment.
  `examples/afterimage/film.tsx` still uses its rect `Line`.
- GPU path shading (2026-10-03, issue #116; review fixes 2026-10-04): ordinary
  paths use GPU coverage; outlines with over 64 edges in a tile or paths
  exceeding the remaining frame storage budget use CPU raster textures.
  `celesta_renderer::flatten_path`
  builds the same outlines `rasterize_path` fills (same tiny-skia stroker and
  resolution scale, transformed to output pixels, same region cut to the
  frame) and flattens them into `LineSegment`s relative to the region:
  curves to 0.05 px, edges cut to its rows, what lies left of it moved onto
  `x = 0`, what lies right dropped, horizontal edges kept and every cut
  landing exactly on its row or side so the edges meet end to end.
  `ShadedPath`/`bin_tiles` use Vello's backdrop scheme with 8x8-pixel tiles
  (Vello's GPU tiles are 16x16): a
  tile lists the edge parts at or right of its left side, plus a vertical
  edge down that side wherever an edge crosses it, and a backdrop (the
  winding just left of its top-left corner). Only tiles with edges or a
  nonzero backdrop are stored (in a per-frame storage buffer, binding 2 of
  the clip bind group, rebuilt every frame and grown to the largest frame
  like the instance buffer, no caches) and drawn: `vs_main` places one quad
  per listed tile (`GpuDraw::vertices` is `0..6 * tiles` for a path; one-tile
  paths can correctly merge with an adjacent rect draw) and passes the tile's
  entry to the fragment. Buffer indices are capped at 2^24 entries for exact
  f32 representation. Clipping tests cover tiny boundary pieces whose
  interpolation parameters round to an endpoint.
  `layer.wgsl`'s `path_texel` measures nonzero coverage on four scanlines per
  pixel row (tiny-skia's sampling) with exact horizontal spans, paints stroke
  over fill with `paint_color` at the pixel centre mapped back to layer
  coordinates, and leaves the layer opacity, clip and blend mode to the
  ordinary layer path, so each path composites as one layer. tiny-skia
  rounds crossings to quarter pixels and flattens curves more coarsely, so
  edge pixels can differ by a few 1/16 samples: the GPU tests pin max 64 /
  mean 0.5 / at most 1% of channels over 16 against the CPU renderer (or
  its rasterizer, for rotations), and the GPU is the closer of the two to
  an 8x supersampled rendering. Historical Apple M4 measurements at
  `9a91b34` (before the final shader), 1080p: the 24 NEBULA rings went
  from 6.46 to 1.94 ms/frame (medians of submit/drain with readback;
  stroking and flattening take 0.27 ms of it), the 217-layer path ribbons of
  `dense-geometry-bench` from 8.11 to 2.84 ms/frame. See `docs/performance/gpu-path-coverage.md`.
- Export speed (2026-09-25): `GpuRenderer` caches layer textures across
  frames (images, PSD composites, text; keyed by their inputs, text
  also by `TextRasterizer::loaded_font_count`), so unchanged layers are
  neither re-rasterized nor re-uploaded; entries a frame does not use are
  dropped. `submit`'s reclaim waits on the oldest slot's own
  `SubmissionIndex`, not the latest submission. Decoded video frames share
  their pixels (`VideoFrame.pixels: Arc<Vec<u8>>`) and the exporter hands
  readback buffers to the encoder with `write_owned`, which removes two 8MB
  copies per 1080p frame. Project exports mix audio on a scoped thread while
  frames render. `ExportOptions.video` (CLI `--preset`/`--crf`) sets the
  libx264 preset and CRF (default `medium`/18, the previous fixed values). On
  a CPU-only lavapipe box, a 1080p30 10s project with 15 static text layers
  went from 21.6s to 5.0s, and the video + text project from about 10.8s to
  9.2s. Frame and audio hashes match the previous output.
- GPU yuv420p readback (2026-09-25): `GpuRenderer::set_readback_format(
  ReadbackFormat::Yuv420p)` makes `submit`/`drain` return packed I420 frames.
  `yuv420p.wgsl` renders the slot's RGBA texture into R8 Y/U/V plane textures
  (one luma pass, one two-target chroma pass), and the planes are copied into
  one readback buffer. That is 1.5 instead of 4 bytes per pixel, and the
  `VideoWriter` takes `yuv420p` input without a libswscale pass. The matrix is
  BT.601 limited range, like swscale's for untagged RGB input, and the stream
  tags are unchanged. Chroma is a 2x2 average where swscale filters
  bicubically. In a lossless 1080p comparison, luma is within 1 code value and
  96-99.6% of chroma samples are within 1; the largest differences (up to 25)
  are at hard color edges. `VideoEncoding.color_conversion` (CLI
  `--color-conversion auto|gpu|encoder`) picks the side; `auto` uses the GPU
  unless `GpuRenderer::is_software()`. On lavapipe the two passes cost about
  13 ms per 1080p frame, slower than swscale, while the smaller readback alone
  saved about 1.3 ms. Hardware GPU numbers are not measured yet.
- The editor toolbar and Command-Shift-E open a native MP4 destination prompt,
  snapshot the current `Project`, and invoke `celesta-exporter` on a dedicated
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
  cache (`~/Library/Caches/com.natsuneko.celesta/audio-v1` on macOS). Keys include
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

- `packages/react` is a TypeScript package managed with pnpm. `@celesta/react`
  exports `Composition`, `Group`, `Image`, and `Text` components (typed props
  in `src/components.ts`; `src/scene.ts` re-exports the `Scene`/`Layer`/...
  types from `src/generated/`, ts-rs bindings generated from
  `celesta_composition`, rather than hand-mirroring the JSON shape).
- The entry's default export must render a single root `<Composition width
  height fps durationInFrames>` element. Layer ids default to a
  path-based string (for example `root.0.1`) stable across repeated renders of
  the same tree shape, or an explicit `id` prop.
- `pnpm run build` compiles `src/*.ts` to `dist/*.js` (plain CommonJS, plus
  `.d.ts`) with `tsc`; `celesta-react-bridge` and `celesta-exporter` spawn
  `dist/cli.js`, not the TypeScript sources. `dist/` is gitignored like
  `node_modules/` and `.tmp/`, so it must be rebuilt after checkout.
- `celesta-react-render` (`packages/react/src/cli.ts`, compiled to
  `dist/cli.js`) is the Node.js CLI: it bundles the given entry with esbuild
  (`jsx: automatic`, entry's own `@celesta/react` import kept external so the
  same component-marker objects are compared, not a bundled duplicate),
  writes the bundle beside the package under `.tmp/` (self-reference
  resolution needs the bundle to live inside the package directory tree),
  prints one `{"config": ..., "componentSchemas": ...}` startup line, then
  answers one request per line — `{"time": ...}` frame requests with
  `{"scene": ..., "audio": [...]}` and component-resolution requests with
  `{"components": [...]}` — or `{"error": ...}`. esbuild transpiles the user's entry
  file directly (TS or TSX) without type-checking it; `@celesta/react`'s own
  source is type-checked by `pnpm run build`.
- `celesta-react-bridge` spawns and owns this Node process for the lifetime of
  an export or preview, mirroring `celesta-media`'s one-process-per-composition
  sequential decoding session rather than spawning Node per frame.
- An entry can additionally export an async `prepare()`. `cli.ts`'s `main()`
  awaits it exactly once, before mounting the composition and before the
  first frame request — the one point in the pipeline where async work (e.g.
  fetching remote data) is allowed, since `renderAt()` itself and the Rust
  side's request/response loop are both fully synchronous. Data fetched in
  `prepare()` should be stashed in module-level state and read synchronously
  by the rendered components, so it is fetched once per export/preview
  session rather than once per frame (see `examples/homepage-demo.tsx`).
- `celesta-exporter --react <entry> <output.mp4>` renders every frame of the
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
  `react/jsx-runtime`/`react/jsx-dev-runtime`, not just `@celesta/react`.**
  Without this, the entry's bundle gets its own copy of React with its own
  internal dispatcher slot, separate from the one this process's
  `react-reconciler` actually sets — hooks then fail at runtime with React's
  "Invalid hook call" warning (reproduced and fixed during this work). All
  of `react`, its jsx-runtime, and `@celesta/react` need to resolve to the
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
    bridge protocol (`celesta-react-bridge`) is a synchronous one-request-per-
    line pipe where Rust always initiates and blocks on Node's response. If
    `<ProjectTimeline />` tried to ask Rust to evaluate while rendering,
    Rust would already be blocked waiting for *this* response and could
    never service that nested request — deadlock. Instead,
    `ReactBridge::scene_at_with_project(time, Option<&[Layer]>)`
    (`celesta-react-bridge`) embeds the already-evaluated layers in the
    request itself: `{"time": ..., "project": {"layers": [...]}}`
    (`project` omitted entirely when there is no companion project, via
    `skip_serializing_if`).
  - **`celesta-exporter`**: `Exporter::export_react_entry_with_project[
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
    `absolutize_fonts`/`absolutize_asset` (`celesta-exporter`) rewrite the
    project-evaluated `Layer`s' (and `Scene.fonts`') relative `File` paths
    into absolute ones (joined against `project_asset_root`) before they are
    sent to Node — `local_asset_path` already left absolute paths alone, so
    this needed no `GpuRenderer` changes. Project fonts are evaluated once
    (not per frame, since they do not vary by time) and merged into every
    frame's `Scene.fonts` after Node responds.
  - **CLI**: `celesta-exporter --react <entry> --project <project.celesta.json>
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
  `celesta_composition`, an unrelated project.json-facing concept, already
   used that name) provides `linear`/`easeIn`/`easeOut`/`easeInOut` plus the
   usual sine/quad/cubic/quart/quint/expo/circ/back/elastic/bounce families
   (expanded 2026-08-26 alongside the `<Sequence>` work).
  `celesta_composition::Easing` (the project.json-facing enum a `Keyframe`'s
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
  (position) rendered through `celesta-exporter --react` shows the text
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
  `celesta-evaluator` — Rust has no registry, so it always evaluates that
  content to `LayerContent::MissingComponent { component, props }` (see
  `crates/evaluator/src/lib.rs`), and `visual_only_project()`
  (`celesta-exporter`) now keeps `component` items through its filter
  (previously dropped, alongside `dialogue`, in the earlier v1 pass — see
  "`dialogue` timeline content" below for when that changed too).
  `registerComponent(name, Component)`
  (called at module scope in the entry, so registration happens before any
  frame renders — the registry is a plain process-global `Map`, safe
  because one `celesta-react-render` process only ever handles one entry) is
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
    `celesta-exporter --react --project`, and a project with an unregistered
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
  before. `getComponentSchema(name)` (also exported from `@celesta/react`,
  alongside the `ComponentPropertyField`/`ComponentPropertySchema` types)
  looks it up, returning `undefined` for both an unregistered name and a
  registered one with no schema — callers that need to tell those apart
  check `resolveComponent(name)` (internal, not exported) first.
  `listComponentSchemas()` collects every registered component's schema,
  keyed by name (skipping schema-less registrations); `cli.ts` sends this
  once, alongside `config`, in the startup `Ready` message — all
  `registerComponent()` calls have already run by module-scope time, so
  nothing is missing.
  - **GPUI editor integration.** `celesta-project`'s `ProjectSettings` gained
    an optional `react_entry: Option<String>` field (relative to the
    project file, like an asset path) — project.json's only pointer to
    which `.tsx` entry a `TimelineContent::Component` item's `component`
    name resolves against; nothing in `celesta-project`/`celesta-evaluator`
    reads it, it exists purely so the editor knows which Node process to
    query. `celesta-react-bridge` gained `ComponentPropertyField`/
    `ComponentPropertySchema` Rust types (a real enum mirroring the
    TypeScript shape field-for-field, `#[serde(tag = "type", rename_all =
    "camelCase", rename_all_fields = "camelCase")]` — the same
    `rename_all_fields` gotcha noted above applies here too) and
    `ReactCompositionMetadata::component_schemas: BTreeMap<String,
    ComponentPropertySchema>`, populated by parsing the `Ready` message's
    new `componentSchemas` field during `ReactBridge::spawn`'s handshake —
    no new request/response round trip needed, since this rides the
    existing startup message.
  - `celesta-editor` (previously fully Node-independent — confirmed by
    grepping for zero `Command::new`/`ReactBridge` references before this
    change) now depends on `celesta-react-bridge`. `EditorDocument` gained
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
    `celesta-exporter --react` resolves — purely to read
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
  - Verified: new `celesta-editor` unit tests
    (`react_entry_path_is_stored_relative_and_resolves_back_to_absolute`,
    `react_entry_and_component_props_are_undoable`) cover the document
    layer end to end including undo; a new `celesta-react-bridge` integration
    test (`reports_a_registered_components_property_schema_when_node_is_available`)
    spawns the real Node runtime against
    `packages/react/examples/with-registered-component.tsx` (which now
    declares a `bossIntroductionSchema`) and asserts
    `bridge.metadata().component_schemas` carries the exact declared
    fields; two new `celesta-react-bridge` unit tests cover `Ready`-message
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
    `@celesta/react`) stores a process-global schema — call at module scope,
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
  - Verified: two new `celesta-react-bridge` unit tests (`Ready` parsing with
    all five field types present / absent-or-null defaulting to None); a new
    integration test (`reports_a_declared_project_property_schema_when_node_
    is_available`, against a new `packages/react/examples/with-properties.tsx`
    declaring all five field types over an embedded `loadProjectFromString()`
    project) asserting the metadata round-trips exactly, that an entry
    without `defineProjectProperties()` yields `None`, and that the entry's
    own `useProjectProperty()` reads its embedded project's value; a new
    `celesta-editor` document test covering set/remove/undo semantics. The
    interactive Inspector click-through remains untested for the same
    environment reason noted above; UI wiring is covered by clippy on the
    real `main.rs`.
- **Per-track access** (`useProjectTrack()`, `<ProjectTrack id="..." />`,
  `src/project-runtime.ts`). `<ProjectTimeline />` embeds every track's
  layers flattened together, which is fine for the whole-composition case
  but gives an entry no way to read or re-embed just one track — needed new
  evaluator-side surface, not just a new React component, since
  `celesta-evaluator::Evaluator::scene_at` only ever evaluated all tracks
  together.
  - `Evaluator::layers_for_track(track_id, time)`
    (`crates/evaluator/src/lib.rs`) evaluates one track's active visual
    layers at an exact time, independent of the others; both it and
    `scene_at` now share an `active_track_layers(track, time)` helper. An
    unknown `track_id`, or a track disabled at the project level, evaluates
    to an empty `Vec` rather than an error — from the caller's perspective
    both are just "nothing to show."
  - `celesta-exporter`'s `render_react_video` per-frame loop now evaluates
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
    other — confirmed via `celesta-exporter --react --project` producing an MP4
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
  Verified: a new `celesta-exporter` unit test
  (`visual_only_project_keeps_dialogue_and_component_but_drops_audio`)
  constructs a project with one `dialogue` item (with `audio` set) and one
  bare `audio` item, asserting the filter keeps only the former and that
  evaluating it produces the expected portrait+subtitle `Group`. End to end,
  `celesta-exporter --react packages/react/examples/with-project.tsx
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
  the same formula `celesta-evaluator::visual_layer` uses for a project
  `TimelineContent::Video` at a constant (non-animated) playback rate.
  `secondsToTime`/`secondsFromTime` (new small helpers) convert between a
  plain seconds number and the generated `Time { value, timescale }` shape,
  using a microsecond timescale so trimming isn't visibly quantized.
  `celesta-exporter`'s `render_react_video` now attaches a video decoder
  (`FfmpegBackend::with_sequential_video`) unconditionally rather than only
  when a companion project is present — previously a React-only export
  (no `--project`) had no decoder at all, so any `LayerContent::Video`
  layer would have hit `GpuRenderError::MissingVideoDecoder`.
  Verified: a new `celesta-react-bridge` integration test
  (`computes_video_timing_from_the_composition_clock_when_node_is_available`,
  against a new `packages/react/examples/with-video.tsx` declaring
  `startFrom={1} playbackRate={2}`) asserts `sourceTimeSeconds` at frame
  15/30 (0.5s in) comes out to `1 + 0.5 * 2 = 2.0`. End to end, a real
  `celesta-exporter --react` export against a synthetic `ffmpeg testsrc`
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
    `LayerContent` variant (`celesta_composition::Scene`/`LayerContent` stay
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
  - **`celesta-exporter`'s audio graph construction**
    (`crates/exporter/src/lib.rs`'s new `build_audio_graph`). Builds one
    `celesta_composition::AudioGraph` per React export: React-declared clips
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
  - Verified: two new `celesta-react-bridge` unit tests
    (`deserializes_audio_clips_from_the_ready_message`,
    `ready_message_without_audio_clips_defaults_to_empty`) cover the
    `Ready`-message parsing; a new integration test
    (`crates/react-bridge/tests/node_integration.rs`,
    `collects_audio_clips_from_the_ready_message_when_node_is_available`,
    against a new `packages/react/examples/with-audio.tsx` declaring
    `startFrom={1} playbackRate={2} volume={0.5} muted={false}`) asserts the
    collected `audio_clips` metadata exactly, and that the `<Audio>` element
    contributes no layer to the rendered scene (only the entry's sibling
    `<Text>` layer appears). End to end, a real `celesta-exporter --react`
    export of an entry with `<Audio src="<absolute path to
    examples/assets/voices/001.wav>" />` (default `startFrom`/`playbackRate`/
    `volume`/`muted`) against the real Node/FFmpeg toolchain produced an MP4
    with both an `h264` video stream and an `aac` audio stream at 48 kHz
    stereo (confirmed with `ffprobe`), while re-exporting the pre-existing
    `packages/react/examples/title.tsx` (no `<Audio>`, no companion project)
    still produced a video-only MP4 with no `mixing audio`/`muxing MP4`
    progress stages, confirming the silent path is unregressed.

### TypeScript type generation and the Project loader

`celesta-composition`'s and `celesta-project`'s public serde types carry
`#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]` plus `#[cfg_attr(feature
= "codegen", ts(export))]`, gated behind a `codegen` cargo feature so `ts-rs`
is not a dependency of default builds (editor, exporter, etc. build exactly
as before). `pnpm run codegen` in `packages/react` (or directly, `cargo test
-p celesta-project -p celesta-composition --features codegen`) runs the tests
`ts-rs`'s derive macro generates, one per exported type, each of which writes
a `.ts` file. `.cargo/config.toml` sets `TS_RS_EXPORT_DIR` so those land in
`packages/react/src/generated/` (workspace-relative, harmless when `codegen`
is off) and `TS_RS_LARGE_INT = "number"` so `Time.value` (`i64`) binds to
`number`, not `bigint` — `serde_json` has no bigint literal, so a `bigint`
binding would not round-trip through the stdin/stdout JSON protocol
`celesta-react-bridge` uses. `serde-json-impl` is enabled so `PropertyValue`
(`serde_json::Value`) binds to a `JsonValue` type under
`generated/serde_json/`. `src/generated/` is gitignored, like `dist/`, and
regenerated from Rust rather than committed.

`packages/react/src/scene.ts` now re-exports the render-output side (`Scene`,
`Layer`, `LayerContent`, `Time`, `Rational`, ...) from `src/generated/`
instead of hand-mirroring it, and `src/project.ts` adds a
`loadProject(path)` / `loadProjectFromString(json)` reader for `.celesta.json`
files, typed against the generated `Project`. This loader is read-only and
intentionally thin (`JSON.parse` plus a type assertion): the canonical
validation is `celesta-project`'s `Project::load`/`Project::from_json`, and a
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
keeps renaming the tag values. Re-verified end to end: `celesta-exporter
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

- `examples/minimal.celesta.json`: smallest valid empty project.
- `examples/editor-demo.celesta.json`: self-contained 10-second title clip used
  by the default editor launch.
- `examples/voiceroid.celesta.json`: dialogue from 5 to 8 seconds, portrait,
  subtitle styling, and a voice asset reference.
- `examples/assets/akane/default.ppm`: tiny built-in portrait placeholder so
  the visual VOICEROID path works without downloads.

`examples/assets/voices/001.wav` is a 48 kHz stereo PCM spoken fixture generated
from the macOS Kyoko system voice. It is an executable example asset, not a
licensed VOICEROID voice sample.

- `packages/react/examples/title.tsx`: a five-second, single-`Text` React
  composition used by `celesta-react-bridge`'s integration test and as the
  `celesta-exporter --react` example.
- `packages/react/examples/with-project.tsx`: `<ProjectTimeline />` alongside
  a React-authored `<Text>`, used by `celesta-react-bridge`'s
  project-companion integration test and the `celesta-exporter --react
  --project` example.
- `packages/react/examples/with-registered-component.tsx`: registers a
  `BossIntroduction` component (with a `ComponentPropertySchema` declaring
  its `bossName`/`level` props) and renders `<ProjectTimeline />`, used by
  `celesta-react-bridge`'s component-registry integration test.
- `packages/react/examples/with-project-track.tsx`: reads one track via
  `useProjectTrack('titles')` and renders another whole via `<ProjectTrack
  id="overlays" />`, used by `celesta-react-bridge`'s per-track integration
  test.
- `packages/react/examples/with-video.tsx`: a `<Video src="./clip.mp4"
  startFrom={1} playbackRate={2} />`, used by `celesta-react-bridge`'s video
  timing integration test (no actual `clip.mp4` needed there — Node
  evaluates the layer tree without decoding).
- `packages/react/examples/with-audio.tsx`: a `<Text>` alongside `<Audio
  src="./voice.wav" startFrom={1} playbackRate={2} volume={0.5}
  muted={false} />`, used by `celesta-react-bridge`'s per-frame audio
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
  rounded `<Rect>`, used by `celesta-react-bridge`'s rect-evaluation
  integration test.
- `packages/react/examples/homepage-demo.tsx`: the Remotion-homepage-style
  four-card demo composition described above; not tied to a Rust test, run
  manually via `celesta-exporter --react`.
- `packages/react/examples/with-media-info.tsx`: calls `preloadMedia()` from
  `prepare()`, derives the composition duration with
  `mediaDurationInFrames()`, and displays the probed audio sample rate.
- `packages/react/examples/with-debug.tsx`: registers a `DebugCard` component
  using `DebugOverlay` and `DebugBounds`; the guides appear in editor
  component resolution but are absent from normal composition evaluation.

## Validation baseline

At this handoff, the Rust workspace has 150 passing tests and
`packages/react` has 15 passing Node tests. This slice adds live bridge tests
for media metadata preload and preview-only debug guides. The earlier baseline
had 126 workspace tests (125 from the previous handoff, plus a
`celesta-renderer` color-emoji rasterization test). Before that, 125 (124 from
the handoff before that, plus a `celesta-gpu-renderer`
test for `submit`/`drain` pipelining order/correctness). Before that, 124
(122 from the handoff before that, plus a `celesta-renderer` `Rect`
rasterization test and a `celesta-react-bridge` `<Rect>` evaluation
integration test). Before that, 122 (110 from the
handoff before that, a live-FFmpeg `celesta-media` test freezing
on the last frame past a source's end, three dialogue-authoring editor
tests, and two character-creation editor tests, two character-name tests,
two safe character-deletion tests, and two character-expression tests). The
media test skips itself when ffmpeg/ffprobe are missing, or locates them via
`CELESTA_FFMPEG_DIR`. The last checks were:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check
cargo test -p celesta-project -p celesta-composition --features codegen
cargo clippy -p celesta-composition -p celesta-project --all-targets --features codegen -- -D warnings
cd packages/react && pnpm run codegen && pnpm run build && pnpm test
```

The `codegen`-feature checks add no new test count (the ts-rs-generated
`export_bindings_*` tests just write files as a side effect) but are worth
running whenever composition/project types change, since that is what
regenerates `packages/react/src/generated/`.

`celesta-react-bridge`'s integration test spawns the real `celesta-react-render`
CLI and skips itself with a message if `node` is not on `PATH` or if
`packages/react/node_modules` or `packages/react/dist` do not exist yet (run
`pnpm install && pnpm run codegen && pnpm run build` there first).
`celesta-media`'s FFmpeg integration tests use the same skip-if-missing pattern
for `ffmpeg`/`ffprobe`.

The editor and VOICEROID example were also launched successfully:

```sh
cargo run -p celesta-editor
cargo run -p celesta-editor -- examples/voiceroid.celesta.json
```

A full React-entry export was also run end to end and its output frame was
inspected: `cargo run -p celesta-exporter -- --react
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
build` in `packages/react` and the full `celesta-react-bridge` integration
suite (real Node, no skip) were run for real; `libasound2-dev`, `ffmpeg`, and
`mesa-vulkan-drivers`/`libegl1` (software Vulkan, for `celesta-gpu-renderer`'s
wgpu backend) were installed via `apt-get` to unblock `celesta-editor`'s build
and a real `celesta-exporter --react` run respectively — neither is a repo
change, just sandbox setup, and is not guaranteed present in a future
session. With that in place, a real `celesta-exporter --react` export of an
entry declaring `<Audio src="<absolute path to
examples/assets/voices/001.wav>" />` alongside a `<Text>` was run end to end
(see "`<Audio>` component and React export audio mixdown" above for the
`ffprobe`-confirmed result), and re-exporting `packages/react/examples/
title.tsx` (no audio) was re-verified to still skip the mux stage.

There are future-incompatibility warnings in transitive dependencies
`block 0.1.6` and `proc-macro-error2 2.0.1`; these are not current Celesta lint or
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
slice (`packages/react`, `celesta-react-bridge`, `celesta-exporter --react`) is
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
- ~~An `AudioGraph` source for React entries so `celesta-exporter --react` can
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
  end no longer fails the frame either (2026-08-26): `celesta-media` clamps
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
