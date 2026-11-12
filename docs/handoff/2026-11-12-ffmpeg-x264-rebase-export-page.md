# FFmpeg x264 split: rebase onto the export page (#813)

Continues [PR #810](https://github.com/mika-f/Celesta/pull/810) on
`codex/ffmpeg-codec-downloads` after #813 (the export page, plugin display
names) and #812 merged into `main`. The branch was rebased onto `main` as
AGENTS.md asks; the x264 split itself is described in
[`2026-11-11-ffmpeg-x264-only-split.md`](2026-11-11-ffmpeg-x264-only-split.md).

## Textual conflicts

- **Export guide (en/ja `09-export.md`).** #813 replaced the export dialog
  with the export page. The resolution keeps the page's text from `main` and
  the branch's notes on the standard plugin, the x264 download and the first
  container. The component-label example names **ProRes (prores_ks) ·
  FFmpeg**, since `H.264 (x264)` now comes from `ffmpeg-x264`. The
  `mediafoundation` paragraph keeps the branch's native H.264 default.
- **Plugins page (en/ja `05-plugins.md`).** `main`'s display-name sentence is
  kept, with the example id `ffmpeg-x264/libx264`.
- **`plugins/ffmpeg/tests/process.rs`.** `main` added a `name` assertion to
  the manifest test the branch replaced. It now sits in
  `manifest_and_describe_exclude_x264`.
- **`plugins/ffmpeg/src/tests.rs`.** Both `main`'s display-name test and the
  branch's licence-version test are kept.

## Changes after the rebase

- **Wording.** The branch's notes in the export and troubleshooting guides
  (en/ja) and the Skill's error table say "export page", not "dialog".
- **`PageWarning::PassedOver`.** #813 made the page's warnings a typed
  `PageWarning` enum. `resolve_first_available` now returns
  `PassedOver { first, used, reason }` for the container it moved past.
  `ExportModel::warning_text` words it as before ("MP4 cannot be exported
  here, so the export starts as MOV: …"), passed through `plain` like the
  page's other text.
- **`RegisteredPlugin::name`.** #813 added a required display name.
  `celesta_codec_ffmpeg_x264::registered()` takes it from `plugin.json`'s
  `name`, or the id without one, and `--manifest` keeps a `name` when present.
  `ffmpeg-x264/plugin.json` has no `name`, so the page labels the encoder
  **H.264 (x264) · ffmpeg-x264**. Giving it a display name is a product
  decision left open.

## Checks

- **Root workspace:** `cargo check --workspace --tests --locked` passes.
  `cargo clippy --workspace --all-targets -- -D warnings` passes, and so does
  `cargo fmt --all -- --check` after formatting.
- **Root tests:** `cargo test` passed 831 editor tests. Plugin-host, exporter,
  MCP, react-bridge, codec-test and media passed 855 more. The exporter
  integration tests need a GPU adapter: lavapipe (`mesa-vulkan-drivers`), as
  in CI. Without one, 18 of them fail at "could not select a GPU".
- **Plugin scripts (Vitest):** 244 passed, 2 skipped.
- **Website:** Vitest 52 passed; `makit check` reported no issues.
- **Python packaging tests:** 19 passed.
- **Plugins workspace:** `rustfmt --check` on the changed files only. The
  FFmpeg-linked crates were not built, because this container has no FFmpeg
  8.1 development libraries; PR CI covers them.
