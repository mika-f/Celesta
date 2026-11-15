# `celesta` command (plugin registry plan 4) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a `celesta` binary whose `celesta plugin …` subcommands declare, lock, install, update, audit, and clean registry plugins, and make `celesta-exporter` and the MCP server honour a project's `celesta.lock` without ever downloading or changing it.

**Architecture:** A new crate `crates/cli` (package `celesta-cli`, binary `celesta`, `publish = false`) is a thin command layer over `celesta-registry-client` (resolution, fetching, store) and `celesta-plugin-host` (lock, discovery). Load-time checks the exporter and MCP server need (is the lock satisfied?) go into `celesta-plugin-host`, because those tools must not depend on the network crate. The plan-3 gaps assigned here are fixed in the crate that owns the code.

**Tech Stack:** Rust 2024 (MSRV 1.89), clap 4 (workspace), serde_json, `indexmap` 2 with `serde`, semver, time; Vitest is not involved (no TypeScript changes).

**Spec:** `docs/handoff/2026-11-14-plugin-registry-design.md` ("Project files", "Store and discovery", "Resolution", "Command line"), `docs/handoff/2026-11-14-registry-client.md` (plan 3: what exists, decisions, "Known gaps").

## Global Constraints

- Every commit is signed (1Password); "failed to fill whole buffer" means stop and ask the user to unlock it. Never amend. End each commit message with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- `celesta-cli` has `publish = false` (it depends on `celesta-registry-client`, which is `publish = false`). Its binary is `[[bin]] name = "celesta"`.
- The packaged executable is named `Celesta-cli` (`Celesta-cli.exe` on Windows) beside `Celesta-export` and `Celesta-mcp`, because `celesta` collides with the editor's `Celesta` on case-insensitive file systems. Source builds and the Linux image keep `celesta`. Every message the tools print says `celesta plugin …`.
- Root key: `official_root_key()` stays `None`. With neither variable set, commands that need the registry fail with exactly: `this build of Celesta has no registry key yet, so it cannot reach the plugin registry; set CELESTA_REGISTRY and CELESTA_REGISTRY_ROOT_KEY to use another registry`. `CELESTA_REGISTRY` (base URL, `https://…`, `http://…`, or `file://<absolute dir>`) and `CELESTA_REGISTRY_ROOT_KEY` (standard base64, `RootKey::from_base64`) must be set together; one alone is a usage error. They are documented in the handoff note only, not in the user guide.
- Lock and trust: `celesta`, `celesta-exporter`, and `celesta-mcp` read the `celesta.lock` of the project they act on whether or not `settings.json` trusts it. The desktop app keeps plan 3's trust gate. Trust still gates a project's own `plugins.paths` folders everywhere.
- Advisories: refreshed at most once per `celesta` run, bounded to 3 seconds in total (`CLI_ADVISORY_TIMEOUT`), then the cached list is used. Warn when the newest verified list is older than 7 days (`advisories_stale`).
- `celesta-exporter` and the MCP server never write `celesta.lock` and never download.
- Output: results on stdout; warnings as `celesta: warning: <text>` and errors as `celesta: error: <text>` on stderr. Exit 0 on success, 1 on failure, 2 on usage errors (clap's default). Deprecated and yanked plugins are warned about on stderr, with the replacement when the registry names one.
- `celesta.config.json` is rewritten keeping every other key, the order of keys, and the text of values the command did not change.
- Tests: Rust only (`cargo test`). Tests never touch the real user folders: each sets `HOME`, `XDG_DATA_HOME`, `XDG_CONFIG_HOME`, and `APPDATA` to temp dirs on the child process, or calls the functions with explicit paths.
- Editing `celesta-plugin-host`'s `[dependencies]` also requires updating `plugins/Cargo.lock` (CI builds the plugins workspace with `--locked`). No task here should need that; if one does, run `cargo update -w --manifest-path plugins/Cargo.toml` and commit the lock.
- For plugin builds set `PKG_CONFIG_PATH="$(brew --prefix ffmpeg@8)/lib/pkgconfig"`. The root workspace tests need no FFmpeg.
- mika-f/Celesta is public: never name or describe the registry service's repository.

## Review Focus

- A `--plugin-dir` / `CELESTA_PLUGIN_PATH` copy of a locked id while the store lacks the locked version: the explicit copy runs and the exporter does not fail (plugin authors test builds this way). Pinned in Task 2 (`explicit_copy_excuses_an_unsatisfied_lock`).
- `celesta.config.json` with other keys, nested objects, unusual formatting, or invalid JSON: `add`/`remove` keep everything else byte-for-byte in value text and order, and invalid JSON fails without writing. Pinned in Task 7.
- `add` that fails after the config was edited (no matching version, network down, install failure): the config and lock are restored to their exact previous bytes (or the created config is removed). Pinned in Task 9 (`a_failed_add_leaves_the_project_as_it_was`).
- Running `celesta plugin …` from a subfolder of the project: the root is the nearest ancestor holding `celesta.config.json`. Pinned in Task 8 (`project_is_found_from_a_subfolder`).
- An advisory endpoint that hangs: the command still finishes its advisory step within the budget and uses the cache; `gc` never removes a version the current project's lock or `registryPlugins` names, nor a `.tmp-*` folder younger than 1 hour. Pinned in Tasks 5 (`a_hanging_transport_times_out_within_the_budget`) and 6 (`gc_keeps_what_locks_and_settings_name`, `gc_removes_old_temporary_folders_only`).

---

### Task 1: One "same build" helper in plugin-host

Plan 3 checks "this folder is that id, version, and tree hash" three times (`plugin-host` `StoreFolder::holds`, `registry-client` `Store::contains` and `Bundled::holds`). Make it one function.

**Files:**
- Modify: `crates/plugin-host/src/registry.rs` (add the helper beside `StoreFolder`; `StoreFolder::holds` calls it)
- Modify: `crates/plugin-host/src/lib.rs` (export it)
- Modify: `crates/registry-client/src/store.rs` (`Store::contains`, `holds`), `crates/registry-client/src/project.rs` (`Bundled::holds`)

**Interfaces:**
- Produces: `pub fn is_build(directory: &Path, id: &str, version: &semver::Version, tree_hash: &Hash) -> bool` in `celesta_plugin_host`: reads the manifest at `manifest_path(directory)` (unparsable or missing → false), requires `manifest.id == id` and `Version::parse(manifest.version) == version`, then `tree_hash(directory) == tree_hash`. It does not check `celesta` ranges or advisories.

- [ ] **Step 1: Write the failing tests** in `crates/plugin-host/src/registry.rs` tests: `is_build_needs_id_version_and_tree_hash` — a temp plugin folder written with a manifest `{"id":"acme.charts","version":"1.2.3",…}` and one file; assert `is_build(dir,"acme.charts",&v("1.2.3"),&tree_hash(dir).unwrap())`, and false for id `"acme.other"`, version `1.2.4`, a different hash, and a folder without a manifest.
- [ ] **Step 2: Run** `cargo test -p celesta-plugin-host is_build` — Expected: FAIL (not defined).
- [ ] **Step 3: Implement `is_build`**; make `StoreFolder::holds` call `is_build(&entry.directory, &self.id, &self.version, expected)`; make `Store::contains` (which must keep its `is_plugin_id` guard) and `Bundled::holds` call it; delete `store.rs`'s private `holds` if nothing else needs it (the `install` early return uses `is_build` with the request's id and version).
- [ ] **Step 4: Run** `cargo test -p celesta-plugin-host -p celesta-registry-client` — Expected: PASS.
- [ ] **Step 5: Commit** `refactor(plugin-host): one check for a store or bundled build`.

### Task 2: Lock checks and project discovery for command-line tools

**Files:**
- Create: `crates/plugin-host/src/lock_check.rs`
- Modify: `crates/plugin-host/src/registry.rs` (`RegistryContext::for_project`), `crates/plugin-host/src/manifest.rs` (`InstalledPlugins::find_for_project`, and a `find_allowing_in` that `find_allowing` delegates to), `crates/plugin-host/src/lib.rs`

**Interfaces:**
- Produces:
  - `pub enum LockProblem` (thiserror, `Debug, Clone, PartialEq`):
    - `Config(String)` — `celesta.config.json` could not be read: `"{0}"`.
    - `Unreadable(String)` — `celesta.lock` could not be read: `"the project's celesta.lock could not be read: {0}"`.
    - `NoLock` — `"the project declares registry plugins but has no celesta.lock; run `celesta plugin install`"`.
    - `OutOfDate { ids: Vec<String> }` — `"celesta.lock does not match celesta.config.json for {ids, joined ", "}; run `celesta plugin install`"`.
    - `OtherCelesta { locked: Version, running: Version, ids: Vec<String> }` — `"celesta.lock was resolved with Celesta {locked}, and this is Celesta {running}; {ids} cannot load with it. Run `celesta plugin update` to resolve them again"` (the same words as plan 3's `ProjectError::OtherCelesta`).
    - `Unsatisfied { plugins: Vec<(String, String, String)> }` (id, version, reason from `DiscoveryError::LockUnsatisfied`) — `"the project's locked plugins are not installed: {id} {version} ({reason}); …; run `celesta plugin install`"`.
  - `pub fn check_lock(lock: Option<&Lockfile>, project: &ProjectDependencies, celesta: &semver::Version) -> Result<(), LockProblem>`: `None` with non-empty dependencies → `NoLock`; `None` with none → Ok; `unloadable` non-empty → `OtherCelesta`; then `OutOfDate` listing, sorted and deduplicated, every dependency id missing from the lock or whose requirement does not match the locked version, **and every lock entry whose id is no dependency** (orphan).
  - `RegistryContext::for_project(root: &Path) -> (Self, Vec<DiscoveryError>)`: as `current`, but the lock of `root` is read without consulting `settings.is_trusted`.
  - `InstalledPlugins::find_for_project(root: &Path, explicit: &[PathBuf], allow: &[String]) -> Result<(Self, Vec<DiscoveryError>), LockProblem>`: loads `UserSettings::current()` (its error becomes `DiscoveryError::Settings` as in `find`), reads `project_dependencies(root)` (error → `Config`) and `Lockfile::load(root)` (error → `Unreadable`), runs `check_lock` against `CELESTA_VERSION`, discovers with `RegistryContext::for_project` through `find_allowing_in` (same allowance and waiting-plugin handling as `find_allowing`), and turns every `DiscoveryError::LockUnsatisfied` into `LockProblem::Unsatisfied`, **except** those whose id some returned plugin with `PluginSource::Explicit` supplies (those stay in the error list as warnings). A project without config dependencies and without a lock behaves exactly as `find`.
  - `InstalledPlugins::find_for_project_reporting(root, explicit, allow) -> Result<Self, LockProblem>`: prints the remaining errors as `find_reporting` does.

- [ ] **Step 1: Write the failing tests** in `lock_check.rs`: `no_lock_is_fine_without_dependencies`, `no_lock_with_dependencies_is_no_lock`, `orphan_entries_are_out_of_date` (lock has `a`,`b`; config has `a` → `OutOfDate { ids: ["b"] }`), `a_requirement_the_locked_version_misses_is_out_of_date`, `other_celesta_names_both_versions` (lock `celesta: 0.8.0`, entry range `>=0.8.0, <0.9.0`, running `0.9.0` → `OtherCelesta { locked: 0.8.0, running: 0.9.0, ids: ["a"] }`, and its `to_string()` contains `celesta plugin update`). In `manifest.rs` tests, with a temp store/HOME set through `RegistryContext` built by hand where needed: `for_project_reads_the_lock_of_an_untrusted_root` (`RegistryContext::for_project(root)` with a lock and no trust → `project.is_some()`), `find_for_project_reads_an_untrusted_lock` (untrusted root whose lock names a store version present with the right tree hash → plugin found with `PluginSource::Locked`), `missing_store_version_is_unsatisfied` (→ `Err(LockProblem::Unsatisfied { plugins })` with the locked id), `explicit_copy_excuses_an_unsatisfied_lock` (same, plus an explicit folder with that id → `Ok`, the explicit copy runs, the `LockUnsatisfied` stays as a warning).
- [ ] **Step 2: Run** `cargo test -p celesta-plugin-host lock_check find_for_project` — Expected: FAIL.
- [ ] **Step 3: Implement** the items above; to keep `find_for_project` testable without the real user folders, put its body in a private `find_for_project_with(root, explicit, allow, settings: &UserSettings, registry: RegistryContext)` that the tests call.
- [ ] **Step 4: Run** `cargo test -p celesta-plugin-host` — Expected: PASS.
- [ ] **Step 5: Commit** `feat(plugin-host): check a project's lock for command-line tools`.

### Task 3: Test registry as a crate feature

The command's tests and Task 5 need plan 3's signed test registry, which lives in `crates/registry-client/tests/common/mod.rs`, out of reach of other crates.

**Files:**
- Create: `crates/registry-client/src/testing.rs` (moved from `tests/common/mod.rs`)
- Modify: `crates/registry-client/Cargo.toml` (feature `testing = ["dep:ed25519-dalek", "dep:base64"]`; those two become optional `[dependencies]`; `[dev-dependencies] celesta-registry-client = { path = ".", features = ["testing"] }`), `crates/registry-client/src/lib.rs` (`#[cfg(feature = "testing")] pub mod testing;`), `crates/registry-client/tests/common/mod.rs` (becomes `pub use celesta_registry_client::testing::*;`)

**Interfaces:**
- Produces: everything `tests/common` exported (`BASE`, `CELESTA_RANGE`, `at`, `plugin_files`, `tree_of`, `write_files`, `TestRegistry` with `new`, `root_key`, `transport`, `client`, `publish_with_roles`, `publish`, `edit`, `advisories`), plus `TestRegistry::with_base(base: &str) -> Self` (archive and `api` URLs under `base`; `new()` is `with_base(BASE)`), `TestRegistry::served_dir(&self) -> &Path`, and `TestRegistry::root_key_base64(&self) -> String`.

- [ ] **Step 1: Move the module** and make `tests/common` re-export it; add `with_base`, `served_dir`, `root_key_base64`.
- [ ] **Step 2: Write the test** `with_base_serves_archives_under_the_base` in `testing.rs` (`#[cfg(test)]`): `TestRegistry::with_base("file:///x")`, publish `acme.one 1.0.0` for `any`, read the served index JSON, assert every `url` starts with `file:///x/`.
- [ ] **Step 3: Run** `cargo test -p celesta-registry-client --all-targets` and `cargo build -p celesta-registry-client` (without the feature, so `ed25519-dalek` stays out of normal builds) — Expected: PASS; `cargo tree -p celesta-registry-client -e normal | grep ed25519` prints nothing beyond `celesta-registry-format`'s own use, if any.
- [ ] **Step 4: Commit** `test(registry-client): expose the test registry behind a feature`.

### Task 4: Resolver and `install_locked` gaps

**Files:**
- Modify: `crates/registry-client/src/resolve.rs` (`keep`), `crates/registry-client/src/project.rs` (`install_locked`, `ProjectError`)
- Test: `crates/registry-client/tests/resolve.rs`, `crates/registry-client/tests/project.rs`

**Interfaces:**
- Consumes: `check_lock`, `LockProblem` (Task 2).
- Produces: `ProjectError::LockCheck(LockProblem)` (`#[error("{0}")]`, `#[from]`) replacing `ProjectError::NoLock`, `OutOfDate`, and `OtherCelesta`; `install_locked` calls `check_lock` first, so orphan lock entries make `--locked` fail. New `ResolveWarning::LockedWithdrawn { id, version }` (`"{id} {version} is locked but is no longer distributed; it was resolved again"`) and `ResolveWarning::LockedBlocked { id, version, advisory }` (`"{id} {version} is locked but blocked by advisory {advisory}; it was resolved again"`), pushed by `keep` when it drops an entry for those reasons (plan-3 gap: re-resolved without a warning).

- [ ] **Step 1: Write the failing tests**: in `tests/resolve.rs`, `a_kept_entry_is_resolved_again_for_a_new_target` — previous lock `targets: [linux-x86_64]`, entry `1.0.0` with only a `linux-x86_64` build, index also has `1.1.0` with `linux-x86_64` and `windows-x86_64`; project targets now `[linux-x86_64, windows-x86_64]`, `Update::Keep` → locked version `1.1.0`. `a_partial_entry_is_kept_while_targets_stay` — same lock, but project targets unchanged `[linux-x86_64]` and the entry partial since the lock was written → stays `1.0.0`. `a_media_entry_is_kept_for_a_new_target` — media role → stays. `a_withdrawn_lock_entry_warns` and `a_blocked_lock_entry_warns` (the new warnings appear). In `tests/project.rs`, `install_locked_refuses_orphan_entries` (lock names `b`, config only `a` → `Err(ProjectError::LockCheck(LockProblem::OutOfDate { ids }))` with `ids == ["b"]`); update existing assertions on `NoLock`/`OutOfDate`/`OtherCelesta` to the new variant.
- [ ] **Step 2: Run** `cargo test -p celesta-registry-client --test resolve --test project` — Expected: FAIL.
- [ ] **Step 3: Implement.** In `keep`, for a non-media index, drop the entry when some project target is in neither `previous.targets` nor the entry's targets (and `any` is not among them). Emit the two warnings where `keep` already returns `None` for withdrawn/blocked.
- [ ] **Step 4: Run** `cargo test -p celesta-registry-client` — Expected: PASS.
- [ ] **Step 5: Commit** `fix(registry-client): re-resolve for new targets and refuse orphan lock entries`.

### Task 5: Bounded advisory refresh

**Files:**
- Create: `crates/registry-client/src/refresh.rs`
- Modify: `crates/registry-client/src/cache.rs` (serialize stamp updates), `crates/registry-client/src/lib.rs`

**Interfaces:**
- Produces:
  - `pub enum AdvisoryRefresh { Fresh(AdvisoryList), Failed(String), TimedOut }`.
  - `pub fn refresh_advisories_within(registry: Registry, now: OffsetDateTime, budget: Duration) -> AdvisoryRefresh`: runs `registry.refresh_advisories(now)` on a spawned thread that owns `registry`, waits on a channel with `recv_timeout(budget)`, and returns `TimedOut` when the budget passes (the thread is left to finish or die with the process). The CLI passes `CLI_ADVISORY_TIMEOUT` as `budget` and builds `registry` with `HttpTransport::with_timeout(CLI_ADVISORY_TIMEOUT)`.
  - In `cache.rs`, a process-wide `static STAMPS: Mutex<()>` held across `record_stamp`'s read-modify-write, so a refresh thread outliving its budget cannot lose a stamp the main thread records.

- [ ] **Step 1: Write the failing tests** in `refresh.rs`: `a_hanging_transport_times_out_within_the_budget` (a `Transport` whose `open` sleeps 5 s; budget 200 ms; assert `TimedOut` and elapsed < 1 s); `a_working_registry_is_fresh` (`testing::TestRegistry` from Task 3 with an advisory list; assert `Fresh(list)` and that `cached_advisories()` now returns it); `a_failure_is_reported` (`DirTransport` at an empty dir → `Failed(message)` with the `config.json` URL in it). In `cache.rs`: `concurrent_stamps_are_all_kept` (two threads record stamps for different paths 100 times each; every path's newest stamp is present at the end).
- [ ] **Step 2: Run** `cargo test -p celesta-registry-client refresh concurrent_stamps` — Expected: FAIL.
- [ ] **Step 3: Implement**; export `AdvisoryRefresh` and `refresh_advisories_within`.
- [ ] **Step 4: Run** `cargo test -p celesta-registry-client` — Expected: PASS.
- [ ] **Step 5: Commit** `feat(registry-client): bound the advisory refresh to one budget`.

### Task 6: Store use records and garbage collection

`gc` must know every project that uses the store. The client records each project root it locks or installs for, and `gc` keeps what any recorded project's lock (still on disk), the current project, or `registryPlugins` names.

**Files:**
- Create: `crates/registry-client/src/gc.rs`
- Modify: `crates/registry-client/src/store.rs` (`Store::root`, project record), `crates/registry-client/src/project.rs` (`lock_project` and `install_locked` record the root after success), `crates/registry-client/src/lib.rs`

**Interfaces:**
- Produces:
  - `pub const PROJECTS_FILE: &str = ".projects.json"` in the store root: `{"projects": ["<absolute root>", …]}`, sorted, written atomically (temp + rename, 0600 on Unix via the existing private-write helper pattern). Unknown keys are kept on rewrite.
  - `Store::record_project(&self, root: &Path) -> io::Result<()>` (absolute, links resolved with `std::fs::canonicalize`, deduplicated).
  - `pub struct GcPlan { pub versions: Vec<(String, Version)>, pub temporary: Vec<PathBuf>, pub forgotten: Vec<PathBuf> }` and `pub struct GcInput<'a> { pub store: &'a Store, pub settings: &'a UserSettings, pub current: Option<&'a Path>, pub now: SystemTime }`.
  - `pub fn plan_gc(input: &GcInput) -> io::Result<GcPlan>`: keeps every `<id>/<version>` named by `settings.registry_plugins`, by the lock of `current` (whether recorded or not), and by the lock of each recorded project whose `celesta.lock` still loads; lists other `<id>/<version>` folders, `.tmp-*` folders whose modification time is more than 1 hour before `now`, and recorded roots whose lock is gone or unreadable (`forgotten`). An id folder left empty is removed with its last version.
  - `pub fn run_gc(store: &Store, plan: &GcPlan) -> io::Result<()>`: removes them and rewrites `.projects.json` without `forgotten`.

- [ ] **Step 1: Write the failing tests** in `gc.rs`: `gc_keeps_what_locks_and_settings_name` (store holds `a/1.0.0`, `a/1.1.0`, `b/2.0.0`, `c/0.1.0`; recorded project locks `a 1.1.0`; settings `registryPlugins` has `b 2.0.0`; `current` unrecorded project locks `c 0.1.0` → `versions == [("a",1.0.0)]`); `gc_forgets_projects_without_a_lock` (recorded root deleted → in `forgotten`, its versions collected); `gc_removes_old_temporary_folders_only` (`.tmp-old` with mtime 2 h ago is listed, `.tmp-new` is not; set mtime with `File::set_modified`); `run_gc_removes_and_rewrites_the_record`. In `tests/project.rs`: `lock_project_records_the_project`.
- [ ] **Step 2: Run** `cargo test -p celesta-registry-client gc lock_project_records` — Expected: FAIL.
- [ ] **Step 3: Implement.** A record failure after a successful lock or install is not an error of that operation: return it as `LockOutcome::record_error: Option<String>` / `InstallOutcome::record_error: Option<String>` so the caller warns.
- [ ] **Step 4: Run** `cargo test -p celesta-registry-client` — Expected: PASS.
- [ ] **Step 5: Commit** `feat(registry-client): record projects and collect unused store versions`.

### Task 7: Editing `plugins.dependencies` in `celesta.config.json`

**Files:**
- Create: `crates/cli/src/config.rs`
- Create (minimal, so the crate builds): `crates/cli/Cargo.toml`, `crates/cli/src/lib.rs`; add `"crates/cli"` to the root `Cargo.toml` `members` and `celesta-cli = { path = "crates/cli" }` to `[workspace.dependencies]`

**Interfaces:**
- Produces:
  - `pub struct ProjectConfig` holding the file as `IndexMap<String, Box<serde_json::value::RawValue>>` (enable `serde_json`'s `raw_value` feature in this crate; `indexmap = { version = "2", features = ["serde"] }`), with `plugins` likewise and `dependencies` as `IndexMap<String, String>`.
  - `ProjectConfig::load(root: &Path) -> Result<Option<(Self, Vec<u8>)>, ConfigError>` (the parsed file and its exact bytes, for restoring).
  - `ProjectConfig::new() -> Self` (for a project without the file).
  - `fn set_dependency(&mut self, id: &str, requirement: &str)` (replaces in place, else appends), `fn remove_dependency(&mut self, id: &str) -> bool` (drops `dependencies` when it becomes empty, and `plugins` when that leaves it empty).
  - `fn save(&self, root: &Path) -> Result<(), ConfigError>`: two-space pretty JSON, one trailing newline, temp file + rename; raw values are written as their original text.
  - `pub fn restore(root: &Path, previous: Option<&[u8]>) -> io::Result<()>`: writes the exact previous bytes back, or removes the file when there was none.
  - `pub enum ConfigError { Read{path, source}, Parse{path, source}, NotAnObject{path}, Write{path, source} }` with messages like plugin-host's `ProjectConfigError`.

- [ ] **Step 1: Write the failing tests** in `config.rs`: `adding_keeps_other_keys_in_order` (input `{"name":"x","plugins":{"paths":["p"],"dependencies":{"b":"1"}},"zeta":{"k":[1, 2]}}` → after `set_dependency("a","0.2")` the output parses with keys in order `name, plugins, zeta`, `plugins` keys `paths, dependencies`, dependencies `b, a`, and the text `[1, 2]` survives verbatim); `replacing_keeps_the_position`; `removing_the_last_dependency_drops_empty_objects` (`{"plugins":{"dependencies":{"a":"1"}}}` → `{}\n`); `removing_keeps_plugins_with_other_keys`; `invalid_json_is_refused_and_not_written` (load of `{"plugins":` → `Err(Parse)`, file bytes unchanged); `a_non_object_is_refused` (`[]`); `restore_puts_back_the_exact_bytes` and `restore_removes_a_created_file`; `the_saved_file_is_read_by_plugin_host` (after `set_dependency("acme.charts","1.2.3")` and `save`, `celesta_plugin_host::project_dependencies(root)` returns `{"acme.charts": ^1.2.3}`).
- [ ] **Step 2: Run** `cargo test -p celesta-cli config` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p celesta-cli config` — Expected: PASS.
- [ ] **Step 5: Commit** `feat(cli): edit plugins.dependencies without losing other settings` (include `Cargo.lock`).

### Task 8: The `celesta` binary: arguments, session, advisories

**Files:**
- Create: `crates/cli/src/main.rs`, `crates/cli/src/args.rs`, `crates/cli/src/session.rs`, `crates/cli/src/report.rs`, `crates/cli/src/advisories.rs`, `crates/cli/tests/common/mod.rs`, `crates/cli/tests/session.rs`
- Modify: `crates/cli/Cargo.toml`, `crates/cli/src/lib.rs`

**Interfaces:**
- Consumes: `Registry`, `Store`, `HttpTransport`, `DirTransport`, `official_root_key`, `OFFICIAL_REGISTRY`, `refresh_advisories_within`, `AdvisoryRefresh`, `CLI_ADVISORY_TIMEOUT` (Task 5), `TestRegistry` (Task 3), `advisories_stale`, `registry_store_dir`, `registry_cache_dir`, `user_settings_path`, `bundled_plugin_dir`, `CELESTA_VERSION`, `Target::host()`.
- Produces:
  - `args.rs`: `#[derive(Parser)] pub struct Cli { #[command(subcommand)] pub command: Command }`; `pub enum Command { Plugin(PluginArgs) }`; `pub struct PluginArgs { #[arg(long, global = true)] pub project: Option<PathBuf>, #[command(subcommand)] pub command: PluginCommand }`; `pub enum PluginCommand { Search { text: String }, Info { id: String }, Add { spec: String, #[arg(long)] pin: bool }, Remove { id: String }, Install { #[arg(long)] locked: bool, #[arg(long)] offline: bool, #[arg(long, value_name = "ID[@REQ]")] global: Option<String> }, Update { id: Option<String> }, List, Audit, Gc { #[arg(long)] dry_run: bool } }`. `--global` conflicts with `--locked`; `--offline` with `--global`.
  - `pub fn parse_spec(spec: &str) -> Result<(String, Option<VersionReq>), String>` (`id[@req]`).
  - `session.rs`: `pub struct Session { pub registry: Registry, pub has_key: bool, pub store: Store, pub cache: PathBuf, pub settings_path: PathBuf, pub celesta: Version, pub host: Target, pub bundled: Option<PathBuf>, pub now: OffsetDateTime }`; `pub fn Session::from_env() -> Result<Session, SessionError>`; `pub fn Session::environment(&self) -> Environment<'_>`. Registry selection: both env vars → that base and key; `file://` base → `DirTransport { base, root }` with the path from the URL (on Windows strip the `/` before a drive letter); `http(s)://` → `HttpTransport::new()`; neither var and `official_root_key()` is `None` → `has_key = false` and a `Registry` whose transport fails every `open` with the Global Constraints' no-key message as `TransportError::Failed { message }`; one var alone → `SessionError::HalfRegistry`. No user data folder → `SessionError::NoUserFolder`.
  - `pub fn project_dir_root(dir: &Path) -> PathBuf`: nearest ancestor of `dir` (inclusive, made absolute) holding `celesta.config.json`, else `dir`.
  - `advisories.rs`: `pub fn refresh(session: &Session) -> Option<AdvisoryList>`: when `has_key`, builds a second `Registry` for the same base with `HttpTransport::with_timeout(CLI_ADVISORY_TIMEOUT)` (or the `DirTransport`), calls `refresh_advisories_within(…, CLI_ADVISORY_TIMEOUT)`; warns on `Failed(reason)` with `could not refresh the security advisories ({reason}); using the cached list` and on `TimedOut` with `the security advisories did not arrive within 3 seconds; using the cached list`. Then reads `cached_advisories()`: none → warn `no security advisory list has been fetched yet; plugins under an advisory may run`; `advisories_stale(list, now)` → warn `the security advisory list is {days} days old; plugins under newer advisories may run`. Commands that refresh: `search`, `info`, `add`, `install` (not `--offline`), `update`, `list`, `audit`.
  - `report.rs`: `pub fn warn(text: impl Display)`, `pub fn error(text: impl Display)`, and `pub fn resolve_warnings(warnings: &[ResolveWarning])` (each via `warn`).
  - `lib.rs`: `pub fn run(cli: Cli) -> ExitCode` dispatching to `commands::*` (Tasks 9–10; until then each arm returns `ExitCode::FAILURE` with `report::error("not implemented")`).
  - `tests/common/mod.rs`: `pub struct Home` (temp dir; `fn command(&self) -> std::process::Command` for `env!("CARGO_BIN_EXE_celesta")` with `HOME`, `XDG_DATA_HOME`, `XDG_CONFIG_HOME`, `APPDATA` under it and `CELESTA_REGISTRY`/`CELESTA_REGISTRY_ROOT_KEY` removed), `fn with_registry(&self, registry: &TestRegistry) -> Command` (sets `CELESTA_REGISTRY=file://<served_dir>` and the key; the `TestRegistry` must be made with `with_base` of that same URL), and `fn project(&self, config: &str) -> PathBuf`.

- [ ] **Step 1: Write the failing tests** in `tests/session.rs`: `no_key_fails_with_the_exact_message` (`celesta plugin search x` without the env vars → exit 1, stderr contains the no-key message); `half_a_registry_is_a_usage_error` (only `CELESTA_REGISTRY` set → exit 2 and stderr names `CELESTA_REGISTRY_ROOT_KEY`); `help_lists_every_plugin_command` (stdout of `celesta plugin --help` contains `search`, `info`, `add`, `remove`, `install`, `update`, `list`, `audit`, `gc`); `list_works_without_a_key` (no registry env, a project with a lock → exit 0, no network). Unit tests in `advisories.rs` on a pure `fn notes(refresh: Option<&AdvisoryRefresh>, cached: Option<&AdvisoryList>, now: OffsetDateTime) -> Vec<String>` that `refresh` prints: `a_list_eight_days_old_is_stale` (`generated` = now − 8 days → one note containing `8 days old`), `a_list_six_days_old_is_not_stale`, `no_list_is_said`, `a_timeout_is_said` (contains `3 seconds`). Unit tests in `session.rs`: `project_is_found_from_a_subfolder` (`project_dir_root(root/a/b) == root` when `root/celesta.config.json` exists; `== dir` when none), `file_urls_become_dir_transports`, `parse_spec_splits_id_and_requirement` (`"acme.charts@0.2"` → `("acme.charts", Some(^0.2))`, `"acme.charts"` → `(…, None)`, `"@1"` → `Err`).
- [ ] **Step 2: Run** `cargo test -p celesta-cli` — Expected: FAIL.
- [ ] **Step 3: Implement.** `main.rs` is `fn main() -> ExitCode { celesta_cli::run(Cli::parse()) }`; `HalfRegistry` maps to exit 2.
- [ ] **Step 4: Run** `cargo test -p celesta-cli` — Expected: PASS.
- [ ] **Step 5: Commit** `feat(cli): add the celesta command with its registry session`.

### Task 9: Project commands: `add`, `remove`, `install`, `update`

**Files:**
- Create: `crates/cli/src/commands/mod.rs`, `crates/cli/src/commands/project.rs`, `crates/cli/tests/project.rs`

**Interfaces:**
- Consumes: `lock_project`, `install_locked`, `Update`, `LockOutcome`, `InstallOutcome`, `ProjectError` (Task 4), `ProjectConfig`, `restore` (Task 7), `Session`, `advisories::refresh`, `report` (Task 8), `check_lock`, `project_dependencies`, `Lockfile`.
- Produces (each `-> ExitCode`, taking `&Session` and the project root from `project_dir_root(args.project or current dir)`):
  - `add(spec, pin)`: fetch the index; a media plugin without `--pin` fails `{id} is a media plugin; install it for every project with `celesta plugin install --global {id}`, or pin it to this project with --pin`; `--pin` on a non-media plugin fails `--pin is for media plugins`. Without `@req`, the requirement written is the version `resolve` would choose (newest eligible, via a one-dependency `resolve` call with `Update::All`), as a bare version string such as `"1.2.3"` (Cargo's `^1.2.3`). Save config, then `lock_project(Update::Only({id}))`; on any error restore the config and lock bytes. Prints `added {id} {requirement}, locked {version}` and `installed {id} {version}` per download; resolve warnings via `report`.
  - `remove(id)`: not a dependency → error `{id} is not a dependency of this project`. Otherwise remove from config and from the lock (load, remove entry, save; no lock is fine) without network. Prints `removed {id}`.
  - `install(locked, offline)`: if `--locked` or `--offline`, or the lock exists and `check_lock` passes → `install_locked(offline)`; else `lock_project(Update::Keep)`. Without a key, `install` runs `install_locked(offline = true)` when the lock checks out and maps `ProjectError::Missing` to the no-key message. Prints installed versions, then `skipped {id} {version}: {reason}` per skipped one, then `celesta.lock is satisfied` (or `wrote celesta.lock`). Warns for each locked version the cached index (`Registry::cached_index`) marks yanked or deprecated.
  - `update(id)`: `None` → `Update::All`; an id that is not a dependency → error as `remove`. Prints each version that changed as `{id} {old} -> {new}` and `celesta.lock is up to date` when none did.
  - Every operation that succeeded but returned a `record_error` warns `could not record this project for celesta plugin gc: {error}`.

- [ ] **Step 1: Write the failing tests** in `tests/project.rs` (against `TestRegistry::with_base(file URL)`): `add_writes_config_lock_and_store` (publish `acme.charts 1.2.3` for `any`; `celesta plugin add acme.charts` → exit 0, config has `"acme.charts": "1.2.3"`, `celesta.lock` locks `1.2.3`, store holds `acme.charts/1.2.3`); `add_with_a_requirement_writes_it`; `a_failed_add_leaves_the_project_as_it_was` (config with other keys, `add acme.missing` → exit 1, config bytes unchanged, no lock created; and `add acme.charts@9` → same); `add_refuses_a_media_plugin_without_pin` and `add_pin_locks_a_media_plugin`; `remove_drops_config_and_lock_entries`; `install_locked_fails_on_an_orphan_entry`; `install_offline_fails_for_a_missing_version` (stderr names the id and version); `install_without_a_lock_locks`; `update_moves_to_the_newest_match` (publish `1.3.0` after adding → `update` prints `acme.charts 1.2.3 -> 1.3.0`); `yanked_locked_version_is_warned_about` (yank via `TestRegistry::edit`, run `install`, stderr contains `yanked`); `deprecation_names_the_replacement`.
- [ ] **Step 2: Run** `cargo test -p celesta-cli --test project` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p celesta-cli` — Expected: PASS.
- [ ] **Step 5: Commit** `feat(cli): add, remove, install, and update a project's plugins`.

### Task 10: Registry and store commands: `search`, `info`, `list`, `audit`, `install --global`, `gc`

**Files:**
- Create: `crates/cli/src/commands/registry.rs`, `crates/cli/src/commands/store.rs`, `crates/cli/tests/registry.rs`

**Interfaces:**
- Consumes: `Registry::search`, `Registry::index`, `Registry::cached_index`, `install_global`, `plan_gc`, `run_gc`, `GcInput` (Task 6), `UserSettings`, `Lockfile`.
- Produces:
  - `search(text)`: case-insensitive substring match of `text` on `id` and `description`; one line per match, `{id}  {latest or "-"}  {description}`, with ` (deprecated)` appended; none → `no plugin matches {text}` and exit 0.
  - `info(id)`: header `{id} by {publisher}: {description}`, kind roles, then one line per version newest first: `{version}  celesta {range}  {targets joined ", "}  {state}` where state is `published`, `deprecated`, `yanked`, `withdrawn`, or `blocked ({advisory})`; then `source:` lines saying whether this project locks it (`locked in this project: {version}`), it is installed globally (`installed globally: {version}`), or neither.
  - `list`: `This project` section (each lock entry `{id} {version}  {installed | not installed | no build for {host} | blocked ({advisory})}` plus ` yanked`/` deprecated` from the cached index) and `Global` section (each `registryPlugins` entry the same way); `no registry plugins` when both are empty.
  - `audit`: refresh advisories, then for every locked and global plugin fetch the index (falling back to `cached_index` with the warning `using the cached index of {id}: {error}`); print one line per finding: `{id} {version}: blocked by {advisory}: {title}` (with `; {fixed} fixes it`), `… is no longer distributed`, `… has been yanked`, `… is deprecated: {message}` (with replacement). Exit 1 when anything is blocked or withdrawn; otherwise 0, printing `no problems found` when there is nothing.
  - `install --global <id>[@req]` (requirement defaults to `*`): `install_global(&session.settings_path, …)`, prints `installed {id} {version} for every project`.
  - `gc(dry_run)`: `plan_gc` with `current = Some(project root)` when it holds a config or lock; prints `removed {id} {version}` (`would remove` with `--dry-run`) per version and `removed {n} unfinished downloads` when temporary folders were collected; `--dry-run` never calls `run_gc`.

- [ ] **Step 1: Write the failing tests** in `tests/registry.rs`: `search_matches_id_and_description`; `info_shows_states_and_source` (one yanked and one withdrawn version, project locks one → each state word and `locked in this project` appear); `list_shows_project_and_global` (after `add` and `install --global`); `audit_fails_on_a_blocked_lock_entry` (advisory for the locked version → exit 1, stdout names the advisory id); `audit_passes_a_clean_project` (exit 0, `no problems found`); `install_global_records_registry_plugins` (settings file under `XDG_CONFIG_HOME` has the entry); `gc_dry_run_removes_nothing` and `gc_removes_an_unused_version` (two projects add different versions; delete one project's folder; `gc` removes its version and keeps the other).
- [ ] **Step 2: Run** `cargo test -p celesta-cli --test registry` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p celesta-cli` — Expected: PASS.
- [ ] **Step 5: Commit** `feat(cli): search, inspect, list, audit, and clean registry plugins`.

### Task 11: `celesta-exporter` and the MCP server read the project lock

**Files:**
- Modify: `crates/exporter/src/error.rs` (`ExportError::Plugins(LockProblem)`, `#[error("{0}")]`), `crates/exporter/src/report.rs` (`classify`: code `"plugins_unsatisfied"`, no extra hint since the message carries the command), `crates/exporter/src/main.rs` (the three `find_reporting` calls for a source: `components` keeps `find_reporting` when no source is given), `crates/exporter/src/exporter.rs` (`Exporter::new` without `options.plugins` discovers for the source's project), `crates/exporter/src/lib.rs` (export a helper)
- Modify: `crates/mcp/src/*.rs` call sites that discover plugins for a job's source (exports, renders, inspect); `SharedMedia::current` takes `Option<&Path>` (the job's project root) and discovers with `find_for_project_reporting` when given
- Test: `crates/exporter/tests/integration/plugins_lock.rs` (register in `main.rs`), `crates/mcp` unit test beside the changed call site

**Interfaces:**
- Consumes: `InstalledPlugins::find_for_project_reporting`, `LockProblem` (Task 2), `project_root`.
- Produces: `pub fn project_plugins(source: &Path, explicit: &[PathBuf], allow: &[String]) -> Result<InstalledPlugins, ExportError>` in `celesta_exporter` (`project_root(source)` then `find_for_project_reporting`, mapping the error to `ExportError::Plugins`). The CLI path prints `celesta: error: …` with the problem's message and exits 1; `--json` reports `code: "plugins_unsatisfied"`; MCP reports it in its usual error shape with `HintStyle::Mcp`.

- [ ] **Step 1: Write the failing tests**: `exporter_fails_when_the_lock_is_not_installed` (temp project: `celesta.config.json` with `{"plugins":{"dependencies":{"acme.charts":"1"}}}`, a lock naming `acme.charts 1.0.0` with an `any` tree hash, empty store under a temp `XDG_DATA_HOME`; run `celesta-exporter --json --react <entry> out.mp4` → exit non-zero, JSON `error.code == "plugins_unsatisfied"`, message contains `celesta plugin install`); `exporter_names_both_celesta_versions` (lock `celesta: 0.8.0`, entry range `>=0.8.0, <0.9.0` → message contains `0.8.0`, `0.9.0`, and `celesta plugin update`); `exporter_never_writes_the_lock` (lock bytes and store listing unchanged after the failing run) (each test sets `HOME`, `XDG_DATA_HOME`, `XDG_CONFIG_HOME`, `APPDATA` on the child; the check runs before the React runtime starts, so these need no Node build; the existing integration tests, which have no `celesta.config.json`, cover the unchanged path). For MCP: `export_tool_reports_an_unsatisfied_lock`.
- [ ] **Step 2: Run** `cargo test -p celesta-exporter --test integration plugins_lock` and `cargo test -p celesta-mcp unsatisfied` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p celesta-exporter -p celesta-mcp` — Expected: PASS.
- [ ] **Step 5: Commit** `feat(exporter,mcp): honour the project's celesta.lock`.

### Task 12: Packaging

**Files:**
- Modify: `scripts/package-macos.py` (build `-p celesta-cli`, copy `celesta` to `Contents/MacOS/Celesta-cli`), `scripts/package-windows.ps1` (build, copy `celesta.exe` to `Celesta-cli.exe`), `packaging/linux/Dockerfile` (build `-p celesta-cli`, copy `celesta` beside the others in `/opt/celesta`), `packaging/windows/Celesta.iss` if it lists executables, `packaging/*/README*` where they list the executables
- Check: `.github/workflows/package.yml` (no change expected: `celesta-cli` is `publish = false`, so `scripts/unpublished-crates.sh` skips it; confirm by running it)

- [ ] **Step 1: Edit the scripts.**
- [ ] **Step 2: Verify**: `python3 -c "import ast,sys; ast.parse(open('scripts/package-macos.py').read())"`; `scripts/unpublished-crates.sh Cargo.toml | grep -c celesta-cli` prints `0`; `cargo build -p celesta-cli --release --locked` produces `target/release/celesta`; on this Mac, if the package script supports `--skip-build` with prebuilt binaries, run it into a temp dir and list `Contents/MacOS` (record in the handoff note if not run).
- [ ] **Step 3: Commit** `build: ship the celesta command as Celesta-cli`.

### Task 13: User guide, Agent Skill, and handoff

**Files:**
- Modify: `packages/website/content/en/docs/05-(more)/05-plugins.md` and `packages/website/content/ja/docs/05-(more)/05-plugins.md` (new section "Registry plugins" / "レジストリのプラグイン": `celesta.config.json` `plugins.dependencies` and `targets`, `celesta.lock` committed, every command with one example, `--locked` for CI, `--global` for codecs, `--pin`, advisories and the 7-day warning, what the exporter says when the lock is not installed or was made with another Celesta minor, and the packaged name `Celesta-cli`); `packages/website/content/{en,ja}/docs/01-(getting-started)/02-create-project.md` (the executables sentence names `Celesta-cli`)
- Modify: `skills/celesta/references/plugins.md` (registry plugins: declare, lock, install before export, the `plugins_unsatisfied` error and its fix), `skills/celesta/references/errors.md` (`plugins_unsatisfied`), `skills/celesta/SKILL.md` (index row mentions `celesta plugin`)
- Modify: `crates/mcp/src/docs.rs` only if its `plugins` topic is not read from the website file
- Modify: `HANDOFF.md` (workspace map row for `celesta-cli`; the Linux/macOS/Windows executable names), `docs/handoff/2026-11-14-plugin-registry-design.md` (status line: plan 4 done), `docs/handoff/2026-11-14-registry-client.md` (Known gaps: strike the ones fixed, pointing at the new note)
- Create: `docs/handoff/2026-11-14-celesta-command.md` (Specification: the three decisions above — `Celesta-cli`, the env override, CLI tools read the lock without trust — plus the store project record, the `gc` rules, the advisory budget; Implementation table; Checks table with real results; Later plans: plan 5 uses `refresh_advisories_within` with its own budget, `check_lock`, and `Store::record_project` through `lock_project`/`install_locked`; Known gaps carried over: `locked.registry` ignored, tree hashing cost, `config.json` re-fetched per call, no `--json` output for `celesta`)

- [ ] **Step 1: Write the docs** (English first, then the Japanese page with the same structure and examples).
- [ ] **Step 2: Verify**: `pnpm --dir packages/website run build` if it builds offline (else record why not); every command in the docs exists in `celesta plugin --help`.
- [ ] **Step 3: Run the full checks**: `cargo test --workspace --locked --no-fail-fast`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo fmt --all -- --check`, `git diff --check`; record results in the note.
- [ ] **Step 4: Commit** `docs: document the celesta plugin command`.
