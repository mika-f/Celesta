# Component ownership and unavailable reasons as types

Issue #822, a follow-up of #810. Nothing user-facing changes: every
message, error code, and hint reads as before. What changes is that two
decisions that were made by an `else` or by matching words are now made by
types, so a future change that skips them fails to compile.

## Specification

### Which FFmpeg program serves a component (`plugins/ffmpeg`)

- `VideoCodec::profile()` and `AudioCodec::profile()` say, with an
  exhaustive `match`, which build serves each encoder: libx264 the GPL
  `ffmpeg-x264` program, every other one the LGPL `ffmpeg` program. A codec
  added to `VideoCodec::ALL` or `AudioCodec::ALL` (libx265, say) is a
  compile error until someone decides which program it belongs to.
- `Profile::components()` is the one table of `(kind, id, profile)`: the
  decoder and the muxers are the standard build's (libavformat's muxers are
  LGPL), the encoders take their codec's `profile()`.
- `Profile::of(kind, id)` looks a component up in that table (an id no
  component has goes to the standard build, which refuses it as unknown,
  as before). `describe()` and the `open_*` refusals use it through
  `FfmpegCodec::serves`.
- `decoder()`, `video_encoder()`, `audio_encoder()` and `muxer()` answer
  `Some` when the program's build serves any component of that kind
  (`Profile::serves_kind`), rather than restating `profile == Standard`.
  The answers are the same as before: the standard program offers all four,
  the x264 program only `video_encoder()`.

### Why a component is unavailable (`crates/exporter`)

- `ExportError::ComponentUnavailable::reason` is an `UnavailableReason`
  (exported from `celesta_exporter`) instead of a `String`:
  - `NotInstalled`: no installed and allowed plugin has the component.
  - `NoDefaultEncoder { kind, codec }`: a muxer whose default `kind`
    (video or audio encoder) codec no encoder writes.
  - `Other(String)`: anything else in its own words: a plugin's reason,
    why discovery skipped the program serving it (`skipped at discovery
    (…)`), "not a muxer" and the like, a still muxer refused for video.
- Its `Display` gives the same words as before (`not installed`,
  `no video encoder writes avc1 by default`), so messages, `--json`'s and
  MCP's `message`, and the editor's text are unchanged.
- `report::classify` picks the two hints by variant. A reason given as text
  that only reads like one of them (`"not installed".into()`) is `Other`
  and gets no hint. `resolve::NOT_INSTALLED`, `resolve::no_default_encoder`
  and `resolve::is_no_default_encoder` are gone.

The issue's other option for part 2, a manifest field marking a component
as an optional download so the hint could come from the plugin, is not
needed now: since #826 the not-installed hint is generic and names no
download, so nothing in the core depends on knowing which plugin is
optional. It remains the way to bring a download hint back.

## Implementation

- `plugins/ffmpeg/src/components.rs`: `VideoCodec::profile`,
  `AudioCodec::profile`.
- `plugins/ffmpeg/src/lib.rs`: `Profile::components`, `Profile::of`,
  `Profile::serves_kind`, `FfmpegCodec::serves_kind`; the four
  `MediaProvider` accessors.
- `crates/exporter/src/error.rs`: `UnavailableReason` with `Display` and
  `From<String>` / `From<&str>` (both `Other`).
- `crates/exporter/src/resolve.rs`: `not_installed` returns an
  `UnavailableReason`; `default_encoder` builds `NoDefaultEncoder`.
- `crates/exporter/src/report.rs`: hints by variant.
- Tests: `each_program_describes_and_offers_exactly_the_components_its_build_serves`
  (ffmpeg plugin) checks that each program's `describe()` lists exactly the
  table's components for its build and that each accessor is `Some`
  exactly when it describes a component of that kind;
  `other_unavailable_components_have_no_hint` (exporter) now includes the
  hinted reasons' words as `Other`. Existing tests compare reasons through
  `to_string()` or by variant.
- `HANDOFF.md`'s crate table names `Profile::components` instead of
  `Profile::of`.

## Checks

- `cargo fmt --all` in the root and in `plugins/`.
- `cargo clippy --workspace --all-targets -- -D warnings`.
- `cargo test -p celesta-exporter -p celesta-editor -p celesta-mcp` (after
  `cargo build -p celesta-codec-test`).
- `node scripts/plugins/run-cargo.mts plugins test -p celesta-codec-ffmpeg`
  and `… plugins clippy -p celesta-codec-ffmpeg -p celesta-codec-ffmpeg-x264
  --all-targets -- -D warnings` on macOS.
- No rendering changes, so no screenshots. Windows/Linux not run locally.
