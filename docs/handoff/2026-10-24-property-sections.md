# Properties sections (2026-10-24)

The Properties tab edits every prop a Celesta primitive declares, not only
its transform: one section per group of the primitive's property table
(Transform, then Size and Appearance for a `Rect`, Text and Layout for a
`Text`, Source and Size for an `Image`, …), a row per prop, and a control
per type (number, string, multi-line text, switch, menu, color, file). A
number row scrubs from its label, previewed through the runtime like a
canvas drag. A value computed from others lists its inputs as editable
sub-rows, a component call's own props get a "Component" section, and
`Text`'s font rows edit `style` when `style` sets them.
Design: `docs/superpowers/specs/2026-10-23-editor-property-editing-design.md`
(sections "Property tables", "Properties tab", "Inputs", and the `Text`
shorthand paragraph). This is part 2b; it builds on 2a
(`docs/handoff/2026-10-23-property-editing-engine.md`), and 2c–2d build the
canvas tools and inline text editing on it.
Plan: `docs/superpowers/plans/2026-10-24-editor-property-sections.md`.
User docs: `packages/website/content/{en,ja}/docs/01-(getting-started)/03-preview.md`
("Edit properties", and the lock reasons under "Move layers on the canvas").

## What changed

- Property tables (`packages/react/src/property-tables.ts`,
  `components.ts`, `internal.ts`; `packages/shapes/src/index.ts`;
  `packages/cli/src/cli.ts`): `propertyTable<XProps>({ size, groups })`
  declares, per primitive, its groups of `PrimitivePropertyField`s (`num`,
  `str`, `text`, `bool`, `color`, `select`, `path`) and the props that hold
  its box (`size`). Each table is typed with that primitive's own props, so
  a renamed prop fails `tsc -b`, and so does a prop the props type requires
  that is not wrapped in `required(…)` (or an optional one that is). `@celesta/react` registers `Group`,
  `Image`, `Text`, `Rect`, `Video` and `@celesta/shapes` registers `Ellipse`,
  `Circle`, `Triangle`, `Polygon`, `Star`, `Pie`, `Heart`, beside their
  `markPrimitive` loops. `listPrimitiveProperties()` turns them into ordered
  arrays (`WirePropertyTable`), and the ready message always carries them as
  `primitiveProperties`. `Path`, `Line`, `Polyline`, `Arrow`, and `Mask`
  have no table.
- Tables through the bridge (`crates/react-bridge/src/types.rs`,
  `protocol.rs`, `bridge.rs`; `crates/editor/src/preview.rs`, `source.rs`,
  `refresh.rs`, `view.rs`): `PrimitivePropertyTable { size, groups:
  Vec<PrimitivePropertyGroup { name, fields: Vec<PrimitivePropertyRow> }> }`
  keeps the declared order end to end; only the outer map
  (`ReactCompositionMetadata.primitive_properties`) is keyed by name. A
  row's `required` is sent inside its `field`, as the table declares it, and
  lifted onto `PrimitivePropertyRow.required` (the field enum stays as it
  was).
  `apply_property_outcome` replaces the tables on a successful reload and
  keeps them on a failed one, as it does the project schema;
  `EditorView::primitive_table(name)` reads them.
- Source queries (`crates/source-edit/src/attributes.rs`, `resolve.rs`):
  `text_style_path` says which path a `Text` shorthand row edits
  (`style.<key>` or the shorthand) or locks it `StyleMaySet` ("May be set by
  `style`"); `written_attributes` lists a call's attribute names in source
  order without spreads, `key`, `ref`, and namespaced names;
  `resolve_call_site_prop` resolves a prop written at a component's call
  without comparing it to a received value (the runtime reports none for a
  component's own props).
- Removal (`crates/source-edit/src/remove.rs`, `apply.rs`): `removal(text,
  element, prop)` returns an `Edit::Remove { start, end }` that deletes an
  attribute with the whitespace before it, or a `style` object's property
  with the comma that separates it (the previous one's for the last; the
  only property leaves `{}`). A pass-through is removed at its call site by
  the parameter's name. `None` for `children`, an absent prop, a prop
  written twice, one a spread after it may set, or a `style` that is not an
  object literal.
- Inputs at the call site (`crates/source-edit/src/inputs.rs`,
  `pass_through.rs`, `scope.rs`): `resolve_inputs` takes the call site and
  resolves an input that is a parameter of the enclosing component as a
  `PassThrough`, with 2a's parameter rules in one place
  (`resolve_parameter`). The expression the inputs are read from is the span
  `resolve` locked on, recorded where it locks (`Gather::lock_expression`)
  instead of re-derived (`locked_span` is gone). Names bound inside the
  expression hide reads only in their own function body, reads of one name
  inside the expression count as one place for `SharedBinding`, and only a
  callee or tag that is a name or a static chain of names (`f`,
  `Math.max`) is skipped (`fns[i](x)` lists `fns` and `i`).
- Element targets keyed by prop (`crates/editor/src/element_edit.rs`, was
  `transform_edit.rs`): `PropKey` is `Prop(table key)`, `Input { row, name
  }`, or `CallSite(name)`. A `Target` holds the file, the resolved `path`
  (`fontSize`, `style.fontSize`, `children`), the `EditTarget`, the call
  site, and, on an `Expression`-locked row, its inputs. `resolve_targets`
  resolves every table row in order (`scaleX`/`scaleY` in `scale`'s place
  when either is written; the transform props alone without a table), locks
  a literal of another kind than its field as `Expression`, lists inputs,
  and fills `ElementTargets.component` (`ComponentTargets`) when the owner
  call site is found. `commit_files` writes typed `EditValue`s for any key,
  or removes the prop for a `None` (`removal_of`: the element's file, the
  call site's for a pass-through, `ComponentTargets.call` for a component
  prop), and re-resolves rows, inputs, and component props against every
  file it read. `TransformProp::received` and `TransformProp::label` are gone
  (`received_number` reads the received props, `row_label` names the rows);
  `ElementEdit` (was `TransformEdit`) records written values of any type, or
  a removal; `commit_element` (was `commit_transform`) takes `(PropKey,
  Option<EditValue>)` pairs.
- Component section (`element_edit.rs`): the attributes written at the
  owner call site, minus `children` and any `param` a row or input already
  passes through to, typed from their literal (number, string, boolean) or,
  when `registerComponent()` registered the tag, from its schema
  (`type_component_props`, `apply_component_schema`), which locks a value of
  another kind and gives a number its range.
- Shared controls (`crates/editor/src/property_controls.rs`):
  `ControlKind`, `ControlEvent`, `Control`, `build_control`, and
  `render_control` build and draw the text, multi-line text, switch, select,
  color, and path controls for both the project Properties panel
  (`property_panel.rs`, `property_panel_view.rs`) and the element rows, with
  the number and color helpers both use (`parse_number_input`,
  `parse_color_input`, `parse_color`, `color_hex`, `number_hint`,
  `take_picked_color`). The project panel keeps its own `Value` commit flow;
  its control kind comes from `element_rows::control_kind(&schema_field(…))`.
- Rows (`crates/editor/src/element_rows.rs`, `element_rows_view.rs`,
  `layer_panel.rs`; `transform_rows.rs` is gone): one `inspector_section`
  per table group, a row per target with its control, the keyframe and lock
  icons, "Set at `<file>`:`<line>`", the lock reason or keyframe jumps, and
  the row's message; inputs indented under their row; then "Component ·
  `<Tag>`" with the call's "Set at" line and its rows. `row_text` picks the
  value or the dimmed placeholder, `parse_row_value` the typed value or the
  message. A typed field commits on Enter or blur; a switch, a menu choice, a
  picked color (when the picker closes), and "Choose…" commit at once; each
  commit is one `commit_element` call and one history entry. The label
  column (`LABEL_WIDTH`) is 84 px, shared with the header rows. A row whose
  prop `removable` says can go removes it when its field is emptied and
  committed (`typed_text`), or with its hover × (`remove_element_row`);
  `written_text` shows the default until the reload resolves. A commit made
  while the targets are not current waits in `HeldCommits` and
  `drain_held_rows` writes it once they are.
- Label scrub (`element_rows.rs`, `element_rows_view.rs`,
  `transform_drag.rs`): dragging an editable number row's label changes it
  by `scrub_value` (the field's `step` per pixel, else 0.01 for a field
  with `max` ≤ 1, else 1; Shift ×10; clamped to `min`/`max`), sends
  each frame with a prop override at the row's write position, and writes
  once on release. `override_sites` returns a site per editable `PropKey`,
  at the row's resolved path, so drags and scrubs share it.
- `style.<key>` overrides (`packages/react/src/jsx-dev-runtime.ts`): an
  override whose prop is `style.fontSize` sets that key in a copy of the
  element's `style` (a new object when none is written), at most one copy per
  element, and never mutates the written props.
- One temp writer (`crates/react-bridge/src/props_edit.rs`,
  `crates/editor/src/edit_history.rs`): `celesta_react_bridge::write_temp`
  writes `.<name>.tmp-<pid>` beside the file; `replace_file` and the
  multi-file history both use it.

## Decisions

These differ from, or settle what the spec left open:

- **Table fields are their own type.** `PrimitivePropertyField` has
  `ComponentPropertyField`'s types plus `text`, no `label` (the row label is
  the key in words: `cornerRadius` → "Corner radius", `children` → "Text",
  `src` → "Source"), and an optional default.
- **`Text` shorthands map to `style.fontFamily`, `style.fontSize`,
  `style.fontWeight`, and `style.fill`** (`color` fills `fill`). The row
  edits the `style` property when the object's own properties set it, and the
  shorthand otherwise, inserting `fontSize={48}` beside a `style` that does
  not set it. It locks `StyleMaySet` when `style` is not an object literal,
  when a spread in the object may set the key (`{ fontSize: 48, ...base }`;
  `{ ...base, fontSize: 48 }` edits the object), and when the element has a
  spread with no `style` written after it (`rest.style` may win).
- **No `style`-only rows.** The `Text` table carries only props with a
  shorthand, so 2a's `NoStyleObject` lock never shows here.
- **Colors** accept `#RRGGBB`, `#RRGGBBAA`, or a theme color name
  (`^[A-Za-z][A-Za-z0-9_-]*$`) written as typed; the picker shows no color
  for a name.
- **A literal of another kind than its field locks as `Expression`** with
  its source (`anchorY="baseline"` on the Anchor Y row), for table rows and
  for schema-typed Component props alike.
- **Placeholder of an absent prop:** the source's default (a parameter's),
  else the received value, else the table's default. A scale axis written
  on the element shows the received `scale`; a pass-through to a `Missing`
  with a default shows that default.
- **Number ranges:** a typed value outside `min`/`max` is refused ("Enter a
  number from 0 to 1"; "of at least {min}" or "of at most {max}" when only
  one bound is set); a scrub clamps.
- **Impact scoping and "Can't move" stay transform-only.** `written_at` and
  `locked_summary` look at `x`, `y`, `rotation`, `scale`, `scaleX`, and
  `scaleY`, so the impact line is the one 2a shipped; editing `fill` written
  on the element changes every instance even when the line is scoped to one
  call.
- **No table** (a primitive without one): the Transform section from the
  transform props, as 2a.
- **Path rows** write `import_path(entry_dir, entry_dir, chosen)`; "None"
  writes `""` over a written path and nothing for a `Missing` one.
- **Scrubs** preview like a drag, write once on release, and do nothing on
  Escape or a release without movement, or with the value back where it
  started. Input rows, locked rows, and keyframe rows do not scrub;
  Component number rows do, overriding the attribute at the call site.
- **Scrub steps are declared by the fields.** A number field may declare
  `step` (`num(1, { step: 0.01 })`; a component schema's `step` for a
  Component row): the tables declare 0.01 for `scale`, `anchorX`, and
  `anchorY` in every Transform group and for `Video`'s `playbackRate` and
  `volume`, unbounded ratios and fractions where 1 per pixel is unusable.
  Without a step, a field with `max` ≤ 1 steps 0.01 and any other 1. The
  transform rows without a table give `scale`, `scaleX`, and `scaleY` a
  field with step 0.01. `Text`'s `fontWeight` is `{ min: 1, max: 1000,
  step: 100 }`: the renderer picks the nearest static face by its OS/2
  weight class (there is no `wght` axis), so 1-unit steps change nothing.
- **Number bounds follow the components.** `min: 0` where a negative value
  is rejected or draws nothing more than 0 (`Rect` width/height/
  cornerRadius/strokeWidth, `Text` fontSize/maxWidth, `Video`
  startFrom/playbackRate/volume, the shapes' sizes, radii, and corner
  radii per `sizeOf`/`dimensionOf`); `min: 3, integer: true` for `sides`
  and `points` (`countOf` rejects fractions). `integer` is its own option
  on number fields (`integer: Option<bool>` in Rust), never implied by a
  step of 1: a typed fraction gets "Enter a whole number of at least 3"
  (and the matching range forms), and a scrub rounds, its start too.
  Props that must be positive get the smallest sensible
  inclusive bound, since a field's bounds are inclusive: `Image` width/
  height `min: 1`, the shapes' `strokeWidth` `min: 0.1` (`strokeWidthOf`).
  Positions, angles, scales, and anchors stay unbounded; `max` only where a
  component enforces one (`opacity`, `apex`, `innerRatio`).
- **Where a scrub starts, and the focus.** A scrub on a row with no value
  starts from the placeholder the row shows (the source's default, else the
  received value, else the table's default), and from 0 only when there is
  none. A scrub and a canvas drag end each other without writing, and
  neither starts while a released one is still being written. Pressing a
  label moves the keyboard focus to the editor, so `⌘ Z` right after a scrub
  undoes it (a row's text field takes `⌘ Z` as its own undo), unless another
  row holds typed text: that field keeps the focus and its text, neither
  lost nor submitted (`scrub_takes_focus`).
- **Component section:** the attributes at the owner call site in source
  order, without `key`, `ref`, spreads, `children`, and the props a row
  above shows with "Set at"; locked rows for anything but a literal. It is
  hidden when nothing is left. Its rows show no received value or table
  default as a placeholder. A row is labeled with the schema field's
  `label` (`component_row_label`), else the prop's name as written: an
  author's identifier is not put into words as a table key is. A React
  composition's schemas come with its loads
  (`ReactPreview.component_schemas`, kept across a failed reload, as the
  tables are); a JSON project's from the entry query
  (`EditorView.component_schemas`). Before, a composition opened directly
  typed its Component rows from their literals only, so no label, range,
  or `integer` applied.
- **`integer` on component number fields.** `registerComponent()` and
  `defineProjectProperties()` number fields take `integer: true`
  (`ComponentPropertyField::Number.integer`), which `schema_field` passes
  on, so a Component row refuses a fraction and its scrub rounds. The
  project Properties panel parses typed numbers with the same
  `parse_field_number`, so it now keeps to a field's `min`/`max` too. The
  runtime does not check `integer` against `--props` values, as it does
  not check `min`/`max`.
- **The call-site file is read whenever the owner call site is found**, for
  the Component section, so it is in `files`: a commit of any row is refused
  with "The source changed while editing" when the call's props moved
  meanwhile.
- **Inputs are not checked against received values.** The runtime reports
  the expression's result, not its inputs, so an input whose value is
  `interpolate(frame, …)` locks `FrameUncertain`.
- **Self-closing `<Text />` stays locked.** `TextProps.children` is
  required, so `<Text />` does not type-check; the 2a handoff's "2b opens
  the tag" is withdrawn.
- **The multi-line text control is `Control::Textarea`** (gpui-base's
  `InputState` is single-line only): Enter submits, Shift+Enter breaks the
  line, it grows from one to six lines.
- **Each row parses on its own**, about twenty parses per selection on the
  background executor; parses are not cached.
- **Removing a prop, both ways.** Emptying a field and committing (Enter or
  blur) removes the prop, and so does the hover × at the row's end (lucide
  `X`, ghost, xsmall, tooltip "Remove"), for every control kind. Removable
  means written and optional: a `Literal` on the element (also one read
  through a `const`, whose attribute goes), in its `style` object, or at the
  call through a pass-through. Not removable: a prop declared `required`, an
  input (a value an expression reads, not a prop), a `Text`'s children,
  and an absent, locked, or keyframed prop. A Component-section prop is
  removable unless the component's `registerComponent()` schema declares
  it `required`, and so is a pass-through removed at the call; without a
  schema, any is (`docs/handoff/2026-10-26-required-component-props.md`). The × is hidden on a row that can't
  remove, during a scrub, and while its removal is being written; it is a
  hover shortcut for the empty-and-commit path, which the keyboard reaches.
- **An emptied field of a row that can't remove its prop writes nothing**
  (on blur the source's value shows again), for every kind. Before, an
  emptied string, text, or path field wrote `""`; an empty string is no
  longer written by emptying (the path row's "None" still writes `""`).
- **A removed row shows its default** (the table's, else a transform prop's
  own; nothing when unknown) dimmed until the reload resolves, not the
  stale received value.
- **A prop written twice** (`x={1} x={2}`) is not removable: deleting one
  would let the other apply. The commit says "X can't be removed here".
- **A removal never crosses a line break.** The attribute alone on its line
  takes the line; first on its line, the spaces after it go (what follows
  keeps the indentation); otherwise the spaces before it. Trimming back
  across the line break joined the next text to a `//` comment. As a safety
  net, every text a commit would write is parsed again, and one with syntax
  errors is refused: "Couldn't edit the source: the result does not parse".
- **Fields stay enabled while their own write reloads.** Only a target that
  can't be written disables its control; scrubbing still needs current
  targets. A commit (typed, switch, menu, color, file, removal, a released
  scrub) made while the targets are being read again, or while another write
  runs, waits in `HeldCommits` for that layer and is written once they are
  current: one at a time (each its own history entry), in the order made,
  the latest value per row winning. While it waits, its row says
  "Saving…" (muted, in the message line under the row; a typed value's
  error takes the line). A write that starts at once shows nothing: it
  lasts milliseconds, and a line for it would make the rows jump. It
  fails at once only when the source can't be read, or the code failed to
  reload. A row's sync never sets text
  equal to what its field holds, so the caret stays.
- **The playhead's keys rest while a text field has the focus.** The
  arrows, Shift+arrows, Home, and End moved the playhead while typing in a
  field, though the keymap's `EDITOR_KEYS` excludes `Input`. One path is
  known: gpui-base's input passes on Left at the start of its text and
  Right at the end, and macOS offers an unhandled key to the menu, whose
  Previous Frame and Next Frame items take those keys. `escapable`, which
  wraps every text field and the multi-line text, keeps a plain Left or
  Right (`field_keeps_key`). Up and Down leaked by a path not found in
  the source, so the actions themselves (`PreviousFrame`, `NextFrame`,
  `JumpBackward`, `JumpForward`, `PreviousEditPoint`, `NextEditPoint`,
  `GoToStart`, `GoToEnd`) do nothing while the focus's key contexts
  include `Input` (`in_text_field`). The trade-off: while a field has the
  focus, those menu items do nothing either; clicking outside the field
  brings them back.
- **Leaving a field commits it** (as Unity's inspector does). Before the
  selection changes by a canvas click, the layer list, clearing it, or
  leaving the interactive mode (`select_layer`), and after a seek by hand
  or the start of playback (`submit_requested`, done on the next render,
  since those paths have no context), the rows' typed text is submitted as
  a blur would; text that does not parse is dropped with its message.
- **A selection change waits for the held commits of the selected layer**
  (`SelectionGate`): it is made once they are written, and a newer request
  replaces the waiting one (clicking the selected layer cancels it).
  `HeldCommits` entries carry their layer and the timeline frame their
  targets were looked up at. They are dropped when the code fails to reload
  ("Not saved: the code failed to reload" on each row, or the Layer section
  for a layer no longer shown; later commits fail with it until the next
  reload), when the preview worker stops ("Not saved: the preview stopped
  unexpectedly"; no lookup answers again, so later commits fail with it
  for the rest of the session, `ElementEdit::preview_stopped`), when the
  layer's lookup answers without a source ("Not saved: the layer's source
  was not found again"), and when the selection clears without waiting
  (the reloaded code lacks the layer). A held commit to a keyframe made
  before a seek is not written to another keyframe ("Not saved: the
  playhead moved off this keyframe"); a plain value is.
- **Leaving the interactive mode waits with the selection change**
  (`InteractiveToggle::LeaveAfterCommits`): the mode, and its source
  lookups, stay until the held commits are written or dropped, so they can
  drain; meanwhile the toggle stays selected with a spinner for its icon
  ("Turning off Select layers once saved…") and stays clickable. Toggling
  again, or picking a layer, cancels the leave and keeps the selection.
  The leave asks for the frame and the sweep without the mode once: a
  reload handler that set it off asks for both itself, and
  `settle_interactive_mode` (each render, once no reload runs) otherwise.

2a carry-overs closed: inputs that are parameters resolve at the call site;
the lambda, one-place, and callee gaps in `resolve_inputs`; the scale-axis
placeholder of a parameter default (`({ sx = 3 }) => <Rect scale={2}
scaleX={sx} />` shows 3); the duplicated readers (`TransformProp::received`,
`locked_span`); and the temp writer `edit_history.rs` copied.

## Known gaps

- A row that passes through to an expression at the call site
  (`<Tag x={a + b} />`) lists no inputs: inputs are read from the element's
  own expression.
- A change to the registered component schemas alone re-types the Component
  section only on the next resolution (the next selection or reload).
- The commit's stale check replays the resolved rows' keys. Replacing
  `scale` with `scaleX` is caught (the `scale` row goes from a literal to
  missing, which `same_place` rejects); adding `scaleX` beside an unchanged
  `scale` is not, when no span the rows resolved moves: the commit writes
  the uniform `scale` it resolved.
- The multi-line control is about 12 px taller than the small single-line
  rows (gpui-component 0.6.6's `Textarea` has no size API); its label is
  top-aligned.
- 2a's `useMemo` limitation applies to scrubs as to drags: an element cached
  with `useMemo` at an override's position follows the first frame only.
- A selection change that waits for a held commit shows nothing beyond
  that row's "Saving…"; it normally lasts only until the editor's own
  write reloads.
- A seek or the start of playback submits typed text on the next render,
  after the playhead moved; a commit made then still writes against the
  targets read before the seek (the new frame's lookup has not answered).
- A required prop that passes through to a parameter with a default
  (`({ width = 100 }) => <Rect width={width} … />` called as
  `<Card width={240} />`) is not removable at the call site, though the
  parameter's default would apply there.
- `element_rows.rs` is still large (about 1.5k lines); splitting its
  commit flow from its pure helpers is a follow-up. `element_edit.rs` keeps
  the targets' types, the `ElementEdit` state, and its sync, with the
  resolution in `element_edit/resolve.rs` and the commit in
  `element_edit/commit.rs`; the element tests are in
  `src/tests/element_edit.rs` and `src/tests/element_rows.rs` (the editor is
  a binary crate, so a top-level `tests/` directory can't reach them).

## What 2c–2d build on

- 2c (Pivot, Resize): the tables' `size` (`width`/`height`, `radius`, or
  `maxWidth` for `Text`) names the props the resize handles change;
  `PropKey` and `commit_element` write any of them with the transform in
  one history entry; `override_sites` previews any `PropKey`, so a resize
  drag can override `width` as a move overrides `x`.
- 2d (inline text editing): the `children` row's target and its `text`
  field, `Control::Textarea`'s Enter / Shift+Enter handling as the model for
  the canvas editor, and the shared controls for anything 2d adds to the
  tab.

## Verify by hand

In the editor, on a scene of your own (the feature tour has no editor-only
content these rows supersede, so no example changed):

- Select a `Rect`: Transform, Size, and Appearance sections show. Pick a new
  fill with the picker; the file gets `fill="#…"` when the picker closes.
  Drag the Corner radius label: the preview follows before release, Shift
  moves faster, and one write happens on release. Escape mid-drag writes
  nothing.
- Type into a field without pressing Enter, then drag another number row's
  label: the field keeps the focus and its text, and the ↔ cursor stays for
  the whole scrub, even when the pointer leaves the label.
- Commit a field with Enter, then drag a number row's label and press `⌘ Z`
  right after releasing: the scrub's write is undone without clicking the
  preview first. Drag the Anchor X label: 10 px moves it by 0.1.
- Select a `Text`: in the Text row, Shift+Enter breaks the line without
  committing; Enter commits. With `style={{ fontSize: 40 }}` on it, change Font size:
  the object's `fontSize` changes, not a new attribute.
- Select an element whose `width` is computed
  (`width={textWidth + padding * 2}`): the row is locked and lists
  `textWidth` and `padding`; edit `padding` (at the call site when it is a
  prop of the component, with its "Set at" line).
- Select a layer inside a component called as
  `<PriceTag label="Sale" />`: the "Component · PriceTag" section lists
  `label`; edit it, and only the call site changes.
- Select a `Text` with `maxWidth={400}`: hover the Max width row and click
  ×; the attribute goes with its space, and the row shows no value. Select a
  `Rect` with `cornerRadius={8}`: empty the Corner radius field and press
  Enter; the attribute goes and the row shows `0` dimmed. Width shows no ×,
  and emptying it puts its value back. Undo each removal: the file is
  restored byte for byte.
- Type in a field and press Enter, then keep typing at once: the field
  stays enabled, the caret stays where it was, and Enter again writes the
  new value after the first write's reload (two history entries).
- Type a value in a field without pressing Enter and click an empty part of
  the canvas: the value is written. Scrub a number label, release, and at
  once click another layer: the scrubbed value is written, and the other
  layer shows selected after the write.
- With `<Rect\n  // background\n  fill="#fff" stroke="#000"\n/>` and with
  `x={1} // left` on the line before `cornerRadius={8} />`, remove Fill and
  Corner radius: the comment stays on its line, `stroke` stays, and the
  file still parses.
- Scrub Font weight: it moves by 100 and stops at 1 and 1000. Scrub Corner
  radius below 0: it stops at 0; type `-4`: "Enter a number of at least 0".
- Select a layer inside a component registered with
  `registerComponent("PriceTag", …, { label: { type: "string", label:
  "Caption", defaultValue: "" } })`: the Component row reads "Caption";
  a prop the schema leaves out shows its name as written. With
  `count: { type: "number", defaultValue: 3, integer: true }`, typing
  `2.5` says "Enter a whole number".
- Type a value in a field and press Enter, then at once type another and
  press Enter: the row says "Saving…" (muted) until the first write's
  reload ends and the second value is written.
- Commit a value and turn **Select layers** off before the reload ends:
  the button keeps its selected look with a spinner for its icon, the
  layer stays selected, and the mode turns off once the value is written.
  Repeat, and click the button again while it spins: the mode and the
  selection stay.
- Undo each of the above: each is one step and restores the file it wrote
  byte for byte.
