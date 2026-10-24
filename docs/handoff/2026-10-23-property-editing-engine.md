# Property editing engine (2026-10-23)

The source editor reads and writes typed values (numbers, strings,
booleans), text children, and `style` object properties, follows a prop to
the call of the component the element is written in, lists an expression's
editable inputs, and writes one edit across several files as one undo step.
The transform write-back uses all of it: `x`, `y`, `rotation`, and `scale`
set at a component's call site (`<PriceTag x={40} />`) can be dragged and
typed, the Transform row says where the value is set, and, when every
editable transform prop is set at that one call, the impact line counts only
what that call changes.
Design: `docs/superpowers/specs/2026-10-23-editor-property-editing-design.md`
(part 2 of `docs/superpowers/specs/2026-10-23-editor-element-editing-overview-design.md`).
This is part 2a; 2b–2d build the Properties tab, the canvas tools, and inline
text editing on it.
Plan: `docs/superpowers/plans/2026-10-23-editor-property-editing-engine.md`.
User docs: `packages/website/content/{en,ja}/docs/01-(getting-started)/03-preview.md`
("Move layers on the canvas", "See what an edit changes", "Undo").

## What changed

- Typed values (`crates/source-edit/src/resolve.rs`, `apply.rs`):
  `EditTarget::Literal` holds an `EditValue` (`Number`, `String`, `Bool`) and
  the `LiteralForm` it was written in (`Number`, `Quoted`, `ContainerQuoted`,
  `Template`, `BareTrue`, `Bool`, `JsxText`, `ChildContainer`), so a write
  keeps the quote style. `Missing` carries an `Option<EditValue>` default
  (`default_for`). `write_value` writes a value in a form, switching to
  `{"…"}` with JSON escapes when the form can't hold it (an attribute string
  with its own quote or a newline). A `const` string or boolean resolves as
  numbers do, with the same `SharedBinding` lock, and every type is compared
  with the received value (`OverriddenByParent`).
- Text children and `style` properties (`resolve.rs`, `shape.rs`,
  `locate.rs`): the prop `children` resolves one JSXText child (whitespace
  collapsed as React renders it), one `{'…'}` child, or one `{name}` child;
  an element with an opening and a closing tag but no children gets
  `Missing` at the end of its opening tag, and a self-closing one is locked
  for now (2b opens the tag).
  `style.<key>` resolves a property of a `style={{ … }}` object literal, and
  a missing one is inserted into the object. Value shapes live in
  `shape.rs`.
- Pass-through (`crates/source-edit/src/pass_through.rs`, with the
  enclosing functions and the names they bind in `scope.rs`):
  `enclosing_component` names the function that encloses an element
  (`function`, `const` arrow, through `memo`/`forwardRef`), `parameter_of`
  finds the destructured parameter (renamed or `props.<name>`) a prop reads,
  and `resolve_through` resolves the prop at the call site as
  `PassThrough { call_site, param, target }`: a literal there, or `Missing`
  with the parameter's default. `ResolveContext::component_props` lets the
  call site, a component element, be read on purpose. A prop of the
  enclosing component without a call site is `CallSiteNotFound`; a call site
  that passes on its own parameter is `ParameterOfCaller`.
- Inputs (`crates/source-edit/src/inputs.rs`): `resolve_inputs` lists the
  identifiers of a prop written as an expression, each resolved like a prop
  value, one level deep. Nothing displays them yet.
- Received props (`packages/react/src/source-chain.ts`,
  `crates/react-bridge/src/types.rs`): the `layerSource` report carries
  every finite number, string, and boolean prop of the innermost primitive,
  `children` included, and each scalar of its `style` object as
  `style.<key>`. `ReceivedProps` keeps the six transform numbers as fields
  and the rest in `others`; `ReceivedProps::get` reads either as a
  `ReceivedValue`.
- Multi-file history (`crates/editor/src/edit_history.rs`): a
  `HistoryEntry` holds a `FileEdit` per file one edit wrote.
  `write_all_recorded` reads every file, lets the edit compute all new texts
  together, writes each changed file to a temp file, and renames them only
  when every temp write succeeded. Undo and redo check every file and refuse
  the whole entry when any one changed outside Celesta.
- Transform via the call site (`crates/editor/src/transform_edit.rs`,
  `layer_select.rs`, `transform_drag.rs`): `owner_call_site` finds the chain
  entry that calls the enclosing component, and `resolve_targets` reads its
  file only when a prop is a parameter. Each `Target` holds the file it is
  written in and, for a pass-through, the `call_site`; `TransformTargets`
  lists every file read, and `commit_files` re-resolves against all of them
  and returns the new text of each file it writes. A drag that moves a
  call-site `x` and rotates an element-side `rotation` writes both files as
  one history entry.
- Drags preview through the runtime (`packages/react/src/jsx-dev-runtime.ts`,
  `render.ts`, `packages/cli/src/cli.ts`, `crates/react-bridge/src/bridge.rs`,
  `crates/editor/src/transform_drag.rs`, `preview.rs`): while a layer is
  dragged, the preview asks for its frame with the drag's values as
  `PropOverride`s (`ReactBridge::scene_at_with_overrides`, `propOverrides`
  in the frame request) instead of patching the layer's transform after
  evaluation (`apply_override` and `override_transform` are gone, and so is
  the motion-blur sample the patch missed). A value overrides the prop its
  write changes: the element's own, or the parameter's prop at the call
  site for a pass-through, so the other layers made from the same value
  move with it. `override_sites` takes the positions when the drag starts,
  since the targets are gone while a released drag writes. `jsxDEV` creates
  matching elements with the value and subscribes their creator to
  `PropOverridesContext`, so it renders again on the next drag frame even
  as a `memo` component; nothing else does. The entry remounts when the
  overridden positions change.
- Rows and impact (`transform_rows.rs`, `impact.rs`, `layer_panel.rs`,
  `icons.rs`): a row whose value is set at a call site shows the state of
  the value there (field, keyframe or lock icon, reason) and, under the
  field, a `CornerDownRight` arrow with "Set at `<file>`:`<line>`", the file
  relative to the entry's directory. `TransformTargets::written_at` is the
  call site when every editable target passes through to that one call;
  `selection_impact` passes it to `impact_view`, which then counts, lists,
  and bands only the instances whose chain contains it, so the header, the
  impact line, and the timeline bands agree.

## Decisions

These differ from, or settle what the plan left open:

- **Escaping in place inside `{'…'}` and `` {`…`} ``.** A container string
  keeps its quotes: the quote and `\` (and `${` in a template) are escaped
  where they are, rather than switching to `{"…"}`.
- **Attribute strings containing `&`, and kind mismatches, lock as
  `Expression`.** `&` may be an HTML entity the source spells differently
  from the value the runtime receives, and a literal of another kind than
  the received value (`x="12"` where the runtime saw a number) is not what
  the prop is drawn with.
- **`style.<key>` without a `style` attribute locks as `NoStyleObject`**
  ("No style object to set this in") instead of adding a `style` object.
- **`{name}` text children resolve like identifiers**: a const, or a
  pass-through.
- **Only the innermost component's parameters pass through.** A parameter of
  an outer component read inside a nested one stays
  `UnresolvedIdentifier`.
- **Reused components skip the `Repeated` lock when the value passes
  through**, since each edit targets one call. A call site inside `.map()`
  is not locked yet; its copies show in the impact line as places in the
  scene.
- **The impact is scoped to a call only when every editable transform prop
  passes through to it.** A mixed selection, such as a call-site `x` with a
  `rotation` written on the element or a `y` missing from it, keeps the
  unscoped impact: a drag can write the element, which every instance
  draws. Locked props do not count either way.
- **A second hop locks as `ParameterOfCaller`** ("Comes from `<Card>`'s
  caller").
- **The runtime also reports `style.<key>` scalars**, so `style` properties
  are checked against what the runtime saw as attributes are.
- **The call site falls back to an "Anonymous" entry only when its tag names
  the component.** `memo` and `forwardRef` of an arrow report no name; the
  nearest anonymous entry may be a component the element is only passed to
  as children, so its tag must be `<PriceTag>` (or `<ui.PriceTag>`).
- **An unreadable call-site file locks only the pass-through props**
  (`CallSiteNotFound`); the element's own props stay editable.
- **The outside-project lock applies per target file.** A call site in
  `node_modules` or outside the entry's folder locks the props set there,
  not the element's own.
- **Only the creators of overridden elements render on a drag frame.**
  `jsxDEV` subscribes a component to the overrides only when it creates an
  element at an override's position; every other component keeps React's
  bailouts, as without a drag. Without overrides `jsxDEV` only checks one
  variable.
- **A drag remounts the entry when it starts and when it is cancelled.** A
  component that rendered before a position was overridden never
  subscribed, and an element cached with `useMemo` while overrides were set
  would keep the dragged value after, so the runtime remounts the entry
  whenever the overridden positions change, never for new values alone.
  The editor keeps the positions fixed for the whole drag: from the
  pointer's first move it overrides every prop the handle can change
  (`preview_values`; `x` and `y` for a move), with the value the runtime saw
  until the drag changes it. So the entry remounts on the first frame of a
  drag and on the first frame without overrides (an Escape; a written drag
  ends with the reload instead). Hook state starts over at those frames,
  and `<Static>` layers are sent to Rust again.
- **`<Static>` follows the drag.** Its `memo` comparison fails when a child
  written in it was created at an override's position, so that child
  renders with the value; other `<Static>`s are kept. Its cached layers are
  walked again only when a host node changed.
- **Module-scope JSX keeps its written values** while dragging: it is
  created once, outside any render.
- **An element cached with `useMemo` at an override position follows only
  the first drag frame.** The first frame remounts the entry, so the
  factory runs with that frame's value; later frames reuse the cached
  element and `jsxDEV` never sees their values. Left as a known limitation;
  `prop-overrides.test.mts` pins it (50 → 50 → 50), so a fix flips that test
  on purpose. A frame without overrides remounts and draws the written value.

## What 2b–2d build on

- 2b (Properties tab, Component section, inputs rows, `Text` shorthand vs
  `style`): typed `EditValue` targets for any prop, `children` and
  `style.<key>`, `resolve_inputs`, `ReceivedProps::get` for every prop,
  `resolve_through` for props written at a component's call site, and
  `write_all_recorded` with one `HistoryEntry` per committed field. Inputs
  that are parameters come back as `CallSiteNotFound` and are re-resolved
  with `resolve_through` when shown.
- 2c (Pivot, Resize): multi-file transform commits through `commit_files`,
  so a Center-mode rotation can write `rotation` in the component's file and
  `x`/`y` at the call site as one step.
- 2d (inline text editing): `children` targets in their literal forms, with
  `write_value` choosing JSXText or `{"…"}`.

## Verify by hand

In the editor:

- In a scene that calls a component with `x={40}`, select a layer inside it:
  X shows "Set at scene.tsx:…" with the arrow icon. When the component
  takes `x`, `y`, `rotation`, and `scale` all as props
  (`({ x = 0, y = 0, rotation = 0, scale = 1 }) => <Rect x={x} y={y} rotation={rotation} scale={scale} />`)
  and is used in two scenes, the impact line counts only that call's
  instances; with `({ x }) => <Rect x={x} />` it still names the other
  scene.
- Dragging the layer writes the call site, and undo after a drag that also
  rotated restores both files in one step.
- While dragging a layer whose `x` is set at the call site, the other
  layers the component draws from the same `x` move with it before the
  mouse is released, and nothing jumps back on release.
