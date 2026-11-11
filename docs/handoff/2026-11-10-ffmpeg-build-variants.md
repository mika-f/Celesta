# FFmpeg plugins by native build

> Distribution policy superseded by [optional codec downloads](2026-11-11-optional-codec-downloads.md): only standard FFmpeg is bundled. Startup license refusal is superseded by the explicit distribution checks in [the review follow-up](2026-11-11-ffmpeg-review.md).


## Specification

Split optional FFmpeg codecs by native build, rather than making the standard
media plugin GPL. The agreed plugins are `ffmpeg` (standard), `ffmpeg-x264`,
`ffmpeg-opus`, and `ffmpeg-vpx`. Each is a separately linked process with its
own native installation prefix and license notices. Celesta's main crates
continue to link no FFmpeg. No rendering algorithm or composition API changes.

| Plugin | Components | Native configuration |
| --- | --- | --- |
| `ffmpeg` | `ffmpeg/ffmpeg` decoder, `ffmpeg/prores`, `ffmpeg/aac`, `ffmpeg/pcm`, `ffmpeg/mp4`, `ffmpeg/mov`, `ffmpeg/webm` | LGPL, built-in FFmpeg components and zlib |
| `ffmpeg-x264` | `ffmpeg-x264/libx264` | GPL, libx264 |
| `ffmpeg-opus` | `ffmpeg-opus/opus` | LGPL, libopus |
| `ffmpeg-vpx` | `ffmpeg-vpx/vp9` | LGPL, libvpx |

MP4 uses the standard muxer/audio encoder and the x264 process. WebM uses the
standard muxer and the VPX/Opus processes. Removing x264 retains H.264 decoding,
ProRes export, and WebM export. Explicit old references (`ffmpeg/libx264`,
`ffmpeg/opus`, `ffmpeg/vp9`), including saved presets, must be changed to the
new IDs; there are no compatibility aliases. Existing preset fallback behavior
still applies to stale default presets.

## Implementation

- `plugins/ffmpeg` retains the MIT/Apache shared FFmpeg implementation.
  `FfmpegCodec::{new,x264,opus,vpx}` select disjoint provider profiles. Both
  `describe` and direct open/decode requests enforce those profiles; an optional
  provider exposes no decoder or muxer. Looking up libx264 through FFmpeg is
  shared source code, not a GPL dependency of the standard native build.
- The three optional crates/manifests each expose only their codec and use the
  shared implementation. They depend on no editor/renderer in production.
- Standard/Opus/VPX programs check all seven loaded FFmpeg libraries' license
  strings before serving or answering `--manifest`, rejecting GPL and nonfree
  builds. Native plugin staging runs this check before copying a binary;
  cross-compiled programs perform the same check when first run on their target.
- `scripts/build-ffmpeg-linux.sh` now builds on macOS as well as Linux.
  `FFMPEG_VARIANT=standard|x264|opus|vpx` selects the native build. macOS uses
  dylibs; Linux uses static FFmpeg archives. Only the x264 variant enables GPL.
- `scripts/plugins/ffmpeg.mts` maps `FFMPEG_{X264,OPUS,VPX}_PKG_CONFIG_PATH` to
  `PKG_CONFIG_PATH` for that package. Windows uses matching `*_VCPKG_ROOT` and
  optional `*_VCPKGRS_TRIPLET`; the default environment selects standard.
  This uses separate native builds, not unified Cargo features.
- `scripts/plugins/run-cargo.mts` runs packages separately with their native
  environments. x264 integration tests build the other three process binaries
  first and exercise actual cross-process export, including shared-memory/ring
  transport. Encoder/export tests and the benchmark moved to `ffmpeg-x264`;
  the benchmark now takes standard and x264 program paths. VP9 alpha decoding
  is checked in the VPX package, where libvpx is linked.
- CI and packaging build all four native configurations independently. macOS
  bundles each plugin's dylibs under its own `Frameworks`; Windows copies each
  process's CRT and gathers all four native notice inventories. Linux bundles
  all four plugin directories. The optional Windows NuGet workflow publishes
  separate variant packages; setup builds/caches the variants directly.
- `cargo install` sibling discovery independently finds the three optional
  program names as well as the standard program; existing precedence and
  project-plugin restrictions apply to each.
- Plugin version bumps, dependency-based affected-plugin checks, package smoke
  checks, English/Japanese guides, and the Celesta Agent Skill cover the split.
  LGPL plugins carry `LICENSE-LGPL-2.1` and `FFMPEG-SOURCE.md`; only x264 carries
  the GPL license and source offer.

## Build and validation

Use four separate prefixes. For example, after installing native dependencies:

```sh
for variant in standard x264 opus vpx; do
  FFMPEG_VARIANT="$variant" scripts/build-ffmpeg-linux.sh "$PWD/plugins/target/ffmpeg/$variant"
done
export PKG_CONFIG_PATH="$PWD/plugins/target/ffmpeg/standard/lib/pkgconfig"
export FFMPEG_X264_PKG_CONFIG_PATH="$PWD/plugins/target/ffmpeg/x264/lib/pkgconfig"
export FFMPEG_OPUS_PKG_CONFIG_PATH="$PWD/plugins/target/ffmpeg/opus/lib/pkgconfig"
export FFMPEG_VPX_PKG_CONFIG_PATH="$PWD/plugins/target/ffmpeg/vpx/lib/pkgconfig"
node scripts/plugins/build.mts plugins/ffmpeg plugins/ffmpeg-x264 plugins/ffmpeg-opus plugins/ffmpeg-vpx
```

Do not use one native FFmpeg environment with `cargo test --workspace` for the
plugins: Cargo resolves `ffmpeg-sys-next` once, which would link all binaries
to that same configuration. Replace the older generic plugins-workspace
commands in `HANDOFF.md` with per-build invocation for this architecture:

```sh
node scripts/plugins/run-cargo.mts plugins test \
  -p celesta-codec-ffmpeg -p celesta-codec-ffmpeg-opus \
  -p celesta-codec-ffmpeg-vpx -p celesta-codec-ffmpeg-x264 \
  -p celesta-plugin-vrm --locked -- --show-output
node scripts/plugins/run-cargo.mts plugins clippy \
  -p celesta-codec-ffmpeg -p celesta-codec-ffmpeg-opus \
  -p celesta-codec-ffmpeg-vpx -p celesta-codec-ffmpeg-x264 \
  -p celesta-plugin-vrm --all-targets --locked -- -D warnings
```

Checked on macOS arm64:

- Built all four native variants from FFmpeg 8.1.2 source, using the GitHub tag
  archive after ffmpeg.org reset the download connection. CI/source defaults
  remain 8.1.3 on Unix, and vcpkg 2026.07.29 supplies 8.1.2 on Windows.
- `otool -L` confirmed the standard libavcodec links none of x264/Opus/VPX;
  each optional variant links only its corresponding external codec library.
- Per-build Rust tests: 103 passed, including GPU export (no skipped codec
  tests), H.264 timing/tags, cross-process audio/video muxing, VP9 alpha,
  shared-memory ring transport, and ProRes/WebM without the x264 plugin.
- VRM workspace member tests: 385 reported passed; sample-dependent checks
  self-skipped because `CELESTA_VRM_SAMPLES_DIR` is not configured.
- `cargo test -p celesta-plugin-host --locked`: 111 passed, including optional
  sibling discovery; corresponding Clippy with `-D warnings` passed.
- Per-build Clippy for the plugins with `--all-targets --locked -- -D warnings`.
- `pnpm --dir scripts/plugins test`: 230 Vitest tests and TypeScript checks
  passed. Local scratch-tree tests used absolute `CARGO`/`RUSTC` paths because
  the user's mise wrappers do not resolve tools outside the repository.
- `python3 -B -m unittest scripts/test_package_macos.py`: 8 passed.
- Root/plugins `cargo fmt --check`, `git diff --check`, shell syntax and Node
  syntax checks. Standalone release and Windows prebuild workflows pass
  actionlint 1.7.12; that version cannot parse the existing `$/.github/...`,
  `background`, and `wait` syntax in CI/package-build workflows.

Windows/Linux native builds and complete release packaging were not executed
locally. Screenshots do not apply: this changes codec
registration/build/linking, with no renderer or scene-layout changes.
