# Exporter error hints for format and input

`celesta-exporter` hints now point at the fix for four failures. Error codes,
messages, exit codes, and the `--json` report shape are unchanged; only the
`hint` text (human `hint:` line and `error.hint` in `--json`) differs.

- **Still extension without a frame selection** (`unsupported_output` for
  `.png`, `.jpg`/`.jpeg`, `.webp`): the hint says these are stills and gives
  `--frame 0` (or `--frame 0,30,60`, `--every 30`), whatever muxers are
  installed. Stills use built-in components, so the old fallback to the
  install-a-media-plugin hint was wrong. MCP: `render_frames`.
- **No muxer for a standard format** (`unsupported_output` for `.mov`,
  `.webm`, `.mp4`, `.m4v`, CLI only): MOV/WebM name the standard `ffmpeg`
  plugin (bundled with an installed Celesta; in a source checkout staged with
  `node scripts/plugins/build.mts plugins/ffmpeg --out <dir>` and
  `--plugin-dir <dir>`). MP4 also names `ffmpeg-x264` (release ZIP into the
  plugin folder plus `--allow-plugin ffmpeg-x264`, or staged with the
  standard plugin). Both mention `celesta plugin install --global <id>` for a
  registry codec, and the built-in GIF when it is available.
- **Muxer without a default encoder** (`component_unavailable` with
  `NoDefaultEncoder`): H.264 (`avc1`/`avc3`) names `ffmpeg-x264` with the
  release download, local staging, and registry paths; other codecs name the
  codec and kind (ProRes, VP9, AAC, Opus, PCM, or the raw codec string). Both
  suggest the other standard formats (never x264 for MOV/WebM) and
  `--list-components --check`.
- **React entry without `--react`** (`project_json` for a source ending in
  `.tsx`, `.jsx`, `.ts`, `.js`, `.mts`, `.mjs`): `report::source_hint` adds
  "add --react". The code stays `project_json`. The human export path prints
  it through `print_export_error`; the `--json` report through `report::build`.
- `--plugin-dir` help says codec folders (`ffmpeg`, `ffmpeg-x264`) and a
  `build.mts --out` folder are valid.

The skill's `references/errors.md` rows for `invalid project JSON` and
`unsupported_output` mention the new hints.

## Checks

- `cargo test -p celesta-exporter`: unit tests pass, including new ones in
  `report.rs` for each case. The integration tests that need a GPU display
  (`a_plugins_refusal_is_an_invalid_option`,
  `json_report_lists_every_written_file_or_a_coded_error`) fail the same way
  without the change on a headless VM with no `XDG_RUNTIME_DIR`.
- `cargo clippy -p celesta-exporter --all-targets`, `cargo fmt --check`.
- Before/after CLI output for each case is in the pull request. The
  `NoDefaultEncoder` MP4 case (ffmpeg installed, ffmpeg-x264 missing) is
  covered by unit tests only: the VM has no FFmpeg development libraries to
  build `plugins/ffmpeg`.
