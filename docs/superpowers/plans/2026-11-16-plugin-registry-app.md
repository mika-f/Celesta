# Registry plugins in the desktop app — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The desktop editor finds, installs, updates, and removes registry plugins (registry.celesta.studio) from the Plugins tab and a plugin page, and honours a trusted project's `celesta.lock`.

**Architecture:** The editor calls `celesta-registry-client` in process on GPUI's background executor. Pieces the CLI owns today that the app needs too (project-config editing, add/remove, registry selection) move into the client crate so both tools share them; the CLI's behaviour and tests stay as they are. One `Entity<RegistryState>` in the editor owns registry work in flight; the Plugins tab and the plugin page read it and emit no network calls themselves.

**Tech Stack:** Rust, GPUI via `gpui-kit` 0.9, `celesta-registry-client`, `celesta-plugin-host`, `semver`, `ureq`; tests with `cargo test` and `#[gpui_kit::test]`.

**Spec:** `docs/handoff/2026-11-16-plugin-registry-app.md` (read it first; it holds every user-facing decision). Background: `docs/handoff/2026-11-14-plugin-registry-design.md` ("Desktop app"), `docs/handoff/2026-11-14-celesta-command.md`.

## Global Constraints

- Build env (macOS here): `export PKG_CONFIG_PATH="$(brew --prefix ffmpeg@8)/lib/pkgconfig"` and `export PATH="/Users/natsuneko/Library/Application Support/mbx/bin:$HOME/.cargo/bin:$HOME/.local/share/mise/installs/node/26/bin:$HOME/.local/share/mise/installs/pnpm/12/bin:$PATH"`.
- Never amend commits; one commit per task (more is fine). Messages end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Commits are signed via 1Password; "failed to fill whole buffer" means it is locked: stop and ask, never bypass signing.
- Tests never write the real user folders: give child processes and store/cache/settings paths temporary folders (as `crates/cli/tests` do).
- The CLI's output, exit statuses, and tests do not change (`cargo test -p celesta-cli` passes unmodified except for import paths in unit tests that move).
- No blocking call (HTTP, extraction, tree hashing, discovery, settings or lock I/O) on the UI thread: use `cx.background_executor().spawn` as `lifecycle.rs` does.
- UI: GPUI Kit Design Guides; theme tokens only (no raw colours in editor code; the palette lives in `crates/editor-theme`); rem helpers, not `px`, except documented physical sizes; versions, sizes, and hashes in the monospace font; sentence-case copy without final periods on labels; `…` on buttons that open a dialog or page; lucide `IconName`s, no glyph arrows; no uppercase labels, no gradients.
- Element ids derive from plugin ids, never list indexes (e.g. `("plugin-row-update", id)` with `SharedString`).
- Every user-facing string the spec quotes is used verbatim.
- `celesta-registry-client` loses `publish = false`; CI's `Crate names` job stays red until the maintainer publishes the placeholder (spec, Constraints). Do not try to make it pass.

## Review Focus

1. A background operation finishing after the user opened another project (or reloaded) must not reload, ask about, or write into the new one: results carry the project root and a generation and are dropped when stale. Test: Task 6.
2. Clicking Install/Add/Update twice, or two different actions on one id, must not start two operations: an id with work in flight refuses new work for it. Test: Task 6.
3. A failed add or update (network drop, hash mismatch, unreadable settings) leaves `celesta.config.json` and `celesta.lock` byte for byte as before and shows the reason on the page. Test: Task 3.
4. A locked plugin with no build for this computer is neither offered for download nor shown as missing; its row says "No build for this computer". Test: Task 10.
5. Offline with no cached `search.json`: the page shows "Couldn’t reach <host>" and **Try Again**, and the Plugins tab still lists everything discovery found. Test: Task 8.

---

### Task 1: Move project-config editing and registry selection into the client

**Files:**
- Create: `crates/registry-client/src/config.rs` (moved from `crates/cli/src/config.rs`, with its tests)
- Create: `crates/registry-client/src/selection.rs` (from `crates/cli/src/session.rs`: `REGISTRY_VAR`, `ROOT_KEY_VAR`, `Remote`, `select_registry`, the override cache folder rule, with their tests)
- Modify: `crates/registry-client/src/lib.rs`, `crates/registry-client/Cargo.toml` (add `sha2` already present; `serde_json` `raw_value` feature if `config.rs` needs it)
- Modify: `crates/cli/src/session.rs`, `crates/cli/src/commands/project.rs`, delete `crates/cli/src/config.rs`

**Interfaces:**
- Produces (all `pub`, re-exported from the crate root):
  - `ProjectConfig` with `new()`, `load(root: &Path) -> Result<Option<(ProjectConfig, Vec<u8>)>, ConfigError>`, `set_dependency(&mut self, id: &str, requirement: &str)`, `remove_dependency(&mut self, id: &str) -> bool`, `save(&self, root: &Path) -> Result<(), ConfigError>`; `ConfigError`; `restore_config(root: &Path, previous: Option<&[u8]>) -> io::Result<()>`; `restore_file(path: &Path, previous: Option<&[u8]>) -> io::Result<()>` (today's `config::restore` is renamed `restore_config`).
  - `REGISTRY_VAR`, `ROOT_KEY_VAR`, `enum Remote { Http, Dir(PathBuf) }` with `Remote::of(base: &str) -> Option<Remote>`.
  - `struct RegistryAccess { pub base: String, pub root: RootKey, pub remote: Remote, pub overridden: bool }` with `transport(&self, timeout: Option<Duration>) -> Box<dyn Transport>`, `cache_dir(&self, cache_root: &Path) -> io::Result<PathBuf>` (official: `cache_root`; override: `cache_root/override-<16 hex>`, created private), `client(&self, cache_root: &Path, timeout: Option<Duration>) -> io::Result<Registry>`.
  - `select_registry(registry: Option<OsString>, key: Option<OsString>) -> Result<RegistryAccess, SelectionError>`; `RegistryAccess::from_env() -> Result<RegistryAccess, SelectionError>`; `SelectionError` with today's `HalfRegistry`, `BadRegistry`, `BadRootKey` variants and messages (exact text unchanged).
- The CLI's `SessionError` keeps `NoUserFolder` and `CacheFolder` and wraps `SelectionError` with `#[error(transparent)]`; `is_usage()` unchanged in effect.

- [ ] **Step 1:** Move the code and its unit tests; fix imports in the CLI. The moved tests keep their names (`adding_keeps_other_keys_in_order`, …, `file_urls_become_dir_transports`, `an_override_registry_has_its_own_cache`, `the_registry_variables_go_together`).
- [ ] **Step 2:** Run `cargo test -p celesta-registry-client -p celesta-cli --locked`. Expected: all pass, the same count as before for the CLI's integration tests.
- [ ] **Step 3:** `cargo clippy -p celesta-registry-client -p celesta-cli --all-targets --locked -- -D warnings` passes. Commit `refactor: share project config and registry selection through the registry client`.

### Task 2: The offered-version rule

**Files:**
- Create: `crates/registry-client/src/offer.rs`
- Modify: `crates/registry-client/src/lib.rs`

**Interfaces:**
- Produces:
  ```rust
  pub enum OfferScope { Global, Project, Pin }
  pub struct OfferInput<'a> {
      pub index: &'a Index,
      pub advisories: Option<&'a AdvisoryList>,
      pub celesta: &'a Version,
      pub host: &'a Target,
      /// The project's targets (`ProjectDependencies::project_targets`); ignored for `Global`.
      pub targets: &'a [Target],
      pub include_prereleases: bool,
      pub scope: OfferScope,
  }
  pub enum NoOffer {
      NoBuild { host: Target },          // no candidate has a build for the host (Global/Pin) or project targets (Project)
      NeedsCelesta { newest: Version, celesta: VersionReq }, // newest otherwise-eligible version's range excludes the running Celesta
      OnlyPrereleases { newest: Version }, // only pre-releases are eligible and include_prereleases is false
      NothingPublished,                  // every version is yanked, withdrawn, or blocked
  }
  pub fn offered_version(input: &OfferInput) -> Result<Version, NoOffer>;
  pub fn prereleases_by_default(celesta: &Version) -> bool; // !celesta.pre.is_empty()
  pub fn offered_requirement(version: &Version) -> VersionReq; // ^version, e.g. "^0.10.0-alpha.3"
  ```
- Rule: candidates are the index's versions, newest first, skipping pre-releases unless `include_prereleases`; the first candidate for which `resolve` succeeds with the requirement `=<candidate>` (dependencies `{id: =v}`, `targets` = `[host]` for `Global`, the project targets otherwise; for `Global` remove the `media` role from `index.kind` first, as `install_global` does) is offered. `NoOffer` is chosen by checking, in order: nothing non-yanked/withdrawn/blocked → `NothingPublished`; only pre-releases eligible and flag off → `OnlyPrereleases`; the newest eligible version's `celesta` excludes the running one → `NeedsCelesta`; else `NoBuild`.

- [ ] **Step 1: Write failing tests** in `offer.rs` using `testing::TestRegistry` indexes (`publish`, `edit`):
  - `a_prerelease_is_offered_only_when_included`: versions `0.10.0-alpha.3` (celesta `>=0.10.0-alpha.3, <0.11.0`) only, celesta `0.10.0-alpha.3`, `Global`, host `macos-aarch64`: flag on → `Ok(0.10.0-alpha.3)`; flag off → `Err(OnlyPrereleases { newest: 0.10.0-alpha.3 })`.
  - `the_newest_stable_wins_over_an_older_prerelease`: `1.0.0`, `1.1.0-beta.1`, `0.9.0` with flag off → `1.0.0`; flag on → `1.1.0-beta.1`.
  - `yanked_and_blocked_versions_are_skipped`: `1.2.0` yanked, `1.1.0` blocked by an advisory (`<1.2.0, >=1.1.0`), `1.0.0` published → `1.0.0`; all three unavailable → `NothingPublished`.
  - `a_version_for_another_celesta_is_not_offered`: only `2.0.0` with celesta `>=0.11.0` and running `0.10.0` → `NeedsCelesta { newest: 2.0.0, .. }`.
  - `global_needs_a_host_build_but_pin_does_not`: a media plugin with only `windows-x86_64`: `Global` on `macos-aarch64` → `NoBuild`; `Pin` with project targets `[macos-aarch64, windows-x86_64]` → `Ok`.
  - `prereleases_by_default_follows_the_running_celesta`: `0.10.0-alpha.3` → true; `0.10.0` → false.
  - `the_offered_requirement_is_a_caret`: `offered_requirement(&"0.10.0-alpha.3".parse()?)` displays `^0.10.0-alpha.3` and matches `0.10.0-alpha.4` and `0.10.2` but not `0.11.0`.
- [ ] **Step 2:** `cargo test -p celesta-registry-client offer` fails (module missing).
- [ ] **Step 3:** Implement as specified above.
- [ ] **Step 4:** The tests pass. Commit `feat(registry-client): choose the version the app offers`.

### Task 3: Project edits as library operations, with requirement rewrites

**Files:**
- Create: `crates/registry-client/src/edit.rs`
- Modify: `crates/registry-client/src/lib.rs`, `crates/cli/src/commands/project.rs` (`add_dependency`, `remove_dependency`, `put_back` call the library)
- Test: in `edit.rs` against `TestRegistry::new()` with temporary store/cache/settings

**Interfaces:**
- Consumes: Task 1 `ProjectConfig`, `restore_config`, `restore_file`; Task 2 `offered_version`, `offered_requirement`; existing `lock_project`, `Environment`, `Update`.
- Produces:
  ```rust
  pub enum EditError {
      MediaNeedsPin { id: String },     // CLI maps to its current message
      PinNeedsMedia { id: String },     // CLI maps to "--pin is for media plugins"
      NotADependency { id: String },    // "{id} is not a dependency of this project"
      Config(ConfigError), Project(ProjectError), Client(ClientError), Io { path: PathBuf, source: io::Error },
  }
  pub fn add_dependency(env: &Environment, root: &Path, id: &str, requirement: &str, pin: bool) -> Result<LockOutcome, EditError>;
  pub fn remove_dependency(root: &Path, id: &str) -> Result<(), EditError>;
  pub struct Rewrite { pub id: String, pub from: String, pub to: String }
  /// For each of `ids`, whether it resolves within its requirement; for each that does not, the rewrite to `^<offered>` (Project scope, or Pin for a media plugin). Fetches the indexes; changes nothing.
  pub fn plan_update(env: &Environment, root: &Path, ids: &BTreeSet<String>, include_prereleases: bool) -> Result<Vec<Rewrite>, EditError>;
  /// Applies `rewrites` to celesta.config.json, then `lock_project(Update::Only(ids))`; on any failure both files go back byte for byte.
  pub fn update_dependencies(env: &Environment, root: &Path, ids: &BTreeSet<String>, rewrites: &[Rewrite]) -> Result<LockOutcome, EditError>;
  ```
- `add_dependency` creates `celesta.config.json` only when called on a folder without one (the CLI's behaviour); the editor never calls it there (spec, "What counts as a project").

- [ ] **Step 1: Write failing tests:**
  - `add_writes_the_requirement_and_locks`: publish `acme.charts 1.2.3` (`any`), `add_dependency(.., "acme.charts", "^1.2.3", false)` → config has `"acme.charts": "^1.2.3"`, lock has `1.2.3`, the store holds it.
  - `a_failed_add_puts_both_files_back`: existing config and lock bytes; publish a plugin whose archive's sha256 is edited to a wrong value (`edit`) → `Err`, and both files equal the old bytes exactly; with no lock before, no lock after.
  - `a_media_plugin_needs_a_pin` / `a_pin_needs_a_media_plugin`: errors `MediaNeedsPin` / `PinNeedsMedia`, files untouched.
  - `remove_drops_the_dependency_and_the_lock_entry`; `removing_an_unknown_id_is_an_error` (`NotADependency`).
  - `plan_update_rewrites_only_what_cannot_be_met`: project requires `vrm ^0.10.0-alpha.3` (locked `0.10.0-alpha.3`) and `code ^1.0` (locked `1.0.0`); registry gains `vrm 0.11.0` (celesta `>=0.11.0, <0.12.0`) and `code 1.1.0`; with running celesta `0.11.0`: `plan_update` for both → `[Rewrite { id: "vrm", from: "^0.10.0-alpha.3", to: "^0.11.0" }]`.
  - `update_with_a_rewrite_changes_the_config_and_lock`: applying that plan → config `vrm` is `^0.11.0`, lock `vrm 0.11.0`, lock's `celesta` is `0.11.0`.
  - `a_failed_update_puts_both_files_back`.
- [ ] **Step 2:** The tests fail to compile (no `edit` module).
- [ ] **Step 3:** Implement; switch the CLI's `add`/`remove` to call `add_dependency`/`remove_dependency`, mapping `EditError` to its current messages (`crates/cli/src/commands/project.rs` keeps its `newest()` with `VersionReq::STAR`).
- [ ] **Step 4:** `cargo test -p celesta-registry-client edit` and `cargo test -p celesta-cli --locked` pass. Commit `feat(registry-client): add, remove, and update project plugins as library calls`.

### Task 4: Install progress, and the editor depends on the client

**Files:**
- Modify: `crates/registry-client/src/project.rs` (`Environment`), `crates/registry-client/src/store.rs` (`InstallRequest`, download loop), `crates/registry-client/Cargo.toml` (remove `publish = false`), `crates/cli/src/session.rs` (`progress: None`)
- Modify: `crates/editor/Cargo.toml` (`celesta-registry-client.workspace = true`, `semver`, `time`; dev: `celesta-registry-client` with `features = ["testing"]`), root `Cargo.toml` workspace dependency entry if the editor uses `.workspace = true`

**Interfaces:**
- Produces: `pub struct Progress<'a> { pub id: &'a str, pub version: &'a Version, pub done: u64, pub total: u64 }`; `Environment` gains `pub progress: Option<&'a (dyn Fn(Progress) + Sync)>`; `InstallRequest` gains `pub progress: Option<&'a (dyn Fn(u64) + Sync)>` called with bytes read so far, at most once per 64 KiB and once at the end.

- [ ] **Step 1: Failing test** `install_reports_progress_up_to_the_size` in `store.rs`: install a test archive with a recording callback; the last value equals the archive size and values never decrease.
- [ ] **Step 2:** Run; fails (no field).
- [ ] **Step 3:** Implement; thread `env.progress` through `install_as_locked`.
- [ ] **Step 4:** `cargo test -p celesta-registry-client -p celesta-cli --locked` and `cargo build -p celesta-editor --locked` pass. Commit `feat(registry-client): report download progress; publish the crate` (body: the maintainer must publish the `0.0.0` placeholder and run `scripts/crates-trusted-publishing.sh`).

### Task 5: Discovery reads the trusted project's lock

**Files:**
- Modify: `crates/plugin-host/src/registry.rs` (`RegistryContext::merge_advisories`), `crates/editor/src/plugins.rs` (`FoundPlugins`)
- Test: `crates/plugin-host/src/registry.rs` tests, `crates/editor/src/plugins.rs` tests

**Interfaces:**
- Produces in plugin-host: `impl RegistryContext { pub fn merge_advisories(&mut self, other: AdvisoryList) }` (appends `other.advisories`; keeps the newer `generated`).
- Produces in the editor, `FoundPlugins` gains:
  - `pub(crate) project: Option<PathBuf>` — the saved-trusted project root with `celesta.config.json` (spec, "What counts as a project"), else `None`;
  - `pub(crate) lock: Option<Lockfile>` and `pub(crate) lock_problem: Option<LockProblem>` (from `check_lock` against `project_dependencies`);
  - `pub(crate) blocked: Vec<BlockedPlugin>` with `pub(crate) struct BlockedPlugin { pub id: String, pub version: String, pub advisory: String, pub title: String, pub fixed: Option<String>, pub bundled: bool }`, taken from `DiscoveryError::Blocked` before errors become strings (and still listed in `skipped`).
- `FoundPlugins::find(project: Option<&Path>, extra_advisories: Option<AdvisoryList>)`: builds `RegistryContext::current(project, &settings)`, merges `extra_advisories` (the override registry's cached list; Task 6 passes it), and calls `InstalledPlugins::find_in(&[], &project_paths, &settings, &registry)`. Callers in `lifecycle.rs` pass `None` until Task 6.

- [ ] **Step 1: Failing tests** in `plugins.rs` using a temp project with `celesta.config.json` (`plugins.dependencies`) and a store under a temp `XDG_DATA_HOME`-free path (inject the store through `RegistryContext` built in a `find_with_context` test seam):
  - `a_trusted_projects_locked_version_runs`: store holds `acme.charts 1.2.3` with the lock's tree hash → `installed` has it with `PluginSource::Locked`, `lock_problem` is `None`.
  - `an_untrusted_projects_lock_is_not_read`: same, project not trusted → no `Locked` entry, `project` is `None`.
  - `a_lock_out_of_date_is_a_lock_problem`: dependency without lock entry → `Some(LockProblem::OutOfDate { .. })`.
  - `a_blocked_plugin_is_listed_as_blocked`: advisory over a hand-placed plugin → one `BlockedPlugin` with its advisory id and `fixed`.
  - in plugin-host: `merged_advisories_stop_plugins_from_both_lists`.
- [ ] **Step 2:** Run `cargo test -p celesta-editor plugins:: -p celesta-plugin-host registry::`; new tests fail.
- [ ] **Step 3:** Implement. `FoundPlugins::problem()` adds the lock problem's text to its detail.
- [ ] **Step 4:** Tests pass; `cargo test -p celesta-editor --locked` passes. Commit `feat(editor): run a trusted project's locked plugins`.

### Task 6: `RegistryState`: the app's registry work

**Files:**
- Create: `crates/editor/src/registry/mod.rs`, `crates/editor/src/registry/client.rs`, `crates/editor/src/registry/state.rs`
- Modify: `crates/editor/src/main.rs` (module, create the entity once per window), `crates/editor/src/view.rs` (field `registry: Entity<RegistryState>`, kept across `replace_contents`), `crates/editor/src/lifecycle.rs` (pass the override list to `FoundPlugins::find`; tag opens with a generation), `crates/editor/src/problems.rs` (sources)

**Interfaces:**
- `client.rs`: `pub(crate) struct EditorRegistry { access: RegistryAccess, store: Store, cache: PathBuf, settings_path: PathBuf, celesta: Version, host: Target, bundled: Option<PathBuf> }` with `from_env() -> Result<Self, String>` (on a `SelectionError`, the official registry is used and the error is reported once under `Source::Registry`), `registry(&self) -> io::Result<Registry>`, `environment<'a>(&'a self, registry: &'a Registry, progress: Option<&'a (dyn Fn(Progress) + Sync)>) -> Environment<'a>`, `override_advisories(&self) -> Option<AdvisoryList>` (the override cache's verified list; `None` for the official registry). `EditorRegistry: Clone + Send + Sync` (wrap in `Arc`).
- `state.rs`:
  ```rust
  pub(crate) enum Work { InstallGlobal { version: Version }, Add, Pin, Update, Remove, Uninstall, Download }
  pub(crate) struct InFlight { pub work: Work, pub done: u64, pub total: u64 }
  pub(crate) enum Fetch<T> { Idle, Loading, Loaded { value: T, at: OffsetDateTime, offline: bool }, Failed(String) }
  pub(crate) enum RegistryEvent {
      /// Store or settings changed: rediscover; `project` is Some(root) when its lock changed, so it reloads.
      PluginsChanged { project: Option<PathBuf> },
      Failed { id: Option<String>, title: String, detail: String },
  }
  pub(crate) struct RegistryState {
      client: Arc<EditorRegistry>,
      pub(crate) include_prereleases: bool,           // initial: prereleases_by_default(celesta)
      pub(crate) search: Fetch<SearchList>,
      pub(crate) indexes: BTreeMap<String, Fetch<Index>>,
      pub(crate) in_flight: BTreeMap<String, InFlight>,
      pub(crate) failures: BTreeMap<String, String>,  // the page's red line, per id; cleared by the next attempt
      pub(crate) no_newer: BTreeSet<String>,          // "No newer version works with Celesta …" until the next check
      generation: u64,                                // bumped by `project_changed`
  }
  impl RegistryState {
      pub(crate) fn new(client: Arc<EditorRegistry>) -> Self;
      pub(crate) fn project_changed(&mut self, root: Option<PathBuf>); // bumps generation
      pub(crate) fn fetch_search(&mut self, cx: &mut Context<Self>);     // falls back to Registry's cached search list offline (Loaded { offline: true })
      pub(crate) fn fetch_index(&mut self, id: &str, cx: &mut Context<Self>);
      pub(crate) fn install_global(&mut self, id: &str, version: Version, cx: &mut Context<Self>) -> bool; // false: work for id already in flight
      pub(crate) fn add(&mut self, root: PathBuf, id: &str, version: Version, pin: bool, cx: &mut Context<Self>) -> bool; // requirement offered_requirement(&version)
      pub(crate) fn update(&mut self, root: PathBuf, ids: BTreeSet<String>, rewrites: Vec<Rewrite>, cx: &mut Context<Self>) -> bool;
      pub(crate) fn update_global(&mut self, id: &str, cx: &mut Context<Self>) -> bool; // offered version under include_prereleases
      pub(crate) fn remove(&mut self, root: PathBuf, id: &str, cx: &mut Context<Self>) -> bool;
      pub(crate) fn uninstall(&mut self, id: &str, cx: &mut Context<Self>) -> bool;     // drops the registryPlugins entry only
      pub(crate) fn download_locked(&mut self, root: PathBuf, cx: &mut Context<Self>) -> bool; // install_locked(offline=false)
      pub(crate) fn refresh_advisories(&mut self, cx: &mut Context<Self>); // emits PluginsChanged { project: None } when the verified list changed
      pub(crate) fn start_timers(&mut self, cx: &mut Context<Self>); // advisories now and every 24 h; search.json with it
  }
  impl EventEmitter<RegistryEvent> for RegistryState {}
  ```
- Each operation runs on the background executor, updates `in_flight[id].done/total` through the progress callback (marshalled to the entity with a channel polled by the spawned task, at most ~10 updates/s), and on completion emits `PluginsChanged` (or `Failed`) only when `generation` is unchanged; otherwise it drops the result silently (the files on disk are already consistent).
- `EditorView` subscribes once: `PluginsChanged { project }` → if `project` is the open project's root or `None`, `reload_plugins_action` (which rediscovers and reopens); `Failed` → `problems.report(Source::Registry, title, detail)`.
- `problems.rs` gains `Source::Registry` (label "Registry", severity Error, toasts), `Source::RegistryNotices` (label "Plugin updates", severity Warning, no toast), `Source::Advisories` (label "Advisories", severity Error, no toast: the notification is the announcement).

- [ ] **Step 1: Failing tests** (`#[gpui_kit::test]`, `TestRegistry::file()` and temp folders through a test constructor `EditorRegistry::for_test(served: &TestRegistry, user: &Path)`):
  - `install_global_records_the_version_and_emits`: `install_global("acme.codec", 1.0.0)` → after `run_until_parked`, settings' `registryPlugins` has it and one `PluginsChanged { project: None }` was emitted.
  - `work_on_an_id_in_flight_is_refused`: second `install_global` for the same id before parking returns `false`; only one emission.
  - `a_result_for_a_previous_project_is_dropped`: start `add(root_a, …)`, call `project_changed(Some(root_b))` before parking → no `PluginsChanged` emitted; `root_a`'s files are still written.
  - `offline_search_uses_the_cached_list`: fetch once, then make the served folder unreadable → `search` is `Loaded { offline: true, .. }`; with no cache → `Failed(..)`.
  - `a_failed_add_reports_and_clears_in_flight`: wrong hash → `Failed { id: Some("acme.charts"), .. }`, `in_flight` empty, `failures["acme.charts"]` set.
- [ ] **Step 2:** Run `cargo test -p celesta-editor registry::`; fails to compile.
- [ ] **Step 3:** Implement; wire into `main.rs`/`view.rs`/`lifecycle.rs` (`project_changed` on every successful open, with `FoundPlugins.project`).
- [ ] **Step 4:** Tests pass; `cargo test -p celesta-editor --locked` passes. Commit `feat(editor): run registry work in the background`.

### Task 7: The Plugins tab's sections

**Files:**
- Create: `crates/editor/src/plugins_tab_model.rs` (pure)
- Modify: `crates/editor/src/plugins_panel.rs` (render from the model; `PluginRow`/`PluginState` move into the model or are replaced)
- Test: unit tests in `plugins_tab_model.rs`; update `plugins_panel.rs` tests

**Interfaces:**
- Consumes: `FoundPlugins` (Task 5), `RegistryState` fields (Task 6), `UserSettings::registry_plugins`.
- Produces:
  ```rust
  pub(crate) struct TabModel { pub project: Option<ProjectSection>, pub global: Vec<TabRow>, pub by_hand: Vec<TabRow>, pub folders: Vec<PathBuf>, pub skipped: Vec<String> }
  pub(crate) struct ProjectSection { pub lock_label: String /* "celesta.lock · 0.10.0-alpha.3" or "No celesta.lock" */, pub action: Option<SectionAction>, pub rows: Vec<TabRow> }
  pub(crate) enum SectionAction { Download { count: usize }, UpdatePlugins, Working { done: u64, total: u64 } }
  pub(crate) struct TabRow { pub id: String, pub version: String, pub kind: &'static str /* "Media", "Content", "Components" */, pub state: RowState, pub action: Option<RowAction>, pub menu: Vec<MenuCommand>, pub directory: Option<PathBuf> }
  pub(crate) enum RowState { On, Off, Waiting { changed: bool, from_the_internet: bool }, Blocked { advisory: String }, Deprecated { replacement: Option<String> }, Yanked, NotInstalled, NoBuildHere, Bundled, NotUsed { runs: String }, Working { done: u64, total: u64 }, NoNewer { celesta: String } }
  pub(crate) enum RowAction { Update { to: Version }, Download, Allow { fingerprint: String } }
  pub(crate) enum MenuCommand { TurnOff, TurnOn, ShowFolder, ShowInFindPlugins, RemoveFromProject, Uninstall }
  pub(crate) fn tab_model(found: &FoundPlugins, registry: &RegistryState, settings: &UserSettings) -> TabModel;
  impl RowState { pub fn label(&self) -> String } // second-line text, e.g. "Media · Bundled · On", "Blocked · CELESTA-2026-0003"
  ```
- Copy (spec): headings `This project`, `Global`, `Installed by hand`, `Added folders`; toolbar `Find plugins…`, `Reload Plugins`, ⋯ menu `Add Folder…`, `Open Plugin Folder`; row actions `Update to {version}`, `Download`, `Allow`; menu `Turn Off`, `Turn On`, `Show Folder`, `Show in Find plugins`, `Remove from this project`, `Uninstall`; `No newer version works with Celesta {version}`; `Not used: {source} runs`.
- Rendering: one row = id + version (mono, muted) on line 1 with the action button trailing in a fixed lane, state on line 2; the ⋯ is a `DropdownMenu` on a ghost icon button (`IconName::Ellipsis`); hairline between rows; no switch, no path line. Ids: `("plugins-tab-update", id)`, `("plugins-tab-menu", id)`.

- [ ] **Step 1: Failing tests:** `locked_plugins_form_this_project` (Locked entries → project rows, lock label `celesta.lock · 0.10.0-alpha.3`), `no_project_section_without_a_saved_project`, `bundled_and_registry_copies_are_two_global_rows` (bundled one `NotUsed { runs: "Registry" }`), `an_update_shows_when_search_latest_is_newer_within_the_requirement` (`^0.2`, locked `0.2.1`, latest `0.2.3` → `Update { to: 0.2.3 }`; latest `0.3.0` → none), `a_missing_locked_version_offers_download_but_not_one_without_a_host_build` (Review Focus 4: missing host build → `NotInstalled` + `Download` counted; locked version with no host target → `NoBuildHere`, not counted), `blocked_rows_name_the_advisory`, `waiting_hand_placed_plugins_offer_allow`.
- [ ] **Step 2:** Run; fails.
- [ ] **Step 3:** Implement the model, then the rendering in `plugins_panel.rs`; menu commands call existing settings changes (`set_enabled`), `cx.open_with_system(dir)`, `RegistryState::remove/uninstall`, or open the plugin page on the id (Task 9's `open_plugin_page(Some(id))`; until Task 9 lands, `ShowInFindPlugins` is omitted from menus).
- [ ] **Step 4:** Tests pass; take a screenshot of the tab (see Task 11's procedure) to check alignment against the mockup. Commit `feat(editor): group the Plugins tab by project, global, and by hand`.

### Task 8: The plugin page model

**Files:**
- Create: `crates/editor/src/plugin_page/mod.rs`, `crates/editor/src/plugin_page/model.rs`
- Test: unit tests in `model.rs`

**Interfaces:**
- Consumes: Task 2 `offered_version`, `OfferScope`, `NoOffer`; Task 6 `RegistryState`; Task 5 `FoundPlugins`.
- Produces:
  ```rust
  pub(crate) struct ListRow { pub id: String, pub kind: &'static str, pub latest: Option<String>, pub prerelease: bool, pub note: Option<&'static str> /* "Installed", "In this project", "No build for this computer", "Deprecated" */ }
  pub(crate) fn list_rows(search: &SearchList, query: &str, found: &FoundPlugins, host: &Target) -> Vec<ListRow>; // id or description contains query, ignoring case; empty query → all; sorted by id
  pub(crate) fn list_footer(search: &Fetch<SearchList>, host: &str) -> String; // "4 plugins · registry.celesta.studio · updated 2 min ago" | "Offline · showing the list from {time}" | "Couldn’t reach {host}"
  pub(crate) struct Details { pub kind: String /* "Media plugin by celesta" */, pub latest: String, pub builds: Vec<String>, pub source: Option<String>, pub here: Vec<String> /* "Bundled 0.10.0-alpha.3 · not used" */, pub advisories: Vec<String>, pub versions: Vec<VersionRow> }
  pub(crate) struct VersionRow { pub version: String, pub state: &'static str /* Published Deprecated Yanked Withdrawn Blocked */, pub size: Option<String> /* host build, "1.2 MB" */ }
  pub(crate) fn details(index: &Index, found: &FoundPlugins, advisories: Option<&AdvisoryList>, host: &Target, include_prereleases: bool) -> Details;
  pub(crate) struct Footer { pub primary: FooterButton, pub secondary: Option<FooterButton>, pub reason: Option<String>, pub progress: Option<(u64, u64)>, pub failure: Option<String> }
  pub(crate) struct FooterButton { pub label: String, pub enabled: bool, pub command: PageCommand }
  pub(crate) enum PageCommand { Install(Version), Add(Version), Pin(Version), Update, None }
  pub(crate) fn footer(index: &Index, found: &FoundPlugins, registry: &RegistryState, host: &Target) -> Footer;
  ```
- Footer rules (spec, "The plugin page" and "Bundled and shadowed copies"): media → primary `Install`, secondary `Pin to this project` (only with `found.project`); other → primary `Add to this project`, disabled outside a project with reason `Add plugins to a project with celesta.config.json`; store already holds the offered version (global `registryPlugins` or the lock) → `Installed` disabled; an older one → `Update to {v}`; work in flight → `Installing…` disabled with progress; `NoOffer` reasons: `NoBuild` → `No build for this computer`, `NeedsCelesta` → `Needs Celesta {range}`, `OnlyPrereleases` → `Only pre-releases are available. Turn on Include pre-releases.`, `NothingPublished` → `No version can be installed`; failure line from `registry.failures[id]`.

- [ ] **Step 1: Failing tests:** `search_matches_id_and_description_ignoring_case`, `an_empty_query_lists_everything`, `the_footer_says_offline_and_when` and `the_footer_says_unreachable_without_a_cache` (Review Focus 5), `media_plugins_install_and_pin`, `add_is_disabled_outside_a_project_with_the_reason`, `a_bundled_copy_alone_keeps_install_available`, `the_store_copy_makes_it_installed`, `only_prereleases_explain_the_checkbox`, `work_in_flight_shows_installing_and_progress`, `the_version_table_hides_prereleases_when_excluded` and shows sizes from the host build (`1215625` bytes → `1.2 MB`).
- [ ] **Step 2:** Run; fails.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Tests pass. Commit `feat(editor): model the plugin page`.

### Task 9: The plugin page view and the widened Assets area

**Files:**
- Create: `crates/editor/src/plugin_page/view.rs`
- Modify: `crates/editor/src/export_control.rs` (`LeftPanel::Plugins`, `left_panel(react, export_open, plugins_open)`), `crates/editor/src/render.rs` (the row layout), `crates/editor/src/view.rs` (`plugin_page: PluginPageSlot`, `plugins_split: Option<Entity<ResizableState>>`), `crates/editor/src/main.rs` (create `plugins_split`), `crates/editor/src/lifecycle.rs` (keep the split across `replace_contents`), `crates/editor/src/plugins_panel.rs` (Find plugins… opens it), `crates/editor/src/actions.rs` (`FindPlugins` action, Plugins menu item `Find Plugins…`)
- Test: `#[gpui_kit::test]` tests in `view.rs`

**Interfaces:**
- Consumes: Task 8 model; Task 6 `RegistryState`.
- Produces: `pub(crate) struct PluginPage` (entity: `focus_handle`, `query: Entity<InputState>`, `selected: Option<String>`, `registry: Entity<RegistryState>`, `found: FoundPlugins` snapshot updated by `set_found`); `pub(crate) enum PluginPageEvent { Close }`; `PluginPageSlot` with `is_open()`, `open(&mut self, page)`, `hide()`, `page()`; `EditorView::open_plugin_page(&mut self, select: Option<String>, window, cx)` and `close_plugin_page`. Opening the plugin page closes the export page and the other way round.
- Layout (spec B): while open, the dock row's first panel is the page with its own `plugins_split`, `resizable_panel().size(rem * 40.0).size_range(rem * 32.0..rem * 56.0)` (comment as the export width's); inside, an `h_flex`: list column `w(rem * 16.0)` with a hairline trailing border, detail column `flex_1().min_w_0()`. List: header `Plugins`; `Input` (placeholder `Search plugins`); `Checkbox` `Include pre-releases` bound to `RegistryState.include_prereleases`; rows (selected = `list_active` fill); footer line. Detail: header id + ghost `Close`; field grid (`Kind`, `Latest`, `Builds`, `Source`, `Here`, `Advisories`); `Versions` table; pinned footer with primary/secondary buttons, the reason (muted) or failure (`danger`), progress text (mono) and a `Progress` bar. Nothing selected: muted `Select a plugin to see its versions`.
- Keyboard: the search input takes focus on open; Up/Down move the selection within the list; Enter runs the enabled primary command; Escape closes the page and returns focus to the editor (as the export page does).

- [ ] **Step 1: Failing UI tests** against `TestRegistry::file()` with plugins `acme.codec` (media, host build) and `acme.charts` (any): `opening_fetches_and_lists_the_registry`, `typing_filters_the_list`, `selecting_shows_details_and_the_footer`, `install_runs_and_the_button_becomes_installed` (click primary; `run_until_parked`; label `Installed`), `escape_closes_the_page`, `opening_the_export_page_closes_the_plugin_page`.
- [ ] **Step 2:** Run; fail.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Tests pass; `cargo test -p celesta-editor --locked`. Commit `feat(editor): find and install plugins from a plugin page`.

### Task 10: Opening a project, advisories, and Problems

**Files:**
- Create: `crates/editor/src/registry/questions.rs`, `crates/editor/src/registry/notices.rs`
- Modify: `crates/editor/src/lifecycle.rs` (after a successful open, `ask_about_lock` after `ask_about_pending_plugins`), `crates/editor/src/view.rs` (`asked_locks: BTreeSet<PathBuf>`, `announced_blocks: BTreeSet<(String, String, String)>`)

**Interfaces:**
- Produces:
  ```rust
  pub(crate) enum LockQuestion {
      Download { project: String, items: Vec<DownloadItem> },              // title "Download {n} plugins for “{project}”?" (1: "Download 1 plugin for …")
      OtherCelesta { project: String, locked: String, ids: Vec<String> },   // title "“{project}” was locked with Celesta {locked}"
  }
  pub(crate) struct DownloadItem { pub id: String, pub version: String, pub size: Option<u64> }
  /// Blocking (reads the store, tree-hashes). None when nothing is missing or the project is not a saved trusted one.
  pub(crate) fn lock_question(found: &FoundPlugins, store: &Store, bundled: Option<&Path>, host: &Target) -> Option<LockQuestion>;
  pub(crate) fn notices(found: &FoundPlugins, search: Option<&SearchList>, cached_index: impl Fn(&str) -> Option<Index>, stale: bool) -> Option<Problem>; // Deprecated/Yanked/stale → Source::RegistryNotices
  pub(crate) fn blocked_problem(found: &FoundPlugins) -> Option<Problem>;  // → Source::Advisories
  pub(crate) fn newly_blocked<'a>(found: &'a FoundPlugins, announced: &BTreeSet<(String, String, String)>) -> Vec<&'a BlockedPlugin>;
  ```
- Dialog copy (spec): download body `This project’s celesta.lock names plugins that are not on this computer. They come from {host}.`, table id / version (mono) / size (mono, `—` when unknown), `Total`, buttons `Not Now` (outline) and `Download` (primary). Other-Celesta body lists the ids; when `plan_update` returns rewrites, one line per rewrite `Changes {id} in celesta.config.json from {from} to {to}`; buttons `Not Now`, `Update Plugins`. Sizes and rewrites are fetched before the dialog opens with a 3-second budget (`registry_within`-style client with `timeout`); on timeout sizes are `—` and the rewrite list is computed when Update Plugins runs (and shown as a Problems entry after).
- Blocked notification: `window.push_notification` with title `{id} {version} was stopped`, body `{advisory}: {title}.` plus ` Fixed in {fixed}.` or, for a bundled one, ` Update Celesta to get a fixed version.`; action `Update to {fixed}` when fixed and not bundled (runs `RegistryState::update` for a locked id with `plan_update` rewrites, or `update_global`). Once per `(id, version, advisory)` per run.
- Problems copy: `{id} is deprecated` / detail `Use {replacement} instead. Replace it from the Plugins tab.` (no replacement: `Remove or replace it from the Plugins tab.`); `{id} {version} was yanked` / `It keeps working. Update it from the Plugins tab when you can.`; stale: `The advisory list is more than 7 days old` / `Celesta couldn’t check for blocked plugins recently. Check your connection.`; a registry copy (global or locked) that another source shadows: `{id} {version} from the registry is not used` / `The copy in {folder} runs instead.` (same `RegistryNotices` problem).

- [ ] **Step 1: Failing tests:** `a_missing_locked_version_is_asked_about`, `a_locked_version_without_a_host_build_is_not_asked_about` (Review Focus 4), `a_bundled_copy_satisfies_the_lock`, `another_celesta_minor_asks_to_update`, `the_question_is_asked_once_per_project` (view-level gpui test: open twice → one dialog), `a_new_block_is_announced_once`, `deprecated_and_yanked_become_one_warning`.
- [ ] **Step 2:** Run; fail.
- [ ] **Step 3:** Implement and wire: after `replace_contents`, compute the question on the background executor and open the dialog only if the open project is unchanged; `Download` → `RegistryState::download_locked`; `Update Plugins` → `RegistryState::update` with the planned rewrites; after each rediscovery set the three Problems sources and announce new blocks.
- [ ] **Step 4:** Tests pass; `cargo test -p celesta-editor --locked`. Commit `feat(editor): ask to download locked plugins and announce blocked ones`.

### Task 11: Docs, screenshots, and the live check

**Files:**
- Modify: `packages/website/content/en/docs/05-(more)/05-plugins.md`, `packages/website/content/ja/docs/05-(more)/05-plugins.md` (an "In the app" part of "Registry plugins": Find plugins…, Install/Add/Pin, Include pre-releases, updating, the download question, Blocked), `skills/celesta/references/plugins.md` (the app can do what `celesta plugin` does; the pre-release checkbox), `docs/handoff/2026-11-16-plugin-registry-app.md` (status → built; Implementation table; Checks table with real results; Known gaps), `docs/handoff/2026-11-14-plugin-registry-design.md` (status line: plan 5 built), `docs/handoff/2026-11-14-celesta-command.md` ("Later plans": plan 5 done), `HANDOFF.md` (workspace map rows for the new editor modules if it lists editor modules)

- [ ] **Step 1:** Screenshots, same scene before and after: the "before" is the scratchpad's `before-1.png` (Plugins tab, `main` build); "after": the Plugins tab, the plugin page with `avfoundation` selected, the download question (open a temp project with a lock on `acme.charts` against a `file://` test registry: launch with `CELESTA_REGISTRY` and `CELESTA_REGISTRY_ROOT_KEY` set and a temporary `HOME`), the Blocked notification. Capture with `screencapture -x -o -l <window id>` (window id from a `CGWindowListCopyWindowInfo` Swift one-liner, owner `celesta-editor`); the user clicks trust dialogs. Save under `docs/handoff/images/2026-11-16-plugin-registry-app/` only if the repo keeps images there; otherwise attach to the PR with `gh pr edit --attach`.
- [ ] **Step 2:** Live check: in a dev build with the user's real folders, open the page, select `avfoundation`, Install (`0.10.0-alpha.3`, pre-releases on by default). Then `target/debug/celesta plugin list` shows it under global. Record the result.
- [ ] **Step 3:** Write the docs; run `pnpm --dir packages/website run build`.
- [ ] **Step 4:** Commit `docs: describe registry plugins in the app`.

### Task 12: Final verification and the pull request

- [ ] **Step 1:** `cargo build -p celesta-codec-test --locked`, then `cargo test --workspace --locked --no-fail-fast` (run in the background; record pass/fail counts), `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo fmt --all -- --check`, `git diff --check`. All pass except the known timing check noted in the plan 4 note.
- [ ] **Step 2:** Push the branch and open the PR (title `feat: find, install, and update registry plugins in the app`), body: summary, before/after screenshots, the checks table, and **Maintainer steps before merging**: publish `celesta-registry-client` `0.0.0` to crates.io and run `scripts/crates-trusted-publishing.sh`; until then `Crate names` fails. End with the attribution line.
- [ ] **Step 3:** Bind the PR with the `ccd_pr` tools and report CI.
