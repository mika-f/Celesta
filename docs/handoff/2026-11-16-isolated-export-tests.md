# Export tests ignore the developer's own plugins (2026-11-16)

## Problem

Three integration tests failed on a machine where the developer had installed
registry codecs globally (`celesta plugin install --global ffmpeg avfoundation`,
recorded as `registryPlugins` in the user's `settings.json`):

- `celesta-exporter`: `export::an_export_through_the_programs_ring_is_byte_identical_to_the_pipe`
  and `stills::separate_stills_and_videos_name_the_frame_that_failed`.
- `celesta-mcp`: `export::export_video_writes_an_mp4_with_progress`.

Each gave its child (`celesta-exporter` or `celesta-mcp`) the test codec with
`--plugin-dir` or `CELESTA_PLUGIN_PATH`, but the child also discovered the real
user folders, so a global registry codec became the default muxer and encoder.
The output was then not a test-codec file, and `celesta_codec_test::read`
failed with `Unsupported { … tried: [] }`. The tests only run after
`pnpm run build:runtime`; without it they skip.

[`2026-11-14-celesta-command.md`](2026-11-14-celesta-command.md) accepted
read-only discovery of the real user folders in tests. That still holds for
`celesta-plugin-host` tests, whose assertions only look at test ids, but not
for a test that expects its export to be written by the test codec.

## Specification

A test that starts `celesta-exporter` or `celesta-mcp` with the test codec sets
the child's environment from `celesta_codec_test::isolated_home_env(home)`,
with `home` a temporary folder:

- `HOME`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, and `APPDATA` at `home`, as
  `crates/cli/tests` and the exporter's `cli_formats` and `plugins_lock` tests
  already did. The child then reads no settings, presets file, or plugin of
  the developer's, and the test codec is its only codec.
- `PATH` with the folder of the developer's real Node.js first. A source build
  runs `node` from `PATH` for the React runtime, and that is often a version
  manager's shim (mise here) that reads its own state from the real home: with
  `HOME` moved, mise refused to run (`Config files in ~/.config/mise/config.toml
  are not trusted`) and the React export failed. The test finds the program
  once with `node -p process.execPath` under the developer's own environment.
  When no `node` runs, `PATH` is left as it is and the tests skip or fail as
  before.

## Implementation

| Path | Change |
| --- | --- |
| `crates/codec-test/src/home.rs` | `isolated_home_env(home)`, exported from the crate. |
| `crates/exporter/tests/integration/export.rs` | The ring test's exporter runs with `isolated_home_env`. |
| `crates/exporter/tests/integration/stills.rs` | `react_export` runs every export (stills and videos) with `isolated_home_env` in a temporary folder of its own; the frame assertion prints the report. |
| `crates/exporter/tests/integration/{cli_formats,plugins_lock}.rs` | `isolated_home_env` replaces their own variables; `a_broken_preset_file_is_a_usage_error` now sets it too. |
| `crates/mcp/tests/integration/common/mod.rs` | `client_with_test_codec(plugin, home)` always isolates the user folders; `client_with_test_codec_in` is folded into it. |
| `crates/mcp/tests/integration/{export,server}.rs` | Every test-codec client binds a temporary `home` before the client. |

Tests that start `celesta-mcp` with `client()` (no test codec) are unchanged:
they render stills or check arguments and use no codec.

## Checks

With the developer's real `settings.json` in place (global `ffmpeg`,
`ffmpeg-x264`, and `avfoundation` 0.10.0-alpha.3), after
`pnpm run build:runtime` and `cargo build -p celesta-codec-test`:

- `cargo test -p celesta-exporter -p celesta-mcp -p celesta-codec-test --locked`
  passes, the three tests above included (they run real exports, about 14 s
  each, rather than skipping), with `node` on `PATH` being mise's shim.
- `cargo clippy -p celesta-exporter -p celesta-mcp -p celesta-codec-test --tests -- -D warnings`
  and `cargo fmt --check` are clean.

In a fresh worktree, `pnpm run build:runtime` failed in
`@celesta.studio/code#build` with `Cannot find module '@celesta.studio/react'`:
Turbo's shared worktree cache reported a hit for `@celesta.studio/react#build`
without restoring `packages/react/dist`. `TURBO_FORCE=1 pnpm run build:runtime`
built it. Not addressed here.
