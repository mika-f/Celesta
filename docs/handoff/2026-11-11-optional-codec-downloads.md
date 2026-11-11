# Optional codecs as separate release downloads

## Specification

Bundle only the standard LGPL `ffmpeg` plugin in the macOS/Windows app
packages and Linux image. Distribute `ffmpeg-x264`, `ffmpeg-opus` and
`ffmpeg-vpx` as separate, individually installable downloads on the same
GitHub release. Installing an optional codec must not require Rust, Node.js
or a separate FFmpeg installation.

The standard package reads media (including H.264) and exports ProRes/MOV,
AAC/PCM and still images. MP4 requires the x264 download. Normal WebM export
requires both VPX and Opus downloads. Browser WebCodecs export is unaffected.
This supersedes the all-four-bundled policy in the November 10 task note.

## Implementation

- `package-macos.py` and `package-windows.ps1` stage only `ffmpeg`. Native
  notices and normal/build Rust dependency inventories exclude optional codec
  crates/libraries. macOS's existing license collector accepts a selected
  package set and an app-inventory switch for reuse by standalone archives.
- Linux Docker builds only the standard FFmpeg prefix, stages only `ffmpeg`,
  and installs no optional x264, libopus or libvpx runtime packages. Its default
  example exports MOV; optional downloaded plugins can be mounted separately
  and passed with `--plugin-dir`.
- `scripts/package-media-plugins.py` builds one ZIP per selected codec. Names
  are `Celesta-plugin-ffmpeg-<codec>-<version>-<os>-<arch>.zip`. A ZIP contains
  exactly one installable `ffmpeg-<codec>/` folder: manifest, executable,
  native libraries, INSTALL.txt, license texts and dependency inventories.
  File modes are preserved and pre-1980 dependency timestamps are clamped to
  ZIP's supported range. Existing archives are never overwritten.
- macOS standalone archives bundle dylibs under their own Frameworks, check
  for build-machine dependency paths, sign all Mach-O files (ad hoc locally),
  and run `--manifest` without DYLD overrides before publication. Release
  builds sign with Developer ID and notarize each ZIP. ZIPs cannot carry a
  stapled ticket; notarization is online when required.
- Windows archives copy their own x64 MSVC CRT DLLs and selected vcpkg native
  notices. Linux archives bundle any dynamically linked codec shared object
  with a plugin-relative RPATH and corresponding Ubuntu copyright notices;
  glibc and platform runtime libraries remain system supplied.
- `package-build.yml` builds codec archives separately from app packages on
  macOS arm64, Windows x64 and Linux x64. Codec uploads are separate artifacts;
  Windows app ZIP upload uses the exact app filename, avoiding a wildcard
  that would also pick up codec ZIPs. The existing release artifact collector
  attaches all app/codec assets to the same release.
- Package smoke checks reject bundled optional plugins/inventory entries and
  export ProRes/MOV using only the standard plugin. Their MCP tool expectation
  also includes the existing list_components/describe_component tools.
- Packaging guides, startup text, README, English/Japanese installation,
  plugin and export guides, and the Celesta Agent Skill describe the new
  downloads and user-folder installation/approval. The x264 source offer now
  covers the optional archives, including Linux, rather than app bundles.

## Validation

- Created actual unsigned/ad-hoc-signed macOS arm64 release-profile archives
  for all three plugins under `target/packages/optional-codec-downloads/`.
  Each is about 12 MiB, includes its own native libraries/notices, and passes
  dependency-path validation, codesign verification and staged --manifest.
- Extracted all three final ZIPs with ditto into temporary paths containing
  spaces. Executable permissions survive; codesign verification and
  --manifest succeed without DYLD search-path overrides after relocation.
- Inspected ZIP contents and Rust inventories: one plugin root each, the
  selected optional codec plus shared implementation only, no application,
  exporter, VRM or other optional codec crates. The final x264 archive includes
  the updated standalone-download source offer.
- `python3 -B -m unittest scripts/test_package_macos.py scripts/test_package_media_plugins.py`:
  11 passed. Covers standard-only dependency selection, standalone dylib path
  resolution, installable ZIP shape/modes/timestamps, and exclusive output.
- `pnpm --dir scripts/plugins test`: TypeScript checks and 230 Vitest tests
  passed (absolute local CARGO/RUSTC used for scratch repositories).
- `pnpm --dir packages/website check`: no issues, 73 nonfatal metadata warnings.
- `node --check scripts/test-package.mjs` and `git diff --check` passed.
- actionlint 1.7.12 validates the changed release workflows when only its
  unsupported `$/.github/actions/setup-ffmpeg` action-reference diagnostic is
  excluded; this notation was already used by the repository.

Windows/Linux native archive builds, Developer ID notarization and a full app
installer build were not executed locally. No release was published. Rust/runtime/rendering code did not change in this
follow-up, so the previous codec/GPU tests were not repeated. Screenshots do
not apply to these distribution changes.
