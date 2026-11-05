# First-party plugins in one workspace, the codec out of the core, and plugin CI on demand (2026-11-05)

Design for a four-layer stack. Each layer is one pull request, stacked with
`gh stack` in the order below.

## Why

- CI runs the VRM plugin's job on every pull request, though most pull
  requests touch nothing the plugin uses. More plugins are coming, so a
  per-plugin workflow with a hand-written `paths` list does not scale. The
  lists would also go stale as dependencies change.
- `celesta-codec-ffmpeg` (`2026-11-05-codec-plugin.md`) took FFmpeg out of
  Celesta's *executables*, but the crate is still a member of the root
  workspace under `crates/`. `celesta-exporter` and `celesta-react-bridge`
  use it as a dev-dependency for their tests, and `celesta-exporter` also
  uses `ez-ffmpeg` to make its test fixtures. So every root build and test
  links FFmpeg, and the root `Test` job sets FFmpeg up on all three
  operating systems: Homebrew on macOS, and on Windows the vcpkg package
  that `ffmpeg-windows.yml` builds. The codec is a plugin and belongs with
  the plugins.

## Goals

- First-party plugins live in `plugins/<id>/` and share one Cargo
  workspace, `plugins/Cargo.toml`.
- The root workspace has no FFmpeg anywhere in its dependency graph,
  including dev-dependencies. The root `Test` job no longer sets up FFmpeg.
- The core still checks that it writes correct video and audio, with no
  FFmpeg: a lossless test codec stands in.
- Plugin CI runs only for the plugins a change affects. It finds them from
  the dependency graphs, not from hand-kept lists. Each affected plugin is
  tested on all three operating systems.
- Packages ship the same files as before: the FFmpeg codec is bundled in
  the plugin folder, with its GPL text and source offer.

## Non-goals

- Declaring the host version a plugin requires in its manifest. Today a
  manifest has only `protocol` (an exact `u32`). This is a separate issue.
- Choosing between several codecs, for example by output format, or falling
  back to another codec when one lacks an encoder. Today the first runnable
  codec in search order is used for everything. This is a separate issue,
  related to mika-f/celesta#645.
- Splitting the VRM implementation into its own crate (mika-f/celesta#669).
- Moving `examples/plugins/` (flag, turntable, beep-voice). They stay root
  workspace members and stay in the root `Test` job on every pull request.

## Anti-goals

- A development setup that loads a development-only plugin path without the
  developer asking for it. `CELESTA_PLUGIN_PATH` is set by hand and never
  from `mise.toml` or another file every shell picks up. Tests do not
  depend on that variable.
- A plugin that the affected-plugin check misses and that is then never
  tested. Pushes to `main` test every plugin.
- Core tests that lose their check of the real output. The test codec
  writes the exact bytes the exporter hands it, so the core's output can
  be compared bit for bit.

## Constraints

- Cargo allows only one package with a given `links` value per workspace,
  and `ffmpeg-sys-next` declares `links = "ffmpeg"`. If a future plugin
  needs a different, incompatible FFmpeg binding, it cannot share the
  plugins workspace. That plugin is excluded from `plugins/Cargo.toml` and
  becomes a workspace of its own, with its own `Cargo.lock`. The tooling
  below handles both layouts.
- `celesta-codec-ffmpeg` keeps Celesta's version (0.9.0 today) and stays
  publishable on crates.io. Its dependencies on core crates keep the exact
  pin (`path` plus `version = "=0.9.0"`), so a release bump that misses the
  codec fails to resolve. `celesta-plugin-vrm` keeps its own version
  (0.1.0) and `publish = false`. So `plugins/Cargo.toml`'s
  `[workspace.package]` shares no `version`.
- GPU tests follow the root `Test` job. On Linux, Mesa's lavapipe runs them
  and `CELESTA_REQUIRE_GPU=1` makes a missing adapter a failure. On macOS
  and Windows, `CELESTA_REQUIRE_GPU=0`, and a test that finds no adapter
  skips itself and is reported by `scripts/report-skipped-tests.mjs`. On
  `main`'s latest run all three operating systems ran every GPU test.
- Tests are Vitest in TypeScript for JavaScript, and Rust tests stay in
  Rust (`AGENTS.md`).
- When a commit or step changes user-facing behavior, the user guide is
  updated in both `packages/website/content/en/` and `.../ja/`.

## Layer 1: the plugins workspace

- `plugins/Cargo.toml` is a virtual workspace whose members are the plugin
  directories under `plugins/`, minus any excluded plugin. It has one
  `Cargo.lock` and one `target/`.
  `plugins/vrm/Cargo.toml` loses its empty `[workspace]`, and its
  `Cargo.lock` becomes `plugins/Cargo.lock`.
- The root `Cargo.toml` excludes `plugins` (was `plugins/vrm`).
- One build script builds any plugin directory. It replaces the two drifted
  copies, `plugins/vrm/build.mjs` and `examples/plugins/build.mjs`, and
  fixes mika-f/celesta#692 (the target triple to `<os>-<arch>` mapping, from
  `rustc --print cfg`). It has a Vitest test. It writes a plugin directory
  Celesta loads into the `dist/` folder beside the plugin directory:
  `plugins/dist/<id>` for first-party plugins (was `plugins/vrm/dist/vrm`),
  so one `CELESTA_PLUGIN_PATH` entry finds them all, and
  `examples/plugins/dist/<id>` as before. The codec in layer 3 is built with
  the same script.
- The `vrm-plugin` CI job only points at the new workspace in this layer
  (`--manifest-path plugins/Cargo.toml -p celesta-plugin-vrm`, rust-cache
  `plugins -> target`).
- `plugins/vrm/README.md`, `HANDOFF.md`, and
  `docs/handoff/2026-11-03-plugin-vrm.md` say where the plugin is built from.

## Layer 1: what was done

- **Workspace.** `plugins/Cargo.toml` is a virtual workspace
  (`resolver = "3"`, `members = ["vrm"]`); `plugins/vrm/Cargo.lock` moved to
  `plugins/Cargo.lock`, and the build goes to `plugins/target`. The root
  `Cargo.toml` excludes `plugins`, and `.gitignore` ignores `/plugins/target`
  and `/plugins/dist`, and `/plugins/*/target` and `/plugins/*/dist` for an
  older checkout's `plugins/vrm/target` and `plugins/vrm/dist` and for a
  plugin excluded into a workspace of its own (`/plugins/vrm/assets`
  stays).
- **Build script.** `scripts/plugins/` is a private pnpm package
  (`celesta-plugin-scripts`) with `platform.mjs` (a platform's
  `<os>-<arch>` from `rustc --print cfg`'s `target_os` and `target_arch`,
  for the host or a `--target`, which closes mika-f/celesta#692),
  `manifest.mjs` (the manifest from `plugin.json` and what the build made),
  `cargo.mjs` (`cargo metadata --no-deps`: the target directory and the
  members' versions), and `build.mjs`. Vitest tests cover `platform.mjs`
  and `manifest.mjs`, one of them running the real `rustc`, and
  `repository.test.mts` checks the repository's plugins' versions through
  `cargo.mjs` (below).
  `node scripts/plugins/build.mjs <plugin directory>... [--out <dir>]
  [--target <triple>] [--skip-js]` replaces `plugins/vrm/build.mjs` and
  `examples/plugins/build.mjs`, which are deleted. Each directory resolves
  against the current directory, and its output goes by default to the
  `dist/` folder beside it: `plugins/vrm` to `plugins/dist/vrm`,
  `examples/plugins/flag` to `examples/plugins/dist/flag`. There is no
  default list of plugins; the samples are built by naming all three.
- **Versions.** Every `plugin.json` gives its `version`, which the
  manifest takes. A plugin with a `cargoPackage` gives the same version in
  that Cargo package; the two are bumped together (by tooling that updates
  both), and the check catches a miss. The build script reads the
  package's version with `cargo metadata` and refuses a plugin whose
  versions differ (`<plugin.json>: version A differs from the Cargo package
  P's B`) before the release build, and a `plugin.json` without a
  `version` before anything is built. `scripts/plugins/test/repository.test.mts`
  checks every `plugins/*/plugin.json` and `examples/plugins/*/plugin.json`
  with a `cargoPackage` the same way, so a miss fails the tests before a
  build. The root plugin-host test
  `the_repositorys_plugins_pass_the_manifest_checks` writes each
  `plugin.json`'s version into a manifest, so it relies on every
  `plugin.json` keeping one.
- **CI.** The `vrm-plugin` job (still `VRM plugin`) no longer sets up
  FFmpeg: neither `plugins/Cargo.lock` nor the codegen `build:runtime` runs
  has it. Its rust-cache workspaces are `plugins -> target` and
  `. -> target`, without the FFmpeg cache key. It runs
  `pnpm --dir scripts/plugins test` after the component test, then
  `cargo fmt --check` with `working-directory: plugins`,
  `cargo clippy --manifest-path plugins/Cargo.toml --workspace --all-targets --locked -- -D warnings`,
  `cargo check --manifest-path plugins/Cargo.toml -p celesta-plugin-vrm --locked --target x86_64-pc-windows-msvc --lib --bins`,
  `env CELESTA_REQUIRE_GPU=1 cargo test --manifest-path plugins/Cargo.toml --workspace --locked`,
  and `node scripts/plugins/build.mjs plugins/vrm`. The design's
  `-p celesta-plugin-vrm` became `--workspace` for clippy and the tests,
  so a new member is checked without editing the job.
- **Formatting without `--all`.** The plan's
  `cargo fmt --manifest-path plugins/Cargo.toml --all --check` also checks
  the root crates the plugins depend on by path (the renderer, the plugin
  host, and what they pull in), which root CI does not format-check, and
  it fails on their existing formatting (`crates/editor`, `crates/mcp`,
  `crates/react-bridge`, `crates/source-edit`). Without `--all`, how the
  workspace is named matters. Run in `plugins/`, `cargo fmt --check`
  checks every member and not their path dependencies, so CI's Format step
  runs it there (`working-directory: plugins`). From the repository root,
  `cargo fmt --manifest-path plugins/Cargo.toml --check` works today only
  because the workspace has one member: on a virtual manifest with two, it
  exits 1 with "Failed to find targets". `-p <package>` with the manifest
  path checks that one member, which is how `plugins/vrm/README.md` gives
  it. Reproduced with rustfmt 1.10.0 (Rust 1.99.1) on a scratch virtual
  workspace with members `a` (with a path dependency `dep` outside the
  workspace) and `b`, each with one unformatted file:
  - `cargo fmt --manifest-path ws/Cargo.toml --check`: "Failed to find
    targets", exit 1.
  - `cargo fmt --manifest-path ws/Cargo.toml -p a --check -- -l`: lists
    `a` only.
  - `cd ws && cargo fmt --check -- -l`: lists `a` and `b`, not `dep`.
  - `cd ws && cargo fmt --all --check -- -l`: lists `a`, `b`, and `dep`.
- **Docs.** `plugins/vrm/README.md`, `examples/plugins/README.md`, the flag
  and turntable READMEs, the four demos' usage comments, the user guide's
  plugins page (en and ja), the Celesta skill's `plugins.md`, `HANDOFF.md`
  (the flag and turntable rows, new `celesta-plugin-vrm` and
  `scripts/plugins` rows, and the plugins workspace's commands in its
  validation section), and `2026-11-03-plugin-vrm.md`'s build, file table,
  and checks give the new commands and paths. `plugins/vrm/js`'s
  `package.json` description and `tsconfig.json` comment name the new
  script.
- **Stale references left.** `git grep -n -E "plugins/vrm/build\.mjs|examples/plugins/build\.mjs|plugins/vrm/dist|plugins/vrm/Cargo\.lock|manifest-path plugins/vrm"`
  still matches dated plans under `docs/superpowers/plans/` (45 lines in
  six plans), earlier handoff notes (`2026-11-01-plugin-system.md`,
  `2026-11-02-plugin-add-menu.md`, `2026-11-03-plugin-vrm-motion.md`,
  `2026-11-05-codec-plugin.md`, one line each), this note's Layer 1
  section, and the dated design
  `docs/superpowers/specs/2026-11-03-plugin-3d-content-vrm-design.md`
  (one line). They record what was true when they were written.

### Checks (macOS, Apple Silicon, Metal)

- `cargo fmt --check` in `plugins/`: clean; `cargo fmt --check -v` lists
  only `plugins/vrm`'s files. `cargo fmt --manifest-path plugins/Cargo.toml
  -p celesta-plugin-vrm --check` (the README's form): clean.
- `cargo clippy --manifest-path plugins/Cargo.toml --workspace --all-targets --locked -- -D warnings`:
  no warnings.
- `cargo test --manifest-path plugins/Cargo.toml --workspace --locked`:
  374 passed, 0 failed (309 unit, 55 `mtoon`, 2 `process`, 2 `render`, 6
  `samples`). `CELESTA_VRM_SAMPLES_DIR` was unset, so the 6 `samples` tests
  passed as skipped. The GPU tests ran on Metal: `mtoon`, `render`, and
  `samples` also pass with `CELESTA_REQUIRE_GPU=1`.
- `pnpm --filter celesta-plugin-vrm-js test`: 17 passed (1 file).
- `pnpm --dir scripts/plugins test`: 23 passed (3 files). With flag's
  `plugin.json` version changed to 0.1.1 by hand, `repository.test.mts`
  failed for flag (expected 0.1.0, received 0.1.1), and
  `node scripts/plugins/build.mjs examples/plugins/flag` stopped with
  "version 0.1.1 differs from the Cargo package celesta-plugin-flag's
  0.1.0" before building; the version was then put back.
- `cargo metadata --format-version 1 --locked --no-deps` at the root:
  succeeds.
- At the root, `cargo test -p celesta-plugin-host --lib --locked
  the_repositorys_plugins_pass_the_manifest_checks`: 1 passed (it fails,
  "invalid type: null, expected a string", if a `plugin.json` has no
  `version`). `cargo test -p celesta-plugin-host --locked`: 53 passed,
  0 failed.
- `node scripts/plugins/build.mjs plugins/vrm`: writes `plugins/dist/vrm`
  whose manifest has version 0.1.0 and an `executable` for this machine
  only (`macos-aarch64`), with `js/index.js` and `js/index.d.ts`.
- `node scripts/plugins/build.mjs examples/plugins/flag
  examples/plugins/turntable examples/plugins/beep-voice`: writes the three
  into `examples/plugins/dist/`, each at version 0.1.0; flag and turntable
  have a `macos-aarch64` executable, and beep-voice has no `executable`.
- From `plugins/`, `node ../scripts/plugins/build.mjs vrm --skip-js`
  resolves `vrm` against the current directory and writes
  `plugins/dist/vrm` again (with `--out` to a scratch folder too: a
  manifest and `bin/macos-aarch64/` only, no `js/`).
- The Windows-target
  `cargo check --manifest-path plugins/Cargo.toml -p celesta-plugin-vrm --locked --target x86_64-pc-windows-msvc --lib --bins`
  was not run here; it is left to CI's `vrm-plugin` job.

## Layer 2: a lossless test codec in the core

- A test codec implements `celesta_media::CodecProvider` in pure Rust and
  runs in-process, as `FfmpegCodec::factory()` does today.
  - **Encoding.** It writes the bytes the exporter passes to
    `encode_frame` (in the `PixelFormat` it was opened with) and the audio
    passed to `mux_audio`, compressed losslessly with a fast pure-Rust
    compressor (LZ4, for example `lz4_flex`). No conversion to RGB, so the
    frames are exactly what the core produced.
  - **Decoding.** It reads the same files back, so `probe`,
    `decode_video`, and `decode_audio` work on media that tests write
    themselves, in place of fixtures generated with FFmpeg's `lavfi`. It
    also reads PCM WAV, so tests on the checked-in voices
    (`examples/assets/voices/*.wav`, the lip-sync and audio examples) keep
    their real waveforms in the core. *(Builder-check call.)*
  - It honours the `CodecProvider` contract the exporter relies on:
    containers, `check_encoder`, the lead-in, and errors for what it does
    not support.
- `celesta-exporter` and `celesta-react-bridge` tests move to it (about 100
  tests: `crates/exporter/src/tests.rs`, `crates/exporter/tests/integration/`,
  `crates/react-bridge/src/decode.rs`,
  `crates/react-bridge/tests/node_integration.rs`, and
  `crates/react-bridge/examples/audio-bench.rs`). Where a test checked an
  FFmpeg decode within a tolerance, it now checks exact values where the
  core's own output allows.
- Tests of the contract with FFmpeg move into `celesta-codec-ffmpeg`'s
  tests and drive the real exporter through a dev-dependency on
  `celesta-exporter`. These are: MP4 with silent audio, ProRes 4444 and VP9
  WebM with alpha, every format muxing audio as long as the video, and
  `converting_colors_on_the_gpu_matches_the_encoder_conversion`. Tests that
  open a checked-in MP4 or other compressed media (for example
  `with-video.tsx`'s `./clip.mp4` in `node_integration.rs`) move there too,
  through a dev-dependency on `celesta-react-bridge`. *(Builder-check
  call.)*
- **Tests that export from a child process.** Some tests start Celesta's
  own programs and export video from them: `crates/mcp/tests/integration/export.rs`
  (an MP4 with progress, and cancelling it) and `react_export` in
  `crates/exporter/tests/integration/stills.rs`. Today the child finds
  `celesta-codec-ffmpeg` beside it in `target/<profile>`. An in-process
  codec cannot serve a child, so:
  - The test codec's crate also has a program (for example
    `celesta-codec-test`) that serves the codec protocol. It is a root
    workspace member that no package ships.
  - Such a test writes a temporary plugin folder whose manifest
    (`"roles": ["codec"]`) names that program, and gives it to the child:
    `--plugin-dir` for `celesta-exporter`, and `CELESTA_PLUGIN_PATH` set on
    the child's own environment for `celesta-mcp`. The developer's own
    `CELESTA_PLUGIN_PATH` is removed from the child's environment first.
  - The test finds the program in `target/<profile>` from its own
    executable's path. When it was not built (say, `cargo test -p
    celesta-mcp` alone), the test fails with a message that says to build
    it, as `crates/exporter/examples/preview-bench.rs` does for the codec
    today.
  - `InstalledPlugins::codec()` and the packages do not change for this.
- After this layer, no core crate depends on `celesta-codec-ffmpeg` or
  `ez-ffmpeg`. The codec is still a root member until layer 3.

## Layer 2: what was done

- **The test codec.** `crates/codec-test` is the package
  `celesta-codec-test` (library `celesta_codec_test`, program
  `celesta-codec-test`), a root member with `publish = false`. Its
  `[workspace.dependencies]` entry is `path` only, so `cargo publish` drops
  it from the dev-dependencies of the crates that are published. It is a
  crate of its own rather than a `celesta-media` feature, so no feature
  unification can pull it into a release build.
  - **File format.** One file per output, whatever its extension (fixtures
    are named `*.ctc`): the magic `CLSTTEST`, a `u32` LE format version 1, a
    `u32` LE header length, a JSON header (`container`, `video` with
    `width`, `height`, `frameRate`, and `format`, and `audio` with
    `sampleRate`, `channels`, and `leadInFrames`, each `null` when absent),
    then chunks: a tag byte, a `u32` LE payload length, and a payload
    compressed with `lz4_flex::block::compress_prepend_size`. `V` is one
    video frame, the exact bytes passed to `encode_frame`; `A` is the whole
    audio as interleaved `f32` LE samples, lead-in included; `E` ends the
    file. A file without `E` is unfinished, as an aborted encode leaves it,
    and `probe`, `read`, and decoding refuse it with "unfinished".
  - **Encoding** stores every `PixelFormat` byte for byte, checks each
    frame's length (YUV chroma is `ceil(w/2) × ceil(h/2)`), and refuses an
    MP4 with a CRF over 51 or a bit depth other than 8 or 10 with FFmpeg's
    messages. `check_encoder` accepts all three containers, `mux_audio`
    refuses a video that already has audio, and `abort_encoder` drops the
    writer at once.
  - **Decoding** returns RGBA8 (`Rgba16` rounded per channel; YUV refused),
    the frame at `floor(time × fps + 1e-9)` clamped to the last one, and a
    negative or non-finite time as `InvalidTimestamp`. Audio is the samples
    after the lead-in; it is never resampled (another rate is an error that
    names both), and mono and stereo convert exactly. It reads 16-bit PCM
    WAV (format tag 1, unknown chunks skipped), so the checked-in voices
    keep their real waveforms; any other WAV is an error that names the
    file, its format tag, and its bit depth. A missing file is
    `MediaError::NotFound(path)`.
  - `celesta_codec_test::read` gives a test-codec file's container, frames,
    and samples (with `lead_in()` and `played()`), for comparing byte for
    byte; it refuses WAV. `write_rgba_video` and `write_audio` write
    fixtures, and `install()` sets the codec in-process.
  - **The program and `TestCodecPlugin`.** `src/main.rs` serves the codec
    protocol as `celesta-codec-ffmpeg`'s does. `TestCodecPlugin::new()`
    finds the program in the test executable's profile folder (the parent
    of `deps`), hard-links it into a temporary folder (falling back to a
    copy, as the host refuses a symlink that leads outside the folder), and
    writes a `celesta-plugin.json` with `"roles": ["codec"]` naming it with
    `EXE_SUFFIX`. When it is not built, it panics with "celesta-codec-test
    is not built in <profile>: build it first with cargo build -p
    celesta-codec-test (with --release for release tests)".
- **Core tests on the test codec.** `crates/exporter/src/tests.rs`
  (`the_test_codec_has_every_format_encoder`),
  `crates/exporter/tests/integration/export.rs`,
  `crates/react-bridge/src/decode.rs`, and
  `crates/react-bridge/tests/node_integration.rs` use it in-process; the
  `lavfi` fixtures became `write_rgba_video` and `write_audio` fixtures.
  Frame contents and counts, the audio's length, the silent mix, the
  lead-in, reported versus written loudness, repeated exports, and the WAV
  samples are now exact. The tolerances kept measure the core's DSP or the
  half-float canvas: the pitch (±3 %), the fades' and the ducked bed's RMS
  (±10 %), the loudness target (±0.1 LU), and the transparent quadrant (one
  8-bit code, 257, in 16 bits). The solid-video fixture sets the item's
  position to the canvas centre, so it covers every pixel the exact check
  reads. The range test asserts that the full export's four frames all
  differ, so the windowed frames cannot match by accident.
- **Child processes.** `react_export` in
  `crates/exporter/tests/integration/stills.rs` runs `celesta-exporter`
  with `--plugin-dir` set to a `TestCodecPlugin` folder and
  `CELESTA_PLUGIN_PATH` removed; the MCP export tests set
  `CELESTA_PLUGIN_PATH` on the child to that folder
  (`client_with_test_codec`), which replaces the developer's value. Each
  child-written video is read back with `celesta_codec_test::read`, which
  only a test-codec file passes: the MCP MP4 (60 frames, `mp4`) and, from
  the CLI, the bad-frame React entry exported with `--to 1`, before its
  failing frame 6 (4 frames, `mp4`). The MCP tests stop the server
  (`client.cancel().await`) before the plugin folder is removed, which
  matters on Windows, where a running program cannot be deleted.
- **FFmpeg's contract in its own tests.** `crates/codec-ffmpeg/tests/export.rs`
  drives the exporter through `FfmpegCodec` with the `lavfi` fixtures and
  the original assertions and tolerances: MP4 with silent audio (with its
  cancellation check), `converting_colors_on_the_gpu_matches_the_encoder_conversion`,
  ProRes 4444 and VP9 WebM with alpha, every format muxing audio as long as
  the video, `a_looping_mp4_starts_its_sound_from_its_end` (the AAC edit
  list), `brings_the_mix_to_a_loudness_target` (24-bit PCM and AAC), and
  `the_ffmpeg_codec_has_every_format_encoder`. Each has a core counterpart
  on the test codec. `celesta-codec-ffmpeg` takes `celesta-exporter`,
  `celesta-gpu-renderer`, `celesta-project`, `strum`, and `png` as
  dev-dependencies.
- **No react-bridge test moved to `celesta-codec-ffmpeg`.** The design's
  example, `with-video.tsx`'s `./clip.mp4`, is not checked in, and the
  bridge tests only read its timing; the only media they decode are the
  checked-in WAVs. So `celesta-codec-ffmpeg` has no dev-dependency on
  `celesta-react-bridge`.
- **`audio-bench`** no longer links a codec: it uses
  `InstalledPlugins::find_reporting(&[]).codec()`, and when that finds
  none, the `celesta-codec-ffmpeg` program in `target/<profile>`, as
  `preview-bench` does. The plan's Decisions said `codec()` alone "still
  finds `celesta-codec-ffmpeg` beside it in `target/release`"; it does not,
  because its sibling lookup looks beside the example's own executable,
  `target/<profile>/examples/`. Without a codec the bench prints "no codec
  plugin found: build one first (cargo build --release -p
  celesta-codec-ffmpeg) or set CELESTA_PLUGIN_PATH" and exits 1. Layer 3
  revisits the `target/<profile>` fallback once the codec leaves the root
  workspace.
- **Manifests.** `celesta-exporter` and `celesta-react-bridge` lost their
  `celesta-codec-ffmpeg` and `ez-ffmpeg` dev-dependencies and take
  `celesta-codec-test`, as `celesta-mcp` does. `cargo tree --workspace -e
  normal,build,dev -i celesta-codec-ffmpeg` lists only the crate itself,
  `cargo tree --workspace -e normal,build,dev -i ez-ffmpeg` lists only
  `celesta-codec-ffmpeg` as its dependent, and
  `git grep -n -E "celesta_codec_ffmpeg|ez_ffmpeg|FfmpegBackend|FfmpegCodec|lavfi" -- crates ':!crates/codec-ffmpeg'`
  matches nothing. The root `Test` CI job still sets up FFmpeg, for
  `celesta-codec-ffmpeg`; layer 3 removes it.
- **Docs.** `HANDOFF.md` (development setup, the `celesta-codec-ffmpeg`
  and new `celesta-codec-test` rows, the architecture boundary, and the
  validation paragraph: tests that spawn `celesta-exporter` or
  `celesta-mcp` need `celesta-codec-test` built), the README's FFmpeg
  sentence, and `decode.rs`'s module comment (it decodes through the codec
  plugin's `MediaBackend`) say the core's tests use the test codec. The
  user guide and the Celesta skill do not change: no user-facing behavior
  or command changed.
- **Left for layer 3.** The children that
  `crates/exporter/tests/integration/inspect.rs` starts pass no
  `--plugin-dir`; once the sibling codec is gone from `target/<profile>`,
  check what their native inspection probes media with.

### Checks (macOS 26.7.1, Apple M5, Metal)

Rust 1.99.1, rustfmt 1.10.0, Node.js 26.13.0, with `packages/cli/dist`
built (`pnpm run build:runtime`) and `cargo build -p celesta-codec-test`
run first. No test printed a "skipping" line.

- `cargo build --workspace --all-targets --locked`: succeeds. Its only
  warnings are the existing `block v0.1.6` future-incompatibility note and
  the editor's debug-build linker note about the `__eh_frame` section.
- `cargo test --workspace --no-run --locked`: builds every test target.
- `cargo clippy -p celesta-codec-test -p celesta-exporter -p celesta-react-bridge -p celesta-mcp -p celesta-codec-ffmpeg --all-targets --locked -- -D warnings`:
  no warnings.
- `rustfmt --edition 2024 --check` on the 18 `.rs` files
  `git diff --name-only claude/plugins-workspace -- '*.rs'` lists: clean
  (not `cargo fmt --all`, which fails on existing formatting in
  `crates/mcp` and `crates/react-bridge`).
- `cargo test -p celesta-codec-test`: 12 passed (9 `codec`, 2
  `plugin_folder`, 1 `process`).
- `cargo test -p celesta-exporter --lib the_test_codec_has_every_format_encoder`:
  1 passed.
- `cargo test -p celesta-exporter --test integration export::`: 16 passed,
  the GPU tests on Metal.
- `cargo test -p celesta-exporter --test integration stills::separate_stills_and_videos_name_the_frame_that_failed`:
  1 passed, including the `--to 1` export read back with
  `celesta_codec_test::read`.
- `cargo test -p celesta-react-bridge --lib decode::tests`: 4 passed.
- `cargo test -p celesta-react-bridge --test node_integration`: 43 passed.
- `cargo test -p celesta-mcp --test integration export::`: 5 passed.
- With `CELESTA_PLUGIN_PATH` naming a codec plugin folder whose program
  does not exist, the MCP `export::` tests (5) and the `stills::` test
  above (1) passed again, so the developer's value does not reach the
  children. Discovery does choose that plugin: `celesta-exporter
  --plugin-dir <folder> examples/minimal.celesta.json out.mp4` fails with
  "encoder check failed: the codec plugin is unavailable: could not start
  …/does-not-exist".
- `cargo test -p celesta-codec-ffmpeg --test export`: 8 passed; also with
  `CELESTA_REQUIRE_GPU=1`.
- `git diff --check claude/plugins-workspace`: clean.
- The full workspace suite was not run here; it is left to CI, whose first
  Linux (lavapipe) and Windows runs are also the first checks of the exact
  pixel comparisons off Metal. If they fail, look first at `assert_solid` in
  `exports_frame_exact_mp4_with_silent_audio`.

## Layer 3: the codec moves to `plugins/ffmpeg`

- `crates/codec-ffmpeg` moves to `plugins/ffmpeg` and joins the plugins
  workspace. The package and program stay `celesta-codec-ffmpeg`. It gets
  a `plugin.json` with `"roles": ["codec"]`, so the shared build script
  makes its plugin directory.
- **One source of the plugin folder.** *(Builder-check call.)* The shared
  build script makes the folder that ships, for every plugin:
  - `plugin.json` and the plugin's Cargo package (`cargoPackage`) carry
    the same version, bumped together, and the script refuses a plugin
    whose versions differ (layer 1 does this). The codec's `plugin.json`
    gives Celesta's version, as its Cargo package does.
  - It copies a plugin's `licenses/` folder when there is one. The codec
    keeps its GPL text and source offer there.
  - `scripts/stage-codec-plugin.mjs` is retired, and packaging calls the
    shared script. `scripts/test-package.mjs` checks the folder it makes.
- The root `Cargo.toml` loses the member, the `celesta-codec-ffmpeg` and
  `ez-ffmpeg` workspace dependencies, and the root `Cargo.lock` loses
  FFmpeg.
- `ci.yml`'s `Test` job drops `setup-ffmpeg`, its background wait, and
  `FFMPEG_VERSION FFMPEG_FEATURES` in the rust-cache key.
- Until layer 4, the plugin job tests the whole plugins workspace on all
  three operating systems, with FFmpeg set up, so the codec stays tested.
- **Finding the codec.** `InstalledPlugins::codec()` keeps its order: the
  first runnable codec plugin in search order, else `celesta-codec-ffmpeg`
  beside the running executable. That sibling fallback stays for
  `cargo install`, which puts both programs in `~/.cargo/bin`. It no longer
  serves a source build, whose `target/` directories are now separate.
- **Developing.** A source build builds the codec's plugin directory with
  the shared build script and points `CELESTA_PLUGIN_PATH` at it by hand.
  `HANDOFF.md`, `README.md`, and the user guide's build-from-source pages
  (en and ja) say so. The exporter's `no_codec` hint
  (`crates/exporter/src/report.rs`) and the editor's missing-codec problem
  name what a source build is missing.
- **Packaging.** `package-build.yml`, `packaging/linux/Dockerfile`,
  `scripts/package-macos.py`, `scripts/package-windows.ps1`, and
  build the codec from the plugins workspace with the shared build script.
  The plugin folder, its GPL text, and its source offer are unchanged, and
  `scripts/test-package.mjs` still checks them.
  `packaging/GPL-SOURCE-OFFER.md` names the new path.
- **perf.yml** builds the codec from the plugins workspace where a
  benchmark needs one, and drops `setup-ffmpeg` where none does.
- **Publishing** (`2026-11-04-crates-io-publish.md`): the codec is
  published from the plugins workspace after the core crates.
- **Release check**: the codec's own `version` matches the root
  `workspace.package.version`.

## Layer 3: what was done

The plan is `docs/superpowers/plans/2026-11-05-plugins-workspace-layer3.md`;
its Decisions give the reasons for the three deviations below.

- **The move.** `git mv crates/codec-ffmpeg plugins/ffmpeg`;
  `plugins/Cargo.toml` has `members = ["ffmpeg", "vrm"]`. The package and
  program stay `celesta-codec-ffmpeg`. Its `Cargo.toml` writes out the
  package fields (version 0.9.0, edition 2024, `MIT OR Apache-2.0`,
  `rust-version = "1.89"`) and pins `celesta-composition`,
  `celesta-media`, and the dev-dependencies `celesta-exporter`,
  `celesta-gpu-renderer`, and `celesta-project` as
  `{ path = "../../crates/<dir>", version = "=0.9.0" }`; it stays
  publishable. The root `Cargo.toml` lost the member and the
  `celesta-codec-ffmpeg` and `ez-ffmpeg` entries, and the root
  `Cargo.lock` lost `ez-ffmpeg`, `ffmpeg-next`, `ffmpeg-sys-next`,
  `bindgen`, and `vcpkg`. `plugins/Cargo.lock` only gained entries, at the
  versions the root lock had. `tests/export.rs`'s `workspace_root()`
  still lands on the repository root. The plugins workspace has no
  `[profile.dev.package.signalsmith-stretch] opt-level = 3`, which the
  root has; the codec's export tests showed no slowdown without it.
- **The codec's manifest.** `plugins/ffmpeg/plugin.json` is
  `{"id": "ffmpeg", "version": "0.9.0", "cargoPackage": "celesta-codec-ffmpeg", "roles": ["codec"], "licenses": [...]}`,
  and the build script writes
  `{"id": "ffmpeg", "version": "0.9.0", "protocol": 1, "roles": ["codec"], "executable": {"<os>-<arch>": "bin/<os>-<arch>/celesta-codec-ffmpeg[.exe]"}}`.
  `the_repositorys_plugins_pass_the_manifest_checks` writes each manifest
  as the build script does and asserts that `ffmpeg` has the codec role and
  no module.
- **The build script** (`scripts/plugins/build.mjs`) reads and checks every
  `plugin.json` before it creates a folder or runs Cargo. A plugin without
  a `module` gets no `module`/`types` and no module build, and the
  `project-types` prerequisite applies only when some plugin in the run has
  a module. A plugin with neither `module` nor `cargoPackage`, or with
  `components` but no `module`, is refused. `roles` passes through to the
  manifest. `licenses.mjs` resolves the optional `licenses` list; each run
  replaces `<plugin directory>/licenses/` with those files, copied by name,
  and refuses a missing file or two files with one name.
- **Deviations from the spec text.**
  - (a) The spec says the script "copies a plugin's `licenses/` folder".
    Instead, `plugin.json` has a `licenses` list naming repository files
    (`../../LICENSE-GPL-2.0`, `../../LICENSE-MIT`, `../../LICENSE-APACHE`,
    `../../packaging/GPL-SOURCE-OFFER.md`), which the build script copies.
    No copy is checked in under `plugins/ffmpeg/`, and the shared script
    knows nothing about the GPL.
  - (b) The spec says the plugin folder is unchanged. The packaged program
    moves from `bin/` to `bin/<os>-<arch>/`, the layout the build script
    gives every plugin. The host reads the program's path from the
    manifest's `executable`, so it did not change. On macOS the codec's
    dylibs stay in the plugin's `Frameworks/`, at
    `@executable_path/../../Frameworks/` from the program, and
    `package-macos.py` reads the program's folder from the manifest; on
    Windows the MSVC runtime DLLs go beside the program.
  - (c) The spec says the plugin folder's "source offer [is] unchanged".
    Instead, `packaging/GPL-SOURCE-OFFER.md` names `LICENSE-GPL-2.0`,
    `LICENSE-MIT`, and `LICENSE-APACHE` as code spans instead of
    `../LICENSE-*` links, so the same bytes read right in `packaging/`,
    the app's `licenses/`, and the plugin's `licenses/`; the build script
    never rewrites a file. It names `plugins/ffmpeg` for the source.
- **Versions.** `node scripts/plugins/bump-version.mjs X.Y.Z` (helper
  `version.mjs`) bumps the root `workspace.package.version`, every pin on
  one of Celesta's crates (a one-line inline table with `path` and
  `version = "=X"`; comments, third-party pins, and pins without a `path`
  stay), and, for each folder in `FOLLOWS_CELESTA` (`plugins/ffmpeg`), its
  Cargo package's `version` and `plugin.json`'s. A file whose version is
  not Celesta's stops it before anything is written. It then runs
  `cargo update --workspace` for both workspaces; when one fails, it
  writes every manifest and lockfile back. Each `packages/*/package.json`
  stays a hand step, which `HANDOFF.md` names. `repository.test.mts`
  checks that each `FOLLOWS_CELESTA` plugin gives the root's version, and
  `package.yml`'s release check refuses a tag whose version differs from
  `plugins/ffmpeg/plugin.json`'s.
- **CI.** The `vrm-plugin` job ("VRM plugin") became `plugins`
  ("Plugins (<OS>)"), a matrix over the `Test` job's three runners with
  `fail-fast: false`. It is not a required check (the "CI Green" ruleset
  does not name either job). It sets up FFmpeg as the `Test` job did
  (in the background with `wait-all` off Windows, in the foreground on
  Windows), keys its rust-cache on `FFMPEG_VERSION FFMPEG_FEATURES`, and
  runs Format and Clippy on Linux only. The Linux
  `cargo check --target x86_64-pc-windows-msvc` step is dropped: Windows
  builds and tests natively. Tests stay `cargo test`, because vrm's
  `process` target is `harness = false` and nextest cannot list it, so a
  GPU test skipped on macOS or Windows shows only as a "skipping" line in
  the log until layer 4. The `Test` job dropped setup-ffmpeg, its wait,
  and the FFmpeg cache key. `perf.yml` dropped them too, and
  `.github/actions/setup-ffmpeg/**` from its paths: `celesta-bench` links
  no codec.
- **Packaging.** `scripts/package-macos.py`, `scripts/package-windows.ps1`,
  and `packaging/linux/Dockerfile` build the root crates without the codec
  and the codec with `scripts/plugins/build.mjs plugins/ffmpeg --out <the
  package's plugin folder>`, into `plugins/target`. macOS passes its
  `build_env` without `CARGO_TARGET_DIR`, with `--target <triple>` and the
  headerpad `RUSTFLAGS`; `--skip-build` and `-SkipBuild` still build the
  codec, which costs nothing when `plugins/target` is current.
  `package-build.yml`'s rust-cache lists `. -> target` and
  `plugins -> target`, and `.dockerignore` has `plugins/target`. Both
  packagers merge `cargo metadata --manifest-path plugins/Cargo.toml` into
  the root's, deduplicated by package `id`, so the license inventory keeps
  `ez-ffmpeg`, `ffmpeg-sys-next`, and the codec. That call runs without
  `--offline`: the packages never build VRM, so its crates are not
  downloaded and an offline call would fail. `scripts/stage-codec-plugin.mjs`
  is deleted. `scripts/test-package.mjs` checks the codec's manifest (the
  codec role, one `executable` that exists), its `licenses/`, and that
  `rust-packages.json` names `ez-ffmpeg` and `celesta-codec-ffmpeg`.
  `test_package_macos.py` tests reading the program from the manifest and
  resolving the plugin's dylibs from its folder; its existing fixture now
  creates `Contents/MacOS`, without which POSIX cannot resolve
  `MacOS/../Frameworks` (that test failed on macOS before this layer).
- **Finding the codec.** `InstalledPlugins::codec()` is unchanged; its
  sibling fallback now serves only `cargo install`. The exporter's
  `no_codec` hint and the editor's missing-codec problem tell a source
  checkout to run `node scripts/plugins/build.mjs plugins/ffmpeg` and add
  the absolute path of `plugins/dist` to `CELESTA_PLUGIN_PATH` (the editor
  also offers to reinstall Celesta), and their tests assert that. Both
  bench examples find the codec through `find_reporting(&[]).codec()`
  alone, read their arguments first, and without a codec exit 1 with the
  same instruction; their profile-folder fallbacks are gone.
- **Docs.** `HANDOFF.md` (setup with `ffmpeg@8`, the workspace map, the
  run commands, deleting a pre-move `celesta-codec-ffmpeg` from
  `target/<profile>`, the plugins workspace's checks needing FFmpeg, the
  release step), `README.md`, the user guide's build-from-source,
  troubleshooting, and plugins pages (en and ja), the Celesta skill's
  `export.md`, `plugins.md`, `verify.md`, and `setup.md`, the packaging
  READMEs, `2026-11-04-crates-io-publish.md` (the codec is published from
  the plugins workspace after the core crates; 17 root crates are
  publishable), the `docs/performance/` reproductions (every one that decodes
  media or writes video builds the codec plugin and sets
  `CELESTA_PLUGIN_PATH`; the benches that link no codec drop the FFmpeg
  setup), and
  the `.cargo/config.toml` comment.
- **The branch.** The plan names the branch `claude/codec-in-plugins`; it
  is `claude/codec-plugin-move`.
- **Stale references left.** `git grep -n -E "crates/codec-ffmpeg|stage-codec-plugin|-p celesta-codec-ffmpeg"`
  matches `2026-11-05-codec-plugin.md` (8 lines), the layer 2 and 3 plans
  (14 and 25), and this note's Layer 2 and Layer 3 sections and this
  record; they record what was true when they were written or name the
  old paths on purpose. The other matches, in
  `2026-11-04-crates-io-publish.md`, are the current
  `cargo publish --manifest-path plugins/Cargo.toml -p celesta-codec-ffmpeg`.

### Checks (macOS 26.7.1, Apple M5, Metal)

Rust 1.99.1, rustfmt 1.10.0, Node.js 26.13.0, Homebrew `ffmpeg@8` 8.1.3 on
`PKG_CONFIG_PATH` for the plugins workspace only, and `packages/cli/dist`
built.

- `cargo build --workspace --all-targets --locked` at the root, with no
  FFmpeg on `PKG_CONFIG_PATH`: succeeds (only the existing `block v0.1.6`
  and `__eh_frame` notes).
- `cargo metadata --format-version 1 --locked --no-deps` at the root and
  with `--manifest-path plugins/Cargo.toml`: succeed.
  `git grep -n -i ffmpeg -- Cargo.lock`: nothing.
- `cargo clippy -p celesta-exporter -p celesta-editor -p celesta-react-bridge -p celesta-plugin-host --all-targets --locked -- -D warnings`:
  no warnings. `rustfmt --edition 2024 --check` on the five root `.rs`
  files the layer changed: clean.
- `cargo test --manifest-path plugins/Cargo.toml --workspace --locked --no-run`:
  builds every test target of both plugins.
  `cargo clippy --manifest-path plugins/Cargo.toml --workspace --all-targets --locked -- -D warnings`:
  no warnings. `cargo fmt --check` in `plugins/`: clean.
- `cargo test --manifest-path plugins/Cargo.toml -p celesta-codec-ffmpeg --locked --test export`
  with `CELESTA_REQUIRE_GPU=1`: 8 passed. `--test process`: 4 passed.
- `pnpm --dir scripts/plugins test`: `tsc -p test`, then 45 passed
  (6 files). Task 3 also ran the bump on the tree (`0.9.1`: exactly the
  root and codec manifests, `plugin.json`, and both lockfiles, only
  Celesta's packages moved) and a hand-made miss (the repository test and
  the bump both refuse it), then restored the files.
- `python3 scripts/test_package_macos.py`: 6 passed.
- `cargo test -p celesta-plugin-host --lib --locked the_repositorys_plugins_pass_the_manifest_checks`:
  1 passed.
- With no codec anywhere (no `celesta-codec-ffmpeg` under `target/`,
  `CELESTA_PLUGIN_PATH` and `CELESTA_SYSTEM_PLUGIN_DIR` unset, no user
  plugin folder, none in `~/.cargo/bin`):
  `cargo test -p celesta-exporter --locked --test integration inspect::`:
  7 passed (and `cli_formats::`, 3, in Task 2); neither needed a test
  codec folder. `cargo run -p celesta-exporter -- --react packages/cli/examples/title.tsx <out>.mp4 --json`
  exits 1 with `no_codec` and the new hint. Both bench examples print the
  "no codec plugin found" instruction and exit 1 (Task 6; not rerun).
- `node scripts/plugins/build.mjs plugins/vrm plugins/ffmpeg`: writes
  `plugins/dist/vrm` and `plugins/dist/ffmpeg`; the codec folder holds the
  manifest above (`macos-aarch64`), the program, and the four license
  files, each equal to its source. With
  `CELESTA_PLUGIN_PATH="$PWD/plugins/dist"`, the same export succeeds:
  150 frames, 21,082 bytes, `ftyp` at byte 4.
- `cargo test -p celesta-exporter --lib report::tests::media_nothing_could_decode_is_reported_as_no_codec`
  and `cargo test -p celesta-editor --bin celesta-editor plugins::tests::a_missing_codec_is_a_problem`
  (the editor has no lib target): 1 passed each.
- `ruby -ryaml -e 'ARGV.each { |f| YAML.load_file(f) }' .github/workflows/*.yml`:
  all parse. `git diff --check claude/core-test-codec`: clean.
- Not run here: the Windows, macOS, and Linux packages
  (`dylibbundler` and `pwsh` are not installed; no workflow builds the
  Docker image, so `docker build -f packaging/linux/Dockerfile .` needs a
  manual run), the plugins workspace's metadata call without `--offline`
  on a cold runner, and the first `plugins` job runs on Windows and macOS,
  where VRM's tests and dev-dependencies run for the first time and
  rust-cache starts empty. `cargo install --git https://github.com/mika-f/Celesta celesta-codec-ffmpeg`,
  which the user guide keeps, now finds the crate in a workspace the root
  excludes; it was not run. At the root, `cargo fmt --all --check -- -l`
  lists seven files with existing formatting, none of them changed by this
  layer (in `crates/editor`, `crates/mcp`, `crates/react-bridge`, and
  `crates/source-edit`); root CI runs no format check. The full suites
  are left to CI.

## Layer 4: plugin CI for affected plugins

- A script lists the plugins a change affects, as JSON, from a base
  revision. It has a Vitest test.
  - **Rust.** The local packages a plugin depends on, dev-dependencies
    included, come from its lockfile: `plugins/Cargo.lock`, or the
    plugin's own `Cargo.lock` when it is excluded. A local package has no
    `source`. Each package name maps to its directory through the
    manifests (`cargo metadata --no-deps` of the root workspace, or reading
    `path` entries).
  - **JavaScript.** The pnpm dependency closure of the plugin's `js/`
    package, and the repository paths `turbo.json` names for those
    packages' tasks (`$TURBO_ROOT$/` inputs and `globalDependencies`):
    `@celesta.studio/react`'s codegen reads `crates/project` and
    `crates/composition`. Each plugin's `js/package.json` declares what its
    build really uses, including `@celesta.studio/cli` for the project
    types, so the graph matches the build.
  - **Every plugin** is affected when shared inputs change: the plugin
    workflow, `.github/actions/setup-ffmpeg/`, the shared build script and
    this script, the root `Cargo.toml` (the root crates inherit their
    package fields and dependencies from it when the plugins workspace
    builds them) and
    `Cargo.lock` (the third-party versions the codegen builds with
    `--locked`, and part of rust-cache's key for the root `target/`; a
    source edit in a codegen crate does not touch it, and `turbo.json`'s
    inputs cover that edit; for the codec, which runs no codegen, it only
    changes that cache key), `plugins/Cargo.toml` and `plugins/Cargo.lock`,
    `package.json`, `pnpm-lock.yaml`, `pnpm-workspace.yaml`, `turbo.json`,
    `mise.toml`, and `mise.lock`.
  - On a push to `main` and on `workflow_dispatch`, every plugin is
    affected.
- `ci.yml`'s `vrm-plugin` job becomes a `Plugins (<OS>)` job per operating
  system. Each job runs the script first. With nothing affected it skips
  its remaining steps and passes, so the check names stay stable and can be
  made required later. Otherwise it runs, for the affected plugins:
  `cargo fmt --check`, `cargo clippy --all-targets -D warnings`, and
  `cargo test` with `-p` for each plugin, the JavaScript tests, and the
  plugin-directory build. The VRM job's Windows-target `cargo check` on
  Linux is dropped, since every plugin now builds and tests on Windows
  itself. *(Builder-check call.)*
- No per-plugin declaration of operating systems: every plugin runs on all
  three. A declaration can be added when a plugin truly needs an
  exception.

## Layer 4: what was done

The plan is `docs/superpowers/plans/2026-11-05-plugins-workspace-layer4.md`;
its Decisions give the reasons for the choices below.

- **The script.** `node scripts/plugins/affected.mjs [--base <rev>]
  [--head <rev>] [--github-output]` reads every `plugins/*/plugin.json`
  (not `examples/plugins/`, which stay in the root `Test` job). With
  `--base`, the changed paths are
  `git diff --name-only --no-renames -z <base> <head>` (`<head>` defaults
  to `HEAD`; a moved file's old path counts too); without it every plugin
  is affected. It prints one JSON line,
  `{"plugins": [{id, directory, workspace, cargoPackage, jsPackage, because}, ...]}`,
  sorted by `id`. `workspace` is the Cargo workspace's folder (`plugins`),
  and `workspace`, `cargoPackage`, and `jsPackage` are `null` when absent.
  Each plugin's `because` is the first changed path, in `git diff` order,
  that its own inputs or a shared input cover, so two plugins can name
  different paths; without `--base` it is `"no base revision"`. An error
  prints `affected.mjs: <message>` and exits 1; a bad argument, and
  `--head` without `--base`, exit 2. `--github-output` appends `any`,
  `plugins` (the array as compact JSON), `directories`, `js` (one
  `--filter <jsPackage>` per plugin with one), and `cargo` (one
  `<workspace> -p <package>...` line per workspace, in a
  `cargo<<CARGO_WORKSPACES` block) to `$GITHUB_OUTPUT`, with `\n` line
  endings only.
- **A plugin's inputs** (`inputs.mjs`), as repository-relative `/` paths; a
  folder covers the paths under it, not a sibling sharing its prefix
  (`crates/plugin` does not cover `crates/plugin-host/...`), and a file
  covers only itself:
  - its folder;
  - **Rust**: the folders of the packages without `source` that its
    workspace's lockfile reaches from `cargoPackage` (`lockfile.mjs`, which
    reads `\r\n` lockfiles and tells a local package from a git or registry
    package of the same name and version). Names map to folders through
    `cargo metadata --no-deps` of the root and of each plugin workspace
    (`cargo.mjs`'s `directories`: members and path dependencies). A local
    package with no known folder, or a name two folders claim, is an
    error;
  - **JavaScript**: the folders
    `pnpm --filter "<name>..." ls --depth -1 --json` lists for its
    `js/package.json`, and the paths `turbo.json` gives those packages'
    tasks (below). A `module` without a `js/package.json` is an error;
  - the files its `plugin.json` `licenses` lists.

  On this repository, VRM's Rust closure is `crates/budoux`, `composition`,
  `gpu-renderer`, `media`, `plugin`, `plugin-host`, `remote`, and
  `renderer`; its JavaScript closure is `plugins/vrm/js` and 17 packages
  (`aivis-speech`, `character`, `cli`, `code`, `components`, `debug`,
  `layout`, `lottie`, `math`, `media-utils`, `project`, `react`, `shader`,
  `shapes`, `text`, `transitions`, `voicevox`); `turbo.json` adds
  `crates/project` (and `crates/composition`, already in its Cargo
  closure, and `Cargo.toml`, `Cargo.lock`, and `.cargo/config.toml`,
  already shared inputs). The codec's Rust
  closure is `crates/budoux`, `composition`, `evaluator`, `exporter`,
  `gpu-renderer`, `media`, `plugin`, `plugin-host`, `project`,
  `react-bridge`, `remote`, and `renderer`, plus `LICENSE-APACHE`,
  `LICENSE-GPL-2.0`, `LICENSE-MIT`, and `packaging/GPL-SOURCE-OFFER.md`;
  it has no JavaScript package.
- **`turbo.json`'s paths** (found in the layer's review). The VRM module
  build runs `build:runtime`, whose `@celesta.studio/react` codegen runs
  `cargo test -p celesta-project -p celesta-composition --features codegen`
  and writes the ts-rs types; `turbo.json` declares that as `$TURBO_ROOT$/`
  inputs, and `crates/project` is in no other graph of VRM's. For each
  package in a plugin's pnpm closure, the inputs of `tasks.<script>` and
  `tasks["<package>#<script>"]` for each of its `scripts` that start with
  `$TURBO_ROOT$/` are inputs, with the prefix and a trailing `/**`
  removed, and so are `globalDependencies`. `$TURBO_DEFAULT$`,
  package-relative entries (inside the package's folder), and `!`
  exclusions are skipped, which can only add paths; a deferred-hashing
  entry's `globs` are read the same way. A pattern that maps to no folder
  or file (a glob before its last part, `**` alone, a path outside the
  repository, `$TURBO_ROOT$` elsewhere than the start or in
  `globalDependencies`), a package's own
  `turbo.json`, or a root `turbo.jsonc` is an error. Before the fix, a
  change to `crates/project/src/lib.rs` affected only the codec; now it
  affects both.
- **Shared inputs** (`SHARED_INPUTS`; a test checks each exists):
  `.github/workflows/ci.yml`, `.github/workflows/actions.lock`,
  `.github/actions/setup-ffmpeg`, `scripts/build-ffmpeg-linux.sh`,
  `scripts/plugins`, `scripts/report-skipped-tests.mjs`,
  `.cargo/config.toml`, `Cargo.toml`, `Cargo.lock`, `plugins/Cargo.toml`,
  `plugins/Cargo.lock`, `package.json`, `pnpm-lock.yaml`,
  `pnpm-workspace.yaml`, `turbo.json`, `mise.toml`, and `mise.lock`.
- **`@celesta.studio/cli`.** `plugins/vrm/js/package.json` lists it in
  `devDependencies` (`workspace:*`), since the build checks the module
  against the CLI's project types and bundles with its `tsc` and
  `esbuild`; that makes the whole runtime VRM's closure, which is what its
  build uses.
- **The job.** `Plugins (<OS>)` keeps the `Test` job's three runners,
  `fail-fast: false`, 150 minutes, and bash. It checks out, fetches the
  pull request's base at depth 1 (`pull_request` only; `HEAD` is the merge
  commit onto it, so the two-dot diff is the pull request's change),
  installs Rust 1.99.1 with clippy and rustfmt and mise, and runs
  `affected.mjs` (`--base` the base SHA on a pull request; on a push to
  `main` and on `workflow_dispatch`, no base, so every plugin). `ci.yml`
  has no `merge_group` trigger. Every later step but `wait-all` is gated
  at the step level, so the three check names stay stable: the Cargo
  steps on `cargo != ''`, the JavaScript steps (`pnpm install`, the
  codegen with `--locked` and `build:runtime`, `pnpm $JS test`) on
  `js != ''`, and the rest on `any == 'true'`. FFmpeg is set up for any
  affected plugin, not only the codec: rust-cache keys on
  `FFMPEG_VERSION FFMPEG_FEATURES` and only `main` saves, so a job without
  it would miss main's cache. rust-cache lists `plugins -> target` and
  `. -> target`, with no `key`. Format and Clippy run on Linux only. Tests
  run `cargo test <-p ...> --locked --no-fail-fast -- --show-output` in
  each workspace's folder, through `tee` into a log, and
  `scripts/report-skipped-tests.mjs` reads that log through `libtest.mjs`
  (a JUnit file still when the path ends in `.xml`), naming a test
  `<binary label> <test>`, or the label alone for a `harness = false`
  binary. The plugin directories are then built with
  `node scripts/plugins/build.mjs $DIRECTORIES`.
- **The scripts' tests moved** to the root `Test` job's "Test Celesta
  packages" step (`pnpm --dir scripts/plugins test`), which runs on every
  pull request on all three systems, `inputs.test.mts`'s repository test
  on Windows paths included.
- **Docs.** `HANDOFF.md`'s `scripts/plugins` row and plugins-workspace
  checks say how CI picks the plugins, what a plugin's inputs are, and
  that a push to `main` and a manual run test every plugin.
- **Deviations from the plan.** `--head` without `--base` exits 2 (the
  plan left it open). `because` is per plugin, as above. `turbo.json`'s
  paths are an addition from the review. The scratch pnpm workspace in
  `inputs.test.mts` lives under `scripts/plugins/node_modules/.cache/`,
  inside the repository, where the pnpm `mise.toml` pins is on the path.
- **Known gaps.** Build and test inputs outside both graphs and outside
  `turbo.json` do not affect a plugin: `scripts/resolve-package-manifest.mjs`
  (imported by `packages/cli/scripts/stage-project-types.mjs`),
  `templates/` (read by `packages/cli/scripts/stage-templates.mjs`), and
  `examples/minimal.celesta.json` (read by the codec's `tests/export.rs`).
  Hand-kept lists are a non-goal; a push to `main` tests every plugin and
  catches such a change there. On Windows, the skip report's labels keep
  Cargo's `\` separators.

### Checks (macOS 26.7.1, Apple M5, Metal)

Rust 1.99.1, Node.js 26.13.0, pnpm 12.16.1.

- `pnpm --dir scripts/plugins test`: `tsc -p test`, then 113 passed
  (10 files; `inputs.test.mts` 53).
- `pnpm install --frozen-lockfile`: up to date.
- `node scripts/plugins/affected.mjs`: ffmpeg and vrm, both
  `"no base revision"`. `--base HEAD`: `{"plugins":[]}`, and with
  `--github-output` `any=false`, empty `plugins` list, `directories`, and
  `js`, and an empty `cargo` block. `--base claude/codec-plugin-move`:
  both, `because: ".github/workflows/ci.yml"`; with `--github-output`,
  `any=true`, `directories=plugins/ffmpeg plugins/vrm`,
  `js=--filter celesta-plugin-vrm-js`, and the `cargo` line
  `plugins -p celesta-codec-ffmpeg -p celesta-plugin-vrm`, with `\n`
  endings only. Task 3 also ran single-commit ranges that affect the codec
  alone (`crates/exporter/src/tests.rs`, `crates/react-bridge/src/decode.rs`)
  and the error exits (`GITHUB_OUTPUT` unset and a bad revision exit 1,
  an unknown option and `--head` alone exit 2).
- `affectedPlugins` over the repository's plugins with the one changed
  path `crates/project/src/lib.rs`: before the `turbo.json` fix, ffmpeg
  only; after it, ffmpeg and vrm.
- In `plugins/`,
  `cargo fmt --check -p celesta-codec-ffmpeg -p celesta-plugin-vrm -- -l`:
  clean; with `-v` it lists 12 targets, all under `plugins/`.
- `cargo test --manifest-path plugins/Cargo.toml -p celesta-plugin-vrm --test render --locked -- --show-output`:
  2 passed; `scripts/report-skipped-tests.mjs` on its log: "No tests
  skipped themselves."
- `ruby -ryaml -e 'ARGV.each { |f| YAML.load_file(f) }' .github/workflows/*.yml`:
  all parse. The gate check (every `plugins` step after `affected` except
  `wait-all` has an `if` reading `steps.affected.outputs.`): nothing
  printed, exit 0. `git diff --check claude/codec-plugin-move`: clean.
- Not run here: the `Plugins (<OS>)` jobs themselves, so a pull request
  with nothing affected (the gated steps skipped, the job passing in about
  a minute) is checked only by the gate check; Windows path behaviour
  (`cargo metadata` and `pnpm ls` giving `C:\` paths, the `pnpm` `.cmd`
  shim, a CRLF lockfile outside the unit test), which the `Test` job's
  Windows run of the scripts' tests covers; the plugins' whole suites,
  Clippy, and the JavaScript tests, which CI runs.

## Left to the builder

- The test codec's file format beyond the constraints above, and whether it
  lives in a crate of its own under `crates/` or behind a feature of
  `celesta-media`.
- The build script's name and location, and the affected-plugin script's
  name.
- How the per-OS plugin jobs set up their tools (FFmpeg only when the codec
  is affected, or always).
- Which of the moved exporter tests still need a tolerance.

## Related issues

- mika-f/celesta#645: codec and encoder plugins. This finishes separating
  GPL dependencies from the core.
- mika-f/celesta#669: the VRM crate's location. The plugins workspace is
  one of the options it lists.
- mika-f/celesta#692: the drifted build scripts. Layer 1 closes it.
- mika-f/celesta#652: deferred plugin follow-ups, including bundled plugin
  locations.
