# The editor's Plugins tab

The third layer of making content plugins usable from the desktop editor,
on top of `2026-11-02-plugin-trust.md`: a tab that shows what was found and
lets the user change it without editing files or environment variables.

## Specification

- **Plugins** is a right-dock tab (`RightTab::Plugins`) offered in both
  modes, between Properties (or Details) and Problems. Its label shows how
  many plugins wait for approval. With four tabs the dock's default width no
  longer fits every label, so the tab bar shows gpui-component's overflow
  menu (`TabBar::menu(true)`).
- Each plugin the last discovery found is listed (`PluginRow::all`), by
  source in search order, then by id, with:
  - its id, version, and source;
  - whether it brings only components, a program built for this computer,
    or a program built for others only;
  - its folder;
  - its state: On, Waits for approval (or Changed: waits for approval), or
    Off;
  - a download warning for a waiting plugin with a Mark of the Web.
- Actions, each saved with `UserSettings::update` before the project
  reopens through `ReloadPlugins`:
  - **Allow** (waiting plugins) stores the plugin's current fingerprint.
  - The switch turns a plugin on or off (`disabledPlugins`, by id and
    folder).
  - **Show Folder** opens the plugin's folder in the system file manager.
  - **Open Plugin Folder** opens the user's plugin folder, creating it
    first.
  - **Add Folder…** asks for a folder (`pluginFolders`, absolute only);
    **Remove** takes one out.
  - **Reload Plugins** is the same as File > Reload Plugins.
- Settings: `settings.json` gains `disabledPlugins` (`id`, `directory`) and
  `pluginFolders`. Added folders are searched as `PluginSource::Folder`
  after the user's plugin folder and before the bundled one. They are
  permission-checked on Unix and their plugins need approval, as the user
  folder's do.
- A plugin that is off does not run anywhere: `InstalledPlugins::find`
  (what the exporter and MCP use) reads the settings, searches the added
  folders, and leaves out plugins turned off. `allowed_by` sorts them into
  `Allowed::disabled` for the editor. Like a waiting plugin, a plugin that
  is off does not give way to a later plugin with its id.
- Skipped plugins (bad manifests, unsafe folders, unreadable settings) are
  listed at the bottom of the tab as well as in Problems.

## Implementation

| Where | What |
| --- | --- |
| `crates/plugin-host/src/settings.rs` | `PluginKey`, `disabled_plugins`, `plugin_folders`, `is_disabled`, `set_enabled`, `add_plugin_folder`, `remove_plugin_folder`. |
| `crates/plugin-host/src/locations.rs` | `PluginSource::Folder`; `search_paths(explicit, project, folders)`. |
| `crates/plugin-host/src/manifest.rs` | `InstalledPlugins::find` reads the settings; `find_with`. `DiscoveryError::Settings`. |
| `crates/plugin-host/src/approval.rs` | `Allowed::disabled`. |
| `crates/editor/src/plugins_panel.rs` | `PluginRow`, `PluginState`, the tab and its actions. |
| `crates/editor/src/plugins.rs` | `FoundPlugins::disabled`; the view keeps the whole `FoundPlugins`. |
| `crates/editor/src/view.rs`, `problems_panel.rs` | The tab and its overflow menu. |

## Checks

- `cargo test -p celesta-plugin-host`: turning plugins off and on, adding
  and removing folders (relative ones refused), search order with added
  folders, and plugins turned off in `allowed_by`.
- `cargo test -p celesta-editor`: rows listed by source and id with their
  states; availability per platform; the tab order.
- The editor run on Windows 11 with `APPDATA` pointed at a scratch folder
  holding the flag and beep-voice samples as user plugins (flag allowed
  before, beep-voice new):
  - the tab listed both, beep-voice waiting;
  - **Allow** saved it and the reopened project listed it On;
  - the switch turned flag Off and saved it to `disabledPlugins`;
  - the tab stayed selected across the reopen.

  Screenshots are on the pull request. Add Folder's native folder picker
  was not driven.
