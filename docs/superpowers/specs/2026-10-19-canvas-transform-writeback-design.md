# Canvas transform write-back design

Issue: [mika-f/Celesta#313](https://github.com/mika-f/Celesta/issues/313)

## Goal

Layout in a React composition is adjusted only by editing the source; the
app's GUI changes nothing but props-file properties. In the standalone
React preview, a selected layer should be movable, rotatable, and
scalable by dragging on the canvas or typing in the Layer section, and the
result should be written back into the JSX in the user's source, so that
reloading reproduces it.

A value is written back only when the source states it literally: a
numeric literal, a missing prop with a known default, or a literal
keyframe in an `interpolate()` call at the current frame. Anything else is
shown read-only with the reason. Canvas edits and props-file edits share
one undo history.

## Non-goals (first version)

- `width`, `height`, `maxWidth`, shape-specific size props (`radius`,
  points, …), `anchorX`/`anchorY`, `skewX`/`skewY`, 3D rotation, opacity.
  Size changes go through `scale`.
- Props on user-defined components (`<Card x={100}>`). Only Celesta
  primitives written directly in the user's files are edited. The resolver
  is shaped so a later rule can add them (see
  [Extending to user components](#extending-to-user-components)).
- Inserting keyframes. Between keyframes values are read-only.
- `spring()`, easing functions, `interpolateColor()`, computed ranges.
- Expressions (`W / 2`, `x + 40`, `COL.x`, `i * 24`), and identifiers
  other than a one-level `const` (see [Resolving](#resolving-a-prop)).
- Keyboard nudging, snapping, guides, multi-selection.
- Project (`.celesta`) layers and resolved component clips; only
  `ReactPreviewMode::WholeScene`, like selection (#306).
- A file-level formatter pass. Edits replace the exact span and nothing
  else.

## Overview

```text
select layer ─> layerSource (chain + kind + received props + localFrame)
             ─> celesta-source-edit::resolve per prop ─> EditTarget
             ─> Layer section Transform rows + canvas handles

drag ─> local delta via inverse parent transform ─> prop values
     ─> evaluate the frame with them as prop overrides, re-render
        (no file write)
mouse-up / Enter ─> re-read file, resolve again, apply edits
               ─> atomic write ─> EditHistory entry ─> resample
                  watched_mtime ─> reload, keep selection
Cmd+Z / Cmd+Shift+Z ─> EditHistory undo/redo (props.json too)
```

## Runtime additions (`packages/react`, `packages/cli`)

The `layerSource` response from #306 gains, per chain entry, a `kind`:

```ts
interface LayerSourceEntry {
  name: string;
  file: string;
  line: number;
  column: number;
  /** Whether the element's type is a Celesta primitive. */
  kind: 'primitive' | 'component';
}
```

`kind` is decided by type identity against the set of Celesta primitive
components (`Group`, `Image`, `Text`, `Rect`, `Video`, `Mask`, and the
shapes in `@celesta/shapes`), not by name, so a user component named
`Rect` is a `component`.

The `layerSource` object also gains:

- `localFrame: number` — the value `useCurrentFrame()` returns at the
  layer's fiber, read from the nearest frame context on its `return`
  chain. It differs from the timeline frame inside `<Sequence>`.
- `props: { x?, y?, rotation?, scale?, scaleX?, scaleY? }` — the values
  the innermost primitive actually received (`memoizedProps`), omitted
  keys when absent. Only finite numbers are included.

- `renderCount: number` — how many layers in the frame have the same
  innermost source position as this one.

All three are present only when the chain is non-empty.

## `celesta-source-edit` crate

A new crate with no GPUI dependency. It parses with `oxc_parser` (plus
`oxc_allocator`, `oxc_ast`, `oxc_span`) and never re-prints the AST:
edits are byte-span replacements on the original text, so formatting and
comments survive.

### Locating the element

`element_at(text, line, column) -> Option<JsxOpeningElement span>` finds
the JSX opening element whose `<` is at the 1-based position from
`layerSource`. Columns are converted with the same unit esbuild uses
(UTF-16 code units are not assumed; a test pins the convention with a
non-ASCII line).

### Resolving a prop

```rust
pub enum EditTarget {
    Literal { span: Span, value: f64 },
    Keyframe { span: Span, value: f64, frame: f64 },
    Missing { insert_at: u32, default: f64 },
    Locked(LockReason),
}

pub enum LockReason {
    NotPrimitive,
    Expression { source: String },
    UnresolvedIdentifier { name: String },
    SharedBinding { name: String, uses: usize },
    BetweenKeyframes { previous: f64, next: f64 },
    OutsideKeyframes,
    UnsupportedCall { callee: String },
    OverriddenByParent,
    Spread,
    Repeated { count: u32 },
}
```

`resolve(text, element, prop, ctx) -> EditTarget`, where `ctx` carries
the local frame and the received prop value. Rules, in order:

1. The element must be `kind: 'primitive'` → else `NotPrimitive`.
   If the runtime reports the element rendered more than once in the
   frame (`renderCount > 1`, e.g. inside `.map()`) → `Repeated`, since
   editing it would move every copy.
2. An explicit attribute after a spread (`{...rest}`) is fine; a prop
   that is absent but a spread is present → `Spread` (the spread may set
   it).
3. Attribute absent → `Missing` at the end of the last attribute (or
   after the tag name). Default: `x`, `y`, `rotation` 0; `scale`,
   `scaleX`, `scaleY` 1.
4. Value is a numeric literal, optionally with unary `-` → `Literal`.
5. Value is `interpolate(...)` → [keyframe rule](#keyframes).
6. Value is an identifier bound by `const` in an enclosing function scope
   of the same file:
   - its initializer is a literal → `Literal` on the initializer, or an
     `interpolate(...)` call → the keyframe rule on that call;
   - the binding is referenced more than once in the file →
     `SharedBinding` (editing it would move other things too);
   - anything else (imports, `let`, parameters, module-level objects) →
     `UnresolvedIdentifier`.
7. Any other expression → `Expression` with its source text.

Last, the resolved value is compared with the received prop from the
runtime. If they differ (a parent cloned the element with another value,
or the binding is not the one the runtime saw), the target becomes
`OverriddenByParent`. For `Missing`, the received prop must also be
absent.

### Keyframes

`interpolate(input, inputRange, outputRange, options?)` is editable when:

- `input` is an identifier bound to `useCurrentFrame()` in the same
  function, or that identifier minus a numeric literal (`frame - 30`);
- `inputRange` and `outputRange` are array literals of numeric literals
  of equal length.

The effective frame is `localFrame` (minus the literal offset). If it
equals `inputRange[k]` (within 1e-6), the target is `Keyframe` on
`outputRange[k]`. If it lies strictly between two entries →
`BetweenKeyframes { previous, next }` in local frames, translated back to
timeline frames by the editor for the jump buttons. Outside the range →
`OutsideKeyframes`. `options` is ignored: at a keyframe the easing does
not affect the output value. Other callees → `UnsupportedCall`.

### Applying

`apply(text, edits: &[Edit]) -> String`, where an edit is a span
replacement or an insertion. Several edits (a move writes `x` and `y`)
are applied in one pass, sorted by offset; overlapping edits are an
error. Inserted attributes are `name={value}` preceded by a space.

Numbers are formatted as an integer when the value rounds to one within
0.005, otherwise with up to 2 decimals and trailing zeros trimmed.
Rotation uses the same rule. A negative number is written `-12`.

### Extending to user components

The editor depends only on `resolve`/`EditTarget`. A later version can add
a rule for `kind: 'component'` entries by tracing pass-through: the
runtime would record, for a primitive's prop, that its value is the same
object path as a prop of the enclosing user component, and the resolver
would continue at that component's call site. Nothing in this version
needs to change for that.

## Editor

### Selection and resolution

When the source lookup for the selected layer returns, the view reads the
innermost chain entry's file, runs `element_at` and `resolve` for `x`,
`y`, `rotation`, and the scale props on a background executor, and keeps
the per-prop `EditTarget`s next to the selection. Scale props: when
`scaleX` or `scaleY` is written, those two are the targets (each resolved
on its own); otherwise `scale` is, and scaling is uniform.

### Dragging

- The local delta is computed from the scene-space pointer delta through
  the inverse of the parent's world transform, composed from the
  `Group` layers above the selected layer in the held `Scene`.
- During the drag nothing is written. The editor turns the local delta,
  angle delta, and scale ratio into the prop values the write will put into
  the source, and asks the React runtime for the frame with those values as
  prop overrides, so everything computed from them moves too (see
  "Drag preview through the runtime" in
  `2026-10-23-editor-property-editing-design.md`).
- Axes whose target is `Locked` do not move. Uniform-scale targets keep
  the aspect.
- On mouse-up: re-read the file, `resolve` again, and if any target's
  kind or span differs from the drag start, abort, restore the scene, and
  show "The source changed while dragging". Otherwise `apply` and
  write.

### Writing

The write reuses the property panel's path: background executor, atomic
replace through a temp file, then resample `watched_mtime` so the reload
watcher does not see an outside edit, then `request_react_reload`. After
a reload caused by the editor's own write, the selection is kept if the
same layer id exists; other reloads clear it as before.

### Edit history

A new `EditHistory` owned by the view:

```rust
struct FileEdit { path: PathBuf, before: String, after: String }
```

- Every canvas write and every `props_edit::set_property` commit pushes
  one entry; a drag or one committed field is one entry. Pushing clears
  the redo stack.
- Undo reads the file; if it equals `after`, writes `before` (same write
  path) and moves the entry to redo. Otherwise it refuses and shows "The
  file changed outside Celesta" and drops the entry and everything older
  for that file. Redo is symmetric.
- The history is in memory and cleared when the entry changes. It holds
  at most 100 entries.
- Shortcuts: Cmd+Z / Cmd+Shift+Z (Ctrl on Windows/Linux) while the
  preview or the property panel has focus and no text input is focused.

## UI

All styles follow the existing inspector: radius 0, no gradients, no
uppercase labels, lucide icons. Labels are English like the existing
Layer section.

### Canvas

- Selected layer: the existing 1px accent outline plus handles for the
  editable targets.
  - Move: drag inside the quad.
  - Scale: square handles at the corners and edge midpoints; edge handles
    are hidden for uniform scale. Scaling is about the anchor.
  - Rotate: a short stem from the top edge's midpoint ending in a square
    handle; Shift snaps to 15°.
  - Anchor: a small cross at the anchor point.
- No editable target: no handles, a dashed outline, a not-allowed cursor
  over the quad, and a tooltip with the first lock reason.
- While dragging: a small label beside the quad, e.g. `x 120 → 184`.
- A selected `Group` (via parent selection) has no hit region; its
  outline is the bounding box of its descendants' regions.

### Layer section

Under the existing source rows, a "Transform" group with rows X, Y,
Rotation, and Scale (or Scale X / Scale Y), each a numeric field and a
state icon:

| State | Icon | Field |
| --- | --- | --- |
| Literal | none | editable |
| Keyframe | `diamond` filled | editable |
| Between keyframes | `diamond` outline | read-only, plus "Between keyframes at 0 and 30" and `chevron-left`/`chevron-right` buttons jumping to them |
| Missing | none | editable, default shown dimmed |
| Locked | `lock` | read-only, plus the reason on one line |

Reason texts:

- `NotPrimitive` — "Set by a component, not written on this element"
- `Expression` — "Written as an expression: `W / 2`"
- `UnresolvedIdentifier` — "Comes from `name`, which can't be traced"
- `SharedBinding` — "`enter` is used in 3 places"
- `OutsideKeyframes` — "Outside the keyframes"
- `UnsupportedCall` — "Computed by `spring()`"
- `OverriddenByParent` — "A layout component sets this value"
- `Spread` — "May be set by a spread (`{...props}`)"
- `Repeated` — "Rendered 12 times from this element"

A field commits on Enter or blur through the same resolve-apply-write
path and pushes one history entry. Each source-chain row gains a "Select"
button that selects that element's `Group` layer when the element is a
primitive `Group`.

Write errors, aborted drags, and refused undos show in the Layer section
in the same place as lookup errors.

## Testing

Rust, `celesta-source-edit` (fixtures as TSX strings):

- `element_at` on nested, self-closing, fragment-adjacent, and non-ASCII
  lines.
- Every `resolve` rule and lock reason, including `-12`, `const`
  initializers, shared bindings, spreads before/after, `frame - 30`,
  keyframe hit, between, outside, and `OverriddenByParent`.
- `apply`: single replace, multi-edit, insertion after the last
  attribute and after a bare tag name, number formatting, preserved
  comments and whitespace.

Rust, editor:

- `EditHistory` undo/redo, refusal on outside change, cap, clear.
- Inverse parent transform for translated, rotated, and scaled groups.

TypeScript:

- `kind` by identity (a user `Rect` is `component`), `localFrame` inside
  nested `<Sequence>`s, received `props`.
- Export scenes are unchanged with and without `--source-locations`
  (already covered by #306's test; extended to the new fields).

Manual: in a fixture with literals, keyframes, a `<Stack>` child, and an
expression, drag, type, undo, redo, and reload; check that the file diff
touches only the expected spans. Screenshots of the same scene before and
after a drag go in the PR.

## Documentation

- A handoff note `docs/handoff/<date>-canvas-transform-writeback.md`.
- The React preview user docs describe what can be dragged, the lock
  reasons, and undo.
