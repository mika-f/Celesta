# Set timing beside the entry's file name (2026-11-01)

Issue: [mika-f/Celesta#558](https://github.com/mika-f/Celesta/issues/558).
Builds on `docs/handoff/2026-10-26-timeline-timing.md`.

## Problem

On a standalone React composition's row, the composition block starts at
`view.x(0.0)` and writes the entry's file name at its left edge;
`set_timing_overlay` put the **Set timing** button at `view.x(start)`, the
start of the band holding the lookup frame. When that band starts at frame 0
(for example Intro's background `<Rect>` in `examples/scene-timing/film.tsx`
with the playhead inside Intro), the button covered "film.tsx".

## What changed

- `crates/editor/src/timeline.rs`: while the button shows, the composition
  block leaves its file name out and `set_timing_overlay` (now taking
  `entry`) draws it. The overlay is a flex row from the block's start: a
  column holding the file name and two empty spacers, one as wide as the
  band's start from the block's start and one as wide as the part of the
  block scrolled off before the lane (`-view.x(0.0)`, at least 0), then the
  button with the same `pl_1` as before. The column is as wide as the widest
  of the three, so the button starts after the file name, the band's start,
  and the lane's leading edge, whichever is furthest, without measuring text.
- User guide (`packages/website/content/{en,ja}/docs/01-(getting-started)/03-preview.md`,
  "Change timing on the timeline"): where the button sits when the band
  starts under the file name or before the view.

## Decisions

- **The button moves, the name stays.** Of the issue's options (shift the
  button, hide the name, move the name to the track header), shifting keeps
  both readable and leaves the header as it was.
- **The name draws above the bands while the button shows.** It is part of
  the overlay, which draws after the bands so the button takes clicks; the
  filled band (accent at 42%) no longer tints the name. The overlay has no
  mouse handlers of its own, so a press on the name still reaches the band.
- **Unchanged elsewhere.** A band starting past the name puts the button at
  `band start + pl_1` as before, and a band starting before the view puts it
  at the lane's leading edge as `view.x(start).max(0.0)` did.

## Checks

- `cargo build -p celesta-editor`
- `cargo test -p celesta-editor`
- `cargo clippy -p celesta-editor --all-targets -- -D warnings`
- `cargo fmt --all -- --check`, `git diff --check`

The layout is GPUI flex and has no unit test; the editor has no GPUI test
harness. Screenshots of the same scene before and after (Intro's background
in `examples/scene-timing/film.tsx`, playhead inside Intro; a band starting
mid-scene; zoomed in with the band's start scrolled off) go with the pull
request.
