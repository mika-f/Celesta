# Error and option enums use thiserror, strum and clap (2026-10-28)

The hand-written `Display`/`Error`/`FromStr` boilerplate is replaced with
derives (issue #569, "thiserror / strum / ValueEnum"). Every message keeps
its exact text, and every option keeps its names.

## Dependencies

- Workspace dependencies: `thiserror` 2, `strum` 0.28 (`derive`), `clap` 4
  (`derive`), and `anyhow` 1. thiserror and strum were already in the
  lockfile through other crates.
- `celesta-gpu-renderer` and `celesta-media` gain an optional `clap`
  feature that derives `clap::ValueEnum` on their command-line options
  (`GpuDriver`, `RenderQuality`, `PeakHandling`). The exporter and the
  editor enable it; the libraries do not need clap otherwise.
- `anyhow` is for binaries only (`celesta-editor`, `celesta-mcp`). Library
  crates keep typed errors so callers can match on variants.

## Errors (thiserror)

Converted: `TimeError`, `DuckError`, `AnimationError` (composition);
`LoadError` (project); `EvaluationError` (evaluator); `MediaError`,
`AudioMixError` (media); `RenderError` (renderer); `GpuRenderError`
(gpu-renderer); `RemoteAssetError` (remote); `ReactBridgeError`,
`PropertyInputError`, `PropsEditError`, `TemplateError` (react-bridge);
`ApplyError` (source-edit); `EditorDocumentError` (editor-core);
`ExportError`, `FrameFailure`, `TimecodeError` and the `Unknown*` parse
errors (exporter); `LaunchError`, `HistoryError`, `CommitError` (editor).

- Messages that need computation (`ReactBridgeError::UnexpectedExit`'s
  indented stderr, `InvalidProperties`, `InvalidDuck`, the Google Fonts hint
  of `RemoteAssetError::Request`, `TemplateError::Conflicts`,
  `ExportError::FramesFailed`) call small private functions from the
  `#[error]` attribute.
- `#[from]` is used where a hand-written `From` existed; elsewhere inner
  errors are `#[source]`, so no new `From` conversions appear.
- `source()` is now reported uniformly: `RenderError` (remote asset, asset
  I/O, image/PSD decode, media, time, I/O, PNG), `EvaluationError` (time,
  animation) and the editor's `HistoryError::Io` had no `source()` before
  and now return the inner error. `LaunchError` and `CommitError` were
  `Display`-only and now implement `Error`, without a source. Every message already contains the
  inner error's text, and `problems::error_chain` skips a cause whose text
  the message contains, so the Problems panel text is unchanged.
- `ExportError::Frame` is `#[error(transparent)]` over `FrameFailure`,
  which now implements `Error` with `error` as its source; the text
  (`frame N: …`) and `source()` are as before.
- Left hand-written on purpose: `ValidationErrors` (joins a list),
  `PropertyIssue`, `FontFallback`, `MissingGlyphs`, and `Color` (value
  formatting rather than errors).

## Option enums (strum, ValueEnum)

`GpuDriver`, `RenderQuality`, `PeakHandling`, `VideoFormat`, `EncoderPreset`,
`ColorConversion`, `BitDepth`, `StillFormat`, `SubtitleFormat`, and
`AssetKind` derive their names with strum (`serialize_all = "lowercase"`;
`BitDepth` names its variants `8` and `10`).

- `as_str()` stays as the public way to get the name; it is no longer
  `const` (nothing used it in a const context).
- The `ALL` arrays are gone; use `strum::VariantArray` (`T::VARIANTS`).
- `FromStr` comes from `strum::EnumString` with `parse_err_ty`/
  `parse_err_fn`, so the public `Unknown*` error types and their messages
  are unchanged. `PeakHandling` gains `FromStr` (strum's `ParseError`),
  which the MCP server now uses.
- The exporter's and editor's command lines take these with `value_enum`;
  the `names()` value parser in `celesta-exporter`'s `main.rs` and the
  editor's `PossibleValuesParser` are gone. Accepted values and defaults are
  the same. `--help` (the long form) now lists each value with its
  variant's doc comment, kept on purpose, so those doc comments double as
  user-facing help; `-h` and error messages list the names as before.

## Tests

- `option_values_take_the_library_names` (exporter binary): every
  command-line enum's `ValueEnum` name equals its `as_str()`, so clap and
  `FromStr` accept the same names.
- The exporter and gpu-renderer round-trip tests iterate `VARIANTS` and pin
  the full `Unknown*` messages (`unknown driver 'd3d12' (expected auto,
  vulkan, dx12, metal, gl)`, the encoder preset list, and so on).
- Existing message tests (`ReactBridgeError::UnexpectedExit`,
  `InvalidProperties`, `ExportError::Frame`, `TemplateError`) pass
  unchanged.

## Checks

Run on macOS with FFmpeg 8.1 (`PKG_CONFIG_PATH` at Homebrew's `ffmpeg@8`):

```bash
cargo test --workspace
cargo test -p celesta-project -p celesta-composition --features codegen
cargo clippy -p celesta-composition -p celesta-project --all-targets --features codegen -- -D warnings
cargo clippy --workspace --all-targets -- -D warnings -A clippy::large_enum_variant
cargo fmt --all -- --check
git diff --check
```

All pass. Without `-A clippy::large_enum_variant`, the local clippy
(0.1.99) rejects `SourceLookup` in `crates/editor/src/layer_select.rs`,
which this change does not touch. React packages were not rebuilt: the
composition and project types keep their shape, so the generated bindings
do not change.

No composition API or user guide content changes. The CLI accepts the same
values; only the long `--help` text gains per-value descriptions.
