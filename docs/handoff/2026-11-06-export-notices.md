# Export warnings and notes in the editor and the exporter TUI

Issue: [mika-f/Celesta#675](https://github.com/mika-f/Celesta/issues/675).

## Problem

- The editor dropped `ExportProgress::Warning` and `ExportProgress::Note`
  on the assumption that the preview already listed them. The preview lists
  only the current frame's notices (`plugin_notices` is cleared every frame,
  and font fallbacks are per preview), so a notice from a frame that was never
  previewed reached only the exporter's terminal or `--json` output.
- When the exporter's TUI closed, it printed every note and then every
  warning, while the plain output prints them in arrival order.

## Specification

- **Editor.** While an export runs, `EditorView::export_notices`
  (`problems::ExportNotices`) keeps its warnings and notes in arrival order,
  each once. When the export ends (finished, cancelled, failed, or the worker
  stopped), `Problems::log_export_notices` logs them as one history entry
  under `Source::Export`, before the entry for how it ended, and empties the
  list. `start_export` also empties it.
  - Title: `Export reported 2 warnings and 1 note` (or only the kinds
    reported). Detail: one line per notice, as the exporter prints it:
    `warning: …` for a warning, and a note as it is (it starts with
    `note: `).
  - Severity: `Warning` when a warning was reported, otherwise the new
    `Severity::Info` (an `Info` icon in muted color). The entry has no toast,
    since the export's own entry is announced right after it, and it is not
    active, so the status bar counts do not change.
- **Exporter TUI.** `progress::State` keeps one `diagnostics` list of
  `Diagnostic::Warning` / `Diagnostic::Note` in arrival order. The panels
  still list only the warnings (`State::warnings`); after the display closes,
  `State::scrollback` prints every entry in order, the same lines the plain
  output prints.

## Checks

- `cargo test -p celesta-editor problems` — `export_notices_are_logged_once_in_arrival_order`,
  `export_notes_alone_are_logged_as_information`,
  `export_notices_are_emptied_once_logged`, `an_export_without_notices_logs_nothing`.
- `cargo test -p celesta-exporter --bins progress` —
  `scrollback_keeps_warnings_and_notes_in_arrival_order`.
- `cargo clippy -p celesta-editor -p celesta-exporter --all-targets -- -D warnings`.
- `cargo test -p celesta-editor -p celesta-exporter --lib --bins`.

The Problems tab change was not checked by running the app.
