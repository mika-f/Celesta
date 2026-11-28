# Official MP4 Export Presets via Media Plugin Manifest (2026-11-27)

## Specification

Provide five official MP4 export presets for major video platforms from the `ffmpeg` media plugin manifest (`plugins/ffmpeg/plugin.json` `media.presets`, emitted by `celesta-codec-ffmpeg --manifest`) rather than hardcoding plugin-specific preset data in the plugin host or exporter. Users can export with `--preset "<name>"`, MCP `export_video`, `@celesta.studio/exporter`, or the editor Export page without authoring `export-presets.json` first:

| Preset | Muxer | Video bitrate (H.264) | Audio bitrate (AAC) | Primary source |
| --- | --- | --- | --- | --- |
| `YouTube 1080p` | `ffmpeg/mp4` | 8,000,000 bps (8 Mbps) | 384,000 bps (384 kbps) | [YouTube recommended upload encoding settings](https://support.google.com/youtube/answer/1722171?hl=en) (1080p SDR 24–30 fps: 8 Mbps; stereo AAC-LC: 384 kbps) |
| `YouTube Shorts` | `ffmpeg/mp4` | 8,000,000 bps (8 Mbps) | 384,000 bps (384 kbps) | Same YouTube 1080p H.264/AAC encoding target ([YouTube Shorts help](https://support.google.com/youtube/answer/10059070?hl=en) / [encoding settings](https://support.google.com/youtube/answer/1722171?hl=en)); vertical resolution (e.g. 1080×1920) comes from the composition |
| `TikTok` | `ffmpeg/mp4` | 8,000,000 bps (8 Mbps) | 192,000 bps (192 kbps) | [TikTok Content Posting API Media Transfer Guide](https://developers.tiktok.com/docs/en/content-posting-api-media-transfer-guide) (MP4, H.264, AAC; 8 Mbps / 192 kbps targets clean 1080p playback within TikTok's size limits) |
| `Instagram Reels` | `ffmpeg/mp4` | 8,000,000 bps (8 Mbps) | 128,000 bps (128 kbps) | [Meta Reels Publishing Guide](https://developers.facebook.com/docs/video-api/guides/reels-publishing/) & [IG User Media reference](https://developers.facebook.com/documentation/instagram-platform/instagram-graph-api/reference/ig-user/media) (MP4, H.264, VBR up to 25 Mbps, AAC 128 kbps stereo) |
| `niconico` | `ffmpeg/mp4` | 12,000,000 bps (12 Mbps) | 320,000 bps (320 kbps) | [niconico Help FAQ #5685](https://qa.nicovideo.jp/faq/show/5685) (1080p H.264 MP4 up to 12 Mbps video, AAC stereo up to 320 kbps; note: direct fetch of the FAQ page was blocked during research, values cross-checked against indexed FAQ excerpts) |

## Implementation Details

- **`crates/plugin-host/src/media.rs`**:
  - Added `pub presets: Vec<Value>` to `MediaManifest` and parsed optional `media.presets` in `MediaManifest::from_json` (validating each entry is an object with a non-empty unique `"name"`, reporting problems leniently like component entries).
- **`plugins/ffmpeg/plugin.json`**:
  - Declared the five official MP4 presets under `media.presets` in the same JSON shape as `export-presets.json`'s `presets` array. `plugins/ffmpeg/src/lib.rs::printed_manifest()` emits `plugin["media"]` directly, so `celesta-codec-ffmpeg --manifest` includes `media.presets` automatically.
  - Each preset sets `muxer: "ffmpeg/mp4"`, leaves `video.component` and `audio.component` unset so the platform's default H.264 and AAC encoders are selected, and sets portable `rateControl: "bitrate"` + `bitrate` on video and `bitrate` on audio.
- **`crates/exporter/src/presets.rs` & `crates/exporter/src/lib.rs`**:
  - Added `plugin_presets(registry: &MediaRegistry) -> Vec<Preset>` and `find_plugin_preset(registry: &MediaRegistry, name: &str) -> Option<Preset>`, reading presets generically from registered media plugins' `media.presets` in registry search order with zero hardcoded plugin names or preset definitions.
  - Added `PresetSource::Plugin`, changed `FoundPreset.path` to `Option<PathBuf>` (`None` for plugin-provided presets), and added `PresetLookup::find_with_plugins(name, registry)` falling back to plugin presets after `--preset-file`, project `export-presets.json`, and user `export-presets.json`.
- **`crates/exporter/src/resolve.rs`**:
  - Updated `resolve_output` to resolve presets through `PresetLookup::find_with_plugins(name, registry)`.
  - Added generic `adapt_portable_options` for presets that do not pin a specific encoder component: when a portable preset sets `rateControl` to `"bitrate"` (or `"vbr"`) and the selected encoder schema uses the other token for its `bitrate`-gated rate-control mode (e.g. `"vbr"` on x264, `"bitrate"` on native encoders), the resolver adapts `rateControl` to the schema's token before sanitization.
- **`crates/editor/src/export_page/{model,view,wording}.rs`**:
  - Loaded `plugin_presets` from `MediaRegistry` in `ExportModel::new`, listed them under a `"Plugins"` section in the Export page preset picker, and fell back to them when resolving a project's `default` preset.
- **CLI / MCP / TypeScript / Docs**:
  - Updated `--preset` help text in `crates/exporter/src/cli.rs`, `crates/mcp/src/args.rs`, `packages/exporter/src/types.ts`, and `packages/exporter/README.md`.
  - Updated EN/JA export and troubleshooting guides (`packages/website/content/{en,ja}/docs/02-(guides)/09-export.md`, `05-(more)/04-troubleshooting.md`) and Celesta skill references (`skills/celesta/references/{export,errors}.md`).

## Checks

- `cargo test -p celesta-plugin-host --lib`
- `cargo test -p celesta-exporter --lib`
- `cargo test -p celesta-editor export_page`
- `cargo clippy -p celesta-plugin-host -p celesta-exporter --all-targets -- -D warnings`
- `cargo fmt --check`
