# Celesta's core without first-party plugin names

## Specification

Celesta's own crates (`crates/`) no longer name or special-case the media
plugins in `plugins/` (`ffmpeg`, `ffmpeg-x264`, `avfoundation`,
`mediafoundation`). What depended on those names now comes from the
plugins' manifests or works the same for any plugin:

- **Ranking.** A media manifest may give `media.priority`, a whole number
  in `i32`'s range, 0 when absent. Among plugins of the same origin, a
  higher priority ranks first: its decoders are tried first, and its muxers
  and encoders are defaults first. Origins still rank as before (decoders:
  user, bundled, project, built-in; defaults: built-in, bundled, user, a
  project's never). A `priority` that is not a whole number in range is a
  skipped part (said like other skipped media entries) and reads as 0.
  `plugins/ffmpeg/plugin.json` has `"priority": 1`, which keeps the old
  order: before `avfoundation` and `mediafoundation` among bundled plugins.
  This replaces `FFMPEG_PLUGIN_ID`.
- **`cargo install` siblings.** Every file beside the running executable
  named `celesta-codec-<name>` (`<name>` non-empty without a `.`, plus
  `EXE_SUFFIX`) is asked for `--manifest`, in name order. As before, none
  are looked at in a Cargo build folder, and a sibling is used only when no
  plugin but a project's has its id (it then replaces the project's, said
  on stderr). A second sibling printing an id already taken is left out.
  One that fails `--manifest` is kept as `SkippedMedia` (from #825) with
  the id its name gives after the prefix (`celesta-codec-ffmpeg-x264` →
  `ffmpeg-x264`), so naming that plugin's components still reads as
  `skipped at discovery (…)`. This replaces `CODEC_PROGRAM`,
  `X264_CODEC_PROGRAM`, `X264_PLUGIN_ID`, and `SIBLING_PROGRAMS` with
  `CODEC_PROGRAM_PREFIX`; `celesta-codec-ffmpeg` now defines its own
  `PLUGIN_ID` and `X264_PLUGIN_ID` instead of taking them from
  `celesta-plugin-host`.
- **Hints.** The exporter's `classify` (the CLI's `hint:` line, `--json`'s
  and MCP's `hint`, and the editor export page's messages) no longer names
  plugins:
  - `component_unavailable` with reason `not installed`: says that
    `--list-components` (MCP: `list_components`) shows the installed
    components, and how to allow the component's own plugin
    (`--allow-plugin <plugin>`, or the Plugins tab) if it waits for approval.
  - `component_unavailable` for a muxer whose default codec no encoder
    writes (`no video encoder writes avc1 by default`): install a media
    plugin with an encoder for that codec, or write another format.
  - Other `component_unavailable` reasons have no hint; the reason, which
    the plugin gives, is in the message.
  - `unknown_preset` has no hint (it used to suggest
    `ffmpeg-x264/libx264`'s `preset` for x264's preset names;
    `X264_PRESETS` is gone from the exporter).
  - `no_decoder`'s hint, and the editor's "No media decoder can run"
    problem, say to install a media plugin, or in a source checkout to build
    the media plugins under `plugins/` with `node scripts/plugins/build.mts`.
- The exporter's `--help` and the MCP `export_video` `output` description
  describe the formats as those of the media plugins that come with
  Celesta, without plugin IDs.

## Implementation

- `crates/plugin-host/src/media.rs`: `MediaManifest::priority` and its
  parsing in `from_json`; `sibling_programs`, `is_codec_program_name`, and
  `sibling_media(program)` replace the per-name lookup.
- `crates/media/src/registry.rs`: `default_rank` returns
  `Option<(u8, Reverse<i32>)>` (origin, then priority) and `decoder_order`
  sorts the same way. Callers in `crates/exporter/src/resolve.rs` and
  `crates/editor/src/export_page/model.rs` compare the tuples.
- `crates/exporter/src/report.rs`: the plugin-specific branches are
  replaced by the two generic ones above; `resolve::is_no_default_encoder`
  recognizes `no_default_encoder`'s reason.
- Docs: the en/ja plugin guide (the `priority` field, decoder and default
  order, the `cargo install` section), the en/ja troubleshooting and export
  guides (hints), and `skills/celesta/references/` (`plugins.md`,
  `errors.md`, `export.md`).

Test fixtures still use names such as `ffmpeg/mp4` as data; only behaviour
keyed on names was removed. Packaging scripts and the plugins themselves
still name the first-party plugins, as they should.

## Checks

- `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`.
- `cargo test -p celesta-plugin-host -p celesta-media -p celesta-exporter
  -p celesta-mcp -p celesta-editor -p celesta-media-image
  -p celesta-codec-test` (after `cargo build -p celesta-codec-test`).
- `node scripts/plugins/run-cargo.mts plugins test -p celesta-codec-ffmpeg
  -p celesta-codec-avfoundation` and the same with `clippy -- -D warnings`
  on macOS (with `CARGO_TARGET_DIR` in a scratch folder, since this
  worktree's `plugins/target` link pointed at a removed cache folder).
- `media::tests::sibling::a_process_left_holding_stderr_does_not_delay_the_diagnostic`
  (unchanged by this task) failed twice, only when run with several
  packages in one `cargo test` on a busy machine (its 2.5 s bound); it
  passed every run on its own and in a `-p celesta-media -p
  celesta-plugin-host` run. The Media Foundation plugin and Windows/Linux
  were not run locally.
