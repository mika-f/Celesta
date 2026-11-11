# Tests: a unit group and a runtime group (2026-11-11)

## Problem

Rust's test harness cannot report a test as skipped. `cargo test --workspace`
was the only entrance, so it tried to run every test even where one could not
run, and the tests that spawn `node` or `packages/cli/dist/cli.js` guarded
themselves: without the runtime they printed `skipping ...` and passed.
`CELESTA_REQUIRE_RUNTIME=1`
([2026-11-11-require-runtime-in-tests.md](2026-11-11-require-runtime-in-tests.md))
turned those skips into failures in CI as a stopgap.

## Groups

Two axes, kept apart.

- **Phase**, what a test needs to be built before it can run. This note
  groups the root workspace's tests by it:
  - the **unit group**, everything `cargo test --workspace` builds: needs
    nothing but the Rust build;
  - the **runtime group**, the `runtime` test target of
    `celesta-react-bridge`, `celesta-exporter`, and `celesta-mcp`: needs
    Node.js and the built React runtime (`pnpm install && pnpm run
    build:runtime`).
- **Capability**, what a test needs of the machine: a GPU adapter or format,
  a color emoji font, symbolic links. Such tests live in both groups (the
  GPU renderer's own tests are unit tests), so grouping by phase does not
  remove them; they keep their `skipping` lines, and `CELESTA_REQUIRE_GPU`
  is unchanged.

Inside a selected group a missing prerequisite is a failure that says what to
build, never a skip.

## Mechanism

Each of the three crates has a feature, `runtime-tests`, and a test target
named `runtime` with `required-features = ["runtime-tests"]`:

| Crate | Runtime target | Unit target it left |
| --- | --- | --- |
| `celesta-react-bridge` | `tests/runtime.rs` (was `tests/node_integration.rs`; all 45 tests start `node`) | none left |
| `celesta-exporter` | `tests/runtime/{export,stills,inspect}.rs`: 20 tests | `tests/integration/` |
| `celesta-mcp` | `tests/runtime/{export,inspect,render,server}.rs`: 28 tests | `tests/integration/` |

The exporter's and the MCP server's helpers both targets use are in
`tests/common/mod.rs`, included by each target's `main.rs` with `#[path]`;
every item there is used by both, so neither target needs `allow(dead_code)`.
Helpers only one target uses stay in it (the exporter's `runtime()` and the
MCP server's `entry()` and `require_runtime()` in the runtime target, the
hanging media plugin and `client_with_test_codec_in` in the unit target).

Entrances:

| Group | `cargo test` | nextest |
| --- | --- | --- |
| unit | `cargo test --workspace` | `cargo nextest run --workspace` |
| runtime | `cargo test --workspace --features runtime-tests --test runtime` | `cargo nextest run --workspace --features runtime-tests -E 'binary(runtime)'` |
| both (CI) | `cargo test --workspace --features runtime-tests` | `cargo nextest run --workspace --features runtime-tests` |

`--test runtime` without the feature is refused by Cargo itself: "target
`runtime` in package `celesta-exporter` requires the features:
`runtime-tests`".

A missing runtime fails with "the runtime tests need the built React runtime:
run `pnpm install && pnpm run build:runtime` first (<path> is missing)", and a
missing `node` (react-bridge) with "the runtime tests need Node.js: node was
not found on PATH or at CELESTA_NODE". The exporter's and the MCP server's
runtime tests do not look for `node` themselves: the exporter and the server
spawn it, so a missing `node` already fails them.

### Why a feature and separate targets

Candidates weighed:

- **`#[ignore = "..."]`, with `--include-ignored` / `--run-ignored all` in
  CI.** Plain `cargo test` would leave the tests out and count them as
  ignored. But `#[ignore]` already has another meaning here:
  `celesta-gpu-renderer`'s `write_web_effect_fixtures` is a fixture writer,
  not a check, and CI running ignored tests would run it too (every system
  rewriting `packages/web/test/fixtures/effects`), so every entrance would
  have to exclude it by name. An ignored test also reads as a disabled one.
- **nextest filtersets with a per-profile `default-filter`** (0.9.146 has
  `default-filter`, but no profile key for `run-ignored`). Only nextest
  reads them: plain `cargo test --workspace` would still run the runtime
  tests and fail without the runtime, so the documented local entrance would
  no longer pass.
- **A cargo feature on the tests themselves** (`#[cfg(feature = ...)]` per
  test). The tests could stay where they were, but a group could not be run
  alone (one binary holds both), and helpers only the runtime tests use
  would be dead code without the feature.
- **Separate targets with `required-features`** (chosen). Plain
  `cargo test --workspace` builds no runtime target and passes; one group is
  one command for each runner; the feature is the only switch, and a bare
  `--features runtime-tests` with `--workspace` turns it on in every crate
  that has it (Cargo refuses it only when no selected package has it), so a
  crate that adds a runtime target is in CI's run without a workflow change.

Costs of the choice:

- The runtime tests are absent, not reported, from `cargo test --workspace`.
- `clippy --all-targets` leaves a target out without a word when its
  feature is off, so CI's (and HANDOFF.md's) clippy passes
  `--features runtime-tests`. That one command lints both groups: the unit
  targets compile the same with or without the feature.
- The feature is a crate feature, so switching between the groups locally
  compiles the three crates, and the crates depending on them (the editor,
  the bench), once more; both builds then stay in `target/`. CI builds with
  the feature only (the Build step passes it as the Test step does).
- CI could lose the feature from its Test step and still pass. A step after
  the tests, "Check that the runtime group ran", fails unless the JUnit
  report has a `<crate>::runtime` suite.

## What changed besides the tests

- `.github/workflows/ci.yml`, Test job: Build, Test, and Clippy pass
  `--features runtime-tests`; `CELESTA_REQUIRE_RUNTIME` is gone; the new
  check above. The Plugins job is unchanged: no test in `plugins/` or
  `examples/plugins/` needs the runtime, so it has no runtime group, and
  passing the feature to `cargo test -p <plugin>` would be refused for a
  package without it.
- `HANDOFF.md` ("Examples and validation") gives the groups and their
  entrances; `README.md` likewise.
- `scripts/report-skipped-tests.mts`: comment only. What it reports now is
  capability skips: GPU adapter and formats (yuv420p, half floats), color
  emoji and bold fonts, symbolic links, the VRM samples folder, and
  `cli_formats`'s precondition skips (`stills_need_no_media_plugin`'s
  "skipped: ... sits beside the test binary"). `scripts/plugins/libtest.mts`
  is unchanged, for the same skips in the plugins job's log.
- `CELESTA_REQUIRE_RUNTIME` and the skip branches are gone. The skip lines
  the runtime group printed (`skipping live Node.js test: ...`,
  `skipping ...: build packages/cli first`, `skipping: build packages/cli
  first (pnpm run build:runtime)`) are gone with them.
- Comments naming the moved files: `packages/cli/test/inspect.test.mts`,
  `packages/character/test/lipsync-and-preset.test.mts`,
  `plugins/ffmpeg/tests/decode.rs`.

Left as they were:

- The GPU gate (`CELESTA_REQUIRE_GPU`) and the capability skips listed above.
- `crates/bench`: its React workload returns an error when `cli.js` is
  missing, so a benchmark run already fails; its `#[test]`s need no runtime.
- Test names: the `_when_node_is_available` suffix of 42 react-bridge tests
  is no longer a condition (the target always has `node`), but dropping it
  would rename 42 tests and touch most of the file that #805 also edits, so
  it stays for a follow-up.
- The fixture writer keeps its `#[ignore]`.

## Checks

Linux x64 container, Rust 1.99.1, cargo-nextest 0.9.146, Node.js 22,
`mesa-vulkan-drivers` (lavapipe).

- Same tests in CI: `cargo nextest list --workspace --profile ci` at #815's
  head (`da3d785`) listed 2829 tests to run and 1 ignored
  (`write_web_effect_fixtures`). On this branch, with
  `--features runtime-tests`, it lists 2829 and the same 1 ignored; matched
  by crate and test name (the module paths of moved tests changed), the two
  lists are equal, nothing missing and nothing added. Without the feature it
  lists 2736, and with the feature and `-E 'binary(runtime)'` 93
  (react-bridge 45, exporter 20, MCP 28): no test in both, and together
  exactly the 2829.
- Unit group, `packages/cli/dist` absent: `cargo test --workspace --locked
  --no-fail-fast -- --show-output` exits 0, 2735 passed plus the
  harness-less `stdout` test, 1 ignored. No `build packages/cli first` or
  `skipping live Node.js test` line; the only skip lines are lavapipe's
  three `yuv420p` ones.
- Runtime group, `packages/cli/dist` absent: `cargo test --workspace --locked
  --features runtime-tests --test runtime --no-fail-fast` exits 101; 90
  tests fail with "the runtime tests need the built React runtime: run
  `pnpm install && pnpm run build:runtime` first (…/packages/cli/dist/cli.js
  is missing)"; react-bridge's 3 fake-runtime tests, which need only `node`,
  pass. With `PATH=/usr/bin:/bin` and no `CELESTA_NODE`, a fake-runtime test
  fails with "the runtime tests need Node.js: node was not found on PATH or
  at CELESTA_NODE".
- Runtime group, runtime built: the same command passes, 45 + 20 + 28, with
  no skip line.
- CI's Test step: `CELESTA_REQUIRE_GPU=1 cargo nextest run --workspace
  --features runtime-tests --locked --no-fail-fast --profile ci`: 2829
  passed, 1 skipped (the ignored fixture writer). The new check finds the
  suites `celesta-exporter::runtime` (20), `celesta-mcp::runtime` (28), and
  `celesta-react-bridge::runtime` (45) in `junit.xml`;
  `report-skipped-tests.mts` lists only the three `yuv420p` skips. A
  `--profile ci` run without the feature (`-E 'package(celesta-mcp)'`, 67
  tests) leaves no runtime suite in the report, and the check fails.
- `cargo clippy --workspace --all-targets --features runtime-tests --locked
  -- -D warnings` and the `codegen` clippy command pass; each of the three
  crates also passes `cargo clippy -p <crate> --all-targets -- -D warnings`
  without the feature. `cargo fmt --all --check`, the fframes and
  `plugins/` (`-p celesta-codec-ffmpeg`) format checks, and
  `git diff --check` pass.
- `git merge-tree` of this branch with `main` and with each open pull
  request: no textual conflict. #805's two added lines land in
  `crates/react-bridge/tests/runtime.rs`, and no open pull request adds a
  call to the removed helpers (`runtime_built`, the `Option`-returning
  `live_react_runtime` and `node`) or `CELESTA_REQUIRE_RUNTIME`.

Not run here: Windows and macOS; the plugins workspace's clippy and tests
(no FFmpeg development libraries here; the only change there is a comment in
`plugins/ffmpeg/tests/decode.rs`). `pnpm run build:runtime` failed in this
container before building anything (turbo: "unable to spawn child process:
Exec format error"), as in #815's checks, so the runtime was built with the
same package set through `pnpm --dir packages/react run codegen --locked`
and `pnpm -r --filter '@celesta.studio/cli...' run build`. The builds here
used CI's `CARGO_PROFILE_DEV_DEBUG=1` and `CARGO_PROFILE_TEST_DEBUG=1`, and
`CARGO_INCREMENTAL=0`, to fit the container's disk.
