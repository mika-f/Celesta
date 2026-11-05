# Plugin CI for affected plugins (layer 4) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** CI tests only the first-party plugins a pull request affects, found from the Cargo lockfile and the pnpm workspace graph, in one `Plugins (<OS>)` job per operating system whose check passes with nothing to do when no plugin is affected.

**Architecture:** `scripts/plugins/affected.mjs` diffs a base revision against `HEAD`, maps the changed paths onto each plugin's inputs (its folder, the local Cargo packages in its lockfile closure, its `js/` package's pnpm closure, its `licenses` files) and onto a list of shared inputs that affect every plugin, and prints the affected plugins as JSON; with `--github-output` it also writes the step outputs the workflow gates on. Pure helpers live in `lockfile.mjs` and `inputs.mjs`; `cargo.mjs` learns package folders and the workspace root. `cargo test -- --show-output` logs are read by a libtest parser so the existing skip reporter covers the plugin tests too.

**Tech Stack:** Node.js 26 ES modules with JSDoc types, Vitest and `tsc -p test`, `git diff`, `cargo metadata --no-deps`, `pnpm ls`, GitHub Actions (`background`, `wait-all`, step outputs).

**Spec:** `docs/handoff/2026-11-05-plugins-workspace.md` (Goals, Anti-goals, Constraints, "Layer 1: what was done" (the formatting notes), and "Layer 4: plugin CI for affected plugins"). The starting state is layer 3's end state: `docs/superpowers/plans/2026-11-05-plugins-workspace-layer3.md`, especially its Task 1 (`plugin.json` `licenses`, module-less plugins, `checkPluginSource`) and Task 4 (the `plugins` job).

## Global Constraints

- The stack is managed with `gh stack`: this layer is `claude/plugin-ci-affected`, a new branch on layer 3's `claude/codec-plugin-move`, created after layer 3's last commit ("Layer 3: what was done") lands; the parallel tasks' worktrees branch from it. Never amend; commit and test steps are separate calls; no push or pull request steps here.
- Edit files with the Edit tool (no sed, perl, or python rewrites). `pnpm-lock.yaml` changes only through `pnpm install`.
- Every test step runs only the affected scope: the task's Vitest files (`pnpm --dir scripts/plugins exec vitest run test/<file>.test.mts`), `tsc -p test`, a `node` dry run of the script, one narrow `cargo test` target, a YAML parse. No task runs `pnpm --dir scripts/plugins test`, a plugin's whole suite, or a workspace suite; CI does.
- Reviews are per task, scoped to that task's diff, plus one comprehensive review of the whole branch before Task 7 records the checks.
- JavaScript tests are Vitest in TypeScript (`scripts/plugins/test/*.test.mts`, importing the `.mjs` sources, type-checked by `tsc -p test`).
- Plugin CI "finds them from the dependency graphs, not from hand-kept lists. Each affected plugin is tested on all three operating systems." No per-plugin declaration of operating systems.
- Anti-goal: "A plugin that the affected-plugin check misses and that is then never tested. Pushes to `main` test every plugin." Any doubt in the script (an unknown local package, a module without a `js/` package, a failed `git diff`) fails the job; it never shrinks the list.
- GPU tests: Linux `CELESTA_REQUIRE_GPU=1`, macOS and Windows `0`.
- No user-facing behavior or composition API changes: the user guide (`packages/website/content/{en,ja}/`) and `skills/celesta/` do not change.

## Decisions

- **Script and CLI:** `node scripts/plugins/affected.mjs [--base <rev>] [--head <rev>] [--github-output]`. With `--base`, the changed paths are `git diff --name-only --no-renames -z <base> <head>` (`<head>` defaults to `HEAD`; `--no-renames` lists a moved file's old path too). Without `--base`, every plugin is affected (`because: "no base revision"`). It prints one JSON line to stdout, `{"plugins": [Affected, ...]}`, sorted by `id`; errors print `affected.mjs: <message>` and exit 1; bad arguments exit 2 (`parseArgs` strict).
- **Which plugins exist:** every `plugins/*/plugin.json` (not `examples/plugins/`, which stay in the root `Test` job).
- **A plugin's inputs** (repository-relative POSIX paths; a folder covers every path under it, so `crates/plugin` does not cover `crates/plugin-host/…`):
  - its folder (`plugins/<id>`, `plugin.json` included);
  - **Rust**, when `plugin.json` has `cargoPackage`: the folders of every package without `source` reachable from `cargoPackage` in the lockfile of its workspace, whose root `cargo metadata --no-deps` of the plugin's `Cargo.toml` gives (`plugins/`, or the plugin's own folder when it is excluded; the lockfile itself is then a shared input or inside the plugin's folder, so it is not listed again) (the lockfile already merges normal, build, and dev dependencies, and holds `[patch]` path packages without `source` too). Names map to folders through `cargo metadata --no-deps` of the root `Cargo.toml` and of each plugin workspace: every member's folder and every dependency's `path`. A local package with no known folder is an error, not a skip.
  - **JavaScript**, when `plugins/<id>/js/package.json` exists: the folders `pnpm --filter "<its name>..." ls --depth -1 --json` lists (the package and its workspace dependencies, devDependencies included; it works before `pnpm install`). A plugin whose `plugin.json` has `module` but no `js/package.json` is an error: its module's dependencies would be unknown.
  - the files its `plugin.json` `licenses` lists (layer 3: the codec's `../../LICENSE-GPL-2.0`, `../../LICENSE-MIT`, `../../LICENSE-APACHE`, `../../packaging/GPL-SOURCE-OFFER.md`), resolved against its folder. These are the codec's inputs only, not shared.
- **Shared inputs** (`SHARED_INPUTS`, each path checked to exist by a test): `.github/workflows/ci.yml` (the plugin job lives there, so any `ci.yml` change tests every plugin), `.github/workflows/actions.lock`, `.github/actions/setup-ffmpeg`, `scripts/build-ffmpeg-linux.sh` (setup-ffmpeg builds Linux's FFmpeg with it and keys its cache on it), `scripts/plugins`, `scripts/report-skipped-tests.mjs`, `.cargo/config.toml` (Cargo reads it in `plugins/` too: Windows' `rust-lld`), `Cargo.toml`, `Cargo.lock`, `plugins/Cargo.toml`, `plugins/Cargo.lock`, `package.json`, `pnpm-lock.yaml`, `pnpm-workspace.yaml`, `turbo.json`, `mise.toml`, `mise.lock`.
- **Known gap, accepted:** inputs a test reads outside both graphs, such as `examples/minimal.celesta.json`, which the codec's `tests/export.rs` loads. The spec forbids hand-kept lists; the push to `main` tests every plugin and catches it there. Task 7 records it.
- **`@celesta.studio/cli`:** `plugins/vrm/js/package.json` adds `"@celesta.studio/cli": "workspace:*"` to `devDependencies`, since the build checks the module against the CLI's project types and bundles with its `tsc` and `esbuild`. Its closure is the whole runtime (17 packages today), so most `packages/` changes affect VRM; that is what its build really uses.
- **Affected entry** (`Affected`): `{ id, directory, workspace, cargoPackage, jsPackage, because }`: `workspace` is the Cargo workspace root's folder (`"plugins"`) or `null` without `cargoPackage`; `cargoPackage` and `jsPackage` are `null` when absent; `because` is the first changed path (in `git diff` order) that affects it, or `"no base revision"`.
- **Step outputs** (`--github-output` appends to `$GITHUB_OUTPUT`; the workflow and the script are written against this contract, so Tasks 3 and 5 run in parallel):
  - `any`: `true` or `false`.
  - `plugins`: the `Affected` array as compact JSON.
  - `directories`: the affected plugins' folders, space-separated (`plugins/ffmpeg plugins/vrm`).
  - `js`: `--filter <jsPackage>` for each affected plugin with one, space-separated, or empty.
  - `cargo`: one line per Cargo workspace, sorted, `<workspace> -p <cargoPackage> -p …` (`plugins -p celesta-codec-ffmpeg -p celesta-plugin-vrm`), written as `cargo<<CARGO_WORKSPACES` … `CARGO_WORKSPACES`; empty when no affected plugin has a Cargo package. Cargo commands run in the workspace's folder, which is how `cargo fmt --check -p a -p b` checks those members only (verified on a scratch two-member workspace: it lists only the members' files; `--manifest-path` with `-p a -p b` also works, without `-p` it fails).
- **Base revision per event:** `pull_request` fetches `github.event.pull_request.base.sha` at depth 1 (`git fetch --no-tags --depth=1 origin "$BASE_SHA"`) and passes it as `--base`. `HEAD` is the merge commit of the pull request onto that base, so the two-dot diff is the pull request's change; no `fetch-depth: 0`, and no merge base is needed. If the base moved between the event and the merge commit, the diff can only grow (more plugins), never lose the pull request's own paths. `push` (only `main` triggers it) and `workflow_dispatch` pass no `--base`: every plugin. There is no `merge_group` trigger in `ci.yml`, so no merge-group handling; one added later would pass `github.event.merge_group.base_sha` the same way, and until then it would test every plugin. Locally: `node scripts/plugins/affected.mjs --base "$(git merge-base origin/main HEAD)"` after committing.
- **The job:** `plugins`, `Plugins (${{ matrix.name }})`, the `test` job's matrix, `fail-fast: false`, `timeout-minutes: 150`, bash. Steps: checkout; fetch the base (`pull_request` only); isolate the toolchain; Rust 1.99.1 with `clippy, rustfmt`; mise; **Find the affected plugins** (`id: affected`). Every later step is gated at the step level (`if:`), not the job level: a job-level `if` on a matrix job reports one unexpanded check name, so the names would not stay stable. `wait-all` stays ungated (it takes no `if`; the `test` job's Windows already waits with nothing in the background).
- **FFmpeg is set up whenever any plugin is affected**, not only for the codec. rust-cache keys on `env-vars: FFMPEG_VERSION FFMPEG_FEATURES` and only `main` saves, where every plugin runs with FFmpeg; a pull request job without setup-ffmpeg would hash empty values, miss main's cache, and build VRM cold. The cost is about 10 s on Windows, a cached build on Linux, and about 2 minutes in the background on macOS.
- **rust-cache per OS:** `workspaces: plugins -> target` and `. -> target` (the codegen's), `env-vars: FFMPEG_VERSION FFMPEG_FEATURES`, `save-if` on `main`, no `key`: the three matrix jobs share the job id, and rust-cache's key already hashes `rustc -vV`, whose host triple differs per OS, as the `test` job relies on.
- **JavaScript steps only when `js` is non-empty:** `pnpm install --frozen-lockfile`, the codegen (`--locked`) and `build:runtime`, and `pnpm $JS test`. Layer 3's build script needs the runtime's project types only for a plugin with a module, and the codec's Rust tests need no runtime.
- **Format and Clippy on Linux only** (layer 3's call, as in the `test` job): formatting is the same everywhere, and Clippy on Linux finishes first. Windows-only code is compiled by Windows' own `cargo test`; its lints are not checked, as in the root.
- **Tests:** `cargo test <-p …> --locked --no-fail-fast -- --show-output` in each workspace folder, `2>&1 | tee -a "$RUNNER_TEMP/cargo-test.log"` (bash in Actions runs with `pipefail`). Not nextest: VRM's `process` target is `harness = false`. `--show-output` prints each passing test's captured output under `---- <test> stdout ----`, `eprintln!` included; a `harness = false` binary's own lines appear after its `Running tests/process.rs (…)` line (both verified on a scratch workspace with Cargo 1.99.1). `process.rs`'s `main` ignores its arguments.
- **Skipped tests:** `scripts/report-skipped-tests.mjs <file>` reads JUnit when the file ends in `.xml` (as today) and otherwise a `cargo test -- --show-output` log through `skippedTests` in `scripts/plugins/libtest.mjs`. A test is named `<binary label> <test>` (`tests/mtoon.rs some_test`); a `harness = false` binary's line is named by its label alone. Cargo's `Running` line names no package, so two packages' `tests/process.rs` share a label; accepted.
- **The scripts tests move to the root `Test` job** (`pnpm --dir scripts/plugins test` in its "Test Celesta packages" step) and leave the plugin job. They check `examples/plugins/*` versions, which affect no first-party plugin, and they run on every pull request on all three systems, which also runs `inputs.test.mts`'s repository test on Windows paths.
- **Dropped** (layer 3 already dropped it): the Windows-target `cargo check` on Linux.

## Inventory (file → task)

| File | Change | Task |
| --- | --- | --- |
| `plugins/vrm/js/package.json`, `pnpm-lock.yaml` | `@celesta.studio/cli` devDependency | 1 |
| `scripts/plugins/lockfile.mjs`, `test/lockfile.test.mts` | new: `Cargo.lock` packages and local closure | 2 |
| `scripts/plugins/cargo.mjs`, `test/cargo.test.mts` | `workspaceRoot`, `directories` | 2 |
| `scripts/plugins/inputs.mjs`, `affected.mjs`, `test/inputs.test.mts`, `package.json` (description) | new: plugin inputs, the affected list, the CLI | 3 |
| `scripts/plugins/libtest.mjs`, `test/libtest.test.mts`, `scripts/report-skipped-tests.mjs` | new parser; the reporter reads both formats | 4 |
| `.github/workflows/ci.yml` | the `plugins` job; scripts tests in the `test` job | 5 |
| `HANDOFF.md` | the `scripts/plugins` row, the validation paragraph | 6 |
| `docs/handoff/2026-11-05-plugins-workspace.md` | "Layer 4: what was done" | 7 |

## Review Focus

- **Windows paths and line endings.** `cargo metadata` and `pnpm ls` give `C:\…` paths, and a checkout with `core.autocrlf` gives a CRLF `Cargo.lock`; inputs must still be `crates/plugin`, never `crates\plugin`, or nothing matches `git diff`'s `/` paths. Task 2 tests a CRLF lockfile; Task 3's `repositoryPath` test and the repository test (run by the `Test` job on Windows) pin the rest. `pnpm` on Windows is a `.cmd` shim: spawn it with `shell: process.platform === 'win32'`.
- **Prefix matching.** `crates/plugin` must not cover `crates/plugin-host/src/lib.rs`, and `plugins/vrm` must not cover `plugins/vrm-extra/…`; a file input (`LICENSE-MIT`) covers only itself. Task 3 tests all three.
- **A local package the map does not know** (a `[patch]` path package, or a path dependency of a path dependency outside both workspaces). The script must stop with the lockfile's path and the package's name, so the job fails rather than skipping a plugin. Task 3 tests it.
- **Nothing affected.** Every step after `affected` except `wait-all` is gated, Cargo steps on `cargo != ''` (a plugin with only a module has no Cargo work), JavaScript steps on `js != ''`, the rest on `any == 'true'`; the job passes in about a minute. Task 5 checks the gates mechanically.
- **Deleted and moved files.** A path deleted from a plugin's folder or from a crate it uses affects that plugin; `--no-renames` lists a moved file's old path. Task 3 tests a deleted path through `affectedPlugins`, and the CLI dry run uses a real diff.

## Parallel groups

1. Tasks 1, 2, 4, 5, and 6 in parallel, each in its own worktree. Owners: Task 1 `plugins/vrm/js/package.json`, `pnpm-lock.yaml`; Task 2 `lockfile.mjs`, `cargo.mjs`, their tests; Task 4 `libtest.mjs`, its test, `scripts/report-skipped-tests.mjs`; Task 5 `ci.yml`; Task 6 `HANDOFF.md`. Task 5 and Task 6 write against the Decisions' CLI and step outputs. Merge in task order.
2. Task 3 (needs Tasks 1 and 2).
3. Comprehensive review of the branch, then Task 7.

---

### Task 1: The VRM module declares the CLI it is built against

**Files:**
- Modify: `plugins/vrm/js/package.json`, `pnpm-lock.yaml`

- [ ] **Step 1:** Add `"@celesta.studio/cli": "workspace:*"` to `devDependencies` (alphabetical, before `@celesta.studio/react`), then run `pnpm install`. Only the `plugins/vrm/js` importer of `pnpm-lock.yaml` changes (`git diff --stat` lists the two files).
- [ ] **Step 2:** Run `pnpm --filter "celesta-plugin-vrm-js..." ls --depth -1 --json`. It lists `@celesta.studio/cli` and `@celesta.studio/react`. Run `pnpm install --frozen-lockfile` (passes) and `pnpm --filter celesta-plugin-vrm-js test` (the package's 17 tests pass, after `pnpm run build:runtime` if `packages/cli/dist` is missing).
- [ ] **Step 3:** Commit: `build(plugin-vrm): declare the CLI the module is built against`.

### Task 2: A plugin's local Cargo packages

**Files:**
- Create: `scripts/plugins/lockfile.mjs`, `scripts/plugins/test/lockfile.test.mts`, `scripts/plugins/test/cargo.test.mts`
- Modify: `scripts/plugins/cargo.mjs` (and its header comment)

**Interfaces:**
- Produces, in `lockfile.mjs`:
  - `@typedef {{ name: string, version: string, source: string | null, dependencies: string[] }} LockPackage`
  - `export function parseCargoLock(text: string, path: string): LockPackage[]`: the `[[package]]` tables, line-based (no TOML dependency), `\r\n` or `\n`. Throws `${path}: a [[package]] has no name or version`.
  - `export function localPackages(packages: LockPackage[], name: string, path: string): string[]`: the sorted names of the packages with `source === null` reachable from the package `name` (itself included), following every dependency. A dependency entry is `"name"`, `"name version"`, or `"name version (source)"`, matched on name, then version, then source. Throws `${path}: no package ${name}` and `${path}: ${entry} names no package`.
- Produces, in `cargo.mjs`: `CargoMetadata` gains `workspaceRoot: string` (`workspace_root`) and `directories: Map<string, string>` (each member's folder from `manifest_path`, and each dependency's `path`, by package name; absolute, as Cargo gives them). `targetDirectory` and `versions` are unchanged, so `build.mjs` and `repository.test.mts` keep working.

- [ ] **Step 1: Write the failing tests.**
  - `lockfile.test.mts` with an inline lockfile (`version = 4`): local `p` (deps `"lib"`, `"serde"`, `"r"`, `"x 2.0.0"`), local `lib` (dep `"p"`, a cycle like VRM's dev-dependency on itself), registry `serde` and `r` (`source = "registry+https://github.com/rust-lang/crates.io-index"`; `r` depends on `"patched"`), local `patched`, registry `x` 1.0.0 and local `x` 2.0.0, git `g` (`source = "git+https://…"`), and local `other` that nothing reaches. `parseCargoLock` gives 9 packages, `serde.source` the registry and `p.source` `null`; the same text with `\r\n` parses equal. `localPackages(…, 'p', 'Cargo.lock')` is `['lib', 'p', 'patched', 'x']`. `'missing'` throws `Cargo.lock: no package missing`; a dependency `"nope"` throws `Cargo.lock: nope names no package`; a `[[package]]` without `version` throws.
  - `cargo.test.mts` in a `mkdtempSync` tree: `Cargo.toml` `[workspace] members = ["crates/x"]`, `crates/x` with `y = { path = "../y" }`, `crates/y`, each with an empty `src/lib.rs`; `cargo generate-lockfile --offline` first (`cargoMetadata` passes `--locked`). `cargoMetadata(<tree>/Cargo.toml)` gives `workspaceRoot` the tree (compare with `realpathSync`, as macOS's temp folder is a symlink) and `directories` with `x` → `<tree>/crates/x` and `y` → `<tree>/crates/y`.
- [ ] **Step 2:** Run `pnpm --dir scripts/plugins exec vitest run test/lockfile.test.mts test/cargo.test.mts`. They fail: `lockfile.mjs` does not exist and `cargoMetadata` has no `directories`.
- [ ] **Step 3:** Implement `lockfile.mjs` and extend `cargoMetadata` (parse `manifest_path`, `dependencies[].path`, `workspace_root` from the same JSON).
- [ ] **Step 4:** Run `pnpm --dir scripts/plugins exec tsc -p test` and the two Vitest files. They pass. Run `pnpm --dir scripts/plugins exec vitest run test/repository.test.mts`. It still passes.
- [ ] **Step 5:** Commit: `build(plugins): read a plugin's local Cargo packages from its lockfile`.

### Task 3: The affected plugins

**Files:**
- Create: `scripts/plugins/inputs.mjs`, `scripts/plugins/affected.mjs` (CLI only, with a header comment in `build.mjs`'s style giving the usage), `scripts/plugins/test/inputs.test.mts`
- Modify: `scripts/plugins/package.json` (description: the plugin build script's helpers and the affected-plugin list)

**Interfaces:**
- Consumes: Task 2's `parseCargoLock`, `localPackages`, `cargoMetadata` (`workspaceRoot`, `directories`); Task 1's devDependency (the repository test's `packages/cli`).
- Produces, in `inputs.mjs`:
  - `export const SHARED_INPUTS: string[]` (the Decisions' list, in that order).
  - `@typedef {{ id: string, directory: string, workspace: string | null, cargoPackage: string | null, jsPackage: string | null, inputs: string[] }} PluginInputs` (`inputs` sorted, deduplicated, repository-relative POSIX).
  - `@typedef {Omit<PluginInputs, 'inputs'> & { because: string }} Affected`
  - `export function repositoryPath(root: string, path: string): string`: `path` relative to `root` with `/` separators; throws `${path} is outside the repository ${root}`.
  - `export function covers(input: string, path: string): boolean`: `path === input` or `path` starts with `input + '/'`.
  - `export function repositoryPlugins(root: string): PluginInputs[]`: every `plugins/*/plugin.json`, sorted by `id`, its inputs per the Decisions; one `cargo metadata` per workspace root (the root `Cargo.toml` and each plugin workspace), memoized. Throws `${plugin.json}: its module needs a js/package.json that declares what the module uses` and `${Cargo.lock}: no folder is known for the local package ${name}`, both paths repository-relative.
  - `export function affectedPlugins(plugins: PluginInputs[], changed: string[] | null, shared?: string[]): Affected[]`: `null` affects all with `because: "no base revision"`; a changed path a shared input covers affects all with that path; otherwise each plugin with the first changed path one of its inputs covers.
  - `export function githubOutputs(affected: Affected[]): string`: the Decisions' five outputs, in the order `any`, `plugins`, `directories`, `js`, `cargo`, each line ending in `\n`.

- [ ] **Step 1: Write the failing tests** in `inputs.test.mts`:
  - `covers`: `crates/plugin` covers `crates/plugin/src/lib.rs`, not `crates/plugin-host/src/lib.rs`; `plugins/vrm` not `plugins/vrm-extra/a`; `LICENSE-MIT` covers itself only. `repositoryPath(root, join(root, 'crates', 'plugin'))` is `crates/plugin`; a path outside `root` throws.
  - `affectedPlugins` with two fixture plugins, `a` (inputs `crates/x`, `plugins/a`) and `b` (inputs `LICENSE-GPL-2.0`, `crates/y`, `plugins/b`), and `shared` `['Cargo.lock', 'scripts/plugins']`: `['crates/x/src/lib.rs']` gives `a` only, `because` that path; `['crates/y/gone.rs']` (a deleted file) gives `b`; `['LICENSE-GPL-2.0']` gives `b`; `['docs/a.md', 'scripts/plugins/build.mjs']` gives both, `because: 'scripts/plugins/build.mjs'`; `[]` gives `[]`; `null` gives both with `because: 'no base revision'`; the result never has `inputs`.
  - `githubOutputs([])` is `any=false\nplugins=[]\ndirectories=\njs=\ncargo<<CARGO_WORKSPACES\nCARGO_WORKSPACES\n`. For `vrm` (`plugins`, `celesta-plugin-vrm`, `celesta-plugin-vrm-js`), `ffmpeg` (`plugins`, `celesta-codec-ffmpeg`, no JS), and `solo` (`plugins/solo`, `celesta-plugin-solo`): `directories=plugins/ffmpeg plugins/solo plugins/vrm`, `js=--filter celesta-plugin-vrm-js`, and the `cargo` lines `plugins -p celesta-codec-ffmpeg -p celesta-plugin-vrm` and `plugins/solo -p celesta-plugin-solo`.
  - `an excluded plugin reads its own Cargo.lock`, in a `mkdtempSync` tree: root `Cargo.toml` `[workspace] members = ["crates/x"] exclude = ["plugins"]`; `plugins/Cargo.toml` `[workspace] members = ["a"]`; `plugins/a` (package `a`, no deps) and `plugins/b` (package `b`, its own `[workspace]`, `x = { path = "../../crates/x" }`), each with `src/lib.rs` and a `plugin.json` naming its `cargoPackage`, `b`'s with `"licenses": ["../../NOTICE"]`; a `NOTICE` at the root; `cargo generate-lockfile --offline` for the three workspaces. `repositoryPlugins(tree)` is `a` with `workspace: 'plugins'` and inputs `['plugins/a']`, and `b` with `workspace: 'plugins/b'` and inputs `['NOTICE', 'crates/x', 'plugins/b']`, both `jsPackage: null`. Add `"module": "js/index.ts"` to `a`'s `plugin.json`: it throws "needs a js/package.json".
  - `a local package without a folder is an error`: in the same kind of tree (no `b`; the root `Cargo.toml` excludes `["plugins", "vendor"]`), `a` depends on `m = { path = "../../vendor/m" }` and `vendor/m` on `ghost = { path = "../ghost" }`; neither is a member of a workspace, so no `cargo metadata` names `ghost`'s folder (offline-safe, unlike a `[patch]` of a registry crate). `repositoryPlugins(tree)` throws `plugins/Cargo.lock: no folder is known for the local package ghost` (the lockfile path as given).
  - The repository: `repositoryPlugins(root)` ids are `['ffmpeg', 'vrm']`. `vrm`: `workspace: 'plugins'`, `cargoPackage: 'celesta-plugin-vrm'`, `jsPackage: 'celesta-plugin-vrm-js'`, inputs contain `plugins/vrm`, `crates/plugin`, `crates/composition`, `crates/gpu-renderer`, `crates/plugin-host` (dev), `packages/cli`, `packages/react`, `packages/character`, and not `crates/exporter`, `crates/editor`, `plugins/ffmpeg`. `ffmpeg`: `cargoPackage: 'celesta-codec-ffmpeg'`, `jsPackage: null`, inputs contain `plugins/ffmpeg`, `crates/media`, `crates/exporter` (dev), `LICENSE-GPL-2.0`, `packaging/GPL-SOURCE-OFFER.md`, and not `packages/cli`, `crates/editor`. If one of these disagrees with `plugins/Cargo.lock`, the lockfile is right: fix the assertion and say so in Task 7. Every `SHARED_INPUTS` path exists under `root`. Timeout 60 s (`cargo metadata` and `pnpm ls`).
- [ ] **Step 2:** Run `pnpm --dir scripts/plugins exec vitest run test/inputs.test.mts`. It fails: `inputs.mjs` does not exist.
- [ ] **Step 3:** Implement `inputs.mjs`, then `affected.mjs`: parse the arguments, `git -C <root> diff --name-only --no-renames -z <base> <head>` when `--base` is given (a failure is `git diff failed: <its stderr>`), `affectedPlugins(repositoryPlugins(root), changed)`, print the JSON, and with `--github-output` append `githubOutputs` to `$GITHUB_OUTPUT` (`GITHUB_OUTPUT is not set` otherwise). `pnpm` runs with `shell` on Windows.
- [ ] **Step 4:** Run `pnpm --dir scripts/plugins exec tsc -p test` and `pnpm --dir scripts/plugins exec vitest run test/inputs.test.mts`. Both pass.
- [ ] **Step 5: Dry runs**, each its own call:
  - `node scripts/plugins/affected.mjs`: both plugins, `because: "no base revision"`.
  - `node scripts/plugins/affected.mjs --base HEAD`: `{"plugins":[]}`.
  - With `c=$(git log -1 --format=%h --grep='^test(exporter): check exports on the lossless test codec')`: `node scripts/plugins/affected.mjs --base "$c^" --head "$c"` gives `ffmpeg` only, `because` a `crates/exporter/…` path. With the layer 2 commit `style(react-bridge): format the decode test's sample closure` the same way: `ffmpeg` only, if `celesta-react-bridge` is in its lockfile closure (record which).
  - `GITHUB_OUTPUT=<scratch>/out node scripts/plugins/affected.mjs --github-output`: the file has `any=true`, `js=--filter celesta-plugin-vrm-js`, and the `cargo` block `plugins -p celesta-codec-ffmpeg -p celesta-plugin-vrm`. Without `GITHUB_OUTPUT` it exits 1; `--base no-such-rev` exits 1 with "git diff failed"; `--nope` exits 2.
- [ ] **Step 6:** Commit: `build(plugins): list the plugins a change affects`.

### Task 4: Report the plugin tests that skipped themselves

**Files:**
- Create: `scripts/plugins/libtest.mjs`, `scripts/plugins/test/libtest.test.mts`
- Modify: `scripts/report-skipped-tests.mjs` (the header and usage: a nextest JUnit report or a `cargo test -- --show-output` log)

**Interfaces:**
- Produces: `export function skippedTests(log: string): Map<string, string[]>` in `libtest.mjs`: skipped tests by reason line, as the JUnit reader collects them. ANSI color codes are removed and lines split on `\r?\n`. A line `Running <label> (<path>)` or `Doc-tests <crate>` starts a binary (label `tests/mtoon.rs`, `unittests src/lib.rs`, `doctests <crate>`); `---- <test> stdout ----` starts that test's output, which ends at the next `----` header, a `successes:` or `failures:` line, or a new binary; a line starting with `skipping` is a reason, for `<label> <test>` inside a test's output and for `<label>` outside one.

- [ ] **Step 1: Write the failing test** `libtest.test.mts` with this log, written with `\x1b[1m\x1b[92m     Running\x1b[0m` as Cargo colors it and with `\r\n` in a second case:
  ```
       Running unittests src/lib.rs (target/debug/deps/a-7bc534db127be6c4)
  running 0 tests
       Running tests/gpu.rs (target/debug/deps/gpu-777820c74e284fab)
  running 2 tests
  successes:

  ---- t stdout ----
  skipping GPU test because none
  other

  ---- v stdout ----
  skipping GPU test because none

  successes:
      t
      u
      v
       Running tests/process.rs (target/debug/deps/process-cf4fb80f53236909)
  skipping plain: no gpu
  test the_process_answers_as_the_provider_does ... ok
     Doc-tests a
  ```
  `skippedTests` gives `skipping GPU test because none` → `['tests/gpu.rs t', 'tests/gpu.rs v']` and `skipping plain: no gpu` → `['tests/process.rs']`; a log without `skipping` lines gives an empty map.
- [ ] **Step 2:** Run `pnpm --dir scripts/plugins exec vitest run test/libtest.test.mts`. It fails: `libtest.mjs` does not exist.
- [ ] **Step 3:** Implement `libtest.mjs`. In `report-skipped-tests.mjs`, build `reasons` from the JUnit loop when the file ends in `.xml`, else from `skippedTests(readFileSync(file, 'utf8'))` (imported from `./plugins/libtest.mjs`); the "No report at" exit, the annotations, and the summary are shared.
- [ ] **Step 4:** Run `pnpm --dir scripts/plugins exec tsc -p test` and the Vitest file. Both pass.
- [ ] **Step 5:** Run `cargo test --manifest-path plugins/Cargo.toml -p celesta-plugin-vrm --test render --locked -- --show-output 2>&1 | tee <scratch>/render.log`, then `node scripts/report-skipped-tests.mjs <scratch>/render.log`: "No tests skipped themselves." on Metal. Write the Step 1 log to `<scratch>/skip.log` and run the reporter on it: `2 tests: skipping GPU test because none` and `1 test: skipping plain: no gpu`. If a nextest JUnit report is at hand (`target/nextest/ci/junit.xml`), the reporter still reads it.
- [ ] **Step 6:** Commit: `ci: report the plugin tests that skipped themselves`.

### Task 5: `Plugins (<OS>)` runs only for affected plugins

**Files:**
- Modify: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: the Decisions' CLI and step outputs (Task 3), `node scripts/report-skipped-tests.mjs <log>` (Task 4).

- [ ] **Step 1: The `test` job.** Its "Test Celesta packages" step also runs `pnpm --dir scripts/plugins test` (a comment: the plugin scripts' own tests, here because they check `examples/plugins` too and must run on every pull request on every system).
- [ ] **Step 2: The `plugins` job**, per the Decisions, with a comment over it: the plugins workspace is excluded from the root; each job finds the affected plugins first and gates every later step, so the three check names always report; a push to `main` and a manual run test every plugin. Steps:
  1. `actions/checkout@v7.0.1`.
  2. "Fetch the pull request's base", `if: github.event_name == 'pull_request'`, `env: BASE_SHA: ${{ github.event.pull_request.base.sha }}`, `git fetch --no-tags --depth=1 origin "$BASE_SHA"` (comment: `HEAD` is the merge commit onto it, so the diff is the pull request's change).
  3. Isolate Rust toolchain; Rust 1.99.1 with `components: clippy, rustfmt`; mise (`cache_save` on `main`), as now. Ungated: the script runs `cargo metadata` and `pnpm ls`.
  4. `id: affected`, "Find the affected plugins", `env: BASE_SHA: ${{ github.event.pull_request.base.sha }}` (empty off `pull_request`): `node scripts/plugins/affected.mjs --base "$BASE_SHA" --github-output` when `BASE_SHA` is set, else `node scripts/plugins/affected.mjs --github-output`.
  5. Linux dependencies, `if: steps.affected.outputs.any == 'true' && runner.os == 'Linux'`.
  6. setup-ffmpeg as layer 3 has it (background with `id: ffmpeg` off Windows, foreground on Windows), each also gated on `any`; the comment says why FFmpeg is set up for any plugin (the cache key).
  7. `pnpm install --frozen-lockfile`, `if: steps.affected.outputs.js != ''`.
  8. `wait-all:` (ungated; keep the `test` job's comment why).
  9. rust-cache, `if: any`, per the Decisions.
  10. "Build Celesta packages" (`pnpm --dir packages/react run codegen --locked`, `pnpm run build:runtime`), `if: js != ''`.
  11. "Test the components", `if: js != ''`, `env: JS: ${{ steps.affected.outputs.js }}`, `pnpm $JS test`.
  12. Format, `if: cargo != '' && runner.os == 'Linux'`, `env: CARGO_WORKSPACES: ${{ steps.affected.outputs.cargo }}`: `while read -r workspace packages; do [ -n "$workspace" ] || continue; (cd "$workspace" && cargo fmt --check $packages) < /dev/null; done <<< "$CARGO_WORKSPACES"` (the empty-line guard keeps a trailing newline from running Cargo at the repository root); keep layer 1's comment on why it runs in the workspace's folder.
  13. Clippy, same gate and loop: `cargo clippy $packages --all-targets --locked -- -D warnings`.
  14. Test, `if: cargo != ''`, `CELESTA_REQUIRE_GPU: ${{ runner.os == 'Linux' && '1' || '0' }}`, same loop: `(cd "$workspace" && cargo test $packages --locked --no-fail-fast -- --show-output) < /dev/null 2>&1 | tee -a "$RUNNER_TEMP/cargo-test.log"`; comment: not nextest (`harness = false`), `--show-output` keeps passing tests' output for the next step.
  15. Report skipped tests, `if: ${{ !cancelled() && steps.affected.outputs.cargo != '' }}`: `node scripts/report-skipped-tests.mjs "$RUNNER_TEMP/cargo-test.log"`.
  16. "Build the plugin directories", `if: any`, `env: DIRECTORIES: ${{ steps.affected.outputs.directories }}`: `node scripts/plugins/build.mjs $DIRECTORIES`.
  Remove the job's "Test the build script" step (now in `test`).
- [ ] **Step 3:** Run `ruby -ryaml -e 'YAML.load_file(ARGV[0])' .github/workflows/ci.yml` (parses), then this gate check, which prints nothing and exits 0:
  ```sh
  ruby -ryaml -e 'steps = YAML.load_file(ARGV[0])["jobs"]["plugins"]["steps"]; i = steps.index { |s| s["id"] == "affected" } or abort "no affected step"; steps.drop(i + 1).each { |s| next if s.key?("wait-all"); abort "ungated: #{s["name"] || s["uses"]}" unless s["if"].to_s.include?("steps.affected.outputs.") }' .github/workflows/ci.yml
  ```
  `git grep -n -E "vrm-plugin|Check for Windows|scripts/plugins test" -- .github/workflows/ci.yml` matches only the `test` job's scripts tests line.
- [ ] **Step 4:** Commit: `ci: test only the plugins a pull request affects, on every system`.

### Task 6: `HANDOFF.md`

**Files:**
- Modify: `HANDOFF.md` (edit by section, not line number)

- [ ] **Step 1:** The `scripts/plugins` row of the workspace map adds `affected.mjs` (lists the first-party plugins a change affects, from `plugins/Cargo.lock` or a plugin's own lockfile and the pnpm graph, for CI's `Plugins (<OS>)` jobs), its helpers `inputs.mjs` and `lockfile.mjs`, and `libtest.mjs` (skipped tests in a `cargo test -- --show-output` log, for `scripts/report-skipped-tests.mjs`).
- [ ] **Step 2:** The validation paragraph on the plugins workspace adds: CI's `Plugins (<OS>)` jobs run these checks with `-p` for the affected plugins only (every plugin on a push to `main`); to see which a branch affects, commit and run `node scripts/plugins/affected.mjs --base "$(git merge-base origin/main HEAD)"`; a plugin's inputs are its folder, its lockfile's local packages, its `js/` package's workspace dependencies, and its `licenses` files, so a module's `js/package.json` must declare every workspace package its build uses.
- [ ] **Step 3:** `git grep -n -E "affected\.mjs" -- HANDOFF.md` shows both places. Commit: `docs: say how CI picks the plugins to test`.

### Task 7: The checks, and "Layer 4: what was done"

**Files:**
- Modify: `docs/handoff/2026-11-05-plugins-workspace.md` (a "Layer 4: what was done" section after the Layer 4 section)

- [ ] **Step 1:** Run the checks, each as its own call:
  - `pnpm --dir scripts/plugins exec tsc -p test`; `pnpm --dir scripts/plugins exec vitest run test/lockfile.test.mts test/cargo.test.mts test/inputs.test.mts test/libtest.test.mts test/repository.test.mts`
  - `pnpm install --frozen-lockfile`
  - Task 3's dry runs (all plugins, `--base HEAD`, the exporter commit, `--github-output` to a scratch file)
  - in `plugins/`, `cargo fmt --check -p celesta-codec-ffmpeg -p celesta-plugin-vrm -- -l`: clean, and with `-v` it names only `plugins/` files
  - Task 4's narrow run: `cargo test --manifest-path plugins/Cargo.toml -p celesta-plugin-vrm --test render --locked -- --show-output 2>&1 | tee <scratch>/render.log`, then the reporter on it
  - `ruby -ryaml -e 'ARGV.each { |f| YAML.load_file(f) }' .github/workflows/*.yml` and Task 5's gate check
  - `git diff --check claude/codec-plugin-move`
- [ ] **Step 2:** Write "Layer 4: what was done" in the style of layer 1's: the script, its CLI and JSON, and each plugin's inputs (with the repository's two plugins' Rust and JavaScript closures as found); the shared inputs; the `@celesta.studio/cli` devDependency; the job (its steps and gates, the base per event, no `merge_group`, FFmpeg for any plugin and why, rust-cache, Format and Clippy on Linux, `cargo test -- --show-output` and the skip report, the scripts tests moved to `test`); the known gap (`examples/minimal.celesta.json` and other inputs outside the graphs, caught on `main`); what was not run (the `Plugins (<OS>)` jobs themselves, a pull request with nothing affected, Windows paths outside the `test` job's run of `inputs.test.mts`); and each check's result (pass counts, the machine).
- [ ] **Step 3:** Commit: `docs: record the plugins workspace's fourth layer`.
