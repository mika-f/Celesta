# Typed component ownership and component refusals

Issue #822: two design follow-ups from #810. Nothing user-visible changes:
messages, `--json`/MCP codes, and hints read as before.

## Specification

### Which FFmpeg program serves a component

- `VideoCodec`, `AudioCodec`, and `MuxerFormat` in
  `plugins/ffmpeg/src/components.rs` each have `profile()`, an exhaustive
  `match` naming the build (`Profile::Standard`, LGPL; `Profile::X264`, GPL)
  that serves each variant. A new variant (libx265, say) does not compile
  until someone decides which program serves it.
- `Profile::table(kind)` in `plugins/ffmpeg/src/lib.rs` lists every
  component of a kind with its build (the decoder is `Standard`'s). It
  replaces `Profile::of`'s `else`, and everything that asked "is this the
  standard program" derives from it:
  - `Profile::serves(kind, id)`: `describe` filters on it, and every
    `probe`/`decode_*`/`open_*` refuses a component it does not serve.
  - `Profile::serves_kind(kind)`: `decoder()`, `video_encoder()`,
    `audio_encoder()`, and `muxer()` answer `Some` only for kinds the
    build serves some component of.
- `Profile::gpl()` (what `--check-license` requires) and
  `Profile::optional()` (checking components before printing the manifest
  or serving) replace `== Profile::X264` / `!= Profile::Standard` in
  `process.rs`.
- The test `each_program_serves_what_its_manifest_declares` checks the
  table against `plugins/ffmpeg/plugin.json` and
  `plugins/ffmpeg-x264/plugin.json`, kind by kind, in order.

### Exporter refusals as types

- `ExportError::ComponentNotInstalled { component }` ("… is unavailable:
  not installed") is what a reference no registered plugin has gives,
  unless discovery skipped that plugin's program, which stays
  `ComponentUnavailable` with the skipped reason.
- `ExportError::NoDefaultEncoder { muxer, kind, codec }` ("… is
  unavailable: no video encoder writes avc1 by default") is what a default
  muxer gives when no encoder writes its default codec.
- `report::classify` picks the two hints by variant; it no longer compares
  reason text (`resolve::NOT_INSTALLED`, `no_default_encoder`, and
  `is_no_default_encoder` are gone). A `ComponentUnavailable` whose reason
  merely reads "not installed" has no hint.
- `ExportError::unavailable_reason()` gives the text after "is
  unavailable:" for the three variants; preset fallback warnings use it,
  so they read as before.
- The editor's `resolution_error` adds the hint for all three variants.

The issue's other option, a manifest field marking a component as an
optional download, was not added: since #826 no hint names a download, so
nothing would read it.

## Checks

- `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`.
- `cargo test -p celesta-exporter -p celesta-mcp -p celesta-editor`
  (after `cargo build -p celesta-codec-test`).
- `node scripts/plugins/run-cargo.mts plugins test -p celesta-codec-ffmpeg
  -p celesta-codec-ffmpeg-x264` and the same with `clippy --all-targets --
  -D warnings`, with `CARGO_TARGET_DIR` in a scratch folder.
