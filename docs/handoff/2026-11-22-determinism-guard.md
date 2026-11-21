# Determinism guard: frame-derived clock and random

## Specification

Every frame is a pure function of its time. Until now only the skill's rule
5 said so; the runtime now holds every React entry to it, on every path that
evaluates one (the editor preview, `celesta-exporter` videos, stills and
subtitles, `--inspect` and `inspect.mjs`, the MCP tools, and
`@celesta.studio/web`, which uses the same `mount`).

While a frame is evaluated:

- `Math.random()` draws from a mulberry32 generator seeded with the FNV-1a
  hash of the frame's time as a reduced fraction (`1/30` and `2/60` are the
  same seed). Every render pass restarts the sequence, and a nested
  evaluation (a render pass inside the frame's walk) leaves the outer
  sequence where it was, so a frame that took two passes (fonts changed, a
  ref was read) draws what a one-pass frame draws.
- `Date.now()` is `floor(time × 1000)` milliseconds after
  1970-01-01T00:00:00Z (`FROZEN_EPOCH_MS = 0`); `performance.now()` is
  `time × 1000` with its fraction; `new Date()` and `Date()` read
  `Date.now()`. `new Date(value)`, `Date.parse`, `Date.UTC` and
  `instanceof Date` are unchanged (the replacement shares `Date.prototype`).
- When the entry's code calls one of them, it is reported once per runtime
  process, with the stable code `nondeterministic_render` starting the
  warning:
  `nondeterministic_render: the entry calls Math.random() and Date.now() while rendering; Celesta reseeds …`.
  The APIs are listed in a fixed order (`Math.random()`, `Date.now()`,
  `performance.now()`, `new Date()`).

### What `prepare()` gets

Decision: `prepare()` and the entry's module scope run outside any frame,
with the real `Date`, `performance.now` and `Math.random`, and are not
reported. `prepare()` runs once per process, before any frame, and may need
real time: timeouts and retries of a fetch, cache stamps, today's date for a
title card. Whatever it produces is computed once and read by every frame,
so it cannot make frames disagree with each other; it can only make two runs
disagree, which is what network data does anyway. The docs tell authors to
read real dates there and keep them in module state.

Timers (`setTimeout`, `setInterval`) are not replaced: their callbacks never
run inside a synchronous frame. Passive effects that React flushes outside
`root.render` see the real clock.

## Implementation

- `packages/react/src/determinism.ts`: `evaluateDeterministically(time, fn)`
  installs the replacements on entry to the outermost call and restores the
  originals on exit (also when `fn` throws), so nothing outside a frame sees
  them. `setNondeterminismSource(predicate)` says which stack frames are the
  entry's; `takeNondeterministicCalls()` returns the APIs first seen since
  the last call. Attribution captures a 4-frame stack
  (`Error.captureStackTrace`) at an API's call and checks the first frame
  with a file position, skipping built-ins such as
  `Array.prototype.map(Math.random)`. Once an API is reported, its calls skip
  the stack capture, so a particle field's thousands of `Math.random()` calls
  cost only the generator. Without `captureStackTrace` (non-V8 engines) or a
  source, nothing is reported; the values are still deterministic.
- `packages/react/src/render.ts`: `mount`'s `renderTree` (every
  `root.render`, including the discovery passes at time 0), `renderFrame`
  (`renderAt`, `collectAudio`, `collectSubtitles`) and `walkLayerNodes`
  (`layerSource`, `collectOccurrences`) evaluate under the guard, as does
  `createResolver().resolve` with the request's runtime time (frame 0 when
  absent).
- `packages/cli/src/cli.ts`: the source is the bundle's URL, taken from the
  bundle's real path because Node names ES modules (and their stack frames)
  by it (`/private/var/…` for macOS's `/var/…` temporary folder). Everything
  bundled counts as the entry's: its files, its npm packages, and plugin
  modules; React and the `@celesta.studio/*` packages are external, so the
  reconciler's own `performance.now()` reads get frozen values but are not
  reported. Every answer (ready message, frame, collections, layer source,
  resolutions, errors) carries `nondeterministic: [...]` with the APIs new
  since the previous answer (`answer()`); requests Node sends meanwhile
  (`measureText`, …) never do.
- `crates/react-bridge`: `protocol::Answer` reads a response together with
  its `nondeterministic` list (and `ReadyMessage::Ready` its own);
  `ReactBridge::nondeterministic_calls()` returns the accumulated
  `NondeterministicCalls`, whose `warning()` is the text above
  (`NONDETERMINISTIC_RENDER` is the code). An older runtime sends no list.
- Surfaces:
  - editor: `preview.rs` adds the warning to the preview's warnings, shown in
    Problems for as long as the bridge runs the entry (a reload starts over);
  - `celesta-exporter`: `react_entry::report_nondeterminism` after a React
    video's frames, a subtitles-only run, and React stills (also when a
    still fails), so `--json` lists it in `warnings`; exports from the
    editor and the MCP `export`/`render_frames` tools go through the same
    code;
  - `celesta-exporter --inspect`: the handshake and each answer carry the
    new APIs as `nondeterministic`, like the CLI's messages;
  - `inspect.mjs`: gathers `nondeterministic` from either mode and prints the
    same warning (`warnings` in `--json`, now also with `--loop`);
  - MCP `inspect`: adds the warning to `warnings`.

## Checks

Run on macOS (Apple silicon), Node 26.13, with the runtime built by
`pnpm run build:runtime`:

- `pnpm --dir packages/react exec vitest run` and `tsc -b test`:
  `test/determinism.test.mts` covers order independence, the seed's
  reduction, the frozen clock values, restoring the real functions (also
  after a throw), nested evaluation, once-per-API reporting, React's own
  reads not being reported, and resolved components.
- `pnpm --dir packages/cli exec vitest run test/determinism.test.mts
  test/inspect.test.mts`: the protocol field on ready, frame and error
  answers; module scope and `prepare()` keeping the real clock; the
  `inspect.mjs --no-native` warning in text and `--json`.
- `cargo test -p celesta-react-bridge`: the warning's wording and order,
  parsing `nondeterministic` on answers and the ready message, and the live
  tests `frames_are_deterministic_whatever_their_order_when_node_is_available`
  (two runtimes, opposite orders, identical frames; the four APIs reported)
  and `an_entry_that_reads_no_clock_reports_nothing_when_node_is_available`.
- `cargo test -p celesta-exporter --test integration`:
  `native_inspection_reports_nondeterministic_calls_once` passes.
  `json_reports_warn_about_math_random_and_the_clock_in_stills_and_video`
  passes its stills half (the warning in `--json`, identical pixels for the
  same still twice); its video half could not run locally: the
  `celesta-codec-test` program copied into a temporary plugin folder did not
  answer `hello` within 30 s while several other sessions were building, and
  the existing `an_export_through_the_programs_ring_is_byte_identical_to_the_pipe`
  failed the same way at the same time. Left to CI.
- `cargo clippy -p celesta-react-bridge -p celesta-exporter -p celesta-mcp
  -p celesta-editor --all-targets -- -D warnings`, `cargo fmt --all --
  --check`, `git diff --check`; `pnpm --dir packages/web exec vitest run`
  and the whole `packages/cli` suite still pass.

Entries that never call these APIs evaluate the same scenes as before. No
example or template calls them while rendering; `packages/cli/examples/homepage-demo.tsx`
reads `Date.now()` in `prepare()` for its GitHub query, which keeps working
because `prepare()` keeps the real clock. For the pull request, a 640×360
scene placing 40 squares with `Math.random()` and printing `Date.now()` was
exported at frame 30 twice with each build: the main checkout's exporter
gave two different PNGs (and a wall-clock `Date.now()`), this branch's gave
byte-identical PNGs showing `Date.now() = 1000`, with the warning on stderr.
