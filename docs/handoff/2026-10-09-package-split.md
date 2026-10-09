# Splitting `@celesta/react` into packages (2026-10-09)

`@celesta/react` held the core renderer, high-level components, character
dialogue, Node-only project loading, and the bridge CLI in one package. It is
now split the way Remotion is, with no re-exports for compatibility: a
composition imports each API from the package that provides it.

| Package | Contents (from the old `packages/react/src`) |
| --- | --- |
| `@celesta/react` | `components.ts` (minus characters), `hooks.ts`, `render.ts`, `reconciler.ts`, `animation.ts`, `keyframes.ts`, `series.ts`, `timing.ts`, `time.ts`, `text-measure.ts`, `registry.ts`, `properties.ts`, `media.ts` (probe only), generated types |
| `@celesta/shapes` | `shapes.ts` |
| `@celesta/layout` | `layout.ts`, `camera.ts` |
| `@celesta/transitions` | `transition.ts`, `transition-series.ts` |
| `@celesta/text` | `text-motion.ts`, `text-fit.ts` |
| `@celesta/debug` | `debug.ts` |
| `@celesta/media-utils` | `preloadMedia`, `mediaDurationInFrames` |
| `@celesta/character` | `Character`/`CharacterView`/`Dialogue` and their render code, `dialogue-series.ts`, `lipsync.ts`, `blink.ts`, `psd-preset.ts` |
| `@celesta/project` | `project-runtime.ts`, `project.ts`, project file types |
| `@celesta/cli` | `cli.ts`, `property-inputs.ts`, `bin/`, `stage-project-types.mjs`, the single-file `examples/` |

`@celesta/voicevox` now depends on `@celesta/character` instead of the core.

## Core runtime hooks

- `@celesta/react/internal` exports what the other packages and the CLI need
  from the core: `mount`/`createResolver`, the runtime contexts (including
  `ProjectLayersContext` and `ProjectTrackLayersContext`, which moved into
  `hooks.ts`), text measurement internals, the media probe, property input
  plumbing, `entryRelativePath`/`isRemoteUrl`, and the project document types
  ts-rs generates into this package. It is not for compositions.
- `host-elements.ts`: `registerHostElement(type, { prepare?, audio?, content? })`
  teaches the scene walker a host element. `@celesta/character/src/render.ts`
  registers `asset-character`, `character-view`, and `dialogue` when it loads;
  the walker still rejects unregistered types. `prepare` replaces the old
  `collectCharacterViewOverrides` pass (it runs only when an element defines
  it), and `HostWalk.frameState` replaces `WalkContext.characterViewOverrides`.
  The `<CharacterView>` content and `<Dialogue>` subtitle logic moved verbatim.
- `<FreezeFrame>` provides a per-freeze scope object through
  `FreezeFrameContext` (previously a boolean); `@celesta/character` keys its
  view scopes on it.
- `AssetReference.kind` is a plain string, and its character fields moved to
  `CharacterReference` in `@celesta/character`. `<Character>`'s ref stays typed
  as `AssetReference`, so existing `useRef<AssetReference>()` code compiles.
- State the CLI installs (text measurer, media probe) stays in the core, so a
  package never needs the CLI to reach its own copy.

## Resolution

- The CLI keeps `react` and every `@celesta/*` import (and
  `@celesta/react/internal`) external and resolves them from itself. It
  depends on all packages, so one module instance of each is shared by the
  CLI and the entry. Code and VOICEVOX are no longer bundled into entries.
  An unknown `@celesta/*` name fails the build with `… is not a Celesta
  package`, and a name imported from the wrong package fails with
  `@celesta/react does not export Circle; import it from @celesta/shapes`.
- The runtime staged for packaging is `@celesta/cli` and its dependency
  closure; it still lands in `Resources/react` / `runtime/react`, so the Rust
  runtime paths and `.celesta/` template location are unchanged. Source
  builds use `packages/cli/dist/cli.js`.
- `.celesta/tsconfig.json` maps every `@celesta/*` package and
  `@celesta/react/internal`, which the other packages' declarations import.
- The web worker serves `react`, `@celesta/react`, `math`, `shapes`, `layout`,
  `transitions`, `text`, `debug`, `media-utils`, `character`, and `code`
  (`runtimeModules` in `project-files.ts`). Vite and the worker test alias
  every `@celesta/*` import to its source so one core instance is bundled.

## Migrating compositions

`skills/celesta/scripts/migrate-packages.mjs` (Node only, no dependencies, so
it ships with the skill) rewrites `@celesta/react` named imports and
`export … from` re-exports by its `movedExports` table, joins an existing
import of the same kind from the destination package, adds the new packages
beside `@celesta/react` in a `package.json` that lists it, and reports
`import * as` namespaces that use moved names. `--dry-run` and `--check`
report without writing. `packages/cli/test/migrate-packages.test.mjs` checks
the table against every package's declarations, so a name added to a split
package without a table entry fails the test. Run on the pre-split examples,
it reproduces this branch's migration of them.

## Build and tests

- `pnpm run build:runtime` (root) builds `@celesta/cli` and its dependencies;
  `pnpm run test:runtime` runs their tests. Codegen is still
  `pnpm --dir packages/react run codegen`.
- Tests moved with their modules. Tests that cross packages (composition
  language, freeze frames with characters and project layers, motion
  helpers) and the CLI protocol tests live in `packages/cli/test`. Packages
  spawn the CLI through `../../cli/bin/celesta-react-render.js`.
- The 159 vitest cases from before the split all pass in their new packages,
  plus two CLI tests for the import errors and a web worker case that
  compiles a composition importing `@celesta/layout`, `@celesta/shapes`, and
  `@celesta/character`.
