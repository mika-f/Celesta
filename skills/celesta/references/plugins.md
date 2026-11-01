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
  `$XDG_DATA_HOME/celesta/plugins` / `~/.local/share/celesta/plugins`), and
  the folder bundled with Celesta. A folder is a plugin (it has
  `celesta-plugin.json`) or holds several. On macOS and Linux a user or
  bundled plugin that other users can write to is skipped.
- The app looks again each time a project opens and on File > Reload
  Plugins; skipped plugins show in its Problems tab.
- The app asks the user to trust a project folder before it evaluates
  anything in it, and to allow each user-folder or project plugin (again
  when its files change); a plugin waiting for approval draws nothing and
  shows in Problems. Only the user can answer: tell them to, rather than
  working around it. `Celesta-export` and MCP do not ask.
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
- `.celesta/plugins/<id>/index.d.ts` holds each installed plugin's
  declarations for type-checking; they are refreshed with the rest of
  `.celesta/`.

## Errors

| Message | Fix |
| --- | --- |
| `no plugin "<id>" is installed (looked in CELESTA_PLUGIN_PATH)` | The entry imports a plugin no folder provides. Put it in the user plugin folder, pass `--plugin-dir`, or set `CELESTA_PLUGIN_PATH` (absolute paths). |
| `…; <folder> was skipped: <reason>` | That folder's manifest has a bad id or names a file outside the folder (`..`, an absolute path, a link out). The plugin must be fixed; do not work around it. |
| `plugin "<id>" has no JavaScript module` | The plugin has only a program; its layers come from another package. |
| `… has no executable for this platform` | The plugin was not built for this OS and CPU. |
| `plugin "<id>": …` | The plugin's own error, such as a missing asset. |

## Writing one

Start from the samples in the Celesta repository's `examples/plugins`
(`flag`: a program that returns meshes; `beep-voice`: a module-only speech
channel) and `docs/handoff/2026-11-01-plugin-system.md` (manifest and
protocol).
