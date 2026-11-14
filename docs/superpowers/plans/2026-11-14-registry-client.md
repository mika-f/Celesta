# Registry client core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Plan 3 of the plugin registry: the client core. Projects declare and lock registry plugins, a per-user store holds verified copies, a resolver picks versions, signed index and advisory files are fetched and cached behind a transport trait, and discovery in `celesta-plugin-host` runs locked and global store plugins and stops plugins under an advisory everywhere.

**Architecture:** Load-time pieces live in `celesta-plugin-host`, because the editor, exporter, MCP server, and React bridge already discover plugins through it: `plugins.dependencies`/`plugins.targets`, `celesta.lock`, store and cache paths, `registryPlugins` in `settings.json`, the cached advisory list, and the new discovery sources. Network-facing pieces go in a new crate, `celesta-registry-client` (`crates/registry-client`): the `Transport` trait, fetch, verify, and cache, the resolver, the store installer, and the project operations that plans 4 and 5 call. `celesta-registry-format` gets the three known gaps fixed. Dependencies run one way: `registry-client` → `plugin-host` → `registry-format`.

**Tech Stack:** Rust 2024 (MSRV 1.89), `semver`, `serde`/`serde_json`, `sha2`, `time`, `tempfile`, a new `zip` dependency (reading only), `ureq` through `celesta-remote`.

**Spec:** `docs/handoff/2026-11-14-plugin-registry-design.md` (Client, States, Client-facing files, Security summary), `crates/registry-format/README.md`, `docs/handoff/2026-11-14-registry-format.md` (Known gaps for later plans).

## Global Constraints

- The `celesta` command, the desktop app's UI, publishing, and the migration of four packages are plans 4 to 6+. Nothing in this plan calls the network from the editor, exporter, or MCP server, and no existing caller changes behaviour except through advisories and `registryPlugins`, which `InstalledPlugins::find*` picks up by itself.
- `mika-f/Celesta` is public. Never name or describe the registry service's repository or internals in code, docs, commits, or PRs.
- Tests: `cargo test` in Rust. No network in tests: every test uses `DirTransport` or a `127.0.0.1` listener.
- Requirements use Cargo semantics (`semver::VersionReq`: `"0.2"` is `^0.2`; a pre-release matches only when the requirement names one).
- Project targets default to `macos-aarch64`, `windows-x86_64`, `linux-x86_64`.
- Store: `<user data>/Celesta/registry/<id>/<version>/` = `user_plugin_dir()`'s parent joined with `registry` (so `celesta/registry` on Linux). Created 0700 on Unix with `create_private_dir_all`.
- Advisories: refuse a list more than 10 minutes in the future and one older than the cached list; warn when the newest verified list's `generated` is older than 7 days; a cached verified list stays in force offline and after its key expires.
- Yanked versions are skipped by resolution but locked ones are kept. Withdrawn: skipped, and a missing locked copy fails with "no longer distributed". Blocked: never resolved, never run, from any source.
- Never amend commits. Commits are signed through 1Password: on "failed to fill whole buffer", stop and ask the user to unlock it. End each commit message with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Every task ends with `cargo test -p <crates it touched> --locked`, `cargo clippy -p <crates> --all-targets --locked -- -D warnings`, and `cargo fmt --all -- --check` passing.

## Decisions this plan makes (for the maintainer to confirm)

1. **`celesta-registry-format` becomes publishable** (`publish = false` removed in Task 3), since `celesta-plugin-host` is published and now depends on it. The maintainer must publish a `0.0.0` placeholder for `celesta-registry-format` and run `scripts/crates-trusted-publishing.sh` before the next release (see `docs/handoff/2026-11-14-crates-io-trusted-publishing.md`). `celesta-registry-client` stays `publish = false`; it needs the same setup once the editor depends on it (plan 5).
2. **Rollback stamps are a format change**: `config.json` and `index/<id>.json` gain a required `generated` timestamp (search.json and advisories.json have one). The service syncs fixtures from a pinned commit, so it must bump the pin and emit the field.
3. **Search order** becomes: Explicit (`--plugin-dir`, `CELESTA_PLUGIN_PATH`), Locked, Project, User, Registry (global), Folder, Bundled. Locked comes after Explicit rather than first: an explicit folder is the caller's choice for one run, and plugin developers test a build that way.
4. **An entry without `celesta.config.json` uses global registry plugins only**, not "the newest store version of each id": a store version installed only for another project was never chosen to run here (anti-goal: nothing runs that the user did not decide to run).
5. **Install metadata lives in what chose the plugin**: the lock records each target's `treeHash`, `registryPlugins` in `settings.json` records the global one's. The store holds only plugin directories.
6. **No root key is compiled in yet.** `official_root_key()` returns `None` until the maintainer supplies the root public key; every client API takes a `RootKey`, and tests use the fixture root.
7. **Cache location**: `<user data>/Celesta/registry-cache/` (the store's sibling), not an OS cache folder, because purging a cache would drop the advisory list.

## Review Focus

1. A crash between extraction and rename leaves `registry/.tmp-*` behind: discovery must ignore it and the next install must succeed (Task 10 test `a_stale_temporary_folder_is_ignored`).
2. A hand-placed plugin whose manifest `version` is not semver (`"1"`) and whose id has an advisory: it must be stopped, not run (fail closed; Task 6 test `an_unreadable_version_with_an_advisory_is_blocked`).
3. A `celesta.lock` from a newer Celesta with unknown fields: read, and the fields kept on rewrite; an unknown `format` fails naming it (Task 5 tests).
4. The registry unreachable or the clock off so the fetched list cannot be verified: the cached list keeps blocking (Task 8 test `a_failed_refresh_keeps_the_cached_list`).
5. Two installs of the same version racing, or an occupied but tampered store folder: the good copy wins, never a half one (Task 10 tests `a_correct_existing_copy_is_kept`, `a_tampered_existing_copy_is_replaced`).

---

### Task 1: Keyring re-derives from the raw strings (gap 1)

**Files:**
- Modify: `crates/registry-format/src/keyring.rs` (`Keyring::from_config`)
- Test: `crates/registry-format/tests/signing.rs`

**Interfaces:**
- Produces: `Keyring::from_config` decodes `OnlineKey::public_b64` and parses `OnlineKey::expires_text` itself and ignores `public`/`expires`. A key whose strings do not decode or parse is skipped. Public types unchanged.

- [ ] **Step 1: Write the failing tests** in `tests/signing.rs`:
  - `from_config_ignores_a_changed_public`: load `fixtures/signed/config.json`, overwrite one valid key's `public` with `[0; 32]`, build the keyring at the fixture's "now", and assert a file signed by that key still verifies.
  - `from_config_ignores_a_changed_expiry`: take the expired key, set `expires` to `now + 1 year` in code (leave `expires_text`), assert the keyring does not hold it (verifying a file it signed is `UnknownKey`).
- [ ] **Step 2:** `cargo test -p celesta-registry-format --test signing` fails on both.
- [ ] **Step 3:** Implement. Doc comment on `OnlineKey` states that `public` and `expires` are derived conveniences and that trust decisions use the raw strings.
- [ ] **Step 4:** Tests pass.
- [ ] **Step 5: Commit** `fix(registry-format): derive trusted keys from config.json's own strings`.

### Task 2: Rollback stamps on config and index, and the junk-name helper

**Files:**
- Modify: `crates/registry-format/src/files.rs` (`Config`, `Index`), `src/advisories.rs`, `src/tree.rs`, `src/lib.rs`, `schema/v1/config.schema.json`, `schema/v1/index.schema.json`, `fixtures/valid/*config*`, `fixtures/valid/index-*.json`, `fixtures/signed/` (regenerated), `README.md`
- Create: `fixtures/invalid/config-missing-generated.json`, `fixtures/invalid/index-missing-generated.json`
- Test: `tests/parse.rs`, `tests/advisories.rs`, `tests/tree.rs`

**Interfaces:**
- Produces:
  - `Config.generated: OffsetDateTime` and `Index.generated: OffsetDateTime`, required, strict timestamp (same `timestamp` serde module as `SearchList`).
  - `pub fn check_fresh(generated: OffsetDateTime, cached: Option<OffsetDateTime>, now: OffsetDateTime) -> Result<(), FormatError>` in `advisories.rs`, re-exported: `FutureDated` when `generated > now + 10 min`, `Rollback` when `generated < cached`. `AdvisoryList::check_newer` delegates to it.
  - `pub fn is_os_junk_name(name: &str) -> bool` in `tree.rs`, re-exported (the existing `is_junk`, made public).

- [ ] **Step 1: Write failing tests**: `config_and_index_need_generated` (both invalid fixtures refused by the schema test and the parser, through the existing `invalid` harness); `check_fresh_matches_check_newer` (equal accepted, 10 min ahead accepted, 10 min + 1 s refused `FutureDated`, older refused `Rollback`, `None` cached passes); `junk_names_are_recognised` (`.DS_Store`, `THUMBS.DB`, `Desktop.ini` true; `desktop.ini.txt`, `DS_Store` false).
- [ ] **Step 2:** They fail.
- [ ] **Step 3:** Implement; add `generated` to the valid fixtures (`2026-11-20T00:00:00Z`), the schemas (`required`, the existing timestamp pattern), and regenerate `fixtures/signed/` with `cargo run -p celesta-registry-format --example sign-fixtures --locked` (run twice; the second run changes nothing). README: `generated` rows under `config.json` and `index/<id>.json`; a "Rollback stamps" paragraph saying clients refuse a served file whose `generated` is older than the newest they verified at that path, and that the registry sets `generated` on every regeneration. Replace the third known gap in `docs/handoff/2026-11-14-registry-format.md` with a pointer to this plan.
- [ ] **Step 4:** `cargo test -p celesta-registry-format --locked` passes.
- [ ] **Step 5: Commit** `feat(registry-format): stamp config.json and index files with generated`.
- [ ] **Step 6:** Write `registry-format-generated.md` in the shared notes folder (`/Users/natsuneko/Library/Application Support/Claude/side-session-notes/local_6852df62-b159-430e-96c8-4c3825f82ecb/`) for the service session: the commit, the two new required fields, and that the sync pin must move.

### Task 3: plugin-host depends on the format; manifest paths ending in a junk name are refused (gap 2)

**Files:**
- Modify: `crates/registry-format/Cargo.toml` (drop `publish = false`), `crates/plugin-host/Cargo.toml` (add `celesta-registry-format.workspace = true`; add `semver`'s `serde` feature), root `Cargo.toml` (`[workspace.dependencies] celesta-registry-format = { path = "crates/registry-format", version = "=<workspace version>" }`, as the other crates are listed), `Cargo.lock`, `crates/plugin-host/src/manifest.rs` (`invalid_reason`)
- Test: `crates/plugin-host/src/manifest.rs` tests

**Interfaces:**
- Consumes: `celesta_registry_format::is_os_junk_name` (Task 2).
- Produces: `invalid_reason` refuses any `executable`, `module`, or `types` path whose last component (split on `/` and `\`) `is_os_junk_name`, with reason `"{path:?} names a file the tree hash leaves out (.DS_Store, Thumbs.db, desktop.ini)"`. Applies to every source.

- [ ] **Step 1: Failing test** `manifest_paths_ending_in_a_junk_name_are_refused`: plugins with `"module": "desktop.ini"`, `"module": "js/Thumbs.db"`, and `"executable": {"linux": "bin\\.DS_Store"}` are each `DiscoveryError::Invalid` with that reason; `"module": "js/desktop.ini.js"` is accepted.
- [ ] **Step 2:** It fails.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `cargo test -p celesta-plugin-host --locked` passes; `cargo package -p celesta-registry-format --locked --allow-dirty --no-verify` lists `schema/` and `fixtures/` (so its tests run from the package).
- [ ] **Step 5: Commit** `feat(plugin-host): refuse manifest paths the tree hash would skip`.

### Task 4: `plugins.dependencies` and `plugins.targets`

**Files:**
- Modify: `crates/plugin-host/src/project.rs`, `src/lib.rs`

**Interfaces:**
- Produces:
  ```rust
  pub const RELEASE_TARGETS: [&str; 3] = ["macos-aarch64", "windows-x86_64", "linux-x86_64"];
  pub struct ProjectDependencies {
      pub dependencies: BTreeMap<String, semver::VersionReq>,
      /// `plugins.targets` as written, or `None` when absent.
      pub targets: Option<Vec<Target>>,
  }
  impl ProjectDependencies {
      /// `targets`, else RELEASE_TARGETS; sorted, deduplicated.
      pub fn project_targets(&self) -> Vec<Target>;
  }
  pub fn project_dependencies(root: &Path) -> Result<ProjectDependencies, ProjectConfigError>;
  ```
  New `ProjectConfigError` variants: `BadDependency { path, id, reason: String }` (id not `is_registry_id`, or requirement not a `VersionReq`), `BadTarget { path, target: String }`, `NoTargets { path }` (an empty `plugins.targets`). No file → empty dependencies, `targets: None`. `ProjectPlugins` gains the two fields; `project_plugin_paths` is unchanged.

- [ ] **Step 1: Failing tests** in `project.rs`: `dependencies_are_read` (the design's example: `vrm` → `^0.2`, `ffmpeg-x264` → `=0.9.0`); `targets_default_to_the_release_targets`; `given_targets_are_used` (`["linux-x86_64", "any"]` → both); `bad_entries_are_reported` (`"VRM"`, `"a.b.c"`, `"0.2 || 0.3"`, `"plugins": {"targets": []}`, `"targets": ["Linux"]`, each the matching variant).
- [ ] **Step 2:** They fail.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Pass.
- [ ] **Step 5: Commit** `feat(plugin-host): read plugins.dependencies and plugins.targets`.

### Task 5: `celesta.lock`

**Files:**
- Create: `crates/plugin-host/src/lock.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Produces:
  ```rust
  pub const LOCK_FILE: &str = "celesta.lock";
  pub const LOCK_FORMAT: u32 = 1;
  pub struct Lockfile {
      pub celesta: semver::Version,              // the Celesta it was resolved with
      pub targets: Vec<Target>,                  // project targets, sorted
      pub plugins: BTreeMap<String, LockedPlugin>,
      pub other: serde_json::Map<String, Value>, // unknown keys, kept
  }
  pub struct LockedPlugin {
      pub version: semver::Version,
      pub registry: String,                      // base URL, e.g. https://registry.celesta.studio
      pub celesta: semver::VersionReq,           // the version's range, so a minor upgrade is seen offline
      pub targets: BTreeMap<Target, LockedArtifact>, // every target the version has
      pub other: serde_json::Map<String, Value>,
  }
  pub struct LockedArtifact { pub sha256: Hash, pub tree_hash: Hash }   // JSON: sha256, treeHash
  pub enum LockError { Read { path, source }, Parse { path, source }, Format { path, found: u64 }, Write { path, source } }
  impl Lockfile {
      pub fn parse(bytes: &[u8], path: &Path) -> Result<Self, LockError>;
      pub fn load(root: &Path) -> Result<Option<Self>, LockError>;   // None when absent
      pub fn to_bytes(&self) -> Vec<u8>;
      pub fn save(&self, root: &Path) -> Result<(), LockError>;      // temp file beside, renamed over
      /// Ids whose locked range leaves `celesta` out, sorted.
      pub fn unloadable(&self, celesta: &semver::Version) -> Vec<String>;
      /// The artifact for `host`, else for `any`.
      pub fn artifact(&self, id: &str, host: &Target) -> Option<&LockedArtifact>;
  }
  ```
  On disk: JSON, `"format": 1` first, two-space indentation, keys sorted (BTreeMap and `serde_json`'s map order), one trailing newline. `Format` error text: `"{path} was written in lock format {found}, which this Celesta {CELESTA_VERSION} does not read; update Celesta"`.

- [ ] **Step 1: Failing tests** in `lock.rs`: `a_lock_round_trips` (build one with two plugins, `to_bytes` → `parse` equal, and `to_bytes` again byte-identical); `the_bytes_are_stable` (assert the exact text of a one-plugin lock, so formatting is pinned); `unknown_fields_are_kept` (top-level and per-plugin unknown keys survive parse → to_bytes); `a_newer_format_fails_naming_it` (`"format": 2` → `Format { found: 2 }`, message contains `lock format 2`); `crlf_is_read`; `unloadable_lists_ids_outside_their_range` (`>=0.9.0, <0.10.0` against `0.10.0` listed, against `0.9.5` not); `artifact_falls_back_to_any`.
- [ ] **Step 2:** They fail.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Pass.
- [ ] **Step 5: Commit** `feat(plugin-host): read and write celesta.lock`.

### Task 6: Store and cache paths, `registryPlugins`, cached advisories, and blocking in discovery

**Files:**
- Create: `crates/plugin-host/src/advisories.rs`
- Modify: `src/locations.rs`, `src/settings.rs`, `src/manifest.rs` (`discover_in_with`, `DiscoveryError`), `src/lib.rs`

**Interfaces:**
- Produces:
  ```rust
  pub fn registry_store_dir() -> Option<PathBuf>;   // user_plugin_dir()?.parent()/registry
  pub fn registry_cache_dir() -> Option<PathBuf>;   // user_plugin_dir()?.parent()/registry-cache
  pub const ADVISORIES_CACHE: &str = "advisories.json";  // in registry_cache_dir()
  // settings.rs
  pub struct RegistryPlugin { pub id: String, pub version: semver::Version, pub tree_hash: Hash } // JSON camelCase
  UserSettings.registry_plugins: Vec<RegistryPlugin>      // #[serde(default)], key "registryPlugins"
  // advisories.rs
  /// The verified list the client cached; None when absent. A file other users can change is an error.
  pub fn load_cached_advisories(path: &Path) -> Result<Option<AdvisoryList>, String>;
  pub fn cached_advisories() -> (Option<AdvisoryList>, Option<String>);   // at the standard path
  /// Some(age) when the list's `generated` is more than 7 days before `now`.
  pub fn advisories_stale(list: &AdvisoryList, now: OffsetDateTime) -> Option<time::Duration>;
  /// The advisory that stops `entry`: by manifest id, and version parsed as semver;
  /// an unparsable version with any advisory for its id fails closed (the first one).
  pub fn blocking_advisory<'a>(list: &'a AdvisoryList, entry: &PluginEntry) -> Option<&'a Advisory>;
  // manifest.rs
  DiscoveryError::Blocked { path: PathBuf, id: String, version: String, advisory: String, title: String, fixed: Option<String> }
  ```
  `Blocked`'s message: `"{path} was stopped: the plugin {id} {version} has a security advisory ({advisory}: {title}){}"`, the tail `"; {fixed} fixes it"` when `fixed` is set. A blocked plugin is treated as turned off: it takes no id, so the next plugin with its id is found in its place, and it never reaches `InstalledPlugins::plugins`. `find_with` loads the cached list with `cached_advisories()` and reports its error as `DiscoveryError::Settings`. `discover_in_with` takes the list as a parameter (`Option<&AdvisoryList>`); `discover_in` passes `None`.

- [ ] **Step 1: Failing tests**: `the_store_and_cache_sit_beside_the_user_folder` (per OS, via the existing `user_plugin_dir_from` pattern; add `registry_dir_from(os, var, name)`); `registry_plugins_round_trip_in_settings` (save, load, equal; an older settings file without the key loads empty); `a_blocked_plugin_gives_way` (two folders with id `p` 1.0.0 and 1.1.0, an advisory `<1.1.0`: the 1.1.0 one is found, `Blocked` reported for 1.0.0); `bundled_and_explicit_plugins_are_blocked_too`; `an_unreadable_version_with_an_advisory_is_blocked` (`"version": "1"`); `stale_lists_are_noticed` (8 days → `Some`, 6 days → `None`); `a_cache_others_can_write_is_refused` (Unix, 0o666 file).
- [ ] **Step 2:** They fail.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `cargo test -p celesta-plugin-host --locked` passes.
- [ ] **Step 5: Commit** `feat(plugin-host): stop plugins under a cached advisory, from any source`.

### Task 7: Store sources in discovery: locked and global plugins, the approval rule

**Files:**
- Create: `crates/plugin-host/src/registry.rs`
- Modify: `src/locations.rs` (`PluginSource`), `src/approval.rs` (`needs_allowance`), `src/manifest.rs` (`PluginEntry`, `discover_in_with`, `InstalledPlugins::find_with`), `src/media.rs` (`MediaOrigin` mapping), `src/lib.rs`; every struct literal of `PluginEntry` in `crates/editor`, `crates/gpu-renderer`, `crates/react-bridge`, and `crates/codec-test` (add `replaces: None`)

**Interfaces:**
- Consumes: Tasks 4–6.
- Produces:
  ```rust
  // PluginSource, in this order (derived Ord is search order):
  Explicit, Locked, Project, User, Registry, Folder, Bundled
  // labels: "locked by project", "installed from the registry"; is_checked: true; reports_missing: false;
  // needs_allowance: false (their check is the tree hash, below)
  PluginEntry.replaces: Option<PluginSource>   // the source of the first later entry with its id it shadowed
  impl PluginEntry { pub fn media_origin(&self) -> MediaOrigin; }
  // Locked → replaces.map(MediaOrigin::from).unwrap_or(User); Registry → User; others as today.
  pub struct ProjectLock { pub root: PathBuf, pub lock: Lockfile }
  pub struct RegistryContext {
      pub store: Option<PathBuf>,
      pub advisories: Option<AdvisoryList>,
      /// A trusted project's lock; None for an untrusted project or no lock.
      pub project: Option<ProjectLock>,
  }
  impl RegistryContext {
      /// Store, cached advisories, and `project`'s lock when `settings` trusts it.
      pub fn current(project: Option<&Path>, settings: &UserSettings) -> (Self, Vec<DiscoveryError>);
  }
  impl InstalledPlugins {
      pub fn find_in(explicit: &[PathBuf], project: &[PathBuf], settings: &UserSettings,
                     registry: &RegistryContext) -> (Self, Vec<DiscoveryError>);
  }
  // find_with(explicit, project, settings) = find_in(.., &RegistryContext::current(None, settings).0)
  DiscoveryError::LockUnsatisfied { id: String, version: String, reason: String }
  DiscoveryError::TreeHashMismatch { path: PathBuf, id: String }
  ```
  Rules:
  - Locked search paths: for each lock entry, `store/<id>/<version>` with source `Locked`, in id order. Registry search paths: each `registryPlugins` entry's `store/<id>/<version>`, source `Registry`.
  - A `Locked` or `Registry` entry runs only when `tree_hash(directory)` equals the expected hash (the lock's artifact for `Target::host()` else `any`; the settings entry's `tree_hash`) and its manifest's `id`/`version` equal the entry's. Otherwise `TreeHashMismatch` (text: `"{path} was skipped: its files changed since it was installed from the registry; install {id} again"`), and it takes no id.
  - A locked entry missing from the store is satisfied by a `Bundled` entry with that id and version whose tree hash equals the locked one: that entry is re-tagged `Locked` (with `replaces: Some(Bundled)`) and ranks first for its id. Otherwise `LockUnsatisfied` with reason `"it is not installed"`, `"it has no build for <host target>"` (no host and no `any` artifact), or `"its installed files differ from the lock"`, and the id is found from the other sources.
  - A `Locked` entry may replace a bundled media plugin (unlike `Project`).
  - A pin only decides which version of its id runs: component defaults still follow the origin and `media.priority` order, with the pinned entry ranked as the origin it replaced (`media_origin()`).

- [ ] **Step 1: Failing tests** in `registry.rs` (helpers: write a plugin dir, hash it with `tree_hash`, write a lock and settings in temp dirs, build `RegistryContext` by hand):
  - `a_locked_version_ranks_first` (user folder has `p` 1.0.0, store has locked `p` 0.9.0: 0.9.0 runs, `replaces == Some(User)`).
  - `a_pinned_bundled_media_plugin_keeps_its_origin` (locked media `ffmpeg` 0.9.0 over bundled 0.9.2: runs, `media_origin() == Bundled`).
  - `explicit_folders_still_come_first`.
  - `a_bundled_copy_satisfies_the_lock` (store empty, bundled `p` 0.9.0 with the locked tree hash → source `Locked`).
  - `a_tampered_store_copy_is_skipped` (append a byte to a file → `TreeHashMismatch`, the bundled one runs).
  - `an_untrusted_projects_lock_is_ignored` (`RegistryContext::current` with an untrusted root → `project == None`).
  - `a_missing_locked_plugin_is_reported` (`LockUnsatisfied` "it is not installed"; the user folder's copy runs).
  - `no_build_for_this_target_falls_back` (lock has only `other-target`).
  - `global_registry_plugins_rank_after_the_user_folder`.
  - `a_blocked_locked_plugin_does_not_run` (advisory on the locked version).
  - Keep `search_paths_come_in_order` passing with the new variants.
- [ ] **Step 2:** They fail.
- [ ] **Step 3:** Implement. `Allowed`/approval: `needs_allowance` returns false for both new sources.
- [ ] **Step 4:** `cargo test -p celesta-plugin-host -p celesta-editor -p celesta-gpu-renderer -p celesta-react-bridge -p celesta-codec-test --locked` passes.
- [ ] **Step 5: Commit** `feat(plugin-host): run locked and global registry plugins from the store`.

### Task 8: `celesta-registry-client`: transport, verified fetch, cache, rollback stamps

**Files:**
- Create: `crates/registry-client/Cargo.toml` (`publish = false`; deps: `celesta-plugin-host`, `celesta-registry-format`, `semver`, `serde`, `serde_json`, `sha2`, `tempfile`, `thiserror`, `time`), `src/lib.rs`, `src/transport.rs`, `src/cache.rs`, `src/registry.rs`, `src/error.rs`, `tests/fetch.rs`
- Modify: root `Cargo.toml` (member + workspace dependency), `Cargo.lock`

**Interfaces:**
- Consumes: `Keyring::from_signed_config`, `Keyring::verify`, `Index::parse`, `SearchList::parse`, `AdvisoryList::parse`, `check_fresh`, `RootKey`; `registry_cache_dir`, `ADVISORIES_CACHE`, `create_private_dir_all`, `unsafe_reason`.
- Produces:
  ```rust
  pub const OFFICIAL_REGISTRY: &str = "https://registry.celesta.studio";
  pub fn official_root_key() -> Option<RootKey>;   // None until the maintainer supplies it
  pub const MAX_JSON_BYTES: u64 = 16 * 1024 * 1024;
  pub trait Transport: Send + Sync {
      /// The body at `url`; NotFound for a 404.
      fn open(&self, url: &str) -> Result<Box<dyn std::io::Read + Send + '_>, TransportError>;
  }
  pub enum TransportError { NotFound(String), Failed { url: String, message: String } }
  /// Serves `<base><path>` from `<root><path>`, for tests and local registries.
  pub struct DirTransport { pub base: String, pub root: PathBuf }
  pub struct Registry { /* base, root key, transport, cache dir */ }
  impl Registry {
      pub fn new(base: &str, root: RootKey, transport: Box<dyn Transport>, cache: PathBuf) -> Self;
      pub fn base(&self) -> &str;
      pub fn keyring(&self, now: OffsetDateTime) -> Result<Keyring, ClientError>;
      pub fn index(&self, id: &str, now: OffsetDateTime) -> Result<Index, ClientError>;
      pub fn cached_index(&self, id: &str) -> Option<Index>;
      pub fn search(&self, now: OffsetDateTime) -> Result<SearchList, ClientError>;
      /// Fetches, verifies, checks against the cached list, and caches advisories.json.
      pub fn refresh_advisories(&self, now: OffsetDateTime) -> Result<AdvisoryList, ClientError>;
  }
  pub enum ClientError { Transport(TransportError), Format { path: String, source: FormatError }, TooLarge { url: String }, Cache { path: PathBuf, message: String }, NoSuchPlugin(String) }
  ```
  Each fetch reads `<file>` and `<file>.sig` (at most `MAX_JSON_BYTES` each), verifies at the served path (`/config.json`, `/index/<id>.json`, `/search.json`, `/advisories.json`) with a keyring built from a freshly verified `config.json`, checks `check_fresh` against the newest `generated` recorded for that path in `<cache>/stamps.json`, then writes the bytes to `<cache>/<path>` and the stamp, each through a temp file renamed over (dir 0700, files 0600). `refresh_advisories` writes `<cache>/advisories.json` — the file `cached_advisories()` reads. Cached files are read back without re-verifying (they were verified when fetched).

- [ ] **Step 1: Failing tests** in `tests/fetch.rs`, serving a temp copy of `crates/registry-format/fixtures/signed/` with `DirTransport` and the fixture root key (paths found through `CARGO_MANIFEST_DIR`):
  - `files_verify_and_are_cached` (index of the fixture plugin; `cached_index` returns it afterwards with the transport's root deleted).
  - `a_bad_signature_is_refused` (flip a byte of the index → `Format { source: Signature }`, nothing cached).
  - `an_older_file_is_a_rollback` (record a later stamp for `/index/<id>.json` first → `Rollback`).
  - `a_future_list_is_refused` (`now` 11 minutes before `generated` → `FutureDated`).
  - `a_failed_refresh_keeps_the_cached_list` (cache once, then a transport that fails: `refresh_advisories` errs and `load_cached_advisories` still returns the list).
  - `an_expired_key_does_not_drop_the_cache` (cache at 2026-11-20, refresh at 2100-01-01, after the `2026-11` key's 2099-01-01 expiry → error; cache unchanged and still loaded).
  - `oversized_files_are_refused` (a transport returning `MAX_JSON_BYTES + 1` bytes → `TooLarge`).
- [ ] **Step 2:** They fail (crate missing).
- [ ] **Step 3:** Implement. If the signed fixtures lack an index that `Index::parse` accepts for some id, use the id the fixture index carries.
- [ ] **Step 4:** `cargo test -p celesta-registry-client --locked` passes.
- [ ] **Step 5: Commit** `feat(registry-client): fetch, verify, and cache registry files`.

### Task 9: Resolution

**Files:**
- Create: `crates/registry-client/src/resolve.rs`, `tests/resolve.rs`

**Interfaces:**
- Consumes: `Index`, `VersionEntry`, `AdvisoryList`, `Lockfile`, `LockedPlugin`, `LockedArtifact`, `Target`.
- Produces:
  ```rust
  pub enum Update { Keep, All, Only(BTreeSet<String>) }
  pub struct ResolveInput<'a> {
      pub dependencies: &'a BTreeMap<String, VersionReq>,
      pub indexes: &'a BTreeMap<String, Index>,
      pub advisories: Option<&'a AdvisoryList>,
      pub celesta: &'a Version,
      pub targets: &'a [Target],   // project targets
      pub host: &'a Target,
      pub registry: &'a str,
      pub previous: Option<&'a Lockfile>,
      pub update: &'a Update,
  }
  pub struct Resolution { pub lock: Lockfile, pub warnings: Vec<ResolveWarning> }
  pub enum ResolveWarning {
      MissingTargets { id: String, version: Version, targets: Vec<Target> },
      Deprecated { id: String, version: Version, message: String, replacement: Option<String> },
      LockedYanked { id: String, version: Version },
  }
  pub enum ResolveError { NotInIndex(String), NoVersion { id: String, requirement: VersionReq, reason: String } }
  pub fn resolve(input: &ResolveInput) -> Result<Resolution, ResolveError>;
  ```
  Rules, per dependency id, in id order:
  1. **Keep** the previous lock entry when `update` does not name the id (`Keep`, or `Only` without it), the requirement matches its version, its locked `celesta` range matches `celesta`, and no advisory blocks it — yanked or withdrawn included (`LockedYanked` warning for yanked).
  2. Otherwise **candidates**: versions where the requirement matches, `!yanked`, `!withdrawn`, not blocked, and `entry.celesta.matches(celesta)`.
  3. A plugin whose index `kind.roles` contains `"media"` takes the highest candidate (pinned media are exempt from targets). Others take the highest candidate with an artifact for every project target or for `any`; failing that, the highest with one for `host` or `any`, with `MissingTargets` naming the project targets it lacks; failing that, `NoVersion`.
  4. `NoVersion.reason` says why the newest version matching the requirement was refused: `"yanked"`, `"withdrawn"`, `"blocked by <advisory id>"`, `"works with Celesta <range>, not <celesta>"`, or `"no build for <host>"`; `"no version matches"` when none does.
  5. `Deprecated` warning when the chosen version or the index is deprecated.
  6. The lock entry records every target the version has; `lock.celesta = celesta`; `lock.targets = targets` sorted; dependencies no longer listed drop out; the previous lock's unknown fields are kept.

- [ ] **Step 1: Failing tests** in `tests/resolve.rs` (indexes built in code):
  - `the_newest_matching_version_is_chosen` (`^0.2` over 0.2.0, 0.2.3, 0.3.0 → 0.2.3).
  - `yanked_withdrawn_and_blocked_versions_are_skipped`.
  - `a_locked_yanked_version_is_kept` (with `LockedYanked`).
  - `the_celesta_range_is_respected` (0.2.3 `>=0.10.0, <0.11.0` on Celesta 0.9.5 → 0.2.2).
  - `every_project_target_is_preferred` (0.2.3 lacks `windows-x86_64`, 0.2.2 has all → 0.2.2).
  - `missing_targets_fall_back_with_a_warning`.
  - `any_covers_every_target`.
  - `pinned_media_ignore_the_targets` (macOS-only media plugin resolves on a `linux-x86_64` host).
  - `a_minor_upgrade_re_resolves_only_what_cannot_load` (lock from 0.9.4 with `a` `>=0.9.0, <0.10.0` and `b` `>=0.9.0`; Celesta 0.10.0, `Update::Only(previous.unloadable(..))` → `a` moves to a 0.10 build, `b` kept, `lock.celesta == 0.10.0`).
  - `keep_does_not_move_a_satisfied_lock` (a newer version exists; `Update::Keep` keeps the old one).
  - `removed_dependencies_leave_the_lock`.
  - `no_version_says_why` (only a yanked match → reason `"yanked"`).
- [ ] **Step 2:** They fail.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Pass.
- [ ] **Step 5: Commit** `feat(registry-client): resolve plugin requirements into a lock`.

### Task 10: The store installer

**Files:**
- Create: `crates/registry-client/src/store.rs`, `tests/store.rs`
- Modify: `crates/registry-client/Cargo.toml` (`zip` with default features off and `deflate` on; check its license is MIT or Apache-2.0 compatible with `cargo deny`'s config if the repository has one), `Cargo.lock`

**Interfaces:**
- Produces:
  ```rust
  pub const MAX_UNPACKED_BYTES: u64 = 4 * 1024 * 1024 * 1024;
  pub const MAX_ENTRIES: usize = 100_000;
  pub struct Store { root: PathBuf }
  pub struct InstallRequest<'a> {
      pub id: &'a str, pub version: &'a Version, pub url: &'a str, pub size: u64,
      pub sha256: &'a Hash, pub tree_hash: &'a Hash,
  }
  pub enum InstallError {
      Transport(TransportError), Io { path: PathBuf, source: io::Error },
      Size { expected: u64, found: u64 }, Checksum { url: String },
      Unsafe { entry: String, reason: String }, TooLarge,
      TreeHash { expected: Hash, found: Hash }, Manifest(String), Format(FormatError),
  }
  impl Store {
      pub fn new(root: PathBuf) -> Self;
      pub fn standard() -> Option<Self>;                 // registry_store_dir()
      pub fn path(&self, id: &str, version: &Version) -> PathBuf;
      /// Whether the version is there with this tree hash.
      pub fn contains(&self, id: &str, version: &Version, tree_hash: &Hash) -> bool;
      /// Downloads, checks, extracts, hashes, and renames into place; returns the directory.
      pub fn install(&self, transport: &dyn Transport, request: &InstallRequest) -> Result<PathBuf, InstallError>;
  }
  ```
  Steps of `install`: `contains` already → return it with no download. Create the root with `create_private_dir_all`. Work in `tempfile::Builder::new().prefix(".tmp-").tempdir_in(root)` (ids never start with `.`, so it never names a plugin, and the rename stays on one file system). Download into `archive.zip`, reading at most `size + 1` bytes: more or fewer than `size` is `Size`. Compare SHA-256 (`Checksum`). Extract into `plugin/`: each name must be UTF-8, relative, without `\`, `:`, NUL, `..`, or empty parts (a trailing `/` marks a directory); each entry a directory or regular file (a Unix mode of type link or anything else is `Unsafe`); no duplicate names; at most `MAX_ENTRIES` entries and `MAX_UNPACKED_BYTES` in all, counted while writing, not from the headers (`TooLarge`). On Unix, files get 0o755 when the archived mode has any execute bit, else 0o644, and directories 0o755. Then `tree_hash(plugin)` must equal the request (`TreeHash`), and `celesta-plugin.json`'s `id` and `version` must equal the request (`Manifest`). Create `root/<id>` (0700) and rename `plugin` to `root/<id>/<version>`. An existing destination with the right tree hash is kept (another install won); one with another hash is moved into the temp dir, the new one renamed in, and the old one deleted with the temp dir.

- [ ] **Step 1: Failing tests** in `tests/store.rs`, building archives in the test with `zip::ZipWriter` and serving them with `DirTransport`:
  - `an_archive_installs_and_hashes` (installed dir's `tree_hash` equals the request; `contains` true; an executable keeps 0o755 and a data file is 0o644 on Unix).
  - `a_wrong_checksum_is_refused`, `a_wrong_size_is_refused`, `a_wrong_tree_hash_is_refused`, `a_wrong_manifest_id_is_refused` — each leaves no `root/<id>/<version>` and no `.tmp-*`.
  - `zip_slip_is_refused` (`../evil`, `/abs`, `a\\b`, `C:x`, `a//b`).
  - `links_are_refused` (an entry with Unix mode `0o120777`).
  - `duplicate_names_are_refused`.
  - `bombs_are_refused` (a test-only lower limit: make the limits fields of `Store` with `Store::with_limits(root, bytes, entries)` used by tests).
  - `a_correct_existing_copy_is_kept` (second install makes no request: a transport that panics on `open`).
  - `a_tampered_existing_copy_is_replaced`.
  - `a_stale_temporary_folder_is_ignored` (a leftover `.tmp-old/plugin/` does not stop install, and discovery over the store root with `PluginSource::Registry` lists nothing from it).
  - `the_store_passes_the_permission_check` (Unix: `unsafe_reason` is `None` for the root and the plugin dir under umask 0o022 and 0o002).
- [ ] **Step 2:** They fail.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Pass.
- [ ] **Step 5: Commit** `feat(registry-client): install registry archives into the store`.

### Task 11: Project and global operations

**Files:**
- Create: `crates/registry-client/src/project.rs`, `tests/project.rs`

**Interfaces:**
- Consumes: Tasks 4, 5, 6, 8, 9, 10; `bundled_plugin_dir`, `discover_in`, `tree_hash`, `UserSettings`.
- Produces:
  ```rust
  pub struct Environment<'a> {
      pub registry: &'a Registry, pub store: &'a Store,
      pub celesta: &'a Version, pub host: &'a Target,
      pub bundled: Option<&'a Path>,     // bundled_plugin_dir() in production
      pub now: OffsetDateTime,
  }
  pub struct LockOutcome { pub lock: Lockfile, pub warnings: Vec<ResolveWarning>, pub installed: Vec<(String, Version)> }
  pub struct InstallOutcome { pub installed: Vec<(String, Version)>, pub skipped: Vec<(String, Version, String)> }
  pub enum ProjectError {
      Config(ProjectConfigError), Lock(LockError), Client(ClientError), Resolve(ResolveError), Install { id: String, version: Version, source: InstallError },
      NoLock, OutOfDate { ids: Vec<String> },
      OtherCelesta { locked: Version, running: Version, ids: Vec<String> },
      Missing { id: String, version: Version },
      Withdrawn { id: String, version: Version },
      Blocked { id: String, version: Version, advisory: String },
      LockMismatch { id: String, version: Version },
      Settings(SettingsError),
  }
  /// Resolves the project's dependencies (fetching their indexes), writes celesta.lock, installs host builds.
  pub fn lock_project(env: &Environment, root: &Path, update: &Update) -> Result<LockOutcome, ProjectError>;
  /// Installs what celesta.lock names, never changing it.
  pub fn install_locked(env: &Environment, root: &Path, offline: bool) -> Result<InstallOutcome, ProjectError>;
  /// Resolves `id` against `requirement` for the host alone, installs it, and records it in `settings_path`'s registryPlugins (replacing that id's entry).
  pub fn install_global(env: &Environment, settings_path: &Path, id: &str, requirement: &VersionReq) -> Result<RegistryPlugin, ProjectError>;
  ```
  Messages pinned:
  - `OtherCelesta`: `"celesta.lock was resolved with Celesta {locked}, and this is Celesta {running}; {ids} cannot load with it. Run `celesta plugin update` to resolve them again"`.
  - `OutOfDate`: `"celesta.lock does not match celesta.config.json for {ids}; run `celesta plugin update`"`.
  - `Missing` (offline): `"{id} {version} is not installed, and this run is offline"`.
  - `Withdrawn`: `"{id} {version} is no longer distributed"`.
  `install_locked` order: `NoLock`; `OtherCelesta` when `lock.unloadable(celesta)` is not empty; `OutOfDate` when a dependency has no lock entry or its requirement does not match the locked version; then per entry: no host or `any` artifact → `skipped` with "no build for <host>"; the store `contains` it, or a bundled plugin (discovered under `bundled`) with that id, version, and tree hash exists → nothing to do; offline → `Missing`; else fetch the index: version absent or withdrawn → `Withdrawn`; blocked by `cached_advisories` → `Blocked`; the index artifact's `sha256` or `treeHash` differs from the lock → `LockMismatch`; else install.

- [ ] **Step 1: Failing tests** in `tests/project.rs`, with a test registry signed by the fixture keys: a helper `TestRegistry` that writes `config.json`, indexes, archives, and their `.sig` files into a temp folder with the fixture keys (root seed 32 × `0x01`, online key `2026-11` seed 32 × `0x02`, from `crates/registry-format/fixtures/keys/README.md`; `ed25519-dalek` as a dev-dependency), stamping `generated`:
  - `lock_then_install_on_another_machine` (lock in one store, delete the store, `install_locked` into a fresh one: same tree hashes).
  - `a_satisfied_lock_needs_no_network` (`install_locked` with a transport that panics).
  - `a_bundled_copy_satisfies_install`.
  - `offline_reports_what_is_missing`.
  - `a_withdrawn_locked_version_fails`, `a_blocked_locked_version_fails`.
  - `a_lock_the_index_contradicts_is_refused`.
  - `a_locked_yanked_version_still_installs`.
  - `install_locked_refuses_another_celesta_minor` (message contains both versions and `celesta plugin update`).
  - `install_locked_refuses_a_stale_lock` (`OutOfDate`).
  - `install_global_records_the_tree_hash` (settings file gains `registryPlugins` with the installed hash; a second install of another version replaces it).
- [ ] **Step 2:** They fail.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Pass.
- [ ] **Step 5: Commit** `feat(registry-client): lock and install a project's registry plugins`.

### Task 12: HTTP transport through `celesta-remote`

**Files:**
- Modify: `crates/remote/src/lib.rs` (one public function), `crates/registry-client/Cargo.toml` (add `celesta-remote`), `Cargo.lock`
- Create: `crates/registry-client/src/http.rs`

**Interfaces:**
- Produces:
  ```rust
  // celesta-remote
  /// GET `url` with the shared agent's TLS and user agent; `timeout` replaces the
  /// phase and stall limits when given. A 404 is RemoteAssetError::Request with that status.
  pub fn open_url(url: &str, timeout: Option<Duration>) -> Result<Box<dyn Read + Send>, RemoteAssetError>;
  // registry-client
  pub struct HttpTransport { timeout: Option<Duration> }
  impl HttpTransport { pub fn new() -> Self; pub fn with_timeout(timeout: Duration) -> Self; }
  impl Transport for HttpTransport { .. }   // 404 → TransportError::NotFound
  pub const CLI_ADVISORY_TIMEOUT: Duration = Duration::from_secs(3);
  ```
  Justification recorded in the handoff note: the asset cache never revalidates and keys files by URL, which suits immutable assets but not signed files that change; `open_url` reuses its agent configuration rather than adding a second HTTP stack.

- [ ] **Step 1: Failing tests**: in `celesta-remote`, `open_url_reads_a_body` and `open_url_times_out` (reusing the module's `serve`/`serve_with` listeners; the timeout test passes 200 ms to a listener that never answers and expects an error within 2 s); in `registry-client`, `http_maps_404_to_not_found` with a `127.0.0.1` listener.
- [ ] **Step 2:** They fail.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `cargo test -p celesta-remote -p celesta-registry-client --locked` passes.
- [ ] **Step 5: Commit** `feat(registry-client): fetch over HTTPS with celesta-remote's agent`.

### Task 13: Documentation and the full check

**Files:**
- Create: `docs/handoff/2026-11-14-registry-client.md`
- Modify: `HANDOFF.md` (workspace map: a `celesta-registry-client` row; the `celesta-plugin-host` row gains the store sources, `celesta.lock`, and advisories; the `celesta-registry-format` row loses "publish = false for now"), `crates/registry-format/README.md` (publish line), `docs/handoff/2026-11-14-plugin-registry-design.md` (Status line: plan 3 built, with a link to the new note, and the decisions above marked as decided in plan 3)

The handoff note records: specification (the decisions list, the lock format with an example, search order, approval rule, advisory behaviour, the store layout and extraction rules, the resolver rules), implementation (a path/responsibility table), checks actually run with results, what is deferred (callers passing a project lock and fetching advisories: plans 4 and 5; the user guide and skill, which change when the `celesta` command makes this user-facing; the root key), and the maintainer's crates.io step.

- [ ] **Step 1:** Write the docs.
- [ ] **Step 2:** Run, and record the results in the note:
  ```bash
  cargo test --workspace --locked --no-fail-fast
  cargo clippy --workspace --all-targets --locked -- -D warnings
  cargo fmt --all -- --check
  git diff --check
  ```
  Expected: all pass. A failure in a timing test that this work does not touch is rerun alone and reported as such, not hidden.
- [ ] **Step 3: Commit** `docs: record the registry client core`.
