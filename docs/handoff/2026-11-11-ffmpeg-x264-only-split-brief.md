# Work brief: reduce PR #810 to an x264-only split

This is a brief for a new session that continues
[PR #810](https://github.com/mika-f/Celesta/pull/810)
(`codex/ffmpeg-codec-downloads`, head `381d2223` when this was written). It
is not a record of finished work. When the work is done, write the usual
task note (`docs/handoff/<date>-<task>.md`) and delete this file in the same
pull request.

## Goal

Keep PR #810's policy and change its method:

- **Policy (keep):** app packages and the Linux image ship no GPL binary.
  libx264 is an optional download, `ffmpeg-x264`, on the same release.
- **Method (change):** only x264 is split out. Opus and VP9 go back into the
  standard `ffmpeg` plugin. Windows gets its FFmpeg from a pinned NuGet
  package again, not from a build inside CI.

The owner (mika-f) confirmed both changes. The four-way split was not
intended: the intent was only to separate x264.

## Where PR #810 stands

- Nine commits by Codex, 110 files, +5110/-1380. PR CI is green on all three
  systems. A manual run (`43023447375`) also passed: unsigned macOS and
  Windows apps, nine codec ZIPs, and a real MOV export from the Docker image.
- Two review rounds by hinaloe (with Claude Code) are addressed. One thread
  is open: the Windows cache key includes `ImageVersion`. This brief removes
  what that thread is about.
- Read these notes on the PR branch first: `2026-11-10-ffmpeg-build-variants.md`,
  `2026-11-10-plugin-license-placement.md`,
  `2026-11-11-optional-codec-downloads.md`, `2026-11-11-ffmpeg-review.md`,
  and `2026-11-11-ffmpeg-ci-native-cache.md` (all in `docs/handoff/`).

What the PR does today:

| Plugin | Components | Native FFmpeg build |
| --- | --- | --- |
| `ffmpeg` (bundled) | decoder, `prores`, `aac`, `pcm`, `mp4`, `mov`, `webm` | LGPL, zlib only |
| `ffmpeg-x264` (download) | `libx264` | GPL, libx264 |
| `ffmpeg-opus` (download) | `opus` | LGPL, libopus |
| `ffmpeg-vpx` (download) | `vp9` | LGPL, libvpx |

Each optional program is the shared `FfmpegCodec` with a `Profile`, linked
against its own FFmpeg prefix. `scripts/plugins/ffmpeg.mts`
(`ffmpegEnvironment`) picks the prefix for each Cargo package, and
`scripts/plugins/run-cargo.mts` runs Cargo one package at a time, because one
Cargo invocation links `ffmpeg-sys-next` once.

## Why the method changes

- libopus and libvpx are BSD licensed. An FFmpeg built with them and without
  `--enable-gpl` is still LGPL, so splitting them serves no licence purpose.
- The split costs two extra FFmpeg builds on every system, two crates, two
  more ZIPs for each system, and two downloads before WebM works.
- On `main`, Windows downloads the `Celesta.FFmpeg.Windows` NuGet package
  (pinned version and SHA-256, about 10 seconds). The PR replaced that with
  four vcpkg builds inside CI behind `actions/cache`. A cold build took
  58 minutes in review, the key changes with every runner image, and the
  save step also runs on pull request branches. `main`'s comment in
  `ffmpeg-windows.yml` explains why the package exists; the PR deleted it.
- The PR rewrote `ffmpeg-windows.yml` to publish four packages that nothing
  downloads.

## Target

| Plugin | Components | Native FFmpeg build |
| --- | --- | --- |
| `ffmpeg` (bundled) | decoder, `prores`, `vp9`, `aac`, `opus`, `pcm`, `mp4`, `mov`, `webm` | LGPL: zlib, libopus, libvpx |
| `ffmpeg-x264` (download) | `libx264` | GPL: libx264 |

- Component ids: `ffmpeg/opus` and `ffmpeg/vp9` stay as on `main`. The only
  migration is `ffmpeg/libx264` to `ffmpeg-x264/libx264`, with no alias (the
  owner's decision, unchanged).
- WebM works with the bundled plugin alone. MP4 uses the bundled native
  H.264 on macOS and Windows and needs the x264 download on Linux.
- One optional ZIP for each system:
  `Celesta-plugin-ffmpeg-x264-<version>-<os>-<arch>.zip`.

## Tasks

### 1. Fold Opus and VP9 back into `plugins/ffmpeg`

- Delete `plugins/ffmpeg-opus` and `plugins/ffmpeg-vpx`, and remove them from
  `plugins/Cargo.toml`, `plugins/Cargo.lock`, and the dev-dependencies of
  `ffmpeg-x264`, `avfoundation`, and `mediafoundation`.
- Restore `vp9` and `opus` in `plugins/ffmpeg/plugin.json`. The PR also
  reformatted that file; restore `main`'s compact layout so the diff shows
  only the real change.
- Reduce `Profile` in `plugins/ffmpeg/src/lib.rs` to standard and x264, and
  drop `FfmpegCodec::opus` and `FfmpegCodec::vpx`. Consider one place that
  maps a component to its profile instead of the repeated guards.
- Move the tests back into `plugins/ffmpeg/tests`, keeping the versions the
  review strengthened, not `main`'s older ones: the rendered VP9 quadrant
  export and decode, the four alpha levels, the WebM `alpha_mode` tag, and
  the Opus encode, mux, and decode round trip.
- `--check-build` on the standard program must now open `vp9` and `opus`
  too, so a standard build without them fails when it is staged.

### 2. Native builds: two variants

- `scripts/build-ffmpeg-linux.sh`: `standard` is
  `--disable-gpl --disable-nonfree --enable-zlib --enable-libopus --enable-libvpx`;
  `x264` is unchanged. Remove `opus` and `vpx`.
- `scripts/plugins/ffmpeg.mts`, `run-cargo.mts`, `build.mts`, and
  `version.mts`: remove the Opus and VPX names. The per-package environment
  switch stays, for x264 only. Define the list of FFmpeg packages once and
  import it, instead of repeating it in each script.
- `.github/actions/setup-ffmpeg/action.yml`: two Unix builds. Update
  `FFMPEG_FEATURES` so the Rust caches do not reuse four-variant output.
- `crates/plugin-host/src/media.rs`: the sibling programs are
  `celesta-codec-ffmpeg` and `celesta-codec-ffmpeg-x264`.

### 3. Windows: back to the pinned NuGet package

- `.github/workflows/ffmpeg-windows.yml`: a matrix of two packages,
  `Celesta.FFmpeg.Windows.Standard` (`ffmpeg[zlib,opus,vpx]`) and
  `Celesta.FFmpeg.Windows.X264` (`ffmpeg[x264]`). Restore `main`'s header
  comment about why the package exists.
- `setup-ffmpeg`: restore `main`'s download step (index lookup, SHA-256
  check, `.vcpkg-root` marker, `installed/vcpkg/updates`), once for each
  package, and export `VCPKG_ROOT` and `FFMPEG_X264_VCPKG_ROOT`. Remove the
  Windows build and cache steps. Restore `packages: read` in `ci.yml` and
  `package-build.yml`.
- Keep the vcpkg tag in one place.
- Put `packaging/FFMPEG-SOURCE.md` in the standard package and
  `packaging/GPL-SOURCE-OFFER.md` in the x264 package. Today the package
  holds vcpkg's `installed/` tree with `share/*/copyright` only, so it
  redistributes binaries with no source notice.
- **Needs the owner:** CI cannot pass on Windows until both packages are
  published and their versions and hashes are pinned in `setup-ffmpeg`. Only
  the owner can run `ffmpeg-windows.yml` (it publishes to GitHub Packages).
  Prepare the workflow, ask the owner to run it, then pin what its job
  summary reports. Do not publish anything yourself.

### 4. Packaging and licence notices

- `scripts/package-media-plugins.py`: `VARIANTS` is x264 only. Remove the
  Opus and VPX branches in the Linux and Windows helpers, and update
  `scripts/test_package_media_plugins.py`.
- The standard plugin now links libopus and libvpx, so its notices must name
  them on every system: `packaging/FFMPEG-SOURCE.md`, macOS
  `licenses/homebrew.json` and the bundled dylibs (`scripts/package-macos.py`),
  the Windows `share` tree (it comes with the package), and Linux.
- Linux image (`packaging/linux/Dockerfile`): the PR installs no Opus or VPX
  runtime packages. Check how the standard build links them (static archives
  from `libopus-dev` and `libvpx-dev`, or shared objects) and either keep it
  static or install the runtime packages and record their provenance as the
  x264 ZIP does (`debian-packages.json`). This was not checked.
- `scripts/test-package.mjs`, `package.yml`, and `package-build.yml`: one
  codec ZIP for each system. The smoke test can export WebM with the bundled
  plugin again; MOV stays the portable default.

### 5. Exporter, editor, and MCP

- `crates/exporter/src/report.rs`: remove the `ffmpeg/webm` hint and its
  test rows. Keep the MP4 and native H.264 hints.
- `crates/exporter/src/main.rs` (`--help`) and `crates/mcp/src/args.rs`:
  WebM no longer needs downloads.
- Keep `initial_output`'s check for installed default encoders
  (`crates/editor/src/export_dialog/model.rs`); Linux without x264 still
  needs MOV first.
- Keep the discovery diagnostics in `plugin-host` (first stderr line).

### 6. Documentation

AGENTS.md requires these to move together:

- `packages/website/content/{en,ja}/`: installation, export, build from
  source, troubleshooting, plugins.
- `skills/celesta/` (`SKILL.md`, `references/export.md`, `plugins.md`,
  `errors.md`, `setup.md`, `performance.md`, `verify.md`).
- `HANDOFF.md` (setup, workspace map, validation commands), `README.md`,
  `packaging/**/README*`, `packaging/GPL-SOURCE-OFFER.md`.
- Mark the PR's earlier notes as superseded where they describe four
  plugins, and write this task's own note.
- Update the PR title and body: they still describe three downloads.

To find what is left, search the tree (outside `docs/handoff`) for
`ffmpeg-opus`, `ffmpeg-vpx`, `FFMPEG_OPUS`, `FFMPEG_VPX`,
`codec-ffmpeg-opus`, and `codec-ffmpeg-vpx`. About 65 files match on the PR
head.

## Out of scope

- Linking libx264 directly, without FFmpeg, in `ffmpeg-x264`. It would remove
  the second FFmpeg build and the environment switch, and the Media
  Foundation plugin shows the shape (its own YUV conversion and `avcC`,
  writing into `ffmpeg/mp4`). It is a new encoder implementation, so it is a
  separate pull request if the owner wants it.
- Compatibility aliases for `ffmpeg/libx264`.
- Any rendering change. No screenshots are needed.

## Rules for the work

- Work on `codex/ffmpeg-codec-downloads` (or the branch the owner names) and
  add commits. Never amend. To update from `main`, rebase.
- Do not run `ffmpeg-windows.yml`, publish a release, or sign anything.
- Vitest and TypeScript for script tests; Rust tests stay in Rust.

## Checks

Run what `HANDOFF.md` and the PR's notes list, adjusted to two variants:

- Per-package plugin tests and Clippy through `run-cargo.mts`
  (`celesta-codec-ffmpeg`, `celesta-codec-ffmpeg-x264`, the system's native
  plugin, `celesta-plugin-vrm`), with `CELESTA_REQUIRE_GPU=1` where a GPU is
  present.
- Root: `cargo test --workspace`, Clippy with `-D warnings`, `cargo fmt`,
  `git diff --check`.
- `pnpm --dir scripts/plugins test`, the website's tests and content check,
  and `python3 -B -m unittest scripts/test_package_macos.py scripts/test_package_media_plugins.py`.
- Probes worth repeating: a standard program reports LGPL with libopus and
  libvpx linked (`--check-license`), and opens `vp9` and `opus`
  (`--check-build`); an x264 program built against the standard prefix fails
  discovery.
- After the push: PR CI on three systems, then a manual CI run for the
  packaging jobs and the Docker image, as the PR did before. Windows waits
  for the owner's package run (task 3).

Report what ran and what did not, with the reason, in the task note.
