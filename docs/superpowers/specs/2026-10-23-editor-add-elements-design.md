# Adding elements design

Part 5 of `2026-10-23-editor-element-editing-overview-design.md`. Builds on
design 2 (property tables, typed values), design 3 (scope), and design 4 (JSX
insertion, imports).

## Goal

The user picks a parent, adds a rectangle, ellipse, circle, text, image, or
video to it by drawing it on the canvas, and gets an element written into the
TSX with literal values only, timed from the playhead to the end of the
timing window around the parent. It is then selected and editable like any other element (designs 1, 2,
and 4).

## Non-goals

- Deleting, duplicating, or reordering elements (z-order). Undo removes an
  element just added.
- Adding user components, `Audio`, or path shapes (`Path`, `Line`, `Polyline`,
  `Arrow`).
- Adding into a parent whose only child is `{children}` or another prop.
- Formatting beyond indenting the inserted lines like their siblings.

## Picking the parent

The parent is shown in the Add menu's header ("Add to `<Group>` in
`<PriceTag>`", the component the group is written in; "Add to `<Group>`"
outside one):

1. The selected layer, if it is a primitive `Group`.
2. Otherwise the nearest `Group` in the selected layer's source chain (a
   design 1 "Source chain" entry whose element is a primitive `Group`).

Either qualifies only when its chain entry has a layer id, its file is inside
the project (a group in a file outside it is skipped, and the pick moves
outward), and its JSX has a children region in the source: any element with a
closing tag, or a self-closing one (opened on insert). Only a parent whose
sole child is `{children}` or another prop (`{props.children}`, a
destructured prop) is disqualified, as its children come from the caller;
`<Group>{items.map(…)}</Group>` qualifies, the new element going after the
map. The `Group` tag may be an alias of the import.
If none qualifies, or nothing is selected, the menu says why and its items are
disabled ("Select a group, or a layer inside one, to add into"). (Decided by
the designer: finding a scene's root without a selection would mean following
`defineScenes`' `Scene:` across imports, which `<Scenes>` gives no position
for.) An empty `<Group />` draws no region and is in no chain, so it can't be
picked; opening a self-closing parent is built for when it can.

Design 3's scope applies when the parent is shared: when the occurrence
index's instance chains through it have more than one outer part (the chain
above it), or their ranges show in more than one scene. The scope in effect is
the selected element's. "Shared value" adds into the shared parent (every
instance gets the element); "This scene" picks the nearest qualifying parent
written in the scene's own code (not shared) instead, and the header ends
"· This scene". Without a scope control on the selected element (design 3's
"not shared"), the default "This scene" applies, so a parent shared only
through `{children}` is never added into unasked. A remembered choice that is
now locked, or a parent whose sharing the index can't answer, disables the
items with the lock's text or design 3's note ("Scanning scenes…",
"Couldn't scan scenes: …", "Not found in the scene scan", "The scenes are not
scanned"). Under "This scene" with no unshared group in the chain: "No group
in this scene's own code to add into".

Other reasons disable the items too, shown on the line under the header:
"The source is still being read" (the selected layer's lookup, the groups'
files, the scope, or the parent's space is pending), "Can't add inside a loop
or freeze frame" (the nearest window around the parent is remapped), "Can't
draw into a group turned in 3D" (the parent or an ancestor has
`rotationX`/`rotationY` or a perspective, so it has no flat children space),
and "Can't draw into a group scaled to zero" (its children space has no
inverse). Per item: "`Ellipse` comes from `@celesta/shapes`, which this
composition doesn't import" (and "`Circle` …") when the composition registered
no property table for it, and the import clash below.

## Adding

An "Add" menu lists Rect, Ellipse, Circle, Text, Image, and Video. It sits at
the start of the canvas-tool row (design 2's), which is the viewer toolbar:
the row now shows whenever Select layers is on in a React preview, holding the
Add menu alone without a selection and the Rotate about and Handles toggles
after it with one, so the canvas no longer resizes when a layer is selected.
Choosing an item pauses playback and fixes the parent shown at that moment;
the button then reads "Adding Rect", and a muted hint beside it names that
parent and the gesture ("to `<Group>` in `<PriceTag>` · Drag a box, or
click · Esc cancels"; "Drag from the center · Esc cancels" for Circle, "Click
where the text starts · Esc cancels" for Text, "Choose a file…" while the
file dialog is open). After choosing:

- Rect, Ellipse, Image, Video: drag a box on the canvas. A click without a drag
  makes a 200 × 200 box at the press (Image and Video keep the file's aspect,
  400 wide).
- Circle: drag from the center outward; a click makes radius 100 (the click
  box's circle, as a radius of 0 draws nothing).
- Text: click where the text starts.
- Image, Video: a file picker opens first; the file goes through
  `import_path`, so a file outside the project is copied into `assets/`.
  Cancelling it ends the mode; a failure shows "Couldn't show the file
  dialog: …" or "Couldn't use the chosen file: …".
- A press that moves less than 4 viewer pixels is a click. The press must be
  inside the frame.
- Escape cancels.

The add mode also ends after one addition, when the selection changes, when
Select layers is turned off, when playback starts, and when any reload
applies.

The press and the pointer are converted into the parent's children space with
the inverse parent transform (as the drag write-back already does), and the
box is the min/max of the two points there, so the element appears where it
was drawn and turned with a rotated parent. Sizes, the click sizes included,
are in the parent's units; every drawn side is at least 1. Image and Video
take their width from the drag and their height from the file's aspect,
extending in the drag's direction; without a known aspect (an SVG, a failed
probe) the outline follows the drag but only `width` is written. The overlay
strokes that parent-space shape mapped back to the scene (a turned quad, a
circle as a polygon, a caret mark for Text).

At the release, the addition is refused when the parent is no longer the one
fixed when the item was chosen ("The parent changed; choose the item again"),
for any of the menu's reasons that arose meanwhile, and when the parent's
space is not known yet ("The source is still being read"). A refusal, and a
write that fails ("The source changed while editing" when the parent's tag is
no longer at its position, or the lock's text), shows as the viewer's notice
until the next press.

## What is written

```tsx
<Sequence from={45} durationInFrames={75}>
  <Rect x={120} y={80} width={320} height={180} fill="#4F7CFF" />
</Sequence>
```

- The new element is the parent's last child, so it draws in front.
- It is wrapped in a `Sequence` from the parent's local frame at the playhead
  to the end of the nearest timing window around the parent (design 4's "Set
  timing" rule). The windows are the parent's, not the selected element's:
  those written below the parent in the chain (the selected element's own
  `Sequence`) do not wrap the new child and are dropped. The parent's own
  window counts like a scene's here, both being the window around the
  parent. Without a window, the span runs on the composition's timeline to
  the end of the scene at the playhead (the composition's end outside one).
  When the playhead is on the window's first frame, or at frame 0 with the
  span running to the composition's end, no wrapper is written. When less
  than a frame of the window is left, the child is wrapped in a one-frame
  `Sequence`, even on the window's first frame (unwrapped it would run the
  whole window).
- Values come from the drawn box and the primitive's table defaults (design 2):
  - Rect, Ellipse: `x`, `y`, `width`, `height`, `fill="#4F7CFF"` (a fixed
    literal; `<Theme>` has no color the editor could call the accent);
  - Circle: `x`, `y` at the center with `anchorX={0.5} anchorY={0.5}`,
    `radius`, `fill`;
  - Text: `x`, `y`, `fontSize={48}`, `color` as above, children `Text`;
  - Image, Video: `src`, `x`, `y`, `width`. `<Video>` takes `width` and
    `height` like `<Image>` (both given: both apply; one given: the frame's
    aspect; none: the frame's size), in the composition model, the GPU and
    CPU renderers, and the web runtime, with a Size group in its property
    table; it has no `fit`.
- Only literals are written, so every value is editable afterwards.
- A self-closing parent (`<Group />`) is opened (`<Group>…</Group>`).
- Inserted lines are indented one level deeper than the parent's opening tag,
  with the file's indent unit (tabs or the parent's leading spaces).

### Imports

- `Rect`, `Text`, `Image`, `Video`, `Group`, `Sequence` come from
  `@celesta/react`; `Ellipse`, `Circle` from `@celesta/shapes`.
- If the file has a named import from that module, the name is added to it,
  keeping the list's order style (sorted lists stay sorted). An existing local
  binding of the name that is not that import locks the action ("`Rect` here is
  not Celesta's").
- Otherwise a new import line is added after the last import, in its quotes
  and semicolon; a file without imports gets `import { Rect } from
  "@celesta/react";` first (after a hashbang or directives).
- A file importing the module as a namespace (`import * as C`) gets
  `C.Rect` in the JSX instead.

One addition is one history entry; undo removes it. After the write, the
reload selects the new element. Its layer id can't be predicted, but it is the
parent layer's last child in the reloaded scene (that child's first child when
wrapped), so the first frames drawn with the reloaded code ask the preview for
it under the parent's layer id. Found: it is selected, and its first lookup
must put the element at the inserted position, else the selection is cleared.
Not found: the kept selection stays.

## Testing

`<Video>` size: a 320 × 180 frame sized by `width`, both sides, `height`,
and neither gives the same sizes in the GPU renderer, the CPU renderer, and
the web runtime; the composition model round-trips the size; React rejects a
size that is not finite and positive.

Rust (`crates/source-edit`):

- Inserting into a parent with children, an empty parent, and a self-closing
  parent; tabs and spaces.
- Imports: added to an existing named import (sorted and unsorted), a new
  import line, already imported, namespace import, a clashing local binding.
- The `Sequence` wrapper omitted on the window's first frame.

Rust (`crates/editor`):

- Parent picking rules 1–2, nothing selected, and the "This scene" fallback.
- The timing windows around the parent, not the selected element; the
  one-frame wrapper.
- Drawn box to parent space for a translated, rotated, scaled parent; the
  click sizes.
- Selecting the new element after the reload.

Manual, with before/after screenshots of the same frame: add each kind into a
scene, check the TSX diff, the timing band, and that the element is selected
and editable.

## Documentation

- The React preview user docs: the Add menu, parents, what is written.
- The media guide, the reference, and the skill: `<Video width height>`.
- A handoff note.
