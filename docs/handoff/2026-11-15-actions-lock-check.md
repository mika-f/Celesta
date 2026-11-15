# actions.lock check (2026-11-15)

The `v0.10.0-alpha.1` tag's Plugin release run failed at "Set up job" on
every runner with `lockfile missing pin for actions/cache@v6.1.0`. The
composite `.github/actions/setup-ffmpeg` restores and saves FFmpeg with
`actions/cache/restore` and `actions/cache/save`, and `plugin-release.yml`'s
list in `.github/workflows/actions.lock`, written by hand in #837, left
`actions/cache@v6.1.0` out. GitHub checks a workflow's list only when the
workflow starts, so the gap in a tag-only workflow showed first on the
release tag. #841 added the pin; this adds a check that would have caught it.

## Specification

- `node scripts/check-actions-lock.mts [repository root]` reads every
  `.github/workflows/*.yml` and `*.yaml`, run by CI or not, and fails (exit 1)
  when a workflow uses an action that its list in `actions.lock` does not
  pin. It also fails when a listed pin has no `dependencies:` entry. Exit 2
  is a workflow it cannot read (an action without a ref, a local action
  without an `action.yml`). Under GitHub Actions each problem is an error
  annotation on the workflow (or on the lock).
- A workflow's actions are its own `uses:` and those of the local composite
  actions it uses (`uses: $/.github/actions/<name>` or
  `./.github/actions/<name>` → `<path>/action.yml` or `action.yaml`),
  followed through nested composite actions.
- An action in a repository's subfolder is pinned by its repository:
  `actions/cache/restore@v6.1.0` needs `actions/cache@v6.1.0`. Owners and
  repositories compare without case (`Swatinem/rust-cache` is the lock's
  `swatinem/rust-cache`); refs compare exactly.
- A local reusable workflow (`uses: ./.github/workflows/package-build.yml`)
  is not followed: it has its own list in the lock, as `package.yml`'s and
  `ci.yml`'s lists show (neither repeats `package-build.yml`'s
  `upload-artifact`). `docker://` images are not pinned.
- A pin listed but no longer used is not an error: GitHub accepts it, and
  `gh actions-lock` drops it on its next run.

## Implementation

- `scripts/plugins/actions-lock.mts` holds the logic: `usesValues` (the
  `uses:` keys of a workflow or action, line by line, quoted or not, with
  comments and `run:` text left out), `classifyUses`, `parseActionsLock`
  (the layout `gh actions-lock` writes), `workflowPins`, and
  `checkActionsLock`. No YAML library: the scripts package has no runtime
  dependencies, and the lock's and the workflows' `uses:` lines are regular.
- `scripts/check-actions-lock.mts` is the command, beside
  `report-skipped-tests.mts`; its tests' tsconfig type-checks it.
- `ci.yml`'s `Actions lock` job (ubuntu, checkout and mise's Node.js, about
  a minute) runs it on every pull request and push to `main`. It uses only
  actions `ci.yml`'s list already pins.
- The branch also carries #841's `actions.lock` line, so this pull request's
  check passes before #841 merges; a rebase onto `main` drops that commit
  once #841 is in.

## Checks

- `node scripts/check-actions-lock.mts` on `main` before #841: exit 1, one
  problem, `plugin-release.yml uses actions/cache@v6.1.0 (through
  .github/actions/setup-ffmpeg/action.yml)`. With #841's line: exit 0, 7
  workflows.
- A copy of `.github` with `actions/cache@v6.1.0` deleted from
  `plugin-release.yml`'s list: exit 1 with the same error annotation
  (`GITHUB_ACTIONS=true`). `scripts/plugins/test/actions-lock.test.mts`
  repeats both on a copy of the repository's `.github`, and covers nested
  composite actions, reusable workflows, a workflow missing from the lock,
  a pin without a dependency, case, quoting, and `\r\n` lock files.
- `pnpm --dir scripts/plugins test` (tsc and Vitest): 266 passed on macOS,
  with the mbx cargo shim on `PATH` for the tests that run Cargo.
- actionlint 1.7.12 reports nothing for the new job; it still flags the
  workflow's existing `background:`, `wait:` and `$/` syntax, which it does
  not know yet.
- Not run: the job itself on GitHub, until the pull request's CI.
