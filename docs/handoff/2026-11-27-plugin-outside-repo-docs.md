# Docs for a plugin outside the repository, as of 0.10 (2026-11-27)

What the repository said about building a content plugin kept in a
repository of its own dated from the first plugin system (0.9): crates
patched in from a checkout "until the crates are published",
`registerHostElement` from the runtime's `internal` entry, `@celesta/*`
imports, `build.mjs`, and 0.9 in every example. This note records what is
true in 0.10 and which docs changed. No code changed.

## What holds in 0.10

- **The crates are on crates.io.** A plugin's program builds from
  `celesta-plugin = "0.10"` with no `[patch.crates-io]` table and no
  checkout; `celesta-plugin-host` and `celesta-gpu-renderer` are there too,
  for tests that start the program through the host and draw it
  (`2026-11-04-crates-io-publish.md`). The table is now only for building
  against crates a checkout has and the registry does not.
- **The JavaScript still needs a checkout.** The `@celesta.studio/*`
  packages are private (`npm view @celesta.studio/react` answers 404), so a
  module's declarations, `tsc`, and `esbuild` come from a checkout's built
  runtime, through `scripts/plugins/build.mts`.
- **A module draws with `<PluginLayer>`** from `@celesta.studio/react`, and
  imports the runtime as `@celesta.studio/*`.
- **A manifest needs a `celesta` range** that includes the running Celesta
  (`2026-11-06-plugin-celesta-range.md`); the build script writes it.
- **`ContentProvider`** answers `Opened` and `ContentFrame`
  (`2026-11-03-plugin-scene-content.md`).

What `scripts/plugins/build.mts` expects of a plugin, which the docs did not
say:

| What | Otherwise |
| --- | --- |
| `plugin.json`'s `cargoPackage` is also the binary's name: the script runs `cargo build --bin <cargoPackage>`. | `no bin target named …` |
| `Cargo.lock` is there and up to date: the script passes `--locked`. | `cannot create the lock file … because --locked was passed` (or `cannot update …`) |
| The module imports `@celesta.studio/*`: the script keeps `react` and its subpaths, `@celesta.studio/*`, and Node's built-in modules as imports, and bundles the rest. | `Could not resolve "@celesta/react"` |

A sample moved out also loses what its `Cargo.toml` inherits from Celesta's
workspace (`edition.workspace = true`, `celesta-plugin.workspace = true`):
each such line is written out, or Cargo stops with ``error inheriting
`edition` from workspace root manifest's `workspace.package.edition` ``.

## Docs changed

- `examples/plugins/README.md`, "Moving a sample out of this repository":
  the crates come from crates.io, the patch table is optional, and the
  three expectations above; the range example is 0.10's.
- `docs/handoff/2026-11-01-plugin-system.md`: the JavaScript side
  (`<PluginLayer>`, `@celesta.studio/*`), building a plugin (crates.io),
  what of "Publishing an SDK" is done, the manifest's range example, and a
  pointer to the note that replaced `build.mjs`.
- `packages/website/content/{en,ja}/docs/05-(more)/05-plugins.md`: a
  paragraph on a plugin in its own repository; the range, manifest, and ZIP
  name examples are 0.10's.
- `skills/celesta/references/plugins.md`: the same, in "Writing one".

## Not changed, noticed

- A plugin's own error reaches the user with its id twice. `<Flag>` with a
  file that is not an image:

  ```text
  could not draw plugin layer root.0: plugin "flag": plugin "flag": could not read …/bad.png: Format error decoding Png: Invalid PNG signature.
  ```

  `protocol::serve_on` sends a `PluginError::Failed`'s `Display`, which
  already starts with the id, and `PluginHost` wraps that message in another
  `Failed`.
- The user guide's media plugins section still says to install
  `celesta-codec-ffmpeg` from the repository "until it is published to
  crates.io"; `cargo search` lists it at 0.10.0.

## Checks

- A folder holding `plugin.json` (`{ "id": "oldname", "version": "0.1.0",
  "module": "js/index.ts" }`) and a `js/index.ts` that imports
  `@celesta/react`, built with `node scripts/plugins/build.mts <folder>`:
  `Could not resolve "@celesta/react"`.
- A package outside the workspace whose `[[bin]]` has another name than its
  `cargoPackage`, built the same way: `no bin target named …`. With the
  names equal, and `celesta-plugin = "0.10"` from crates.io with no patch
  table, the script writes the plugin directory, its manifest carrying
  `">=0.10.0, <0.11.0"`.
- `examples/plugins/flag` copied out of the repository with a `[workspace]`
  table added: Cargo stops on the inherited `edition`. With the
  `Cargo.toml` the samples' README gives and `cargo generate-lockfile`, the
  script builds it from crates.io; without the lock file it stops on
  `--locked`.
- `pnpm --filter @celesta.studio/website test`.
