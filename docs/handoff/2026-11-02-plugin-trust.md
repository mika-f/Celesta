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
  folders, links resolved; a folder covers what is inside it (component-wise,
  so `project-other` is not inside `project`).
  - `load_path_with` checks trust on the background thread before
    `load_source` runs anything. An untrusted project returns a
    `TrustRequest` and the window shows **Trust this project?**
    (`ask_to_trust`). **Trust and Open** saves the folder and opens it;
    **Don’t Open** keeps the current contents.
  - At start-up an untrusted command-line project opens the demo and asks
    once the window's root exists (`window.defer` in `main.rs`).
  - **File > Create New Project** trusts the folder it initialized.
  - The question says when the entry has a Mark of the Web naming the
    internet or restricted zone (`from_the_internet`, Windows only).
- **Project plugin folders** (`project_plugin_paths`): `plugins.paths` in
  the root's `celesta.config.json`. Every entry must be relative and inside
  the root, by its text (`is_contained_path`) and with links resolved; a
  bad entry fails the whole list, reported in Problems. They are searched as
  `PluginSource::Project`, after the explicit folders and before the user
  folder, and only for a trusted project (`trusted_root`). Unix permission
  checks apply to them like user folders. The exporter and MCP do not read
  the file.
- **Approval** (`InstalledPlugins::allowed_by`): plugins from the user
  folder and from a project (`PluginSource::needs_allowance`) run only when
  `settings.json` allows that id in that directory with the same
  fingerprint. The fingerprint (`fingerprint`) is SHA-256 over the manifest
  and every file it names that exists (each with its name and length).
  Explicit and bundled plugins are never asked about. A waiting plugin
  neither runs nor gives way to a later plugin with its id. Plugins whose
  files cannot be read are skipped.
  - After an open, `ask_about_pending_plugins` lists the waiting plugins
    with their version, source, folder, "changed since you allowed it", and
    "downloaded from the internet" (any of its files marked). **Allow and
    Reload** saves them and reopens the project; **Not Now** leaves them
    waiting. A plugin (directory and fingerprint) is asked about once per
    window (`asked_plugins` carries over reopens).
  - Problems (`Source::Plugins`) lists what waits and what was skipped.
- **Settings** (`UserSettings`, `settings.json` in `user_settings_dir`:
  `%APPDATA%\Celesta`, `~/Library/Application Support/Celesta`,
  `$XDG_CONFIG_HOME/celesta` or `~/.config/celesta`): `trustedProjects` and
  `allowedPlugins` (`id`, `directory`, `fingerprint`); unknown keys are kept.
  Written whole through a temporary file and renamed. On Unix a settings
  file other users can change is refused (treated as empty, reported).

## Implementation

| Where | What |
| --- | --- |
| `crates/plugin-host/src/settings.rs` | `UserSettings`, `Allowance`, `user_settings_dir`. |
| `crates/plugin-host/src/project.rs` | `project_root`, `project_plugin_paths`, `PROJECT_CONFIG`. |
| `crates/plugin-host/src/approval.rs` | `fingerprint`, `from_the_internet`, `PendingPlugin`, `Allowed`, `InstalledPlugins::allowed_by`. |
| `crates/plugin-host/src/locations.rs` | `PluginSource::Project`; `search_paths(explicit, project)`. |
| `crates/editor/src/plugins.rs` | `FoundPlugins::find(project)`, `TrustRequest`, `trust_request`, `trusted_root`, the two dialogs. |
| `crates/editor/src/lifecycle.rs`, `main.rs` | `load_path_with` with the trust gate; start-up deferral; Create New Project trusts. |

## Checks

- `cargo test -p celesta-plugin-host`: settings folders per platform,
  trust coverage, allowances following fingerprints, settings round-trip
  with unknown keys and no leftover temporary file, unreadable and (Unix)
  unsafe settings, project roots ignoring a `package.json` above,
  `plugins.paths` containment (text and links), fingerprints changing with
  files, waiting by source, and the Mark of the Web (zone parsing, and a
  real `Zone.Identifier` stream on Windows).
- `cargo test -p celesta-editor`: the Problems entry, and an untrusted
  temporary project producing a trust request.
- The editor run on Windows 11 with `APPDATA` pointed at a scratch folder
  holding the flag sample as a user plugin: an untrusted project showed the
  trust question over the demo; **Trust and Open** saved the folder and
  opened it; the approval question listed `flag 0.1.0 · user plugins`;
  **Allow and Reload** saved its fingerprint and the warning cleared. A
  project whose entry carries a Mark of the Web showed the download
  warning. Screenshots are on the pull request.
