# DELTA 0.10.0 declares its sample plugins as project plugins

## Problem

Opening `examples/delta/film-0.10.0.tsx` in the desktop app failed with:

```
v0.10.0/demos/FlagDemo.tsx:2:21: ERROR: [plugin: shared-runtime] no plugin "flag" is installed (looked in CELESTA_PLUGIN_PATH)
v0.10.0/demos/TurntableDemo.tsx:2:26: ERROR: [plugin: shared-runtime] no plugin "turntable" is installed (looked in CELESTA_PLUGIN_PATH)
```

`flag` and `turntable` are the repository's sample plugins
(`examples/plugins/`), not registry plugins. The film reached them only
when the user built `examples/plugins/dist` and added that folder in the
Plugins tab by hand, which the README mentioned in one line.

## Change

- `examples/delta/package.json`: `pnpm run plugins` builds both plugins
  into `examples/delta/plugins/` (`build.mts --out plugins`); `export` and
  `poster` pass `--plugin-dir plugins`.
- `examples/delta/celesta.config.json`: `plugins.paths: ["./plugins"]`, so
  the app finds the built plugins once the project is trusted. A project's
  plugin folders must sit inside its root, so the shared
  `examples/plugins/dist` could not be declared.
- `examples/delta/.gitignore`: ignores `/plugins/`.
- `examples/delta/README.md`: the open and export steps use the new folder.

Project plugins still wait for approval in the Plugins tab. Before
`pnpm run plugins` has run, the app reports the missing folder in Problems
(`plugins.paths entry "./plugins" is not a folder that exists`), and the
composition fails on the imports as before.

The manifests' `celesta` range comes from the repository version
(`>=0.10.0, <0.11.0`), so a 0.10.0 pre-release app (such as
0.10.0-alpha.4) skips these plugins; use 0.10.0 or later.

## Checks

- `pnpm run plugins` in `examples/delta` wrote `plugins/flag` and
  `plugins/turntable`; `git status` does not list them.
- `celesta-exporter --react --plugin-dir plugins --plugin-dir ../../plugins/dist ./film-0.10.0.tsx --frame 330`
  and `--frame 420` drew the plugin `<Flag>` (Δ01) and `<Turntable>` (Δ02).
- A throwaway test calling `project_plugin_paths(examples/delta)` and
  `InstalledPlugins::find_in` with those paths found `flag` and
  `turntable` with no errors (not committed).
- Not checked in the app GUI.
