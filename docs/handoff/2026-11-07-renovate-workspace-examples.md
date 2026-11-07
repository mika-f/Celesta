# Let Renovate update the workspace examples (2026-11-07)

## Specification and implementation

`pnpm-workspace.yaml` lists `examples/*` as workspace packages, and the root
`Cargo.toml` lists `examples/plugins/flag` and `examples/plugins/turntable` as
workspace members. They share `pnpm-lock.yaml` and `Cargo.lock` with
`packages/*` and `crates/*`. `renovate.json`'s `ignorePaths`, however, still
left out every top-level example except `examples/versus/bench`
(`examples/!(versus)/**`, `examples/versus/*`, `examples/versus/!(bench)/**`).

Renovate therefore raised a dependency in `packages/*` but not the same
dependency in an example, and both versions stayed in the shared lockfile:

- #627 moved `@types/node` to `^26.0.0` everywhere but `examples/versus`,
  which kept `^24.13.6` (lockfile: 24.19.2 and 26.6.6).
- #635 moved `ag-psd` to `^34.0.0` in `packages/cli` only;
  `examples/zunda` and `examples/shorts-explainer` kept `^31.0.2`
  (lockfile: 31.0.3 and 34.1.0).

The three `examples/` patterns are removed. Every example is now in scope:
the pnpm workspace packages, the two Cargo workspace plugins, and
`examples/versus/bench` as before. `*/**/examples/**` stays, so nested
`examples/` directories are still ignored as in `config:recommended`.
`@celesta.studio/*` dependencies stay disabled by the existing package rule.

Once this lands, Renovate adds the examples to the update branches that
already exist (for example `renovate/node-26.x` and `renovate/ag-psd-34.x`),
since one branch covers a dependency in every file.

## Checks

- `renovate-config-validator` (renovate 44.220.0) validated `renovate.json`.
- Ran Renovate 44.220.0's own ignore matching (`file.includes(pattern)` or
  `minimatch(pattern, { dot: true })`) over every tracked `package.json`,
  `package-lock.json`, `Cargo.toml`, `Cargo.lock`, and `pnpm-lock.yaml`,
  with the old and the new `ignorePaths`. The new config adds the 27
  `examples/*/package.json` files and the two `examples/plugins/*/Cargo.toml`
  files. It drops nothing, and no workspace package stays ignored.
- `git diff --check` passed.

No application code or lockfiles change in this task.
