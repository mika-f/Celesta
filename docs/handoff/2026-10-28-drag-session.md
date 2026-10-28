# One drag flow for the timeline's band and chapter-end drags (2026-10-28)

The timeline's band drag (`BandDrag`) and chapter-end drag (`SceneDrag`)
now share one implementation of move, release, Escape, commit and
"drop the released drag on reload" (issue #574, part of #569). Every
pointer-drag's lost-release check goes through one helper.

## The shared flow

`crates/editor/src/drag_session.rs`:

- `DragSession<T>` holds what every timeline drag tracked on its own: the
  press x, the farthest the pointer went (`moved_px`), the snap targets read
  at the press, and `released`. The drag's own state is `gesture`.
- `DragGesture` is the pure part a drag supplies: the edges that snap, its
  delta, `follow` (clamps a snapped delta to its bounds; true when what is
  drawn changed), the frame a click seeks to, and where a notice sits.
- `TimelineDrag: DragGesture` is the view part: its slot on `EditorView`, its
  notice slot, `write` (the files and edit of the release, `None` for nothing
  to write, `Err` for why it can't be written), and `written_nothing` (after
  a successful write, no reload comes to drop the drag).
- `EditorView::{timeline_drag_active, continue_timeline_drag,
  end_timeline_drag, escape_timeline_drag, drop_released_timeline_drag}::<D>`
  replace the per-drag `band_*` / `scene_*` copies. Callers in `playback.rs`,
  `render.rs` and `refresh.rs` name the drag with a turbofish.

What stays per drag, and why:

| | Band | Chapter end |
| --- | --- | --- |
| Clamp | `clamp_band_delta` | `clamp_scene_delta`, with the bounds note |
| Click seeks | the pressed band's first frame, if any | the chapter's start |
| Write | `band_write` gate under the press's scope | `scene_write` edge check; nothing when the entry already holds the length |
| After a successful write | dropped when the window is still current | kept until the reload |

Both keep a released drag drawn while it is written and, after a successful
write, until the reload's `drop_released_timeline_drag`, so the bands and
chapters do not flash back.

## Lost release

`EditorView::release_unseen(event, cx, end)` handles a move without the
button: it runs `end`, notifies when it returns true, and returns true so the
caller stops following. The band and chapter drags, the transform drag, the
row scrub, the add-element drawing, the inline text selection and the master
volume drag use it.

The master volume drag used to ignore such a move and keep running until the
next mouse-up; it now ends there, like the others.

The transform drag, row scrub and add-element drawing keep their own state
and release logic (2D values, scrub value, add state machine): only the lost
release is shared with them.

## Checks

```sh
cargo fmt --all -- --check
cargo clippy -p celesta-editor --all-targets -- -D warnings -A clippy::large_enum_variant
cargo test -p celesta-editor
```

`drag_session::tests` covers the shared session: the click distance, whole
frames within the bounds, snapping and Shift, and a drag back to its start.
The existing `timeline_drag` tests (`drag_end`, clamps, snapping, labels)
pass unchanged.

`clippy::large_enum_variant` on `layer_select::SourceLookup` fails with
clippy 1.99 on `main` as well; it is not from this change.

The app was not run: no UI check of the drags.
