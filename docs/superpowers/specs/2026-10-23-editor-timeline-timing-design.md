# Timing on the timeline design

Part 4 of `2026-10-23-editor-element-editing-overview-design.md`. Builds on
design 1 (timing windows, bands), design 2 (typed targets, pass-through), and
design 3 (scope, multi-file history).

## Goal

The selected element's start and end, and a scene's length, are changed by
dragging on the timeline, written into the TSX. An element that has no timing
of its own can be given one.

## Non-goals

- Retiming animation inside an element (moving keyframes, stretching
  `interpolate()` ranges).
- Durations computed from audio or plans (`planDialogue`, VOICEVOX timing,
  `matchVoiceLevels`): locked.
- `<Loop>` repeat length and `<FreezeFrame>` frames.
- Changing the composition's length when it is an expression or a literal
  that the scenes do not feed: the timeline warns instead.
- Track reordering or moving an element to another scene.

## What a band edits

Design 1 draws the selected instance's occurrences as filled bands. The band
that contains the playhead (or the first one) becomes draggable when its
nearest timing window (`windows[0]`) is the element's own (see below), has a
source position, and is not `remapped` (inside a `FreezeFrame` or `Loop`,
locked: "Timing inside a loop or freeze frame can't be dragged").

A window is **the scene's**, not the element's, when it has no source (a
`<Scenes>` scene) or its span equals the current chapter's span. Any other
window is the element's own. (Decided by the designer.) A scene's window is
edited on the chapter row instead ([Scenes](#scenes)).

| Window | Start edge | End edge | Body |
| --- | --- | --- | --- |
| `<Sequence from durationInFrames>` | `from` += d, `durationInFrames` −= d | `durationInFrames` += d | `from` += d |
| `<Series.Sequence durationInFrames offset>` | `offset` += d, `durationInFrames` −= d | `durationInFrames` += d | `offset` += d |
| `defineScenes` entry (via `<Scenes>`) | see [Scenes](#scenes) | | |

- `from`, `durationInFrames`, and `offset` resolve through design 2's
  resolver as numbers (literal, const, pass-through, `Missing` with a default).
  A missing `durationInFrames` defaults to the window's current length, so
  dragging the end inserts it.
- The values are local to the window's parent timeline; drag deltas are frames
  and need no conversion.
- Items after a `Series` item move with it (that is what `Series` does). The
  bands of every instance the drag moves are shown moving during the drag.
- Locked targets: the edge or body does not drag; the cursor is not-allowed
  and the tooltip gives the reason (design 2's texts).
- Design 3's scope applies when the window is written inside a shared
  component: "This scene" lifts `from`/`durationInFrames` like any number.

During the drag nothing is written: the band and a label (`from 30 → 45`)
follow the pointer, and the preview shows the playhead frame. Mouse-up writes
one history entry.

### Snapping

Edges snap to the playhead, chapter edges, and edges of the other bands within
6 px; Shift turns snapping off. Values are whole frames.

## Scenes

On the chapter row, a chapter's end edge drags when the chapter comes from a
`defineScenes` entry whose `durationInFrames` resolves (literal or const) in an
array literal of object literals. Dragging writes that entry's
`durationInFrames`; later scenes move, as `defineScenes` places them. How the
entry is found is described below.

Scene durations are often expressions over a musical grid
(`S.field - S.words`, `BAR * 3`). Those are locked with design 2's inputs
listed in the tooltip; the grid constants can be edited in the Properties tab
when they resolve.

`defineScenes` is a function call, not JSX, so the build's JSX positions do not
reach it. The source position of an entry is found by the editor instead: it
parses the `.ts`/`.tsx` files under the entry's folder (skipping
`node_modules`), finds `defineScenes([...])` calls, and picks the object
literal whose `id` property is the chapter's id as a string literal. Zero or
several matches lock the edge ("Can't find scene `intro` in the source"). The
search runs on the background executor once per reload.

The composition's length is not changed by a scene drag. When it does not
follow the scenes (a literal, or an expression other than the scene list's
`durationInFrames`), the drag is clamped so that no chapter starts at or after
the composition's end (`compositionChapters` rejects that and the reload would
fail), and the clamp is explained beside the edge: "The composition is N
frames long".

## Giving an element its own timing

When the selected instance has no window of its own (no windows, or
`windows[0]` is the scene's by the rule above), the band shows a "Set timing"
action. It wraps the
element in a `Sequence` covering the current scene from the playhead to the
scene's end:

```tsx
<Sequence from={45} durationInFrames={75}>
  <Rect … />
</Sequence>
```

- `from` is the element's local frame at the playhead; the length runs to the
  end of the nearest window.
- The wrapper is indented like the element; the element is re-indented one
  level. Nothing else in the file is re-formatted.
- `Sequence` is added to the file's existing `@celesta/react` import, or a new
  import is added after the last import (see design 5's import rules).
- The element must be a JSX child (not the return value's root or a prop
  value); otherwise the action is disabled ("Wrap the element in a parent
  first").
- Inside a `Sequence`, `useCurrentFrame()` restarts at 0, so an animation the
  element reads from the frame (`interpolate(frame, …)`) starts at the new
  `from` instead of where it started before. That is `Sequence`'s meaning and
  is kept; the action's tooltip says so ("Animations inside start at the new
  start"), and the user docs too.

The wrap is the first use of JSX insertion; design 5 reuses it.

## Testing

Rust (`crates/source-edit`):

- Edits for each window kind and edge, including `Missing` duration.
- Wrapping a child element, with the import added, merged, or already present;
  indentation with tabs and spaces.
- Finding a `defineScenes` entry by id; zero and two matches locked.

TypeScript (Vitest):

- `Chapter` ids line up with `defineScenes` entries; windows of `Series`
  items report the `Series.Sequence` position.

Rust (`crates/editor`):

- Own versus scene window: no source, span equal to the chapter, a
  hand-placed `Sequence` inside a chapter; `remapped` windows locked.
- Scene drag clamped at a literal composition length.
- Drag-to-frames conversion at several zoom levels, snapping, Shift.
- `Series` follow-on bands move during the drag.

Manual, with before/after screenshots of the same frame: drag a `Sequence`'s
end, drag a scene's end in a literal `defineScenes`, set timing on an element,
and check the TSX diff and the preview.

## Documentation

- The React preview user docs: dragging bands and chapters, "Set timing",
  locks.
- A handoff note.
