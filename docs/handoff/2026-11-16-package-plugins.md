# The four packages and vrm as registry plugins (2026-11-16)

Carries out [Migrating four packages to plugins](2026-11-14-plugin-registry-design.md#migrating-four-packages-to-plugins)
of the registry design, and publishes `vrm`.

## Scope (agreed with the maintainer)

- `@celesta.studio/aivis-speech`, `code`, `lottie`, and `voicevox` become the
  component-only first-party plugins `aivis-speech`, `code`, `lottie`, and
  `voicevox`, imported as `@celesta-plugin/<id>`. `vrm` is published too,
  after a public plugin-layer API replaced its use of
  `@celesta.studio/react/internal`.
- Every first-party plugin follows Celesta's version. This change leaves them
  at the workspace version (0.10.0-alpha.3); `plugin-release.yml` publishes
  them on the next release tag, after the usual bump pull request.
- The old `@celesta.studio/<id>` imports stay bundled in the runtime and warn.
- The examples move to the new imports and to `plugins.dependencies` here;
  their `celesta.lock` files come after the plugins are published (below).

## What changed

### `<PluginLayer>` (`packages/react/src/plugin-layer.ts`)

`<PluginLayer plugin assets time data {...common}>` renders the host element
`plugin-layer`, whose content is `{ type: 'plugin', plugin, assets, time,
data }`: each asset resolved as `src` is, `time` the enclosing sequence's
(`useCurrentFrame() / fps`) unless given, and `assets` and `data` left out
when not given. It throws on an empty `plugin` or a time that is not finite.
`<Vrm>`, `<Flag>`, and `<Turntable>` render it, so no plugin module in the
repository calls `registerHostElement`; `vrmElementProps` now returns
`PluginLayerProps` (the `.vrm` is asset `model`, each motion `motion<i>`).
`beep-voice` still imports `entryRelativePath` from `/internal`: it has no
layer, and it is a sample that is not published.

### The component-only plugins

- `plugins/<id>/plugin.json` gives `module: ../../packages/<id>/src/index.ts`.
  The source stays in `packages/<id>`, so the bundled package and the plugin
  are one source. `plugins/code/LICENSE-twinkleplop` is the MIT text every
  bundled `@twinkleplop/*` package carries (identical in all of them;
  `packages/code/test/plugin-license.test.mts` fails if one differs).
- `scripts/plugins/inputs.mts`: a plugin's module belongs to the nearest
  `package.json` above it (`js/package.json` for vrm, `packages/<id>` here),
  so CI's affected-plugin detection tests `@celesta.studio/<id>` when a
  plugin's inputs change.
- `scripts/plugins/build.mts` copies every declaration file of the module
  (a module of several files has an `index.d.ts` that re-exports from the
  others), and `crates/react-bridge/src/project_types.rs` copies the `.d.ts`
  files beside and below the manifest's `types` into
  `.celesta/plugins/<id>/`.
- `FOLLOWS_CELESTA` (`scripts/plugins/version.mts`) lists all nine plugins;
  `bump-version.mts` skips the Cargo.toml a component plugin lacks. vrm's
  Cargo package and plugin.json moved from 0.1.0 to the workspace version.

### Release (`.github/workflows/plugin-release.yml`)

- The version check covers every `plugins/*/plugin.json`.
- The per-system build job runs `pnpm install`, codegen, and
  `build:runtime` (vrm's module is checked against the runtime's types), and
  `package-media-plugins.py` packages vrm like the media plugins (signed and
  notarized on macOS; no extra libraries).
- A new `components` job (ubuntu-24.04) runs
  `package-media-plugins.py --registry --plugins aivis-speech code lottie voicevox`,
  which writes `target/registry/<id>/any.zip` (`package_component`), and
  uploads them as `registry-plugins-any` for the publish job, which now
  waits for it too.

### Old imports

`movedToPlugin` (`packages/react/src/package-scope.ts`) maps the four
package names to ids. The CLI keeps resolving them to the bundled packages
and reports each as a `LegacyImport` with `current: @celesta-plugin/<id>` and
`plugin`: what `celesta plugin add` takes, the id, or `id@<Celesta version>`
on a pre-release (a bare id matches no pre-release). The editor (Problems)
and the exporter (stderr) print:

```text
film.tsx imports @celesta.studio/lottie, which is now a plugin; run `celesta plugin add lottie@0.10.0-alpha.3` and import @celesta-plugin/lottie instead
```

The web editor serves `@celesta-plugin/code` and `@celesta-plugin/lottie`
from the same packages it already bundles (`packageOfPlugin`), without a
warning.

### Examples and docs

- delta, font-stack, shorts-explainer, shorts-tips, spectra, versus, and
  zunda import `@celesta-plugin/<id>`, drop the package dependencies, and
  declare `plugins.dependencies` (`"0.10.0-alpha.3"`, which matches the next
  pre-releases and 0.10.x) in a new `celesta.config.json`. Text on screen that
  names the packages as history (delta's chapter lists and its v0.6.0 code
  sample) is unchanged.
- `scripts/typecheck-examples.mjs` builds the first-party plugins the
  examples declare and stages their declarations; the website's
  `check:examples` stages the four from the runtime's copies
  (`packages/website/scripts/check-examples.mjs`).
- The user guide (en, ja): the four package pages, dialogue, reference
  (`PluginLayer`), and plugins (vrm from the registry, `<PluginLayer>`).
  The skill and the MCP docs index likewise.

## Checks

- Vitest: `packages/react` (`plugin-layer`, `package-scope`), `plugins/vrm/js`,
  `packages/cli` (`misplaced-import`: the moved-package warning),
  `packages/web` (a `@celesta-plugin/lottie` frame equals the
  `@celesta.studio/lottie` one), `packages/code` (`plugin-license`),
  `scripts/plugins` (inputs, bump-version), `packages/website`.
- Cargo: `celesta-react-bridge` (`ready_message_carries_legacy_imports`,
  the declarations tree test, the live legacy-import test), `celesta-mcp`.
- Python: `scripts/test_package_media_plugins.py` (every plugin packaged;
  a component plugin is one `any.zip`).
- Locally on macOS: `package-media-plugins.py --registry` wrote `any.zip`
  for the four and `macos-aarch64.zip` for vrm (ad-hoc signed), and
  `publish-plugins.py --dry-run` with a bundle naming those archives accepted
  all five.
- `celesta-exporter` (release build): the same Lottie frame from
  `@celesta.studio/lottie` (warning above) and from `@celesta-plugin/lottie`
  with the registry archive unpacked into a `--plugin-dir` are the same
  bytes. `examples/plugins/flag` and `turntable` frame 30 with the modules
  built from before and after `<PluginLayer>` are the same bytes.
- `node scripts/typecheck-examples.mjs`, `pnpm check:examples` (website).
- Not run: the release workflow itself (it runs only on tags), and Linux and
  Windows packaging of vrm.

## Left for the maintainer

1. Merge, then bump to the next version (for example
   `node scripts/plugins/bump-version.mts 0.10.0-alpha.4`, plus the
   `package.json` files by hand) and tag it. `plugin-release.yml` publishes
   aivis-speech, code, lottie, voicevox (`any`) and vrm (three targets) with
   the media plugins.
2. Phase 1 of the registry accepts first-party ids without a dot from this
   repository's release workflow, so no registry change should be needed for
   the new ids; if the registry keeps an allow-list of first-party ids, add
   these five there first.
3. After publishing, in each example that declares plugins, run
   `celesta plugin install` and commit the `celesta.lock` it writes. Until
   then those examples fail in `celesta-exporter` and MCP with "the project
   declares registry plugins but has no celesta.lock".
