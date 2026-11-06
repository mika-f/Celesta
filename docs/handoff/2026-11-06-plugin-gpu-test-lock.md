# Plugin GPU tests run one at a time

## Problem

CI's `Plugins (Linux x64)` job sometimes failed in
`cargo test -p celesta-codec-ffmpeg --test export` (in the `plugins/`
workspace): the test binary died with `signal: 11, SIGSEGV` about nine
seconds after `running 8 tests`, before any test reported (for example
job 127952412107 on PR 739). Reruns and `main` passed, and every test passes
on macOS (Metal).

The tests in `plugins/ffmpeg/tests/export.rs` each build an `Exporter`, and
so a `GpuRenderer` with its own wgpu device, and libtest ran them on
parallel threads with no lock. Linux CI renders with Mesa's lavapipe, which
is not safe with several devices made and used at once in one process.
`crates/gpu-renderer/src/tests.rs` (`GPU_LOCK`) and
`plugins/vrm/tests/mtoon.rs` (`GPU`) already serialize their GPU tests for
the same reason; the plugin export tests did not.

## Change

- `plugins/ffmpeg/tests/export.rs`: a static `GPU` mutex and `gpu()`, which
  recovers from poisoning so one failed test does not fail the rest. Every
  test that exports (all but `the_ffmpeg_codec_has_every_format_encoder`,
  which only checks encoders) holds `let _gpu = gpu();` for its whole body,
  so one exporter renders at a time.
- `plugins/vrm/tests/render.rs`: the same lock for its two renderer tests,
  which also ran in parallel unlocked.

Other plugin tests need nothing: `plugins/ffmpeg/tests/process.rs` and
`decode.rs` never render, `plugins/vrm/tests/samples.rs` has one renderer
test, and `mtoon.rs` already locks. Cargo runs test binaries one after
another, so a lock per binary is enough.

A new plugin test that makes a `GpuRenderer` or an `Exporter` should take
its file's `gpu()` lock for as long as it renders.

## Checks

With `PKG_CONFIG_PATH` set to Homebrew `ffmpeg@8`'s pkgconfig folder on
macOS:

- `cargo test --manifest-path plugins/Cargo.toml --workspace --locked`
- `cargo clippy --manifest-path plugins/Cargo.toml --workspace --all-targets --locked -- -D warnings`
- `cargo fmt --check` in `plugins/`

The crash is intermittent and lavapipe-only, so the PR's
`Plugins (Linux x64)` job is the check that matters.
