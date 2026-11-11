# Repair PR #810's website and Windows native-cache failures

> Superseded by [the x264-only split](2026-11-11-ffmpeg-x264-only-split.md): Windows no longer builds FFmpeg in CI behind `actions/cache`; it downloads two pinned NuGet packages. The per-package `VCPKG_ROOT` fingerprint in `scripts/plugins/ffmpeg.mts` remains, for the standard and x264 roots.

## Specification and diagnosis

Fix the failures on PR head `50550398` in CI run `42853206899` without
weakening tests or merging the optional codec builds back together.

- All three root Test jobs failed the website's content contracts: the
  build-from-source guide no longer had its required macOS/Windows/Linux tab
  group. Both locales had been flattened, reducing the total tab count from
  26 to 24. Makit content checking alone did not exercise these Vitest tests.
- Windows Plugins failed `alpha_survives_encoding_and_webm_muxing` because
  `libvpx-vp9` was unavailable. The vcpkg configure log correctly enabled
  libvpx in the VPX prefix, but Cargo compiled `ffmpeg-sys-next` only for the
  first, standard configuration and reused it for Opus/VPX. vcpkg-rs does not
  emit rerun-if-env-changed for VCPKG_ROOT; ffmpeg-sys-next delegates to it.
  Unix pkg-config builds already track their native environment variables.

The connected diagnostics tool was unavailable; logs were obtained through
GitHub CLI. The failing website contracts were reproduced locally.

## Implementation

- Restore matching platform tabs in both source-build guides, retaining
  separate native prefixes and making the macOS dependency PKG_CONFIG_PATH
  command executable rather than a commented example. Existing website
  expectations remain unchanged.
- For the four FFmpeg Cargo packages using VCPKG_ROOT, append their selected
  `installed/<triplet>/lib` native search path through CARGO_ENCODED_RUSTFLAGS.
  This gives Cargo a distinct artifact fingerprint for each native install,
  including ffmpeg-sys-next, while leaving output directories and sibling
  executable discovery unchanged.
- Preserve existing encoded flags and Cargo's precedence over ordinary
  RUSTFLAGS. Space-containing native paths stay one argument. Unrelated
  packages and pkg-config-only environments remain unchanged.
- Add regression coverage that compiles a tiny real Cargo project whose
  build script reads VCPKG_ROOT without tracking it. Standard, Opus, VPX,
  x264 and standard again must report their own native root in one shared
  target directory. Both implicit and explicit host targets are tested.
  This models the actual vcpkg cache failure without needing FFmpeg or a
  Windows host. Tests are TypeScript/Vitest, including the Rust fixture.

## Local checks

- Plugin scripts' TypeScript checks and Vitest: 236 passed, including the
  real-Cargo cache regression and encoded/ordinary flag preservation.
- Website Vitest: all 52 passed; website test TypeScript check passed.
- `git diff --check`: passed.

The complete local website npm test also generates Worker types from local
settings; it encountered an unrelated Env fixture mismatch due to local
CELESTA_DOWNLOAD_URL variables. CI's existing type-generation step passed on
all three systems before the content tests failed. The changed content was
verified with the full website Vitest suite and its normal test type check.
Windows native linkage is verified by the new PR CI run after this fix is
pushed; no Windows host is available locally. No Rust production or rendering
code changed, and no tests are skipped or relaxed by this fix.
