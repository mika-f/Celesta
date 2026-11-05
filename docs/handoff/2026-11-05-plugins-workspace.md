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
  and `/plugins/dist` (`/plugins/vrm/assets` stays).
- **Build script.** `scripts/plugins/` is a private pnpm package
  (`celesta-plugin-scripts`) with `platform.mjs` (a platform's
  `<os>-<arch>` from `rustc --print cfg`'s `target_os` and `target_arch`,
  for the host or a `--target`, which closes mika-f/celesta#692),
  `manifest.mjs` (the manifest from `plugin.json` and what the build made),
  and `build.mjs`. Vitest tests cover both helpers; one runs the real
  `rustc`.
  `node scripts/plugins/build.mjs <plugin directory>... [--out <dir>]
  [--target <triple>] [--skip-js]` replaces `plugins/vrm/build.mjs` and
  `examples/plugins/build.mjs`, which are deleted. Each directory resolves
  against the current directory, and its output goes by default to the
  `dist/` folder beside it: `plugins/vrm` to `plugins/dist/vrm`,
  `examples/plugins/flag` to `examples/plugins/dist/flag`. There is no
  default list of plugins; the samples are built by naming all three.
- **Versions.** A plugin with a `cargoPackage` takes its manifest version
  from that Cargo package, and its `plugin.json` must not have one (vrm,
  flag, and turntable lost theirs). beep-voice, with no program, keeps its
  `version`.
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
  plugins page (en and ja), the Celesta skill's `plugins.md`, `HANDOFF.md`'s
  flag and turntable rows, and `2026-11-03-plugin-vrm.md`'s build, file
  table, and checks give the new commands and paths. `plugins/vrm/js`'s
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

- `cargo fmt --manifest-path plugins/Cargo.toml --check`: clean.
- `cargo clippy --manifest-path plugins/Cargo.toml --workspace --all-targets --locked -- -D warnings`:
  no warnings.
- `cargo test --manifest-path plugins/Cargo.toml --workspace --locked`:
  374 passed, 0 failed (309 unit, 55 `mtoon`, 2 `process`, 2 `render`, 6
  `samples`). `CELESTA_VRM_SAMPLES_DIR` was unset, so the 6 `samples` tests
  passed as skipped. The GPU tests ran on Metal: `mtoon`, `render`, and
  `samples` also pass with `CELESTA_REQUIRE_GPU=1`.
- `pnpm --filter celesta-plugin-vrm-js test`: 17 passed (1 file).
- `pnpm --dir scripts/plugins test`: 17 passed (2 files).
- `cargo metadata --format-version 1 --locked --no-deps` at the root:
  succeeds.

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

## Layer 3: the codec moves to `plugins/ffmpeg`

- `crates/codec-ffmpeg` moves to `plugins/ffmpeg` and joins the plugins
  workspace. The package and program stay `celesta-codec-ffmpeg`. It gets
  a `plugin.json` with `"roles": ["codec"]`, so the shared build script
  makes its plugin directory.
- **One source of the plugin folder.** *(Builder-check call.)* The shared
  build script makes the folder that ships, for every plugin:
  - It takes the manifest's `version` from the plugin's Cargo package
    (`cargoPackage`) instead of `plugin.json`, so the two cannot drift.
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
    package. Each plugin's `js/package.json` declares what its build
    really uses, including `@celesta.studio/cli` for the project types, so
    the graph matches the build.
  - **Every plugin** is affected when shared inputs change: the plugin
    workflow, `.github/actions/setup-ffmpeg/`, the shared build script and
    this script, the root `Cargo.toml` (workspace inheritance) and
    `Cargo.lock` (the codegen `build:runtime` runs), `plugins/Cargo.toml`
    and `plugins/Cargo.lock`, `package.json`, `pnpm-lock.yaml`,
    `pnpm-workspace.yaml`, `turbo.json`, `mise.toml`, and `mise.lock`.
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
