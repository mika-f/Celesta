# Plugin approval in command-line tools (2026-11-08)

Before this change, only the desktop app asked before running a plugin
(`2026-11-02-plugin-trust.md`). `celesta-exporter`, the MCP server, and
any React runtime started without the app's plugin set ran every plugin
they found, whether or not the user had allowed it, including one the app
asked about again after an update. This was left open in
mika-f/Celesta#652 ("CLI と MCP でもプラグインの承認を求めるか").

The maintainer chose:

- Command-line tools follow the app's approvals in `settings.json`.
- `celesta-exporter` gets `--allow-plugin <id>` to run a plugin for one
  export without the app.
- An entry that imports a plugin waiting for approval fails to load
  instead of rendering without that plugin's layers.

## Specification

- **Which plugins need approval** is unchanged
  (`PluginSource::needs_allowance`): the user's plugin folder and the
  folders they added. `--plugin-dir`, `CELESTA_PLUGIN_PATH`, and the
  bundled folder are never asked about. Command-line tools do not read a
  project's `celesta.config.json`, so project plugins do not reach them.
- **`InstalledPlugins::find(explicit, allow)`** (what the exporter, the
  MCP server, `PluginHost::discover`, and the React bridge's default use)
  now sorts what it finds with `allowed_allowing(settings, allow)`, the
  app's `allowed_by` plus `allow`:
  - A plugin turned off is left out, as before. `--allow-plugin` does not
    turn it on, and does not count as naming an unknown id.
  - A plugin that needs approval runs when `settings.json` allows that id
    in that folder with the same fingerprint, or when its id is in
    `allow`. Otherwise it is left out of `plugins`, listed in `waiting`,
    and reported as `DiscoveryError::WaitsForApproval`. The message says
    whether the plugin is new or changed since it was allowed, and how to
    allow it: the app's Plugins tab or `CELESTA_PLUGIN_PATH`, then what only
    the exporter takes (`--allow-plugin <id>`, `--plugin-dir`), since the
    MCP server shows the same text.
  - An id in `allow` that matches no plugin found (running, waiting, or
    turned off) is reported once as `DiscoveryError::UnknownAllowance`.
    `find_reporting` prints it as `celesta: --allow-plugin <id> names no
    plugin that can run: no folder has one with that id, or it was skipped
    (see why above)`, after the skipped plugins and without their "skipped
    a plugin" prefix: a plugin skipped as incompatible or invalid is not
    found either.
  - A plugin whose files cannot be fingerprinted is skipped and reported as
    `DiscoveryError::Unreadable`, as the app already did.
  - `find_allowing(explicit, allow, settings)` is the same with given
    settings, for tests.
- **What an import of a waiting plugin resolves to**
  (`InstalledPlugins::waiting_imports`, `WaitingImports`):
  - `allowed_by` / `allowed_allowing` set `StandIn`: the app's behaviour
    from `2026-11-02-plugin-waiting-stand-in.md`, unchanged.
  - `find` sets `Fail`, which is also the `Default`, so a set built by hand
    fails closed.
  - `RuntimePlugins` carries it, and `env()` now sets three variables:
    `CELESTA_PLUGIN_PATH`, `CELESTA_WAITING_PLUGIN_PATH`, and
    `CELESTA_WAITING_PLUGIN_IMPORTS` (`WAITING_IMPORTS_ENV`: `stand-in` or
    `fail`).
- **The React runtime** (`resolvePluginImport` in
  `packages/cli/src/plugins.ts`) takes a `standIns` flag, which `cli.ts`
  reads as `CELESTA_WAITING_PLUGIN_IMPORTS === 'stand-in'`. Any other value,
  or none, fails the import of a waiting plugin with:
  `plugin "<id>" waits for your approval in the Celesta app, and does not
  run until you allow it (<folder>): allow it in the app's Plugins tab, or
  name its folder in CELESTA_PLUGIN_PATH; celesta-exporter also takes
  --allow-plugin <id> and --plugin-dir`. A waiting plugin without a module
  fails, in either mode, with `plugin "<id>" has no JavaScript module`, as
  an allowed one does, since allowing it would not help. Plugins that may
  run, and ids no folder provides, resolve as before.
- **`celesta-exporter --allow-plugin <ID>`** is repeatable and is checked
  with `is_valid_id`. It covers both export and `--inspect`. It writes
  nothing to `settings.json`. The MCP server has no flag: the user allows
  the plugin in the app, or names its folder in `CELESTA_PLUGIN_PATH`.
- **Project TypeScript support** (`react-bridge::project_types`) still
  copies declarations of plugins that wait (`plugins` and `waiting`), since
  declarations run nothing.
- **Codecs**: a codec plugin that waits is left out like any other, so
  `InstalledPlugins::codec` picks the next codec in search order, usually
  the bundled `ffmpeg`, as the app already did.

This is a breaking change for anyone who keeps plugins in the user's
plugin folder and runs only `celesta-exporter` (for example on Linux
without the app). They now need to allow the plugin in the app once, pass
`--allow-plugin`, or put the folder on `CELESTA_PLUGIN_PATH`. The release
notes for 0.10 should say so.

## Implementation

| Where | What |
| --- | --- |
| `crates/plugin-host/src/manifest.rs` | `WAITING_IMPORTS_ENV`, `WaitingImports`, `InstalledPlugins::waiting_imports`, `RuntimePlugins::waiting_imports` and a three-variable `env()`, `DiscoveryError::{WaitsForApproval, UnknownAllowance, Unreadable}`, `find(explicit, allow)`, `find_allowing`, `find_reporting(explicit, allow)`. |
| `crates/plugin-host/src/approval.rs` | `allowed_allowing(settings, allow)`; `allowed_by` sets `StandIn`. |
| `crates/exporter/src/main.rs` | `--allow-plugin` and `parse_plugin_id`. |
| `crates/react-bridge/src/project_types.rs` | Copies waiting plugins' declarations too. |
| Other callers (`exporter.rs`, `codec.rs`, `bridge.rs`, `host.rs`, the benches, the codec test) | Pass `&[]` for `allow`. |
| `packages/cli/src/plugins.ts`, `cli.ts` | `WAITING_IMPORTS_ENV`, and `resolvePluginImport(…, standIns)`. |
| User guide (en, ja) `05-plugins.md`, `skills/celesta/references/plugins.md` | The new behaviour, `--allow-plugin`, and the new error. |

## Checks

- `cargo test -p celesta-plugin-host`, including
  `approval::tests::command_line_discovery_runs_only_what_the_app_or_the_command_allowed`.
  It covers: a user-added folder's plugin waits, is reported with the
  `--allow-plugin` hint, and fails imports; `--allow-plugin` runs it
  without touching the settings; it runs once the settings allow it; it
  waits again, as a change, once its module changes; a plugin turned off
  stays off under `--allow-plugin`, with no unknown-id report; a folder
  named explicitly runs without approval; an unknown id is reported once.
  `allowed_by` sets `StandIn`, and a hand-made set's runtime env says
  `fail`.
- `packages/cli/test/plugins.test.mts` (after `pnpm run build:runtime`):
  `resolvePluginImport` without stand-ins fails with the hint and still
  resolves allowed plugins. An entry importing a waiting `<Flag>` fails
  with the variable unset and with `fail`. The stand-in tests now set
  `stand-in` explicitly.
- `cargo clippy` with `-D warnings`, and the tests of the crates whose
  callers changed: `celesta-exporter` (lib and bins), `celesta-react-bridge`,
  `celesta-codec-test`, and `celesta-editor`'s plugin tests.

- End to end on macOS with the debug `celesta-exporter --react --inspect`.
  `HOME` pointed at a scratch folder, and `beep-voice` (built with
  `scripts/plugins/build.mts --out`) sat in its user plugin folder. The
  entry called `prepareBeepNarration` in `prepare`. Results:
  - Without approval, stderr said the plugin was skipped with the hint, and
    the request failed with the import error naming the plugin and its
    folder.
  - With `--allow-plugin beep-voice`, and with the folder given as
    `--plugin-dir`, the frame evaluated.
  - `--allow-plugin nope` printed `celesta: --allow-plugin nope names no
    plugin that was found`.
  - No `settings.json` was written.
  - Node had to be on `PATH` directly: mise's shim fails under a changed
    `HOME`.

Not done:

- The MCP server cannot allow a plugin for one call. An MCP client-side
  confirmation, suggested in #652, is not built.
