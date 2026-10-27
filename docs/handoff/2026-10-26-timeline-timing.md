# Timing on the timeline (2026-10-26)

The selected element's timing and a scene's length are changed by dragging on
the timeline and written into the TSX. The filled band at the lookup frame
takes grips at the edges of the element's own `<Sequence>` or
`<Series.Sequence>` window: the start edge, the end edge and the body write
`from`/`offset` and `durationInFrames` by the spec's table. A chapter's end
on the Chapters row writes its `defineScenes` entry's `durationInFrames`. An
element without a window of its own gets a "Set timing" button that wraps it
in `<Sequence from durationInFrames>` from the playhead to the scene's end,
importing `Sequence`. Drags snap, write once at mouse-up, and are one history
entry each.
Design: `docs/superpowers/specs/2026-10-23-editor-timeline-timing-design.md`
(part 4 of `docs/superpowers/specs/2026-10-23-editor-element-editing-overview-design.md`).
It builds on design 1 (`docs/handoff/2026-10-23-selection-impact.md`: timing
windows, bands, the occurrence index), design 2
(`docs/handoff/2026-10-23-property-editing-engine.md`: the resolver,
pass-through, multi-file history, `record_own_writes`) and design 3
(`docs/handoff/2026-10-26-edit-scope.md`: `Edit::Splice`, `commit_scoped`,
`gesture_gate`, `scope_in_effect`, `SceneCalls`).
Plan: `docs/superpowers/plans/2026-10-26-editor-timeline-timing.md`.
User docs: `packages/website/content/{en,ja}/docs/01-(getting-started)/03-preview.md`
("Change timing on the timeline"; "See what an edit changes" and "Undo" point
to it).
Example: `examples/scene-timing` (a literal `defineScenes` list, a hand-placed
`<Sequence>`, one with an expression `from`, and a component two scenes draw).

## What changed

- Runtime (`packages/react/src/source-chain.ts`, `render.ts`;
  `crates/react-bridge/src/occurrences.rs`): `occurrenceKeys` also returns
  the positions of the chain's `'wrapper'` entries as `wrappers`, innermost
  first; each occurrence instance keeps their union over frames (the
  instance key stays the chain), and `InstanceOccurrences.wrappers`
  (`serde(default)`) is unioned by `merge`. Every transparent component is a
  wrapper, so the list holds `<Composition>`, `<Series>` and `<Scenes>`
  positions too; a `Series` item's innermost wrapper is the item's position.
- Source edits (`crates/source-edit/src/{timing,scenes,imports,insert}.rs`,
  `resolve.rs`, `inputs.rs`, `scope.rs`, `pass_through.rs`, `lib.rs`):
  `timing_kind(text, element)` reads `Sequence` / `Series.Sequence` through
  the file's `@celesta/react` import (aliases, `import * as C`; a name the
  file declares more than once is `None`); `later_series_items` lists the
  `<Series.Sequence>` siblings after an item (direct element children).
  `resolve_unchecked` reads a wrapper's prop without a received value.
  `scene_lists(text, path)` returns each `defineScenes([...])` list (a call
  of `@celesta/react`'s, through the file's import) of object
  literals with string-literal ids (anything else skips the list) with each
  entry's duration resolved as a prop value (module-level `const`s resolve for
  a value-anchored entry; an exported one locks as `LockReason::Exported`, an
  absent one as `LockReason::NoSceneDuration`); `composition_length` reads how
  the first `<Composition>` writes `durationInFrames` (`ListDuration { module,
  name }` or `Other`). `parses_file`/`parse_checked_as` parse by extension.
  `imports.rs` holds the one import reader (type-only specifiers kept) and
  `ensure_imports`/`ensure_import` (`ImportName::{Named, Namespace}`,
  `ImportLock::Clash`); `insert.rs` holds `wrap_element` (`WrapLock::NotAChild`)
  and `indent_unit`. `PACKAGE` is public and re-exported as `CELESTA_REACT`.
- Window classification and Set timing (`crates/editor/src/timing.rs`):
  `window_role` → `WindowRole { Own, Scene, Remapped, None }`;
  `set_timing_span`; `set_timing_offer` → `SetTimingOffer { Hidden,
  Disabled(reason), Enabled { from, duration } }`; `set_timing_files` (find
  the element again, `ensure_import` + `wrap_element`, one checked text).
- Window targets (`element_edit/resolve.rs`, `element_edit/commit.rs`,
  `element_edit.rs`): `RowRead { Received, Unchecked }` on `resolve_rows`;
  `window_report` (the chain from the wrapper entry outward); `resolve_window`
  → `WindowTargets { source, kind, targets, followers }`; `window_files`
  (`commit_scoped` re-resolving with `resolve_window`). The background job
  beside `sync_element_targets` classifies the window, resolves an own one
  (`Resolved.window_targets: Option<Result<WindowTargets, String>>`, a
  remapped one as an error), and for a scene's window or none runs a dry
  `wrap_element` and `ensure_import(…, "Sequence")` (`Resolved.wrap_lock`,
  `Resolved.import_lock`); `finish_with_window` carries them.
  `sync_window_scene_calls` reads the scene calls of the window's component
  when it isn't the element's. `apply_checked` takes the path it writes;
  `CommitError::Import`.
- The band drag (`crates/editor/src/timeline_drag.rs`, new): pure helpers
  (`draggable_band`, `grip_at`, `grip_lock`, `grip_refusal`,
  `adds_duration`, `timing_values`, `clamp_band_delta`, `band_edit`,
  `band_writes`, `drag_frames`, `snap_targets`, `still_ranges`,
  `snapped_delta`, `preview_occurrences`, `drag_label`, `band_label`,
  `drag_end`, `not_written`) and the view's `BandDrag`,
  `BandGrips`, `BandNotice`, `band_grips`, `begin/continue/end/escape_band_drag`,
  `commit_band_drag`, `band_write`, `drop_released_band_drag`,
  `set_timing_action`, `set_timing`. `impact.rs`: `moved_impact_view` draws
  the bands from previewed occurrences while a drag runs.
- The scene drag (`crates/editor/src/scene_entries.rs`, new):
  `find_scene_sources`, `composition_follows`, `scene_edges` → `SceneEdge {
  Editable { file, duration, first, later, follows }, Locked(reason) }`,
  `clamp_scene_delta`, `written_length`, `writes_nothing`,
  `preview_chapters`, `scene_snap_targets`, `scene_files`,
  `refresh_scene_sources` (background, generation-guarded, started where
  `react_reload_generation` is bumped and on the first load); `SceneDrag` in
  `timeline_drag.rs`. `source.rs` shares its walk as `source_files`.
- The timeline (`timeline.rs`, `playback.rs`, `render.rs`, `refresh.rs`,
  `view.rs`): grip overlay over the band (`ResizeLeftRight` on enabled edges,
  pointer on the body, `OperationNotAllowed` plus the reason on locked ones);
  the chapter end zone; the "Set timing" `Button` at the start of the band
  holding the lookup frame (tooltip "Animations inside start at the new
  start", or the disabled reason); the drag label and notice drawn outside the
  lane's `overflow_hidden`. `drag_move`/`drag_end`/Escape route both drags;
  starting playback drops a pressed one; any mouse press clears the notices;
  a reload drops a released drag.

## Decisions

These settle what the spec left open, or differ from it; the spec has been
amended to match.

- **Grips sit on the window.** They are drawn from `windows[0]` (moved by a
  running drag), over the filled band in the current scene that holds the
  report's lookup frame; other bands don't drag. A press that is a click
  seeks to the start of the drawn band holding the pressed frame, as a click
  on the band does.
- **What the preview moves.** Every instance whose `wrappers` hold the
  window's source moves, and for the end edge and the body, every instance
  through a later item of the same `Series`. A range that reached the old
  window end keeps reaching the window's end for either edge and either sign
  of the delta: the band must not shrink from the right while its start is
  dragged left.
- **A body drag on a `Sequence` without `durationInFrames` writes it** at the
  length shown: the runtime keeps such a window's end at its parent's end.
  The label names it (`from 30 → 45 · durationInFrames 60`; `band_label`).
- **Band drags use the window's own scope.** The state comes from
  `scope::window_scope_state` over the window's targets and
  `window_scene_calls`, read at the lookup frame (the frame the targets
  answer, as `resolved_scope_state` reads the element's) and cached per
  render behind the resolution, the window, the occurrence generation and
  `scene_calls_version` (bumped when the window's calls land too). A drag
  that would be refused does not start, as for every other gesture since
  design 3: `grip_refusal` locks a grip with design 2's lock of a prop it
  writes, else with `gesture_start_refusal` over those props (the body's
  added `durationInFrames` included) under the remembered choice and that
  state, so the grip shows the not-allowed pointer and the reason. The
  scope is captured with `commit_scope` at the press and gated by
  `gesture_gate` at release (the scanned-meanwhile refusal included) for what
  changes meanwhile; the label's second line is `scope_in_effect`. The
  element-level `gesture_refusals`/`kept_scope`/`gesture_gate_for` read the
  element's rows, where the window's keys never appear.
- **A window's sharing is its own.** The window's instances are every
  index instance, of any element, whose `wrappers` hold its source; it is
  shared when one of them comes through another call of the component it is
  written in: its chain doesn't end with the selected chain above the
  window (wrapper entries left out). So a `Delayed` that two scenes call
  around different elements is shared although the selected element has
  one instance, and a `<Sequence>` written once around a shared `Tag` is
  not. "Spans more than one scene" was the alternative; it misses a wrapper
  component called twice in one scene and one call reached by two paths.
  The options are design 3's (`confined`, `row_lock`) over those instances;
  the scene calls read for the element's instances are carried to each
  window instance whose chain ends as one of them does from the call.
  A window's options leave `called_here` out and take `drawn_alone` as
  true: its instances belong to every element it wraps, so they don't line
  up with the selected chain for counting the scene's calls of the
  component, and two elements in one call are not one call drawing it
  twice. In a scene calling the component more than once, "This scene"
  then writes the selected call only, and the drag label reads "Edits
  change this scene".
- **A default is a value.** A window prop passed through to a parameter the
  call leaves out reads as the parameter's default (`number_of`), so a drag
  writes default + delta; a defaulted `durationInFrames` is not added by a
  body drag (`adds_duration` fires for `Missing { default: None }` only).
  "Set timing" and the scene-end drag read no defaults.
- **Nothing is held.** A gate that holds or refuses, a window no longer
  current at release, and a write that fails leave a notice "Not written: …"
  beside the grip (the scene-end drag's too). The released drag stays drawn
  until the write's reload (dropped at once when the write changed nothing).
- **Snap targets** leave out the window's own edges (and a playhead or chapter
  edge sitting on them) and the bands that move with the drag. A scene's end
  snaps to the playhead and the chapters that stay; its own end and the later
  entries' edges are left out.
- **Scene lists.** An exported `const` duration locks ("Exported, so other
  files may read it"): a drag must not silently move importers; such grid
  constants are edited in the text editor. An entry without
  `durationInFrames` locks ("Scene `<id>` has no durationInFrames") rather
  than inserting one (`defineScenes` throws on it). An expression locks with
  its inputs, one per line. A scene entry is not a component prop, so design
  3's scope does not gate a scene drag.
- **Scene bounds.** The written value is the chapter's visible length plus the
  delta; a chapter cut at the composition's end dragged back to exactly its
  entry's length writes nothing (no reload would come to drop the drag). The
  floor is the list's first chapter's `from` (the earliest of the dragged and
  later chapters when the first isn't listed): a floor from the dragged and
  later chapters alone would block shrinking when a later entry overlaps
  back. A length may reach 0.
- **The composition follows the scenes** across files only through a named
  import with a relative specifier resolving to `.ts`, `.tsx`, `/index.ts` or
  `/index.tsx`; anything else clamps.
- **Set timing.** Hidden for an own or remapped window and while the
  element's resolution is stale or pending (a misleading reason costs more
  than a moment without the button); disabled, in order, by the wrap lock,
  the import lock, less than one frame after the playhead ("No frame left
  after the playhead"; design 5's add clamps to one frame instead, since an
  addition must land somewhere), and the scope: enabled when not shared or
  under "Shared value", "Can't be set per scene yet" under "This scene", the
  index's note when it can't answer. A click pauses playback first. A
  failed write leaves "Not written: …".
- **Only a JSX child is wrapped**: an element inside an expression container
  (`{cond && <Rect />}`, `.map(() => <Rect />)`) is `NotAChild` ("Wrap the
  element in a parent first"); users wrap those by hand.
- **One import reader.** The reader that `timing_kind` used moved to
  `imports.rs` and gained type-only specifiers for the clash rule; there is
  no second one. Unparsable text gives the single-variant locks
  (`Clash`/`NotAChild`); both callers act only after a successful resolve.
- **Classification is signed and rounded.** A `<Sequence>` may start before
  frame 0, so spans are signed; `set_timing_span` returns `None` when under
  one frame is left.
- **Parsing by extension.** `resolve_value_at`/`inputs_at` take the program
  parsed by the file's extension, so a `.ts` scene file with `<number>v`
  doesn't lock every entry as a syntax error.

## Known gaps

- A `defineScenes` list with a spread or a `.map()` (such as
  `examples/delta/v0.8.0/timeline.ts`) is skipped, so its chapters lock as
  "Can't find scene `<id>` in the source". A part (a chapter held inside
  another) that no list has locks as "Not a `defineScenes` entry"; a
  top-level chapter listed with `listChapters` can't be told from a scene
  of a skipped list and keeps "Can't find scene".
- A runtime that reports no `wrappers` leaves every window "Not found in the
  scene scan", so no grip drags.
- The scene calls reach a window instance of another element only through
  an instance of the selected element with the same call; others count as
  unread (not setting their own value), which can show a scene under
  "Shared value is used in" that sets its own, and miss "Every scene sets".
- A window written directly in a scene component has no chain entry above
  it: `<Scenes>` creates the scene with `React.createElement(Scene)`, which
  records no source position. The window is treated as not shared, so when
  that scene component is also called explicitly elsewhere (`<Intro />`
  inside Main, say), a drag moves both renders and the label has no "Edits
  change" line. "This scene" would be locked there anyway (no call), and the
  write is the literal "Shared value" would write; only the label line is
  lost. A fallback for an empty chain above the window was considered and
  not taken: it would either lock every window in a scene component or show
  options where nothing is shared.
- `Series` items written once and rendered by a `.map()` share one source
  position: their bands move as one, and a drag writes the one element every
  item comes from. `later_series_items` counts direct element children only,
  so followers inside a fragment or `{…}` aren't previewed moving.
- Chapters placed inside a later scene (`listChapters(plan, { from:
  scenes[1].from })`) don't move with a scene-end drag and don't bound it.
  Shortening the last scene while the composition follows can make the next
  reload fail (`compositionChapters` throws for a chapter at or after the
  end); undo recovers. No example uses the pattern.
- A scene's end doesn't snap to the selected element's band edges.
- After every reload each chapter end shows "Reading source…" until the
  scene search lands.
- An import written with its extension (`'./timeline.tsx'`) is not followed:
  the composition is treated as fixed and the drag clamps.
- A hand-written `<Sequence>` whose span equals a chapter's counts as the
  scene's: it has no grips and its band offers "Set timing", which would wrap
  the element in a second `<Sequence>`.
- "Set timing"'s tooltip says animations inside start at the new start; that
  holds for components inside the wrapped element that read the frame, not
  for props of the element computed outside it (the user docs say so).
- "Set timing" blinks out while a new resolution runs after each seek.
- A file that doesn't parse reads as a `Sequence` clash in the dry check
  (cosmetic; nothing is offered on such a file anyway).
- A released band drag survives a selection change until the reload (old
  grips and label on the composition row); a second press while a released
  drag waits starts nothing and falls through to the click-seek. The notices
  clear on any press, not on a selection change.
- The scene label's "before" shows the visible length of a cut chapter, not
  the entry's value.
- `exported_names` counts a re-export with a source (`export { X } from
  './y'`) as a local export.
- The window job runs after the element's resolution in the same background
  task (a small delay before the grips unlock).
- Import editing: whitespace-only lines inside a wrapped element gain the
  indent unit; a trailing comment on the last import moves after a new import
  line; a new line in a file with no imports always uses `"` and `;`;
  `indent_unit` can pick an alignment as the unit; a name shadowed anywhere in
  the file locks.
- Default imports of a scene list read as `ListDuration { name: "default" }`
  (undocumented); a list binding is keyed by its init call's start.
- Not checked on screen: grip tooltip and cursor stacking, the label's
  clipping at the top of the timeline, the chapter end zone against the
  block's click-to-seek, the end highlight, and whether bands flash their old
  positions between the reload and the occurrence rescan.
- Untested: getter or method durations, a duplicate key, a spread after
  `durationInFrames` on `<Composition>`, `C.Series` and an aliased `Series`
  in `later_series_items`, a shadowed import in `composition_length`,
  inserting before an inline `type` member, CRLF in a wrap or a new import
  line, the state wiring of `finish_with_window`/`window_targets`, the
  press-frame seek, and `band_write`'s gate. The chapters Vitest compares
  the runtime to itself.
- Code nits: the occurrence index rebuilds key strings per node per frame to
  dedupe wrappers (a `Set` would do); `SceneDrag` duplicates the edge's
  fields; `scene_notice` reuses `BandNotice`; `scene_edge` clones per chapter
  per render; the window lookup match is duplicated; `finish_with_window`
  takes six arguments; window scene-call state lives on `ElementEdit`;
  `window_report` keeps the element's windows, span and frames; a few
  `scope.rs` doc comments are stale.

## What design 5 builds on

- `ensure_imports(text, module, names)` adds several names in one edit, and
  `wrap_element`/`indent_unit` insert JSX; `CELESTA_REACT` names the package.
- `CommitError::Import` gives "`<name>` here is not Celesta's".
- `ElementEdit::finish_with_window` and `Resolved` carry `wrap_lock` and
  `import_lock`; the Escape chain and the timeline press handler route the
  band and scene drags.

## Verify by hand

In the editor, with **Select layers** on, open `examples/scene-timing/film.tsx`.
For each check, take before and after screenshots of the same frame, for the
pull request, and keep the TSX diff.

- Select "A Sequence of its own" in Intro. Drag the band's end edge right by
  15 frames: the label reads `durationInFrames 40 → 55`, and the diff is
  `durationInFrames={55}`. Undo, then drag the start edge right by 10:
  `from={40}` and `durationInFrames={30}`, the end unmoved. Undo, then drag
  the middle: only `from` changes. Hold Shift: no snapping; without it the
  edges snap to the playhead and the chapter edges. Press Escape mid-drag:
  nothing is written.
- Select the circle in Main: the start edge and the middle show the
  not-allowed pointer and "Written as an expression: LANDING - 30"; the end
  edge drags.
- Drag Intro's end on the Chapters row right by 30: the label reads
  `durationInFrames 90 → 120`, the diff changes only Intro's entry, Main and
  Outro move, and the composition becomes 330 frames long. Undo.
- Replace `SCENES.durationInFrames` on `<Composition>` with `300` and drag
  Intro's end right: it stops at +89 with "The composition is 300 frames
  long" in the label; the write is `durationInFrames: 179`.
- Scene-end locks: write Intro's length as `90 * 1` (tooltip "Written as an
  expression: 90 * 1"), as `INTRO` read from `export const INTRO = 90`
  (tooltip "Exported, so other files may read it"); duplicate the `main` id
  in a second list ("Can't find scene `main` in the source"); right after a
  save, the ends read "Reading source…". (An entry without a length makes
  `defineScenes` throw, so its lock can't be seen on a loaded composition.)
- Select Intro's background and put the playhead 45 frames into Intro;
  click **Set timing**: the `<Rect>` is wrapped in `<Sequence from={45}
  durationInFrames={45}>`, indented one level, the import unchanged (the
  file already imports `Sequence`), and one undo removes the wrapper. In a
  scene file that doesn't import `Sequence` yet, the same click also adds it
  to the `@celesta/react` import, and one undo removes both.
- Select the "Scene timing" title: **Set timing** is disabled ("Wrap the
  element in a parent first"). Select the `<Group>` around `<Title />` from
  **Called from** and click **Set timing**: the title's fade now starts at
  the playhead.
- Select the tag's `<Rect>` (Intro and Main): under **This scene**, **Set
  timing** is disabled ("Can't be set per scene yet"); under **Shared
  value**, it wraps the `<Rect>` inside `Tag`, and both scenes change. Then,
  in Intro, click **This scene** again (the choice of **Shared value** is
  remembered for the element) and drag the middle of that new band: the
  label's second line reads "Edits change this scene", `Tag` gains a
  `sequenceFrom` prop defaulting to the old `from`, and Intro's `<Tag>` call
  passes the new value; Main is unchanged, and one undo restores both files.
- Right after that write, while "Edits change" reads "Scanning scenes…", the
  tag's grips show the not-allowed pointer with "Scanning scenes…" and a
  press starts nothing. Once scanned, choose **Shared value** in Intro (whose
  call now passes `sequenceFrom`): the start edge and the middle are locked
  with "This scene sets its own `sequenceFrom`", and the end edge drags.
- After that "This scene" lift in Intro, select the tag's `<Rect>` in Main
  (whose call doesn't pass `sequenceFrom`) and drag the middle of its band
  right by 15: the label starts from `sequenceFrom`'s default in `Tag`, not
  0, and under **This scene** Main's `<Tag>` call gains `sequenceFrom` set
  to that default plus 15.
- Write a component `const Delayed = ({ children }) => <Sequence
  from={30}>{children}</Sequence>`, call it in Intro around a `<Rect>` and
  in Main around a `<Text>`, and select Intro's `<Rect>`: the Properties tab
  shows no scope options, yet the band's drag label has "Edits change this
  scene", and the drag adds `sequenceFrom` to `Delayed` and passes the value
  at Intro's call; Main's text doesn't move.
- Drag the middle of a `<Sequence>` without `durationInFrames`: the label
  reads `from 30 → 45 · durationInFrames 60`, and both props are written.
- Click a grip without moving: the playhead jumps to the band's start; click
  a chapter's end: it jumps to the chapter's start.
