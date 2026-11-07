# Timeline timing: known gaps (2026-11-07)

Issue mika-f/Celesta#566 collected the known gaps of timing edits on the
timeline (`docs/handoff/2026-10-26-timeline-timing.md`): the band drag, the
scene-end drag and **Set timing**. Each item is fixed, verified not to be a
gap, or kept with the reason below. #558 and #559 (both closed) and #560 track the
items split off earlier.

Paths are under `crates/editor/src/` or `crates/source-edit/src/` unless
they say otherwise.

## Scene lists and scene ends

### Fixed

- **A list with a spread or a `.map()` is read in part.** `scene_lists`
  no longer skips such a `defineScenes` call. It keeps the object-literal
  entries with string-literal ids and records where the other items sit:
  `SceneList::unread` holds, for each, the number of read entries before
  it (one `0` when the argument is not an array literal). In
  `scene_edges`, an entry with an unread item after it locks as "Entries
  after it in its list can’t be read (a spread or `.map()`)": the scenes
  after it move with the drag by an unknown amount. An entry after the last
  unread item drags as before; its `later` list is complete. A top-level
  chapter that no list has still reads "Can’t find scene `<id>` in the
  source". When some list has unread items, a second line names the file of
  the first such list: "A `defineScenes` list in `timeline.ts` has entries
  Celesta can’t read". A scene of a skipped list and a top-level
  `listChapters` chapter are told apart when no list has unread items. When
  some list does, both show the second line, which states a fact about the
  file and does not claim the chapter comes from it. In
  `examples/delta/v0.8.0/timeline.ts`, `open` (before the spread) now reads
  "Entries after it in its list can’t be read", `outro` (after it) reads
  "Exported, so other files may read it" (its length is an exported
  `const`), and the `chapter-<i>` scenes read the two lines. Tests:
  `a_list_with_a_spread_keeps_the_entries_it_can_read` (source-edit),
  `a_list_with_entries_it_can_t_read` (editor).
- **The scene end snaps to the selected element's bands.**
  `scene_snap_targets` takes the bands the composition row draws for the
  selection (`timeline::selection_ranges`, read at the press) and adds the
  edges of those that end at or before the dragged end. A band after the
  end may sit in a later scene that moves with the drag, so it is left out.
  Test: `scene_snap_targets_leave_out_the_end_and_what_moves`.
- **No "Reading source…" after every reload.** `scene_edge` (the chapter
  row's cursor, tooltip and press) shows the last edges found while the
  search for the current reload runs. "Reading source…" shows only before
  the first search lands. The release reads `current_scene_edge`, which is
  the current reload's edge or "Reading source…". `scene_write` then
  refuses a drag released before the search lands ("Not written: Reading
  source…"), as well as an edge that changed since the press. A drag
  started on the last edges therefore never writes what the current
  source doesn't hold.
- **The scene label's "before" is the entry's value.** `scene_label`
  shows the entry's `durationInFrames` as the value before. The value after
  is what the release writes (`written_length`: the visible length plus
  the drag). For a chapter the composition cuts (entry 200, visible 150,
  dragged −10), the label reads `durationInFrames 200 → 140`. Before, it
  read `150 → 140`.
- **A default or renamed import of a list is followed.**
  `SceneList::exports` lists the names the file exports a list under: `X`
  for `export const X` and `export { X }`, `Y` for `export { X as Y }`, and
  `default` for `export default X`, `export { X as default }` and `export
  default defineScenes(…)` (also with `as`/`satisfies`). A type-only export
  is left out. `composition_follows` now takes the list: an import
  (`ListDuration { module: Some(_), name }`) matches when `exports` holds
  `name`, so `import SCENES from './timeline'` (`name: "default"`, now
  documented on `CompositionLength`) follows a default-exported list. The
  entry's own binding (`module: None`) still matches `binding`. Tests:
  `a_list_knows_the_names_it_is_exported_under`,
  `composition_follows_a_list_by_the_name_it_is_exported_under`.

### Verified, not a gap

- **An import with its extension (`'./timeline.tsx'`) is followed.** Fixed
  by #702 (`scene_entries::module_files`,
  `docs/handoff/2026-11-07-editor-cleanups.md`). This change also updates
  the user guide, which still said "a path without an extension".
- **`exported_names` doesn't count `export { X } from './y'`.** In the oxc
  version the crate uses (0.156), that statement is an
  `ExportFromDeclaration`, a variant of its own, so the
  `ExportNamedDeclaration` arm never saw it. The function is now
  `local_exports` (it also gives the exported names) and skips
  `export type { … }`. A test pins the re-export case in
  `an_exported_const_duration_is_locked`.

## Element bands and Set timing

### Fixed

- **A `<Sequence>` as long as a chapter, written around the element, is
  the element's own.** `timing::written_around(read, report)` returns the
  nearest window's source when its `<Sequence>` is written directly around
  the element: same file, and the element is one of its JSX children or
  sits inside one of its `{…}` children
  (`celesta_source_edit::encloses_directly`). `window_role` takes it as
  `wrapped`: a window equal to it is `Own` whatever its span. The
  background job in `sync_element_targets` now reads `written_around` first
  and classifies the window there, then resolves an own window or tries the
  wrap. `Resolved::wrapped_window` keeps the result. `band_grips` and
  `set_timing_action` read it through `ElementEdit::wrapped_window`, which
  serves the layer's newest resolution, also while a newer one is read, so
  the role doesn't flip after a seek. Before the first resolution lands,
  such a window is still the scene's. Grips are then absent and **Set
  timing** is hidden (pending), so nothing offers a second wrap. A
  `<Sequence>` around a `<G>` that holds the element is still the scene's,
  and `add_element::added_timing` passes `None` (it counts an own window as
  a scene's anyway). Test:
  `a_sequence_written_around_the_element_is_its_own_whatever_its_span`.
- **Set timing doesn't blink out after a seek.** `set_timing_action` reads
  the layer's newest resolution, as the Properties panel does
  (`shown_scope_context`), instead of `element_targets()`, which answers
  only once the new lookup is resolved. It stays hidden before the first
  resolution, after a write (stale), and while a failed one is replaced.
  The role and the span come from the current lookup. The wrap and import
  locks come from the last resolution: they are the element's file's,
  whatever the frame. A click in the moment the new lookup is being read
  writes nothing and leaves "Not written: The source is still being read"
  (`set_timing`).
- **A file with syntax errors is not a `Sequence` clash.** `ImportLock` and
  `WrapLock` have a `SyntaxError` variant for text that doesn't parse
  (before, `Clash` and `NotAChild`). `ImportLock::text` gives the editor's
  text ("The source has syntax errors", as `LockReason::SyntaxError`
  reads). `CommitError::Import` and `AddLock::text` use it, and
  `set_timing_offer` disables with it. Tests: in
  `a_clashing_local_binding_locks`, `refuses_a_root_or_a_prop_value` and
  `set_timing_offer_follows_the_role`.
- **A selection change drops a released band drag and its notice.**
  `change_selection` (`viewer.rs`) calls
  `drop_released_timeline_drag::<BandDrag>()` and clears `band_notice`.
  The old window's grips, label and "Not written" note no longer stay on
  the composition row. The write already sent still goes. A drag still
  running (button held) is not dropped: a selection change needs a press
  elsewhere, which can't happen during a drag. A second press on the
  grips while a released drag waits for its reload, without a selection
  change, still starts nothing and falls through to the click-seek; the
  reload follows the write within a moment.
- **`later_series_items` reads items inside `{…}`.** `<Series>` flattens
  the arrays of its `{…}` children (`React.Children.forEach`) and throws on
  a fragment. So the items are now the element children and the
  `<Series.Sequence>` elements of `{…}` children (`{on &&
  <Series.Sequence>}`, `{xs.map(…)}`), in source order, and nothing inside
  a fragment or another element. An item selected inside `{…}` gets the
  items after it too. Test:
  `later_series_items_include_those_inside_braces` (also `C.Series` and an
  aliased `Series`).

### Verified, not a gap

- **A runtime that reports no `wrappers`.** The editor's preview resolves
  every `@celesta.studio/*` import from the CLI it ships with (the
  `shared-runtime` esbuild plugin in `packages/cli/src/cli.ts`), never
  from the project. A packaged editor therefore always runs a runtime that
  reports `wrappers`. Only a development build whose `packages/react/dist`
  is older than the editor can lack them; `pnpm run build:runtime` fixes
  that.

### Kept, with the reason

- **`Series` items drawn by one `.map()` move as one.** They share one
  source position, which is all the runtime reports and all the source
  holds: the drag writes that one element, so moving its bands together
  shows what the write does. The user guide says so.
- **The scene calls reach another element's window instance only through
  the selected element's instances.** Fixing it means reading the scene
  calls of every window instance, not only those whose chain ends as one of
  the selected element's does. `sync_window_scene_calls` reads the calls of
  the window's component only when it isn't the element's, keyed by the
  element's chain. Making it read per window instance changes the
  `SceneCalls` keying that `window_scope_state`, `impact_filter` and
  `gesture_gate` share. That is a change to design 3's scope reading of
  its own, better as a separate issue than inside this list of small items.
  Until then, such an instance counts as not setting its own value: it may
  appear under "Shared value is used in", and "Every scene sets" may not
  show. A write is still gated by the window's own scope at the release.
- **A window written directly in a scene component counts as not shared.**
  As the timing note says: `<Scenes>` creates the scene with
  `React.createElement(Scene)`, which records no source position, so the
  chain has no entry above the window. Treating an empty chain as shared
  would lock every such window or show options where nothing is shared.
  Only the label's "Edits change" line is lost when the scene component is
  also called elsewhere. The write is what "Shared value" writes.
- **The window job runs after the element's resolution.** Both are oxc
  parses of the element's file (and the call sites') on the background
  executor, well under a millisecond each for a component file. Running
  them as two tasks would need a second result to pair with one request
  and a state where the grips are known and the element's rows are not.
  Not worth it unless a profile shows it.

## Imports and the wrap

### Fixed

- **Blank lines inside a wrapped element are left alone.** `wrap_element`
  skips a line with only whitespace, not just an empty one. Before, it
  gained an indent unit (trailing whitespace).
- **A comment ending the last import's line stays with it.** A new import
  line goes after a `// …` or a `/* … */` that closes on that line
  (`after_trailing_comment`). A `/* …` that goes on to later lines is not
  one; the line goes right after the import, as before.
- **A file without imports gets its own quotes and semicolon.**
  `file_style` takes the quote of the first string literal outside JSX
  attributes (prettier quotes those apart), a directive included. It ends
  the line with `;` unless the first directive or statement has none.
  Without a string, `"` as before.
- **`indent_unit` skips alignment.** A line after one ending in `,` doesn't
  count as a level: `f(one,` / `            two)` aligns an argument. The
  nearest other line decides.

Tests: in `adds_a_new_import_after_the_last` (quotes, comments, CRLF),
`wraps_a_multi_line_element_re_indenting_every_line` (a line of spaces,
CRLF), `indent_unit_reads_tabs_or_one_level_of_spaces`.

### Kept, with the reason

- **A name shadowed anywhere in the file locks.** `Module::declared`
  counts every binding of the name in any scope, so a parameter named
  `Sequence` in an unrelated function locks the import ("`Sequence` here is
  not Celesta’s"). Telling whether the binding at the wrap point is the
  import needs scope analysis (`oxc_semantic`), which the crate doesn't use
  anywhere. Every other reader of `Module` (`timing_kind`, `scene_lists`,
  `composition_length`, the add-element tags) makes the same conservative
  choice. The lock never writes the wrong binding, and the case is rare in
  a composition file.

## Tests and code

### Added

- Getter and method durations, and duplicate keys in an entry
  (`a_getter_a_method_or_a_later_key_is_read_as_the_runtime_reads_it`).
- A spread after `durationInFrames` on `<Composition>`, a shadowed import,
  a namespace and a default import in `composition_length`.
- `C.Series` and an aliased `Series` in `later_series_items`.
- Inserting a name around an inline `type` member.
- CRLF in a wrap and in a new import line.
- `finish_with_window`'s state: `a_window_that_does_not_resolve_keeps_why`
  already covered it, and now passes a `WindowRead`.

### Code

- `finish_with_window` takes a `WindowRead` (targets, wrap lock, import
  lock, wrapped window) instead of six arguments.
- `collectOccurrences` (`packages/react/src/render.ts`) keeps a `Set` of
  wrapper keys per instance instead of rebuilding the key strings of every
  known wrapper for each node and frame.

### Kept

- The press-frame seek and `band_write`'s gate are still untested: both
  read `EditorView` state that needs a GPU-backed view, as the drag-session
  note says of `release_unseen`. The pure parts they call (`draggable_band`,
  `gesture_gate`) are tested.
- The chapters Vitest still compares the runtime with itself.
- The other nits stay: `SceneDrag` repeats the edge's `first`, `later` and
  `follows`; `scene_notice` reuses `BandNotice`; `scene_edge` clones per
  chapter per render; the window lookup match is written twice; the window
  scene-call state lives on `ElementEdit`; `window_report` keeps the
  element's windows, span and frames. Each is cosmetic, and changing it
  touches code this change doesn't otherwise need. The stale `scope.rs`
  doc comments the note mentioned were not identified.

## Checks

```sh
cargo test -p celesta-source-edit
cargo test -p celesta-editor
cargo clippy -p celesta-source-edit -p celesta-editor --all-targets -- -D warnings
cargo fmt --all -- --check
(cd packages/react && pnpm run test)
```

All pass: source-edit 218 tests, editor 684 tests, react 313 tests.

## Not checked on screen

The app was not run in this change; the session could not drive the
editor's window. That covers the earlier note's "Not checked on screen"
items (grip tooltip and cursor stacking, the label's clipping at the top of
the timeline, the chapter end zone against the block's click-to-seek, the
end highlight, bands flashing their old positions between the reload and
the rescan), and this change's own behavior:

- In `examples/delta/v0.8.0`, hover over the chapter ends: `open` reads
  "Entries after it in its list can’t be read (a spread or `.map()`)",
  `outro` reads "Exported, so other files may read it", and a `chapter-<i>`
  end reads "Can’t find scene `chapter-0` in the source" and "A
  `defineScenes` list in `timeline.ts` has entries Celesta can’t read".
- In `examples/scene-timing/film.tsx`, save the file and hover over a
  chapter end right after: it keeps its cursor and tooltip, with no
  "Reading source…".
- Select an element with a band inside Intro and drag Intro's end near
  the band's end: it snaps there.
- Write `<Sequence from={0} durationInFrames={90}>` directly around an
  element of Intro (90 frames): its band gets grips, and **Set timing** is
  not offered.
- With an element without a window selected, seek: **Set timing** stays.
- Release a band drag that writes nothing (a "Not written" note), then
  select another layer: the note and the old grips go.
