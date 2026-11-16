# The `celesta` command (2026-11-14)

Plan 4 of the plugin registry. The design is
`docs/handoff/2026-11-14-plugin-registry-design.md` ("Project files", "Store and
discovery", "Resolution", "Command line"); plan 3, the client core this builds on,
is `docs/handoff/2026-11-14-registry-client.md`. This plan adds a `celesta` binary
whose `celesta plugin …` subcommands declare, lock, install, update, audit, and
clean registry plugins, and makes `celesta-exporter` and the MCP server honour a
project's `celesta.lock` without ever downloading or changing it. The user guide
is the "Registry plugins" section of `packages/website/content/{en,ja}/docs/05-(more)/05-plugins.md`;
the Agent Skill covers it in `skills/celesta/references/plugins.md`.

## Specification

Decisions the maintainer made for this plan:

1. **Packaged name `Celesta-cli`.** The crate `celesta-cli` (`publish = false`,
   since it depends on `celesta-registry-client`) builds the binary `celesta`. The
   macOS app and the Windows package ship it as `Celesta-cli` (`Celesta-cli.exe`)
   beside `Celesta-export` and `Celesta-mcp`, because `celesta` and the editor's
   `Celesta` are the same name on a case-insensitive file system. Source builds
   and the Linux image (`/opt/celesta/celesta`) keep `celesta`. Every message the
   tools print says `celesta plugin …`.
2. **Registry override through the environment.** ~~`official_root_key()` stays
   `None`: no root key is compiled in yet. With neither variable set, the commands
   that need the registry fail with exactly `this build of Celesta has no registry
   key yet, so it cannot reach the plugin registry; set CELESTA_REGISTRY and
   CELESTA_REGISTRY_ROOT_KEY to use another registry`.~~ Since 2026-11-15
   `official_root_key()` returns the official registry's pinned root key, so with
   neither variable set the commands use `https://registry.celesta.studio`, and the
   no-key message is gone; see [`2026-11-15-official-root-key.md`](2026-11-15-official-root-key.md). `CELESTA_REGISTRY` is a base
   URL, `https://…`, `http://…`, or `file://<absolute folder>` (`file:///C:/x` on
   Windows); `CELESTA_REGISTRY_ROOT_KEY` is the root public key in standard base64
   (`RootKey::from_base64`). They are set together: one alone is a usage error
   (exit 2). `file://localhost/…` and percent-escapes are not supported. The user
   guide does not mention these variables; this note is their documentation.
3. **Command-line tools read the lock regardless of trust.** `celesta`,
   `celesta-exporter`, and `celesta-mcp` read the `celesta.lock` of the project
   they act on whether or not `settings.json` trusts that project
   (`RegistryContext::for_project`, `InstalledPlugins::find_for_project`). The
   desktop app keeps plan 3's trust gate, and trust still gates a project's own
   `plugins.paths` folders everywhere.

Rulings made while building it:

- ~~**No key, no registry commands.** `run()` checks for a key before it dispatches
  `search`, `info`, `add`, `update`, and `install --global`, and fails with the
  no-key message. `install` without a key installs from the store alone when the
  lock checks out, and turns a version missing from the store into the no-key
  message. `remove`, `list`, `audit`, and `gc` run without one.~~ Every build has
  a key since 2026-11-15; see [`2026-11-15-official-root-key.md`](2026-11-15-official-root-key.md).
- **A project with no registry plugins.** `install` and `update` (without an
  id) in a project with no dependencies and no lock print `this project has
  no registry plugins` and exit 0, `install` with or without a key; neither
  writes a lock or records the folder.
- **The store's project record.** `.projects.json` in the store root holds
  `{"projects": ["<absolute root>", …]}`, sorted and deduplicated, each root made
  absolute with links resolved; it is written through a temporary file renamed over
  it, 0600 on Unix, and keys this build does not know are kept. `lock_project`
  records the project right after it saves the lock, before any install, so a
  partial install never leaves versions `gc` would collect; `install_locked`
  records it once the lock checks out, before any install. A record that fails is not a failure of the
  operation: `celesta` warns `could not record this project for celesta plugin gc:
  …`.
- **What `gc` keeps.** Every `<id>/<version>` that `registryPlugins` in
  `settings.json` names, that the current project's lock names (when the folder
  holds `celesta.config.json` or `celesta.lock`), or that the lock of a recorded
  project names. A recorded project whose `celesta.lock` is gone is forgotten and
  its versions are collected. A recorded or current project whose lock exists but
  does not load makes `gc` fail, naming that lock and the store's
  `.projects.json`, because the unreadable lock may still name versions. `.tmp-*`
  folders whose modification time is more than one hour old are removed; an id
  folder left empty goes with its last version. `--dry-run` never removes.
- **The advisory budget.** `search`, `info`, `add`, `install` (not `--offline`,
  and `--global` too), `update`, `list`, and `audit` refresh the advisory list once
  per run through `refresh_advisories_within`, which runs the refresh on its own
  thread and waits at most `CLI_ADVISORY_TIMEOUT` (3 seconds) in total, then uses
  the cached list. A refresh thread that outlives the budget is left to finish; the
  cache's check-then-write and stamp update happen under one process-wide lock
  (`cache::lock`), so it cannot lose or lower a stamp. The command warns when the
  refresh failed or timed out, when no list was ever fetched, and when the newest
  verified list is more than 7 days old (`advisories_stale`).
- **Override caches.** An override registry caches in
  `registry_cache_dir()/override-<first 16 hex digits of the SHA-256 of the base
  URL without a trailing />`, so its rollback stamps and advisories are kept apart
  from the official registry's. The exporter and the app read only the official
  cache, so an override's advisories never reach them (acceptable for a development
  path).
- **`audit` and dropped plugins.** `audit` treats a plugin the registry no longer
  lists, and a locked version missing from its index, as no longer distributed and
  fails, as `install_locked` does. With nothing to report it prints `no problems
  found`, or `no problems found in what could be checked` when it warned that it
  could not check a plugin (neither the registry nor the cache had its index).
- **A blocked locked version.** `install_locked` checks every locked entry
  against the cached advisories before it looks in the store, so `install` and
  `install --locked` fail on a blocked version even when it is downloaded, with
  `{id} {version} is blocked by advisory {advisory}; run \`celesta plugin update
  {id}\``.
- **Global installs warn.** `install_global` returns `GlobalOutcome` (the
  recorded entry and resolution's warnings), and `install --global` prints the
  warnings, such as a deprecation with its replacement.
- **The exporter's prefix.** `celesta-exporter` keeps its own error prefix
  (`Celesta export: …`); `celesta: error: …` is the `celesta` command's.
- **Tests and real folders.** Tests never write the real user folders: child
  processes get `HOME`, `XDG_DATA_HOME`, `XDG_CONFIG_HOME`, and `APPDATA` under a
  temporary folder. Read-only discovery of the real user and bundled folders in
  `celesta-plugin-host` tests is accepted, as before; their assertions only look at
  test ids.
- **A file test registry dates its files from the real clock.**
  `TestRegistry::file()` (a test registry served from its own folder through a
  `file://` base) stamps `generated` from now minus one day, in whole seconds,
  because the binary checks freshness against the real clock.

Output and exit status. Results go to stdout; warnings are `celesta: warning:
<text>` and errors `celesta: error: <text>` on stderr. Exit 0 on success, 1 on
failure, 2 on a usage error. Deprecated and yanked plugins are warned about on
stderr, with the replacement when the registry names one. `celesta.config.json`
is rewritten keeping every other key, their order, and the text of every value the
command did not change; duplicate keys and invalid JSON are refused without
writing. A failed `add` puts `celesta.config.json` and `celesta.lock` back to their
exact previous bytes, or removes a config it created.

Lock checks for the exporter and MCP. `check_lock` fails with
`LockProblem::NoLock` (dependencies but no lock), `OutOfDate` (a dependency
missing from the lock or locked outside its requirement, and every lock entry that
is no dependency), or `OtherCelesta` (a locked range that excludes the running
Celesta, naming both versions and `celesta plugin update`).
`find_for_project` then turns each `DiscoveryError::LockUnsatisfied` into
`LockProblem::Unsatisfied`, except when an explicit plugin (`--plugin-dir`,
`CELESTA_PLUGIN_PATH`) supplies that id: then the explicit copy runs and the
problem stays a warning, so plugin developers can test a build. After
discovery, every lock entry with a build for the host (or `any`) must be
supplied by a plugin found as `PluginSource::Locked` or `Explicit`; one that
is not (a locked version an advisory blocks, or a store folder that cannot be
read) is added to `Unsatisfied` too, with the reason `it is blocked by advisory
{advisory}`, the read error's text, or else `it was not used`, so another copy
of its id never runs in its place unnoticed. A locked store copy that does not
load with the running Celesta gives the reason `it does not load with Celesta
{version}`, and `Unsatisfied` keeps its one hint, `run \`celesta plugin
install\``. A locked version
with no build for the host (neither its target nor `any`) is not a lock failure:
discovery reports it as its own `DiscoveryError::LockNoBuild { id, version, host }`
(`the project locks {id} {version}, which has no build for {host}; another copy of
{id} is used if one is installed`), which `find_for_project` keeps as a warning, and
the other sources supply that id (a global, bundled, or explicit copy), as the
design says for a pinned plugin and as `install_locked` skips it. A locked
version the user turned off (`disabledPlugins` in `settings.json`) gives way
as any turned-off plugin does, so the guide's "a turned-off plugin gives way to
the next one" holds for the lock too: `find_for_project_with` reads the
turned-off copies (`find_allowed_in`), and for a turned-off `Locked` copy keeps
the warning `DiscoveryError::LockTurnedOff { id, version }` (`the project locks
{id} {version}, which is turned off in your settings; another copy of {id} is
used if one is installed`) instead of `Unsatisfied`, whose
`celesta plugin install` hint could not fix it. The exporter
reports any of these as `plugins_unsatisfied` under `--json`; exports, `--inspect`,
and `--list-components`/`--describe` with a source check it, and so do the MCP
server's export jobs and its `inspect` tool.

## Implementation

| Path | Responsibility |
| --- | --- |
| `crates/plugin-host/src/lock_check.rs` | `LockProblem` and `check_lock`. |
| `crates/plugin-host/src/registry.rs`, `manifest.rs` | `is_build` (the one "same build" check), `RegistryContext::for_project`, `InstalledPlugins::find_for_project` and `find_for_project_reporting`, and `DiscoveryError::LockNoBuild` for a locked version without a build for the host (`a_locked_version_without_a_host_build_is_a_warning`). `find_for_project_with` requires each locked version with a build here to be found as `Locked` or `Explicit`, else `Unsatisfied` with `unused_reason` (`a_blocked_locked_version_stops_discovery`, `an_unreadable_locked_version_stops_discovery`, `an_unsafe_locked_version_stops_discovery` on Unix). A turned-off locked version gives way with `DiscoveryError::LockTurnedOff` (`a_turned_off_locked_version_gives_way`, `a_turned_off_locked_version_with_no_other_copy_is_a_warning`). |
| `crates/plugin-host/src/project.rs` | `project_root_of_dir`, the project a folder is in, which `project_root` and the `celesta` command use. |
| `crates/registry-format/src/files.rs` | `Kind::is_media`, the one media-role test. |
| `crates/registry-client/src/testing.rs` | The signed test registry behind the `testing` feature (`with_base`, `file`, `served_dir`, `root_key_base64`). |
| `crates/registry-client/src/resolve.rs`, `project.rs` | Re-resolving a kept entry for a new target, `LockedWithdrawn`/`LockedBlocked` warnings, `ProjectError::LockCheck`, orphan lock entries refused by `install_locked`, `record_error` on outcomes, blocked locked versions refused by `install_locked` before the store is looked at, and `GlobalOutcome` from `install_global`. |
| `crates/registry-client/src/refresh.rs`, `cache.rs` | `refresh_advisories_within`, `AdvisoryRefresh`, and the cache lock. |
| `crates/registry-client/src/store.rs`, `gc.rs` | `Store::record_project`, `PROJECTS_FILE`, `plan_gc`, `run_gc`, `GcPlan`, `GcInput`. |
| `crates/cli/src/config.rs` | `ProjectConfig`: editing `plugins.dependencies` without touching other keys; `restore`. |
| `crates/cli/src/args.rs`, `main.rs`, `lib.rs` | The arguments, `parse_spec`, `run`, and exit statuses. |
| `crates/cli/src/session.rs` | `Session`: registry selection (the official registry and its pinned key, or the `CELESTA_REGISTRY` override), store, caches, settings path. |
| `crates/cli/src/advisories.rs`, `report.rs` | The bounded refresh and its warnings; stderr output. |
| `crates/cli/src/commands/{project,registry,store}.rs` | `add`, `remove`, `install`, `update`; `search`, `info`, `audit`; `list`, `install --global`, `gc`. |
| `crates/cli/tests/{session,project,registry}.rs` | End-to-end tests against a `file://` test registry. |
| `crates/exporter/src/{exporter,job,main,error,report}.rs` | `project_plugins`, `ExportError::Plugins`, the `plugins_unsatisfied` code; each export gets its own media registry. |
| `crates/exporter/tests/integration/plugins_lock.rs` | The lock failures under `--json`, the lock never written, and a locked plugin without a build here not failing the export (`exporter_skips_a_locked_plugin_without_a_build_here`). |
| `crates/mcp/src/{server,export,inspect}.rs`, `docs.rs` | The MCP tools discover for the job's project; `docs.rs`'s description of the `plugins` docs topic now names registry plugins (the topic's text is the skill's `plugins.md`). |
| `scripts/package-macos.py`, `scripts/package-windows.ps1`, `packaging/linux/Dockerfile`, `scripts/test-package.mjs`, `packaging/*/README*` | Shipping `Celesta-cli` (and `celesta` in the image), and a `plugin --help` smoke step in the package check. |
| `packages/website/content/{en,ja}/docs/05-(more)/05-plugins.md`, `01-(getting-started)/02-create-project.md` | The user guide's "Registry plugins" section; the executable names. |
| `skills/celesta/references/{plugins,errors,export}.md`, `SKILL.md` | The Agent Skill: registry plugins, `plugins_unsatisfied`, the index row. `crates/mcp/src/docs.rs` serves the skill's `plugins.md` itself; only its topic description changed (see the MCP row). |
| `HANDOFF.md`, the design note, the plan 3 note | The workspace map row, the executable names, the status line, and the plan 3 gaps this plan fixed. |

## Checks

Run in this worktree, after `pnpm install` and `pnpm run build:runtime`, with
`PKG_CONFIG_PATH` pointing at Homebrew's `ffmpeg@8`.

| Command | Result |
| --- | --- |
| `cargo build -p celesta-codec-test --locked` | Passed. |
| `cargo test --workspace --locked --no-fail-fast` (after the final review's fixes) | 3205 passed, 1 failed, 1 ignored (summed over the 91 test binaries and doc-test runs). The failure was the timing check `a_process_left_holding_stderr_does_not_delay_the_diagnostic` (`took 3.82s`, with the other binaries running alongside). Run alone it passed twice, and `cargo test -p celesta-plugin-host --lib` then passed, 181 of 181. |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed, no warnings (Cargo's future-incompatibility note about the `block` 0.1.6 dependency only). |
| `cargo fmt --all -- --check` | Passed. |
| `git diff --check` | Passed. |
| TDD for the final review's fixes | Each new test failed before its change: `install_locked_refuses_a_blocked_held_version` (exit 0, `celesta.lock is satisfied`), `a_blocked_locked_version_stops_discovery` and `an_unreadable_locked_version_stops_discovery` (`Ok` where `Unsatisfied` was expected), `install_global_warns_about_a_deprecated_plugin` (empty stderr), `update_outside_a_project_does_nothing` (`celesta.lock is up to date`), `a_failed_install_locked_still_records_the_project` (no `.projects.json`), `audit_says_when_it_could_not_check_everything` (`no problems found`), and `a_store_copy_that_does_not_load_with_this_celesta_is_not_called_tampered` with the new reason. `an_unsafe_locked_version_stops_discovery` and the stderr assertion in `exporter_skips_a_locked_plugin_without_a_build_here` were added after the code they cover and pass. |
| `pnpm --dir packages/website run build` | Passed: 56 pages in 2 locales. The built `en` and `ja` plugins pages have every heading id the new section links to. |
| `pnpm --dir packages/website run check` | No issues, 82 warnings. The anchor warnings name every Japanese anchor, the new ones and the ones already there alike, although the built pages have those ids; the English anchors pass. The rest are missing OGP images. |
| `celesta plugin --help` and each subcommand's `--help` | Every command and flag the user guide and the Agent Skill name is listed. |
| After `DiscoveryError::LockNoBuild`: `cargo build -p celesta-codec-test --locked`, then `cargo test -p celesta-plugin-host -p celesta-exporter -p celesta-mcp -p celesta-editor --locked --no-fail-fast` | 1441 passed, 1 failed, 0 ignored. The failure was the timing check `a_process_left_holding_stderr_does_not_delay_the_diagnostic` (`took 2.86s`, with the other binaries running alongside); it failed the same way in a first run of the same command. Run alone it passed twice, and `cargo test -p celesta-plugin-host --lib` then passed, 177 of 177. The new tests `a_locked_version_without_a_host_build_is_a_warning` and `exporter_skips_a_locked_plugin_without_a_build_here` pass; the first failed before the change (`Unsatisfied { … "it has no build for macos-aarch64" }`). |
| After the change: `cargo clippy -p celesta-plugin-host -p celesta-exporter -p celesta-mcp -p celesta-editor --all-targets --locked -- -D warnings`, `cargo check --workspace --all-targets --locked`, `cargo fmt --all -- --check`, `git diff --check` | Passed. |
| `celesta plugin …` without a registry key, in a temporary home | `search` and `add` fail with the no-key message (exit 1); `list`, `audit`, and `gc --dry-run` run (exit 0); `install` without dependencies prints `this project has no registry plugins`; with a dependency and no lock, `install` fails with the no-key message and `install --locked`/`--offline` with the `NoLock` message. |

| After `DiscoveryError::LockTurnedOff`: `cargo build -p celesta-codec-test --locked`, then `cargo test --workspace --locked --no-fail-fast`; `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo fmt --all -- --check`, `git diff --check`, `pnpm --dir packages/website run build` | 3236 passed, 0 failed, 1 ignored; the rest passed (56 pages in 2 locales). `a_turned_off_locked_version_gives_way` and `a_turned_off_locked_version_with_no_other_copy_is_a_warning` failed to build before the change (no `LockTurnedOff`), and before it a turned-off locked copy stopped discovery with `Unsatisfied` (`it was not used`). |
Not run: the first-party plugins' separate workspace (`plugins/`), the `pnpm`
tests, the package scripts (they need a full package build), and the registry
service's tests.

## Later plans

- ~~Plan 5 (the desktop app): the editor depends on `celesta-registry-client` (which
  then needs the crates.io setup of plan 3's decision 1), refreshes advisories with
  `refresh_advisories_within` and a budget of its own, checks a project with
  `check_lock`, and records projects through `lock_project`/`install_locked`
  (`Store::record_project`). Until then the app does not read a project's
  `celesta.lock`: its preview and its exports, which go through `Exporter::new`
  without `project_plugins`, do not check the lock, and they use only global
  registry plugins.~~ Done; see
  [`2026-11-16-plugin-registry-app.md`](2026-11-16-plugin-registry-app.md).
- The user asked to fold export into the `celesta` command, in a later, separate
  session.
- ~~The maintainer supplies the root public key, so `official_root_key()` returns it
  and the no-key message goes away.~~ Done; see [`2026-11-15-official-root-key.md`](2026-11-15-official-root-key.md).

## Known gaps

- `locked.registry` is ignored; one registry is assumed.
- `tree_hash` reads each file whole, and every discovery re-hashes every store
  plugin; consider streaming or caching.
- `config.json` is re-fetched on each index, search, and advisories call, and `add`
  fetches the plugin's index twice.
- `celesta` has no `--json` output.
- A locked version absent from its index is kept by the resolver (only `audit`
  reports it).
- `.projects.json` has no lock across processes: two `celesta` runs recording at
  once can lose one entry (that project is recorded again on its next lock or
  install). A recorded project whose lock became unreadable blocks `gc` until the
  user fixes the lock or edits `.projects.json`; there is no command to forget one
  entry.
- `gc` checks a `.tmp-*` folder's modification time, which may not advance during
  a download longer than an hour; `gc` then removes an unfinished download that is
  still running. Removal also follows `plan_gc` without re-checking, so a lock
  written between the two is not seen.
- `gc` run from a subfolder of a project that has a `celesta.lock` but no
  `celesta.config.json` does not treat it as the current project, because the root
  is found by `celesta.config.json` alone.
- `celesta.config.json` rewrites keep a raw value's text as it was, including CRLF
  line endings and its own indentation inside multi-line values; keys at the edited
  levels are re-escaped.
- If spawning the refresh thread fails, `refresh_advisories_within` panics. Its
  test's "under one second" bound may be tight on a loaded CI machine.
- In the MCP server, the `describe` cache is reset when a job switches projects.
  `list_components`/`describe` without a source publish a project-blind global
  registry, and `Exporter::new` given `project_plugins` still prefers a global
  media registry chosen before it.
- Not tested: `add` failing after the lock was saved (a download or hash failure),
  a successful `install --locked` compared byte for byte, the exporter's
  `--plugin-dir` override of a locked id, `--inspect` and `--list-components
  <source>` failing on the lock, a stray lock without `celesta.config.json`, and
  the `Config`/`Unreadable` lock problems.
- `LockProblem::NoLock` says to run `celesta plugin install` also when it comes
  from `install --locked`; that is the right command, though not the one the user
  ran.
- Windows-hazard names (a trailing `.` or space, device names) are refused only
  through the tree hash.
- Four atomic-write helpers (a temporary file renamed over the target) remain
  separate: `celesta.config.json`, `celesta.lock`, `settings.json`, and the
  store's `.projects.json` each write their own. Fold them into one helper in a
  later change.
- `LockProblem::Unsatisfied` still opens with "the project's locked plugins are
  not installed" and says to run `celesta plugin install` when the reason is a
  blocked version; `install` then fails naming `celesta plugin update {id}`.
