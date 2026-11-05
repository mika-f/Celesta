# React export keeps a remote `<Audio src>` as a URL (2026-11-05)

`<Audio src="https://…">` played in the native preview but made a React
export fail after every frame had rendered. The exporter kept its own copy of
the code that turns `<Audio>` reports into `AudioClip`s, written before the
bridge learned to load `http`/`https` assets. That copy always built
`AssetLocation::File` and joined it onto the entry folder, so the mixer looked
for `/entry/https://…` and failed with NotFound. This hit every React export
path (CLI `--react`, the editor's export, MCP `export_video`), including a
remote `<Video>`'s soundtrack. JSON projects were not affected.

## What changed

- `build_audio_graph` (`crates/exporter/src/project.rs`) adds the clips from
  `celesta_react_bridge::react_audio_clips`, the function the preview uses. A
  remote `src` becomes `AssetLocation::Url`, a relative one is joined onto the
  entry folder, and an absolute one is kept. Ids (`react-audio:{n}`), field
  mapping, and ducking checks are the same as before.
- The exporter's copies of `merge_react_audio_clips`, `seconds_to_time`, and
  `AUDIO_SECONDS_TIMESCALE` are gone. The exporter has one way to build React
  audio clips, the bridge's.
- Semantics are unchanged: identical `<Audio>` reports collapse into one clip,
  so two identical siblings play once, as in the preview and the browser
  export. The comments that claimed the merge preserved multiplicity now say
  this, and the exporter test is renamed `merges_identical_frame_reports`.
  The count `merge_react_audio_clips` returns is the number of frame reports,
  not the number of copies.

No user guide change: the en/ja media pages already say export downloads a
remote `src`.

## Checks

- `crates/exporter/src/tests.rs`: `keeps_a_remote_react_audio_source_as_a_url`
  builds a graph from `https://example.com/a.mp3` against `/entry` and
  expects `AssetLocation::Url`. Before the fix it got
  `File { path: "/entry/https://example.com/a.mp3" }`.
- The existing `build_audio_graph` tests (relative `./…` sources against
  `/entry/root`) pass unchanged.
- `cargo test -p celesta-exporter -p celesta-react-bridge`, and
  `cargo clippy -p celesta-exporter -p celesta-react-bridge --all-targets -- -D warnings`.
- No end-to-end test downloads a remote `<Audio>` during a React export: the
  download itself is covered by `crates/remote`, and the URL resolution by
  the unit tests above.
