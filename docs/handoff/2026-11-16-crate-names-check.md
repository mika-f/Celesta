# crates.io crate names check and faster publish (2026-11-16)

The `v0.10.0-alpha.2` tag's Release run failed in `Publish crates` after
uploading three crates: crates.io refused `celesta-registry-format` with
`403 Forbidden: Trusted Publishing tokens do not support creating new
crates`. The crate was added after the 2026-11-14 setup
([`2026-11-14-crates-io-trusted-publishing.md`](2026-11-14-crates-io-trusted-publishing.md)),
so it had neither the `0.0.0` placeholder nor a Trusted Publishing
configuration. That note says a new crate needs both, but nothing checked
it before a release. This adds a check that fails the pull request instead.

## Specification

- `scripts/unpublished-crates.sh --names [path/to/Cargo.toml]` prints the
  workspace's publishable packages (`publish = false` left out) whose name
  is not on crates.io at all, one per line. Without `--names` it lists the
  ones whose version is not there, as before; the release jobs use that.
  Any answer other than 200 or 404 is an error (exit 1) that names the
  crate and the status.
- `ci.yml`'s `Crate names` job (ubuntu, checkout and the Rust toolchain for
  `cargo metadata --no-deps`) runs it on the root and plugins workspaces on
  every pull request and push to `main`. Each missing crate is an error
  annotation on its workspace manifest that points to the 2026-11-14 note's
  one-time setup. The job uses only actions `ci.yml`'s list in
  `actions.lock` already pins.
- A pull request that adds a publishable crate fails this job until the
  crates.io owner publishes the placeholder. The Trusted Publishing
  configuration (`scripts/crates-trusted-publishing.sh`) cannot be checked
  without an owner's token, so the job does not cover it; the owner runs
  the script right after the placeholder.

## Faster publish job

On `v0.10.0-alpha.2`, `Publish crates` took about 9 minutes per attempt, 7.5
of them in `Verify packages` (`cargo package` building every crate, the
editor included, with no cache), and the second attempt then failed with
`timed out waiting for celesta-remote v0.10.0-alpha.2 to be available`,
leaving its seven dependents unpublished.

- `package.yml`'s new `verify-crates` job (`needs: validate`) installs the
  Linux libraries, bundles the crate files, and runs
  `cargo package --locked --allow-dirty` on every publishable crate of the
  root workspace (`cargo metadata` with `publish != []`, 19 crates). It runs
  beside the `package` jobs, whose macOS build took about 22 minutes, so it
  adds nothing to the release's time. `release` needs it, so a crate that
  does not build stops the run before the GitHub Release or crates.io.
- `crates` (`needs: [release]`) no longer installs the Linux libraries or
  verifies: it lists, bundles, authenticates, and publishes with
  `--no-verify`. A rerun of a failed publish repeats only this job.
- Cargo waits a minute for each published crate to reach the index and then
  gives up on its dependents. `publish.timeout` would raise that, but Cargo
  1.99.1 still gates it behind `-Z publish-timeout`. The `Publish` step
  instead runs up to four rounds: each lists what is still unpublished
  (`scripts/unpublished-crates.sh`), publishes it in the same two groups as
  before, and after a failure waits 60 seconds for the index before the
  next. It fails with an error annotation naming what is left after the
  fourth. A permanent error (a 403, say) costs about four minutes of
  retries before it fails.
- `plugin-crates` is unchanged: its verification resolves the core crates
  from crates.io, so it cannot run before `crates`.

## Recovering v0.10.0-alpha.2

- `celesta-registry-format 0.0.0` was published by hand on 2026-11-16 as a
  placeholder in the 2026-11-14 note's format, and the failed jobs were
  rerun. The release's `List unpublished crates` step skips the three crates
  the first attempt published. The second attempt published seven more and
  timed out on `celesta-remote`; a third attempt was started for the rest.

## Checks

- `scripts/unpublished-crates.sh --names` on both workspaces: nothing, exit
  0. Without `--names` on the root workspace during the rerun: the crates
  the release had not published yet.
- The job's step, run locally against a scratch workspace with a crate named
  `celesta-zz-not-a-real-crate-843` and a `publish = false` one: one error
  annotation for the first, exit 1. Against the repository's manifests: exit
  0.
- The script with an empty User-Agent (crates.io answers 403): the step
  stops with `crates.io answered 403 for celesta-composition`, exit 1.
- The `Publish` step's script, run locally with stub `cargo` and
  `scripts/unpublished-crates.sh`: one round when everything publishes; a
  second round with only the remaining crates after a simulated timeout;
  four rounds and exit 1 with the error annotation when every publish fails.
- `cargo metadata` with the `verify-crates` filter lists the 19 publishable
  root crates (`publish = false` ones left out).
- `bash -n scripts/unpublished-crates.sh`;
  `node scripts/check-actions-lock.mts` passes. `actionlint` reports nothing
  new for `ci.yml` or `package.yml` (only `background`, `wait`, and `$/`
  steps it does not know).
- Not run: the release jobs themselves, which need a release tag.
