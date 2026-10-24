# Adding elements design

Part 5 of `2026-10-23-editor-element-editing-overview-design.md`. Builds on
design 2 (property tables, typed values), design 3 (scope), and design 4 (JSX
insertion, imports).

## Goal

The user picks a parent, adds a rectangle, ellipse, circle, text, image, or
video to it by drawing it on the canvas, and gets an element written into the
TSX with literal values only, timed from the playhead to the end of the
scene. It is then selected and editable like any other element (designs 1, 2,
and 4).

## Non-goals

- Deleting, duplicating, or reordering elements (z-order). Undo removes an
  element just added.
- Adding user components, `Audio`, or path shapes (`Path`, `Line`, `Polyline`,
  `Arrow`).
- Adding into a parent whose children come from `{children}` or another prop.
- Formatting beyond indenting the inserted lines like their siblings.

## Picking the parent

The parent is shown in the Add menu's header ("Add to `<Group>` in
`<PriceTag>`"):

1. The selected layer, if it is a primitive `Group`.
2. Otherwise the nearest `Group` in the selected layer's source chain whose
   JSX has a children region in the source (a design 1 "Source chain" entry
   whose element is a `Group`).
If none qualifies, or nothing is selected, the menu says why and its items are
disabled ("Select a group, or a layer inside one, to add into"). (Decided by
the designer: finding a scene's root without a selection would mean following
`defineScenes`' `Scene:` across imports, which `<Scenes>` gives no position
for.)

Design 3's scope applies when the parent is written inside a shared component:
"Shared value" adds into it (every instance gets the element); "This scene" picks the
nearest qualifying parent written in the scene's own code instead, and the
header says so.

## Adding

A viewer toolbar "Add" menu lists Rect, Ellipse, Circle, Text, Image, and
Video. After choosing:

- Rect, Ellipse, Image, Video: drag a box on the canvas. A click without a drag
  makes a 200 × 200 box (Image and Video keep the file's aspect, 400 px wide).
- Circle: drag from the center outward.
- Text: click where the text starts.
- Image, Video: a file picker opens first; the file goes through
  `import_path`, so a file outside the project is copied into `assets/`.
- Escape cancels.

The box is converted into the parent's local space with the inverse parent
transform (as the drag write-back already does), so the element appears where
it was drawn.

## What is written

```tsx
<Sequence from={45} durationInFrames={75}>
  <Rect x={120} y={80} width={320} height={180} fill="#4F7CFF" />
</Sequence>
```

- The new element is the parent's last child, so it draws in front.
- It is wrapped in a `Sequence` from the parent's local frame at the playhead
  to the end of the nearest timing window (design 4's "Set timing" rule). When
  the playhead is on the window's first frame, no wrapper is written.
- Values come from the drawn box and the primitive's table defaults (design 2):
  - Rect, Ellipse: `x`, `y`, `width`, `height`, `fill="#4F7CFF"` (a fixed
    literal; `<Theme>` has no color the editor could call the accent);
  - Circle: `x`, `y` at the center with `anchorX={0.5} anchorY={0.5}`,
    `radius`, `fill`;
  - Text: `x`, `y`, `fontSize={48}`, `color` as above, children `Text`;
  - Image, Video: `src`, `x`, `y`, `width`.
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
- Otherwise a new import line is added after the last import.
- A file importing the module as a namespace (`import * as C`) gets
  `C.Rect` in the JSX instead.

After the write, the reload selects the new element: the editor remembers the
inserted element's position and, when the reloaded frame has a layer at it,
selects that layer.

## Testing

Rust (`crates/source-edit`):

- Inserting into a parent with children, an empty parent, and a self-closing
  parent; tabs and spaces.
- Imports: added to an existing named import (sorted and unsorted), a new
  import line, already imported, namespace import, a clashing local binding.
- The `Sequence` wrapper omitted on the window's first frame.

Rust (`crates/editor`):

- Parent picking rules 1–2, nothing selected, and the "This scene" fallback.
- Drawn box to parent space for a translated, rotated, scaled parent.
- Selecting the new element after the reload.

Manual, with before/after screenshots of the same frame: add each kind into a
scene, check the TSX diff, the timing band, and that the element is selected
and editable.

## Documentation

- The React preview user docs: the Add menu, parents, what is written.
- A handoff note.
