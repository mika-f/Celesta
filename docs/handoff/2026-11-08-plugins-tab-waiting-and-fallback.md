# Plugins tab: the waiting state, and falling back from a plugin turned off

Settles the two open questions of `2026-11-02-plugins-panel.md`.

## Specification

- **A waiting plugin's switch.** A plugin that waits for approval does not
  run, so its switch no longer shows on. It shows off and greyed out
  (disabled), with the tooltip "Runs once allowed"; the row still says
  "Waits for approval" (or "Changed: waits for approval") in the warning
  colour. Since the switch cannot be used while the plugin waits, the row
  offers **Turn Off** beside **Allow**, which saves the plugin in
  `disabledPlugins` as the switch would. On and Off rows are unchanged.
- **Falling back from a plugin turned off.** A plugin the user turned off
  now gives way to the next plugin with its id in search order, as a
  plugin skipped by discovery already did. Turning off a user-folder
  `ffmpeg` brings back the bundled one, say. Decided with the user on
  2026-11-08, over keeping a turned-off plugin's id taken:
  - only the user turns a plugin off, so this gives no one a new way to put
    a plugin in place of another;
  - the plugin that takes its place is treated as it would be anywhere: one
    from a source that needs approval still waits, and a waiting one still
    does not give way;
  - the tab lists both: the plugin turned off, and the one used in its
    place. Turning the id off entirely means turning off each.
- **A waiting plugin still does not give way**, so a plugin the user has not
  allowed is not silently replaced by a different one further down the list
  (for an update that waits, say, the user would otherwise get another
  version than the one the tab shows).
- Codec selection (`InstalledPlugins::codec`) needs no change of its own:
  the plugin that takes the place of one turned off is among the plugins
  that may run, so the codec is picked from it as from any other.

## Implementation

| Where | What |
| --- | --- |
| `crates/plugin-host/src/manifest.rs` | `discover_in_with(paths, turned_off)`: a plugin `turned_off` names is listed but takes no id. `discover_in` is it with nothing turned off. `InstalledPlugins::find_with` passes `settings.is_disabled`, so both `find` (exporter, MCP) and the editor's discovery fall back. |
| `crates/plugin-host/src/approval.rs` | `allowed_by`'s documentation: a plugin turned off gives way; a waiting one does not. |
| `crates/editor/src/plugins_panel.rs` | `PluginRow::switch` (`SwitchLook`: on, can change, tooltip); the waiting row's disabled switch and **Turn Off** button. |
| `packages/website/content/{en,ja}/docs/05-(more)/05-plugins.md`, `skills/celesta/references/plugins.md` | The fallback, and the waiting switch. |

PR mika-f/Celesta#761 (the exporter and MCP following the app's approvals)
changes `InstalledPlugins::find` and adds `allowed_allowing`; whichever lands
second rebases. The fallback lives in `find_with`, below both, so it should
carry over unchanged.

## Checks

- `cargo test -p celesta-plugin-host`: a user-folder plugin turned off
  gives way to a later one with its id (listed in `disabled`, the later one
  runs); the one in its place from an added folder still waits; a waiting
  one does not give way (the later one is not found); a plugin
  turned off after one with its id that is used is not listed; with every
  copy of an id turned off, nothing with it runs.
- `cargo test -p celesta-editor`: the switch's look per state (waiting:
  off, disabled, "Runs once allowed"); a plugin turned off listed beside
  the one used in its place.
- The editor on macOS, with `HOME` pointed at a scratch folder holding
  `beep-voice` (new) and `badge` 2.0.0 (turned off) as user plugins, and an added folder holding `badge` 1.0.0 (allowed).
  Before: beep-voice's switch on; the added folder's `badge` not listed.
  After: beep-voice's switch off and greyed, with **Turn Off**; the added
  folder's `badge` 1.0.0 listed On under the user one turned off.
  Screenshots are on the pull request. For them, a local patch (not
  committed) opened the Plugins tab at start, widened the right dock to its
  largest (520 px), and left out the approval dialog and problem toasts, so
  nothing had to be clicked. Neither the switch's tooltip (hover) nor
  **Turn Off** was driven.
