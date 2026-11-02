# Stand-ins for plugins that wait for approval

The fifth layer of making content plugins usable from the desktop editor, on
top of `2026-11-02-plugin-add-menu.md`. Before it, an entry that imported a
plugin the user had not allowed yet did not open at all
(`2026-11-02-plugin-trust.md`): the runtime got only allowed plugins, so
bundling the entry failed. The maintainer chose to let such a project open
with that plugin's layers undrawn instead.

## Specification

- **What waits** (`InstalledPlugins::waiting`): `allowed_by` puts every
  pending plugin's entry there as well as in `Allowed::pending`. Plugins the
  user turned off are not waiting, and an entry importing one still fails
  to bundle. Command-line discovery (`InstalledPlugins::find`) never waits
  for anything.
- **What the runtime is told** (`InstalledPlugins::runtime` →
  `RuntimePlugins`): `CELESTA_PLUGIN_PATH` as before, and
  `CELESTA_WAITING_PLUGIN_PATH` (`WAITING_PLUGIN_PATH_ENV`), the waiting
  plugins' directories as the same kind of path list. `SpawnOptions::plugins`
  carries both, and the bridge sets them with `Command::envs`, never through
  Celesta's own environment. Every runtime the editor starts gets them.
- **Resolving an import** (`resolvePluginImport` in
  `packages/cli/src/plugins.ts`): a plugin that may run wins. Otherwise a
  waiting plugin with a module resolves to a stand-in, in esbuild namespace
  `celesta-waiting-plugin`. Anything else is the error it was before. A
  plugin that may run but cannot be imported (no module) keeps its own
  error.
- **The stand-in** (`standInSource`):
  - The plugin's module is bundled on its own, with packages external and
    nothing written, only for esbuild's metafile, which lists its exports.
    None of its code runs.
  - A capitalized export, or a default one, is taken for a component. It
    renders its `children`, else `null`: nothing of its own is drawn, so no
    plugin layer reaches the renderer and no plugin process is needed.
  - Any other export is a function that throws "plugin "<id>" waits for your
    approval in the Celesta app, and does not run until you allow it" when
    called. An entry that calls one (in `prepare`, say) fails with that
    message.
- **The editor**:
  - The approval question says that until a plugin is allowed, what it
    draws is left out and calling its functions fails.
  - A failed open still adds `FoundPlugins::failure_note` (reworded) and
    asks about the waiting plugins, whose **Allow and Reload** reopens that
    project.

Limits:

- Names an `export * from 'package'` brings in are not known. An entry
  importing one of them fails to bundle with esbuild's "No matching
  export".
- A value that is not a function (a constant, an object) stands in as a
  function too.

## Implementation

| Where | What |
| --- | --- |
| `crates/plugin-host/src/manifest.rs` | `InstalledPlugins::waiting`, `RuntimePlugins`, `InstalledPlugins::runtime`, `WAITING_PLUGIN_PATH_ENV`. |
| `crates/plugin-host/src/approval.rs` | `allowed_by` fills `waiting`. |
| `crates/react-bridge/src/types.rs`, `bridge.rs` | `SpawnOptions::plugins: Option<RuntimePlugins>`, set with `Command::envs`. |
| `crates/exporter/src/react.rs`, `inspect.rs`; the editor's runtimes | pass `runtime()` where they passed `runtime_path()`. |
| `packages/cli/src/plugins.ts` | `resolvePluginImport`, `StandIn`, `standInSource`. |
| `packages/cli/src/cli.ts` | the resolver and the stand-in loader in the entry's bundle. |
| `crates/editor/src/plugins.rs`, `lifecycle.rs` | dialog text and `failure_note`. |

## Checks

- `cargo test -p celesta-plugin-host`: waiting plugins are the runtime's
  waiting path and not its plugin path, a plugin turned off is not
  waiting, and `RuntimePlugins` names both variables.
- `cargo test -p celesta-editor`: a trusted project's waiting plugin is on
  the runtime's waiting path.
- `packages/cli/test/plugins.test.mts` (after `pnpm run build:runtime`):
  - imports resolve to an allowed plugin first, else to a stand-in;
  - a stand-in's components render `null` or their children, and its other
    exports throw;
  - an entry importing a waiting `flag` renders its other layers and no
    plugin layer;
  - an entry calling a waiting `beep-voice` in `prepare` fails saying it
    waits.
- The editor (release build) on Windows 11, with `APPDATA` pointed at a
  scratch folder holding the flag sample as a user plugin not yet allowed,
  and a trusted project (with `celesta.config.json`) whose entry draws a
  `Rect` and a `<Flag>`:
  - the project opened, and the approval question showed with the new
    text;
  - after **Not Now** the preview showed the `Rect` without the flag,
    Problems had no error and one warning (the waiting plugin);
  - **Allow** in the Plugins tab reloaded it with the flag drawn.
  - Before this layer, the same project failed to open (the screenshot
    from the trust layer's check).
  - The same build checked the trust layer's change: a folder without
    `celesta.config.json` asked with the "asks again the next time it
    starts" note, opened on **Trust and Open**, and was not written to
    `settings.json`.
