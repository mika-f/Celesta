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

Design 1 draws the selected instance's occurrences as filled bands. When the
element's nearest timing window (`windows[0]`) is its own (see below), grips
are drawn at that window's edges (`windows[0].from`/`to`, not the band's),
over the filled band in the current scene that holds the report's lookup
frame; other bands don't drag. Bands are occurrence ranges cut at the scene
and merged per pixel, while the window describes the lookup frame only. A
`remapped` window (inside a `FreezeFrame` or `Loop`) has every grip locked:
"Timing inside a loop or freeze frame can't be dragged".

A window is **the scene's**, not the element's, when it has no source (a
`<Scenes>` scene) or its span, cut at the composition's end and rounded to
whole frames, equals the span of a chapter holding the lookup frame (a part
or the scene around it). Any other window is the element's own. (Decided by
the designer.) A scene's window is edited on the chapter row instead
([Scenes](#scenes)).

The window's kind comes from the file's `@celesta/react` import, not from the
runtime (which reports a `Series` item as a `Sequence`) or the tag's text:
`Sequence` under any alias or as `C.Sequence` under `import * as C`, and
`Series.Sequence` likewise. Anything else locks every grip ("Not a
`<Sequence>` or `<Series.Sequence>` of @celesta/react"). A window that
doesn't resolve locks every grip with the reason, such as "Couldn't find this
element in the source" when the wrapper is not in the element's chain, or the
targets' own reason ("The source is still being read", "This file is outside
the project").

| Window | Start edge | End edge | Body |
| --- | --- | --- | --- |
| `<Sequence from durationInFrames>` | `from` += d, `durationInFrames` −= d | `durationInFrames` += d | `from` += d |
| `<Series.Sequence durationInFrames offset>` | `offset` += d, `durationInFrames` −= d | `durationInFrames` += d | `offset` += d |
| `defineScenes` entry (via `<Scenes>`) | see [Scenes](#scenes) | | |

- `from`, `durationInFrames`, and `offset` resolve through design 2's
  resolver as numbers (literal, const, pass-through, `Missing` with a default).
  A missing `durationInFrames` defaults to the window's current length, so
  dragging the end inserts it. A body drag on a `Sequence` without
  `durationInFrames` writes it at the length shown too: the runtime keeps
  such a window's end at its parent's end, so moving `from` alone would
  change its length against the preview.
- The window is read as an element of its own: the report's chain from the
  wrapper outward, its props read without a received value to compare with
  (the runtime reports none for a wrapper's props).
- The values are local to the window's parent timeline; drag deltas are frames
  and need no conversion. Deltas keep `durationInFrames ≥ 1` and a `Series`
  item's start (its window's `from` minus the enclosing window's, 0 at the
  root) `≥ 0`, which `Series` requires.
- Items after a `Series` item move with it (that is what `Series` does) for
  the end edge and the body; for the start edge offset and duration cancel
  and they stay. The bands of every instance the drag moves (each instance
  whose wrappers hold the window's source, or a later item's) are shown
  moving during the drag. A moved range follows the grip: the body moves it;
  the start edge moves it, cut at the window's end; the end edge cuts it at
  the new end. A range that reached the old end reaches the (new) end, for
  either edge and either direction, so the band never shrinks from the right
  while its start is dragged left.
- Locked targets: the edge or body does not drag; the cursor is not-allowed
  and the tooltip gives the reason (design 2's texts). The start edge needs
  both its props, the end edge `durationInFrames`, the body `from`/`offset`.
- Design 3's scope applies when the window is written inside a shared
  component: "This scene" lifts `from`/`durationInFrames`/`offset` like any
  number (`sequenceFrom`). The scope is the window's own (its targets and the
  calls of the component enclosing the wrapper), captured at the press and
  checked again at release by design 3's gesture gate; the label's second
  line says it ("Edits change this scene").

During the drag nothing is written: the band and a label (`from 30 → 45`)
follow the pointer, and the preview shows the playhead frame. Mouse-up writes
one history entry. A press that moves less than 3 px is a click and seeks to
the start of the drawn band holding the pressed frame; Escape and starting
playback cancel. A release that writes nothing leaves a notice beside the
grip until the next click: "Not written: …" when the scope gate holds or
refuses it or the window is no longer current, the write's error otherwise.
The released drag stays drawn until the write's reload, so the bands do not
flash back.

### Snapping

Edges snap to the playhead, chapter edges, and edges of the other bands within
6 px; Shift turns snapping off. Values are whole frames. "Other bands" are the
bands drawn that the drag leaves where they are; the window's own edges are
never targets (moving onto them changes nothing), nor are the bands that move
with it.

## Scenes

On the chapter row, a chapter's end edge drags when the chapter comes from a
`defineScenes` entry whose `durationInFrames` resolves (literal or const) in an
array literal of object literals. Dragging writes that entry's
`durationInFrames`; later entries of the same list move, as `defineScenes`
places them. Chapters are sorted by `from` and may mix sources or overlap
through offsets, so only the list's later entries move, not the chapters
after it on the row. How the entry is found is described below.

Scene durations are often expressions over a musical grid
(`S.field - S.words`, `BAR * 3`). Those are locked with design 2's inputs
listed in the tooltip, one per line (`BAR = 30`, or the input's lock reason).
A `const` the file exports is locked too ("Exported, so other files may read
it"; writing it would move the importers, design 2's shared-binding rule),
and so is an entry without `durationInFrames` ("Scene `outro` has no
durationInFrames"; `defineScenes` throws on it anyway).

`defineScenes` is a function call, not JSX, so the build's JSX positions do not
reach it. The source position of an entry is found by the editor instead: it
parses the `.ts`/`.tsx` files under the entry's folder that contain the text
`defineScenes` (skipping `node_modules`, `dist`, `.tmp` and `.git`, as the
reload watcher does), each by its extension (`<number>v` is not TSX), finds
`defineScenes([...])` calls, and picks the object literal whose `id` property
is the chapter's id as a string literal. A list holding anything but object
literals with string-literal ids (a spread, `CHAPTERS.map(…)`, a hole, an
entry with a spread or a computed key) is skipped, so its chapters lock as not
found. Zero or several matches lock the edge ("Can't find scene `intro` in
the source"). The search runs on the background executor after every reload
(generation-guarded); until it lands, every end is locked with "Reading
source…". A scene entry is not a component prop, so design 3's scope does not
gate a scene drag.

The written value is the chapter's visible length plus the drag: a chapter
cut at the composition's end shows less than its entry, and entry plus the
drag would be an unseen change. A cut chapter dragged back to exactly its
entry's length writes nothing. The bounds: the visible length stays `≥ 0`
(`defineScenes` allows 0), and no later entry of the list starts before the
list's first chapter's `from` (a later negative offset must not start before
0; the first entry's own offset is taken as 0, a bound at least as strict).
The end snaps to the playhead and the edges of the chapters that stay; its own
end and the later entries' edges, which move with it, are not targets. The
label reads `durationInFrames 90 → 100`; a press that moves less than 3 px
seeks to the chapter's start; Escape and playback cancel; a release that
writes nothing leaves "Not written: …" beside the end until the next click.

The composition's length is not changed by a scene drag. It **follows the
scenes** when `<Composition durationInFrames={X.durationInFrames}>` in the
React entry file names the binding of the `defineScenes` call holding the
entry: `X` a `const` of the same file, or a named import (alias included)
whose relative specifier resolves (`.ts`, `.tsx`, `/index.ts`, `/index.tsx`)
to the list's file and whose imported name is the list's binding. Then the
composition grows and shrinks with the list on reload. When it does not
follow the scenes (a literal, or any other expression), the drag is clamped
so that no later chapter of the list starts at or after the composition's end
(`compositionChapters` rejects that and the reload would fail) and the dragged
end doesn't pass it, and the clamp is explained beside the edge: "The
composition is N frames long".

## Giving an element its own timing

When the selected instance has no window of its own (no windows, or
`windows[0]` is the scene's by the rule above), the band shows a "Set timing"
action (a button at the start of the filled band holding the lookup frame, in
place of grips). It wraps the
element in a `Sequence` covering the current scene from the playhead to the
scene's end:

```tsx
<Sequence from={45} durationInFrames={75}>
  <Rect … />
</Sequence>
```

- `from` is the element's local frame at the playhead; the length runs to the
  end of the nearest window. With the scene's window: `from = playhead −
  windows[0].from`, `durationInFrames = windows[0].to − playhead` (the window
  cut at the composition's end, both rounded). With no window the element is
  on the composition's own timeline: `from = playhead`, and the length runs
  to the end of the outermost chapter at the playhead, else to the
  composition's end. With less than one frame left the action is disabled
  ("No frame left after the playhead").
- The element must be found where the selection says, its file inside the
  project ("This file is outside the project"), and `Sequence` free to import:
  a name bound otherwise (a local declaration, a type-only import, an import
  from another module) disables it ("`Sequence` here is not Celesta's"). The
  wrap and the import are checked dry in the background job that resolves
  the element; the action is hidden while that job runs, since a stale reason
  would mislead more than a moment without the button.
- The wrapper is indented like the element; the element is re-indented one
  level (the file's unit: a tab, or the spaces one level adds nearby). Nothing
  else in the file is re-formatted. Lines that start inside a template
  literal or a string are not re-indented, and an element that shares its
  line with other code (`<G><Rect /></G>`) is wrapped inline.
- `Sequence` is added to the file's existing `@celesta/react` import, or a new
  import is added after the last import (see design 5's import rules).
- The element must be a JSX child of an element or fragment (not the return
  value's root, a prop value, or a child inside an expression container such
  as `{cond && <Rect />}` or `.map(() => <Rect />)`); otherwise the action is
  disabled ("Wrap the element in a parent first").
- On an element shared between scenes the wrap writes the component every
  scene uses, so it follows design 3's choice: enabled when the element is
  not shared or "Shared value" is chosen, disabled under "This scene" ("Can't
  be set per scene yet"), and disabled with the index's note when the scan
  can't answer ("Scanning scenes…").
- A click pauses playback, as a band drag's press does, and writes one
  history entry (the wrap and the import in one checked text). A write that
  fails leaves "Not written: …" beside the band until the next click.
- Inside a `Sequence`, `useCurrentFrame()` restarts at 0, so an animation a
  component inside the wrapper reads from the frame (`interpolate(frame, …)`)
  starts at the new `from` instead of where it started before. A value
  computed outside the wrapper, such as a prop of the wrapped element computed
  from the enclosing component's `frame`, keeps its timing. That is
  `Sequence`'s meaning and is kept; the action's tooltip says so ("Animations
  inside start at the new start"), and the user docs too.

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
and check the TSX diff and the preview. `examples/scene-timing` has a literal
`defineScenes` list for this (every other example's list is skipped or locked
as an expression).

## Documentation

- The React preview user docs: dragging bands and chapters, "Set timing",
  locks.
- A handoff note.
