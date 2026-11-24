# Linux doctor and export diagnostics

## Behavior

- A failed `celesta doctor` render probe reports the export hint when one is
  available: `checks.output.hint` in JSON and a `hint:` line in plain text.
  Schema version 1 and exit codes are unchanged. Checks without a hint omit it.
- Before the graphics probe on Linux, the CLI supplies a private temporary
  `XDG_RUNTIME_DIR` if missing or empty. It preserves any nonempty value and
  removes the temporary directory at normal exit. Drivers still write directly
  to stderr; no diagnostics are filtered.
- MP4/M4V hints name the registry plugin `ffmpeg-x264` explicitly. If no
  MP4 muxer is installed either, the hint includes the standard `ffmpeg`
  plugin's command as well. Release download and source build options remain.

## Before and after

Observed on headless Ubuntu 22.04.5 with Mesa lavapipe, Node.js 26.13.1,
`DISPLAY`, `WAYLAND_DISPLAY`, and `XDG_RUNTIME_DIR` unset. The reference
binary was built from main at `952cb14e`. The React entry was a copy of
`packages/cli/examples/with-rect.tsx`. The JSON excerpts below select only
the relevant fields from stdout; stderr was captured separately.

```text
$ celesta doctor --react film.tsx film.mov --json       # no media plugins
before (exit 1): {"code":"unsupported_output","message":"no muxer writes .mov files"}
after  (exit 1): {"code":"unsupported_output","message":"no muxer writes .mov files","hint":"MOV (ProRes/PCM) and WebM (VP9/Opus) are written by the standard ffmpeg plugin, which an installed Celesta bundles; in a source checkout, stage it with `node scripts/plugins/build.mts plugins/ffmpeg --out <dir>` and pass --plugin-dir <dir>; a codec from the registry installs with `celesta plugin install --global <id>`; or write an animated GIF to a .gif path"}

$ celesta doctor --json
before (exit 0): status = "ok"; stderr repeated 4 times:
error: XDG_RUNTIME_DIR not set in the environment.
after  (exit 0): status = "ok"; stderr empty

$ celesta doctor --react film.tsx --frame 0 frame.png --json
before (exit 0): checks.output.status = "ok"; same stderr line repeated 8 times
after  (exit 0): checks.output.status = "ok"; stderr empty

$ celesta export --react film.tsx film.mp4 --json       # registry ffmpeg 0.10.0, no x264
before (exit 1): error.code = "component_unavailable"; hint contains `celesta plugin install --global <id>`
after  (exit 1): error.code = "component_unavailable"; hint contains `celesta plugin install --global ffmpeg-x264`
```

The MOV hint matched `celesta export --json` byte for byte. Neither doctor
probe published its intended output. The MP4 case reached missing-encoder
resolution; this doesn't verify launchability of the registry's x264 binary.

## Checks

- Doctor regression tests cover JSON and plain-text hints, unchanged usage
  errors, and a successful Linux PNG probe without the XDG warning or leftover
  runtime files. Both new tests fail against the original CLI on lavapipe.
- Exporter report tests cover both MP4 installation paths, with no literal
  plugin ID placeholder.
- The website suite covers both export guide locales.
