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

Design 1's impact line becomes a control when the element is shared. The
line, or the control in its place, sits at the top of the Properties panel,
outside its scroll area, so it stays in view while the header and the rows
scroll beneath it:

```text
Edits change  [ This scene ]  [ Shared value ]
              Shared value is used in Intro, Outro · Price sets its own
```

The line under the control names the scenes the shared value reaches and the
scenes that set their own value (and so keep it). A scene sets its own when
its call of `C` passes any parameter `E`'s rows read. Several such scenes read
"Price, Sale set their own"; more than three scene names are counted ("5
scenes"). With no scene named (a composition without chapters) the line reads
"Shared value is used everywhere"; when every scene sets its own, "Shared value
is used in no scene". When an option is locked, the line is its reason
instead (the chosen option's first).

The tabs' tooltips are "Write the edit for the instances in this scene only"
and "Write the value every scene uses unless it sets its own"; a locked tab's
tooltip is its reason.

When the current scene calls `C` more than once, "This scene" means this
call: it writes at the selected instance's `S` only (see [What "This scene"
writes](#what-this-scene-writes)). The tab keeps its label; its tooltip is
"Write the edit for this call only", and while "This scene" is in effect or
hovered, the line under the control reads "Edits this `<PriceTag>` only ·
this scene calls it 2 times" (the calls counted are the distinct chain
entries at `S`'s place among the instances shown in this scene; an instance
with no entry there is no call). A composition without chapters is one
scene of the whole timeline, and keeps the one-call copy.

While an option is hovered, the bands and the "Shared" row are read on the
targets that option writes (the element as one instance only under "This
scene"), as a click on it would show them.

- Default: **This scene**. (Decided by the designer: an unintended spread to
  other scenes is the failure the user reported; a deliberate shared edit is
  one click away.)
- The choice is remembered per element (its `file:line:column`) for the
  session, so editing a background repeatedly does not ask again. Celesta's
  own edits carry it for the element being edited: when a lift moves `E` (an
  interface field added above it), the reload moves the choice to `E`'s new
  position. The other choices made in the files that edit wrote are
  forgotten, since a position may now name another element; a change from
  outside the editor forgets every choice. A forgotten choice falls back to
  the default.
- The timeline bands (design 1) and the header's "Shared" row show only the
  instances the chosen option changes; while an option is hovered, they
  preview that option's instead.
- An element that is not shared (one instance, or not written inside a called
  component) has no control: design 1's impact line stays as it was.
- When "This scene" can't be done (see [Locks](#locks)), the option is disabled
  with its reason; the default then is "Shared value".
- A remembered choice that a new lock disables is never widened to the other
  option: neither tab is shown chosen, the line shows the reason in the warning
  color, and commits are refused with it until the user picks an option.

A canvas drag, a label scrub, and an inline text edit use the option in effect
when they start (the remembered choice, else the option shown), and start only
when their end would be written under it: a prop it would refuse has no handle
(no size handles when a size prop is refused), and a scrub or a double-click
shows the refusal instead. While a drag runs, the Properties header and a
second line of the drag label say "Edits change this scene" ("Edits change
this `<PriceTag>` only" when the scene calls `C` more than once) or "Edits
change the shared value". If that option is locked by the time the gesture ends, the
edit is refused with the lock's text, never written under the other option; a
gesture released while another write runs waits its turn and is gated the
same way when it goes; a refused
inline text edit stays open with the reason, so the user can pick an option
and commit again.

## What "Shared value" writes

The value every scene uses unless it sets its own:

- a literal at `E` (never lifted): that literal, as today;
- a prop lifted by this design, or any parameter with a literal default in
  `C` (`{ rectWidth = 200 }`): that default. Call sites that pass the prop
  keep their value; the line under the control names them. When this scene's
  call is one of them, the row is locked (see [Per row](#per-row));
- a parameter without a default, passed at every call site: "Shared value"
  is disabled ("Every scene sets `rectWidth` itself").

So after one scene sets its own value, later "Shared value" edits still reach
every other scene, and never remove a scene's own value. Removing it ("Use
the shared value") is the row's existing remove button while this scene's
call site passes the prop, under either option (its tooltip reads "Use the
shared value"; there is no second button): it deletes the attribute at `S`.

A value written at `E` changes every instance, also those whose call sets its
own value for another parameter: under "Shared value", the timeline bands and
the "Shared" row leave out the scenes that set their own only when every row
the edit writes passes through to a parameter, and show every instance
otherwise.

## What "This scene" writes

Let `E` be the element, written inside component `C`, and `S` the call of `C`
that produces the current scene's instances, found with design 2's owner call
site rule. "This scene" needs `S` to be confined to the current scene: every
occurrence of every instance whose chain contains `S` lies in this scene, by
the occurrence index.

When the current scene calls `C` more than once (an instance shown in this
scene does not go through `S`), `S` is the selected instance's own call, and
"This scene" writes it alone: the scene's other calls keep their values (a
lifted value's default). That needs `S` to draw the selected instance alone in
this scene, one copy at a time; otherwise it is locked (see
[Locks](#locks)). The impact filter is the same call-site filter, so only the
selected instance's band is filled.

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
  has that name, or `S` already writes it (forwarded through `...rest`), a
  number is appended (`rectWidth2`).
- `C`'s first parameter must be an object pattern (or absent, which becomes
  `{ rectWidth = 200 }`). Anything else is locked.
- The default is the current literal, written in the same form the literal had.
- The literal at `E` becomes the identifier.
- `C`'s props type gains an optional field (`rectWidth?: number`, by
  `EditValue` type) when `C`'s parameter has a type annotation that is a type
  literal or names an `interface`/`type` alias declared in the same file. An
  imported props type is locked. No annotation: nothing to add.
- A move writes `x` and `y`: both are lifted in one edit.
- A value written on `E` in a frame that renders `E` more than once is
  locked by design 2 ("Rendered 2 times from this element"): writing it at
  `E` would change every copy. Under "This scene", when `S` draws the
  selected instance alone in this scene, the write goes to `S` instead, so
  the element is resolved as that one instance (a second resolution with a
  render count of 1) and its rows, canvas drag and commit lift like any
  literal; its commit resolves `E` again as one instance. A gate and a
  commit read the targets of the scope they keep, not those shown now.
  Under "Shared value", for an element with no scope control, and when `S`
  draws `E` more than once, the lock stays. While a drag or a scrub of X or
  Y runs, its preview override is keyed by `E`'s position, so every copy
  draws the dragged value until the release; only `S` is written.

After lifting, later edits of the same element in another scene resolve as
case 1 at that scene's call site; the default keeps every other scene as it
was.

Lifting edits two places, often two files; it uses design 2's multi-file
commits and history entries.

## Per row

One rule decides, per row, what an edit writes under each option; the row's
shown value, its locks, its remove button and the commit all read it, so what
the panel shows is what gets written:

| Row's target | This scene | Shared value |
| --- | --- | --- |
| Literal or absent prop at `E` (attribute or children) | lift (or the lift's lock) | as resolved |
| Keyframe; a path under `style.`; a literal input (a `const` an expression reads) | locked: "Can't be set per scene yet" | as resolved |
| Pass-through, `S` passes it (an expression too) | at `S` | locked: "This scene sets its own `rectWidth`"; removable at `S` when the value there is a literal (else the default is edited from a scene that leaves it out) |
| Pass-through, `S` leaves it out | at `S` | the default |
| Pass-through without a default, every call passes it | at `S` | locked: "Every scene sets `rectWidth` itself" |
| Pass-through whose default is an expression / `props.rectWidth` | at `S` | locked: "`rectWidth`'s shared value is written as an expression" / "`PriceTag` doesn't take its props as `{ … }`" |
| Component-section row | at `S`, as resolved | at `S`, as resolved |

- Under "Shared value", a pass-through row that `S` leaves out shows and
  edits the default (no "Set at" line). A parameter without a default that
  some call leaves out gets one (`{ rectWidth }` → `{ rectWidth = 240 }`); its
  row shows the element's default, dimmed.
- When this scene's call passes the parameter itself, a "Shared value" edit
  would not show in this scene (like a settings screen that marks a value
  "modified here" and edits it where it is set). The row shows this scene's
  value, the one the canvas draws, keeps its "Set at" line naming the call,
  and is locked with "This scene sets its own `rectWidth`": its input takes no
  commit, and drag, scrub and inline text lock with the same text. When the
  value at `S` is a literal, its remove button stays, with the tooltip "Use
  the shared value": it deletes the attribute at `S`, after which the row
  shows and edits the default. A value computed at `S` has no remove button;
  the default is edited from a scene whose call leaves the prop out. A lock
  of the default itself (no default and every call passes it, an expression,
  no pattern, not found) comes first.
- A value passed at `S` for a prop the element requires (a `<Rect>`'s
  `width`) is removable when the parameter has a literal default, which then
  applies, under either option and without one; with no default it is not.
- A literal const used once inside `C` lifts like a literal at `E`: the
  `const` takes the new prop as its value.
- Component-section rows are outside the scope: written and removed at `S`.

## Removal per scope

- "This scene": a row passed at `S` deletes the attribute there ("Use the
  shared value"). A literal at `E` can't be removed (that would reach every
  scene): no remove button, and an emptied field restores the value.
- "Shared value": a literal at `E` is removed as today; a row this scene's
  call passes deletes the attribute at `S` ("Use the shared value"); a
  default is never removed.

## Locks

"This scene" is disabled, with the reason shown under the control, when:

| Case | Reason text |
| --- | --- |
| No call of `C` is found for this scene | "Can't find where `<PriceTag>` is called" (design 2's text) |
| `S` produces instances in other scenes too | "`<PriceTag>` is called here for other scenes too" |
| The scenes share one chain: `C` is called once and repeated by `.map()` or `<Scenes>` | "The scenes reuse the same call (`.map()` or `defineScenes`)" |
| More than one component between the scene and `E` | "Passes through `<Card>`" |
| This scene calls `C` more than once, and `S` draws `E` more than once in it (two instances through `S`, or copies in a `.map()`) | "`<PriceTag>` here draws this element more than once" |
| `C`'s parameter is not an object pattern | "`PriceTag` doesn't take its props as `{ … }`" |
| `C`'s props type is imported | "`PriceTag`'s props type is in another file" |
| `C`'s props type can't take a field (an alias without a type-literal member, a mapped type, another declarator annotation) | "Can't add `rectWidth` to `PriceTag`'s props type" |
| The value is a keyframe, in a `style` object, or a literal input | "Can't be set per scene yet" |
| The lift or the write at `S` goes to a file outside the project | "This file is outside the project" |
| `E` is written directly in the scene (not in a component) | not shown: the element is not shared |

The locks are checked in this order: no call; a component entry after `S` in
the chain ("Passes through": a `<Scenes>` scene function adds no entry, so a
component there sits between the scene and `C`; scenes written as components
and placed by hand, such as `<Sequence><PriceScene /></Sequence>`, count too);
an instance through `S` in another scene with a `Scenes` wrapper after `S`
(the reused-call text; `S` inside `.map()` across scenes gets the other-scenes
text); any other instance through `S` in another scene; an instance in this
scene not through `S` while `S` does not draw the selected instance alone
there. A scene that calls `C` once, through `S` drawing several copies, keeps
"This scene": every copy is in this scene. The rest are per row: the row is locked with its text,
and the option is disabled only when no row can be written under it (with the
first row's reason).

"Shared value" has only row locks, and is disabled when every row is locked
by one of these, except "This scene sets its own", which locks the row only
and never the option:

| Case | Reason text |
| --- | --- |
| A parameter without a default, passed at every call | "Every scene sets `rectWidth` itself" |
| This scene's call passes the parameter | "This scene sets its own `rectWidth`" |
| The default is an expression | "`rectWidth`'s shared value is written as an expression" |
| `props.rectWidth`: no pattern to hold a default | "`PriceTag` doesn't take its props as `{ … }`" |
| No pattern key in `C` holds the parameter | "Can't find where `rectWidth`'s shared value is written" |
| `C`'s file is outside the project | "This file is outside the project" |

A row this scene passes itself, and a drag, a scrub or an inline text edit of
it, under "Shared value", are locked with "This scene sets its own
`rectWidth`".

## When the scenes are not scanned yet

The options need the occurrence index. Until it answers, the control shows
"Edits change" and a note in place of the tabs: "Scanning scenes…" (scanning,
or a refreshing index that no longer finds `E` because a lift moved it),
"Couldn't scan scenes: …", "Not found in the scene scan" (a ready index
missing `E`), or "The scenes are not scanned" (no sweep runs).

- A value written at `S` as resolved (a pass-through row, unless "Shared
  value" is remembered, or a Component-section row) goes as today.
- Any other row commit is held only while the index is scanning (the
  refreshing case included), with "Scanning scenes…" on the row in place of
  "Saving…", and written when the sweep lands. A failed scan, a ready index
  missing `E`, and no sweep refuse with the note: no sweep would drain the
  commit.
- A held commit keeps the scope the control showed when it was made (the
  remembered choice, else the option then in effect). One made with no option
  shown is written when the sweep lands only if the option then in effect is
  "This scene"; when "This scene" is locked by then, it is refused with "The
  scenes were scanned while editing; edit again", never written to the shared
  value.
- A selection change (or leaving the interactive mode) does not wait for the
  commits held for the scan: it drops them, and the viewer says "Not saved:
  the scenes were still being scanned". Commits held for a reload are still
  waited for.
- A drag, scrub or inline text edit is never held: one that would write such
  a value does not start while the note shows (it says the note instead), and
  one under way when a scan starts is refused with the note at its end. One
  that started with no option shown (writing at `S`) and ends after the
  options are known is refused with "The scenes were scanned while editing;
  edit again".

A spread to other scenes is never written unscoped.

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
