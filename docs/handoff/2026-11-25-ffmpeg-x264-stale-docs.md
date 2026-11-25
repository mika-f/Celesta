# FFmpeg split documentation cleanup

## Spec

Correct the stale references tracked in [#823](https://github.com/mika-f/Celesta/issues/823)
without changing plugin or export behavior. The old exporter `X264_PRESETS`
comment was already removed when the presets moved to the FFmpeg plugin.

## Implementation

- Distinguish the FFmpeg license texts and document the test use of
  `check_lgpl` and the scope of the `--check-license` flag.
- Clarify that the standard plugin is the only **FFmpeg** plugin staged
  inside the macOS app; AVFoundation is staged alongside it.
- Document the x264 FFmpeg prefix required by the direct Cargo benchmark
  command. Use Media Foundation HEVC as the explicit encoder example in
  both export guides; H.264 remains the default.

## Checks

- `cargo fmt --manifest-path plugins/Cargo.toml --all -- --check` passed.
- `pnpm --filter @celesta.studio/website check` reported no issues
  (95 warnings).
- Offline `cargo check` could not resolve uncached dependencies. The
  targeted FFmpeg license test resolved dependencies online but could not
  build: the FFmpeg development library `libavutil` was not available to
  pkg-config.
