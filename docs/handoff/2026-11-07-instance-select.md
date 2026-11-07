# Selecting a component instance from the canvas (2026-11-07)

Implements mika-f/Celesta#556, options A and B. A click on the canvas
selects the innermost layer under the pointer, so pressing a `<PriceTag>`'s
`<Rect>` and dragging moves only the `<Rect>`; its `<Text>` stays behind.
Moving the whole instance means moving its root `<Group>`, which was
reachable only through **Source chain** > **Called from** > the `<Group>`
row's **Select**. This change makes that selection reachable in two ways:
from the component's own row in the source chain (A), and from the canvas
through a selection mode (B).

Builds on `docs/handoff/2026-10-14-open-layer-source.md` (the source chain)
and `docs/handoff/2026-10-26-edit-scope.md` (**This scene** lifting a value
into a prop). User docs:
`packages/website/content/{en,ja}/docs/01-(getting-started)/03-preview.md`.

## Specification

### An instance and its root

- An **instance** is one call of a component the user wrote: a source chain
  entry whose `kind` is `'component'`. Wrappers (`<Sequence>`, `<Scenes>`,
  `<Composition>`, ...) are never instances.
- An instance's **root** is the one layer its call draws everything in: the
  call draws exactly one top-level layer, meaning one host node with a layer
  that is not inside another host node under the call. Hosts that draw no
  layer (`<Audio>`) don't count, and the host a wrapper (`<Sequence>`,
  `<FreezeFrame>`, ...) renders around its children is looked through, so a
  component returning `<Sequence><Group>…</Group></Sequence>` has the
  `<Group>` as its root. A component that renders a fragment of several
  layers, or no layer (`<Font>`), has no root and cannot be selected as an
  instance. A component whose root is another component's call (`PriceTag`
  returning `<Tag>`) shares that call's root.
- The root must also be written in the source: it is a primitive entry
  (`<Group>`, `<Rect>`, ...) of the same chain. A package component such as
  `<Stack>`, `<SafeArea>`, or `<Panel>` draws its group with no source
  position, so neither the component nor a user component returning it
  directly is an instance, and its **Called from** row keeps no **Select**.
- Selecting an instance selects its root layer, the same selection as the
  root's own **Select** in **Called from**. Dragging it, **This scene**
  lifting `x`/`y` into a prop of the component, and **Shared value** keep
  their current behavior. Nothing new is written to the source.

### A: Select on a component's row

- In **Source chain** > **Called from**, a component's row shows **Select**
  (tooltip "Select this instance") when the runtime reported its root. It
  selects the root layer.
- Primitive rows keep **Select** ("Select this layer"); wrapper rows show
  none, as before. A runtime older than this change reports no root for a
  component, so its rows show no **Select**.

### B: the Select [Layer | Instance] tabs

- The canvas tool row gains **Select** [Layer | Instance] tabs, shaped like
  **Rotate about** [Center | Pivot]. **Layer** is the default and behaves as
  before. The choice lasts for the editor session, like the other tool row
  tabs.
- With **Instance**, a click selects an instance's root instead of the
  layer under the pointer:
  - The **context** is the instance just outside the current selection:
    the nearest instance in the selection's chain whose root is not the
    selection itself. It is derived from the selection each time; the
    editor stores no context of its own.
  - With no context, or when the click lands outside the context's root,
    the click selects the **outermost** instance root in the hit layer's
    chain.
  - When the click lands inside the context's root, it selects the
    outermost instance root inside the context.
  - When no instance is left to pick, the click selects the hit layer
    itself.
- **Double-click** on a layer inside the selected instance descends one
  level: the selected instance becomes the context, and the rule above
  picks inside it. A double-click on a selected text layer itself still
  edits the text on the canvas.
- **Cmd+click** (macOS) or **Ctrl+click** (other platforms) selects the
  innermost layer under the pointer even with **Instance**.
- A press on the selected instance's body still starts a drag of the
  instance, as a press on a selected `<Group>` does today.
- A click on no layer clears the selection, so the next click starts from
  the outermost instance again.
- **One frame of the inner layer.** A hit test finds only the innermost
  layer; its chain arrives with the next frame's source lookup. With
  **Instance**, the click first selects the hit layer and marks the
  selection as waiting to move. When the lookup answers, the selection moves
  to the instance the rule picks, and that layer's own lookup follows. In
  between, the inner layer's outline shows for one frame. When the lookup
  fails or finds no chain (no source positions), the selection stays on the
  hit layer.

### Example

`examples/scene-timing/film.tsx`: with **Instance**, a click on the Intro
price tag's `<Rect>` selects `PriceTag`'s root `<Group x={880} y={560}>`.
Intro and the other scenes come from `defineScenes`, not from JSX, so they
are not in the chain. Dragging the tag moves the `<Rect>` and the `<Text>`
together. With **This scene**, only Intro's tag moves (`x`/`y` are lifted
into `PriceTag` props and written on Intro's call). With **Shared value**,
both tags move. In an entry that calls `<Film>` in JSX, the first click
selects `<Film>`'s root, and a double-click descends into it.

## Implementation

- `packages/react/src/source-chain.ts`: `sourceFibers` gives a
  `'component'` entry `layerId` when `layerIdOf` is given and
  `topHostNodes(fiber)` has exactly one node with a layer id. That function
  returns the host nodes at or below the fiber that are not inside another
  host node there, looking through a wrapper's own host down to the elements
  with source positions. Afterwards, a component's `layerId` is dropped
  unless a primitive entry of the same chain has it (the root must be
  written in the source). The walk runs only when ids are asked for, so the
  `renderCount` and occurrence walks keep their cost. The bridge's
  `LayerSource.layer_id` (`crates/react-bridge/src/types.rs`) is already
  optional, so the protocol does not change.
- `crates/editor/src/layer_select.rs`:
  - `parent_layer_id` returns a component entry's `layer_id`.
  - `instance_target(hit, selected, selected_id, descend)` implements the
    pick rule. It finds the context: the first instance in the selected
    report whose root is not `selected_id`, or `selected_id` itself when
    `descend`. Then it finds the context's root among the hit chain's
    entries and picks the outermost component entry with a `layer_id` inside
    it, skipping entries that share the context's root. It returns `None`
    when the hit layer should stay selected.
  - `PickMode`, `Press`, and `press_pick(mode, secondary, double)` decide how
    a press picks.
  - `PendingInstance` and `pending_instance_fate(pending, selected, lookup)`
    decide where a waiting pick moves the selection once the lookup
    answers.
- `crates/editor/src/layer_panel.rs`: `source_row` shows "Select this
  instance" for a component row.
- `crates/editor/src/transform_drag.rs`: `begin_transform_drag` takes
  `allow_move`. The pure `drag_handle` refuses only the body's Move when
  `allow_move` is false, so a double-click going inside an instance drags
  nothing by its body while its handles and pivot cross still drag.
- `crates/editor/src/view.rs` / `viewer.rs`:
  - `pick_mode`, `pending_instance`, and `instance_due` are new fields.
    `tool_row` draws the Select [Layer | Instance] `ToolTab`s after the Add
    menu, always while the row shows.
  - `select_layer_at` computes the `Press` with `Modifiers::secondary`. It
    passes `allow_move` to the drag, then selects the hit layer. With
    `Instance`, it stores a `PendingInstance`, or, when the hit layer was
    already selected and its lookup answered (no lookup is coming), puts the
    pick in `instance_due` at once.
  - `change_selection` keeps the pending pick only when the new layer is its
    hit; `clear_layer_selection` drops both.
- `crates/editor/src/background.rs`: in `show_preview_result`'s
  `LookupFate::Show` arm, the pending pick is taken and resolved into
  `instance_due`. `settle_row_requests` (`element_rows.rs`), which runs on
  render with a `Context`, passes it to `select_layer`, which waits for held
  row commits like any person's selection.

## Checks

Run on macOS (Apple silicon), FFmpeg 8 through `PKG_CONFIG_PATH`:

- `pnpm --dir packages/react test`: 44 files, 321 tests pass. New tests:
  `source-chain.test.mts` (a component's single root, a component returning
  another, a wrapper and empty children, several layers, wrapper entries,
  a root not written in the source) and `layer-source.test.mts` (a root
  inside a real `<Sequence>` and `<FreezeFrame>`).
- `cargo test --workspace`: 2369 passed, 0 failed, 1 ignored
  (`write_web_effect_fixtures`, which writes fixtures on purpose). The new
  editor tests are `tests/instance_select.rs`,
  `select_parent_layer_or_instance_root`, and
  `a_press_that_may_not_move_still_starts_handle_drags`.
- `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo fmt --all -- --check`, and `git diff --check` are clean.
- `pnpm run build:runtime` succeeds. `pnpm --dir packages/website run check`
  reports no issues; its 64 warnings are missing OGP images that predate this
  change.
- A whole-change review found two defects, both fixed with a test that
  failed first:
  - A component whose root is inside a real `<Sequence>` had no root.
  - A second press within the double-click window skipped handle drags.
- The PR review (cubic) fixed these, with tests in `tests/instance_select.rs`
  where the logic is pure:
  - A pick now waits for its own hit's lookup (`take_pending_for`). A
    selection change waiting for a held commit no longer loses it.
  - Cmd/Ctrl+click and a switch of the pick mode cancel a waiting pick
    (`cancel_instance_pick`).
  - A fast double-click on an unselected instance goes inside what its
    first press picks (`merge_pending`, `then_descend`).
  - A resolved pick (`InstanceDue`) selects its target only while its hit
    is still the selection, so an added layer selected in the same render
    wins (`due_target`).
  - A click on the selected layer resolves at once only from a lookup for
    this frame (not `source_lookup_stale`).
  - `topHostNodes` is a generator, and `soleRootLayer` stops at a second
    root.
  - The review's claim that an old runtime without `kind` shows "Select this
    instance" on a primitive does not hold: `kind` and `layerId` came in the
    same change (#313), so such a runtime sends no `layer_id` either.
- Still open, minor:
  - **Select this instance** can be a no-op when the root is already
    selected.
  - The hit-itself check reads `chain[0]`'s id for layers built by a host.
  - A report is cloned on every press.
- Not run in this session: the hands-on check in the editor
  (`examples/scene-timing/film.tsx` with **This scene** and **Shared
  value**) and the before and after screenshots. This session had no tool to
  drive the app's window. After the review fixes, `cargo test -p
  celesta-editor` passes 713 tests and `pnpm --dir packages/react test` 321.
