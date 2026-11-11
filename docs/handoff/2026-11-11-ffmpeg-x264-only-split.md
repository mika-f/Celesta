# FFmpeg: only x264 is a separate download

Continues [PR #810](https://github.com/mika-f/Celesta/pull/810) on
`codex/ffmpeg-codec-downloads`, from the brief this note replaces. It
supersedes the four-way split described in
[`2026-11-11-optional-codec-downloads.md`](2026-11-11-optional-codec-downloads.md),
[`2026-11-10-ffmpeg-build-variants.md`](2026-11-10-ffmpeg-build-variants.md),
[`2026-11-11-ffmpeg-review.md`](2026-11-11-ffmpeg-review.md), and
[`2026-11-11-ffmpeg-ci-native-cache.md`](2026-11-11-ffmpeg-ci-native-cache.md).

## Specification

Policy kept: app packages and the Linux image ship no GPL binary; libx264 is
an optional download on the same release. Method changed, as the owner
confirmed: only x264 is split out, and Windows takes its FFmpeg from pinned
NuGet packages again.

| Plugin | Components | Native FFmpeg build |
| --- | --- | --- |
| `ffmpeg` (bundled) | decoder, `prores`, `vp9`, `aac`, `opus`, `pcm`, `mp4`, `mov`, `webm` | LGPL: zlib, libopus, libvpx |
| `ffmpeg-x264` (download) | `libx264` | GPL: libx264 |

- libopus and libvpx are BSD licensed; FFmpeg built with them and without
  `--enable-gpl` stays LGPL, so splitting them served no licence purpose.
- Component ids `ffmpeg/opus` and `ffmpeg/vp9` are as on `main`. The only
  migration is `ffmpeg/libx264` to `ffmpeg-x264/libx264`, with no alias.
- WebM works with the bundled plugin alone. MP4 uses the bundled native H.264
  on macOS and Windows and needs the x264 download on Linux.
- One optional ZIP per system: `Celesta-plugin-ffmpeg-x264-<version>-<os>-<arch>.zip`.

## Implementation

### Plugins

- `plugins/ffmpeg-opus` and `plugins/ffmpeg-vpx` are deleted, with their
  workspace members, lockfile entries, and dev-dependencies.
- `plugins/ffmpeg/plugin.json` lists `vp9` and `opus` again, in `main`'s
  compact layout; the diff against `main` is only the libx264 entry and the
  licence list.
- `Profile` is `Standard` or `X264`. `Profile::of(kind, id)` is the one place
  that assigns a component to a program, and `FfmpegCodec::serves` replaces
  the repeated guards in `describe`, the decoder, the encoders, and the
  muxer. `FfmpegCodec::opus` and `FfmpegCodec::vpx` are gone.
- `--check-build` on the standard program opens every encoder it describes,
  now including `vp9` and `opus`, so a standard build without libvpx or
  libopus fails when it is staged (`--distribution`) or tested
  (`tests/manifest.rs`).
- The review-strengthened tests moved back into `plugins/ffmpeg/tests`:
  `alpha.rs` (four alpha levels in four quadrants and the WebM `alpha_mode`
  tag), `export.rs` (a rendered VP9 WebM keeps the transparent quadrant), and
  `opus.rs` (Opus encode, WebM mux, decode). They run in process against the
  standard build.
- `ffmpeg-x264`'s tests start the standard program (`support::process("ffmpeg")`)
  where they used the Opus or VPX programs; its in-process provider links the
  x264 build, which has neither encoder. The AVFoundation and Media
  Foundation tests encode Opus in process again, as on `main`, and register
  only the x264 program as an optional download.
- `plugin-host` recognizes `CODEC_PROGRAM` and the new `X264_CODEC_PROGRAM`
  beside an installed Celesta.

### Native builds and scripts

- `scripts/build-ffmpeg-linux.sh`: `standard` is
  `--disable-gpl --disable-nonfree --enable-zlib --enable-libopus --enable-libvpx`;
  `x264` is unchanged.
- `scripts/plugins/ffmpeg.mts` defines `FFMPEG_PACKAGES` once
  (`isFfmpegPackage`); `run-cargo.mts` and `build.mts` import it. The
  per-package environment switch remains for x264 only
  (`FFMPEG_X264_{PKG_CONFIG_PATH,VCPKG_ROOT,VCPKGRS_TRIPLET}`), with the
  `VCPKG_ROOT` fingerprint in rustc's flags for both packages.
- `.github/actions/setup-ffmpeg`: two Unix builds behind `actions/cache` (as
  before, without the vcpkg part of the key); `FFMPEG_FEATURES` is
  `standard[zlib,opus,vpx],x264` so Rust caches from the four-variant layout
  are not reused.

### Windows

- `ffmpeg-windows.yml` is a matrix of two packages:
  `Celesta.FFmpeg.Windows.Standard` (`ffmpeg[zlib,opus,vpx]`, LGPL) and
  `Celesta.FFmpeg.Windows.X264` (`ffmpeg[x264]`, GPL). `main`'s header
  comment is back, adapted. The vcpkg tag is given only there. Each package
  carries its source notice (`vcpkg/FFMPEG-SOURCE.md` or
  `vcpkg/GPL-SOURCE-OFFER.md`), and the pack step refuses to publish without
  it, without each port's `share/<port>/copyright`, or (standard) with x264.
- `setup-ffmpeg` downloads both packages as `main` did (index lookup,
  SHA-256 check, `.vcpkg-root` marker, `installed/vcpkg/updates`), checks
  their notices and ports, and exports `VCPKG_ROOT`, `FFMPEG_X264_VCPKG_ROOT`,
  `VCPKGRS_TRIPLET`, and `FFMPEG_VERSION`. The Windows build and cache steps
  are gone, so the open review thread on the Windows cache key no longer
  applies. `packages: read` is back in `ci.yml`, `package-build.yml`, and
  `package.yml`.
- **Pinned.** At the owner's request, `ffmpeg-windows.yml` ran on this
  branch (run 43048951525) and published `Celesta.FFmpeg.Windows.Standard`
  and `Celesta.FFmpeg.Windows.X264`, both `8.1.2-2`; their SHA-256s, which
  the Publish step also prints to the job log, are pinned in `setup-ffmpeg`.
  Running it needed `actions/checkout` added to the workflow's entry in
  `.github/workflows/actions.lock`.

### Packaging and notices

- `scripts/package-media-plugins.py`: `VARIANTS` is x264 only; its Linux and
  Windows helpers have no Opus or VPX branches. The ZIP's `INSTALL.txt` says
  WebM needs nothing extra.
- macOS: the standard build links Homebrew's libopus and libvpx, which
  dylibbundler copies into the plugin's `Frameworks`;
  `package-macos.py` passes `STANDARD_NATIVE_FORMULAS = ("opus", "libvpx")` to
  `collect_licenses`, so the app's `licenses/homebrew.json` and
  `licenses/native/<formula>` name them.
- Windows: the app's `licenses/native` is the standard package's share tree,
  which now holds the opus and libvpx copyrights.
- Linux image — the point the brief left open. The standard build is a static
  FFmpeg, but `ffmpeg-sys-next` probes pkg-config with `statik(true)`, and the
  pkg-config crate links a library statically only when it is outside the
  system library directories. libopus and libvpx come from
  `/usr/lib/x86_64-linux-gnu`, so the program links `libopus.so.0` and
  `libvpx.so.9` dynamically (checked with `ldd` here), as `main` did. The image
  keeps that: the build stage installs `libopus-dev libvpx-dev`, the runtime
  stage `libopus0 libvpx12` (Ubuntu 26.04's package names; the first manual run caught `libvpx9`, the 24.04 name, once the runners' apt mirrors recovered), and
  `packaging/linux/record-codec-libraries.sh` records each library's binary
  and source package with exact versions in
  `/opt/celesta/plugins/ffmpeg/licenses/native/debian-packages.json` (the
  x264 ZIP's format) beside their copyright files, failing if the program
  needs a library the image lacks. The image then runs the program's
  `--check-license` and `--check-build`.
- `FFMPEG-SOURCE.md` names libopus, libvpx, and zlib and where each system's
  notices are; `GPL-SOURCE-OFFER.md` points at the X264 package.
- `scripts/test-package.mjs` exports WebM as well as MOV with the bundled
  plugin; the manual CI Docker job does too. CI artifacts are
  `Celesta-plugin-ffmpeg-x264-*`.

### Exporter, editor, MCP, documentation

- The exporter's `ffmpeg/webm` hint and its test rows are gone; the MP4 and
  native H.264 hints stay. `--help` and MCP `export_video` say WebM needs no
  download. `initial_output`'s installed-encoder check and the discovery
  diagnostics are unchanged.
- en/ja guides (installation, create project, export, build from source,
  troubleshooting, plugins), the Celesta Skill, `HANDOFF.md`, `README.md`,
  and `packaging/**/README*` describe one optional download. The PR's earlier
  notes carry a superseded banner.

## Checks

Run in a Linux x86_64 container (Ubuntu 24.04, 4 cores, Rust 1.97 stable
rather than CI's 1.99.1, lavapipe for the GPU tests). ffmpeg.org and
git.ffmpeg.org were unreachable through the container's proxy, so both
prefixes were built by `scripts/build-ffmpeg-linux.sh` from Debian's
`ffmpeg_8.1.2.orig.tar.xz` (`FFMPEG_VERSION=8.1.2 FFMPEG_URL=…`); CI builds
8.1.3.

Passed:

- `node scripts/plugins/run-cargo.mts plugins test -p celesta-codec-ffmpeg
  -p celesta-codec-ffmpeg-x264 -p celesta-codec-avfoundation
  -p celesta-codec-mediafoundation -p celesta-plugin-vrm --locked
  --no-fail-fast` with `CELESTA_REQUIRE_GPU=1`: every FFmpeg suite, including
  the moved `alpha.rs`, `export.rs` (rendered VP9 on lavapipe, not skipped),
  `opus.rs`, `manifest.rs` (`--check-build` opens VP9 and Opus), and the x264
  crate's encoders (23), export (16), process (8), manifest (2). The native
  plugins compile to their stubs here. One VRM test,
  `a_read_only_cache_still_answers_what_it_holds`, fails only because the
  container runs as root, which ignores the read-only mode it sets; VRM is
  untouched by this work.
- The same runner's Clippy with `--all-targets -- -D warnings`.
- Root: `cargo clippy --workspace --all-targets --locked -- -D warnings`;
  `cargo test` for `celesta-plugin-host`, `celesta-exporter`, `celesta-mcp`,
  `celesta-editor`, `celesta-react-bridge`, and `celesta-codec-test` (1500
  passed). The whole `cargo test --workspace` did not fit the container's
  disk allowance with the plugin builds beside it.
- `cargo fmt --check` for both workspaces, `git diff --check`.
- `pnpm --dir scripts/plugins test` (242 passed, 2 skipped).
- Website: Vitest (52 passed, tab-count contracts included) and
  `makit check` (no issues; warnings are the existing OGP/Japanese-anchor ones).
- `python3 -B -m unittest scripts/test_package_macos.py scripts/test_package_media_plugins.py` (16 passed).
- Probes: the standard program reports LGPL (`--check-license`) and opens
  every encoder (`--check-build`); `ldd` shows `libopus.so.0` and
  `libvpx.so.9` linked dynamically, and
  `packaging/linux/record-codec-libraries.sh` wrote a valid
  `debian-packages.json` for them. An x264 program built against the standard
  prefix fails `--manifest` and `--check-build` with "this FFmpeg build has no
  libx264"; the real x264 program passes both its checks.
- Workflow and action YAML parse; the shell scripts pass `sh -n`/`bash -n`.

Not run here:

- actionlint (installing it was not permitted in this session).
- The Docker image build, macOS and Windows packaging, and the codec ZIPs:
  no Docker daemon run, macOS, or Windows host here. They need PR CI and a
  manual CI run (`workflow_dispatch`), as the PR did before.
- Windows: nothing ran here; PR CI downloads the pinned packages.

## Review follow-up

From the brief's author, after the first push:

- **One `distribution` format.** `scripts/package-media-plugins.py` wrote the
  whole of `/etc/os-release` into `debian-packages.json`'s `distribution`.
  It now writes `ID VERSION_ID` (such as `ubuntu 26.04`) through
  `distribution_release`, as `packaging/linux/record-codec-libraries.sh`
  does, and packaging fails when either field is missing (the existing
  "incomplete provenance fails" rule); the script refuses the same with
  `${ID:?}`/`${VERSION_ID:?}`. No `VERSION_CODENAME` field: `ID VERSION_ID`
  names the release, and both writers agree. New tests:
  `test_distribution_is_the_os_release_id_and_version`,
  `test_a_distribution_without_id_or_version_fails`;
  `packaging/plugins/README.md` describes the field.
- **The Docker MP4 example** in the export guide (en and ja, "Export MP4 in
  a container") is now an executed check, not only a reading of the code.
  The dispatch-only `docker` job in `ci.yml` waits for the packages
  (`needs: package`, and runs even when another system's package failed),
  downloads the `celesta-codecs-linux-x64` artifact, which
  `package-build.yml`'s `linux-plugins` job now uploads on manual runs as
  well as releases, extracts the ZIP into `codecs/`, and runs the guide's
  command as written (`--user`, the `:ro` mount, `--plugin-dir /plugins`,
  `--react first-scene.tsx output.mp4`) against the built image. It asserts
  an `ftyp` MP4 with an `avc1` track, then repeats the command with
  `--overwrite --json` and asserts the report's
  `outputs[0].video.component` is `ffmpeg-x264/libx264` with an `avc1` codec.
  This covers what the code reading could not: the ZIP built on the runner
  starting in `ubuntu:26.04` (glibc, the bundled libx264 and its RPATH), the
  image's `ffmpeg` and the mounted `ffmpeg-x264` working together for a real
  MP4 with Docker's default `/dev/shm`, and the command line itself.
  **Until a manual CI run has passed this step on the branch, the example is
  unverified.** What reading the code did establish: `--plugin-dir` folders
  are `PluginSource::Explicit`, which needs no approval
  (`needs_allowance()`), is not permission-checked (`is_checked()`), and may
  hold plugins in subfolders, so `codecs/ffmpeg-x264/` mounted at `/plugins`
  is found.

Checks: `python3 -B -m unittest scripts/test_package_macos.py
scripts/test_package_media_plugins.py` (18 passed); the record script run
against a libavcodec linking libopus and libvpx wrote
`"distribution": "ubuntu 24.04"` and valid JSON; the changed workflows and
`actions.lock` parse as YAML. The guides did not change in this round, so the
website checks were not rerun.
