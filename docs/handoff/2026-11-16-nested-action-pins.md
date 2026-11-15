# actions.lock: the actions that locked actions use (2026-11-16)

The `v0.10.0-alpha.2` tag's Plugin release run built every target, then
stopped `Publish to the registry` at "Set up job" with
`lockfile missing pin for actions/attest@508db95dd578ae2727ebd6217d5ba78e4fbda05d`,
so no plugin was published. `actions/attest-build-provenance@v4.2.2` is a
composite action whose step is
`uses: actions/attest@508db95dd578ae2727ebd6217d5ba78e4fbda05d # v4.2.1`, and
GitHub checks those against the lock too. The `actions.lock` check from
#843 (`2026-11-15-actions-lock-check.md`) follows only this repository's
composite actions, so it passed.

## Specification

- `gh actions-lock` records an action's own actions under its dependencies
  entry, not in the workflow's list: the action gets a `uses:` list of
  their pins, and each pin a dependencies entry of its own
  (`github/gh-actions-lock`'s `.github/workflows/actions.lock` does this
  for the same two actions). `actions.lock` now has
  `actions/attest-build-provenance@v4.2.2`'s `uses:` with
  `actions/attest@508db95…`, and that pin's entry (`ref: 'v4.2.1'`, owner
  44036562, repository 760701061, from the GitHub API), written by hand in
  that layout, as `gh actions-lock` was not installed.
- `parseActionsLock` reads each dependency's `commit` and `uses:` list.
  `checkActionsLock` also reports a pin under a dependency's `uses:` with
  no dependencies entry, and collects the remote `uses:` values the
  workflows reach (subfolders kept: `actions/cache/restore@v6.1.0`).
- `checkNestedActions` reads each of those actions' `action.yml` (or
  `action.yaml`) from `raw.githubusercontent.com` at its locked commit,
  which public repositories need no token for, and reports each action it
  uses that the lock does not list under the using action's `uses:`. It
  follows those actions in turn. A remote reusable workflow (a `.yml` path)
  is not followed, as `gh actions-lock` does not lock its actions; a pin
  without a commit is left to the other check; a locked action with no
  `action.yml` at its commit is a problem.
- `node scripts/check-actions-lock.mts` runs both; `--offline` skips the
  network part. CI's `Actions lock` job runs it without `--offline`; the
  script's tests pass `--offline`.
- Of the ten locked actions, only `actions/attest-build-provenance` uses
  another action at its locked commit.

## 0.10.0-alpha.2's plugins

A rerun of the tag's Plugin release run uses the tag's lock, and
`plugin-release.yml` has no `workflow_dispatch`, so, as with alpha.1
(`2026-11-15-alpha-2-plugin-release-lock.md`), the plugins need a new tag
once this is merged.

## Checks

- `node scripts/check-actions-lock.mts`: exit 0, 7 workflows. With the
  `v0.10.0-alpha.2` lock: exit 1, `actions/attest-build-provenance@v4.2.2
  uses actions/attest@508db95dd578ae2727ebd6217d5ba78e4fbda05d (in
  actions/attest-build-provenance@4d10147), which
  .github/workflows/actions.lock does not list under its uses`. With
  `--offline`, the old lock passes, as #843's check did.
- `scripts/plugins/test/actions-lock.test.mts` covers the parsing, the
  dependency without an entry, the collected actions, and
  `checkNestedActions` with a fake GitHub: an unlisted nested action,
  following two levels with a subfolder and case-insensitive repositories
  read once, a skipped reusable workflow and unlocked pin, and a missing
  `action.yml`. `pnpm test` in `scripts/plugins`: 272 tests pass.
- Not run: the Plugin release workflow, which needs a release tag.
