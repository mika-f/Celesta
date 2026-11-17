# Export progress in the status bar: a bar and time left

## Why

While a video exports, the editor's status bar shows one line of text:
`Starting export…`, `Exporting frame 252/600`, `Mixing export audio…`,
`Muxing…`. There is no time left and no progress bar, so it is hard to tell
at a glance how far along the export is or how long it will take.

## Goals

- A progress bar in the status bar, where the export's progress already shows.
  You can keep editing while an export runs, and the status bar is always
  visible.
- Next to it: percent, frame count, render speed, and time left, e.g.
  `[██████░░░░]  42%  252/600  31.5 fps  0:11 left`.
- Stages without a frame count (starting, mixing audio, muxing) show an
  indeterminate bar and the stage name, so the export never looks stuck.

## Non-goals

- No change to the exporter crate or to `ExportProgress`. Everything comes
  from events the editor already receives, plus the editor's own clock.
- No detailed progress card on the export page (approach C). It can be added
  later on top of this.
- No elapsed time in the "Export finished" notice.

## Approaches

- **A. Status bar (picked).** Replace the text label with a bar and the
  numbers. Always visible, does not get in the way of editing, small change.
  It has no room for the output path or encoder settings.
- B. Title bar, next to **Cancel export**. Most visible, but crowds
  **Open…**/**Create New Project…** and duplicates the status bar.
- C. Keep the export page open during an export with a detailed progress
  card (stages, large bar, elapsed/left, fps, output, encoders). Richest, but
  it reverses "an export returns you to editing" and "the page cannot open
  while exporting", and is several times the work of A.

The user picked A.

## Design

- Layout, right side of the status bar, mono font as today:
  - Starting: indeterminate bar, `Starting export…`
  - Rendering: bar at `done / total`, then `42%  252/600  31.5 fps  0:11 left`
  - Mixing audio: indeterminate bar, `Mixing audio…`
  - Muxing: indeterminate bar, `Muxing…`
  - Cancelling: the stage text becomes `Cancelling…` (the bar stays as it was)
- The bar is gpui-kit's `Progress` (`.value(percent)`, `.loading(true)` for
  indeterminate), at a fixed width so the text next to it does not move.
- Speed and time left (builder's call, below): average rate since the first
  `Rendering` event, as the CLI dashboard measures it. Events mark frame
  starts, so `done = frame - 1`; `fps = done / (now - first_event_at)`,
  `left = (total - done) / fps`. They show once rendering has run for at
  least one second, so the first frames do not give wild numbers. Time left
  reads `m:ss`, or `h:mm:ss` from one hour.
- "Time left" covers rendering only. Mixing and muxing have no measure of
  their own; they show as their own stages afterwards.
- The view requests an animation frame on every render while an export
  runs (`render.rs`), so time left updates continuously.

## Left to the builder (calls made)

- Average since the first rendered frame rather than a moving window: stable
  numbers, simple to test. A moving window can come later if speed varies a
  lot within one export.
- The text strings stay in English, as the rest of the editor UI.
- The rendering clock and the label logic live in a small pure module, so
  they are testable with given `Instant`s.

## Docs

- `packages/website/content/{en,ja}/docs/02-(guides)/09-export.md`: "the
  status bar shows progress" becomes the bar, frames, speed, and time left.

## Implementation

- `crates/editor/src/export_status.rs`: `ExportStatus` keeps the stage
  (`Starting`, `Rendering { frame, total }`, `MixingAudio`, `Muxing`) and
  when the first `Rendering` event with a nonzero total arrived.
  `update(&ExportProgress, Instant)` ignores warnings, notes, the plan,
  React compositions, and the written file. `line(now, cancelling)` returns a
  `StatusLine { percent: Option<f32>, text }`; `None` means an indeterminate
  bar. The percent in the text is floored, so it never reads 100% while the
  last frame renders.
- `EditorView::export_status` replaces `export_progress`; `start_export` sets
  `ExportStatus::new()` (the old `Rendering { 0, 0 }` placeholder is gone),
  `poll_export_events` feeds every progress event to it, and a finished or
  stopped export clears it.
- `status_bar` (`render.rs`) draws gpui-kit's `Progress` (`xsmall`, fixed
  width `metrics::export_progress_width()` = 8 rem) and the mono text.
- `helpers::export_progress_label` is removed; its tests moved to
  `export_status` (the `Output` plan case stays in `tests.rs`).
- User guide: `packages/website/content/{en,ja}/docs/02-(guides)/09-export.md`.
  `skills/celesta/` does not cover the editor's status bar.

## Checks

- `cargo test -p celesta-editor -p celesta-editor-theme`: 976 + 11 passed,
  including 10 new `export_status` tests (starting, the 1 s settle,
  31.5 fps / `0:11 left`, hours, mixing/muxing, cancelling, ignored events).
- `cargo clippy -p celesta-editor -p celesta-editor-theme --all-targets`:
  no warnings.
