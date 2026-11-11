# Reload watcher test: no sleeps for the props.json deletion (2026-11-11)

## Problem

`source::tests::the_watcher_notices_props_json_deletion` slept 1.1 s three
times (3.3 s per run), commented "Outlast the filesystem's mtime
granularity".

The test was written for `watched_mtime`, which sampled only the newest
mtime among the sources, the `--props-file` and the entry's folder. There,
deleting `props.json` (older than the entry) could show only through the
folder's mtime moving past the entry's, so the steps had to be further
apart than the filesystem's mtime granularity. 57f01181 ("notice deleted
and restored modules in the reload watcher") replaced it with
`watched_stamp`, but kept the test's body.

## Change

`watched_stamp` now returns a `WatchStamp` listing every folder and source
file that `walk_sources` visits, with path, mtime and length. `walk_sources`
visits `.json` files, so `props.json` is in the sample, and deleting it
removes that entry: the two samples differ whatever the mtimes are. The
sleeps are removed and the comment now says what the test relies on. The
assertion is unchanged.

No other test in the module sleeps. The test keeps its
`celesta-watch-{pid}` folder: `tempfile` is not a dev-dependency of the
editor crate, and no other test in the process uses that name (see
`2026-11-11-editor-test-scratch-dirs.md`).

## Checks

- `cargo test -p celesta-editor --bin celesta-editor the_watcher_notices_props_json_deletion`,
  50 runs in a row, all green.
- `cargo test -p celesta-editor --bin celesta-editor source::tests`
- `cargo clippy -p celesta-editor --all-targets -- -D warnings`
- `cargo fmt --all -- --check`
