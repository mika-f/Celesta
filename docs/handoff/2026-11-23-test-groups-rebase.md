# Test groups: rebase onto main (2026-11-23)

#815's branch (with #816 merged into it) was 455 commits behind `main` and
conflicted in eight files. It was rebased onto `main` (`a1402a9a`); #816's
merge commit is gone and its five commits sit on top of #815's three.

## What the conflicts were

#816 moved every test that needs the React runtime out of the `integration`
targets into `runtime` targets (`docs/handoff/2026-11-11-test-groups.md`),
while `main` kept editing those tests where they were and added new ones.
Each of `main`'s changes went where its test now lives:

- Edits to moved tests were applied in `tests/runtime/` (exporter
  `export.rs`, `inspect.rs`, `stills.rs`; MCP `export.rs`).
- New React tests on `main` joined the runtime group, their
  `if !runtime_built() { return; }`-style skips replaced by the group's
  `crate::runtime()` / `crate::require_runtime()`:
  - exporter: `a_bare_entry_name_sends_a_plugin_absolute_asset_paths` (with
    `write_probe_plugin`), `json_reports_warn_about_math_random_and_the_clock_in_stills_and_video`,
    `native_inspection_reports_rough_chapters_and_their_desc`,
    `native_inspection_reports_nondeterministic_calls_once`, and
    `a_react_check_warns_about_rough_scenes_and_final_refuses_them` (a new
    `tests/runtime/quality.rs`). Its helper, `quality.rs`'s `exporter()`, is
    shared by both targets as `common::quality_exporter`.
  - MCP: `check_quality_reports_what_the_pixels_show`,
    `inspect_reports_quality_warnings_from_the_layers`, and `main`'s new
    `comments.rs` (one React test), now `tests/runtime/comments.rs`.
  - react-bridge: four new tests use `live_react_runtime()`'s non-`Option`
    form, and lost the `_when_node_is_available` suffix with the rest.
- `main` replaced MCP's `client_with_test_codec_in` with
  `client_with_test_codec(plugin, home)`; both targets use that.
- `scripts/report-skipped-tests.mts` keeps `main`'s nextest-only header
  (the plugins job now writes JUnit reports too) with #816's sentence on
  build prerequisites.

## Checks

Linux x64 container, lavapipe, runtime built with
`pnpm -r --filter '@celesta.studio/cli...' run build` (turbo cannot spawn its
tasks here: "Exec format error").

- `#[test]`/`#[tokio::test]` counts under `tests/` equal `main`'s: exporter
  92, MCP 43, react-bridge 49. A script compared every test function on
  `main` with its counterpart here: only the runtime gates differ.
- `CELESTA_REQUIRE_GPU=1 cargo test -p celesta-exporter -p celesta-mcp -p
  celesta-react-bridge --features celesta-exporter/runtime-tests,celesta-mcp/runtime-tests,celesta-react-bridge/runtime-tests -- --show-output`:
  all pass; `runtime` targets 31 (exporter), 25 (MCP), 49 (react-bridge).
  No runtime skip lines (only "this renderer does not offer yuv420p").
- `packages/cli/dist` moved aside: every runtime test fails with "the
  runtime tests need the built React runtime: run `pnpm install && pnpm run
  build:runtime` first", except react-bridge's three fake-runtime tests.
- `cargo fmt --check`; clippy on the three crates, all targets, with the
  features, `-D warnings`. The container's rustc is 1.97 (CI: 1.99.1), on
  which `crates/mcp/src/inspect.rs`'s `#[expect(clippy::result_large_err)]`
  is unfulfilled, on `main` as well, so that one run added
  `-A unfulfilled_lint_expectations`.
