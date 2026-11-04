# The `@celesta.studio` npm scope

The Celesta packages move from `@celesta/*` to `@celesta.studio/*` so they
can be published to npm under a scope Celesta owns. Compositions written
for the old names keep working: every place that resolves a Celesta import
accepts `@celesta/<name>` as `@celesta.studio/<name>`.

## Specification

- Every workspace package is renamed: `@celesta/react` is
  `@celesta.studio/react`, `@celesta/cli` is `@celesta.studio/cli`, and so on,
  the root included (`@celesta.studio/root`). Package folders do not move.
  The packages stay `private` (only `@celesta.studio/web` was not); publishing
  them is a separate change.
- New code is written with the new names: the examples, templates, guides,
  the Celesta Agent Skill, the MCP docs, and the imports the editor adds.
- The old names still resolve:
  - **CLI** (`packages/cli/src/cli.ts`): the `shared-runtime` esbuild plugin
    matches `@celesta/<name>` and `@celesta/<name>/internal` too, and
    `currentPackageName()` renames them before `require.resolve`, so an
    entry gets the same module instances as with the new names. An unknown
    package fails as before (`@celesta/nope is not a Celesta package`).
  - **Web** (`packages/web/src/project-files.ts`): `currentModuleName()`
    renames the scope; `projectFiles` keeps an old name external and the
    worker's `require` looks it up under its new name.
  - **Types** (`packages/cli/scripts/stage-project-types.mjs`): the staged
    `tsconfig.json` maps each old package name to the same
    `./node_modules/@celesta.studio/<name>` declarations. The staged hash
    changes, so opening a project refreshes its `.celesta/`.
  - **Source edits** (`crates/source-edit/src/imports.rs`): `Module::of`
    reads an import source of `@celesta/<name>` as `@celesta.studio/<name>`
    (`current_source`). A file importing from `@celesta/react` therefore
    counts as importing `PACKAGE`: its `defineScenes`, `Sequence`, and other
    tags are recognized, and names the editor adds join that existing import
    list, which keeps its old spelling. A new import line uses the new name.
- Each use of an old name is warned about, once per file and name:
  - The CLI records `{ file, specifier, current }` for each old-name import
    as it resolves, `file` relative to the entry's real directory (esbuild
    names importers by real path) with `/` separators, including imports
    from content plugins' modules. The ready message carries them as
    `legacyImports`, omitted when there are none.
  - `celesta-react-bridge` reads them into
    `ReactCompositionMetadata::legacy_imports` (`LegacyImport`, whose
    `Display` is the warning text: `scenes/Intro.tsx imports
    @celesta/react, an old name; import @celesta.studio/react instead`).
  - The editor's preview worker adds them to the preview warnings, so the
    Problems tab and the preview's warnings strip show them while the
    bridge lives; they are recomputed with every frame, not logged.
  - The exporter reports them as `ExportProgress::Warning` with the
    composition (video and stills), so they print as `warning: …` and land
    in the `--json` report's `warnings`.
  - The CLI writes nothing to stderr for them: the bridge passes stderr
    through, and the structured warnings already reach both hosts.
  - The web worker's `require` logs `console.warn("@celesta/react is an old
    name; import @celesta.studio/react instead.")` once per name and compile.
- `skills/celesta/scripts/migrate-packages.mjs` renames the scope as well as
  moving the names that left the core: every quoted `@celesta/…` specifier
  (static imports, re-exports, `import()`, subpaths) becomes
  `@celesta.studio/…` before the moved-name pass, and the `package.json` beside
  a changed file has its `@celesta/…` dependency keys renamed, keeping their
  ranges and order. `@celesta-plugin/<id>` is a different scope and is left
  alone.
- Historical notes in `docs/handoff/` and `docs/superpowers/` keep the names
  they were written with.

## Checks

```sh
pnpm install
pnpm run build:runtime
pnpm run test:runtime
pnpm --dir packages/web test
pnpm run typecheck:examples
cargo test -p celesta-source-edit -p celesta-react-bridge -p celesta-mcp -p celesta-editor -p celesta-exporter
```

- `packages/cli/test/misplaced-import.test.mts`: an entry importing
  `@celesta/react` and `@celesta/shapes` loads and lists both in
  `legacyImports`; an entry with only the current names sends none.
- `crates/react-bridge/src/tests.rs`: the ready message's `legacyImports`
  parse, default to empty, and format as the warning.
- `crates/react-bridge/tests/node_integration.rs`: a scene file's
  `@celesta/react` import reaches `metadata().legacy_imports` relative to
  the entry.
- `packages/web/test/project-files.test.mts`: an old name stays a runtime
  import and resolves under its new name.
- `packages/cli/test/migrate-packages.test.mts`: sources and manifests are
  renamed; `@celesta-plugin/*` is not.
- `crates/source-edit/tests/imports.rs` and `scenes.rs`: an `@celesta/react`
  import is extended in place, and its `defineScenes` makes a scene list.
- By hand: a temporary project with `.celesta/` copied from
  `packages/cli/dist/project-types/` type-checks `@celesta/react` and
  `@celesta/shapes` imports with `tsc -p`.
