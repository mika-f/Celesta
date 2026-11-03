# Plugin search locations shared by the editor, exporter, and MCP server

The first layer of making content plugins usable from the desktop editor
(see `2026-11-01-plugin-system.md` for the plugin system itself). Before
this, plugins came only from `CELESTA_PLUGIN_PATH` and the exporter's
`--plugin-dir`, which the exporter put into its own environment with
`set_var` for the React runtime to inherit. A GUI user rarely sets an
environment variable, and the editor looked only once, when it started.

## Specification

- Celesta looks for plugins in this order, the first plugin of an id
  winning (`celesta_plugin_host::search_paths`):
  1. `--plugin-dir` (exporter), made absolute against the current directory
     when given;
  2. each absolute entry of `CELESTA_PLUGIN_PATH` (relative ones are
     reported and skipped);
  3. the user's plugin folder (`user_plugin_dir`): `%APPDATA%\Celesta\plugins`
     on Windows, `$HOME/Library/Application Support/Celesta/plugins` on
     macOS, `$XDG_DATA_HOME/celesta/plugins` else
     `$HOME/.local/share/celesta/plugins` elsewhere. Variables that are
     missing or not absolute give no folder.
  4. the bundled folder (`bundled_plugin_dir`): `CELESTA_SYSTEM_PLUGIN_DIR`
     when a package build sets it (checked absolute, without `..`, by
     `crates/plugin-host/build.rs` under the target's rules), otherwise from
     the running executable with links resolved: `<exe dir>\plugins` on
     Windows, `Contents/PlugIns` inside a macOS `.app`, and
     `<exe dir>/../lib/celesta/plugins` elsewhere (so `/opt/celesta/bin`
     uses `/opt/celesta/lib/celesta/plugins`, `/usr/bin` uses
     `/usr/lib/celesta/plugins`). When the executable cannot be found there
     is no bundled folder; the current directory is never used.
- No location depends on the current directory.
- On Unix, a user or bundled folder, or anything inside a plugin directory
  from one (walked without following links, since a program loads
  libraries and a module imports files the manifest does not name), that is
  group- or other-writable, or belongs to neither root nor the current
  user, is skipped and reported (`unsafe_reason`, ssh's rule). Explicit
  folders (1, 2) are the caller's choice and are not judged. Windows ACLs
  are not judged.
- A plugin directory whose path contains the platform's path-list separator
  (`;` on Windows, `:` elsewhere) or a `"` is skipped, since it could not be
  handed to the React runtime. `join_paths` does not catch it: on Windows it
  quotes an entry with `;`, which the runtime does not unquote.
- A folder that exists but cannot be read is reported; a plugin folder
  whose own path cannot be resolved is skipped (its links could not be
  checked).
- One discovery serves everything: `InstalledPlugins::find` returns the set
  and what was skipped; `host()` makes the renderer's `PluginHost`, and
  `runtime_path()` the React runtime's `CELESTA_PLUGIN_PATH` (each plugin
  directory, in order). The runtime gets it through `Command::env`
  (`SpawnOptions::plugin_path`); `None` there discovers the standard
  locations in the bridge, so the runtime never sees Celesta's own
  environment value as it is.
- The exporter: `ExportOptions::plugins` (`None` discovers once in
  `Exporter::new`); the CLI discovers with `--plugin-dir` and passes the set
  (no more `set_var`); `inspect::run` takes the set too. The MCP server's
  exports and inspection use the standard locations through those defaults.
- The editor discovers on every project open (`load_path`'s background
  step, and `EditorView::open` at start-up) and on **File > Reload Plugins**
  (`ReloadPlugins`), which reopens the project so the renderer's plugin
  processes and every React runtime start over. The view keeps the set
  (`EditorView::plugins`) and gives it to the preview renderer, every React
  runtime it starts (initial load, reload, preview, occurrence sweep, audio
  sweep, component schemas), and exports. What was skipped is one warning
  in Problems (`Source::Plugins`).
- Project TypeScript support copies the declarations of the plugins in the
  same standard locations.

## Implementation

| Where | What |
| --- | --- |
| `crates/plugin-host/src/locations.rs` | `PluginSource`, `SearchPath`, `search_paths`, `user_plugin_dir`, `bundled_plugin_dir`, `unsafe_reason`. |
| `crates/plugin-host/build.rs` | Checks and compiles in `CELESTA_SYSTEM_PLUGIN_DIR`. |
| `crates/plugin-host/src/manifest.rs` | `PluginEntry::source`, `discover_in`, `InstalledPlugins`. `discover` treats its paths as explicit. |
| `crates/react-bridge` | `SpawnOptions::plugin_path`, set with `Command::env`. |
| `crates/exporter` | `ExportOptions::plugins`, `Exporter::plugins`, `export_renderer` and `ReactRuntimeOptions::spawn` take the set; `main.rs` drops `set_var`. |
| `crates/editor/src/plugins.rs` | `FoundPlugins` (the set and what was skipped) and the Problems entry; `ReloadPlugins` in `lifecycle.rs`. |

## Checks

- `cargo test -p celesta-plugin-host`: the user folder per platform
  (including relative variables giving none), the bundled folder per layout,
  search order, Unix permission checks (folders, a single plugin, a file
  the manifest does not name, and explicit folders exempt), the runtime
  path, and a folder named with the path-list separator skipped (and left
  out of a hand-made set's runtime path).
- `cargo test -p celesta-editor`: the skipped-plugins problem, and exports
  carrying the editor's plugins.
- `celesta-exporter --react examples/plugins/flag/demo.tsx --frames 40` with
  the built flag plugin only in a user plugin folder (`APPDATA` pointed at a
  scratch folder, no `CELESTA_PLUGIN_PATH`, no `--plugin-dir`) on Windows 11:
  the frame renders, so the renderer and the React runtime both found it.
