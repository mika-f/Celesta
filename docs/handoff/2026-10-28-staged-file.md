# One staged-file helper for atomic writes (2026-10-28)

Single-file "write beside, then rename into place" writes now go through
`celesta_remote::StagedFile` (issue #571, and the publish half of #572).
`celesta-remote` hosts it because it is the lowest crate the exporter,
react-bridge and editor all depend on.

## The helper

`crates/remote/src/staged.rs`:

- `StagedFile::beside(destination, suffix)` creates the destination's
  directory and an empty `.celesta-<random><suffix>` file in it
  (`tempfile`, so the name is unguessable and created exclusively).
- `publish(destination, overwrite)` renames it into place: `persist` with
  `overwrite`, `persist_noclobber` without, which fails with
  `io::ErrorKind::AlreadyExists` and keeps the existing file.
- A failed publish, or dropping it unpublished, removes the staged file.
- On Unix the file is created with mode `0o666` minus the umask, the
  permissions `File::create` gives, not `tempfile`'s owner-only `0o600`.

It does not `sync_all`; callers that synced before still do.

## Callers

Each keeps its own overwrite, race and sync behavior:

| Caller | Suffix | Overwrite | Notes |
| --- | --- | --- | --- |
| `exporter` stills (`stills.rs`) | `.tmp` | export option | via `exporter/src/output.rs` |
| `exporter` subtitles (`subtitles.rs`) | `.srt` / `.vtt` | export option | via `output.rs` |
| `exporter` video (`exporter.rs`, `react_entry.rs`) | `.<video format>` | export option | FFmpeg picks the container by extension |
| `remote` download (`lib.rs`) | `.tmp` | yes | a failed rename with the entry present still counts as success |
| `react-bridge` `replace_file` | `.tmp` | yes | `sync_all` before publishing |
| `editor` audio cache `store` | `.tmp` | yes | `sync_all`; `.tmp` keeps `prune`, which only lists `.pcm`, off in-flight files |

`exporter/src/output.rs` maps the helper's errors: `AlreadyExists` becomes
`ExportError::OutputExists`, anything else `ExportError::Io` with the caller's
operation. A failure creating the output directory is now reported under the
"create temporary ..." operation rather than "create output directory".

`react-bridge/src/decode.rs` is not a publish: its private PCM file is now a
`tempfile::Builder` file (`celesta-pcm-<random>.f32le` in the temp directory,
created exclusively, `0o600`) kept with `keep()`, replacing the hand-rolled
`RandomState` name and retry loop.

## Behavior changes

- Exported stills, subtitles and videos are published with umask permissions
  (typically `0o644`) instead of `0o600`.
- Temporary file names changed (`.celesta-<random>...` instead of
  `.celesta-export-*`, `.download-*`, `.<name>.<pid>.<n>.tmp`). The video
  export's scratch directory keeps its `.celesta-export-` prefix.

## Left alone

- `editor/src/edit_history.rs` `write_files` keeps its two-phase multi-file
  rename over `celesta_react_bridge::write_temp`, which is unchanged.
- The TypeScript writers (`packages/voicevox/src/narration.ts`
  `writeAtomically`, async; `packages/character/src/voice-folder.ts`
  `snapshot`, sync and tolerant of a concurrent same-file write) are a few
  lines each in packages with no shared dependency, so they stay as they are.

## Checks

```bash
export PKG_CONFIG_PATH=/opt/homebrew/opt/ffmpeg@8/lib/pkgconfig  # macOS + Homebrew
cargo test -p celesta-remote -p celesta-react-bridge -p celesta-exporter -p celesta-editor
cargo clippy -p celesta-remote -p celesta-react-bridge -p celesta-exporter --all-targets -- -D warnings
cargo fmt --all -- --check
```

No composition API or user guide content changed, so the docs site and the
Agent Skill are untouched.
