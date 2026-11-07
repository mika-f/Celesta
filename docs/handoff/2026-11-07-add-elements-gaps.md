# Adding elements: the known gaps (#567)

The known gaps of [adding elements](2026-10-26-add-elements.md), each
fixed or kept with its reason. The video size gap was #562 and was fixed
earlier.

## Fixed

Parent and children space (`crates/editor/src/add_element.rs`,
`crates/editor/src/preview.rs`):

- **A space request that never got an answer.** `asked` holds the parent
  and the preview generation its frame was drawn in. `AddState::take_space`
  clears it when an answer from that generation or a later one comes back
  without the asked group's space. That happens when the frame was replaced
  by a request that asked for no space, such as when the same layer is
  picked again while its lookup is stale. `ask_add_space` then asks again
  on the next render.
- **What a frame says about a group's space.** A frame reports it as
  `ChildrenSpace::{Map, Turned, Absent, Failed}` (`children_space`).
  Before, a frame that could not be evaluated, or had no such group, read
  as "Can’t draw into a group turned in 3D". Now those cases read "This
  frame couldn't be drawn" and "The group isn’t drawn in this frame". A
  failed frame counts as reported, so it is not asked for again in a loop.
  A group inside a mask is `Absent`: a mask's layers are never picked, and
  `parent_space` does not look in masks.
- **Answers from before a reload.** After a reload, a space from an
  earlier preview generation (`space_from`) is dropped. Reads of the groups
  carry the number of reloads applied when they started (`wanted`'s second
  field, `reloads`), so a read started before a reload is not taken for the
  same lookup after it.
- **A release that came early.** A release before the write's inputs are
  there no longer refuses with "The source is still being read". The inputs
  are the lookup, the groups, the parent's space (during or right after
  playback), and an Image's or a Video's measurement (`AddTool::measured`).
  Such a release goes to `AddMode::Waiting`, and `settle_add_release`
  writes it on the render that has them. Meanwhile it is outlined once the
  space is known, and the hint reads "Reading the source…" or "Reading the
  file…". Escape, a selection change, playback, and a reload end it, as
  they end the rest of the add mode.
- **A root `<Group>` that spans scenes.** `is_shared` returns `Sharing {
  calls, scenes }`. Without a scope control (`ParentScope::NoControl`, an
  element written once), `pick_parent` passes over only groups that more
  than one call chain draws. A group written once that spans several
  scenes is used, and is not marked "This scene", since an edit of the
  element itself reaches those scenes too. "This scene" still passes over
  both kinds.
- **A space that maps nothing.** A singular space still reads "Can’t draw
  into a group scaled to zero". A drawn Image or Video with no file now
  reads "Choose a file first".
- **A failed lookup.** `lookup_reason` gives "The selected layer is not in
  this frame", "… has no source position", or "Couldn’t read the selected
  layer’s source: …", instead of "Select a group, or a layer inside one,
  to add into".

Drawing and writing:

- **An Image or a Video of unknown aspect** (`known` is false) writes the
  drawn box's `height` too, so it draws the box that was outlined
  (`NewElement::{Image, Video}::height`, `Option<f64>`).
- **Cancelling during an import.** A cancel after the file dialog and
  before the import copies nothing. A cancel during the copy deletes the
  copy that import made. `celesta_react_bridge::import_file` reports that
  copy in `Imported::copied`. A reused copy of the same bytes, or a file
  already in the project, is never deleted.
- **Escape with an inline text edit open.** An Escape that ends the add
  mode also cancels the canvas text edit (`render.rs`'s key capture).
- **Opening a self-closing parent** (`celesta-source-edit`, `add.rs`)
  replaces only the `/>` and the spaces and tabs before it on its line, so
  a comment after the last attribute stays. A `/>` on a line of its own
  keeps its `>` there, at its indent. In a one-line parent, the spaces
  before the closing tag are replaced by the closing tag's own line.
- **The header and the hint** name the parent without backticks ("Add to
  <Group> in <PriceTag>"). The hint shrinks first in the tool row and is
  cut with an ellipsis.
- **The add notice** comes first in the viewer's notices.

Checks and cleanup:

- **`accepts_children`** resolves the only child's root identifier with
  `oxc_semantic` (a new dependency of `celesta-source-edit`, the same 0.156
  as the other oxc crates, which already need Rust 1.97). The child counts
  as a prop only when the identifier is bound by a component's parameters.
  A component is a function a call site can name (`function Card`, `const
  Card = …`, through `memo`/`forwardRef`, `is_component_name`) or the
  module's default export, named or anonymous. A callback's or a lowercase
  helper's parameter, and a local that shadows a prop, leave a region.
  `props['children']` and `props?.children` count as props.
- **One parse.** `read_parent` reads the element, its component, and the
  locks from one parse; `parent_facts` uses it. A file that does not parse
  gives `None`, since oxc recovers it as an empty program, so the element
  can't be found in it either.
- **Docs.** `turns_in_3d` says why a perspective alone is left out of the
  flat maps. `added_layer` says a wrapper outside its window yields the
  sibling before it, which the confirm check then clears. `parent_space`'s
  doc covers masks and a collapsed map. The worker fixture's comment no
  longer mislabels the mask.
- **New tests:** geometry in a skewed, non-uniformly scaled, and mirrored
  parent; a group's own perspective and `rotationY`, and an ancestor's
  `rotationX`; a non-uniform scale in `parent_space`; an empty parent and
  an empty wrapped group in the worker; `children_space`'s cases;
  `parent_facts`'s skips (outside the project, unreadable, unparsable, no
  element, no region); a source-less window between two windows above the
  parent; CRLF files, both import lines new at once, a comment before
  `/>`, and the binding cases in `source-edit`; the space request's
  re-ask and the reload guards; `AddMode::Waiting`.

## Kept, with reasons

- **An empty `<Group />`.** It draws no region and is in no layer's
  source chain, so there is nothing to select it by. Selecting it needs a
  layer tree or similar UI, which is a feature of its own. Opening a
  self-closing parent is implemented and tested in `source-edit`, so such a
  UI would only have to select the group. The guide says to add into the
  group around it, or to give it a child first.
- **A parent shared through `{children}`.** The element passed in is
  written once and offers no scope control, so the addition goes around
  the component's call, which is never a write into every scene unasked.
  To add into the component's own group, select that group itself (its
  **Select** button under **Source chain**). Its scope control then offers
  **Shared value**. The guide says so.
- **`is_shared` with no chain through the group.** The selected element's
  own chain goes through each of its groups. So no chain is found only when
  the element itself is not indexed, and then its own scope is unknown too.
  Treating the group as unshared could write into a shared group unasked.
  It stays an error, with design 3's note ("Not found in the scene scan").
- **The added layer's check across held commits.** When the added layer's
  selection waits for commits held for the old selection, its first
  lookup, which runs the check, comes before those commits' reload. After
  that reload the layer is kept by its id, as every selection is kept
  across an own write (`selection_keep`). An id that the reload gave to
  another element is that mechanism's limit for every selection, not only
  for added layers. When the check does run and finds the element moved,
  the selection clears rather than showing another element. A second check
  only for added layers would duplicate `selection_keep`.
- **Other texts with backticks.** The scope locks, the add locks
  ("`Rect` here is not Celesta’s"), and other editor notices still show
  literal backticks. No editor text renders them, so dropping them
  everywhere is a separate, app-wide change.

## Checks

Run on macOS (arm64) with the mbx cargo shim on `PATH`:

- `cargo test -p celesta-source-edit`
- `cargo test -p celesta-editor`
- `cargo test -p celesta-react-bridge --lib`
- `cargo test -p celesta-gpu-renderer --lib layer_frame`
- `cargo test --workspace`: passes (one test ignored, as on `main`).
- `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo fmt --all -- --check`, `git diff --check`: clean.
- `packages/website`: `vitest run tests/content.test.ts
  tests/locales.test.ts` passes.

Not verified in the app: the waiting release, the hint's ellipsis, the
notice order, Escape with a text edit open, and the cancelled import.
Check them by hand with **Select layers** on:

- Pick an Image, choose a large file from outside the project, and press
  Escape while it copies: `assets/` holds no new copy.
- Choose Rect during playback and click at once: the hint reads "Reading
  the source…", then the element is written.
- Select an element written once inside a `<Group>` around `<Scenes>`:
  the header reads "Add to <Group>", and the element lands in that group.
- Narrow the viewer with an item armed: the hint is cut with an ellipsis,
  and the toggles keep their size.
