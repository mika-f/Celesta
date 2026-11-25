# FFmpeg plugin split regression tests

Follow-up to the split in #810 ([issue #821](https://github.com/mika-f/Celesta/issues/821)).
No export defaults or plugin discovery rules change.

- The standard FFmpeg subprocess encodes RGBA16 from the shared-memory ring
  into ProRes MOV and VP9 WebM with Opus, then reads both files back.
  Unequal high and low sample bytes expose byte swaps; a media client
  test also checks the wire message contains a slot and no pipe payload.
  The rendered alpha tests check the red quadrant's blue channel.
- The export dialog keeps MP4 with a user-installed x264 when Windows
  has no Media Feature Pack. With equally ranked bundled encoders, the
  native one is discovered first and still wins. Discovery covers an x264-only
  sibling, a broken x264 sibling beside standard FFmpeg, and a user's x264
  taking precedence over a working sibling.
- The package smoke test exports an MP4 without selecting components and
  checks the native H.264 encoder in the export report. It keeps the existing
  Windows skip for runners without a usable system encoder.
- The Linux packaging tests stage a versioned x264 SONAME from representative
  `ldd` output, check its license and provenance, and reject missing or
  system-resolved libraries before or after setting the executable's RPATH.

## Checks

- `cargo test -p celesta-plugin-host --lib`
- `cargo test -p celesta-media process::tests`
- `cargo test -p celesta-editor export_page::model::tests`
- With the standard FFmpeg 8.1 prefix, run `cargo test --manifest-path
  plugins/Cargo.toml -p celesta-codec-ffmpeg --test process --test export`.
- `python3 -B -m unittest scripts/test_package_media_plugins.py`
- `node --check scripts/test-package.mjs`; the package smoke test itself
  requires a built macOS or Windows distribution.
