# The codec moves to `plugins/ffmpeg` (layer 3) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `celesta-codec-ffmpeg` lives in `plugins/ffmpeg` as a member of the plugins workspace, the root workspace and its CI have no FFmpeg, and the shared build script makes the codec's plugin folder that packages ship and source builds load through `CELESTA_PLUGIN_PATH`.

**Architecture:** The shared build script learns plugins without a JavaScript module, passes `roles` through, and copies the license files a `plugin.json` lists. The crate moves with `git mv` and keeps its name, program, and Celesta's version, now set in its own `Cargo.toml` and `plugin.json` and bumped with the core by a new script. CI tests the plugins workspace on three operating systems with FFmpeg; packaging, docs, hints, and the bench examples switch from "the program beside Celesta in `target/<profile>`" to "the plugin folder the build script writes".

**Tech Stack:** Rust 1.99.1 (edition 2024), Cargo workspaces, Node.js 26 ES modules with Vitest and `tsc` (JSDoc types), Python 3 (`unittest`), PowerShell 7, GitHub Actions, Docker.

**Spec:** `docs/handoff/2026-11-05-plugins-workspace.md` (Goals, Non-goals, Anti-goals, Constraints, Layer 1 and its "what was done", Layer 2, and "Layer 3: the codec moves to `plugins/ffmpeg`"). Also `docs/handoff/2026-11-05-codec-plugin.md` (the codec, its manifest, the packages) and `docs/handoff/2026-11-04-crates-io-publish.md`.

## Global Constraints

- The stack is managed with `gh stack`: this layer is `claude/codec-in-plugins`, a new branch on `claude/core-test-codec`, created after layer 2's last commit ("Layer 2: what was done") lands; the parallel tasks' worktrees branch from it. Never amend; commit and test steps are separate calls; no push or pull request steps here.
- Edit files with the Edit tool (no sed, perl, or python rewrites). The move is `git mv crates/codec-ffmpeg plugins/ffmpeg`, the deletion `git rm scripts/stage-codec-plugin.mjs`, and lockfiles change only through `cargo`.
- Every test step runs only the affected scope: named tests, one target, `--no-run` builds, clippy for the touched crates. No task runs a crate-wide, package-wide, or workspace-wide suite; CI does.
- Reviews are per task, scoped to that task's diff, plus one comprehensive review of the whole branch before Task 8 records the checks.
- JavaScript tests are Vitest in TypeScript (`.test.mts`, type-checked by `tsc -p test`); Rust tests stay Rust.
- Building the codec needs FFmpeg 8.1.x on `PKG_CONFIG_PATH` (macOS: `export PKG_CONFIG_PATH="$(brew --prefix ffmpeg@8)/lib/pkgconfig"`).
- "`CELESTA_PLUGIN_PATH` is set by hand and never from `mise.toml` or another file every shell picks up. Tests do not depend on that variable." Its entries must be absolute (`plugin_paths` skips relative ones), so every command, hint, and message says `$PWD/plugins/dist` or "the absolute path of `plugins/dist`".
- "`celesta-codec-ffmpeg` keeps Celesta's version (0.9.0 today) and stays publishable on crates.io. Its dependencies on core crates keep the exact pin (`path` plus `version = \"=0.9.0\"`)." `celesta-plugin-vrm` keeps 0.1.0 and `publish = false`; `plugins/Cargo.toml` gets no `[workspace.package]`.
- GPU tests: Linux `CELESTA_REQUIRE_GPU=1`, macOS and Windows `0`.
- User-facing changes update `packages/website/content/en/` and `.../ja/` with the same content, and `skills/celesta/` where it covers them.

## Decisions

- **Executable layout:** one layout for every plugin, the build script's `bin/<os>-<arch>/<binary>`. Packages ship `bin/macos-aarch64/celesta-codec-ffmpeg` (macOS, with the dylibs in the plugin's `Frameworks/` and `-p @executable_path/../../Frameworks/`), `bin\windows-x86_64\celesta-codec-ffmpeg.exe` with the MSVC runtime DLLs beside it, and `bin/linux-x86_64/celesta-codec-ffmpeg` in the Linux image. The manifest's `executable` names it, so the host is unchanged; `scripts/package-macos.py` reads the program's folder from the manifest.
- **Licenses:** `plugin.json` gets an optional `licenses` list of files, relative to the plugin's folder, that the build script copies by file name into `<plugin directory>/licenses/` (replacing what was there). The codec lists `../../LICENSE-GPL-2.0`, `../../LICENSE-MIT`, `../../LICENSE-APACHE`, `../../packaging/GPL-SOURCE-OFFER.md`: one copy of each text in the repository, nothing checked in under `plugins/ffmpeg/licenses/`, and no GPL knowledge in the shared script. The offer names `LICENSE-GPL-2.0`, `LICENSE-MIT`, and `LICENSE-APACHE` as plain code spans instead of `../LICENSE-*` links, so the same bytes read right in `packaging/`, the app's `licenses/`, and the plugin's `licenses/`; the build script never rewrites a file.
- **A plugin without a module:** no `module`/`types` in its manifest, no module build, and no runtime prerequisite unless another plugin in the same run has a module. A plugin with neither `module` nor `cargoPackage`, or with `components` but no `module` (the host refuses that), is refused before anything is built.
- **The codec's manifest** (from `plugins/ffmpeg/plugin.json`, key order as written): `{"id": "ffmpeg", "version": "0.9.0", "protocol": 1, "roles": ["codec"], "executable": {"<os>-<arch>": "bin/<os>-<arch>/celesta-codec-ffmpeg[.exe]"}}`.
- **Versions:** no tool bumps versions today (`67a40069` edited `Cargo.toml` and each `packages/*/package.json` by hand). `node scripts/plugins/bump-version.mjs <X.Y.Z>` bumps the root `workspace.package.version`, every `version = "=X"` pin, and for each folder in `FOLLOWS_CELESTA` (`plugins/ffmpeg`) its Cargo package `version` and `plugin.json` `version`, then refreshes both lockfiles with `cargo update --workspace`. `packages/*/package.json` stay a hand step, named in `HANDOFF.md`. Checks: `repository.test.mts` (each `FOLLOWS_CELESTA` plugin gives the root version) and `package.yml`'s `validate` job (the release tag's version equals `plugins/ffmpeg/plugin.json`'s).
- **CI:** the `vrm-plugin` job becomes `plugins`, "Plugins (<OS>)", a matrix over the root `Test` job's three runners, with setup-ffmpeg as the `Test` job has it today (background with `wait-all` off Windows, foreground on Windows) and `env-vars: FFMPEG_VERSION FFMPEG_FEATURES` on its rust-cache. Format and Clippy run on Linux only, as in the `Test` job. The Linux `cargo check --target x86_64-pc-windows-msvc` step is dropped: Windows builds and tests natively. Tests stay `cargo test`: vrm's `process` target is `harness = false`, which nextest cannot list, so skips on macOS and Windows show only as "skipping" lines in the log until layer 4 rewrites the job.
- **Packaging builds into `plugins/target`:** the build script runs Cargo in the plugins workspace; `package-build.yml`'s rust-cache lists `. -> target` and `plugins -> target`. macOS passes its `build_env` without `CARGO_TARGET_DIR`, plus `--target <triple>` (with `CARGO_BUILD_TARGET` alone Cargo writes `target/<triple>/release` while the script looks in `target/release`); the headerpad `RUSTFLAGS` must reach the codec for dylibbundler.
- **License inventory:** both packagers merge `cargo metadata` of `plugins/Cargo.toml` into the root's (deduplicated by package `id`), so `ez-ffmpeg`, `ffmpeg-sys-next`, and the codec stay in `rust-packages.json` and `licenses/rust/`. VRM's dependencies come along, as the root's dev-only ones already do.
- **perf.yml:** no workload probes or decodes media (`nebula.tsx` and `spectra/film.tsx`, and `celesta-bench` links no codec), so both jobs drop setup-ffmpeg, its rust-cache `env-vars`, and `.github/actions/setup-ffmpeg/**` from `push.paths`. A `workflow_dispatch` base from before the codec plugin would need FFmpeg again; accepted.
- **Root tests that spawn Celesta without a codec:** `inspect.rs` and `cli_formats.rs` children decode nothing (the inspect entry's `voice.wav` does not exist, so its probe fails today as well), and `crates/mcp/tests/integration/server.rs`'s `a.mp4` is a usage error. Expected: no change. Task 2 proves it with no codec anywhere; a test that then fails gets the test codec's plugin folder as `stills.rs`'s `react_export` does.
- **Bench examples:** both take `InstalledPlugins::find_reporting(&[]).codec()` and, without one, exit 1 with `no codec plugin found: build one with node scripts/plugins/build.mjs plugins/ffmpeg and add the absolute path of plugins/dist to CELESTA_PLUGIN_PATH`.

## Inventory (reference → task)

| Reference | Use today | Task |
| --- | --- | --- |
| `crates/codec-ffmpeg/**` | the codec crate | 2 (moved to `plugins/ffmpeg`); 3 comments its `version` |
| `Cargo.toml` (root) | member, `celesta-codec-ffmpeg` and `ez-ffmpeg` workspace deps | 2 |
| `Cargo.lock`, `plugins/Cargo.toml`, `plugins/Cargo.lock` | FFmpeg in the root lock; members `["vrm"]` | 2 |
| `crates/exporter/Cargo.toml`, `crates/react-bridge/Cargo.toml` | dev-deps on the codec | layer 2 Task 7 removes them; 2 checks they are gone |
| `crates/plugin-host/src/manifest.rs` | `CODEC_PROGRAM`, `sibling_codec` docs ("as `cargo build` … leave it"), `the_repositorys_plugins_pass_the_manifest_checks` | 2 |
| `crates/exporter/tests/integration/inspect.rs`, `cli_formats.rs`, `crates/mcp/tests/integration/server.rs` | children with no `--plugin-dir` | 2 (verify, no change expected) |
| `scripts/plugins/{build,manifest}.mjs`, `test/manifest.test.mts` | module required, no roles or licenses | 1 |
| `scripts/plugins/test/repository.test.mts` | version checks | 3 |
| `scripts/stage-codec-plugin.mjs` | lays out the codec folder | 5 (deleted) |
| `.github/workflows/ci.yml` | `Test`: setup-ffmpeg ×2, `wait-all`, `FFMPEG_*` env-vars; `vrm-plugin` job | 4 |
| `.github/workflows/perf.yml` | setup-ffmpeg ×2, `FFMPEG_*` env-vars, `setup-ffmpeg/**` path | 4 |
| `.github/workflows/package-build.yml` | setup-ffmpeg, `FFMPEG_*` env-vars (both stay); rust-cache of `.` only | 5 |
| `.github/workflows/package.yml` | `validate` job; setup-ffmpeg comment (stays) | 3 |
| `.github/actions/setup-ffmpeg/action.yml`, `ffmpeg-windows.yml`, `renovate.json`, `scripts/build-ffmpeg-linux.sh`, `packaging/linux/README.md`, `scripts/package-macos.py`'s `FFMPEG_FORMULA` | FFmpeg setup for the codec | unchanged (the plugin job and packaging use them) |
| `packaging/linux/Dockerfile`, `.dockerignore` | `cargo build -p celesta-codec-ffmpeg`, `stage-codec-plugin.mjs`; `target` ignored at the root only | 5 |
| `scripts/package-macos.py`, `scripts/test_package_macos.py` | builds and stages the codec; `bin/` assumed by `validate_dependencies` | 5 |
| `scripts/package-windows.ps1`, `scripts/test-package.mjs` | builds and stages the codec; checks its folder | 5 |
| `packaging/GPL-SOURCE-OFFER.md`, `packaging/macos/README.md`, `packaging/windows/README.md`, `packaging/windows/README.txt` | `crates/codec-ffmpeg`, `../LICENSE-*` links; `bin/celesta-codec-ffmpeg` | 5 |
| `crates/exporter/src/report.rs` (`NO_CODEC_HINT`), `crates/editor/src/plugins.rs` (`missing_codec_detail`) | name the codec, not the source build | 6 |
| `crates/exporter/examples/preview-bench.rs`, `crates/react-bridge/examples/audio-bench.rs` | codec in `target/<profile>` | 6 |
| `HANDOFF.md` (development setup, workspace map, architecture boundaries, examples and validation) | `cargo build -p celesta-codec-ffmpeg`, `crates/codec-ffmpeg`, sibling in `target/<profile>` | 7 |
| `README.md` ("Develop Celesta") | `crates/codec-ffmpeg`, `cargo build -p celesta-codec-ffmpeg --release`, `target/release` | 7 |
| `packages/website/content/{en,ja}/docs/05-(more)/03-build-from-source.md`, `04-troubleshooting.md`, `05-plugins.md` | the cargo build, the sibling, `stage-codec-plugin.mjs` | 7 |
| `skills/celesta/references/export.md`, `plugins.md`, `verify.md` | source build's codec beside the executables | 7 |
| `docs/handoff/2026-11-04-crates-io-publish.md` | root publish needs FFmpeg; 18 crates | 7 |
| `docs/handoff/2026-11-05-plugins-workspace.md` | the spec | 8 ("Layer 3: what was done") |
| `docs/handoff/2026-11-05-codec-plugin.md`, `docs/superpowers/**`, other dated notes | historical | unchanged; 8 lists them |
| `examples/versus/README.md`, `examples/versus/scenes/Loop.tsx` (`FFMPEG_DIR`) | another project's FFmpeg | unchanged |

## Review Focus

- **A stale `celesta-codec-ffmpeg` in `target/<profile>`** (from before the move, locally or in a restored CI cache) lets the sibling fallback hide a source build with no codec. Task 2 deletes them before its checks and proves the root tests need none; Task 6 runs the bench with none.
- **A relative `CELESTA_PLUGIN_PATH`** (`plugins/dist`) is skipped without a word in most tools. Every instruction says absolute; Task 6's tests assert the hint and the editor text say "absolute path".
- **The macOS package's codec loading its dylibs** from `bin/macos-<arch>/`: the dylibbundler prefix and `validate_dependencies` must agree. Task 5 adds a plugin-layout test to `test_package_macos.py`.
- **The packages' license inventory losing the codec's crates** after the move. Task 5 merges the plugins workspace's metadata and `test-package.mjs` asserts `ez-ffmpeg` is in `rust-packages.json`.
- **A release bump that misses the codec's own `version`** (its pins fail to resolve, but its `version` would not). Task 3's repository test, the CLI's refusal, and the release check catch it.

## Parallel groups

1. Task 1, then Task 2 (Task 2 makes every manifest and lockfile change, and the plugin-host test change).
2. Tasks 3, 4, 5, 6, and 7 in parallel, each in its own worktree. Owners: Task 3 `scripts/plugins/{version,bump-version}.mjs`, `test/{version,repository}.test.mts`, `plugins/ffmpeg/Cargo.toml` (comment only), `package.yml`; Task 4 `ci.yml`, `perf.yml`; Task 5 `package-build.yml`, `packaging/**`, `.dockerignore`, the packaging scripts and their tests, `stage-codec-plugin.mjs`; Task 6 `report.rs`, `plugins.rs`, the two examples; Task 7 `HANDOFF.md`, `README.md`, the website pages, `skills/celesta/`, the publish note. Merge in task order.
3. Comprehensive review of the branch, then Task 8.

---

### Task 1: The build script builds a plugin without a module, with roles and license files

**Files:**
- Create: `scripts/plugins/licenses.mjs`, `scripts/plugins/test/licenses.test.mts`
- Modify: `scripts/plugins/manifest.mjs`, `scripts/plugins/build.mjs` (header comment too), `scripts/plugins/test/manifest.test.mts`, `scripts/plugins/package.json` (description: the build script's helpers, not only the platform and manifest)

**Interfaces:**
- Produces, in `manifest.mjs`: `PluginSource` gains `module?: string`, `roles?: string[]`, `licenses?: string[]`; `PluginManifest` gains `roles?: string[]`, and `module`/`types` become optional. `export function checkPluginSource(source: PluginSource, sourcePath: string): void` throws `${sourcePath}: components need a module to import them from` and `${sourcePath}: it has neither a module nor a cargoPackage to build`. `pluginManifest` calls it and writes keys in the order `id, version, protocol, roles?, executable?, module?, types?, components?`.
- Produces, in `licenses.mjs`: `export function licenseFiles(source: string, plugin: PluginSource, sourcePath: string): { from: string, name: string }[]`: each `plugin.licenses` entry resolved against `source`, in order, `name` its base name; `[]` without `licenses`. Throws `${sourcePath}: license file ${entry} is not a file` (missing or a directory) and `${sourcePath}: two license files are named ${name}`.

- [ ] **Step 1: Write the failing tests.**
  - `manifest.test.mts` `writes a codec plugin's roles and no module or types`: `pluginManifest({ id: 'ffmpeg', version: '0.9.0', cargoPackage: 'celesta-codec-ffmpeg', roles: ['codec'], licenses: ['../../LICENSE-MIT'] }, { cargoVersion: '0.9.0', executable: { 'macos-aarch64': 'bin/macos-aarch64/celesta-codec-ffmpeg' } }, sourcePath)` equals `{ id: 'ffmpeg', version: '0.9.0', protocol: 1, roles: ['codec'], executable: { 'macos-aarch64': 'bin/macos-aarch64/celesta-codec-ffmpeg' } }` with keys `['id', 'version', 'protocol', 'roles', 'executable']` (no `licenses` in the manifest).
  - `manifest.test.mts` `refuses components without a module` and `refuses a plugin with nothing to build`: `checkPluginSource` with `{ id: 'x', version: '0.1.0', cargoPackage: 'p', components: [component] }` and with `{ id: 'x', version: '0.1.0' }` throw the two messages above; the existing vrm and beep-voice cases still pass unchanged.
  - `licenses.test.mts`, in a `mkdtempSync` tree `repo/LICENSE-MIT`, `repo/plugins/p/NOTICE`, `repo/plugins/p/plugin.json`: `licenseFiles(repo/plugins/p, { …, licenses: ['../../LICENSE-MIT', 'NOTICE'] }, path)` is `[{ from: repo/LICENSE-MIT, name: 'LICENSE-MIT' }, { from: repo/plugins/p/NOTICE, name: 'NOTICE' }]`; without `licenses` it is `[]`; `['missing']` and `['.']` throw "is not a file" naming plugin.json; `['../../LICENSE-MIT', 'LICENSE-MIT']` with both present throws "two license files are named LICENSE-MIT".
- [ ] **Step 2:** Run `pnpm --dir scripts/plugins exec vitest run test/manifest.test.mts test/licenses.test.mts`. The new cases fail (`checkPluginSource` and `licenses.mjs` do not exist; `pluginManifest` writes `module`).
- [ ] **Step 3:** Implement `checkPluginSource`, the manifest changes, and `licenses.mjs`. Restructure `build.mjs`: read and check every `plugin.json` first (`checkPluginSource`, the version-is-present check, `licenseFiles`), so a bad plugin fails before any Cargo build; require the runtime's `project-types` only when `--skip-js` is off and some plugin has a `module`; build the module only for a plugin with one; after the module, remove `<out>/licenses` and copy each license file into it when the list is non-empty.
- [ ] **Step 4:** Run `pnpm --dir scripts/plugins exec tsc -p test` and `pnpm --dir scripts/plugins exec vitest run test/manifest.test.mts test/licenses.test.mts`. Both pass.
- [ ] **Step 5:** After `pnpm install && pnpm run build:runtime`, run `node scripts/plugins/build.mjs examples/plugins/beep-voice examples/plugins/flag --out <scratch>/a`. Both manifests are as before (no `roles`, `module` and `types` present; flag has `bin/macos-aarch64/celesta-plugin-flag`) and neither folder has `licenses/`. Then a scratch folder holding only `plugin.json` `{"id": "none", "version": "0.1.0"}`: `node scripts/plugins/build.mjs <it> --out <scratch>/b` exits 1 with "neither a module nor a cargoPackage" and writes nothing.
- [ ] **Step 6:** Commit: `build(plugins): build plugins without a module, with roles and license files`.

### Task 2: Move the codec into the plugins workspace

**Files:**
- Move: `crates/codec-ffmpeg` → `plugins/ffmpeg` (`git mv`)
- Create: `plugins/ffmpeg/plugin.json`
- Modify: `plugins/ffmpeg/Cargo.toml`, `plugins/Cargo.toml` (`members = ["ffmpeg", "vrm"]`), `Cargo.toml` (root), `Cargo.lock`, `plugins/Cargo.lock`
- Modify: `crates/plugin-host/src/manifest.rs` (`CODEC_PROGRAM` and `sibling_codec` docs: "as `cargo install` leaves it"; the repository test)

**Interfaces:**
- Consumes: Task 1's `roles`, `licenses`, and module-less build.
- Produces: `plugins/ffmpeg/plugin.json` = `{"id": "ffmpeg", "version": "0.9.0", "cargoPackage": "celesta-codec-ffmpeg", "roles": ["codec"], "licenses": ["../../LICENSE-GPL-2.0", "../../LICENSE-MIT", "../../LICENSE-APACHE", "../../packaging/GPL-SOURCE-OFFER.md"]}`; `node scripts/plugins/build.mjs plugins/ffmpeg [--out <dir>] [--target <triple>]` writes `<out>/ffmpeg` (default `plugins/dist/ffmpeg`).

- [ ] **Step 1:** Record the root lock's `ez-ffmpeg` (0.19.3) and `ffmpeg-sys-next` (8.1.0) versions. `git mv crates/codec-ffmpeg plugins/ffmpeg`. In `plugins/ffmpeg/Cargo.toml` replace each `*.workspace = true`: `version = "0.9.0"`, `edition = "2024"`, `license = "MIT OR Apache-2.0"`, `repository = "https://github.com/mika-f/celesta"`, `rust-version = "1.89"`; `celesta-composition`, `celesta-media` (dependencies) and `celesta-exporter`, `celesta-gpu-renderer`, `celesta-project` (dev-dependencies) as `{ path = "../../crates/<dir>", version = "=0.9.0" }`; `ez-ffmpeg = "0.19"` and, for `cfg(not(target_os = "macos"))`, `ez-ffmpeg = { version = "0.19", features = ["static"] }`; `strum = { version = "0.28.0", features = ["derive"] }`. Write `plugin.json`. Root `Cargo.toml`: drop the member and the `celesta-codec-ffmpeg` and `ez-ffmpeg` entries. `plugins/Cargo.toml`: members.
- [ ] **Step 2:** Run `cargo update --workspace` at the root and `cargo update --manifest-path plugins/Cargo.toml --workspace`. If `plugins/Cargo.lock`'s `ez-ffmpeg` or `ffmpeg-sys-next` differs from the recorded version, pin it with `cargo update --manifest-path plugins/Cargo.toml -p <name> --precise <version>`.
- [ ] **Step 3:** Check the root is free of FFmpeg: `git grep -n -i ffmpeg -- Cargo.toml Cargo.lock` matches nothing; `git grep -n -E "celesta-codec-ffmpeg|ez-ffmpeg" -- 'crates/*/Cargo.toml'` matches nothing; `cargo metadata --format-version 1 --locked --no-deps > /dev/null` exits 0.
- [ ] **Step 4: Update the plugin-host test** `the_repositorys_plugins_pass_the_manifest_checks` to write the manifest as the build script does: `module` and `types` only when `plugin.json` has `module`, and `roles` when it has them. Add: the found `ffmpeg` entry `has_role(PluginRole::Codec)` and has no module. Run `cargo test -p celesta-plugin-host --lib --locked the_repositorys_plugins_pass_the_manifest_checks`. It passes.
- [ ] **Step 5:** Run `cargo test --manifest-path plugins/Cargo.toml -p celesta-codec-ffmpeg --locked --no-run` (every codec target builds), then `cargo test --manifest-path plugins/Cargo.toml -p celesta-codec-ffmpeg --locked --test process` and `… --test export` (the eight FFmpeg-contract tests through the exporter, resolved anew in `plugins/Cargo.lock`; GPU on Metal). Both pass. Run `cargo fmt --check` in `plugins/` and `cargo clippy --manifest-path plugins/Cargo.toml -p celesta-codec-ffmpeg --all-targets --locked -- -D warnings`; if either complains about the moved code, fix it in its own commit (`style(codec-ffmpeg): …`).
- [ ] **Step 6:** Run `node scripts/plugins/build.mjs plugins/ffmpeg`. `plugins/dist/ffmpeg/celesta-plugin.json` is the Decisions' manifest with `macos-aarch64` here, `bin/macos-aarch64/celesta-codec-ffmpeg` runs, and `licenses/` holds the four files, byte-equal to their sources. Run `pnpm --dir scripts/plugins exec vitest run test/repository.test.mts`. It passes for `plugins/ffmpeg` and `plugins/vrm`.
- [ ] **Step 7: No codec anywhere.** Delete `target/{debug,release}/celesta-codec-ffmpeg` (and any `.exe`/`.d` beside them). With `CELESTA_PLUGIN_PATH` unset and no codec plugin in the user's plugin folder or settings: `cargo test -p celesta-exporter --test integration inspect::` and `cargo test -p celesta-exporter --test integration cli_formats::` pass unchanged (if one fails for want of a codec, give it a `TestCodecPlugin` folder as `stills.rs`'s `react_export` does, and say so in Task 8); `cargo run -p celesta-exporter -- --react packages/cli/examples/title.tsx <scratch>/a.mp4 --json` reports `no_codec`. Then `CELESTA_PLUGIN_PATH="$PWD/plugins/dist" cargo run -p celesta-exporter -- --react packages/cli/examples/title.tsx <scratch>/b.mp4` writes an MP4 (`ftyp` at bytes 4–8).
- [ ] **Step 8:** Commit: `build: move the FFmpeg codec into the plugins workspace`.

### Task 3: Bump Celesta's version, the codec's with it

**Files:**
- Create: `scripts/plugins/version.mjs`, `scripts/plugins/bump-version.mjs`, `scripts/plugins/test/version.test.mts`
- Modify: `scripts/plugins/test/repository.test.mts`, `plugins/ffmpeg/Cargo.toml` (a comment over `version` and over the pins: Celesta's version, given by `plugin.json` too, bumped by `node scripts/plugins/bump-version.mjs`), `.github/workflows/package.yml`

**Interfaces:**
- Produces, in `version.mjs`: `export const FOLLOWS_CELESTA = ['plugins/ffmpeg']` (relative to the repository root); `export function workspaceVersion(cargoToml: string): string` (the `version` of `[workspace.package]`; throws `no [workspace.package] version`); `export function bumpCargoToml(text: string, from: string, to: string, path: string): string` (the `version = "<from>"` line of `[workspace.package]` or `[package]`, and every `version = "=<from>"`; throws `${path}: version ${found} is not Celesta's ${from}` when that table's version differs, and `${path}: nothing to bump` when nothing matched); `export function bumpPluginJson(text: string, from: string, to: string, path: string): string` (replaces the one `"version": "<from>"`, leaving every other byte; throws `${path}: version ${found} is not Celesta's ${from}`).
- Produces: `node scripts/plugins/bump-version.mjs <X.Y.Z>`: refuses a version not matching `^\d+\.\d+\.\d+$` or equal to the current one; rewrites `Cargo.toml` and each `FOLLOWS_CELESTA` folder's `Cargo.toml` and `plugin.json`, all checked before any is written; then `cargo update --workspace` at the root and `cargo update --manifest-path plugins/Cargo.toml --workspace`; prints the files it changed and that `packages/*/package.json` are bumped by hand.

- [ ] **Step 1: Write the failing tests** in `version.test.mts` with inline fixtures:
  - `workspaceVersion` reads `0.9.0` from a root fixture whose `[workspace.dependencies]` also has `foo = { path = "x", version = "=0.9.0" }`, and throws on `[package]\nversion = "0.9.0"`.
  - `bumpCargoToml(root fixture, '0.9.0', '0.10.0', 'Cargo.toml')` changes the `[workspace.package]` version and both `=0.9.0` pins, and leaves `serde = { version = "1.0" }`, `bar = { version = "0.9.0" }`, and `[workspace.package]`'s `rust-version = "1.89"` byte for byte.
  - `bumpCargoToml(codec fixture)` changes `[package]`'s `version = "0.9.0"` and the `=0.9.0` pins in `[dependencies]` and `[dev-dependencies]`; with `[package]` at `0.8.0` it throws `plugins/ffmpeg/Cargo.toml: version 0.8.0 is not Celesta's 0.9.0`.
  - `bumpPluginJson` of `{\n  "id": "ffmpeg",\n  "version": "0.9.0",\n  "roles": ["codec"]\n}\n` gives the same text with `0.10.0`; at `0.8.0` it throws naming the file.
  - `repository.test.mts` `%s gives Celesta's version`: for each `FOLLOWS_CELESTA` folder, `plugin.json`'s `version` equals `workspaceVersion(<root Cargo.toml>)`.
- [ ] **Step 2:** Run `pnpm --dir scripts/plugins exec vitest run test/version.test.mts test/repository.test.mts`. The new cases fail: `version.mjs` does not exist.
- [ ] **Step 3:** Implement `version.mjs` and `bump-version.mjs` (a line-based scan for the table headers; no TOML dependency). In `package.yml`'s "Check release tag matches the workspace version" step, also fail when `node -p "require('./plugins/ffmpeg/plugin.json').version"` differs from the workspace version, with "plugins/ffmpeg/plugin.json gives <v>, not the workspace version <w>; run node scripts/plugins/bump-version.mjs".
- [ ] **Step 4:** Run `pnpm --dir scripts/plugins exec tsc -p test` and `pnpm --dir scripts/plugins exec vitest run test/version.test.mts test/repository.test.mts`. They pass. Run `ruby -ryaml -e 'YAML.load_file(ARGV[0])' .github/workflows/package.yml`. It parses.
- [ ] **Step 5:** On a clean tree run `node scripts/plugins/bump-version.mjs 0.9.1`. `git diff --stat` lists exactly `Cargo.toml`, `Cargo.lock`, `plugins/Cargo.lock`, `plugins/ffmpeg/Cargo.toml`, `plugins/ffmpeg/plugin.json`; `cargo metadata --format-version 1 --locked --no-deps > /dev/null` passes at the root and with `--manifest-path plugins/Cargo.toml`; the repository test passes. Restore with `git restore` on those five files. Then hand-edit `plugins/ffmpeg/plugin.json` and the codec's `version` to `0.9.1`: the repository test fails for `plugins/ffmpeg`, and the CLI refuses to bump; restore them.
- [ ] **Step 6:** Commit: `build: bump Celesta's version and the codec plugin's together`.

### Task 4: CI without FFmpeg in the root, and the plugins on three systems

**Files:**
- Modify: `.github/workflows/ci.yml`, `.github/workflows/perf.yml`

- [ ] **Step 1:** `ci.yml`'s `test` job: remove both setup-ffmpeg steps and their comments, the `wait-all` step ("FFmpeg is the only background step so far"), `env-vars: FFMPEG_VERSION FFMPEG_FEATURES` from rust-cache, and the timeout comment about building FFmpeg with vcpkg. Keep `packages: read` (the plugin job's setup-ffmpeg downloads Windows' FFmpeg from GitHub Packages).
- [ ] **Step 2:** Replace the `vrm-plugin` job with `plugins`, `name: Plugins (${{ matrix.name }})`, the `test` job's matrix and `fail-fast: false`, `timeout-minutes: 150`, bash defaults, and steps in this order: checkout; Linux deps (`libfontconfig-dev mesa-vulkan-drivers`, `if: runner.os == 'Linux'`); setup-ffmpeg `background: true` off Windows (with `id: ffmpeg`) and in the foreground on Windows, with the `test` job's comments; isolate the toolchain; Rust 1.99.1 with `clippy, rustfmt` (no Windows target); mise; `pnpm install --frozen-lockfile`; `wait-all`; rust-cache with `workspaces: plugins -> target` and `. -> target`, `env-vars: FFMPEG_VERSION FFMPEG_FEATURES`, `save-if` main; codegen `--locked` and `build:runtime`; `pnpm --filter celesta-plugin-vrm-js test`; `pnpm --dir scripts/plugins test`; Format (`working-directory: plugins`, `cargo fmt --check`) and Clippy (`--workspace --all-targets --locked -- -D warnings`) with `if: runner.os == 'Linux'`; Test with `CELESTA_REQUIRE_GPU: ${{ runner.os == 'Linux' && '1' || '0' }}` running `cargo test --manifest-path plugins/Cargo.toml --workspace --locked`; `node scripts/plugins/build.mjs plugins/vrm plugins/ffmpeg`. The job's comment says it tests every first-party plugin on every system until layer 4 picks the affected ones, and why the Windows-target check is gone.
- [ ] **Step 3:** `perf.yml`: remove both setup-ffmpeg steps, both `env-vars` lines, and `.github/actions/setup-ffmpeg/**` from `push.paths`.
- [ ] **Step 4:** Run `ruby -ryaml -e 'ARGV.each { |f| YAML.load_file(f) }' .github/workflows/ci.yml .github/workflows/perf.yml`. Both parse. `git grep -n -E "setup-ffmpeg|FFMPEG_" -- .github/workflows/ci.yml` matches only the `plugins` job and the `packages: read` comment; `.github/workflows/perf.yml` matches nothing.
- [ ] **Step 5:** Commit: `ci: drop FFmpeg from the root tests and test the plugins on every system`.

### Task 5: Packages build the codec with the shared script

**Files:**
- Modify: `.github/workflows/package-build.yml` (both jobs' rust-cache: `workspaces: |` `. -> target` and `plugins -> target`), `.dockerignore` (`plugins/target`)
- Modify: `packaging/linux/Dockerfile`, `scripts/package-macos.py`, `scripts/test_package_macos.py`, `scripts/package-windows.ps1`, `scripts/test-package.mjs`
- Modify: `packaging/GPL-SOURCE-OFFER.md`, `packaging/macos/README.md`, `packaging/windows/README.md`, `packaging/windows/README.txt`
- Delete: `scripts/stage-codec-plugin.mjs` (`git rm`)

**Interfaces:**
- Consumes: Task 2's `node scripts/plugins/build.mjs plugins/ffmpeg --out <dir> [--target <triple>]`.
- Produces, in `package-macos.py`: `plugin_program(plugin: Path) -> Path` (the one program `<plugin>/celesta-plugin.json`'s `executable` names; `RuntimeError` unless it names exactly one).

- [ ] **Step 1: Write the failing test** in `test_package_macos.py`, `test_plugin_libraries_resolve_from_the_plugin_program_folder`: under `Contents/PlugIns/ffmpeg` write `celesta-plugin.json` with `{"executable": {"macos-aarch64": "bin/macos-aarch64/celesta-codec-ffmpeg"}}`, `Frameworks/libavcodec.62.dylib`, `Frameworks/libx264.dylib`; `validate_dependencies` passes for the program with `@executable_path/../../Frameworks/libavcodec.62.dylib` and for `libavcodec.62.dylib` with `@executable_path/../../Frameworks/libx264.dylib`, and raises for the dylib with `@executable_path/../Frameworks/libx264.dylib`; `plugin_program` returns `…/bin/macos-aarch64/celesta-codec-ffmpeg`.
- [ ] **Step 2:** Run `python3 scripts/test_package_macos.py`. The new test fails.
- [ ] **Step 3: macOS.** Define `build_env` before `if not args.skip_build:`; drop `-p celesta-codec-ffmpeg` from its cargo build. Build the codec with `run(helpers / "node", ROOT / "scripts/plugins/build.mjs", ROOT / "plugins/ffmpeg", "--out", contents / "PlugIns", "--target", target, env=codec_env)`, `codec_env` being `build_env` without `CARGO_TARGET_DIR` (with `--skip-build` too: Cargo does nothing when the codec is current). dylibbundler takes `-x plugin_program(codec)`, `-d codec / "Frameworks"`, and `-p @executable_path/<relative path from the program's folder to Frameworks>/`; `required` names `plugin_program(codec)`. `validate_dependencies` resolves a plugin dylib's `@executable_path` against `plugin_program(<plugin>).parent`. `collect_licenses` merges `cargo metadata --manifest-path plugins/Cargo.toml` (same flags) into the root's packages, deduplicated by `id`.
- [ ] **Step 4: Windows.** Drop `-p celesta-codec-ffmpeg` from the cargo build; build the codec with `Invoke-Checked "$runtime/node.exe" @("$PSScriptRoot/plugins/build.mjs", "$root/plugins/ffmpeg", '--out', "$package/plugins")`; copy the CRT DLLs into the folder of the manifest's `executable.'windows-x86_64'`; merge `cargo metadata --manifest-path plugins/Cargo.toml` into `$metadata.packages` (`Sort-Object id -Unique`) before the inventory and the license texts.
- [ ] **Step 5: Linux.** The Dockerfile's cargo build drops `-p celesta-codec-ffmpeg`; the layout step runs `node scripts/plugins/build.mjs plugins/ffmpeg --out /out/plugins` in place of `stage-codec-plugin.mjs`. Add `plugins/target` to `.dockerignore`.
- [ ] **Step 6: `test-package.mjs`.** Read the codec's `celesta-plugin.json`: `roles` includes `codec`, `executable` names exactly one file and it exists, and `licenses/` has `LICENSE-GPL-2.0` and `GPL-SOURCE-OFFER.md`. The package's `licenses/rust-packages.json` (`Contents/Resources/licenses/` on macOS) names `ez-ffmpeg` and `celesta-codec-ffmpeg`.
- [ ] **Step 7: Texts.** `GPL-SOURCE-OFFER.md`: `crates/codec-ffmpeg` → `plugins/ffmpeg`; the three `../LICENSE-*` links become plain code spans ("`LICENSE-MIT` and `LICENSE-APACHE`, at the repository's root and beside this file in a package"). The packaging READMEs give the `bin/<os>-<arch>/` paths and say the shared build script lays the codec out. `git rm scripts/stage-codec-plugin.mjs`.
- [ ] **Step 8:** Run `python3 scripts/test_package_macos.py` (passes), `node --check scripts/test-package.mjs`, `python3 -c "import ast,sys; ast.parse(open(sys.argv[1]).read())" scripts/package-macos.py`, `ruby -ryaml -e 'YAML.load_file(ARGV[0])' .github/workflows/package-build.yml`, and `git grep -n stage-codec-plugin -- scripts packaging .github` (nothing). With `dylibbundler` installed, run `python3 scripts/package-macos.py --version 0.0.0` (it ends with `test-package.mjs`); without it, record it as not run (`package-build.yml` runs only on pushes to `main`). `pwsh` and Docker are not run here.
- [ ] **Step 9:** Commit: `build(packaging): build the codec plugin with the shared build script`.

### Task 6: Tell a source build what is missing

**Files:**
- Modify: `crates/exporter/src/report.rs` (`NO_CODEC_HINT` and its test), `crates/editor/src/plugins.rs` (`missing_codec_detail`'s "none found" text and `a_missing_codec_is_a_problem`)
- Modify: `crates/exporter/examples/preview-bench.rs`, `crates/react-bridge/examples/audio-bench.rs`

- [ ] **Step 1: Write the failing assertions.** `media_nothing_could_decode_is_reported_as_no_codec`: the hint still contains `celesta-codec-ffmpeg`, and contains `node scripts/plugins/build.mjs plugins/ffmpeg` and `absolute path of plugins/dist`. `a_missing_codec_is_a_problem`: the "none found" detail contains the same two strings.
- [ ] **Step 2:** Run `cargo test -p celesta-exporter --lib report::tests::media_nothing_could_decode_is_reported_as_no_codec` and `cargo test -p celesta-editor --lib plugins::tests::a_missing_codec_is_a_problem`. Both fail.
- [ ] **Step 3:** `NO_CODEC_HINT`: "install Celesta's codec plugin (celesta-codec-ffmpeg) beside Celesta or in a plugin folder; in a source checkout, build it with node scripts/plugins/build.mjs plugins/ffmpeg and add the absolute path of plugins/dist to CELESTA_PLUGIN_PATH". Editor: "No codec plugin was found, {consequence}. Install Celesta's {CODEC_PROGRAM} plugin, or reinstall Celesta. In a source checkout, build it with node scripts/plugins/build.mjs plugins/ffmpeg and add the absolute path of plugins/dist to CELESTA_PLUGIN_PATH before starting Celesta."
- [ ] **Step 4:** Both examples per the Decisions: `preview-bench` replaces its profile-folder lookup and `assert!` with `find_reporting(&[]).codec()` and the message; `audio-bench` drops its profile-folder `or_else`. Their headers say to build with `node scripts/plugins/build.mjs plugins/ffmpeg` and run with `CELESTA_PLUGIN_PATH="$PWD/plugins/dist"`.
- [ ] **Step 5:** Run the two tests of Step 2 (pass), `cargo build -p celesta-exporter --example preview-bench`, and `cargo build -p celesta-react-bridge --example audio-bench`. With `CELESTA_PLUGIN_PATH` unset and no codec in the user's plugin folder, `cargo run -p celesta-exporter --example preview-bench -- packages/cli/examples/title.tsx 1` exits 1 with the message.
- [ ] **Step 6:** Commit: `fix(exporter, editor): say how a source build gets the codec plugin`.

### Task 7: Docs for building and finding the codec

**Files:**
- Modify: `HANDOFF.md`, `README.md`, `docs/handoff/2026-11-04-crates-io-publish.md`
- Modify: `packages/website/content/{en,ja}/docs/05-(more)/03-build-from-source.md`, `04-troubleshooting.md`, `05-plugins.md`
- Modify: `skills/celesta/references/export.md`, `plugins.md`, `verify.md`

Edit by section, not line number (layer 2's last task edits `HANDOFF.md` too). One source-build recipe everywhere:

```sh
node scripts/plugins/build.mjs plugins/ffmpeg
export CELESTA_PLUGIN_PATH="$PWD/plugins/dist"   # PowerShell: $env:CELESTA_PLUGIN_PATH = "$PWD\plugins\dist"
```

- [ ] **Step 1: `HANDOFF.md`.** Development setup: the FFmpeg 8.1.x libraries are needed only for the codec plugin, `plugins/ffmpeg`, and the plugins workspace's tests; the root workspace links no FFmpeg. Workspace map: the `celesta-codec-ffmpeg` row says `plugins/ffmpeg`, a member of the plugins workspace with Celesta's version, built into a plugin directory by `scripts/plugins/build.mjs`; the `scripts/plugins` row adds `licenses.mjs`, `version.mjs`, and `bump-version.mjs`. Examples and validation: the recipe above in place of `cargo build -p celesta-codec-ffmpeg` and the sibling paragraph (the sibling fallback serves `cargo install` only); the plugins workspace's `--workspace` checks need FFmpeg; a release line: `node scripts/plugins/bump-version.mjs X.Y.Z`, then `packages/*/package.json` by hand.
- [ ] **Step 2: `README.md`** "Develop Celesta": `plugins/ffmpeg` for `crates/codec-ffmpeg`; the recipe in the code block; Celesta finds the codec through `CELESTA_PLUGIN_PATH`.
- [ ] **Step 3: User guide, en and ja with the same content.** Build from source: the note names `plugins/ffmpeg`; step 4 gives the recipe (absolute path, set in each shell where Celesta runs, again after updating the source). Troubleshooting: a source build runs the recipe. Plugins: the sibling program is "as `cargo install` leaves it"; another FFmpeg build is laid out with `node scripts/plugins/build.mjs plugins/ffmpeg --out <your user plugin folder>`.
- [ ] **Step 4: Skill.** `export.md`: the codec bullet and the `no_codec` row give the recipe for a source build; `plugins.md`: the codec bullet (packages bundle `ffmpeg`, a source build points `CELESTA_PLUGIN_PATH` at `plugins/dist`, `cargo install` uses the program beside the executables); `verify.md`: for entries with video or audio, a source checkout also runs the recipe.
- [ ] **Step 5: Publish note.** The root publish no longer needs FFmpeg (drop `PKG_CONFIG_PATH` there; 17 root crates); after the core crates are on crates.io: `export PKG_CONFIG_PATH=…`, `cargo publish --manifest-path plugins/Cargo.toml -p celesta-codec-ffmpeg --dry-run`, then without `--dry-run` (its pins resolve from crates.io, so neither works earlier).
- [ ] **Step 6:** `git grep -n -E "cargo build[^|]*-p celesta-codec-ffmpeg|crates/codec-ffmpeg|stage-codec-plugin|beside[^.]*target/" -- HANDOFF.md README.md packages/website/content skills docs/handoff/2026-11-04-crates-io-publish.md` matches nothing.
- [ ] **Step 7:** Commit: `docs: build the codec plugin into plugins/dist and point CELESTA_PLUGIN_PATH at it`.

### Task 8: The checks, and "Layer 3: what was done"

**Files:**
- Modify: `docs/handoff/2026-11-05-plugins-workspace.md` (a "Layer 3: what was done" section after the Layer 3 section)

- [ ] **Step 1:** Run the checks, each as its own call:
  - `cargo metadata --format-version 1 --locked --no-deps > /dev/null` at the root and with `--manifest-path plugins/Cargo.toml`; `git grep -n -i ffmpeg -- Cargo.lock` (nothing)
  - `cargo clippy -p celesta-exporter -p celesta-editor -p celesta-react-bridge -p celesta-plugin-host --all-targets --locked -- -D warnings`
  - `cargo clippy --manifest-path plugins/Cargo.toml -p celesta-codec-ffmpeg --all-targets --locked -- -D warnings`; `cargo fmt --check` in `plugins/`
  - `rustfmt --edition 2024 --check` on the root `.rs` files `git diff --name-only claude/core-test-codec -- 'crates/*.rs'` lists
  - `pnpm --dir scripts/plugins exec tsc -p test`, `pnpm --dir scripts/plugins exec vitest run test/manifest.test.mts test/licenses.test.mts test/version.test.mts test/repository.test.mts`, `python3 scripts/test_package_macos.py`
  - the focused runs of Tasks 2 and 6: `cargo test -p celesta-plugin-host --lib --locked the_repositorys_plugins_pass_the_manifest_checks`, `cargo test --manifest-path plugins/Cargo.toml -p celesta-codec-ffmpeg --locked --test process`, `… --test export`, `cargo test -p celesta-exporter --test integration inspect::`, the two tests of Task 6
  - `node scripts/plugins/build.mjs plugins/vrm plugins/ffmpeg` (both folders in `plugins/dist`)
  - `ruby -ryaml -e 'ARGV.each { |f| YAML.load_file(f) }' .github/workflows/*.yml`
  - `git diff --check claude/core-test-codec`
  - `git grep -n -E "crates/codec-ffmpeg|stage-codec-plugin"`: list what is left (dated notes and plans only)
- [ ] **Step 2:** Write "Layer 3: what was done" in the style of layer 1's: the move and the codec's manifest and version; the build script's module-less plugins, roles, and `licenses`; the layout decision; versions (the bump script, the checks, `package.json` by hand); CI (the dropped Windows-target check, skips unreported until layer 4); packaging (the license inventory merge, `plugins/target`); finding the codec; what was not run (the Windows and Linux packages, the Docker image, `pwsh`, the macOS package if `dylibbundler` was missing, the plugin job on Windows, where VRM's tests run for the first time, and `cargo install --git … celesta-codec-ffmpeg` from the nested workspace, which the user guide keeps); stale references left; and each check's result (pass counts, the machine).
- [ ] **Step 3:** Commit: `docs: record the plugins workspace's third layer`.
