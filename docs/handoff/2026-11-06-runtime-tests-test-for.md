# VOICEVOX and AivisSpeech runtime tests use `test.for` (2026-11-06)

The runtime tests in `packages/voicevox/test/runtime.test.ts` and
`packages/aivis-speech/test/runtime.test.ts` ran the source and packaged CLI
variants from a `for` loop over a boolean, with a ternary for the test name.
The matching Code test (`packages/code/test/code.test.mts`) is written with
`test.for` over named cases (mika-f/Celesta#725); these two now read the same.

## Specification

- Each file declares its test with
  `test.for([{ name: 'source', packaged: false }, { name: 'packaged', packaged: true }])`
  and a `$name …` title. The test names are unchanged:
  `source CLI resolves … ` and `packaged CLI resolves …`.
- The test body is unchanged apart from losing one level of indentation.
  `git diff -w` shows only the declaration.
- `test.for` rather than `test.each`: it passes the test context as a second
  argument. These tests do not use the context, but the Code test does (for
  `signal`), and all three use the same form.
- The timeout stays in each package's `vitest.config.mjs`
  (`testTimeout: 60_000`).

## Checks

- `pnpm run build:runtime`
- `pnpm --filter @celesta.studio/voicevox test` and
  `pnpm --filter @celesta.studio/aivis-speech test`: both variants pass, and
  the reported names are the same as before.
