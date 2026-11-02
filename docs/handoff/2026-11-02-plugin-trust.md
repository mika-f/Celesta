# Project trust and plugin approval in the editor

The second layer of making content plugins usable from the desktop editor,
on top of `2026-11-02-plugin-search-paths.md`. Opening a project runs its
code (Node evaluates the entry), and a plugin runs its own, so the editor
now asks before either runs.

## Specification

- **Project root** (`celesta_plugin_host::project_root`): the nearest
  folder from the entry's own up that holds `celesta.config.json`, else the
  entry's folder. `package.json` and `tsconfig.json` deliberately do not
  count: a home folder with a `package.json` would otherwise become the
  project (seen on the development machine).
- **Trust** (`UserSettings::is_trusted`/`trust`): a list of trusted
  project roots, links resolved. A root is trusted only when it is in the
  list itself: a project below a trusted one, with its own
  `celesta.config.json`, is another root and is asked about on its own.
  - Trust is saved only for a root that holds `celesta.config.json`
    (`has_project_config`). A root without it is just the folder the entry
    was opened from (`~/Downloads`, say), so trusting it lasts until the app
    quits (`TRUSTED_FOR_NOW` in the editor, shared by its windows) and the
    question says it will be asked again.
  - `load_path_with` checks trust on the background thread before
    `load_source` runs anything. An untrusted project returns a
    `TrustRequest` and the window shows **Trust this project?**
    (`ask_to_trust`). **Trust and Open** trusts the root (`trust`) and opens
    it; **Don’t Open** keeps the current contents.
  - At start-up an untrusted command-line project opens the demo and asks
    once the window's root exists (`window.defer` in `main.rs`). A trusted
    one that fails to open (see below) opens the demo too and is tried
    again in the window (`StartUp::OpenAgain`), instead of the app exiting
    with the error on a console a release build does not have.
  - **File > Create New Project** trusts the folder it initialized; a
    failure to save that is reported after the load starts, which clears
    the project's problems. Initializing a project (`initialize_project`:
    the menu, `--init`, and MCP's `init_project`) writes `{}` to
    `celesta.config.json` when it is missing, so the new folder is its own
    root, even inside another project, and its trust is saved. Opening or
    trusting a project never writes the file: a downloaded project is not
    changed behind the user's back.
  - The question says when the entry has a Mark of the Web naming the
    internet or restricted zone (`from_the_internet`, Windows only). The
    stream is read as bytes: UTF-16 with its byte-order mark, else loosely
    as UTF-8 (`zone_text`), since a writer may use a local code page.
- **Project plugin folders** (`project_plugin_paths`): `plugins.paths` in
  the root's `celesta.config.json`. Every entry must be relative and inside
  the root, by its text (`is_contained_path`) and with links resolved, and
  must exist; a root that cannot be resolved refuses them all. A bad entry
  fails the whole list, reported in Problems. They are searched as
  `PluginSource::Project`, after the explicit folders and before the user
  folder, and only for a trusted project (`trusted_root`). Unix permission
  checks apply to them like user folders. The exporter and MCP do not read
  the file.
- **Approval** (`InstalledPlugins::allowed_by`): plugins from the user
  folder and from a project (`PluginSource::needs_allowance`, an exhaustive
  match so a new source must be decided) run only when `settings.json`
  allows that id in that directory with the same fingerprint. The
  fingerprint (`fingerprint`) is SHA-256 over the manifest and every file it
  names (a missing one as empty; each with its name and length); other
  files in the folder are not part of it (see Open questions). Explicit and
  bundled plugins are never asked about. A waiting plugin neither runs nor
  gives way to a later plugin with its id. Plugins whose files cannot be
  read are skipped.
  - After an open, `ask_about_pending_plugins` lists the waiting plugins
    with their version, source, folder, "changed since you allowed it", and
    "downloaded from the internet" (any of its files marked). **Allow and
    Reload** saves them and reopens the project; **Not Now** leaves them
    waiting. A plugin (directory and fingerprint) is asked about once per
    window (`asked_plugins` carries over reopens).
  - **An entry that imports a waiting plugin cannot open**: the runtime
    gets only allowed plugins, so bundling fails. (Changed on top of the
    stack: it now opens with stand-ins, and only calling the plugin
    fails; see `2026-11-02-plugin-waiting-stand-in.md`.) `load_path_with` keeps
    that open's discovery: the failure in Problems names the waiting
    plugins (`FoundPlugins::failure_note`), and the approval question is
    asked for them (`ask_to_allow`), whose **Allow and Reload** opens that
    project again with the same props, not the one still shown.
  - Problems (`Source::Plugins`) lists what waits and what was skipped.
- **TypeScript declarations**: on every open the editor gives
  `refresh_project_types` its own discovery (`FoundPlugins::declared`):
  allowed and waiting plugins, a trusted project's own plugins included.
  Declarations run nothing.
- **Settings** (`UserSettings`, `settings.json` in `user_settings_dir`:
  `%APPDATA%\Celesta`, `~/Library/Application Support/Celesta`,
  `$XDG_CONFIG_HOME/celesta` or `~/.config/celesta`): `trustedProjects` and
  `allowedPlugins` (`id`, `directory`, `fingerprint`); unknown keys are kept.
  Written whole through a temporary file and renamed; on Unix the file is
  created 0600 and missing folders 0700 whatever the umask (with umask 002
  a plain write made it group-writable, and the next load refused it). On
  load, folders that are not absolute are dropped (an empty one would have
  covered every path) and the rest have their links resolved. On Unix a
  settings file other users can change is refused (treated as empty,
  reported).

## Decisions from review

Questions raised in review, as the maintainer decided them:

- **Trust of a folder without `celesta.config.json`**: asked each time the
  app starts, and a trusted root covers only itself (above). Before, trust
  covered everything inside the folder, so trusting `~/Downloads/film.tsx`
  trusted every project later put under `~/Downloads`.
- **Create New Project inside another project**: the new project gets its
  own `celesta.config.json` (above), so it is its own root.
- **What the fingerprint covers**: kept as the manifest and the files it
  names, which the guide says. Hashing the whole folder (cost: time for
  large plugins, and re-approval whenever a plugin writes logs or caches
  beside itself) is tracked in mika-f/Celesta#652, undecided.
- **Command-line tools and approval**: `celesta-exporter` and MCP keep
  running the plugins they find without asking, for now (also in #652).
- **Settings that cannot be read**: kept. Everything falls back to empty
  settings: in the app nothing is trusted or allowed (fails closed), but a
  bundled or explicit plugin the user turned off (layer 3) runs again.
- **A waiting plugin in an entry**: the project should open with that
  plugin's layers undrawn rather than fail. Done on top of the stack, in
  `2026-11-02-plugin-waiting-stand-in.md`.

## Implementation

| Where | What |
| --- | --- |
| `crates/plugin-host/src/settings.rs` | `UserSettings`, `Allowance`, `user_settings_dir`. |
| `crates/plugin-host/src/project.rs` | `project_root`, `has_project_config`, `project_plugin_paths`, `PROJECT_CONFIG`. |
| `crates/react-bridge/src/project_templates.rs` | `initialize_project` writes `celesta.config.json` when missing. |
| `crates/plugin-host/src/approval.rs` | `fingerprint`, `from_the_internet`, `PendingPlugin`, `Allowed`, `InstalledPlugins::allowed_by`. |
| `crates/plugin-host/src/locations.rs` | `PluginSource::Project`; `search_paths(explicit, project)`. |
| `crates/editor/src/plugins.rs` | `FoundPlugins::find(project)`, `TrustRequest`, `trust_request`, `trusted_root`, `trust` and `TRUSTED_FOR_NOW`, the two dialogs. |
| `crates/editor/src/lifecycle.rs`, `main.rs` | `load_path_with` with the trust gate; start-up deferral; Create New Project trusts. |

## Checks

- `cargo test -p celesta-plugin-host`:
  - settings folders per platform, trust covering the root alone,
    allowances following fingerprints;
  - settings round-trip with unknown keys and no leftover temporary file,
    unreadable and (Unix) unsafe settings, saved settings 0600 in a 0700
    folder (Unix), empty and relative folders dropped on load;
  - project roots ignoring a `package.json` above, `plugins.paths`
    containment (text and links) and existence;
  - fingerprints changing with the module, the program for this platform
    or another, and the manifest; a plugin whose files cannot be read
    neither runs nor waits; waiting by source;
  - the Mark of the Web: zone parsing, UTF-16 and non-UTF-8 streams, and a
    real `Zone.Identifier` stream on Windows.
- `cargo test -p celesta-react-bridge project_templates`: a new project
  gets `celesta.config.json`; an existing one is kept.
- `cargo test -p celesta-editor`: the Problems entry; with settings given
  rather than the developer's own, a project asked about until trusted, a
  project inside a trusted one asked about on its own, a folder without
  `celesta.config.json` trusted for now but not saved, and
  a project's plugin unsearched while untrusted, waiting (and off the
  runtime's path, but declared) once trusted, and running once allowed.
- The editor run on Windows 11 with `APPDATA` pointed at a scratch folder
  holding the flag sample as a user plugin: an untrusted project showed the
  trust question over the demo; **Trust and Open** saved the folder and
  opened it; the approval question listed `flag 0.1.0 · user plugins`;
  **Allow and Reload** saved its fingerprint and the warning cleared. A
  project whose entry carries a Mark of the Web showed the download
  warning. Screenshots are on the pull request.
