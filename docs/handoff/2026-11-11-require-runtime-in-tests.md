# Tests: `CELESTA_REQUIRE_RUNTIME=1` fails missing-runtime skips (2026-11-11)

## Problem

Rust cannot mark a test skipped, so the tests that spawn
`packages/cli/dist/cli.js` through `node` print a `skipping ...` line and
pass when `node` or the built runtime is missing. That keeps
`cargo test --workspace` usable in a checkout where pnpm never ran, but in CI,
which always builds the runtime first, such a skip can only hide a broken
build step. `scripts/report-skipped-tests.mts` listed them as warnings;
nothing failed. GPU tests already had `CELESTA_REQUIRE_GPU=1` for this, and
the Media Foundation plugin `CELESTA_REQUIRE_MEDIA_FOUNDATION=1`.

## Change

`CELESTA_REQUIRE_RUNTIME=1` turns those skips into failures; unset (or any
other value), the tests skip as before, with the same skip lines. One helper
per test crate, the same shape as the GPU gate (`assert!` on the variable,
then `eprintln!` the skip line):

- `crates/react-bridge/tests/node_integration.rs`: `skip_without_runtime(reason)`
  and `node()`, which finds `node` (`CELESTA_NODE`, then `PATH`) or skips.
  The three fake-runtime tests and `live_react_runtime()` (used by the
  `*_when_node_is_available` tests) go through them. A missing `node` and a
  missing `dist/cli.js` or `packages/cli/node_modules` both fail under the
  variable.
- `crates/exporter/tests/integration/main.rs`: `runtime_built(cli, test)`,
  used by the 14 React tests in `export.rs` and `stills.rs` and by
  `inspect.rs`'s `runtime()`. These tests do not look for `node` themselves:
  the exporter spawns it, so a missing `node` already fails them.
- `crates/mcp/tests/integration/common/mod.rs`: `runtime_built()` asserts
  the same way.

Left out:

- `crates/bench/src/workloads.rs`: not a test skip. A React benchmark run
  returns an error when `cli.js` is missing, so the run already fails; its
  `#[test]`s (`run_frame`) need no runtime.
- The plugins job (`Plugins (<OS>)`): it builds the runtime only for a
  plugin with a JS package, for that package's own build and tests. No test
  in `plugins/` or `examples/plugins/` spawns `packages/cli/dist/cli.js`, so
  the variable would gate nothing there.
- GPU, font and symlink skips, and the precondition skip in
  `stills_need_no_media_plugin`, as scoped.

`Scratch::unmarked` (`crates/react-bridge/src/project_types.rs`, test-only)
no longer skips: a `celesta.config.json` in a folder above the temporary
folder is a mistake on the machine, so it now panics everywhere, with or
without the variable, naming the folder and saying to remove the file or
point `TMPDIR` elsewhere. It returns `Scratch` instead of `Option<Scratch>`,
and its four callers lost their `else { return; }`.

CI's `Test (<OS>)` step sets `CELESTA_REQUIRE_RUNTIME: '1'` on Windows, macOS
and Linux; each builds the runtime in "Build Celesta packages" first.
`HANDOFF.md` and `README.md` describe the variable beside the GPU one.

## Checks

Linux x64 container, Node 22, `mesa-vulkan-drivers` (lavapipe) installed.
`TESTS` below is `-p celesta-react-bridge --test node_integration -p
celesta-exporter --test integration -p celesta-mcp`.

- Runtime built, `CELESTA_REQUIRE_GPU=1 CELESTA_REQUIRE_RUNTIME=1 cargo test
  $TESTS --no-fail-fast -- --show-output`: 72 + 39 + 45 passed, no runtime
  skip lines (only the existing "this renderer does not offer yuv420p" ones).
- `packages/cli/dist` moved aside, variable unset: same counts pass, with
  the old skip lines (`skipping live Node.js test: run ...`, `skipping: build
  packages/cli first`, `skipping inspection test: ...`, ...).
- `packages/cli/dist` moved aside, `CELESTA_REQUIRE_RUNTIME=1`: 42
  react-bridge, 20 exporter and 28 MCP tests fail with "... require the
  built runtime: ... is missing" (react-bridge: "live Node.js tests require
  Node.js and the built runtime: run `pnpm install && pnpm run
  build:runtime` first (... is missing)").
- `node` off `PATH` (`PATH=/usr/bin:/bin`, `CELESTA_NODE` unset), one
  fake-runtime test: skips unset, fails with "live Node.js tests require
  Node.js and the built runtime: node was not found" when set.
- `Scratch::unmarked`: `cargo test -p celesta-react-bridge --lib
  project_types` passes (10); with `TMPDIR` under a folder holding a
  `celesta.config.json`, its four callers fail with the new message.
- `cargo fmt --check`; `cargo clippy -p celesta-react-bridge -p
  celesta-exporter -p celesta-mcp --all-targets -- -D warnings`.

Not run here: Windows and macOS. `pnpm run build:runtime` failed in this
container before building anything (turbo: "unable to spawn child process:
Exec format error"), so the runtime was built with the same package set
through `pnpm -r --filter '@celesta.studio/cli...' run build` after
`pnpm --dir packages/react run codegen --locked`. The exporter tests also
need `celesta-codec-test`'s program built (`cargo build -p
celesta-codec-test --bins`), as CI's Build step does.
