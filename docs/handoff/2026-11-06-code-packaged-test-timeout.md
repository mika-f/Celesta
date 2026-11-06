# Packaged Code CLI test gets the runtime tests' 60 s budget (2026-11-06)

`packaged CLI resolves Code without project dependencies and shares
useTypewriter's runtime` (`packages/code/test/code.test.mts`) intermittently
failed `Test (Windows x64)` with Vitest's "Test timed out in 30000ms", on
`main` and on unrelated PRs.

## Cause

The packaged variant runs `scripts/stage-react-runtime.mjs`, which copies the
whole runtime closure (every `@celesta.studio/*` package, React, esbuild and
their dependencies, dereferencing pnpm's symlinks), then runs `tsc` on a
project that uses the staged types, then starts the staged CLI. The source
variant only starts the CLI and takes about 0.5 s.

`test:runtime` (`pnpm --filter "@celesta.studio/cli..." run test`) runs the
package test scripts in parallel. The VOICEVOX and AivisSpeech packaged tests
do the same staging and `tsc` at the same moment, so the three compete for the
runner's disk and CPU. Copying many small files is slow on Windows, so the
time varies with load. Times from recent `Test (Windows x64)` jobs:

| Run | Code | AivisSpeech | VOICEVOX |
| --- | --- | --- | --- |
| 41197252327 (failed) | 35 258 ms | — | — |
| 42022585396 | 15 217 ms | 18 138 ms | 17 661 ms |
| 42045235547 | 15 920 ms | 14 740 ms | 14 442 ms |
| 42063457385 | 13 222 ms | 10 522 ms | 10 333 ms |
| 42013630584 | 10 001 ms | 8 713 ms | 8 891 ms |

The three move together, which points at runner contention rather than at
anything specific to Code. In run 42063457385 the Code test took 5.9 s on Linux and 6.7 s on macOS.

When Vitest moved these tests over from `node --test` (abb29557), VOICEVOX and
AivisSpeech got `testTimeout: 60_000` in their `vitest.config.mjs` for this
reason. Code kept an inline `{ timeout: 30000 }`.

## What changed

- The test is written with `test.for` over `{ name, packaged }` cases instead
  of a `for` loop with ternaries for its name and timeout. `test.for` passes
  each case together with the test context, which the test needs for
  `signal`; `test.each` does not.
- The timeout is 60 s, the same budget as the two sibling packages. `test.for`
  options apply to every case, so the source variant (about 0.5 s) gets 60 s
  too; the only cost is a hang there failing after 60 s instead of 30 s. The
  other tests in `packages/code` keep their timeouts. A comment above the test
  says why.

Vitest cannot interrupt the synchronous `spawnSync` calls anyway, so the failed
run's 35 s test finished its work and was then reported as timed out. With
60 s it would have passed.

## Follow-up (not done here)

Making the tests faster means staging the runtime once for all three packages,
or running them one after another in CI. That is a change across packages and
CI, not part of this fix.

## Checks

- `pnpm --filter @celesta.studio/code test` after `pnpm run build:runtime`:
  20 tests pass, both CLI variants included.
