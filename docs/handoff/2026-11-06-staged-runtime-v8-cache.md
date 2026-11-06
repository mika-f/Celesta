# The staged React runtime leaves out V8 compile caches (2026-11-06)

The packaged CLI tests in `packages/code`, `packages/voicevox` and
`packages/aivis-speech` each run `scripts/stage-react-runtime.mjs` into their
own temp folder, type-check an entry against the staged
`dist/project-types`, and run the staged CLI. On Windows runners they took
8.7–20.5 s each and rose and fell together. Linux and macOS take about 6 s.
The Code test once passed its 30 s timeout (35.3 s, run 41197252327).

Most of what the stage script copied was junk. ag-psd's npm tarball has no
`files` field and ships a `.v8-cache` directory: 3,498 files (24 MB) of V8
code caches for Node.js 22.13.1, 24.7.0 and 24.13.1 on x64. Nothing loads
them: ag-psd does not use `v8-compile-cache`, and a cache keyed to another
Node.js version or architecture is never valid for the bundled runtime.
Those were 3,498 of the about 4,650 files staged. ag-psd 34.1.0, the latest, still
ships them (`npm view ag-psd dist.fileCount` gives 3,673), so upgrading does
not help.

## What changed

- `scripts/stage-react-runtime.mjs` skips any `.v8-cache` directory inside a
  copied package. A staged runtime has about 1,150 files instead of 4,650,
  and takes 33 MB instead of 57 MB.
- The callers need no change and get the smaller runtime: the three packaged
  CLI tests, `scripts/package-windows.ps1`, `scripts/package-macos.py` and
  `packaging/linux/Dockerfile`. Every package and the Linux image shrink by
  the same ~3,500 files and 24 MB.
- The packaged Code test asserts that `node_modules/ag-psd/dist/index.js` is
  staged and `node_modules/ag-psd/.v8-cache` is not.
- The three packaged tests print `packaged <name> phases (ms):
  {"stage":…,"tsc":…,"cli":…}`, so a slow CI run shows which phase took the
  time. Vitest prints it under the default reporter (CI); an agent reporter
  hides logs from passing tests, so pass `--reporter=default` locally.

## Options weighed

- Stage once in CI and pass the path through an environment variable, with
  each test staging itself when it is unset. This is safe because the CLI
  writes only under `os.tmpdir()` and tsc runs with `noEmit`. It was left for
  later: it moves the copy into a setup step rather than removing it, and the
  phase timings above show first whether staging is still the slow part.
- Running the three packages one at a time in CI adds wall time and does
  not reduce the work. `pnpm -r` runs four packages at once, so these tests
  also overlap the others (`packages/shapes` took 20 s in the same run).
- The voicevox and aivis-speech tests give the stage step its own
  `spawnSync` timeout of 30 s, which the 60 s `testTimeout` does not cover. A
  smaller copy makes that timeout harder to hit.

## Checks

- Red, then green: `vitest run test/code.test.mts -t "packaged CLI"` in
  `packages/code` failed on `Runtime must not ship .v8-cache` before the
  filter and passed after it.
- `pnpm run test` in `packages/code`, `packages/voicevox` and
  `packages/aivis-speech`.
- Windows CI timings, before (six runs on `main`, 2026-11-04/05) and after:
  see the pull request.
