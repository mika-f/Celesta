# P7d: the export page (2026-11-11)

The last sub-project of P7. The editor's modal export dialog (P3) became an
export page, laid out like Resolve's Deliver page: the settings panel takes
the left slot (where Assets is), and the viewer, the right dock and the
timeline stay. Spec: `docs/superpowers/specs/2026-11-11-p7d-export-page-design.md`.
Plan: `docs/superpowers/plans/2026-11-11-p7d-export-page.md`.

## Specification

**Plugin display names (`crates/plugin-host`, `plugins/*`, `scripts/plugins`).**
`celesta-plugin.json` takes an optional top-level `name`, read by
`Manifest::display_name()` (the `id` when absent). The registry carries it
to `RegisteredPlugin.name` and `MediaComponentInfo.plugin_name`;
`MediaPlugin.name` comes from the manifest, or from `PrintedManifest.name`
for the bundled sibling's `--manifest`. The FFmpeg plugin's `plugin.json`
says `"name": "FFmpeg"`, AVFoundation's "AVFoundation", Media Foundation's
"Media Foundation", the test codec "Test codec", and the built-in image
decoder "Built-in". The plugin build (`scripts/plugins/manifest.mts`) accepts
`name` in `plugin.json` (a non-empty string) and copies it into
`celesta-plugin.json`. The CLI, MCP and the JSON report keep the references
(`ffmpeg/libx264`), and the Plugins panel keeps showing the `id`.

**x264 wording (`plugins/ffmpeg`).** The x264 `preset` field is labelled
"Speed preset", and the rate-control field "Mode", so no field repeats its
group's heading ("Rate control"). Setting keys (`preset`, `rateControl`) are
unchanged.

**Resolve warnings (`crates/exporter/src/resolve_warning.rs`).**
`ResolveWarning` (`Unavailable`, `NoAudio`, `AudioOptionsIgnored`,
`StaleValue`, `UnknownSetting`, `Extension`, `Lookup`) replaces the strings
the resolver returned. Its `Display` is byte for byte the old text, so the
CLI's stderr, the JSON report and MCP do not change.

**The page's model (`crates/editor/src/export_page/`).** The module
`export_dialog` became `export_page` (`ExportPage`, `ExportPageEvent`,
`ExportModel`). `wording.rs` turns warnings into page text (`PageWarning`:
a resolve warning, an unreadable presets file, an unknown default) with no
CLI text in it (an unreadable file's message has its backtick quoting made
typographic too), and labels components `"{name} · {plugin_name}"`
(`H.264 (x264) · FFmpeg`). Warnings about the user file's `last` show under
one muted line, "From your last export:", since no row names it; the
warnings themselves carry no prefix.

**The Export panel (`export_page/view.rs`, `view/layout.rs`, `view/save.rs`).**
`PanelHeader` "Export" with an `X` ("Back to editing"). Rows have two lanes
(a 7rem label, `metrics::inspector_label_width()`, and the control), Video
and Audio section headers, group lines ("Rate control"), and Advanced on
the label edge. Range shows the In–Out timecodes with Clear, or "Whole
composition". Loudness is disabled with "No audio is exported." when the
container takes no audio, but not before the first resolve lands. The
footer (Save as preset…, Export…) stays pinned while the body scrolls.
Enter runs Export… (`ConfirmExport` in the "ExportPage" context). While the
save panel Export… opened is up, the editor tells the page
(`ExportPage::set_waiting`): Export… is disabled, with the muted line
"Choosing where to save…", and Enter does nothing. The editor also ignores
an `Export` event while the save panel is up or an export runs, so a double
click cannot start two exports.

**The page in the editor (`export_control.rs`, `render.rs`, `panels.rs`,
`lifecycle.rs`, `main.rs`).** `ExportPageSlot` holds the page, kept between
visits. `left_panel(react, export_open)` picks Assets, Export or nothing.
While the page is open the dock row (`celesta-dock-row` either way, so the
scroll positions its panels keep by element id survive the switch) takes
its own `export_split` state and the panel at 26rem (22–34rem). The title bar's
"Export" is a toggle (`selected`/`toggled`). File → Export… and ⌘⇧E open the
page and do not pause playback. The toggle, the `X` and Escape (last in the
editor's Escape chain) close it, keeping its state. The page cannot open
while an export runs, while the save panel is up or while a project is
opening. Once a path is chosen the page closes and the export starts; a
cancelled save panel leaves it open. Opening another project or Reload
Plugins drops the page and gives the editor the focus back. In and Out on
the timeline update the Range row while the page is open.

## Deviations from the spec

- Row labels always carry a tooltip with their full text, not only when
  they are cut off.
- The save form's Save button is `ghost`, so Export… stays the panel's only
  primary.
- Advanced uses `compact_label` so its chevron sits on the label edge.
- `PresetSource::File` gets the wording "The presets file couldn't be read,
  so its presets aren't listed. {message}" (the editor never reads a
  `--preset-file`).
- After an export starts, the editor takes the focus back, as on close.
- ⌘⇧E with the page already open leaves it as it is (no re-describe, no
  focus change).
- The "your last export" context is one line above the warnings ("From
  your last export:"), not a prefix on each warning.
- An untitled document saved while the page is open is not handled: the
  editor has no save that gives a document a path in place. A document's
  path only changes by opening another project, which replaces the view
  and drops the page.

## Checks

Commands, all passing on the branch head:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace --no-fail-fast` (`celesta-editor` 816 tests). One
  earlier run failed `transform_drag::tests::a_left_edge_resize_is_one_undo`,
  a known flaky test in a file this branch does not touch; it passed alone
  three times and in the next full run.
- `cargo fmt`, `cargo clippy` and `cargo test --workspace` with
  `--manifest-path plugins/Cargo.toml`. Media Foundation's `registered()`
  is Windows-only, so its new `name` line is not compiled on macOS; it is
  the same line as FFmpeg's and AVFoundation's.
- `pnpm test` in `scripts/plugins` (228 tests)
- `pnpm --filter @celesta.studio/website check` (no issues)
- `node scripts/plugins/build.mts plugins/ffmpeg` builds again. Before the
  fix it refused `name` as an unknown key.

Screenshots (`examples/greenscreen/composite.tsx`, a 1440×900 window,
`screencapture -x -o -l <window id>`), attached to the pull request:

- `p7d-before-1-default.png`, `p7d-before-3-advanced.png`,
  `p7d-before-4-save.png` from the editor built from `main`. For the
  warnings state the P3 screenshot `export-dialog-3-preset-warnings.png`
  stands in, since that build listed no project presets.
- `p7d-after-1-default.png` … `p7d-after-4-save.png`, the same states on the
  page (after-2 shows a stale "Web" preset's warnings).
- `p7d-zoom-12.png`, `p7d-zoom-16.png`: `BASE_FONT_SIZE` set to 12 and 16
  for the run, then back to 13. The hierarchy holds, nothing overlaps, and
  at 16 the body scrolls under the pinned footer.
- `p7d-side-by-side.png`: the page beside Resolve 21's Deliver page and
  Premiere Pro 2026's Export mode.

By hand, all passed:

- Escape with the focus in the save form's Name closes the page; reopening
  it shows Advanced and the save form still open.
- Escape with the Container menu open closes only the menu; a second Escape
  closes the page; an edited CRF (23) is kept on reopening.
- The `X` returns to editing.
- The panel dragged to its narrowest and widest: nothing overlaps; a
  cut-off label shows its tooltip.
- Export… then Cancel in the save panel leaves the page as it was.
- Export… to a path: the page closes, the editor is back in editing,
  progress shows in the status bar, and the file plays.
- Reload Plugins with the page open closes the page.
- `examples/minimal.celesta.json`: the Export panel replaces Assets and
  Assets comes back on close; `i` and `o` on the timeline update the Range
  row; Clear empties it.
- `composite.tsx` (a React entry): the left panel shows only with the page;
  Range reads "Whole composition" with no Clear.

The editor finds plugins through the user's settings first. A
`pluginFolders` entry pointing at another checkout's `plugins/dist` loads
that checkout's FFmpeg build, with the old labels. The checks ran with
`CELESTA_PLUGIN_PATH=<this checkout>/plugins/dist`.

## Not done

- Merging the page's row with P7b's `InspectorRow` once both have landed.
- The plugin display name in the Plugins panel (it shows the `id`).
- While the page is open the dock row has its own split state, so a right
  dock resized while editing returns to its default width on the page, and
  back on close. (Scroll positions inside the row are kept: the row's id
  no longer changes.)
- The panel's 26rem is its starting width; a later change of the interface
  zoom moves only its range.
- No test covers the refocus after opening another project or Reload
  Plugins, nor the editor ignoring a second `Export` event while the save
  panel is up, since no test in the crate builds an `EditorView`. The
  page's side (Export… disabled, Enter ignored while waiting) is tested.
- The "Export finished" toast has no border or shadow, sits on the Volume
  control and keeps a title-only toast's text at the top. That is P7a's
  toast styling, not changed here; it is being fixed in its own change.
- P3's other "Not done" items (presets files not re-read on reopen, edits
  overwritten by a resolve that lands, and the rest) are unchanged.
