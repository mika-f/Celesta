# Edit scope design

Part 3 of `2026-10-23-editor-element-editing-overview-design.md`. Builds on
design 1 (occurrence index, impact line) and design 2 (typed values,
pass-through, multi-file history).

## Goal

When the selected element is shared, the user chooses, before committing,
whether an edit changes **this scene** or **the shared value**. Truly shared
things (a background) change everywhere in one edit; a thing that should
differ in one scene changes only there, and the TSX says so in a way a person
would write it. As with a Unity prefab, a value set for one scene stays when
the shared value changes later.

## Non-goals

- "This scene" for a keyframe value (`interpolate()` output) or a value inside
  `style={{ … }}`: locked with a reason.
- Passing a value through more than one component (scene → `Card` → `Tag` →
  `<Rect>`): "This scene" is offered only when the scene's call is the direct
  call of the component that writes the element.
- Renaming the props this design creates. They can be renamed in code.
- Choosing a scope per instance among several in one scene. "This scene"
  means every instance in the current scene.

## Choosing the scope

Design 1's impact line becomes a control when the element is shared:

```text
Edits change  [ This scene ]  [ Shared value ]
              Shared value is used in Intro, Outro · Price sets its own
```

The line under the control names the scenes the shared value reaches and the
scenes that set their own value (and so keep it).

- Default: **This scene**. (Decided by the designer: an unintended spread to
  other scenes is the failure the user reported; a deliberate shared edit is
  one click away.)
- The choice is remembered per element (its `file:line:column`) for the
  session, so editing a background repeatedly does not ask again.
- While the control is hovered, the timeline bands (design 1) show only the
  instances the choice would change.
- An element with one instance shows design 1's plain "Changes this element
  only".
- When "This scene" can't be done (see [Locks](#locks)), the option is disabled
  with its reason; the default then is "Shared value".

A canvas drag uses the current choice; it is visible in the header while
dragging.

## What "Shared value" writes

The value every scene uses unless it sets its own:

- a literal at `E` (never lifted): that literal, as today;
- a prop lifted by this design, or any parameter with a literal default in
  `C` (`{ rectWidth = 200 }`): that default. Call sites that pass the prop
  keep their value; the line under the control names them;
- a parameter without a default, passed at every call site: "Shared value"
  is disabled ("Every scene sets `rectWidth` itself").

So after one scene sets its own value, later "Shared value" edits still reach
every other scene, and never remove a scene's own value. Removing it ("Use
the shared value") is a button on the row while "This scene" is chosen and
this scene's call site passes the prop: it deletes the attribute at `S`.

## What "This scene" writes

Let `E` be the element, written inside component `C`, and `S` the call of `C`
that produces the current scene's instances, found with design 2's owner call
site rule. "This scene" needs `S` to be confined to the current scene: every
occurrence of every instance whose chain contains `S` lies in this scene, by
the occurrence index.

1. **Already a parameter.** If `E`'s prop resolves through pass-through
   (design 2), the edit writes at `S` only. Nothing new is created.
2. **Otherwise, lift the value into a prop of `C`**, keeping today's value as
   the default, and pass the new value at `S`:

```tsx
// before                               // after ("This scene", in the Price scene)
function PriceTag({ label }: Props) {   function PriceTag({ label, rectWidth = 200 }: Props) {
  return <Rect width={200} … />;          return <Rect width={rectWidth} … />;
}                                       }
// Price.tsx                            // Price.tsx
<PriceTag label="Sale" />               <PriceTag label="Sale" rectWidth={260} />
```

Lifting:

- The new prop's name is the element's tag in lower camel case plus the prop's
  name with a capital (`rectWidth`, `textFontSize`, `rectX`). If `C` already
  has that name, a number is appended (`rectWidth2`).
- `C`'s first parameter must be an object pattern (or absent, which becomes
  `{ rectWidth = 200 }`). Anything else is locked.
- The default is the current literal, written in the same form the literal had.
- The literal at `E` becomes the identifier.
- `C`'s props type gains an optional field (`rectWidth?: number`, by
  `EditValue` type) when `C`'s parameter has a type annotation that is a type
  literal or names an `interface`/`type` alias declared in the same file. An
  imported props type is locked. No annotation: nothing to add.
- A move writes `x` and `y`: both are lifted in one edit.

After lifting, later edits of the same element in another scene resolve as
case 1 at that scene's call site; the default keeps every other scene as it
was.

Lifting edits two places, often two files; it uses design 2's multi-file
commits and history entries.

## Locks

"This scene" is disabled, with the reason shown under the control, when:

| Case | Reason text |
| --- | --- |
| `S` produces instances in other scenes too | "`<PriceTag>` is called here for other scenes too" |
| The scenes share one chain: `C` is called once and repeated by `.map()` or `<Scenes>` | "The scenes reuse the same call (`.map()` or `defineScenes`)" |
| More than one component between the scene and `E` | "Passes through `<Card>`" |
| `C`'s parameter is not an object pattern | "`PriceTag` doesn't take its props as `{ … }`" |
| `C`'s props type is imported | "`PriceTag`'s props type is in another file" |
| The value is a keyframe or in a `style` object | "Can't be set per scene yet" |
| `E` is written directly in the scene (not in a component) | not shown: the element is not shared |

## Testing

Rust (`crates/source-edit`), fixtures as TSX strings:

- Lifting with no parameter, an object pattern, an inline type literal, a local
  interface, a local type alias, an imported type (locked), and a name clash.
- A lifted prop is found by pass-through on the next edit.
- "Shared value" after a lift writes the default and leaves the call site's
  attribute; "Use the shared value" deletes it; a parameter without a default
  disables "Shared value".
- A move lifts `x` and `y` in one edit.

Rust (`crates/editor`):

- Scope default and memory per element; "This scene" disabled when `S`'s
  instances span scenes.

Manual, with before/after screenshots of the same scene: a component used in
two scenes, edit its size in one scene, check the other scene is unchanged and
the TSX diff matches the example above; then change the shared value and
check the first scene keeps its own; edit a background with "Shared value".

## Documentation

- The React preview user docs: the scope control and what it writes.
- A handoff note.
