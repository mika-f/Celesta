# Scene-end drag: parts inside later scenes move and bound it (2026-11-07)

Fixes mika-f/Celesta#559. Dragging a scene's end on the Chapters row
changes its `defineScenes` entry's `durationInFrames`, and the entries
after it in the same list move with it. Chapters placed inside one of
those later scenes from another source, such as
`listChapters(plan, { from: scenes[1].from })`, did not count: during the
drag they stayed put (the reload then moved them, since their `from` reads
the scene's start), and they did not bound the drag. When the
composition's length followed the list, shortening the last scene could
leave a chapter starting at or after the new end, and
`compositionChapters` (`packages/react/src/chapters.ts`) threw on the next
reload.

Builds on `docs/handoff/2026-10-26-timeline-timing.md` (the scene drag).
User docs: `packages/website/content/{en,ja}/docs/01-(getting-started)/03-preview.md`
("Drag a scene's end").

## Specification

- **What moves.** `SceneEdge::Editable::later` now lists the chapters that
  move with the end: the entries after the dragged one in its list (as
  before), then every chapter of no entry of that list whose `from` lies
  inside one of those entries' spans (`scene.from <= from < scene.end`), in
  the order the composition reports them. A chapter that is an entry of
  another `defineScenes` list counts too: a nested plan list placed in a
  later scene moves with it. Chapters inside the dragged scene keep their
  start, so they stay. A chapter outside every later scene stays, such as
  a list placed at a written frame: its `from` doesn't read the list.
- `preview_chapters`, `scene_snap_targets` and the literal-length upper
  bound in `clamp_scene_delta` read `later`, so the moving parts are drawn
  moving, are left out of the snap targets, and stop the drag before one
  would start at or after a written composition length (**The composition
  is N frames long**).
- **A following composition ends after every chapter that stays.** When
  the composition's length is the list's (`follows`), `clamp_scene_delta`
  raises the lower bound so that each chapter not in `later`, the dragged
  one included, starts before the end of what moves: the latest end of the
  dragged chapter and of `later`, plus the delta. The entries before the
  dragged one are taken to end at 0 (they are not passed in), a bound at
  least as strict as the list's real end. So the last scene keeps at least
  1 frame (a scene starting at the end throws too), and a part inside it
  holds the end one frame after its start. When a chapter other than the
  dragged one holds it, the label adds **Chapter `<id>` starts at frame
  `<from>`**. A bound already above 0 (a source that would throw already)
  holds the drag at 0 and never pushes it forward, as the floor does.

## Implementation

- `crates/editor/src/scene_entries.rs`: `moving_chapters(chapters, list,
  index)` builds `later` for `scene_edges`; `clamp_scene_delta` gains the
  `follows` lower bound and its note. `timeline_drag.rs` is unchanged: it
  passes `later` through.

## Checks

- `cargo test -p celesta-editor` (new: `chapters_starting_inside_a_later_scene_move_with_the_end`,
  `moving_parts_bound_a_literal_length`,
  `a_following_composition_ends_after_every_chapter_that_stays`,
  `a_drag_moves_the_parts_inside_later_scenes`; the old
  `only_later_entries_of_the_same_list_count`, which asserted the gap, is
  replaced).
- `cargo clippy -p celesta-editor --all-targets -- -D warnings`,
  `cargo fmt --all -- --check`.
- Not checked by hand in the editor: no example uses parts placed in a
  later scene.

## Known gaps

- A chapter placed after the list's end or in a gap between scenes (with
  `from: SCENES.durationInFrames`, say) still stays during the drag and
  moves on reload; it isn't inside a later scene, and telling it from a
  list placed at a written frame needs the `from` expression.
- With `follows`, an earlier entry that ends after the dragged scene (a
  later entry's negative `offset`) is ignored, so the last scene can't
  shrink below 1 frame even when that entry keeps the composition longer.
