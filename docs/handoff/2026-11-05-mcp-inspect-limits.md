# MCP `inspect`: frame cap, deadline, and cancellation

Issue #703. The MCP `inspect` tool evaluated any number of frames, ignored
`notifications/cancelled`, and kept a blocking-pool thread and its Node
process forever when the entry's `prepare()` never resolved.

## Specification

- `loop` with `frames` or `every` is a `usage` error, worded like
  `inspect.mjs`'s: `loop picks its own frames; leave out frames and every`.
- At most `MAX_INSPECT_FRAMES` (1000, the same as `MAX_STILL_FILES`) frames
  per call. A `frames` list longer than that is refused before the entry is
  loaded; `every` is checked once the composition's length is known, before
  any frame is evaluated. Both are `usage` errors worded like `too_many` in
  `stills::select` (`… (pass fewer frames)`, `… (choose a larger every)`).
  The cap is hard: checking every frame of a longer video goes through
  `inspect.mjs --every 1` in a shell, which prints as it goes, and the skill
  says so.
- `timeout` (seconds, default 180 as `inspect.mjs --timeout`) bounds the
  whole call, loading the entry included. Zero, negative, or unrepresentable
  values are `usage` errors. When it passes, the runtime is killed and the
  call fails with error code `timeout`; the message says whether the entry
  was still loading (then it points at `prepare()`) or how many frames were
  done.
- A cancelled request kills the runtime the same way and fails with
  `cancelled`. A request already cancelled when the tool starts loads
  nothing.

## Implementation

- `celesta-react-bridge` gains `RuntimeStop` (`stop.rs`), passed in
  `SpawnOptions::stop`. `ReactBridge` now keeps its `Child` in an
  `Arc<Mutex<_>>` that the stop shares, so another thread can kill the
  runtime while the bridge blocks reading it — including inside
  `spawn_with_options`, where `prepare()` runs. A stop called before the
  spawn kills the runtime as soon as it starts. The bridge still reaps the
  child on drop; `StderrCapture::exit_error` takes the mutex and locks it
  per `try_wait`, so a stop is never held up for long.
- `crates/mcp/src/inspect.rs`: `Stop` wraps a `RuntimeStop` and the first
  `Halt` (`Cancelled`, `TimedOut`). `inspect` checks it before loading,
  after a failed spawn, and before every frame and loop pair; an evaluation
  error after a halt is reported as the halt. `select` is a free function,
  counts `every` frames arithmetically before listing them, and drops
  repeats with a `HashSet`.
- `server.rs` takes the `RequestContext`, halts at once on an already
  cancelled token, and spawns a watcher that halts on cancellation or after
  the timeout, aborted when the inspection returns.

## Checks

```bash
export PKG_CONFIG_PATH="$(brew --prefix ffmpeg@8)/lib/pkgconfig"
pnpm run build:runtime
cargo test -p celesta-mcp
cargo test -p celesta-react-bridge --test node_integration runtime_stop
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- Unit tests (`inspect::tests`): `loop` with `frames` and with `every`, a
  1001-frame list, `every` past the cap (18,000 frames at `every: 1`),
  repeats, `timeout` validation, and an already cancelled stop returning
  `cancelled` without loading the (missing) entry.
- Integration tests: `every: 1` on a 1001-frame entry is a `usage` error; a
  `prepare()` that never resolves ends with `timeout` after `timeout: 2`, and
  cancelling such a request ends its Node process (checked by pid on Unix).
  With `RuntimeStop::stop` made a no-op, both hanging tests fail.
- Bridge test: a `RuntimeStop` ends a spawn stuck in `prepare()`, and one
  already stopped ends the next spawn at once.
