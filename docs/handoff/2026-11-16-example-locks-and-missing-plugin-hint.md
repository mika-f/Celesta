# Example locks and the missing-plugin hint (2026-11-16)

Opening `examples/delta` in Celesta 0.10.0-alpha.4 failed with

```
components/DiffPanel.tsx:2:30: ERROR: [plugin: shared-runtime] no plugin "code" is installed (looked in CELESTA_PLUGIN_PATH)
```

## Cause

- Since alpha.4, `code`, `lottie`, `voicevox`, and `aivis-speech` are
  registry plugins and are not bundled (`scripts/package-media-plugins.py`).
  The macOS app's `Contents/Resources/PlugIns` holds only `avfoundation` and
  `ffmpeg`.
- c321ff63 moved the examples to `@celesta-plugin/<id>` and declared the
  plugins in `celesta.config.json`, leaving the `celesta.lock` files "once the
  plugins are published". They were published with alpha.4, but no example had
  a lock.
- Without a lock, nothing downloads the plugins: the app asks to download only
  what a lock names, and puts `NoLock` in Problems with **Update Plugins…** in
  the This project heading (`docs/handoff/2026-11-16-plugin-registry-app.md`).
  The import then failed with a message about `CELESTA_PLUGIN_PATH`, which the
  user never set and which said nothing about the lock.

## Changes

- `celesta.lock` for every example with `plugins.dependencies`: delta,
  font-stack, shorts-explainer, shorts-tips, spectra, versus, zunda. Written by
  the alpha.4 app's `Celesta-cli plugin install --project examples/<name>`;
  each locks the published `0.10.0-alpha.4` (the requirements, written as
  `0.10.0-alpha.3`, allow it), resolved with Celesta 0.10.0-alpha.4, for the
  release targets.
- `packages/cli/src/plugins.ts`: `readProjectPlugins(directory)` finds the
  entry's project as `celesta-plugin-host`'s `project_root` does (the nearest
  folder up with `celesta.config.json`) and reads the ids its
  `plugins.dependencies` declares and its `celesta.lock` names. It only words
  the error: discovery in Rust still decides what runs, and versions are not
  compared here. `resolvePluginModule` and `resolvePluginImport` take it as an
  optional `project`; for a plugin the project declares, the message says
  which case it is and what to do:
  - no `celesta.lock`: Update Plugins… in the app, or `celesta plugin install`;
  - the lock does not name it: the same;
  - the lock names it but no copy can run here: trust the project and choose
    Download (Problems says why a copy cannot run), or
    `celesta plugin install`;
  - the lock cannot be read: restore it from version control.
  A plugin the project does not declare keeps
  `(looked in CELESTA_PLUGIN_PATH)`.
- `packages/cli/src/cli.ts`: the shared-runtime plugin reads the project only
  when an import fails, once per bundle.
- User guide (`05-plugins.md`, en and ja) and the Agent Skill's error table
  (`skills/celesta/references/plugins.md`) describe the new messages.

## Checks

- `pnpm run build:runtime`, then `pnpm test` in `packages/cli`: 23 files, 123
  tests passed, including the three new ones (the end-to-end entry in a project
  without a lock, `readProjectPlugins`, and each message).
- With the installed 0.10.0-alpha.4 app, after `python3 make-score-0.9.0.py`:
  `Celesta-export --json --react --composition poster film-0.9.0.tsx --frame 0`
  in `examples/delta` returned `"status":"ok"`, so the locked `code` and
  `lottie` resolve.

## Not covered

- The new message itself reaches the app only with a build of this branch; the
  installed alpha.4 app still shows the old one.
- The checkout the app opened (`examples/delta` in the main checkout) gets the
  lock only once this is merged; until then, Update Plugins… there writes the
  same lock.
