# Editor element editing: overview

This is the umbrella for five designs that make editing a React composition
in the editor's GUI dependable. Each design has its own file, Non-goals, and
tests, and can be planned and merged on its own, in the order below.

| # | Design | File |
| --- | --- | --- |
| 1 | Selection and impact | `2026-10-23-editor-selection-impact-design.md` |
| 2 | Property editing | `2026-10-23-editor-property-editing-design.md` |
| 3 | Edit scope (this scene / all scenes) | `2026-10-23-editor-edit-scope-design.md` |
| 4 | Timing on the timeline | `2026-10-23-editor-timeline-timing-design.md` |
| 5 | Adding elements | `2026-10-23-editor-add-elements-design.md` |

## Why

The canvas transform write-back
(`2026-10-19-canvas-transform-writeback-design.md`) lets a selected layer be
moved, rotated, and scaled, writing the change into the JSX. In use it looks
capable and is not dependable, so layout still ends up done in code:

- Only a few elements can be dragged, and it is not visible why one can and
  another can't.
- Only `x`, `y`, `rotation`, and scale can change. Text content, font size,
  colors, and width/height can't.
- An edit can change scenes the user is not looking at. The `Repeated` check
  counts copies in the current frame only, so a component used in scene 1 and
  scene 3 is rewritten for both without a word.
- A component whose size is computed from its content (a label's frame sized
  to its text) can still be scaled, which overrides the computation.
- Rotation and scale turn about the layer's anchor, which is wherever its
  `x`/`y` point (the top-left corner of the box unless `anchorX`/`anchorY`
  say otherwise), while the handles sit on the bounding box and suggest its
  center.
- After a drag the Layer section blanks to "Looking up source…" until the
  reloaded code is looked up again.
- The Properties tab does not say what kind of element is selected.
- An element's start and end can't be changed, and the timeline does not show
  where the selected element is.
- Elements can't be added.

## Principles

These hold across all five designs.

1. **The TSX is the result.** Whatever the GUI changes ends up as TSX that
   reproduces it without the editor. The GUI never keeps state of its own
   that the export does not see.
2. **The scope of an edit is visible before it is made.** The user can see
   which scenes an edit changes, on the timeline and in the Properties tab,
   before committing it. Nothing changes in a scene that was not shown.
3. **An edit is about a scene, not a frame.** "Shared" means "appears in
   another scene", found over the whole timeline, not in the current frame.
4. **Computed values are edited at their inputs.** When a value is computed
   from others, the GUI offers the inputs instead of overriding the result.
5. **Correctness over compatibility.** Wrong behavior is fixed in the default
   (the rotation pivot, for one). No option keeps the old behavior.

## Terms

- **Scene**: a chapter of the composition (`<Composition chapters>`, usually
  from `defineScenes`). A composition without chapters has one scene, the
  whole timeline.
- **Element**: one JSX element in the user's source, identified by its
  `file:line:column`.
- **Instance**: one place an element renders, identified by its source chain
  without timing wrappers (the element and every enclosing element written
  with a position, see design 1). The same element called from two places has
  two instances. Copies made by `.map()` or by library code (`<Scenes>`) can
  share one chain.
- **Occurrence**: a frame range in which an instance renders.
- **Shared**: an element whose occurrences, of one instance or several, fall
  in more than one scene. Impact is always counted in scenes, not instances.
- **Write target**: the source span an edit replaces or inserts at
  (`EditTarget` in `crates/source-edit`).

## Scope (all five)

- React compositions in the standalone preview, `ReactPreviewMode::WholeScene`,
  as the write-back and selection already are.
- `.celesta` JSON projects are out. They have their own Inspector and
  timeline model. (Decided by the designer.)
- English UI labels, following the existing inspector style (radius 0, no
  gradients, no uppercase labels, lucide icons, gpui-kit components).

## Every design's pull request

- Screenshots of the same scene before and after the change (AGENTS.md).
- Tests: Vitest in TypeScript for `packages/*`, Rust unit tests for
  `crates/*`.
- `examples/delta/v0.7.0/demos/EditorDemo.tsx` draws the editor's UI in the
  demo film. Each design checks whether its change supersedes what that scene
  shows and updates it in the same pull request if so (AGENTS.md).

## Shared pieces

Built once, by the design that first needs them:

| Piece | Built in | Used by |
| --- | --- | --- |
| Timing windows in the layer source report | 1 | 1, 4 |
| Wrapper entries in the source chain | 1 | all |
| Occurrence index (whole-timeline sweep) | 1 | 1, 3, 4, 5 |
| Kept selection across the editor's own reloads | 1 | all |
| Typed edit values (`EditValue`) and primitive property tables | 2 | 2, 3, 4, 5 |
| Pass-through to component call sites (owner call site rule) | 2 | 2, 3, 4 |
| Multi-file `EditHistory` entries | 2 | 2, 3, 4, 5 |
| JSX insertion and import editing | 4 (wrap) | 4, 5 |

## Order

1 first: every other design needs to know what is selected and where it
appears. 2 next: its pass-through makes one gesture touch two files, so it
builds multi-file history. 3 after 2 (it lifts values that 2's pass-through
then finds). 4 needs 3's scope choice for windows inside shared components. 5
needs 2's property tables and 4's JSX insertion.

## Where GUI edits are stored

GUI edits are written into the user's own TSX, in place, as the write-back
does today: values where they are written, per-scene values as props passed
at the scene's call (design 3), added elements as JSX in the parent (design
5). Two other models were considered and not taken:

- **A data file the TSX reads** (like `props.json`): the hand-written code is
  never touched, but only values the code reads from that file are editable.
- **An editor-owned TSX module** imported by the hand-written code: edits to
  hand-written values would still need one of the other models.
