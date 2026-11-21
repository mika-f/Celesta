# Scene error boundaries

## Specification

A scene that throws while it renders no longer breaks the whole preview.
`<Scenes>`, `<Series>`, and `<TransitionSeries>` wrap each scene in a
`<SceneBoundary>`, a React error boundary, and a composition can wrap any
other part (a `<Sequence>` of its own) in one too.

- In the editor preview the scene draws an error card in its place: a dark
  card with a red bar, `Scene failed to render`, the scene's name, `id: …`,
  the error's message (cut at 400 characters), and a line saying that
  Problems has the stack and that exports fail. The other scenes keep
  playing. Problems lists the error (`Scene "stats" (Stats) failed to
  render`, an Error of the Preview source) with its stack, which points at
  the entry's own source files, and the components between the scene and
  the throw. The status bar counts it.
- Everywhere else (video and still exports, contact sheets, `inspect.mjs`
  in both modes, the MCP tools, component resolution) the frame fails with
  `scene "stats" (Stats) threw while rendering: <message>`. No output ever
  contains the card.
- The card's label is a `defineScenes` entry's `name`, a
  `<Series.Sequence name>` or `<TransitionSeries.Sequence name>` (new
  props), else their `id`, else `Series item N` / `Scene N`. The reported
  ids are the entry id, the item's `id`, or `series-N` / `scene-N`.
- Only throws while React renders are caught. An error outside every scene
  (a HUD beside `<Scenes>`) and an invalid prop value the walker finds after
  React committed still fail the frame, in the preview too.
- The preview's audio sweep treats a throwing scene the same way: silent on
  its frames, the rest of the film audible.

Which behavior applies is the preview flag (`useIsPreview()`, the
`preview` field of frame requests), which only the editor's preview bridge
sets. It is carried to the boundaries as `catches` on the per-pass sink
(below) rather than read from the runtime context, so the passes that
mount the entry can catch too (see "Mounting").

## How a render-time throw behaved before

Measured on `main` (67450afd) with `packages/cli/examples/with-scene-error.tsx`,
whose middle scene reads `STATS[3].label` from its 45th frame on (composition
frame 105):

| Path | Before |
| --- | --- |
| CLI frame request (any mode) | `{"error":"Cannot read properties of undefined (reading 'label')"}`: no scene, no stack. The root unmounted (uncaught error) and remounted on the next request, so the runtime kept answering. |
| Editor `.tsx` preview | `render_whole_react_scene` took the `Render` error as a dead bridge: it dropped the bridge and remembered the failure, so every later frame, including good ones, showed `Preview unavailable` until a reload. The last good frame stayed on screen. |
| Editor audio sweep | `collectAudio` failed on the first broken frame: no audio for the whole preview. |
| `celesta-exporter --frame 120` | Exit 1, `frame 120: could not evaluate React export: could not render the composition: Cannot read properties of undefined (reading 'label')`; `--json` code `react`. |
| Contact sheet (`--every 30`) | Exit 1, the frame as a failed tile, `1 of 8 contact sheet frames failed`. |
| Video | Same render path as stills: fails on the first broken frame. |
| `inspect.mjs` (`--native` and `--no-native`) | `ERROR: Cannot read properties of undefined (reading 'label')` for the frame, exit 1. |
| MCP `inspect` / `render_frames` | `isError`, the frame's `error` / code `react` with the same message. |

## Implementation

- `packages/react/src/scene-boundary.ts`: `SceneBoundary` (public), the
  class `Boundary` behind it, `SceneRenderError`, `SceneErrorReport`, and the
  internal `SceneErrorSinkContext`. The runtime is `react-reconciler`, so a
  class component with `getDerivedStateFromError` / `componentDidCatch` is an
  ordinary error boundary; nothing in the host config was needed for it.
- Frames stay pure. `render.ts` creates a new sink object for every render
  pass (`renderTree`) and provides it above the entry. The boundary keeps
  the sink it caught in, and `getDerivedStateFromProps` drops the caught
  error when a new sink arrives, so every pass renders the children again,
  in whatever order frames come. `SceneBoundary` reads the sink context, so
  it renders on every pass even below a bailout. The children are
  unmounted while the card shows (a boundary replaces them), so their hook
  state starts over when they draw again; frames must not depend on it
  anyway.
- Preview: `componentDidCatch` reports `{ id, name, message, stack,
  componentStack }` to the sink, once per commit, and `renderAt` returns the
  last pass's reports as `sceneErrors` (`settleTree` may render twice; the
  first pass's sink is dropped with it). The stack is cut at React's work loop
  (`react_stack_bottom_frame`, `react-reconciler/`), and the component stack
  at the `Boundary` frame.
- Elsewhere: the boundary renders by throwing `SceneRenderError` (an
  already-wrapped one passes through, so nested lists name the innermost
  scene). It becomes the root's uncaught error, which `HostRoot.render`
  throws, as any render error did before.
- Logging: React reports every caught error to the root's `onCaughtError`,
  which printed it with `console.error`. `markQuietBoundary()` in
  `reconciler.ts` stops that for `Boundary`, which reports its errors
  itself, so the preview does not print a stack on every frame.
- Mounting: `mount()` renders twice at time 0 to discover the compositions
  and the fonts, discarding the scene. Those passes catch (`catches: true`
  while `mounting`), so a film whose first scene throws on frame 0 still
  mounts; frame 0 then fails or draws the card like any frame. Before, such
  a film could not be loaded at all.
- Stacks: the CLI bundles with an inline source map (`sourcemap: 'inline'`,
  no `sourcesContent`, `sourceRoot` at the entry's folder) and calls
  `process.setSourceMapsEnabled(true)`, so a stack names
  `with-scene-error.tsx:40:47` instead of the temporary `entry.mjs`. This
  applies to every error the entry throws. With source maps on, a missing
  named export reaches the CLI in V8's words (`The requested module '…' does
  not provide an export named '…'`) instead of Node's (`Named export '…' not
  found. …`), so `misplacedImportError` reads both. 150 preview frames of
  `examples/afterimage` took 3.84–3.88 s with and without source maps.
- `collectAudio({ preview })`; the CLI passes the request's `preview`.
  Every frame reports a clip for the whole window it plays in, so a scene
  that fails only on some frames would still be heard on them. The preview
  sweep notes, for each clip, the boundaries around the node that reported
  it (`sceneBoundariesOf`, up the node's fiber; `walkNode` keeps the node
  being walked while the sweep runs), and which boundaries failed on each
  frame (the sink gets the boundary with each report), then cuts each clip
  where one of its boundaries failed (`cutAudioClip`). A cut piece starts
  its source and curves where the clip had got to, earlier by the cut, and
  keeps a fade only at the clip's own edge. Clips of other scenes are left
  as they were.
- Found while doing that: the walker shifts a head-clipped clip's
  keyframes later (`shiftAnimated(volume, -clipped)`), though the mix reads
  them on clip-local time and `2026-08-26-sequence-audio-react-preview.md`
  says they shift earlier. Not changed here.
- The card cuts a message by code points: half a surrogate pair in a text
  layer is JSON Rust refuses, which would stop the preview's runtime.
- An empty `id` on a series item is no id, as it is for layer ids.
- The preview worker holds the scene errors with the scene it reuses for
  inline text edits, so Problems keeps them while a text is edited.
- `celesta-react-bridge`: `SceneError` (with `label()` and `detail()`),
  `FrameEvaluation::scene_errors`, `Response::Ok`'s `scene_errors`,
  `evaluate_with_overrides()` (what `scene_at_with_overrides()` now wraps),
  and `collect_media()` sending `preview` when the bridge is a preview one.
  A non-preview bridge that receives scene errors (a runtime that ignored
  the flag) fails the frame with the same message, so exports never take a
  card even then.
- Editor: `render_whole_react_scene` returns the frame's scene errors into
  `PreviewResult::scene_errors`; `show_preview_result` hands them to
  `Problems::set_scene_errors`, which counts them as errors, shows them under
  Active in the Problems tab, and logs each one the previous frame did not
  have (no toast: the card shows it), so playing through a broken scene logs
  it once. A `Render` error no longer drops the bridge: it is that frame's
  error, and other frames draw. The React audio worker spawns its bridge as a
  preview one.
- `@celesta.studio/web` evaluates its preview and its MP4 export with the
  same `renderAt` call, without `preview`, so it still fails the frame (now
  naming the scene). Drawing the card in the browser preview needs the
  engine to tell the two apart first.
- `<Sequence>` is not wrapped: it also times media and small parts inside
  scenes, and has no name for a card. `SceneBoundary` is public for the
  cases that want it.

## Checks

On macOS (Apple silicon), Node 26.13, after `pnpm run build:runtime`:

- Vitest: `packages/react` (357, including `test/scene-boundary.test.mts`),
  `packages/transitions` (30, including the TransitionSeries card case), and
  `packages/cli` (124, including `test/scene-errors.test.mts`) pass, with
  `tsc -b test` for each. `pnpm run test:runtime` passed every package but
  `packages/voicevox`'s "packaged CLI …" case, which timed out: the esbuild
  binary its staged runtime starts hung at `_dyld_start` (see below).
- `cargo test -p celesta-react-bridge`: 122 unit tests (including
  `reads_the_scene_errors_of_a_preview_frame`) and 46 live bridge tests
  (including `a_throwing_scene_draws_its_card_only_in_the_preview_when_node_is_available`)
  pass. `cargo check -p celesta-editor -p celesta-react-bridge -p celesta-mcp
  -p celesta-exporter --all-targets` is clean, and `cargo fmt --all --check`.
- Not run: `cargo test -p celesta-editor` (its new
  `scene_errors_count_as_errors_and_are_logged_once_while_they_last` test)
  and Clippy. During this work, newly built proc-macro dylibs and newly
  copied executables stopped loading on this Mac (rustc waited in `dlopen`,
  esbuild and the FFmpeg plugin at `_dyld_start`, for 40 minutes and more,
  in other worktrees too), so nothing that needed a new binary could run.
  For the same reason the video export was not tried after the change (the
  plugin did not answer `hello`) and no preview screenshots were rendered
  here; the PR asks for editor screenshots.
- After, on the same fixture, with binaries built before the stall:
  `--frame 120` exits 1 with `… could not render the composition: scene
  "stats" (Stats) threw while rendering: Cannot read properties of undefined
  (reading 'label')` (`--json` code `react`); the contact sheet keeps its
  failed tile; `inspect.mjs` (`--native` and `--no-native`) prints `ERROR:
  scene "stats" (Stats) threw while rendering: …` and exits 1; MCP `inspect`
  and `render_frames` report the same message as errors. The CLI's preview
  frames carry the card and `sceneErrors`, and frames before and after the
  broken ones draw in any order.
