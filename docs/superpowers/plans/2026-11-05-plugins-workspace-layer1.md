# Plugins workspace (layer 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** First-party plugins share one Cargo workspace under `plugins/`, and one build script builds any plugin directory, in place of the two drifted `build.mjs` copies (mika-f/celesta#692).

**Architecture:** `plugins/Cargo.toml` becomes a virtual workspace with VRM as its only member, and VRM's lockfile becomes `plugins/Cargo.lock`. A small private pnpm package, `scripts/plugins/`, holds the build script with two tested helpers: one asks `rustc --print cfg` for the platform, and one makes the manifest with the version taken from Cargo. CI's `vrm-plugin` job and the docs move to the new paths.

> **Superseded:** taking the version from Cargo was replaced while this layer was built: every `plugin.json` gives its `version`, which must equal its Cargo package's. See the **Versions** bullet of "Layer 1: what was done" in `docs/handoff/2026-11-05-plugins-workspace.md`.

**Tech Stack:** Cargo workspaces (Rust 1.99.1), Node 26 ES modules, Vitest 5 + TypeScript 5 for tests, GitHub Actions.

**Spec:** `docs/handoff/2026-11-05-plugins-workspace.md` (Layer 1, plus "One source of the plugin folder" from Layer 3 for the version rule). Layers 2–4 get their own plans when this layer's PR is up.

## Global Constraints

- The stack is managed with `gh stack`. This layer is its bottom branch. Never amend a commit.
- Commit and test steps are separate calls. Red/green runs only the new tests. The full checks run once, in Task 5.
- Edit files with the Edit tool. No sed, perl, or python rewrites.
- Tests are Vitest in TypeScript (`.test.mts`), type-checked with `tsc -p test`, and import the source (`../platform.mjs`), not a build.
- One-off package binaries run with `pnpx --package=…`, never `npx`.
- Changed user-facing commands are updated in both `packages/website/content/en/` and `packages/website/content/ja/`, and in `skills/celesta/`.
- The root workspace keeps `examples/plugins/flag` and `examples/plugins/turntable` as members. Examples are not moved.
- `celesta-plugin-vrm` stays at version `0.1.0` with `publish = false`. `plugins/Cargo.toml` has no `[workspace.package]` version.

## Review Focus

- **A plugin directory given relative to another working directory**, such as `node ../scripts/plugins/build.mjs vrm` from `plugins/`. It resolves against the current directory, as a CLI path does.
- **A `--target` whose standard library is not installed** (for example `riscv64gc-unknown-linux-gnu`). The platform still comes from `rustc --print cfg --target …`, which needs no std. Task 2 tests it with the real `rustc`.
- **A `plugin.json` that has both `cargoPackage` and `version`.** The build stops with a message that names the file, instead of shipping a manifest version that drifts from Cargo's. Task 3 tests it.
- **`rustc` printing no `target_os` or `target_arch`** (a wrapper or a broken toolchain). The build fails with the command's name, not with a manifest keyed `undefined-undefined`. Task 2 tests it.
- **`CARGO_TARGET_DIR` set by the developer.** The executable is found where Cargo built it, through `cargo metadata`'s `target_directory`, as `plugins/vrm/build.mjs` already does. Task 4 keeps this, with no new test.

---

### Task 1: The plugins Cargo workspace

**Files:**
- Create: `plugins/Cargo.toml`
- Move: `plugins/vrm/Cargo.lock` → `plugins/Cargo.lock` (`git mv`)
- Modify: `plugins/vrm/Cargo.toml` (remove the empty `[workspace]` table and its comment)
- Modify: `Cargo.toml` (root: `exclude = ["plugins"]`, comment "First-party plugins are a workspace of their own, plugins/Cargo.toml.")
- Modify: `.gitignore` (`/plugins/vrm/target` and `/plugins/vrm/dist` become `/plugins/target` and `/plugins/dist`; keep `/plugins/vrm/assets`)

**Interfaces:**
- Produces: the workspace at `plugins/Cargo.toml` with `members = ["vrm"]` and `resolver = "3"`. Layer 3 appends `"ffmpeg"`.

- [ ] **Step 1:** Write `plugins/Cargo.toml`: a `[workspace]` with `resolver = "3"` and `members = ["vrm"]`, and a header comment saying that it is the first-party plugins' workspace, that the root workspace excludes it, and that a plugin that cannot share it (a conflicting `links`, such as another FFmpeg binding) is left out of `members` and becomes a workspace of its own. Move the lockfile and edit the three files above.
- [ ] **Step 2:** Run `cargo metadata --manifest-path plugins/Cargo.toml --format-version 1 --locked --no-deps > /dev/null` and `cargo metadata --format-version 1 --locked --no-deps > /dev/null`. Both exit 0. `git diff --stat -M plugins/Cargo.lock` shows a rename with no content change.
- [ ] **Step 3:** Run `cargo build --manifest-path plugins/Cargo.toml --workspace --all-targets --locked`. It builds into `plugins/target`. The tests run once, in Task 5.
- [ ] **Step 4:** Commit: `build(plugins): make plugins/ a Cargo workspace of its own`.

### Task 2: The platform helper

**Files:**
- Create: `scripts/plugins/package.json` (name `celesta-plugin-scripts`, `private`, `"type": "module"`, `"test": "tsc -p test && vitest run"`; devDependencies `@types/node ^24.13.6`, `typescript ^5.7.3`, `vitest ^5.0.3`, as `plugins/vrm/js` pins them)
- Create: `scripts/plugins/vitest.config.mjs` (`include: ['test/**/*.test.mts']`, `environment: 'node'`)
- Create: `scripts/plugins/test/tsconfig.json` (strict, `noEmit`, `allowJs` and `checkJs` so the `.mjs` sources are checked through their JSDoc, `types: ["node"]`, includes `*.test.mts` and `../*.mjs`)
- Create: `scripts/plugins/platform.mjs`
- Test: `scripts/plugins/test/platform.test.mts`
- Modify: `pnpm-workspace.yaml` (add `"scripts/plugins"`), `pnpm-lock.yaml` (`pnpm install`)

**Interfaces:**
- Produces, in `scripts/plugins/platform.mjs`:
  - `parseRustcCfg(output: string): { os: string, arch: string }`. It returns `target_os` and `target_arch` from `rustc --print cfg` output. They are the values of `std::env::consts::{OS, ARCH}`, which the host matches manifests against. It throws `Error('rustc --print cfg named no target_os')` (or `target_arch`) when one is missing.
  - `rustPlatform(options?: { target?: string, cwd?: string }): { os: string, arch: string }`. It runs `${process.env.RUSTC ?? 'rustc'} --print cfg [--target <target>]` in `cwd` and parses the output. It throws, naming the command, on a non-zero exit.

- [ ] **Step 1: Write the failing test** `test/platform.test.mts`:
  - `parseRustcCfg` on literal cfg excerpts (lines like `target_arch="x86"`, `target_os="windows"`, plus unrelated `target_feature=…` lines) gives these results. The values are those of `rustc --print cfg --target <triple>`:

    | Triple | `{ os, arch }` |
    | --- | --- |
    | `i686-pc-windows-msvc` | `windows`, `x86` |
    | `armv7-unknown-linux-gnueabihf` | `linux`, `arm` |
    | `thumbv7neon-unknown-linux-gnueabihf` | `linux`, `arm` |
    | `riscv64gc-unknown-linux-gnu` | `linux`, `riscv64` |
    | `x86_64h-apple-darwin` | `macos`, `x86_64` |
    | `powerpc64le-unknown-linux-gnu` | `linux`, `powerpc64` |
    | `aarch64-apple-darwin` | `macos`, `aarch64` |
  - Output without `target_os` throws `/target_os/`, and output without `target_arch` throws `/target_arch/`.
  - `rustPlatform({ target: 'riscv64gc-unknown-linux-gnu' })` gives `{ os: 'linux', arch: 'riscv64' }` with the real `rustc`.
  - `rustPlatform({ target: 'not-a-triple' })` throws `/rustc/`.
- [ ] **Step 2:** Run `pnpm install`, then `pnpm --dir scripts/plugins exec vitest run test/platform.test.mts`. It fails, because `platform.mjs` does not exist.
- [ ] **Step 3:** Implement `platform.mjs` with `spawnSync` (`encoding: 'utf8'`), matching `^target_(os|arch)="(.*)"$` per line.
- [ ] **Step 4:** Run `pnpm --dir scripts/plugins test`. The type check and the tests pass.
- [ ] **Step 5:** Commit: `build(plugins): name a plugin's platform from rustc --print cfg`.

### Task 3: The manifest helper

**Files:**
- Create: `scripts/plugins/manifest.mjs`
- Test: `scripts/plugins/test/manifest.test.mts`

**Interfaces:**
- Consumes: nothing from Task 2.
- Produces, in `scripts/plugins/manifest.mjs`:
  - `pluginManifest(source: PluginSource, built: { cargoVersion?: string, executable?: Record<string, string> }, sourcePath: string): object`. `PluginSource` is `plugin.json`'s shape: `id`, `version?`, `cargoPackage?`, `module`, `components?`. The function returns the `celesta-plugin.json` object, in the same keys and order the current scripts write: `id`, `version`, `protocol: 1`, `executable` when given, `module: 'js/index.js'`, `types: 'js/index.d.ts'`, and `components` when given.
  - Version rule: with `cargoPackage`, the version is `cargoVersion`. A `version` in `plugin.json` throws `Error(\`${sourcePath}: version comes from the Cargo package ${cargoPackage}; remove it\`)`, and a missing `cargoVersion` throws. Without `cargoPackage`, `plugin.json`'s `version` is required, and its absence throws a message naming `sourcePath`.

- [ ] **Step 1: Write the failing test** `test/manifest.test.mts`:
  - VRM's case: `{ id: 'vrm', cargoPackage: 'celesta-plugin-vrm', module: 'js/src/index.ts', components: [c] }` with `cargoVersion: '0.1.0'` and `executable: { 'macos-aarch64': 'bin/macos-aarch64/celesta-plugin-vrm' }` deep-equals the manifest, with keys in the order above.
  - beep-voice's case (no `cargoPackage`, `version: '0.1.0'`, no executable): no `executable` key, and `version` is `'0.1.0'`.
  - `cargoPackage` and `version` together throw a message that contains the `sourcePath` and `remove it`.
  - `cargoPackage` without `cargoVersion` throws. No `cargoPackage` and no `version` throws a message that contains the `sourcePath`.
- [ ] **Step 2:** Run `pnpm --dir scripts/plugins exec vitest run test/manifest.test.mts`. It fails.
- [ ] **Step 3:** Implement `manifest.mjs`.
- [ ] **Step 4:** Run `pnpm --dir scripts/plugins test`. It passes.
- [ ] **Step 5:** Commit: `build(plugins): take a plugin's manifest version from its Cargo package`.

### Task 4: The shared build script

**Files:**
- Create: `scripts/plugins/build.mjs`
- Delete: `plugins/vrm/build.mjs`, `examples/plugins/build.mjs`
- Modify: `plugins/vrm/plugin.json`, `examples/plugins/flag/plugin.json`, `examples/plugins/turntable/plugin.json` (remove `version`; Cargo has `0.1.0` for each)

**Interfaces:**
- Consumes: `rustPlatform` (Task 2), `pluginManifest` (Task 3).
- Produces: the command `node scripts/plugins/build.mjs <plugin directory>... [--out <dir>] [--target <triple>] [--skip-js]`. Each plugin directory resolves against the current directory. The default `--out` is the `dist/` folder beside the plugin directory, so `plugins/vrm` → `plugins/dist/vrm` and `examples/plugins/flag` → `examples/plugins/dist/flag`. Later layers and CI call this command.

- [ ] **Step 1:** Write `build.mjs` from `plugins/vrm/build.mjs`, keeping its `buildExecutable` (Cargo run in the plugin directory with `--manifest-path`, `--locked`, and `target_directory` and the package version from `cargo metadata --no-deps`), its `buildModule`, and its keep-other-platforms merge of `executable`. Use `rustPlatform({ target, cwd: pluginDirectory })` in place of `platform()`, and `pluginManifest` in place of the inline object. With no positional argument, it prints usage and exits 2. Update the header comment: usage, the runtime prerequisite (`pnpm install && pnpm run build:runtime`), and where the output goes.
- [ ] **Step 2:** Delete the two old scripts and the three `version` fields.
- [ ] **Step 3:** Run `node scripts/plugins/build.mjs plugins/vrm`. `plugins/dist/vrm/celesta-plugin.json` has `"version": "0.1.0"` and an `executable` key for this machine (`macos-aarch64` on Apple silicon). `plugins/dist/vrm/js/index.js` and `js/index.d.ts` exist.
- [ ] **Step 4:** Run `node scripts/plugins/build.mjs examples/plugins/flag examples/plugins/turntable examples/plugins/beep-voice`. Three folders appear under `examples/plugins/dist/`, and `beep-voice`'s manifest has no `executable`. Then, from `plugins/`, run `node ../scripts/plugins/build.mjs vrm --skip-js`. It writes `plugins/dist/vrm` again.
- [ ] **Step 5:** Commit: `build(plugins): build every plugin directory with one script`. The body notes that it closes mika-f/celesta#692.

### Task 5: CI, docs, and the full checks

**Files:**
- Modify: `.github/workflows/ci.yml` (`vrm-plugin` job)
- Modify: `plugins/vrm/README.md`, `examples/plugins/README.md`, `examples/plugins/flag/README.md`, `examples/plugins/turntable/README.md`, and the usage comments of `examples/plugins/{flag,turntable,beep-voice}/demo.tsx`
- Modify: `packages/website/content/en/docs/05-(more)/05-plugins.md` and `packages/website/content/ja/docs/05-(more)/05-plugins.md` (lines 120–134), and `skills/celesta/references/plugins.md` (lines 83–85)
- Modify: `HANDOFF.md` (the flag and turntable rows, lines 114–115, and the VRM row if it names its workspace), `docs/handoff/2026-11-03-plugin-vrm.md` (lines 27–49, 698, and 770)
- Modify: `docs/handoff/2026-11-05-plugins-workspace.md` (add "Layer 1: what was done" with the checks below)

**Interfaces:**
- Consumes: the command from Task 4 and the workspace from Task 1.

- [ ] **Step 1:** Edit the `vrm-plugin` job:
  - Drop `setup-ffmpeg` and its comment. Neither `plugins/Cargo.lock` nor the codegen that `build:runtime` runs contains FFmpeg. Also drop `env-vars: FFMPEG_VERSION FFMPEG_FEATURES`.
  - rust-cache `workspaces` becomes `plugins -> target` and `. -> target`.
  - Format: `cargo fmt --manifest-path plugins/Cargo.toml --all --check`.
  - Clippy: `cargo clippy --manifest-path plugins/Cargo.toml --workspace --all-targets --locked -- -D warnings`.
  - Windows check: `cargo check --manifest-path plugins/Cargo.toml -p celesta-plugin-vrm --locked --target x86_64-pc-windows-msvc --lib --bins`. Layer 4 drops it.
  - Test: `env CELESTA_REQUIRE_GPU=1 cargo test --manifest-path plugins/Cargo.toml --workspace --locked`.
  - Add `pnpm --dir scripts/plugins test` beside the component test.
  - The build step becomes `node scripts/plugins/build.mjs plugins/vrm`.
  - Update the job's header comment: the plugins workspace, which the root excludes.
- [ ] **Step 2:** Update every command in the docs above. `node plugins/vrm/build.mjs` becomes `node scripts/plugins/build.mjs plugins/vrm`, and `plugins/vrm/dist` becomes `plugins/dist`. `node examples/plugins/build.mjs [name]` becomes `node scripts/plugins/build.mjs examples/plugins/<name>`, and `examples/plugins/dist` stays. Change the en and ja guides alike.
- [ ] **Step 3:** Run `git grep -n -E "plugins/vrm/build\.mjs|examples/plugins/build\.mjs|plugins/vrm/dist|plugins/vrm/Cargo\.lock|manifest-path plugins/vrm"`. Only dated plans under `docs/superpowers/plans/` and earlier handoff notes other than `2026-11-03-plugin-vrm.md` may still match.
- [ ] **Step 4:** Commit: `ci, docs: build and test the VRM plugin from the plugins workspace`.
- [ ] **Step 5:** Run the full checks once, each as its own call:
  - `cargo fmt --manifest-path plugins/Cargo.toml --all --check`
  - `cargo clippy --manifest-path plugins/Cargo.toml --workspace --all-targets --locked -- -D warnings`
  - `cargo test --manifest-path plugins/Cargo.toml --workspace --locked`
  - `pnpm --filter celesta-plugin-vrm-js test`
  - `pnpm --dir scripts/plugins test`
  - `cargo metadata --format-version 1 --locked --no-deps > /dev/null` at the root

  Record the results in the handoff note's "Layer 1: what was done", then commit that as `docs: record the plugins workspace's first layer`.
- [ ] **Step 6:** Push with `gh stack` and open the layer's PR. The body says what changed, how to reproduce the checks, and that it closes mika-f/celesta#692. CI's `VRM plugin` job must pass.
