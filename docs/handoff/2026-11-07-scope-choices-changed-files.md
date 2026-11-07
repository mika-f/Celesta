# Scope choices survive changes to other files (2026-11-07)

Issue #561. The editor remembers each shared element's "Edits change"
choice ([This scene] / [Shared value]) by the element's source position
(`ScopeChoices`, `crates/editor/src/scope.rs`;
`docs/handoff/2026-10-26-edit-scope.md`). A reload of the same composition
that was not the editor's own write (a save outside Celesta, or
File → Reload) forgot every choice, since the reload watcher only knew that
some file changed. Now every reload forgets only the choices in the files
that changed since the loaded code was read.

## Specification

- Saving a file outside Celesta forgets the choices of the elements written
  in that file: their positions may name other elements now. Choices in
  other files stay.
- File → Reload does the same: it compares the files on disk with the ones
  the loaded code was read from, so it forgets the choices in the files
  that changed since then, and none when nothing did.
- A change to the `--props-file` alone, or to a `prepare()` dependency (a
  voice folder and the like), moves no position and forgets nothing.
- The editor's own write still forgets the choices in the files it wrote,
  but the one carried for the selected element. If a file changed outside
  Celesta at the same time, its choices are forgotten too (before, an own
  write's reload ignored such a change).
- Loading another composition of the same entry forgets the choices in the
  files that changed since the loaded code was read (before: none, whatever
  changed).
- When which files changed is not known (the entry's folder cannot be
  sampled or resolved), an own write's reload forgets the choices in the
  files it wrote, and any other reload every choice, as before.

## Implementation

- `WatchStamp` (`crates/editor/src/source.rs`) keeps the per-file sample it
  used to hash: each folder and source file walked, by path relative to the
  entry's folder (sorted), with its mtime and size, and the
  `--props-file`'s mtime and size. Equality is what the hash compared, so
  the watcher reloads on the same changes. It is `Clone`, no longer `Copy`.
  `WatchStamp::changed_files(newer)` lists the source files changed, added,
  or removed between two samples (folders and the `--props-file` left out).
- `changed_sources(entry, loaded, latest)` turns that list into real paths
  under the canonicalized entry folder, since esbuild names modules, and so
  `SourcePos::file`, by their real paths (a temporary folder on macOS is
  `/private/var/…` behind `/var`). `None` when either sample is missing or
  the folder cannot be resolved.
- `ReactPreview::loaded` (`preview.rs`) is the sample the loaded code was
  read from: the first load's `watched`, then each successfully applied
  reload's. Unlike `ReactPreview::watched`, the watcher and own writes do
  not move it.
- `reload_composition` (`refresh.rs`) no longer clears the choices when it
  requests a reload that is not an own write. Its background task computes
  `ReactReload::changed` from `loaded` and its own `watched` sample.
  `apply_react_reload` sets `loaded` and calls
  `ScopeChoices::forget_reloaded(changed, scope_written, own_write, carry)`
  for every successful reload; a failed one forgets nothing and keeps
  `loaded` and `scope_written`, so the next successful reload's comparison
  covers its changes too. Two reloads requested from the same `loaded`
  each compare against it: the newer one's list holds the older one's, so
  nothing is missed when the older is dropped.
- A layer picked while a reload runs is cleared when it applies, as before;
  a choice made for it in that time is forgotten with the others when its
  file changed.

## Checks

- `cargo test -p celesta-editor`: 703 passed. New:
  `source::tests::two_samples_name_the_files_changed_added_and_removed`,
  `changed_sources_are_real_paths_under_the_entry_folder`,
  `a_props_file_change_names_no_source`;
  `tests::scope::a_change_outside_the_editor_forgets_only_the_choices_in_its_files`,
  `an_own_write_reload_forgets_the_written_and_the_changed_files`.
- `cargo clippy -p celesta-editor --all-targets -- -D warnings`,
  `cargo fmt --all -- --check`.
- User guide: `packages/website/content/{en,ja}/docs/01-(getting-started)/03-preview.md`
  ("Edit one scene or every scene").
