# nextest for the plugins' tests (2026-11-15)

CI's `Plugins (<OS>)` jobs ran `cargo test`, while the root `Test` job runs
nextest. The reason was the VRM plugin's `process` target: it sets
`harness = false` so its `main` can call `set_var` before any thread exists,
and nextest could not list a binary that ignores libtest's command line.

## Specification

- `plugins/vrm/tests/process.rs` keeps `harness = false` and still sets
  `CELESTA_VRM_TEXTURE_CACHE` in `main` before any thread exists. It then
  hands its two checks to libtest-mimic (dev-dependency, `0.8.2`), which takes
  libtest's command line, so `cargo test` and nextest list and run them as two
  tests (`the_process_answers_as_the_provider_does`,
  `motions_cross_the_process`). A failed check panics, which fails its test.
  The cache directory is dropped before `Conclusion::exit`, which skips
  destructors. Under nextest each test is its own process, with its own cache.
- `scripts/plugins/run-cargo.mts` takes `nextest` beside `test`, `clippy` and
  `build`: it runs `cargo nextest run -p <package> ...` one package at a time,
  each against its own FFmpeg build, as before.
  - The cross-plugin prerequisite builds run for `nextest` as for `test`.
    Under `nextest`, `--profile`/`-P` is nextest's profile and stays off those
    builds; `--cargo-profile X` becomes their `--profile X`.
  - Every package's run writes the profile's JUnit report to the same
    `<target>/nextest/<profile>/junit.xml`, so after each package the script
    renames it to `junit-<package>.xml`. Before each package it deletes both
    names, so a report left by an earlier run (rust-cache restores `target/`)
    never stands for a run that wrote none.
- `plugins/.config/nextest.toml` gives the plugins workspace the same `ci`
  profile as the root (`junit.xml`, `store-success-output = true`); nextest
  reads the config of the workspace it runs in. It is in `SHARED_INPUTS`, so
  changing it tests every plugin.
- `scripts/report-skipped-tests.mts` reads one or more JUnit reports and
  counts each reason across them. It no longer reads `cargo test --
  --show-output` logs: `scripts/plugins/libtest.mts` and its test are gone.
- `ci.yml`'s plugins job installs `nextest@0.9.146` (as the test job, after
  rust-cache) when a Cargo package is affected, runs
  `run-cargo.mts "$workspace" nextest $packages --locked --no-fail-fast --profile ci`,
  and reports skipped tests from every workspace's `junit-*.xml`. A workspace
  without a report passes its unexpanded glob, which the reporter names as
  missing.
- nextest skips doctests; the plugins have none.

## Checks

- `pnpm --dir scripts/plugins test`: 252 tests, including the new
  `run-cargo.test.mts` cases (nextest per package with renamed reports, Cargo
  vs nextest profiles on prerequisite builds, a stale report removed) and a
  reporter case reading two reports.
- `node scripts/plugins/run-cargo.mts plugins nextest -p celesta-plugin-vrm --locked --no-fail-fast --profile ci`
  on macOS arm64: 387 tests passed, `process` listed as two tests, and
  `plugins/target/nextest/ci/junit-celesta-plugin-vrm.xml` written;
  `node scripts/report-skipped-tests.mts plugins/target/nextest/ci/junit-*.xml`
  lists the 10 sample tests skipped without `CELESTA_VRM_SAMPLES_DIR`.
- A deliberately broken assertion in `process.rs` fails that test alone under
  nextest; `cargo test -p celesta-plugin-vrm --test process` still runs both.
- `celesta-codec-ffmpeg` and `celesta-codec-avfoundation` (which builds the
  FFmpeg programs first on macOS) under the same command; clippy on
  `celesta-plugin-vrm`; `cargo fmt --manifest-path plugins/Cargo.toml --all -- --check`;
  `git diff --check`.
- Not run locally: `celesta-codec-ffmpeg-x264` (no x264 FFmpeg build here),
  `celesta-codec-mediafoundation` (Windows), and the Linux and Windows jobs;
  CI covers them.
