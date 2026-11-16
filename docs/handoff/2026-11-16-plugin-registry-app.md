# Registry plugins in the desktop app (2026-11-16)

Plan 5 of the plugin registry: the desktop editor (`crates/editor`) finds,
installs, updates, and removes registry plugins, instead of only the `celesta
plugin` command doing so. The design is the "Desktop app" section of
[`2026-11-14-plugin-registry-design.md`](2026-11-14-plugin-registry-design.md);
the client core is [`2026-11-14-registry-client.md`](2026-11-14-registry-client.md)
(plan 3) and the command line is
[`2026-11-14-celesta-command.md`](2026-11-14-celesta-command.md) (plan 4).

Status: built (2026-11-16). The design was agreed with the maintainer on
2026-11-16 (screens settled with mockups); the sections below keep it, updated
where the build differs, and "Implementation", "Checks", and "Known gaps" at
the end say what was built. Items marked **(designer's call)** were not
discussed and may be corrected.

## Intent

Today only the `celesta` command talks to `registry.celesta.studio`. Someone
who uses Celesta through the app has to switch to a terminal to get a codec,
to add a plugin a composition imports, or to fetch what a project's
`celesta.lock` names. The app should do all of it, with the same rules as the
command line, so a project made in one works in the other.

## Goals

- The right dock's Plugins tab lists **This project** (locked plugins, their
  updates and states), **Global** (codecs and other plugins installed without a
  project, bundled ones marked), **Installed by hand** (today's list and
  approval), and **Added folders**, with **Find plugins…**.
- **Find plugins…** opens a plugin page: search, a plugin's details (kind,
  builds, versions and their states, source, what is installed here,
  advisories), and **Add to this project**, **Install** (global, for a media
  plugin), **Pin to this project** (a media plugin).
- Updating rewrites `celesta.lock` and reloads the project, so the preview
  shows the new version at once.
- Opening a project whose lock is not satisfied asks once, after the trust
  question, listing ids, versions, and sizes.
- A Blocked plugin raises a notification; Deprecated and Yanked ones are
  Problems entries.
- The editor's discovery reads a trusted project's `celesta.lock`, so the
  preview, React runtimes, and exports run the locked versions (the plan 4
  note's "Later plans").

## Non-goals

- Picking an arbitrary older version in the page. The page installs the
  version resolution chooses; the version table is for reading.
- Updating beyond a project's requirement (`^0.2` to `0.3`) as a routine
  update. The app updates within the requirement, as `celesta plugin update`
  does; the one exception is below ("Requirements that can no longer be
  met").
- `gc` from the app, publishing from the app, changing the CLI's default
  requirement (`*`).
- Plugin-to-plugin dependencies (not in the format yet).

## Anti-goals

- The UI thread waits on the network or on extraction.
- The app and the CLI disagree: a lock the app writes is one `celesta plugin
  install --locked` accepts, and the other way round.
- A plugin from the registry runs without the user having trusted the project
  that locks it or installed it themselves.
- A routine install interrupts with a dialog or a success toast; the changed
  row is the feedback.

## Constraints

- `celesta-editor` is published to crates.io; `celesta-registry-client` had
  `publish = false`. The editor depending on it meant removing that line (done),
  and
  **before the next release the maintainer publishes a `0.0.0` placeholder of
  `celesta-registry-client` and runs `scripts/crates-trusted-publishing.sh`**
  (plan 3, decision 1; `2026-11-14-crates-io-trusted-publishing.md`). Its own
  dependencies (`celesta-plugin-host`, `celesta-registry-format`,
  `celesta-remote`) are already publishable. Until the placeholder exists,
  CI's `Crate names` job fails on this pull request; the maintainer chose
  (2026-11-16) to publish the placeholder before merging rather than split the
  pull request, and the pull request's description lists the steps.
- Everything blocking (HTTP through `ureq`, extraction, tree hashing,
  discovery) runs on GPUI's background executor; results return to the view.
- The live registry today holds four media plugins (`avfoundation`, `ffmpeg`,
  `ffmpeg-x264`, `mediafoundation`), all `0.10.0-alpha.3`, with empty
  descriptions. Content-plugin flows are exercised against the signed
  `file://` test registry (`celesta_registry_client::testing`).
- The GPUI Kit Design Guides and the maintainer's taste for the editor: flat
  bands and hairlines like the timeline, monospace for versions and data,
  square corners, no uppercase labels, no gradients, lucide icons.

## Approaches

**Where the plugin page lives** (decided with mockups, 2026-11-16):

- A. In the Assets column at its width, list then details (back to the list).
  Same width and entry as the export page; details stack vertically.
- **B. Chosen: while the page is open, the Assets area widens to show the list
  and the selected plugin's details side by side.** One can scan results while
  switching details; the preview narrows meanwhile.
- C. A list in the Assets column whose selected row expands in place. No
  switching, but long with many plugins.

**How the app reaches the registry:**

- **Chosen: in process, through `celesta-registry-client`** on the background
  executor, with the CLI's project-editing pieces moved into the client crate
  so both tools share them (designer's call).
- Running the `celesta` binary and parsing its output: it has no `--json`, the
  packaged name differs per OS (`Celesta-cli`), and progress and errors would
  be text scraping. Rejected.

## Decisions

### Pre-releases (maintainer, 2026-11-16)

The resolver follows Cargo: a requirement without a pre-release never matches
one, and every published first-party plugin is `0.10.0-alpha.3`, so a `*`
default would install nothing today.

- The plugin page has an **Include pre-releases** checkbox beside the search
  field. Its initial value is on when the running Celesta is a pre-release and
  off otherwise. It is remembered while the app runs, not in `settings.json`,
  so a user back on a stable Celesta does not keep getting pre-releases.
- It decides which version the page offers ("the offered version", below) and
  which rows the version table shows by default; the search list always shows
  the registry's `latest`, marked when it is a pre-release.
- **Add to this project** and **Pin to this project** write the requirement
  `^<offered version>` (for example `^0.10.0-alpha.3`, which also accepts later
  alphas and `0.10.x` releases); **Install** installs the offered version
  exactly (`=<version>`). Later updates follow the written requirement, not
  the checkbox.
- The CLI keeps `*` in this plan.

The offered version is the newest version that resolution would accept: not
Yanked or Blocked, its `celesta` range includes the running Celesta, with a
build for the host (Install) or for the project's targets (Add; media plugins
when pinned are exempt, as in resolution), and a pre-release only when the
checkbox is on. The rule lives in `celesta-registry-client` so the CLI can
adopt it later (designer's call).

### Requirements that can no longer be met (maintainer, 2026-11-16)

First-party plugins follow Celesta's minor version, so after a Celesta
upgrade (0.10 to 0.11) no version that `^0.10.0-alpha.3` accepts loads; an
advisory's fixed version can likewise lie outside a requirement. When
**Update Plugins** (the other-Celesta question) or a Blocked notification's
**Update to X** cannot be resolved within the requirement, the app rewrites
that id's requirement to `^<offered version>` and resolves again. The
question says so before it runs, per id, old and new: "Changes vrm in
celesta.config.json from ^0.10.0-alpha.3 to ^0.11.0". Only ids that cannot be
resolved within their requirement are rewritten; a failed run restores
`celesta.config.json` and `celesta.lock` byte for byte. The operation lives in
`celesta-registry-client` so `celesta plugin update` can offer the same later;
the CLI is not changed in this plan.

No requirement is rewritten without a question listing it first (final
review, 2026-11-16). When the rewrites were not known before a question
opened (the 3-second budget ran out), or for the heading's Update Plugins
with nothing known to rewrite, a page's or row's **Update to X**, and a
Blocked notification's **Update to X**, the app updates within the
requirements; when that fails because a requirement cannot be met
(`ResolveError::NoVersion`), nothing has changed, and the app plans the
rewrites again without a time limit (the heading's Update Plugins… is held
meanwhile) and asks "Update this project’s plugins?" listing them. A
Blocked notification's update plans them offering pre-releases when the
advisory's fixed version is one, whatever Include pre-releases says, and its
global update does the same, so **Update to X** can reach X.

### What counts as a project (designer's call)

**This project**, **Add to this project**, and **Pin to this project** need a
folder with `celesta.config.json` that `settings.json` trusts. An entry
without `celesta.config.json`, or a folder trusted for this run only, shows
no This project section, and the page's Add and Pin are disabled with "Add
plugins to a project with celesta.config.json". The app never creates
`celesta.config.json` for an entry, because discovery reads a lock only in a
saved trusted project (`RegistryContext::current`), and a plugin added there
would not run.

### Bundled and shadowed copies (designer's call)

- Rows are per copy, as today: a bundled `avfoundation` and a global registry
  `avfoundation` are two rows in Global. The copy that does not run says "Not
  used: <the other source> runs" (search order, plan 3 decision 3).
- In the page, **Here** lists every copy found and which one runs. The footer's
  **Installed** means the store (global or this project's lock) holds the
  offered version; a bundled copy of it alone leaves **Install** available, so
  a user can install the registry's copy (the live check does exactly this).
- After an install, when a higher-ranked copy (an explicit path, a project
  folder, the user plugin folder) keeps running instead, the row says so and
  Problems has a warning naming that folder, as with any shadowed plugin.

### Update checks and privacy (designer's call)

The design's anti-goal forbids telling the registry which plugins a user
has. Fetching `/index/<id>.json` for each installed id on a timer would show
that set in request logs, so background checks read only `/search.json` (one
file for everyone): at project open and with the 24-hour advisory refresh.

- A locked or global row shows **Update to X** when `search.json`'s `latest`
  is newer than the installed version and, for a locked one, within its
  requirement. Clicking fetches that one index and resolves; when no
  compatible version exists after all (for example its `celesta` range
  excludes this Celesta), the row says "No newer version works with Celesta
  <version>" until the next check.
- Plugin-level deprecation comes from `search.json`. A version's yank comes
  from cached indexes, which are fetched only by an action the user takes (the
  page's details, an update, a download), so a yank may show late.

### The Plugins tab

Sections in this order; a section with nothing in it shows a one-line note
(This project outside a project: none, the section is left out).

- A toolbar: **Find plugins…** (opens the page; selected while it is open),
  **Reload Plugins**, and a ⋯ menu with **Add Folder…** and **Open Plugin
  Folder**.
- **This project**: heading with `celesta.lock` and the Celesta version it was
  resolved with. One row per locked plugin: id, version (monospace), kind and
  state on the second line (On, Off, Blocked, Deprecated with its replacement,
  Yanked, Not installed, No build for this computer). The row's one visible
  action, when one applies: **Update to X**, **Download** (missing),
  **Allow** never (locked plugins of a trusted project do not ask). A ⋯ menu
  holds Turn Off/On, Show Folder, Show in Find plugins, Remove from this
  project.
- **Global**: `registryPlugins` and bundled plugins. Bundled ones say Bundled.
  ⋯: Turn Off/On, Show Folder, Show in Find plugins, Uninstall (registry ones).
  **Update to X** when a newer offered version exists (designer's call).
- **Installed by hand**: today's rows (user folder, added folders, project
  folders) in the new row shape: **Allow** for a waiting plugin; ⋯ for
  Turn Off/On and Show Folder.
- **Added folders**: as today.
- The always-visible switch, path, and Show Folder button of today's rows move
  into the ⋯ menu and the plugin page.

### The plugin page

Opened by **Find plugins…** or a row's Show in Find plugins; it replaces the
Assets panel as the export page does (the two are exclusive), and the Assets
area widens while it is open.

- **List column**: header "Plugins"; search field (filters `search.json` by id
  and description, ignoring case, as `celesta plugin search` does); Include
  pre-releases; rows with the id and, trailing it in a lane of its own, a
  short state (Installed, In this project, No build for this computer,
  Deprecated); under them the kind and the registry's latest (monospace; a
  pre-release says so by its own suffix, `-alpha.3`), cut short with an
  ellipsis when the column is narrow. A footer line: count, registry host,
  when the list was fetched. Opening the page fetches the list again when the
  one shown is more than 10 minutes old (it stays shown meanwhile), and the
  open page draws again every minute so "updated N min ago" keeps up.
- **Detail column**: header with the id and Close. Fields: Kind (with the
  publisher), Latest, Builds (targets), Source (repository, workflow, short
  commit), Here (what this computer has: bundled, global, locked versions),
  Advisories (none, or each matching advisory). A **Versions** table: version,
  state (Published, Deprecated, Yanked, Withdrawn, Blocked), size of the host's
  build. Pre-releases hidden when the checkbox is off.
- **Footer**: the primary action and the secondary one.
  - Media plugin: **Install** (primary), **Pin to this project** (in a
    project).
  - Other plugins: **Add to this project** (primary; disabled outside a
    project with the reason "Add plugins to a project with
    celesta.config.json").
  - When already installed at the offered version: **Installed** (disabled);
    when an older one is: **Update to X**. For a plugin the project locks, X
    is the newest offered version its requirement in `celesta.config.json`
    accepts; when only a version outside it is newer, **Update to <that
    version>** is disabled with the reason `celesta.config.json asks for
    <requirement>`. For a media plugin these concern the
    global store only; **Pin to this project** is the project-scoped button
    (**Pinned** when the lock holds the offered version), and it carries its own
    reason when disabled.
  - While a change to the project is running, the project-scoped buttons are
    disabled and say so.
  - No offered version: disabled, with the reason under it (no build for this
    computer; needs Celesta X; only pre-releases, turn on Include
    pre-releases).
- **Working**: the button reads **Installing…** (disabled), the footer shows
  bytes done / total, and a thin progress bar runs under it. Closing the page
  does not cancel; the matching Plugins tab row shows Installing… too.
  Done: the button becomes Installed, plugins are rediscovered and the
  project reopened (for a global install too). No success toast.
- **Failed**: the reason in danger color under the footer buttons, for
  example `Couldn’t download avfoundation. Check your connection and try
  again.`; the full error chain goes to Problems.
- **Loading and offline**: the list shows a loading line while `search.json`
  is fetched; offline it shows the last cached list with "Offline · showing
  the list from <time>", or, with no cache, "Couldn’t reach
  registry.celesta.studio" and a **Try Again** button.

### Opening a project

After the trust question (a trusted project only):

1. `check_lock` against `celesta.config.json`. `OtherCelesta` asks
   "“<project>” was locked with Celesta 0.X", lists the plugins that cannot
   load, with **Not Now** and **Update Plugins** (re-resolves only those ids
   and rewrites the lock with the running Celesta's version). `NoLock` and
   `OutOfDate` go to Problems with an **Update Plugins** action in the This
   project heading; the heading offers it for `OtherCelesta` too, so there is
   a way back after Not Now. The heading's Update Plugins runs without a
   question when nothing needs a rewrite; otherwise it asks a third question,
   "Update this project’s plugins?", which lists each requirement it would
   change (the "Requirements that can no longer be met" text above).
2. Otherwise, locked versions with a build for the host that neither the store
   nor a bundled copy holds: a dialog "Download N plugins for “<project>”?"
   with a table (id, version, size, total) and **Not Now** / **Download**.
   Sizes come from the indexes; when they cannot be fetched in time the size
   column says "—" (designer's call).
3. Either question is asked once per project per app run. Not Now leaves the
   project open with the missing plugins as a Problems entry and a
   **Download** action in the This project heading.
4. Download runs `install_locked`, then rediscovers and reloads the project.

### Advisories, deprecations, yanks

- The app refreshes `/advisories.json` at start and every 24 hours on the
  background executor (no budget; it is not on the open path), then
  rediscovers when the verified list changed.
- A plugin that is newly stopped by an advisory raises one notification per
  id, version, and advisory per app run: "<id> <version> was stopped", the
  advisory's id and description, and its one action: **Update to X** when the
  advisory names a fixed version and the copy came from the store (the app
  updates it where it was installed), else **Show in Problems**; a bundled copy
  says "Update Celesta to get a fixed version." (the notification API carries
  one action). While stopped, its row says Blocked and Problems has an error entry.
- With a `CELESTA_REGISTRY` override, discovery reads the official cached list
  and the override's list together, so a test registry's advisory stops a
  plugin and the official list keeps protecting (a `RegistryContext` change
  in `celesta-plugin-host`; the exporter keeps reading the official list
  only) (designer's call).
- Deprecated and Yanked locked or global versions are Problems warnings, read
  from `search.json` and cached indexes (see "Update checks and privacy"):
  "<id> is deprecated" with the replacement, "<id> <version> was yanked" with
  "It keeps working."
- `advisories_stale` (no verified list newer than 7 days) is a Problems
  warning.

### Discovery

`FoundPlugins::find` builds its `RegistryContext` with the trusted project's
root (`RegistryContext::current(project, settings)`), so locked versions run
as `PluginSource::Locked`, and lock problems (`LockUnsatisfied`,
`LockNoBuild`, `LockTurnedOff`) reach Problems as other discovery errors do.
Exports from the app use the same found set, so they honour the lock too.

### Shared code moved into `celesta-registry-client` (designer's call)

- `ProjectConfig` (editing `plugins.dependencies` without touching other keys)
  and `restore`, from `crates/cli/src/config.rs`.
- `add` and `remove` as library operations with the CLI's guarantees (a failed
  add puts `celesta.config.json` and `celesta.lock` back byte for byte;
  `--pin` rules: a media plugin needs a pin, a pin needs a media plugin).
- Registry selection from `CELESTA_REGISTRY` / `CELESTA_REGISTRY_ROOT_KEY`
  (today in `crates/cli/src/session.rs`), so the app can be pointed at a test
  registry the same way. The user guide still does not mention them.
- The offered-version rule above, and the requirement rewrite for ids that
  can no longer be met.
- A progress callback for `Store::install` (bytes read so far), used by the
  page's progress bar.

The CLI keeps its behaviour and output; its tests are the check.

## Left to the builder

- The widths of the two page columns and how the widened Assets area returns
  to its old width.
- How the page and the Plugins tab share state (one entity for registry work
  in flight is suggested, so a row and the page show the same install).
- Cache lifetimes for `search.json` and indexes within a run.
- Exact wording of row states and errors, within the Design Guides'
  interface-language rules.

## Implementation

| Path | Responsibility |
| --- | --- |
| `crates/registry-client/src/config.rs` | `ProjectConfig` (editing `plugins.dependencies` without touching other keys, `dependency(id)` for the text as written), `restore_config`, `restore_file`; moved from `crates/cli/src/config.rs`. |
| `crates/registry-client/src/selection.rs` | `RegistryAccess`: the official registry or the `CELESTA_REGISTRY` / `CELESTA_REGISTRY_ROOT_KEY` override (`from_env`, `transport`, `cache_dir`, `client`), `SelectionError`; moved from the CLI's `session.rs`. |
| `crates/registry-client/src/offer.rs` | `offered_version` (the rule, per `OfferScope`), `NoOffer` (why nothing is offered), `offered_requirement` (`^<version>`), `prereleases_by_default`. |
| `crates/registry-client/src/edit.rs` | `add_dependency`, `remove_dependency`, `plan_update`, `update_dependencies`, `check_pin`, `Rewrite`, `EditError`; the CLI's `add` and `remove` call them, with the same byte-for-byte restore on failure. |
| `crates/registry-client/src/store.rs`, `project.rs`, `registry.rs` | `Progress` and the progress callback on `Store::install` (every 64 KiB and once at the end); `Registry::cached_search`. |
| `crates/plugin-host/src/registry.rs` | `RegistryContext::merge_advisories`: an overriding registry's advisories are read together with the official list. |
| `crates/editor/src/plugins.rs` | `FoundPlugins::find(project, extra_advisories)`: the trusted project's lock (read only for a saved trusted project with `celesta.config.json`), lock problems, blocked plugins, registry plugins, bundled copies, declared requirements, and whether the advisory list is stale. |
| `crates/editor/src/registry/client.rs` | `EditorRegistry`: registry selection, store, cache, and settings paths, host, bundled folder; shared across background jobs. |
| `crates/editor/src/registry/state.rs` | `RegistryState`, one entity per window: the `search.json` list, indexes, work in flight per plugin id (install, add, pin, update, remove, uninstall, download), failures, download progress sampled every 100 ms, the Include pre-releases flag, the 24-hour refresh. Everything blocking runs on the background executor. |
| `crates/editor/src/registry/questions.rs` | The download question, the other-Celesta question, and the requirements question: `lock_question`, `prepare` (sizes or rewrites within a 3-second budget at open, `Budget::Unlimited` after a click), the dialogs, asked once per project per run; `ask_about_requirements` after an update finds a requirement it cannot meet. |
| `crates/editor/src/registry/notices.rs` | Problems entries (deprecated, yanked, shadowed registry copy, stale advisory list, stopped plugins) and the Blocked notification with its one action. |
| `crates/editor/src/plugins_tab_model.rs` | The Plugins tab as data: sections, rows, states, the one action per row, menus, the heading; pure, so it is unit tested. |
| `crates/editor/src/plugins_panel.rs` | Renders that model: toolbar, sections, rows, the ⋯ menus. |
| `crates/editor/src/plugin_page/model.rs` | The page's rows, details, footer (button, reason, progress), and the list's footer line; pure. |
| `crates/editor/src/plugin_page/view.rs`, `mod.rs` | `PluginPage` (search field, Include pre-releases, list and detail columns, keyboard), `PluginPageSlot`, and the `EditorView` methods that open, close, and sync it. |
| `crates/editor/src/export_control.rs`, `render.rs`, `view.rs`, `lifecycle.rs`, `main.rs`, `problems.rs` | The widened Assets area and `LeftPanel::Plugins` (exclusive with the export page), the registry entity's wiring, the questions after the trust question, and the `Registry`, `RegistryNotices`, and `Advisories` Problems sources. |
| `crates/cli/src/session.rs`, `commands/project.rs` | The CLI now uses the shared selection and edit code; its output and tests are unchanged. |
| `packages/website/content/{en,ja}/docs/05-(more)/05-plugins.md`, `skills/celesta/references/plugins.md`, `HANDOFF.md` | The user guide's "The Plugins tab" and "In the app", the skill's paragraph, and the workspace map. |

## Checks

Commands run with the global constraints' build environment (`PKG_CONFIG_PATH`
for `ffmpeg@8`, mise's node and pnpm on `PATH`).

| Command | Result |
| --- | --- |
| `cargo test -p celesta-registry-client -p celesta-cli --locked` (task 1, after the move) | Passed; the CLI's tests ran unchanged (project 19, registry 16, session 7). |
| `cargo test -p celesta-registry-client --locked` (task 3, after `edit.rs`) | Passed, with 14 edit tests. |
| `cargo test -p celesta-plugin-host registry::` (task 5) | 20 passed, including `merged_advisories_stop_plugins_from_both_lists`. |
| `cargo test -p celesta-editor --locked` | 955 passed, 0 failed (final run, task 10). |
| `cargo test -p celesta-editor --locked plugin_page::` (task 9) | 34 passed: 26 model tests and 8 view tests against the `file://` test registry. |
| `cargo clippy -p celesta-editor -p celesta-registry-client --all-targets --locked -- -D warnings`; `cargo clippy --workspace --locked --all-targets -- -D warnings` (task 9) | No warnings. |
| `cargo fmt --all -- --check` | Clean. |
| `pnpm --dir packages/website run build` | Not as one command: the full script stops at `packages/react` (`src/theme.ts(216,25): TS7006`, a file this work does not touch). The site step alone, `pnpm exec makit build --clean`, passed: 56 pages in 2 locales, and the built `en` and `ja` plugins pages have the new heading. |
| `git diff --check` | Passed. |
| Final-review fix wave: `cargo test -p celesta-editor --locked`; `cargo test -p celesta-registry-client -p celesta-cli --locked`; `cargo clippy --workspace --all-targets --locked -- -D warnings`; `cargo fmt --all -- --check`; `git diff --check` | 967 passed; the client and the CLI passed unchanged (CLI project 19, registry 16, session 7); no warnings; clean; passed. |
| `cargo build -p celesta-codec-test --locked`, then `cargo test --workspace --locked --no-fail-fast`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo fmt --all -- --check`, `git diff --check main...HEAD` (at `624e7c74`, before the final-review fix wave) | 3349 passed, 0 failed, 1 ignored; no warnings; clean; passed. |
| Live check, macOS, a dev build with the maintainer's own folders and `https://registry.celesta.studio` | `avfoundation` uninstalled with the Global row's ⋯ menu, then found with **Find plugins…** and installed with **Install** (`0.10.0-alpha.3`; Include pre-releases on by default, since this Celesta is `0.10.0-alpha.3`). The button became **Installed** and the Global row came back. `celesta plugin list` then listed `avfoundation 0.10.0-alpha.3 installed` under Global. |
| Against a signed `file://` test registry (`TestRegistry::file()` leaked to a folder, `CELESTA_REGISTRY`/`CELESTA_REGISTRY_ROOT_KEY` set, a temporary `HOME`) | A project whose lock names `acme.charts 1.0.0` (written with `celesta plugin add 'acme.charts@=1.0.0'`) asked "Download 1 plugin for “demo”?" with the version and size after the trust question; Download installed it and the This project section listed it. With an advisory on `acme.charts <1.1.0` (fixed in `1.1.0`), opening it again showed "acme.charts 1.0.0 was stopped" with **Update to 1.1.0**. |
| Screenshots | Before (main) and after for the Plugins tab, and the new plugin page, download question, and Blocked notification, are attached to the pull request. |

| After rebasing onto main (`fd41ac52`), with `pnpm run build:runtime`: `cargo test --workspace --locked --no-fail-fast`; clippy, fmt, the site step, `git diff --check` | 3359 passed, 3 failed, 1 ignored; the rest passed. The three failures are `celesta-exporter`'s `export::an_export_through_the_programs_ring_is_byte_identical_to_the_pipe` and `stills::separate_stills_and_videos_name_the_frame_that_failed`, and `celesta-mcp`'s `export::export_video_writes_an_mp4_with_progress`: the file was not written by the test codec (`Unsupported … tried: []`). This branch changes neither the exporter, the MCP server, nor media ranking, and with `HOME` pointed at an empty folder the three pass: the builder's own global registry codecs (`ffmpeg`, `avfoundation`, found because these tests read the real user folders) outrank the test codec given with `--plugin-dir`. With a runtime that was not built, these tests skip themselves, which is how the first full run passed. |

Not run by the builder: the packaged apps. CI's `Crate names` job stays red until the maintainer
publishes the `celesta-registry-client` placeholder (see "Constraints").

## Known gaps

- The Blocked notification has one action. The mockup's second, a ghost
  "Details" button, is not there because the notification API carries one
  action; a notification without a fix says "Show in Problems".
- `plan_update` fails the whole run when one id has no offer, and the question
  then shows the reason as blocking. `resolve` skips blocked versions, so a pin
  such as `=0.2.1` that an advisory blocks is rewritten to the offered version
  (after the requirements question lists it).
- The page's `celesta.config.json asks for <requirement>` prints the parsed
  requirement (`semver`'s form, `>=1, <2` for `>=1,<2`), not the text as
  written; the discovery result carries no written text.
- `add` fetches the plugin's index twice when no requirement is written (the CLI
  path, as before).
- A page's **Install** can stay enabled until rediscovery finishes after an
  install, so a second click installs the same version again (idempotent). The
  list's "No build for this computer" shows only for locked plugins, because
  `search.json` has no targets.
- A previous project's failed job is logged as an info entry naming that
  project (no toast, nothing reopens); its success is not announced. Problems
  reported while the window is created (a bad registry override, no user
  folder) are lost when the first project opens, as with every Problems source
  today.
- Opening a project while the approval dialog is up skips the lock question
  without marking it asked; it is asked after Allow and Reload reopens the
  project. A search list is not refetched when one project without a root
  replaces another.
- A blocked bundled copy can be claimed twice (This project and Global). The
  Problems sync clones the loaded indexes on every notify, and the indexes
  `prepare` fetches for the questions are not kept in `RegistryState`.
- Yank state comes only from indexes the user's actions have fetched, so a
  yank may show late (see "Update checks and privacy").
- Tests: no test builds a full `EditorView` in a window (`EditorView::open`
  reads the real user settings), so the registry subscription, the progress
  sampler, the offline branch of the page, focus after closing it, and the
  once-per-project question at view level are not covered; the question's
  once-only rule is tested on `ask_once`. The requirements question after an
  unmet update is tested on its parts (`RequirementsUnmet` from the state,
  `requirements_question` and `ask_requirements` in a window), not through
  `EditorView::ask_about_requirements`. The action lane's width (8 rem) is
  checked only by the screenshots.
- The window re-renders at about 10 Hz while a download runs (`cx.observe` on
  the registry entity).
- The Plugins tab's **Find plugins…** shows it is on with the list's selected
  fill (`list_active`) over its outline style: gpui-kit's own selected fill
  for an outline button is a shade from its rest fill in this palette.
  Changing plugin settings in the tab (Turn On/Off, Allow, folders) is refused
  with a note while registry work is saving `settings.json`.
- `plugins_tab_model.rs` (about 1,300 lines) and `registry/questions.rs`
  (about 950 lines) are large; splitting them was left alone.
- The CLI keeps `*` as its default requirement and does not offer the
  requirement rewrite; both can adopt the library code later.
- An update that turns out to need a rewrite (`RequirementsUnmet`) is dropped
  without a Problems entry when, after planning, the project changed or another
  job holds it; the requirements question it opens does not check for a dialog
  already up.
- For a locked plugin with no newer version that loads here, the page's button
  reads **Installed** without a reason.
- An id declared in `celesta.config.json` but missing from the lock shows
  **Add to this project** (or **Pin to this project**), which replaces its
  written requirement with `^<offered>` without the requirements question; the
  click is the user's, but the label does not say it replaces a requirement.
