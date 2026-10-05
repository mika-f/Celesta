# Preview-only GPUI app (2026-09-25)

The GPUI app (`celesta-editor`) no longer edits projects. Its half-finished
editing features were removed for the public release: saving, undo/redo, asset
import/relink/removal, inserting clips and effects, clip move/trim/delete,
track add/move/rename/enable/lock/delete, character and dialogue editing,
lip-sync generation, clip volume automation, React entry selection, and
Inspector property editing. `celesta-editor-core` keeps only the read-only
`EditorDocument` (load, summaries, scene/audio evaluation) plus track
mute/solo, which change what the preview and an export from it play but are
never written back.

What remains:

- File > Open… (Secondary-O) opens a `.celesta.json` or a React entry into
  the current window; File > Reload (Secondary-R) re-reads it (a React entry
  re-bundles). Loading runs on a background thread and swaps the view in
  place, keeping pane sizes and the preview volume. Opening is refused while
  an export is running. Standalone React entries still hot-reload on save.
- Menus are set with `cx.set_menus` (native on macOS) and mirrored into
  `GlobalState::set_app_menus` for the in-window `AppMenuBar` on
  Windows/Linux. The window uses `TitleBar::window_options()`; the title bar
  carries the menu bar, file name, status messages, Open…, Export…, and the
  preview volume.
- The volume slider is a monitor gain on the rodio `Player`; it does not
  change the project's `masterVolume` or exports.
- Asset list, Inspector, and timeline are read-only views with selection.
  Timeline scrubbing, zoom/pan, export In/Out range, and MP4 export are
  unchanged.
