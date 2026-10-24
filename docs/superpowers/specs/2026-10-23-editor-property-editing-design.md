# Property editing design

Part 2 of `2026-10-23-editor-element-editing-overview-design.md`. Builds on
design 1 (header, impact line, kept selection).

## Goal

A selected element's props can be edited in the Properties tab, the way
Unity's Inspector edits a component: text content, font, colors, size,
corner radius, opacity, image source, not only the transform. On the canvas,
text is edited in place, a box can be resized edge by edge, and rotation turns about the point the
handles suggest. Values computed from others are edited at their inputs.
Props written at a component's call site (`<PriceTag label="…" x={40}>`) are
editable when the element inside reads them.

## Non-goals

- Inserting keyframes, or editing values between keyframes (as today).
- `interpolateColor()`, `spring()`, easing, gradients (`Paint` objects), and
  `dash` arrays: shown read-only.
- Rich text (`<Span>` children), on the canvas or in the Properties tab.
- Updating what the code computes from a text (a label's frame sized to it)
  while typing on the canvas; it follows when the edit is committed.
- Path geometry (`Path`, `Polyline`, `Line` points, `Arrow` ends).
- Expressions deeper than one level of inputs (see [Inputs](#inputs)).
- Changing a prop's form (turning `x={W / 2}` into a literal). A locked value
  stays locked.

## Typed edit values (`crates/source-edit`)

`EditTarget` holds an `f64` today. It becomes generic over a value:

```rust
pub enum EditValue {
    Number(f64),
    String(String),
    Bool(bool),
}

pub enum EditTarget {
    Literal { span: Span, value: EditValue },
    Keyframe { span: Span, value: f64, frame: f64 },
    Missing { insert_at: u32, default: Option<EditValue> },
    /// `param`: the prop name at the call site (it may differ from the
    /// element's prop, as with `{ label: text }`).
    PassThrough { call_site: ElementRef, param: String, target: Box<EditTarget> },
    Locked(LockReason),
}
```

Recognized literal forms, besides the numeric ones resolved today:

| Form | Value | Written back as |
| --- | --- | --- |
| `name="text"`, `name={'text'}`, `` name={`text`} `` (no `${}`) | String | the same quote style. JSX attribute strings have no escapes, so a value with the attribute's quote, `&`, or a newline becomes `{"…"}` with JSON escapes; inside `{'…'}` and `` {`…`} `` the quote, `\`, and (for templates) `${` are escaped in place. An attribute string containing `&` is locked (it may be an HTML entity the source spells differently) |
| `name` (bare) | Bool(true) | `name` for true, `name={false}` for false |
| `name={true}` / `name={false}` | Bool | `{true}` / `{false}` |
| Text children: one JSXText, or one `{'…'}` child | String | JSXText when it has no `{`, `}`, `<`, `>` and no leading or trailing whitespace; otherwise `{"…"}` |
| Text children: one `{name}` child | String | resolved like an attribute value that is an identifier: a const, or a pass-through |
| `style={{ fontSize: 48 }}` (object literal property) | per property | the property's value |

Colors are strings; the panel validates them: `#RRGGBB`, `#RRGGBBAA`, or a
`<Theme>` color name, a word matching `^[A-Za-z][A-Za-z0-9_-]*$` written as
typed. The editor has no list of theme names, so the picker shows no color
for a name. Anything else is refused with "Enter a color: #RRGGBB,
#RRGGBBAA, or a theme color name". A const binding (`const LABEL = 'Sale'`) resolves as
numbers do today, with the same `SharedBinding` lock. The same comparison with
the received value runs for every type (`OverriddenByParent`). Today the
runtime reports only the six transform numbers as received props; the report's
`props` becomes the received value of every prop in the primitive's property
table, as JSON (finite numbers, strings, booleans; other values omitted), plus
`children` when it is a string.

`Text`'s `fontFamily`, `fontSize`, `fontWeight`, and `color` are shorthand for
`style`, and `style` wins. They map to `style.fontFamily`, `style.fontSize`,
`style.fontWeight`, and `style.fill` (`themedTextStyle` fills `fill` from
`color`). A Text row edits `style={{ … }}`'s property when `style` is an
object literal whose own properties set it (`{ ...base, fontSize: 48 }`
counts, since the key after the spread wins), and the shorthand otherwise,
also when there is no `style`: beside `style={{ fontWeight: 700 }}`, editing
Font Size inserts `fontSize={48}` and leaves the object alone. Those rows are
locked ("May be set by `style`") when `style` is not an object literal
(`style={mono(20, C.paper)}`), when the object has a spread that may set the
key (`{ fontSize: 48, ...base }`), and when the element has a spread with no
`style` written after it (`<Text {...rest} fontSize={48}>`: `rest.style`
may set it).

### Non-primitive elements

`resolve` locks every prop of a `component` element (`NotPrimitive`). That
stays for the selected element's own props, and is lifted where a later rule
resolves on a component element on purpose: the call site of a pass-through,
the "Component" section below, and design 4's timing windows (`Sequence`,
`Series.Sequence`).

Text children that are a JSXText split across lines are taken as React
renders them (whitespace collapsed); writing replaces the whole child span.

### Pass-through to the call site

When a prop's value is an identifier bound by the enclosing component's
destructured parameter (`function PriceTag({ label, x = 0 })`), the resolver
continues at that component's call site.

**Owner call site rule.** The source chain walks parent fibers, so its next
entry is not always the call of the component that wrote the element (an
element passed as `children` reports the component it was passed to, and
`'wrapper'` entries sit in between). The call site is the nearest `component`
entry above the element whose `name` equals the name of the function that
encloses the element in its file (`function PriceTag`, `const PriceTag = …`,
through `memo`/`forwardRef`). No such entry, or an anonymous function, locks
the prop ("Can't find where `PriceTag` is called"). At that call site:

- the call site writes the prop as a literal → `PassThrough` wrapping that
  literal's target;
- the call site omits it and the parameter has a literal default → `PassThrough`
  wrapping `Missing` at the call site, with that default;
- otherwise the call site's own rules apply (expression, spread, …), and their
  lock is reported with the call site's position.

Renamed destructuring (`{ label: text }`) and one level of `props.label`
(a parameter named `props`, read as `props.label`) are followed; anything else
is `UnresolvedIdentifier` as today. Only the parameter of the function that
encloses the element counts: a parameter of an outer component read inside a
nested component (`const Inner = () => <Text label={label} />`) stays
`UnresolvedIdentifier`. One hop only: a call site that itself passes a
parameter through is locked (`ParameterOfCaller`, "Comes from `<Card>`'s
caller").

A component element's own call site would otherwise be locked as
`NotPrimitive`; while resolving a pass-through, the call site is read as a
component element on purpose. An element rendered more than once by its
component's callers is not locked as `Repeated` when its value passes
through: each edit targets one call site. A call site inside `.map()` is not
locked yet.

Pass-through edits change one call site, so they change only the instances
that call site produces. Design 1's impact line counts them that way.

### Multi-file edits (`crates/editor`)

With pass-through, one gesture can touch two files: a Center-mode rotation
writes `rotation` in the component's file and `x`/`y` at the call site. Today
a commit writes one file (`TransformTargets.file`). Commits and `EditHistory`
entries hold every file an edit touches:

```rust
struct HistoryEntry { files: Vec<FileEdit> }
```

- Writing: every file is written to a temp file first, then all are renamed;
  if any temp write fails, none is renamed.
- Undo and redo check every file against `after` (or `before`); if any file
  changed outside Celesta, the whole entry is refused and dropped with the
  entries older than it for those files.
- The watched mtimes of all files are resampled after the write, as today for
  one file.
- The resolver is re-run against every file at commit time, as the drag
  commit does today; if any target moved, the commit aborts with "The source
  changed while editing".

### Drag preview through the runtime

With pass-through, a dragged value often feeds more than the dragged layer:
the other layers of the same component call, the lines a `.map()` call
makes, layout computed from it. Patching the dragged layer's transform after
evaluation moves that layer alone until the write reloads the code. Instead,
the preview hands the drag's values to the React runtime as prop overrides
for the frame it asks for anyway (no extra evaluations), so the frame shows
what the write will produce:

```text
PropOverride { file (absolute), line, column, prop, value }
```

- Each value overrides the prop its write changes: the element's own prop
  (`report.chain[0]`) for a `Literal`, `Keyframe`, or `Missing` target, and
  the parameter's prop at the call site (`Target.call_site`, `param`) for a
  `PassThrough`. Locked props are never dragged. The positions are taken
  when the drag starts and kept while a released drag writes, until the
  reload's frame is shown. From the pointer's first move on, every prop the
  handle can change is overridden (`x` and `y` for a move), with the value
  the runtime saw until the drag changes it, so the set of positions stays
  the same for the whole drag.
- The frame request carries `propOverrides` (left out when empty; exports
  never send it). `jsxDEV` creates an element written at an override's
  position with the value, comparing the recorded file name resolved as
  `layerSource` resolves it. Without overrides it checks one variable.
- A component renders only when something it reads changed, so `jsxDEV`
  subscribes a component that creates an element at an override's position
  to a context whose value changes with the values; a `memo` component that
  reads no frame renders again too, and every other component keeps React's
  bailouts as without a drag. Components that rendered before a position was
  overridden never subscribed, and an element cached with `useMemo` while
  overrides were set would keep the value after: the entry remounts when the
  overridden positions change, never for new values alone: with a fixed set
  of positions, once when a drag starts and once when it is cancelled.
  `<Static>` renders a child written in it at an override's position on
  every drag frame and keeps the others; its cached layers are walked again
  only when a host node below changed. Elements created outside any render
  (at module scope) keep their written values, and an element cached with
  `useMemo` at an override's position follows only the first drag frame:
  later frames reuse the element created then, so `jsxDEV` never sees their
  values (a known limitation; a test pins it).
- Field edits (typing in a row) write without a preview, and need no
  overrides. Scrubbing a number row's label previews as a drag does: from
  the first move, the row's value overrides the prop its write changes (the
  element's prop, `style.<key>` for a `Text` font row, whose override sets
  that key in a copy of `style`, or the call site's `param` for a
  pass-through), at positions fixed for the scrub; release writes once,
  Escape drops the overrides and writes nothing. Input rows do not scrub (a
  `const` is not a prop to override), nor do locked and keyframe rows. A
  scrub and a canvas drag never run together.

## Property tables (`packages/react`, `packages/shapes`)

The editor needs, per primitive, the props it can edit and their types. They
are declared next to the components, so they cannot drift from the props
types:

```ts
// packages/react/src/components.ts, beside <Rect>
registerPrimitiveProperties('Rect', propertyTable<RectProps>({
  size: { width: 'width', height: 'height' },
  groups: {
    Transform: {
      x: num(0), y: num(0), rotation: num(0), scale: num(1, { step: 0.01 }),
      anchorX: num(0, { step: 0.01 }), anchorY: num(0, { step: 0.01 }), opacity: num(1, { min: 0, max: 1 }),
    },
    Size: {
      width: required(num(undefined, { min: 0 })),
      height: required(num(undefined, { min: 0 })),
      cornerRadius: num(0, { min: 0 }),
    },
    Appearance: { fill: color(), stroke: color(), strokeWidth: num(0, { min: 0 }) },
  },
}));
// Group, Image, Text, Video likewise, and @celesta/shapes' Ellipse,
// Circle, Triangle, Polygon, Star, Pie, Heart beside theirs
```

- Fields are their own type, `PrimitivePropertyField`: `ComponentPropertyField`'s
  types (`string`, `number`, `boolean`, `color`, `select`, `path`) plus `text`
  for children, with no `label` (the row label is the key in words) and an
  optional `defaultValue` (`Rect`'s `width` has none). `number` may carry
  `min`/`max` (`opacity` 0–1): a typed value outside is refused with "Enter a
  number from {min} to {max}" ("Enter a number of at least {min}" or "Enter
  a number of at most {max}" when only one bound is set), and a scrub
  clamps. It may carry `step`, what one pixel of a scrub changes. The bounds
  are the components' own: `min: 0` where a negative value is rejected or
  draws nothing (sizes, radii, strokes, `fontSize`, `maxWidth`, a video's
  `startFrom`, `playbackRate`, `volume`), the smallest sensible inclusive
  bound where the value must be positive (an `Image`'s size 1, a shape's
  `strokeWidth` 0.1), whole numbers of at least 3 for `sides` and `points`
  (`integer: true`, never implied by a step of 1: a typed fraction is
  refused, "Enter a whole number of at least 3", and a scrub rounds), and
  `fontWeight` 1–1000 in
  steps of 100 (static faces are picked by weight class).
- A prop the props type requires is declared `required(…)` (`Rect`'s
  `width`/`height`, `Image`'s and `Video`'s `src`, `Text`'s `children`, a
  `Circle`'s `radius`, …): its row is never removed. The table's type
  rejects a required prop not declared so, and an optional one declared so.
- `size` names the props that hold the box: `width`/`height`, `radius`
  (uniform), or `maxWidth` (width only, for `Text`). Primitives without one
  (`Group`) have no resize handles.
- `@celesta/shapes` registers its tables through the same internal entry it
  uses for `markPrimitive`.
- The runtime sends the tables in the ready message, beside the component
  schemas; `ReactMetadata` gains `primitive_properties`.
- A Vitest type test checks every key against the component's props type, so
  a renamed prop fails the build.

## Properties tab

Under design 1's header, one section per group of the selected primitive's
table, in the table's order. Each row uses the existing Transform row states
(Literal, Keyframe, Between keyframes, Missing with the default dimmed,
Locked with its reason) and a control by type:

| Type | Control |
| --- | --- |
| number | numeric field; drag on the label scrubs it (the field's declared `step` per pixel, else 0.01 when `max` ≤ 1, else 1; Shift ×10; clamped to `min`/`max`), previewed through the runtime and written once on release |
| string | single-line field |
| text | multi-line field; Enter commits, Shift+Enter breaks the line, blur commits |
| boolean | switch |
| color | swatch with picker, or a theme-name field |
| select | dropdown |
| path | field plus "Choose…", copying outside files in with `import_path` |

A pass-through row shows a small `corner-down-right` icon and the call site
("Set at `Price.tsx:18`"). Commits go through the existing resolve, apply,
write, and history path; one committed field is one history entry.

A written optional prop can be removed, so its default applies again:
emptying its field and committing (Enter or blur), or the "×" (lucide `X`,
ghost, tooltip "Remove") that shows at the row's end on hover, for every
control kind. The removal deletes bytes only, never across a line break:
the attribute with the spaces around it on its line (the whole line when it
stands alone there), a `style` object's property with its separating comma
(the only one leaves `{}`), or for a pass-through the attribute at the call
site, so the parameter's default applies. It is one history entry, and the
row shows the default dimmed until the reload resolves. A required prop, an
input, a `Text`'s children, and an absent, locked, or keyframed prop are
never removed; an emptied field of such a row shows the source's value
again. A Component-section prop is removable (the component's own default
applies).

A row's control is disabled only when its target can't be written. While the
editor's own write reloads, the fields stay enabled and keep their focus and
caret; a commit made meanwhile (or while another write runs) waits and is
written once the targets are current again, one at a time in the order made,
the latest value per row winning. If the code fails to reload, the waiting
commits are dropped, each row saying "Not saved: the code failed to reload".
Every text a commit would write is parsed first; one with syntax errors is
not written.

Leaving a field commits it, as a blur does: before the selection changes (a
canvas click, the layer list, clearing it) and when the playhead is moved by
hand or playback starts, the rows' typed text is submitted, and text that
does not parse is dropped. While commits wait for the selected layer, a
selection change waits for them to be written; a newer one replaces it.

When the selected element is a component call's root (the chain's nearest
component entry exists), a "Component" section lists the props written at that
call site, typed from their literals, or from `registerComponent()`'s schema
when the component is registered. Props the component reads through to a
primitive are not repeated there.

## Inputs

A prop whose value is an expression (`width={textWidth + padding * 2}`) is
still locked, and the row gains its inputs: each identifier in the expression
is resolved with the same rules (const, parameter pass-through), and those that
resolve to an editable target are listed under the row as editable sub-rows
("padding  16"). Identifiers that do not resolve are listed read-only with
their reason. One level only: an input that is itself an expression is not
expanded.

On the canvas, an axis whose size or position prop is locked has no handle for
that axis, as today. This is what keeps a label frame sized to its text from
being stretched.

## Canvas

### Pivot

Rotation and scale today turn about the layer's anchor (`x`/`y`), which is the
top-left corner for most primitives. They now turn about the center of the
layer's box by default, as the handles suggest:

- A viewer toolbar toggle, "Center" (default) / "Pivot", like Unity's. In
  Pivot mode they turn about the anchor, as today.
- In Center mode a rotation or scale also moves `x`/`y` so the box's center
  stays put: the anchor `p` becomes `c + R(Δθ)·(p − c)` for rotation, and
  `c + (p − c)·s` for scale, in parent space. Both writes are one edit.
- When `x` or `y` is locked or a keyframe, Center mode cannot keep the center
  (a keyframe would move at one keyframe only, and the element would orbit
  between them), so that drag uses the anchor, and the toggle shows
  "Pivot (x, y not editable)". The same applies to every compensation below:
  the left/top edge resize and the anchor drag need `x`/`y` targets that are
  `Literal` or `Missing` (directly or through pass-through).
- The anchor is always drawn as a small cross. Dragging the cross with Alt
  moves the anchor: it writes `anchorX`/`anchorY` (0–1 of the box) and moves
  `x`/`y` by the opposite offset so nothing moves on screen. `anchorY="baseline"`
  on `Text` is locked.

### Resize

Handles depend on a viewer toolbar tool, "Rect" (default) or "Scale":

- **Rect**: corner and edge handles change the `size` props. Dragging a left or
  top edge also moves `x`/`y` so the opposite edge stays (through the anchor and
  rotation). Shift keeps the aspect. `radius` primitives get corner handles only
  and resize uniformly; `Text` gets left and right edge handles changing
  `maxWidth`. A primitive without `size` props, or whose `size` props are
  locked, falls back to Scale.
- **Scale**: today's scale handles.

Rotation keeps its stem handle in both tools.

### Inline text editing

Double-clicking a `Text` layer whose children resolve to an editable string
(the `text` row's target is a literal, a const, or a pass-through) edits the
text in place, in its own font, size, wrapping, and transform:

- **Live, without writing.** The editor replaces the layer's `text` in the
  held scene (the scene's `LayerContent::Text` carries the string, and the
  Rust renderer lays it out) and re-renders on every keystroke. Nothing is
  written while typing, and, unlike a drag (see "Drag preview through the
  runtime"), the React code is not run: what the code computes from the
  text (a frame sized to it) keeps its old size until the commit's reload. (Decided with the user:
  a frame changing size while typing gets in the way.)
- **Caret and selection** come from the layout's `TextMetrics`
  (`line_starts`, and each glyph cluster's code-point range, left edge, and
  advance), mapped through the layer's world transform as the handles are, so
  they follow rotated and scaled text. Arrow keys, Home/End, Shift to select,
  Cmd+A, copy, cut, and paste (as plain text) work as in a text field;
  clicking and dragging inside the text places the caret and selects.
- **IME.** The view implements GPUI's input handler for the editing layer;
  marked (composing) text is shown in the live render, underlined by the
  editor using the same caret geometry, and the candidate window is placed at
  the caret.
- **Commit and cancel.** Enter commits, Shift+Enter breaks the line, Escape
  restores the scene and leaves without writing. Clicking outside the text,
  selecting another layer, seeking, or starting playback commits first. A
  commit goes through the same resolve, apply, write, and history path as the
  Properties tab (one history entry), with design 3's scope when the element
  is shared. An outside change to the file while editing cancels with "The
  source changed while editing".
- **Not editable.** On a `Text` whose `text` row is locked (an expression,
  `<Span>` children, a value that can't be traced), double-click shows the
  row's lock reason beside the layer and does not enter editing.

While editing, the transform handles are hidden and the layer keeps its
outline.

## Testing

Rust (`crates/source-edit`):

- Every literal form in the table, for read and write, including quote
  choice, escapes, JSXText rules, bare booleans, and `style` object
  properties.
- Pass-through: literal at the call site, missing with a default, renamed
  destructuring, `props.label`, a second hop locked, a spread at the call site;
  the owner call site found past a `'wrapper'` entry and past a component the
  element was passed to as `children`.
- `Text` shorthand versus a literal `style` object, and a non-literal
  `style` locking the shorthand rows.
- Inputs: an expression with one resolvable and one unresolvable identifier.

Rust (`crates/editor`):

- Multi-file history: undo restores both files; an outside change to one
  refuses the entry; a failed temp write renames nothing.
- Center-mode rotation and scale keep the center for a rotated, scaled parent;
  `x` as a keyframe falls back to the anchor.
- Anchor drag keeps the screen position.
- Edge resize with anchor 0 and 0.5, rotated.
- Inline text: caret and selection geometry from `TextMetrics` for wrapped,
  multi-line, rotated, and combining-character text; the held scene's text is
  replaced and restored on Escape; seeking or selecting another layer
  commits; a locked `text` row does not enter editing.
- Drag overrides: a `Literal` or `Missing` target overrides the element's
  prop, a `PassThrough` the call site's `param`, a locked one nothing; a
  move overrides `x` and `y` from its first frame, an unmoved axis with its
  start value, at the same positions on every frame; each handle's props
  (rotate, uniform and per-axis corners, an edge); no overrides before the
  pointer moves or while playing.

Rust (`crates/react-bridge`): a frame request carries `propOverrides` only
when there are some.

TypeScript (Vitest):

- Property tables type test for every primitive.
- Tables reach the ready message.
- Received props carry strings, booleans, and string `children`.
- Prop overrides: a written prop replaced on the element at the position
  only, an absent prop added, a call-site override reaching every layer that
  reads the parameter, an element in a `memo` component that reads no frame
  following two override frames, an override at a new position mid-drag
  reaching one, a drag frame rendering only the creator of the overridden
  element (not another `memo` component, nor a `<Static>` without one), the
  written values back after a frame without overrides (an element cached
  with `useMemo` too), an element cached with `useMemo` at an override's
  position keeping the first drag frame's value (the limitation, pinned),
  and an element inside `<Static>` following the drag; a CLI frame request
  with `propOverrides`.

Manual, with before/after screenshots of the same scene: edit a `Text`'s
content inline (including Japanese input through the IME) and its font size, a `Rect`'s fill and corner radius, resize a `Rect` from
its left edge, rotate a `Rect` in Center mode, and edit a prop passed at a
component's call site.

## Documentation

- The React preview user docs: inline text editing, the Properties sections, pass-through rows,
  inputs, the Center/Pivot toggle, the Rect/Scale tools.
- A handoff note.
