# TypeScript packages: CaptionTrack padding and small duplicates (2026-11-06)

Leftovers in the TypeScript packages from mika-f/Celesta#710: one behaviour
change (`<CaptionTrack>` padding is checked), two small fixes, and the
duplicates the issue listed. Each landed as its own commit.

## Specification

### `<CaptionTrack>` padding is checked

- `padding` and `highlight.padding` go through `validators('CaptionTrack').insets`
  (`packages/components/src/validate.ts`) instead of the unchecked `edges()`,
  which is gone. Every edge must be a finite number of 0 or more.
- `insets(padding, name?)` takes the prop's name for its message, `padding`
  unless given: `<CaptionTrack> highlight.padding top must be a finite number
  of 0 or more, not -20`. Before, `highlight={{ padding: -20 }}` or `NaN`
  silently drew no band, and a `NaN` track padding failed inside `fitText`
  under the wrong name.
- Track padding is checked with or without a band (`background={null}`) and
  with `render`; it used to be checked only when it reached `<Panel>`.
- `skills/celesta/references/errors.md` lists the message. The user guide
  gives the padding defaults but no error messages, so it is unchanged.
- `docs/handoff/2026-10-28-component-validators.md` said `edges()` spread
  `margin`; it spread `padding`, and `margin` goes through
  `safeAreaInsets()`, which checks it. Corrected.

### Small fixes

- `probeMedia()` (`packages/react/src/media.ts`) no longer caches a failed
  probe, as `decodeAudio()` already did not, so a `prepare()` that probes a
  path, creates the file, and probes it again sees the file. Both drop only
  the entry they set, so a probe or decoder installed since keeps its own.
- `ProtocolLines` moved from `packages/cli/src/cli.ts` to
  `packages/cli/src/channel.ts`. It reads 64 KiB chunks, searches each byte
  for the newline once, and joins a line's chunks once, when the newline
  arrives: linear in the line's length, where it was quadratic. It takes the
  file descriptor (stdin by default), so the tests read from a file.
- The `CompositionRuntimeContext` comment (`packages/react/src/hooks.ts`) said
  a component that only reads `useVideoConfig()` does not re-render per
  frame. Every reader does: `renderTree` provides a new value object on every
  request. The comment says so now; the behaviour is unchanged.

### Duplicates

- `<CaptionTrack>`'s highlight uses `Easings.easeOutCubic`; its own `easeOut`
  is gone.
- `requestQueue()` (`packages/cli/src/channel.ts`) serializes the media probe,
  the audio decoder, and the async text measurer on the channel; each used to
  repeat the queue. A failed request rejects to its caller and the queue goes
  on.
- `currentPackageName()` (`packages/react/src/package-scope.ts`, exported from
  `@celesta.studio/react/internal`) renames the legacy `@celesta/` scope for
  the CLI and the web editor, replacing `currentPackageName` in the CLI and
  `currentModuleName` in `packages/web/src/project-files.ts`. The web editor
  imports the module by path, as it imports the rest of the runtime.
- `useMeasurementFonts` maps its `extra` fonts with `prepareFonts`.
- `runtimeTree(values, child)` (`packages/react/src/render.ts`) nests the six
  runtime providers that `mount()` and `createResolver()` both render with;
  `mount` adds `CompositionSelectionContext` and `PropOverridesContext` inside
  it. A new runtime context goes in `runtimeTree`. `renderFrame` takes
  `RenderAtOptions` (the type `renderAt` already took, now named) plus
  `audioOnly`, instead of nine positional parameters (#569).
- `writeFileAtomic` / `writeFileAtomicSync`
  (`packages/character/src/atomic-file.ts`, exported from
  `@celesta.studio/character` as `@internal`) write through a temporary file
  renamed into place and remove it on any error. `ifExists: 'same-size'`
  treats a failed rename onto a file of the data's size as another process
  having published the same bytes, the voice snapshot's policy. VOICEVOX
  narration (`writeAtomically`) and the voice-folder snapshot use them. The
  sync form now also removes the temporary file when the write itself fails.

## Left alone

- `checkFontFeatures` in `packages/web/src/text-layout.ts` stays a copy of
  `@celesta.studio/react`'s. `text-layout.ts` is part of `@celesta.studio/web`'s
  declaration build (`packages/web/tsconfig.json`), whose `rootDir` is the
  package's `src/`, so it cannot import the react source the way the worker
  does. The issue rated it low priority since `web` mirrors the Rust side.
- `CompositionRuntimeContext` is still one context: moving `VideoConfig` into
  its own memoized context would stop `useVideoConfig()`-only readers from
  re-rendering per frame, but changes behaviour beyond the issue.

## Checks

```sh
pnpm run build:runtime
pnpm run test:runtime
pnpm --filter @celesta.studio/web test
```

`pnpm --filter @celesta.studio/components test` fails on the new padding
tests before the change (no error, or one naming `<Panel>` or `fitText`) and
passes after.

To time `ProtocolLines`, pipe a single line of 256 KiB, 1 MiB, and 4 MiB to a
script that reads one line with it (`new ProtocolLines().next()`), once with
the old class copied from `cli.ts` and once with `dist/channel.js`. On an
Apple Silicon Mac the old class took about 4, 14, and 200 ms; the new one 1,
2, and 3 ms.
