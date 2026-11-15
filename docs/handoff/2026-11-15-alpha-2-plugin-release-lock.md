# 0.10.0-alpha.2: the plugin release's missing action pin (2026-11-15)

## What happened

`v0.10.0-alpha.1` (on the merge of #840) was the first tag
`plugin-release.yml` ran on. Every build runner stopped at "Set up job" with
`lockfile missing pin for actions/cache@v6.1.0`, so nothing was built and the
publish job was skipped. The composite `.github/actions/setup-ffmpeg` restores
and saves its cache with `actions/cache/{restore,save}@v6.1.0`, and
`actions.lock` lists each workflow's actions, composites' included; the
`plugin-release.yml` list, written by hand in #837, left `actions/cache` out.

The Release run for the same tag was cancelled while it packaged, before it
created the GitHub release or published crates, so 0.10.0-alpha.1 has a tag
and nothing else. A re-run of a tag's run uses the tag's commit, and
`plugin-release.yml` has no `workflow_dispatch`, so the fix needs a new tag:
plugins built against alpha.2 give `>=0.10.0-alpha.2, <0.11.0`, which an
alpha.1 app would refuse anyway.

## Change

- `.github/workflows/actions.lock`: `actions/cache@v6.1.0` in the
  `plugin-release.yml` list (the dependency entry was already there, for the
  other workflows that use `setup-ffmpeg`).
- Version 0.10.0-alpha.2, by `bump-version.mts` and the `package.json` files.

## Checks

- Every action the plugin release reaches (`grep -rn "uses:"` over
  `plugin-release.yml` and `.github/actions/`) is now in its lock list.
- Nothing checks the lock before a tag runs a workflow that CI never runs; a
  check over every workflow and the composites it uses would have caught this.
