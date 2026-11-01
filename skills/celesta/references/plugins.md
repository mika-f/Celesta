# Content plugins

Plugins add components Celesta does not ship: layers a plugin's own program
draws (such as `@celesta-plugin/flag`) or component libraries such as a
speech channel (`@celesta-plugin/beep-voice`). They are new and may change.

## Using one

- Import its components as `@celesta-plugin/<id>`. Never `npm install` it:
  the runtime finds the plugin's folder and bundles its module into the
  entry.
- Celesta finds plugins in each folder of `CELESTA_PLUGIN_PATH` (`;` on
  Windows, `:` elsewhere) and, for `Celesta-export`, each `--plugin-dir`.
  A folder is a plugin (it has `celesta-plugin.json`) or holds several.
  `CELESTA_PLUGIN_PATH` entries must be absolute; relative ones are skipped.
  The app's preview reads `CELESTA_PLUGIN_PATH` from the environment it was
  started in.
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
| `no plugin "<id>" is installed (looked in CELESTA_PLUGIN_PATH)` | The entry imports a plugin no folder provides. Pass `--plugin-dir` or set `CELESTA_PLUGIN_PATH` (absolute paths). |
| `…; <folder> was skipped: <reason>` | That folder's manifest has a bad id or names a file outside the folder (`..`, an absolute path, a link out). The plugin must be fixed; do not work around it. |
| `plugin "<id>" has no JavaScript module` | The plugin has only a program; its layers come from another package. |
| `… has no executable for this platform` | The plugin was not built for this OS and CPU. |
| `plugin "<id>": …` | The plugin's own error, such as a missing asset. |

## Writing one

Start from the samples in the Celesta repository's `examples/plugins`
(`flag`: a program that returns meshes; `beep-voice`: a module-only speech
channel) and `docs/handoff/2026-11-01-plugin-system.md` (manifest and
protocol).
