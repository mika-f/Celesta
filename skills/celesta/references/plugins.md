# Content plugins

Plugins add components Celesta does not ship: layers a plugin's own program
draws (such as `@celesta-plugin/flag`) or component libraries such as a
speech channel (`@celesta-plugin/beep-voice`). They are new and may change.

## Using one

- Import its components as `@celesta-plugin/<id>`. Never `npm install` it:
  the runtime finds the plugin's folder and bundles its module into the
  entry.
- Celesta finds plugins, first of an id winning, in: each `--plugin-dir`
  (`Celesta-export`), each folder of `CELESTA_PLUGIN_PATH` (`;` on Windows,
  `:` elsewhere; absolute paths only, relative ones are skipped), the user's
  plugin folder (`%APPDATA%\Celesta\plugins`,
  `~/Library/Application Support/Celesta/plugins`, or
  `$XDG_DATA_HOME/celesta/plugins` / `~/.local/share/celesta/plugins`),
  folders the user added in the app's Plugins tab, and the folder bundled
  with Celesta; the app also searches a trusted project's own folders after
  `CELESTA_PLUGIN_PATH`. A folder is a plugin
  (it has `celesta-plugin.json`) or holds several. On Unix, a plugin from
  anywhere but `--plugin-dir` and `CELESTA_PLUGIN_PATH` that other users
  can write to (any file in it) is skipped.
- The app looks again each time a project opens and on File > Reload
  Plugins; skipped plugins show in its Problems tab. Its Plugins tab lists
  every plugin with its source and state, and lets the user allow, turn
  off, and add folders of plugins (kept in their `settings.json`). A plugin
  turned off there does not run in `Celesta-export` either.
- A plugin whose manifest lists `components` offers them in the app's Add
  menu; the app writes the element with its `@celesta-plugin/<id>` import.
  Code written by hand works the same.
- The app asks the user to trust a project folder before it evaluates
  anything in it (remembered only when the root holds
  `celesta.config.json`, which `--init` writes; a project inside it with
  its own file is asked about separately), and to allow each plugin from
  the user's folder, an added folder, or a project (again
  when its manifest or a file the manifest names changes). Until allowed, a
  plugin does not run: an entry importing it opens in the app with its
  components drawing nothing (missing layers are expected, not a bug in the
  entry), and calling its other exports throws "waits for your approval";
  Problems says it is waiting. Only the user can answer: tell them to,
  rather than working around it. `Celesta-export` and MCP do not ask: they run the
  plugins they find (not a project's own) whether or not the app allowed
  them.
- A project may list its own plugin folders in `celesta.config.json` at its
  root: `{ "plugins": { "paths": ["./plugins"] } }`, relative and inside
  the project. Only the app reads it (after trust); give `Celesta-export`
  the same folders with `--plugin-dir`.
- Check and export with the folder named:

  ```sh
  Celesta-export --json --react film.tsx --frames 0,60 /tmp/celesta-check.png --plugin-dir <plugins>
  CELESTA_PLUGIN_PATH=<plugins> node <this skill>/scripts/inspect.mjs film.tsx --frames 0
  ```

  `inspect.mjs` lists a plugin layer as `plugin <id> at <time> s` with its
  assets (checked like other media) and `data`; only a rendered frame shows
  what it draws.
- Plugin layers draw in the app's preview and `Celesta-export` only, not in
  `@celesta/web`.
- The usual rules hold: props must depend only on the frame, and async work
  (a speech channel's synthesis) goes in `prepare()`.
- `.celesta/plugins/<id>/index.d.ts` holds each plugin's declarations for
  type-checking, refreshed with the rest of `.celesta/` (in the app, a
  trusted project's own plugins and plugins waiting for approval too).

## Errors

| Message | Fix |
| --- | --- |
| `no plugin "<id>" is installed (looked in CELESTA_PLUGIN_PATH)` | In the app, the plugin may be waiting for the user's approval (Problems says so): ask the user to allow it, and do not work around the question. Otherwise the entry imports a plugin no folder provides: put it in the user plugin folder, pass `--plugin-dir`, or set `CELESTA_PLUGIN_PATH` (absolute paths). |
| `…; <folder> was skipped: <reason>` | That folder's manifest has a bad id or names a file outside the folder (`..`, an absolute path, a link out). The plugin must be fixed; do not work around it. |
| `plugin "<id>" has no JavaScript module` | The plugin has only a program; its layers come from another package. |
| `… has no executable for this platform` | The plugin was not built for this OS and CPU. |
| `plugin "<id>": …` | The plugin's own error, such as a missing asset. |
| `plugin layer <layer> cannot be drawn: …` | The plugin answered with meshes or a scene that fails Celesta's checks (bad indices, counts that do not match, a camera that sees nothing, more than 4 directional lights, a material shader that does not compile, parameters of the wrong size, a material that needs tangents on geometry without them). The plugin must be fixed. |
| `<id> (layer <layer>): …`, or `note: <id> (layer <layer>): …` for information | Not an error: the plugin's notice about a model it opened, such as its terms of use. The exporter's terminal output prints a warning as a `warning: …` line; the app's Problems tab and `--json` (either kind among `warnings`) carry the message itself, a note keeping its `note: `. Relay it to the user; do not try to silence it. |

## Writing one

The manifest's optional `components` puts a component in the app's Add
menu: `{ "name": "Flag", "label": "Flag", "src": "image", "props": {
"amplitude": 0.06 } }`. `name` is the export (a capitalized identifier).
With `src` (`image` or `video`) it is added like an Image (`src`, `x`, `y`,
`width`); without, like a Rect's box (`x`, `y`, `width`, `height`). `props`
(strings, numbers, booleans) follow those and may not repeat them.

A plugin's program answers each frame with either 2D meshes or a 3D scene
(skinned, morphing geometry with materials and a camera; Celesta draws it
with depth, each material Celesta's unlit one or the plugin's own in
WGSL, shaded by the directional lights (at most 4) and
spherical-harmonics ambient light and able to add an outline; a frame can
replace a material's parameters), and may return notices when it opens a
model.

Start from the samples in the Celesta repository's `examples/plugins`
(`flag`: a program that returns meshes; `turntable`: a program that returns
a 3D scene; `beep-voice`: a module-only speech channel),
`docs/handoff/2026-11-01-plugin-system.md` (manifest and protocol), the
later `docs/handoff/2026-11-02-plugin-*.md` notes (search locations,
approval, `components`), `docs/handoff/2026-11-03-plugin-scene-content.md`
(scenes, notices, the current `ContentProvider`), and
`docs/handoff/2026-11-03-plugin-scene-materials.md` (materials in WGSL and
their interface, lights, render queues, outlines, per-frame parameters). A
plugin with a bad
component is skipped whole, like one with a bad id.
