# Canvas tools (2026-10-24)

On the canvas, rotation and scale turn about the layer's pivot by default,
or about the middle of its box with Center chosen in the "Rotate about"
[Center | Pivot] toggle of a tool row docked at the top of the viewer
(shown with a selection); a cross marks the point they turn about, and in
Pivot mode the pivot (the anchor point plus `origin`) cross Alt-drags the
anchor without moving the layer; and the row's "Handles" [Size | Scale]
toggle makes the corner and edge handles resize the layer through its
`size` props (`width`/`height`, `radius`, or a `Text`'s `maxWidth`),
keeping the opposite edge where it is. Every drag that has to keep
something still moves `x`/`y` by one rule, and is still one history entry,
in one or two files.
Design: `docs/superpowers/specs/2026-10-23-editor-property-editing-design.md`
(section "Canvas": "Pivot" and "Resize"). This is part 2c; it builds on 2a
(`docs/handoff/2026-10-23-property-editing-engine.md`) and 2b
(`docs/handoff/2026-10-24-property-sections.md`), and 2d builds inline text
editing on it.
Plan: `docs/superpowers/plans/2026-10-24-editor-canvas-tools.md`.
User docs: `packages/website/content/{en,ja}/docs/01-(getting-started)/03-preview.md`
("Move layers on the canvas": "Turn about the center or the pivot",
"Resize with the Size handles", "Move the anchor").

## What changed

- Canvas geometry (`crates/editor/src/canvas_geometry.rs`, new): a layer's
  own pixels (relative to its anchor point, as
  `EvaluatedTransform::homography` lays the plane out) and the maps around
  them, built with the renderer's own `EvaluatedTransform::linear` and
  `translation` (2D part), never re-derived. `layer_map(transform)` is the
  own-pixel → parent map; `pivot(transform)` is `layer_map` applied to
  `origin`, so the anchor point plus `origin`. `LocalBox { min, max }`
  (`size`, `center`, `at(fx, fy)`) is a box in own pixels;
  `local_box(frame)` measures the outline through the inverse maps,
  `anchor_box(frame)` measures `frame.anchor_box`; both are `None` when a
  map collapses or the renderer draws the layer through its homography
  (`perspective_distance().is_some() && is_projective()`).
  `with_anchor_box(frame, primitive, received)` sets the box the source
  element's anchor and size props refer to (see Decisions). `keep_fixed(after,
  fixed_local, fixed_parent)` is the one compensation rule. `apply` is
  shared with `transform_handles.rs`; `box_frame` builds test frames.
- Anchor box in the renderer (`crates/gpu-renderer/src/hit.rs`, `layer.rs`,
  `prepare.rs`, `layer_frame.rs`): `HitRegion.anchor_box` holds a text's
  layout box in scene pixels, in the order of `corners`
  (`PreparedLayer::anchor_box_corners`, from the texture's
  `anchor_origin`/`anchor_span`, mapped through the same `drawn_transform`
  as the quad; `quad_corners` now delegates to `quad_corners_between`). An
  anchor box that doesn't map to the scene through its planes is dropped,
  and the region stays without it. `LayerFrame.anchor_box` is the box the renderer anchors the layer in: the
  region's `anchor_box`, else its quad; `None` for a group, a path, and
  instances.
- Drag props keyed by prop name (`crates/editor/src/transform_handles.rs`,
  `transform_drag.rs`): `drag_values`, `prop_values`, `preview_values`,
  `handle_props`, `drag_label`, `shown_handles`, `TransformDrag.values` and
  `.editable`, and `DragState::Editable` carry `PropKey` (the six transform
  props, `anchorX`/`anchorY`, and the table's `size` props); `TransformProp`
  stays the six. `prop_changes` moves every key by the same delta from the
  received value (position, rotation, anchors, sizes) except scales, which
  move by the same ratio; a `Text`'s `maxWidth` is written as the new box
  width itself (`Resize::absolute`). `start_value` reads the transform for
  the six, `transform.anchor` for the anchors, and the anchor box for sizes
  (`Resize::start`).
- Center / Pivot (`transform_handles.rs`, `transform_drag.rs`, `view.rs`):
  `PivotMode { Center, Pivot (default) }`. `turning_point` is the box
  center (`local_box`) in Center mode, else `pivot()`; `turn()` measures
  angles about it, and the Scale tool's ratios are measured from it.
  `drag_values` collects the props after the drag and, in Center mode,
  appends `x`/`y` from `keep_fixed`. `compensable(targets)`: both `x` and
  `y` resolve (through a pass-through too) to `Literal` or `Missing`.
  `pivot_fallback` / `pivot_used` / `PivotFallback { NotCompensable,
  Animated, AnimatedBox, NoFlatBox }` give the mode a drag uses;
  `TransformDrag` keeps it from the start. `EditorView::pivot_note` is the
  reason a chosen Center is not in effect (the toggle's warning, the
  handles' tooltips); `TransformDrag::with_note` keeps it for a handle that
  turns about the pivot (`turns_about_pivot`), and `TransformDrag::label`
  puts "Pivot — reason" (`pivot_note_text`) before the written props.
  `EditorView.pivot_mode` is session state.
- The pivot cross and the anchor drag (`transform_handles.rs`,
  `transform_drag.rs`, `viewer.rs`): `Handle::Anchor`; `pivot_cross` (the
  scene position of `pivot()`); `anchor_handle` (the cross, when `x`, `y`,
  `anchorX`, `anchorY` are all in `editable` and the anchor box is known);
  `pressed_handle(.., alt)` takes the cross with Alt within reach, even over
  another handle, and is `handle_at` otherwise. `anchor_values` moves the
  anchor by the pointer's move in own pixels over the anchor box's size
  (unclamped) and keeps the box's top-left corner still. `drag_props` adds
  the anchors when `anchor_blocker` finds nothing that keeps them: both
  anchor rows `Literal` or `Missing` (not keyframes), compensable `x`/`y`,
  no keyframed `rotation` or scale (the compensation turns and scales with
  them) or size (`sizes_animated`: the compensation is the anchor's change
  times the box, another on the other keyframes), and a known anchor box.
  `anchor_reason(targets, frame, primitive)` is the cross's tooltip, for
  every cause: a locked anchor row's own reason, `GROUP_ANCHOR`, then
  `anchor_blocker`'s first cause, naming `x`/`y` with their row's reason
  ("X: Comes from DX, which can't be traced", "X is animated") and the
  shared `ANCHOR_ANIMATED` / `TURN_ANIMATED` / `SIZE_ANIMATED` /
  `NO_ANCHOR_BOX`. The viewer draws the cross at the pivot for every
  selected layer with a frame (one handle wide, accent color), with a
  hitbox that sets `OpenHand` while Alt is held when the drag is offered
  and `OperationNotAllowed` when a reason blocks it (`CrossDrag`), and
  redraws on `on_modifiers_changed`.
- The Size tool (`transform_handles.rs`, `transform_drag.rs`, `view.rs`,
  `viewer.rs`): `ResizeTool { Size (default), Scale }` (the "Handles"
  toggle; "Size" was "Rect" before the second hand check); `SizeField { name,
  min, max, integer, absent, keyframed }`; `SizeProps { Box, Radius, Width }`;
  `Resize { props, compensable, animated }`, the Size tool resolved for a
  layer (`animated`: a keyframed `rotation`, scale, or anchor, which hides
  every handle that moves the anchor point).
  `size_props(targets)` reads `table.size` and the fields' bounds, and needs
  every named prop `editable`; `resize_fallback` / `resize_used` /
  `ResizeFallback { NoSize, Locked, Uneditable, Pinned, Animated, NoBox }`
  mirror the pivot pair (`NoSize` also when a flat shape has no anchor box,
  its size unreported; `Pinned`/`Animated` when no corner or edge passes
  `resizes`, see `resizes_any`: "x or y can't move", or `Animated` with the
  shared `TURN_ANIMATED` / `ANCHOR_ANIMATED` / `SIZE_ANIMATED`);
  `EditorView::resize_note` is the reason a chosen Size is not in effect,
  for the toggle's warning. `drag_values` gains
  `resize: Option<&Resize>` (`None` is the Scale tool) and sends corners
  and edges to `resize_values`; `shown_handles` and `handle_props` follow
  `resizes` / `resize_moves`. `handle_quad` puts the Size tool's handles and
  stem on the anchor box, which the viewer then strokes as the selection
  outline; `TransformDrag::with_resize` attaches the tool and
  `with_primitive` the element's name, and `TransformDrag::anchored`
  rebuilds the anchor box of a running (or released, being written) drag
  from the values it draws (`received_now`), so the handles follow the box
  they resize and don't jump to the outline before the reload. An anchor
  drag instead holds the whole overlay (outline, handles, stem) on its
  start frame (`TransformDrag::overlay_frame` / `holds_overlay`) and draws
  only the cross where the drag's `x`/`y` put the pivot (`anchor_cross`):
  the anchor is a reference point, and preview frames, rounded as written
  and lagging the pointer, would make the layer seem to jitter.
  `EditorView.resize_tool` is session state.
- Tool row (`viewer.rs`): `tool_row`, one row docked at the top of the
  viewer panel's column, above the canvas (the canvas, `flex_1`, shrinks by
  `TOOL_ROW_HEIGHT` and re-letterboxes; nothing is covered), shown when
  `shows_tool_row(react_preview, interactive, selected)`: in the transport
  bar's chrome color (`secondary`) with a 1px bottom border, `px_3`. Two
  `tool_group`s, each a muted label ("Rotate about", "Handles") and a
  `mode_toggle`. A toggle is a tabs list after shadcn's Tabs: a
  `rounded_md` container in the theme's `background` (darker than the
  row, since the row already has the muted chrome color) with
  `TAB_LIST_INSET` padding, two fixed-width (`TAB_WIDTH`) `ToolTab`s; the
  chosen one an inset `rounded_sm` pill in `secondary` with the `input`
  border, foreground text and a small shadow, the other unfilled with muted
  text that brightens a step while it is hovered. The selection reads by
  shape, the gap around the pill. Both tabs always carry a hover style (the
  chosen one restating its own colors): GPUI tracks a div's hover flag only
  while it has one, and text colors read that flag before the frame's
  hitboxes exist, so a tab without one kept a stale "hovered" from its click
  and showed it bright when it was next unselected. Ids go by value
  (`pivot-center`, …). Then a reserved slot (`NOTE_SLOT_WIDTH`) with a
  `TriangleAlert` icon carrying the reason (`pivot_note` / `resize_note`)
  when the choice is not in effect, so the labels and the width never
  change. The row clips (`min_w_0`, `overflow_hidden`) in a narrow viewer.
  With Center chosen but not in effect, the rotation handle and the Scale
  tool's handles get hover areas with the tooltip "Pivot — reason" (not
  while a drag runs; its label says it). Those areas, the cross's and the
  locked box's tooltips, and the drag label are placed by the canvas's last
  painted bounds. When those bounds change (the row appears or goes), the
  canvas's prepaint schedules one more frame (`window.on_next_frame` that
  notifies the view; a notify during the draw only marks the view dirty
  and waits for another redraw), so they are placed on the new bounds one
  frame later instead of lagging until the next input. The transport bar no
  longer holds the toggles; its breakpoints are its own again
  (`transport_layout`, 820 and 600 px).
- The cross (`transform_handles.rs`, `transform_drag.rs`, `viewer.rs`):
  `marker(frame, pivot)` gives `Marker::Center` (the box's middle) when the
  drag uses Center, else `Marker::Pivot` (`pivot_cross`); the viewer draws
  the same cross at either. `drag_props_in(targets, frame, pivot)` leaves
  the anchors out in Center mode, so the center cross is a marker only
  (tooltip `center_marker_text`: `CENTER_MARKER`, "Rotates about the
  center; switch to Pivot to move the anchor", or, when the anchor couldn't
  move in Pivot mode either, "Rotates about the center; the anchor can't
  move:" and its reason; just "Rotates about the center" without anchor
  rows, `has_anchor_rows`); in Pivot mode the cross Alt-drags the anchor,
  with its reasons. `swallows_alt_press`: an Alt press within reach of a
  cross that moves nothing but shows why (the center marker, a pivot cross
  with a reason) is taken by `begin_transform_drag` and starts nothing, so
  it neither moves the layer (the press would otherwise fall through to the
  body) nor changes the selection; a pivot cross that shows nothing (no
  anchor rows: `CrossDrag::NoAnchor`, no hitbox) lets the press through.

## Decisions

These differ from, or settle what the spec left open:

- **The pivot is the renderer's**: the anchor point plus `origin`, where
  `EvaluatedTransform::translation` puts the turning point. Pivot mode turns
  about it and the cross is drawn there. 2a measured angles and scale ratios
  about `transform.position`, which is the pivot only with a zero `origin`;
  2c fixes that in both modes.
- **One compensation rule** (`keep_fixed`): a fixed point in own pixels
  after the change and in parent space before it; the map built from the new
  props with the old position; `position' = position + (fixed_parent −
  M'(fixed_local'))`. Center rotation and scale keep the box center, a
  resize keeps the opposite edge's point (the pivot's coordinate along an
  axis it does not change), the anchor drag keeps the box's top-left corner.
  Skew and `origin` need no special case.
- **Compensations are computed from the values as written**
  (`as_written`, `format_number` parsed back): an anchor is written rounded
  to 0.01 and `x`/`y` are computed from that, and so are sizes and the
  Center-mode rotation and scale (which are therefore quantized to 0.01 in
  the preview too; Pivot-mode values are unchanged). The reloaded layer
  lands where the preview drew it.
- **Pivot is the default; Center is opt-in.** Decided with the user after
  trying the build on the feature tour (04 TYPE's card, `x={DX}` from an
  imported const and `y` a computed local): in real scenes `x`/`y` are
  often constants or computed, so Center rarely applies, and a default that
  silently falls back reads as broken.
- **Center needs compensable `x`/`y`, nothing animated it depends on, and a
  flat box.** Otherwise the drag turns about the pivot (and the cross sits
  on the pivot), and the toggle keeps Center chosen with a warning whose
  tooltip is the reason:
  `NotCompensable` "x or y can't be edited", `Animated` "Rotation or scale
  is animated" (an editable `rotation`, `scale`, `scaleX`, or `scaleY` is a
  `Keyframe`), `AnimatedBox` "Size or anchor is animated" (a size prop or an
  anchor is a `Keyframe`: the center is a point of the box), `NoFlatBox`
  "No flat box to center on". The stored choice stays Center for the next
  layer. Center uses the outline's box (`local_box`):
  a text's padding is symmetric horizontally, so its center is the layout
  box's there; vertically it differs by the glyph overflow of multi-line
  text. A running drag keeps the mode it started with.
- **The anchor box.** The anchor and the size props are fractions and
  lengths of one box, and both the anchor drag and the Size tool measure
  that box, not the outline: for `Text` the layout box the renderer
  records (the texture quad is padded for strokes, left and right only on a
  single line); for `Image` and `Video` their quad; for `Rect`, `Ellipse`,
  `Triangle`, `Heart` (`width` × `height`) and `Circle`, `Polygon`, `Star`,
  `Pie` (`2 · radius`) from the received props and `anchorX`/`anchorY`
  (default 0), since a shape is drawn as a path and a dashed `Rect` as a
  group; `with_anchor_box` also sets `transform.anchor` to the props'
  anchor there, as a path layer's own anchor places nothing. A `Group` has
  none: the renderer places its children by its position alone, so its
  cross is inert with "A group's anchor has no effect". `Path`, `Line`,
  `Polyline`, `Arrow`, `Instances`, and a primitive with no renderer box
  have no anchor drag.
- **The anchor is not clamped**: an anchor outside the box is valid, and
  the cross follows the pointer by delta (it does not jump to it).
- **The Size tool keeps the opposite edge regardless of Center/Pivot.**
  Resizing about the center (Unity's Alt) is not in 2c; the rotation stem
  and the Scale tool honor the toggle.
- **Hidden-handle rule, generalized**: a Size-tool handle needs
  compensable `x`/`y` only when its kept edge is not at the anchor
  (`resize_moves`), not "left and top" by name: with `anchorX = 1` the left
  edge needs no compensation and stays, and with `anchorX = 0.5` both
  edges need it. A handle that moves the anchor point along an axis whose
  size is a keyframe is hidden too (literal `x`/`y` would keep the edge on
  that keyframe only); the handles that keep the anchor still resize it.
- **Which handles**: `SizeProps::Box` gets four corners and four edges
  (edges one axis, Shift on a corner projects onto the box's diagonal);
  `Radius` corners only, uniform; `Width` (`Text`) the left and right edges,
  whose kept point is the opposite edge at the pivot's own y. The pointer is
  taken through the full inverse of `layer_map` (scale included).
- **Bounds**: sizes clamp to the field's `min`/`max`, whole where
  `integer`, and never below one own pixel (radius 0.5), even where the
  field declares `min: 0` (shapes, `Rect`, `maxWidth`); no drag collapses or
  flips the box.
- **`maxWidth` is written in whole own pixels**: the renderer lays text out
  `ceil(maxWidth)` wide, and the compensation uses the rounded width. A set
  `maxWidth` is exactly the layout box's width; an absent one starts from
  the measured width and is inserted.
- **An absent box side is pinned.** On a `Box` whose other prop is
  `Missing` (an `Image` sized from its source's aspect), an edge drag also
  writes that side at its current box value, overridden from the first
  move, so the runtime's aspect rule does not change the axis not dragged.
  A drag that changes nothing writes nothing, pinned side included.
- **Size-tool handles sit on the anchor box** (`handle_quad`), and a running
  resize's box follows its current values: a group's outline is its
  children's scene-axis box and a text's is padded, so the outline's
  corners are not the box's. The Scale tool and the Move hit test keep the
  outline.
- **Fallback to Scale**: no `table.size` ("No size to resize": `Group`,
  `Video`, and primitives without a table), a locked size row ("Height:
  Written as an expression: h * 2"), a size row that is neither locked nor
  writable ("Size can't be edited here"), no handle left (`Pinned` "x or y
  can't move", or `Animated` with the animated value), or no flat box ("No
  flat box to resize"). The toggle keeps Size chosen with a warning whose
  tooltip is the reason; the stored choice stays Size.
- **No compensation beside an animated value.** Literal `x`/`y` written
  beside a value that changes between keyframes would hold at that keyframe
  only. Every compensating drag checks what it uses: Center (`x`/`y`,
  `rotation`, scales, sizes, anchors), a resize that moves the anchor point
  (`x`/`y`, `rotation`, scales, anchors, and the size along each axis it
  moves the anchor along), the anchor drag (`x`/`y`, anchors, `rotation`,
  scales, sizes). The reasons are shared constants (`TURN_ANIMATED`,
  `SIZE_ANIMATED`, `ANCHOR_ANIMATED`, `BOX_ANIMATED`).
- **The preview overrides every prop a handle can change from the first
  move** (2a's rule), now including `x`/`y` for a Center rotation or scale,
  a compensating resize, and the anchor drag, and the size keys (whose start
  value is the anchor box, so none is dropped).
- **The tools sit in one row docked above the viewer, labelled, with a
  selection only.** Decided with the user over three hand checks: in the
  transport bar the toggles were easy to miss; an overlay strip at the
  viewer's top-left covered the scene and its handles; a colored fill did
  not make the choice readable. The row takes its own height (the canvas
  shrinks), and the choice shows by shape, a shadcn-style tabs list whose
  chosen tab is an inset pill. Labels "Rotate about" and "Handles"; "Rect"
  renamed "Size".
- **The cross marks what a drag turns about.** In Center mode it sits on the
  box's middle as a marker only; the anchor moves only in Pivot mode, where
  the cross is on the pivot it moves.
- **Toggles are session state**, default Pivot and Size, not saved; no
  keyboard shortcuts.
- **Snapping**: rotation keeps 2a's Shift 15°; resize has only Shift-aspect.

## Known gaps

- A rotated group's Center box is approximate: its outline is the
  scene-axis box of its descendants, so `local_box` takes the box around
  that box mapped back.
- A layer's own 3D rotation (`rotationX`/`rotationY` without a projective
  perspective) is measured with the 2D part of `linear()`, so its box is
  the dropped-z box. Layers drawn through the homography have no box and
  fall back (Pivot, Scale, no anchor drag).
- A text's layout box is the rasterizer's whole-pixel box, so a text's
  implied anchor can be off by up to one raster pixel's fraction. A written
  `anchorX` stays exact, as the drag writes the received value plus the
  delta.
- The cross's tooltip shows on any hover, not only while Alt is held (the
  viewer has no window to read the modifiers from when it builds it); with
  Alt held the cursor also turns not-allowed.
- The Alt cursor over the cross updates through `on_modifiers_changed`; when
  the window does not have the focus it may wait for the next mouse move.
- Only keyframed rows are refused. An expression-driven `rotation`, scale,
  size, or anchor (`rotation={frame * 3}`) is `Locked`, not a `Keyframe`,
  so it can't be told apart from a constant: Center, a compensating resize,
  or the anchor drag on such a layer still writes literal `x`/`y` that hold
  on the current frame only.
- Keyframed props without a table row (`originX`/`originY`,
  `skewX`/`skewY`, `rotationX`/`rotationY`) are not seen, though the
  compensation folds in origin and skew: a compensation beside one of them
  holds on the current frame only.
- A box that changes with its content (a `<Text>` whose text or `fontSize`
  animates) is measured on the current frame, so a compensation computed
  from it holds there only.
- An absent side pinned by an edge drag is written at its current size
  without the field's clamp, and shows as unchanged in the drag's label
  (`height 75 → 75`): it is written, so the label is accurate, but it may
  read oddly.
- During a resize the drawn box (and its handles) is rebuilt from the
  drag's values and can trail the rendered layer by one move.
- Test coverage: the one-undo resize test runs at rotation 0, so it does
  not cover a compensated `y`; the skew compensation test is formula-level.
- Code nits: the box fold does not skip NaN; the test prop lists are
  `LazyLock` statics.
- The renderer's fallback for an anchor box that doesn't map (dropped, the
  region kept) has no test: no cheap scene puts only the anchor box behind
  a 3D plane's camera.
- `with_anchor_box` picks the layout by the innermost element's name
  (`primitive_of`), not by a registration.
- `drag_values` takes eight parameters (`#[allow(clippy::too_many_arguments)]`);
  folding `pivot` and `resize` into one tool argument is a follow-up.
- The tool row's tab width (`TAB_WIDTH` 58 px) is estimated from the
  labels, not measured on screen; if a label clips, widen the constant.
- Selecting and deselecting a layer adds and removes the row, so the canvas
  re-letterboxes by the row's height each time.

## What 2d builds on

- `canvas_geometry`'s `layer_map`, `apply`, and `invert_affine` to map a
  text's `TextMetrics` (line starts, cluster edges and advances, in the
  layout box's own pixels) to the scene and back, through rotation, scale,
  skew, and `origin`, for the caret, the selection, and click hit tests.
- `anchor_box(frame)` for a `Text`: its layout box in own pixels
  (`HitRegion.anchor_box` / `LayerFrame.anchor_box` in the renderer), which
  the metrics are laid out in; the outline is padded and is not.
- `DragState` and the viewer's handle drawing, which 2d hides while
  editing.

## Verify by hand

In the editor, with **Select layers** on, on a scene of your own (the
feature tour has no editor-only content these tools supersede, so no
example changed):

- Turn on **Select layers**: no tool row while nothing is selected. Select
  a layer: a row appears at the top of the viewer, above the frame, and the
  frame shrinks to make room (nothing is covered). It holds "Rotate about"
  [Center | Pivot] and "Handles" [Size | Scale] as tabs lists: Pivot and
  Size are the inset, raised pills with a visible gap around them; the
  other tab is unfilled with muted text. The transport bar has no toggles
  and keeps its old layout. Clicking a tab moves the pill there; without
  moving the pointer, the tab it left is muted at once (not bright until the
  next move), and an unselected tab brightens only while it is hovered.
  Narrow the window: the row clips instead of covering the dock, and the
  tooltips over the canvas line up with the cross as soon as the row
  appears, without moving the pointer. Select
  layers one after another, including ones that fall back: the row's
  widths and labels never change, only a warning icon appears in the
  reserved slot after the tabs. Clearing the selection (click empty
  canvas) or turning the mode off removes the row and the frame grows back.
- 04 TYPE in `examples/feature-tour/film.tsx`, the card `<Rect x={DX} y={top}
  … rotation={-9.5} anchorX={0.8} …>`: choose Center. The Center tab
  stays chosen and a warning icon appears beside it, tooltip "x or y can't
  be edited"; the cross stays on the pivot. Hover the rotation handle:
  "Pivot — x or y can't be edited"; drag it: the label starts with the same
  note, and the card turns about the cross. Hover the cross: "X: Comes from
  DX, which can't be traced" (the `x` row's own reason); with Alt held the
  cursor is not-allowed.
- Choose Center on a plain `Rect` with literal `x`/`y`: the cross moves to
  the middle of the box, its tooltip reads "Rotates about the center; switch
  to Pivot to move the anchor", and Alt-dragging it does nothing: the
  cursor is not-allowed, the layer doesn't move, and the selection stays.
  Rotate it: it turns about its middle, the label shows `rotation`, `x`,
  and `y`, and one undo restores it. Switch to Pivot: the cross jumps to
  the top-left corner and the layer turns about it. Alt-drag the cross to
  the middle: nothing moves on screen (the outline, handles, and stem hold
  perfectly still, during the drag and after the release until the reload;
  only the cross follows the pointer), and `anchorX`, `anchorY`, `x`, `y`
  are written in one undo step. Rotate again: it turns about the new
  anchor. With `originX={40}` set, Pivot mode turns about the cross 40 px
  to the right of the anchor.
- Hold Alt over the pivot cross of that `Rect` in Pivot mode: the cursor
  becomes an open hand.
- Choose Center on a `Text` with literal `x`/`y` and `anchorY="baseline"`:
  the cross sits on the middle of its layout box and its tooltip reads
  "Rotates about the center; the anchor can't move: Anchor Y: Written as an
  expression: "baseline"". On a `Path` (no anchor props) in Center mode it
  reads only "Rotates about the center"; in Pivot mode a `Path`'s cross has
  no tooltip, and Alt-pressing it presses the body as before.
- Resize a rotated `Rect` from its left edge: `width`, `x`, `y` change and
  the right edge stays on screen; from its right edge only `width` changes.
  Shift on a corner keeps the aspect. Drag an edge through the opposite
  one: the box stops at 1 px and does not flip.
- Resize a `Circle` (or `Star`) from a corner: corner handles only,
  `radius` changes, the opposite corner stays.
- Drag a `Text`'s right edge: `maxWidth` is written in whole pixels,
  starting from `maxWidth` when set and from the longest line's width when
  absent (then inserted), and the text wraps; the left
  edge keeps the right one. Its handles sit on the text, not on the padded
  outline of a stroked text.
- Select a dashed `Rect` and rotate it 45°: the Size-tool handles sit on its
  own corners, not on the scene-axis box; drag a corner outward and both
  sides grow.
- Select an `Image` with only `width` written: an edge drag writes `height`
  too, at its current value, and the other side does not change.
- Rotate a layer whose `x` is set at a component call, in Center mode: the
  rotation is written on the element and `x` at the call (two files; `y`
  goes to the call too when the element reads it from a component parameter
  (inserted there if the call leaves it out), else to the element), and one
  undo restores both.
- On a keyframe frame, drag the right edge of a `Rect` with
  `width={interpolate(frame, [0, 30], [100, 200])}`: only the matching
  output entry changes.
- Select a layer with keyframed `x`, Center chosen: the warning reads "x or
  y can't be edited"; the left and top edges (and the corners touching them)
  are hidden while the right and bottom ones resize; the cross's tooltip
  reads "X is animated". Select a plain layer: the warning goes away.
- Select a stroked `Text` with the Size tool: the selection outline is drawn
  on its layout box, through the handles; with Scale it is the padded quad
  again. A `Rect` anchored at its middle with keyframed `x`: Size stays
  chosen with the warning "x or y can't move", and the scale handles show.
- Select a layer with keyframed `rotation`, Center chosen: the warning reads
  "Rotation or scale is animated", and the rotation handle turns it about
  the pivot (the cross), writing `rotation` only; its left and top resize
  handles are hidden too (anchored top-left). On a `Rect` with keyframed
  `width` anchored at its top-left, Center's warning reads "Size or anchor
  is animated"; the left edge and the left corners are hidden, the right
  edge, the top and bottom edges, and the right corners still resize, and
  the cross's tooltip reads
  "Size is animated". With keyframed `anchorX`: Center's warning reads
  "Size or anchor is animated" and the cross's "Anchor is animated".
- Select a `Group`: Size stays chosen with the warning "No size to
  resize", and the 2a scale handles show; hover over the cross: "A group's
  anchor has no effect", and Alt-drag does nothing. A `Rect` with
  `height={h * 2}` reads "Height: Written as an expression: h * 2".
- Select a `Text` with `anchorY="baseline"`: the cross is drawn, Alt-drag
  does nothing, and its tooltip reads `Anchor Y: Written as an expression:
  "baseline"`.
- Alt-drag the cross of a stroked `Text` to the middle of its text:
  `anchorX` lands near 0.5 of the text's width (not of the padded quad),
  and the text does not move. Alt-drag the cross of a stroked `Ellipse` or
  `Circle`: the shape stays put, and after the reload it is exactly where
  the preview drew it.
- On a layer turned in 3D in its own perspective (`rotationY={30}
  perspective={800}`), Center chosen: the warning reads "No flat box to
  center on"; the Handles warning reads "No flat box to resize"; the
  cross's tooltip reads "No box to anchor in".
- Undo each of the above: each is one step and restores the files it wrote
  byte for byte.
