# Official Built-in MP4 Export Presets (2026-11-27)

## Specification

Add five official built-in MP4 export presets for major video platforms so users can export with `--preset "<name>"`, MCP `export_video`, `@celesta.studio/exporter`, or the editor Export page without authoring `export-presets.json` first:

| Preset | Muxer | Video bitrate (H.264) | Audio bitrate (AAC) | Primary source |
| --- | --- | --- | --- | --- |
| `YouTube 1080p` | `ffmpeg/mp4` | 8,000,000 bps (8 Mbps) | 384,000 bps (384 kbps) | [YouTube recommended upload encoding settings](https://support.google.com/youtube/answer/1722171?hl=en) (1080p SDR 24–30 fps: 8 Mbps; stereo AAC-LC: 384 kbps) |
| `YouTube Shorts` | `ffmpeg/mp4` | 8,000,000 bps (8 Mbps) | 384,000 bps (384 kbps) | Same YouTube 1080p H.264/AAC encoding target ([YouTube Shorts help](https://support.google.com/youtube/answer/10059070?hl=en) / [encoding settings](https://support.google.com/youtube/answer/1722171?hl=en)); vertical resolution (e.g. 1080×1920) comes from the composition |
| `TikTok` | `ffmpeg/mp4` | 8,000,000 bps (8 Mbps) | 192,000 bps (192 kbps) | [TikTok Content Posting API Media Transfer Guide](https://developers.tiktok.com/docs/en/content-posting-api-media-transfer-guide) (MP4, H.264, AAC; 8 Mbps / 192 kbps targets clean 1080p playback within TikTok's size limits) |
| `Instagram Reels` | `ffmpeg/mp4` | 8,000,000 bps (8 Mbps) | 128,000 bps (128 kbps) | [Meta Reels Publishing Guide](https://developers.facebook.com/docs/video-api/guides/reels-publishing/) & [IG User Media reference](https://developers.facebook.com/documentation/instagram-platform/instagram-graph-api/reference/ig-user/media) (MP4, H.264, VBR up to 25 Mbps, AAC 128 kbps stereo) |
| `niconico` | `ffmpeg/mp4` | 12,000,000 bps (12 Mbps) | 320,000 bps (320 kbps) | [niconico Help FAQ #5685](https://qa.nicovideo.jp/faq/show/5685) (1080p H.264 MP4 up to 12 Mbps video, AAC stereo up to 320 kbps; note: direct fetch of the FAQ page was blocked during research, values cross-checked against indexed FAQ excerpts) |

## Implementation Details

- **`crates/exporter/src/presets.rs` & `crates/exporter/src/lib.rs`**:
  - Added `builtin_presets() -> &'static [Preset]` and `find_builtin_preset(name: &str) -> Option<&'static Preset>`.
  - Added `PresetSource::Builtin` and changed `FoundPreset.path` to `Option<PathBuf>` (`None` for built-ins).
  - Updated `PresetLookup::find` to fall back to `find_builtin_preset(name)` after `--preset-file`, project `export-presets.json`, and user `export-presets.json`, so file-based presets override built-ins of the same name.
  - Each built-in preset sets `muxer: "ffmpeg/mp4"`, leaves `video.component` and `audio.component` unset so the platform's default H.264 and AAC encoders are selected, and sets portable `rateControl: "bitrate"` + `bitrate` on video and `bitrate` on audio.
- **`crates/exporter/src/resolve.rs`**:
  - Added `adapt_portable_options` for presets that do not pin a specific encoder component: when a portable preset sets `rateControl` to `"bitrate"` (or `"vbr"`) and the selected encoder schema uses the other token for its `bitrate`-gated rate-control mode (e.g., `ffmpeg-x264/libx264` uses `"vbr"`, while `avfoundation/h264` and `mediafoundation/h264` use `"bitrate"`), the resolver adapts `rateControl` to the schema's token before sanitization so no warning is emitted and the target bitrate takes effect on all platforms.
- **`crates/editor/src/export_page/{model,view,wording}.rs`**:
  - Added a `"Built-in"` section to the Export page preset picker and fell back to built-ins when resolving a project's `default` preset.
- **CLI / MCP / TypeScript / Docs**:
  - Updated `--preset` help text in `crates/exporter/src/cli.rs`, `crates/mcp/src/args.rs`, `packages/exporter/src/types.ts`, and `packages/exporter/README.md`.
  - Updated EN/JA export guides (`packages/website/content/{en,ja}/docs/02-(guides)/09-export.md`) and `skills/celesta/references/export.md`.

## Checks

- `cargo test -p celesta-exporter`
- `cargo test -p celesta-editor export_page`
- `cargo clippy -p celesta-exporter -p celesta-mcp --all-targets -- -D warnings`
- `cargo fmt --check`
