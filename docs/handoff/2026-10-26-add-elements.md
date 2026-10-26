# Adding elements (2026-10-26)

With **Select layers** on, the **Add** menu at the start of the canvas-tool
row adds a `Rect`, `Ellipse`, `Circle`, `Text`, `Image`, or `Video` by drawing
it on the canvas. The element is written into a `Group` (the selected one, or
the nearest one around the selected layer, moved into the scene's own code
under "This scene") as its last child, with literal values only, wrapped in a
`<Sequence>` from the playhead to the end of the timing window around that
group, as one history entry; after the reload it is selected. To make the
written `<Video width>` mean something, `<Video>` now takes `width` and
`height` in every renderer, as `<Image>` does.
Design: `docs/superpowers/specs/2026-10-23-editor-add-elements-design.md`
(part 5 of `2026-10-23-editor-element-editing-overview-design.md`). It builds
on design 2 (property tables, `import_path`, `record_own_writes`, the
canvas-tool row: `docs/handoff/2026-10-24-canvas-tools.md`), design 3
(`docs/superpowers/specs/2026-10-23-editor-edit-scope-design.md`: `Scope`,
`ScopeState`, `chosen_scope`, `Edit::Splice`), and design 4
(`docs/superpowers/specs/2026-10-23-editor-timeline-timing-design.md`:
`ensure_imports`, `indent_unit`, `window_role`, `set_timing_span`).
Plan: `docs/superpowers/plans/2026-10-26-editor-add-elements.md`.
User docs: `packages/website/content/{en,ja}/docs/01-(getting-started)/03-preview.md`
("Add elements on the canvas", the tool row paragraph, and the `<Video>` rows
of the Size handles and property sections); `<Video width height>` in
`02-(guides)/06-media.md` and `03-(reference)/01-reference.md` (en, ja), and in
`skills/celesta/references/react-core.md` and `helpers.md`.

## What changed

- `<Video>` size (`crates/composition/src/model.rs`, `crates/renderer`,
  `crates/gpu-renderer/src/prepare.rs`, `packages/react`, `packages/web`):
  `LayerContent::Video { asset, timing, width, height }` with the `Image`
  arm's serde attributes. `celesta_renderer::image_source::display_size(natural_w,
  natural_h, width, height)` holds the sizing rule `Source::layout` had
  inline (both given: both apply; one: the frame's aspect; none: the frame's
  size), shared by `Image` and `Video`. The GPU renderer scales a video's
  quad by `display / texture` like an image's (the texture-limit shrink
  stays; mipmaps now follow the shown size); the CPU renderer scales the
  decoded frame, and the placeholder without a decoder is drawn at
  `display_size(320, 180, …)`. React: `VideoProps.width`/`height`
  (documented as `ImageProps`'), validated in `render.ts` ("Video width must
  be finite and positive"), the Video table's `size` and a Size group
  (`min: 1`) after Source; no `fit`. Web: `scene-canvas.ts` sizes a video
  with the image formula. The MCP scene summary (`crates/mcp/src/summary.rs`)
  and `skills/celesta/scripts/inspect.mjs` print a video's `size W×H` like an
  image's. `examples/greenscreen/composite.tsx` sizes the hero and the
  raw-footage corner by `width` instead of `scale`.
- A group's children space (`crates/gpu-renderer/src/layer_frame.rs`):
  `parent_space(scene, id) -> Option<[f64; 6]>`, the map from a group's
  children space to scene pixels from the layer tree alone (so an empty
  group has one), in `LayerFrame::parent`'s layout; `None` for a missing id,
  a non-group, or a group that itself or through an ancestor turns in 3D or
  has a perspective (`turns_in_3d`, shared with `find`).
- Insertion (`crates/source-edit/src/add.rs`): `NewKind` (`tag`, `module`),
  `NewElement`, `Timing { from, duration }`, `AddLock { NoChildrenRegion,
  Import(ImportLock) }`, `accepts_children`, `add_lock`, and
  `add_child(text, parent, new, timing) -> Result<Added { edits, line,
  column }, AddLock>`. One `ensure_imports` per module (`@celesta/react`
  with the tag and `Sequence` when wrapped; `@celesta/shapes` for `Ellipse`
  and `Circle`); tags written as the import names them (alias, `C.Rect`);
  attributes through `write_value` in `"`; the block placed after the last
  non-whitespace byte of the children, one indent unit deeper than the
  parent's opening-tag line; a self-closing parent opened (`</C.Group>` as
  written). `line`/`column` (1-based, UTF-16) point at the new element's `<`
  (not the wrapper's) in the edited text.
- The preview worker (`crates/editor/src/preview.rs`, `refresh.rs`):
  `PreviewRequest` gains `space_of` and `added_under: Option<AddedUnder {
  parent, wrapped }>`; `PreviewResult` gains `parent_space` and `added`.
  Both are answered after the render from the drawn scene, for interactive
  whole-scene frames only, like `frame_of`. `added_layer` is the named
  parent's last child (its first child when wrapped), never a mask's layer.
- The rules (`crates/editor/src/add_element.rs`, pure): `parent_scope`,
  `is_shared` (the occurrence index's instance chains through the entry:
  more than one outer part, or ranges in more than one scene; the notes
  come from `scope.rs`'s `occurrence_index` and `not_indexed_note`),
  `pick_parent`, `add_header`/`parent_label`, `report_at_parent` (drops every
  window up to and including the last one whose source is a chain entry
  below the parent), `added_timing`, `drawn_geometry`, `drawn_outline`,
  `menu_items`, `AddMode`/`AddEvent`/`add_step`, `add_files`, `PendingSelect`,
  `added_fate`, `added_confirmed`, `parent_facts` (per primitive `Group` with
  a layer id, in a readable file inside the project, accepting children: its
  tag as written, enclosing component, timing, and the lock per kind).
- The view (`add_element.rs`'s `impl EditorView`, `viewer.rs`, `render.rs`,
  `playback.rs`, `background.rs`, `refresh.rs`, `view.rs`): `AddState` on the
  view. `sync_add_facts` reads the chain's files on the background executor
  once per lookup (layer, report, frame); `add_target` runs `pick_parent` on
  them with design 3's scope; `add_menu` builds the header, the reason line,
  and the items. `shows_tool_row(react_preview, interactive)` no longer takes
  the selection: the row shows the Add button (gpui-kit `dropdown_menu`,
  `IconName::Plus`) alone, and the Rotate about and Handles toggles after it
  with a selection. `choose_add_kind` pauses, stores the `ChosenParent`, and
  for Image/Video runs `prompt_for_paths` → `import_path` → `media_aspect`
  (`image::ImageReader::into_dimensions`, `FfmpegBackend::probe`). The add
  press comes before the double-click check; move and release ride
  `drag_move`/`drag_end`; Escape is caught in `render.rs`'s key capture.
  `write_addition` refuses unless the target is the chosen parent, then
  `record_own_writes(vec![file], add_files…)`; `done` stores the pending
  selection with the reload number the write requests. `add_reload_applied`
  ends the mode, drops the facts and the space, and gives the pending
  selection its first generation; `add_reload_failed` drops it;
  `take_add_answers` (from `background.rs`) applies `added_fate`;
  `settle_added_selection` selects on the next render; `added_lookup_holds`
  checks the selected layer's first lookup. The overlay strokes
  `drawn_outline` in the accent color; the cursor is a crosshair while armed
  or drawing; the notice joins the viewer's notice chain.
- `scope.rs` exposes `occurrence_index` and `not_indexed_note`;
  `element_edit.rs` exposes `scope_context` and re-exports `tag_name`.

## Decisions

- **`<Video>` gets a real size.** The spec writes `<Video width>`, which every
  renderer ignored, so `Video` gains `width`/`height` sized exactly like
  `Image` in all three runtimes and the property table, rather than a
  `scale` workaround. No `fit`: the addition writes only `width`.
  `MediaFrame` still scales a video itself and so still needs
  `sourceWidth`/`sourceHeight`.
- **The canvas-tool row is the viewer toolbar,** shown whenever Select layers
  is on, so the menu can explain itself without a selection and the canvas no
  longer resizes when a layer is selected.
- **Parent qualification.** A chain entry needs a layer id, a file inside the
  project, and a children region. Only a sole `{children}` or prop child
  disqualifies; a `.map()` child does not. Rule 1 (the selected `Group`)
  passes the same check, so a selected `<Group>{children}</Group>` falls
  through outward. The `Group` tag is matched as written, so an alias works.
- **Scope.** The scope is the selected element's: its `chosen_scope` when it
  has a scope control, else "This scene", so a parent shared only through
  `{children}` is never added into unasked.
- **Timing around the parent.** The windows below the parent (the selected
  element's own `Sequence`) are dropped positionally; the parent's own window
  counts as a scene's for `set_timing_span`; a remapped nearest window
  disables the items. With less than a frame left, the child is wrapped in a
  one-frame `Sequence`, even on the window's first frame, as unwrapped it
  would run the whole window. The playhead is the lookup's frame.
- **Gestures.** A press moving less than 4 viewer pixels is a click; click
  sizes are 200 × 200, radius 100 for a Circle (radius 0 draws nothing), and
  width 400 for media, all in the parent's units at the press. The box is the
  min/max of the mapped press and release in parent space, so it turns with
  the parent. Every side is at least 1.
- **The parent is fixed when the item is chosen.** It is named in the hint
  while drawing, the overlay uses its space, and a release refuses ("The
  parent changed; choose the item again") when the target then differs, so
  nothing lands in a group the user was not shown.
- **Choosing pauses playback,** since the next played frame would end the
  mode.
- **Selecting the added layer.** Its id can't be
  predicted, so requests from the reloaded code's first generation carry
  `added_under`; the answer selects that layer, and its first lookup must have
  `chain[0]` at the inserted position, else the selection is cleared. A
  failed reload drops the pending selection (an undo before the next reload
  would make that one answer with an older sibling).
- **New import line in a file without imports:** `ensure_imports` writes
  `import { … } from "@celesta/react";` first (after a hashbang or
  directives); otherwise after the last import in its quotes and semicolon.

## Known gaps

- An empty `<Group />` draws no region and is in no chain, so it can't be
  selected or picked as a parent; opening a self-closing parent is built and
  tested in `source-edit` but unreachable from the canvas.
- A parent shared through `{children}` (an unshared element passed into a
  shared component) falls back outward under "This scene" and without a
  scope control; adding into it needs "Shared value", which such an element
  does not offer.
- The Rust renderers do not reject an invalid `Video` size (zero, negative,
  NaN); only the React runtime does. `display_size` could return a `Result`
  as the image path does. `render.ts` repeats the image's size check instead
  of sharing a helper.
- A zero-scaled or collapsed group still gets a (singular) children space;
  the editor reports it as "Can't draw into a group scaled to zero", also when
  a gesture maps to nothing for another reason.
- `turns_in_3d`'s doc says "not affine", but a perspective-only flat map is
  affine; excluding it is deliberate. A group's own perspective, own
  `rotationY`, and an ancestor's `rotationX` are not tested directly.
  `find_layer` descends mask layers but `parent_space`'s `find` does not.
- `accepts_children` uses its own parameter-name visitor: a callback or
  helper-function parameter and a local shadowing a parameter count as the
  prop, `props['children']` counts as a region, and anonymous components are
  missed. "Its children come from a prop" is also the text for an
  unparsable file or a missing element (never shown, as `parent_facts`
  filters those).
- Opening a self-closing parent drops a comment between its last attribute
  and `/>`; a one-line parent with spaces before its closing tag keeps them
  after the inserted block. CRLF files, both import lines new at once, and a
  comment before `/>` are untested. The position is counted from line breaks
  rather than re-read from the applied text (the tests compare it with it).
- `is_shared` with no chain through the entry disables the items ("Not found
  in the scene scan") rather than treating the group as unshared. A
  source-less window between two remaining windows (a `<Scenes>` inside the
  parent, above the element) stays in the timing; untested.
- No geometry test for skew or non-uniform scale. An Image or Video click
  without a known aspect outlines 400 × 400 but writes only `width`.
- `added_layer`'s doc should say a wrapper outside its window yields the
  previous sibling (the confirm check then clears it); `parent_space`'s
  result doc is narrower than the function. An empty parent and an empty
  wrapped group are untested in the worker.
- With a failed lookup, the menu says "Select a group, or a layer inside
  one, to add into", which is misleading. The add notice is last in the
  viewer's notice chain, so another notice hides it.
- Cancelling the add mode while a chosen file is being imported still copies
  it into `assets/`.
- With an element armed, Escape ends the add mode and stops there: an inline
  text edit open at the same time needs a second Escape.
- `parent_facts`'s skip branches (outside the project, unreadable, no
  region) are untested; `sync_add_facts` clones the report on every render
  to compare it.
- `take_add_answers` accepts a parent space from a preview generation
  before the reload (it could check `generation >= from`), and a parent read
  started before a reload can land after it.
- The header and the hint show literal backticks (`` `<Group>` ``); a long
  hint can crowd a narrow tool row.
- A selection of the added layer held back by held commits drops its confirm
  check if a reload applies meanwhile; one whose reload moved the element
  then fails the check and clears. A release while the lookup is pending
  drops the gesture with "The source is still being read".
- Not verified in the app yet: the menu, the drawing, the overlay, and the
  selection after the reload need the checks below.

## Verify by hand

Take before/after screenshots of the same frame for the pull request: before
on `main`, after on this branch.

`<Video>` size: render frame 45 of `examples/greenscreen/composite.tsx` with
the native exporter (`cargo run --release -p celesta-exporter -- --react
examples/greenscreen/composite.tsx --frame 45 --overwrite out.png`) and in the
web runtime, on `main` and on this branch. On `main` the example's `width`
props are ignored and the footage draws at 640 × 360; on this branch the hero
is 960 × 540 and the corner 224 × 126, as the old `scale` gave. In the editor,
select a `<Video>`: its **Properties** show a Size section, and the Size
handles resize it.

In the editor, with **Select layers** on, on a scene of your own:

- With nothing selected, the tool row shows the **Add** button alone, and its
  menu says "Select a group, or a layer inside one, to add into" with every
  item disabled. Select a layer: the toggles appear after it, and the frame
  keeps its size.
- Select a `<Rect>` inside a `<Group>` in a component: the header reads "Add
  to `<Group>` in `<Component>`". Add each kind into the scene and take a
  before/after screenshot of the same frame for each: Rect and Ellipse (drag
  in all four directions, then a click: 200 × 200 at the click), Circle (drag
  from the center; a click: radius 100), Text (click), Image and Video (pick
  a file outside the project: it is copied into `assets/`; drag; a click:
  400 wide with the file's height). Check the TSX diff: the element is the
  group's last child, indented one level deeper, literals only, the imports
  added (`Ellipse`/`Circle` from `@celesta/shapes`).
- A group rotated 30° and scaled 2: the element appears where it was drawn,
  turned with the group, and its `width` is half the dragged screen width.
- Timing: at frame 45 of a group's 120-frame `<Sequence>`, the element is
  wrapped `<Sequence from={45} durationInFrames={75}>`; on its first frame,
  no wrapper; with the selected `<Rect>` in its own `<Sequence>` inside the
  group, the group's window times it. The timeline's band shows the new
  element from the playhead to the window's end.
- After each addition, the new element is selected and editable (drag it,
  edit a row); one Cmd+Z removes it and its import.
- A shared component used in two scenes: with **Edits change** on **This
  scene**, the header ends "· This scene" and the element lands in the
  scene's own group; on **Shared value**, in the component's group, and both
  scenes show it.
- Refusals: a group inside a `<Loop>` ("Can't add inside a loop or freeze
  frame"), a group with `rotationX` ("Can't draw into a group turned in
  3D"), a selected group whose **Scale** row is then set to 0 (a zero-sized
  group can't be clicked; "Can't draw into a group scaled to zero"),
  a composition without `@celesta/shapes` (Ellipse and Circle disabled), a
  file with its own `const Rect` ("`Rect` here is not Celesta's").
- Choose an item while playing: playback pauses. Escape, a selection change,
  turning Select layers off, starting playback, and a reload each end the add
  mode without writing; an edit saved outside Celesta does too, through its
  reload. A refusal at the release (such as a group scaled to zero after the
  item was chosen) shows its notice at the top-left until the next click.
