# `@celesta/math` (2026-10-02)

`random` and `noise` moved out of `@celesta/react` into the new
`packages/math` (`@celesta/math`), which also adds more random, noise, scalar,
wave, angle, and point helpers. `@celesta/react` no longer exports `random` or
`noise`; entries import them from `@celesta/math`. Their output is unchanged
(pinned by `packages/math/test`).

- `@celesta/react`'s `build` is `tsc -b`: its tsconfig references
  `../math`, so one `pnpm run build` in `packages/react` builds both. Both keep
  their `.tsbuildinfo` in `dist/`, so `pnpm run clean` forces a full rebuild.
- `cli.ts`'s `shared-runtime` esbuild plugin resolves `@celesta/math` from
  the runtime like `@celesta/react` (an entry's folder has no copy of it).
- `scripts/stage-react-runtime.mjs` copies `@celesta/math` into the packaged
  runtime; `stage-project-types.mjs` stages its declarations and maps it in
  the `.celesta/tsconfig.json` `paths`.
- The web editor worker (`packages/web/src/engine.worker.ts`) imports
  `packages/math/src` directly and serves it for `require('@celesta/math')`.
