# Publishing the crates with crates.io Trusted Publishing

A release now publishes the crates to crates.io from GitHub Actions, with no
API token stored in the repository. This builds on
[`2026-11-04-crates-io-publish.md`](2026-11-04-crates-io-publish.md), which
made the crates packageable and published them by hand.

## Specification

- `.github/workflows/package.yml` (Release) gains two jobs after
  `release` (the GitHub Release exists first):
  - `crates` publishes the root workspace's 18 publishable crates.
  - `plugin-crates` then publishes the plugins workspace's four media plugin
    crates: `celesta-codec-ffmpeg`, `celesta-codec-ffmpeg-x264`,
    `celesta-codec-avfoundation`, and `celesta-codec-mediafoundation`
    (`celesta-plugin-vrm` stays `publish = false`). Their pins on the core
    crates resolve from crates.io, so they need the `crates` job done.
- Both jobs run in the `crates-io` GitHub environment with
  `id-token: write`. `rust-lang/crates-io-auth-action@v1.0.6` exchanges the
  job's OIDC token for a crates.io token that lasts 30 minutes and is
  revoked when the job ends. crates.io matches the token's repository
  (case-insensitively), its top-level workflow file (`package.yml`; a
  reusable workflow it calls would not count), and its environment against
  each crate's Trusted Publishing configuration.
- Each job:
  1. lists the crates whose version is not on crates.io yet
     (`scripts/unpublished-crates.sh`, which asks the crates.io API for
     each publishable package of the workspace); a rerun publishes only
     what an earlier attempt left, and a job with nothing left skips the
     rest;
  2. verifies them with `cargo package --locked` (each crate built from its
     package against the others' packages, so nothing is uploaded unless all
     of them build);
  3. only then fetches the token, since verifying (the editor included)
     takes longer than the token lasts;
  4. publishes with `cargo publish --locked --no-verify`, in dependency
     order. In `crates`, `celesta-editor` and `celesta-mcp` go last with
     `--allow-dirty` for their `bundled/` copies
     (`scripts/bundle-crate-files.mjs`), so the other crates record a clean
     checkout, as in the 2026-11-04 note.
- `plugin-crates` sets up FFmpeg with `setup-ffmpeg`. Both FFmpeg plugins'
  verification builds link the standard build (`PKG_CONFIG_PATH`); the x264
  plugin needs libx264 only when its program runs. AVFoundation and Media
  Foundation build their stub programs on Linux.
- `.github/workflows/actions.lock` lists `rust-lang/crates-io-auth-action`
  (v1.0.6, `3268eb41`) and `actions/cache` (used by `setup-ffmpeg`) for
  `package.yml`. `gh actions-lock` was not available, so the entries were
  written by hand from the GitHub API, in the generated format.
- `scripts/crates-trusted-publishing.sh` creates the Trusted Publishing
  configuration (`mika-f/Celesta`, `package.yml`, environment `crates-io`)
  for every publishable crate of both workspaces through crates.io's API,
  skipping crates that already have it. `--trustpub-only` also sets each
  crate's "Trusted Publishing only" setting (`trustpub_only`), after which
  crates.io refuses new versions published with API tokens.

## One-time setup (crates.io owner)

crates.io lets a configuration be added only to a crate that exists, and a
Trusted Publishing token cannot create a crate. 18 crates were published as
`0.0.0` placeholders on 2026-11-05. The other four were published as `0.0.0`
placeholders on 2026-11-14, the same way: `celesta-media-image` (the
exporter depends on it), `celesta-codec-ffmpeg-x264`,
`celesta-codec-avfoundation`, and `celesta-codec-mediafoundation`. A
placeholder is a crate of its own, outside the repository, holding only a
`Cargo.toml` (an empty `[workspace]`; description "Placeholder for the
Celesta project. The real crate is not published yet.",
`MIT OR Apache-2.0`, the repository URL, `readme = "README.md"`), a
`README.md` saying it reserves the name, and a `src/lib.rs` with only a doc
comment. It is published with `cargo publish` and the owner's API token.
A crate added later needs a placeholder like this before its first release.

1. Create an API token with the `trusted-publishing` endpoint scope and run:

   ```bash
   CRATES_IO_TOKEN=... scripts/crates-trusted-publishing.sh
   ```

   Owners need a verified email address and a linked GitHub account.
   Revoke the token afterwards.
2. In the repository's Settings > Environments, protect `crates-io`
   (GitHub creates it on the first run otherwise, without protection):
   limit deployments to tags matching `v*.*.*`, plus the branch a manual
   Release run uses, and optionally require a reviewer.
3. After the first release has published everything, optionally run the
   script again with `--trustpub-only`.

## Checks

- `scripts/unpublished-crates.sh` lists the 18 root crates and the four
  plugin crates at 0.9.0; `celesta-budoux 0.0.0` answers 200 from the API,
  which the script treats as published.
- `cargo package --locked -p celesta-composition -p celesta-project`
  verifies a crate against a workspace dependency's package through Cargo's
  temporary registry, with `--locked`.
- `cargo package --manifest-path plugins/Cargo.toml -p <plugin> --list`
  for the four plugin crates (their full verification needs the core
  crates on crates.io).
- The publish step's argument splitting, run with `echo` in place of
  `cargo`, and with an empty list.
- `bash -n` on both scripts; `scripts/crates-trusted-publishing.sh` with an
  invalid token stops at the first crate with crates.io's error, and with
  an unknown option prints its usage.
- `actionlint .github/workflows/package.yml` reports only the
  `$/.github/actions/setup-ffmpeg` form, which it does not know and also
  reports for `package-build.yml`; `package.yml` and `actions.lock` parse as
  YAML.
- `scripts/crates-trusted-publishing.sh` ran on 2026-11-14 and configured
  all 22 crates; the API lists `mika-f/Celesta`, `package.yml`,
  `crates-io` for each one checked, and a second run reports every crate as
  already configured. `--trustpub-only` has not been set.
- Not run: the jobs themselves, which need a release.
