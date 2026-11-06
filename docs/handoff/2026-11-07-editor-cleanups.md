# Editor cleanups: lost-release drags, reload watcher, undo checks, shared helpers (2026-11-07)

Fixes mika-f/Celesta#702: eleven small editor items, one commit each.
Paths are under `crates/editor/src/` unless they say otherwise.

## Specification

- **The ruler scrub and the overview thumb drag end at a release the view
  did not see.** A pointer move without the button ends them, as it ends
  every other pointer drag. Before, the next left drag anywhere also sought
  the playhead or panned the timeline, and the thumb stayed highlighted.
- **The React reload watcher notices every change to the files and folders
  it watches**: the source files under the entry's folder (`source_files`'
  extensions, to depth 8, skipping `node_modules`, `dist`, `.tmp` and
  `.git`), those folders, and the `--props-file`. It samples a fingerprint (`source::WatchStamp`) of every walked
  source file's path, mtime and size, every walked folder's mtime, and the
  `--props-file`'s, instead of the newest mtime. Paths are hashed relative
  to the entry's folder, so the relative path given on the command line and
  the absolute entry sampled later give the same stamp. A change that keeps
  a file's mtime and size is not noticed. Deleting or renaming a
  module that is not the newest file, and restoring a file with an older
  mtime (`cp -p`, `rsync -a`), now reload the preview.
- **Scene-list imports that name their extension are followed.** A
  specifier with a module extension names that file first (`'./scenes.ts'`
  is `scenes.ts`, not `scenes.ts.ts`); `.js`, `.jsx`, `.mjs` and `.cjs`
  also name the `.ts`/`.tsx`, `.tsx`, `.mts` and `.cts` file they compile
  from, after the literal file, as esbuild resolves them. The scene scan
  and the watcher read `.mts` and `.cts` files too. A specifier
  without one tries `.ts`, `.tsx` and the folder's `index.ts`/`index.tsx`,
  as before. An entry whose length is `SCENES.durationInFrames` with such
  an import now lets a chapter-end drag move the composition's end.
- **Undo and redo refuse a read-only file** with `HistoryError::ReadOnly`
  ("Couldn’t write `<path>`: the file is read-only"), before touching any
  file of the entry. The entry stays on its stack, so the step applies once
  the file is writable. Before, a rename replaced the file and dropped its
  read-only bit on Unix (and undoing a created file removed it), and failed
  with a bare "Access is denied" on Windows.
- **`ChangedOutside` names the file**: "`<path>` changed outside Celesta".
- **Opening a layer's source with no `VISUAL`/`EDITOR`** goes through
  GPUI's `App::open_with_system` (ShellExecute on Windows, `xdg-open` on
  Linux) instead of `cmd /C start`, which split a path at `&` and expanded
  `%NAME%`. On Windows, an editor command not found as `<program>.exe` is
  tried as `<program>.cmd`, then `<program>.bat`, so `EDITOR=code` finds VS
  Code's `code.cmd`. The user guide (en, ja) says so.
- **UI text uses ’** throughout the editor, and in `celesta-source-edit`'s
  lock text, which the editor shows; the user guide's quotes of those
  messages follow. Prose in the guide is unchanged.

## Implementation

- `timeline::RulerDrag` (`None`, `Scrub`, `Overview { grab_x, view_start }`)
  replaces `EditorView::scrubbing` and `timeline_overview_drag`; one press
  starts at most one of them. `RulerDrag::follow(dragging)` ends the drag
  on a move without the button and returns the drag to continue;
  `EditorView::continue_ruler_drag` and `end_ruler_drag` replace the four
  `continue_*`/`end_*` methods. The drag-session note says the ruler drags
  end this way rather than through `release_unseen`, whose view-level test
  would need a GPU-backed `EditorView`.
- `source::watched_stamp` replaces `watched_mtime`; `ReactPreview::watched`,
  `ReactReload::watched` and `apply_own_write_sample` carry
  `Option<WatchStamp>`. `walk_sources` is the one walk behind
  `source_files` and the stamp; it visits folders too. `None` means the
  entry has no folder or its folder's metadata can't be read; a subfolder
  that can't be listed is left out of the sample.
- `scene_entries::module_files` handles the extension cases.
- `edit_history::write_files` checks every file it writes or removes for
  the read-only bit first, so `write_recorded` and `write_all_recorded`
  dropped their own `refuse_read_only`. It stages each text with
  `celesta_remote::StagedFile::beside(path, ".tmp")` and publishes them once
  all are written; a staged file not published is removed when it drops.
  `celesta_react_bridge::write_temp` and its re-export are deleted.
  `StagedFile::beside` creates a missing folder, so redoing a created file
  whose folder was deleted recreates the folder.
- `helpers::lexically_normalized` replaces `scene_entries::normalized` and
  `element_edit::normalize`. It keeps a `..` it can't take off (`../x`,
  `a/../../x` → `../x`) and drops one right after a root. The old
  `..` after a Windows drive prefix without a root (`C:..\x`, drive-relative)
  stays. The old `element_edit` version dropped a leading `..`; the old `scene_entries`
  version popped a kept `..` with the next one (`../../x` → `x`).
- `editor-core`'s `react_entry_absolute_path` and `local_asset_path` call
  `Path::join` directly; it keeps an absolute argument as it is.
- `inline_text::TextMap` holds `celesta_composition::Matrix3` values and
  uses `multiply3`, `invert3` (which also rejects a non-finite inverse) and
  `project`. `square_to` stays.
- `editor_launch::open_source` returns `Opened::{InEditor, NoEditor}`;
  `layer_panel::open_layer_source` calls `cx.open_with_system` on
  `NoEditor`. `default_opener` is deleted. `script_fallbacks` lists the
  `.cmd`/`.bat` names, used only when `cfg!(windows)`.
- `helpers::pick_one_path(cx, PathPick, prompt)` is the single-path dialog
  for the seven sites: `Ok(None)` when it is dismissed, `Err` when it could
  not be shown. `EditorView::choose_imported_path` is the Choose… flow of
  the property row and the element row; it applies the row's guard (session,
  and for the element row the selected layer) to the dialog error and the
  import result alike. Before, the element row reported a dialog error
  without its guard.
- `icons.rs` embeds the 15 Lucide icons with
  `gpui_kit::assets::icon_assets!(LucideIcons, […])`, and the call sites
  name them with `gpui_kit::assets::IconName` (the full catalog; its
  variants for the default bundle have the same names and paths as
  `gpui_kit::component::IconName`). `DiamondFilled` is the one custom icon.
  The upstream `skip-back`, `skip-forward` and `repeat` are newer Lucide
  drawings than the copies were, with rounder corners; the other icons
  render pixel for pixel the same.

## Checks

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check
```

Tests that fail before their change:

- `timeline::tests::a_ruler_drag_ends_at_a_move_without_the_button`
- `source::tests::the_watcher_notices_a_module_deleted_from_a_subfolder`
  and `the_watcher_notices_a_file_restored_with_an_older_mtime` (both
  checked against the old max-mtime sample)
- `scene_entries::tests::composition_follows_an_imported_scene_list`
  (explicit `.tsx`, `.js` and `.mjs` imports)
- `edit_history::tests::undo_and_redo_refuse_a_read_only_file`
- `tests::lexical_normalizing_keeps_a_leading_parent`, and the relative
  case in `tests/element_edit.rs`'s
  `transform_files_outside_the_project_are_locked`
- `edit_history::tests::undo_refuses_when_file_changed_outside` and
  `an_outside_change_refuses_the_whole_entry` (the message)

`edit_history::tests::a_failed_staging_publishes_nothing` replaces the
test that blocked `write_temp`'s fixed temp name.

The icon change was checked in the app (macOS) with before and after
screenshots of the transport bar, the viewer toolbar, the Properties
rows (layer kind icon, keyframe diamond, locks, "Set at" line) and the
Layer section (parent Select). A local, uncommitted patch selected a layer
from an environment variable for both runs, since the session could not
click in the app; it is not part of the change. The Windows `.cmd`
fallback and `open_with_system` were not run on Windows or Linux.
