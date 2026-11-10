# Editor tests: a scratch folder per call (2026-11-11)

## Problem

`cargo test -p celesta-editor` failed now and then on tests that pass alone,
for example `transform_drag::tests::a_left_edge_resize_is_one_undo` and
`a_missing_max_width_is_inserted`, with `Os { code: 2, kind: NotFound }` at
`fs::read_to_string(&scene)`. Their helper named its temporary folder
`celesta-resize-{pid}-{SystemTime nanos}`. On macOS the clock ticks in
microseconds, so two tests running in parallel could read the same tick, get
the same folder, and overwrite or remove each other's `scene.tsx`.

## Change

- `crates/editor/src/scratch.rs` (`#[cfg(test)]`):
  `scratch_dir(name) -> PathBuf` makes `celesta-{name}-{pid}-{n}` under the
  system's temporary folder, with `n` from a process-wide `AtomicU64`. It
  clears a folder of that name first (left by an earlier run whose process
  had the same id) and creates it.
- Every test helper that named its folder after the clock now calls it:
  `transform_drag.rs` (`one_file`, `a_two_file_center_rotation_is_one_undo`),
  `export_dialog/view.rs` (`scratch`), `tests.rs` (the last-settings and
  disk-cache tests), `tests/add_element.rs`, `tests/element_edit.rs` (two
  tests), `tests/inline_text_view.rs`, `tests/scope_commit.rs` and
  `tests/timeline_timing.rs`.
- Left as they are: helpers keyed by process id and a name each test uses
  once (`source.rs`, `plugins.rs`, `property_panel.rs`, `scene_entries.rs`),
  and the ones already on a counter (`edit_history.rs`, `audio_cache.rs`).

New tests that need a folder of their own should call
`crate::scratch::scratch_dir`.

## Checks

- `cargo test -p celesta-editor`, five runs in a row, all green.
- `cargo clippy -p celesta-editor --all-targets -- -D warnings`
- `cargo fmt --all --check`
