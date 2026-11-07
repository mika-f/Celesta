# Pin exact tool versions in mise.toml (2026-11-08)

## Specification and implementation

`mise.toml` asked for `node = "26"`, `pnpm = "12"`, and `rust = "1.99"`.
`mise.lock` records the exact version each range resolves to. Renovate's
weekly lockfile maintenance (`:maintainLockFilesWeekly`) re-resolves those
ranges, so it can move a tool in `mise.lock` without touching the other
places that pin the same tool:

- **pnpm**: the maintenance of 2026-11-03 (a89fb64f) moved `mise.lock` to
  12.17.0, and #768 would move it to 12.18.0. The `packageManager` fields
  in `package.json` and `packages/logos/package.json` stayed at 12.16.1.
  pnpm switches to the `packageManager` version inside the repository: here,
  `pnpm --version` printed 10.28.0 outside the repository and 12.16.1 inside
  it. So the pnpm that mise installed from `mise.lock` was never the one
  that ran, and the `pnpm` package rule ("mise.lock and packageManager pin
  the same pnpm") did not hold.
- **Node.js and Rust**: the same thing could happen to them. Node.js is also
  pinned in the packaging scripts and the Windows cache key (the `Node.js`
  group), and Rust in the workflows and the Linux Dockerfile (the `Rust
  toolchain` group). They agree today only because 26.13.0 and 1.99.1 are
  the latest releases.

`mise.toml` now pins `node = "26.13.0"`, `pnpm = "12.16.1"` (the version
that already runs), and `rust = "1.99.1"`. `mise lock` (mise 2026.11.4,
as in CI) rewrote `mise.lock`. The specifiers are now exact, and pnpm went
back from 12.17.0 to 12.16.1; Node.js and Rust are unchanged. Lockfile
maintenance can no longer move these tools. Renovate updates `mise.toml`
in the same group as the other pins, and the lockfile follows from there.

The `pnpm` package rule's description now names `mise.toml` as well.

## Checks

- `mise lock` with mise 2026.11.4 checked all 28 targets. The pnpm 12.16.1
  checksums and URLs it wrote are identical to the ones Renovate wrote for
  12.16.1 in 26476fe6.
- Ran Renovate 44.220.0 locally (`--platform=local --dry-run=lookup`, on
  Node.js 26.13.0) on this tree:
  - pnpm in `mise.toml`, `package.json`, and `packages/logos/package.json`
    all have the npm datasource and one proposed update, 12.16.1 → 12.17.0,
    on the same branch, `renovate/pnpm`. 12.18.0 is held back by the 3-day
    `minimumReleaseAge`.
  - Node.js in `mise.toml` and the three regex-managed pins all have the
    `node-version` datasource. Rust in `mise.toml` and the workflows all
    have the `rust-version` datasource. Neither has an update today.
- `renovate-config-validator` validated `renovate.json`.
- `git diff --check` passed.

## Left as they are

- `.github/workflows/package.yml` installs Rust with `toolchain: stable`.
- `packaging/linux/Dockerfile` copies from `node:26-bookworm-slim`, a
  floating tag.

Both were already loose before this change.
