# Mikan implementation handoff

Last updated: 2026-08-25

## Goal

Mikan is a code-first video editor. The GPUI editor owns and edits project
data. Projects and future React compositions are evaluated into the same Rust
composition model, and preview/export should consume the same renderer inputs.

The current milestone is a usable editor foundation for gameplay videos with
VOICEROID-style portraits, dialogue subtitles, voice assets, and ordinary
video/audio tracks.

## Repository state

- Workspace: `/Users/natsuneko/ghq/github.com/mika-f/mikan`
- Rust edition: 2024
- Minimum Rust version: 1.89
- No commit has been created yet.
- The repository currently has no tracked files; `git status --short` reports
  the whole implementation as untracked. Do not assume there is a clean Git
  baseline or discard any of these files.
- FFmpeg and FFprobe are installed and available on `PATH`.
- GPUI is pinned to crates.io version `0.2.2`.

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
| `mikan-editor` | GPUI application, editor-owned document state, playback clock, GPU preview bridge, asset panel, timeline, and inspector. |

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
- GPU preview rendering/readback and FFmpeg audio decoding/mixing run on
  dedicated workers rather than the GPUI thread.
- Preview and audio requests carry monotonically increasing generations.
  Queued work is coalesced to the latest request, stale results are discarded,
  and superseded audio mixing stops at cancellation checkpoints.
- Audio and dialogue clips display downsampled peak waveforms aligned to their
  project ranges after the background mix completes.
- Audio-capable track headers expose persistent Mute and Solo controls. Mute
  affects audio without hiding visual layers; when any audible track is soloed,
  non-solo audio tracks are excluded from the shared `AudioGraph`.
- The toolbar exposes a persistent master-volume control from 0% to 200% in 5%
  steps. It is evaluated into `AudioGraph` and applied at final mixdown.
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
- Selected clips can be deleted from the Timeline header or with Backspace /
  Forward Delete. Insert/delete operations are undoable, derived timelines
  recalculate duration, and locked tracks reject both deletion and drag edits.
- A dedicated FFprobe worker caches duration, video dimensions, and audio-stream
  presence by asset ID for the editor session. Assets show this metadata or a
  probe-failure state without persisting probed facts in v0 JSON.
- The audio worker caches decoded PCM by path, project sample rate, and channel
  count. Subsequent edits remixes cached samples instead of invoking FFmpeg for
  every audio asset again.
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

## Preview integration decision

GPUI owns its macOS Metal presentation surface. Creating and presenting a
second `wgpu::Surface` for the same GPUI window would introduce conflicting
surface ownership. The editor therefore currently:

1. evaluates the project into a `Scene`;
2. renders an offscreen `GpuFrame` with `GpuRenderer`;
3. converts RGBA bytes to GPUI's expected BGRA layout;
4. displays an `Arc<RenderImage>` with GPUI.

This performs a GPU-to-CPU readback for each preview frame, but rendering,
readback, and RGBA-to-BGRA conversion now happen on the preview worker. Preserve
this bridge boundary until a safe native Metal texture/CVPixelBuffer integration
is designed. `GpuRenderer::render_to_surface` remains useful for a separate,
renderer-owned preview window.

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

## Validation baseline

At this handoff, the workspace has 63 passing tests. The last checks were:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check
```

The editor and VOICEROID example were also launched successfully:

```sh
cargo run -p mikan-editor
cargo run -p mikan-editor -- examples/voiceroid.mikan.json
```

There are future-incompatibility warnings in transitive dependencies
`block 0.1.6` and `proc-macro-error2 2.0.1`; these are not current Mikan lint or
test failures.

The frame-based mutation model and its boundaries are unit-tested. Native
window startup is verified, but automated pointer-drag testing is not yet
available because the current environment lacks macOS Accessibility permission
for synthetic GUI input. Keep drag behavior easy to exercise manually.

## Recommended next work

The initial editor mutation, persistence, audio, and background-worker
milestones are complete. Continue with:

1. Consider an on-disk PCM/waveform cache keyed by file identity, mtime, sample
   rate, and channel count so decoding can be reused across editor sessions.
2. Add per-track level meters and replace the stepped master-volume buttons
   with a draggable, keyboard-accessible control when GPUI input primitives are
   introduced.
3. Add timeline track creation/reordering controls and drag clips between
   compatible tracks.

Later performance work should replace GPUI image readback with a native texture
bridge. Do not optimize this by letting GPUI and wgpu both present to the same
window surface.

## Working conventions

- Preserve unrelated or pre-existing worktree changes.
- Use targeted validation while iterating, then the full baseline above.
- Do not commit unless the user explicitly asks.
- If a commit is requested, stage only accepted files.
