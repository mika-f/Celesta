# Selection impact (2026-10-23)

When a layer is selected in the editor's standalone React preview, the
Properties tab and the timeline show what it is, when it is on screen, which
scenes an edit of it would change, and why its props can't move. The
selection no longer blanks the Properties tab after the editor's own write.
Design: `docs/superpowers/specs/2026-10-23-editor-selection-impact-design.md`
(part 1 of `docs/superpowers/specs/2026-10-23-editor-element-editing-overview-design.md`).
Plan: `docs/superpowers/plans/2026-10-23-editor-selection-impact.md`.
User docs: `packages/website/content/{en,ja}/docs/01-(getting-started)/03-preview.md`
("See what an edit changes").

## What changed

- Runtime source chain (`packages/react/src/source-chain.ts`,
  `components.ts`): every component passed to `markTransparent`
  (`Composition`, `Sequence`, `FreezeFrame`, `Loop`, `MotionBlur`, `Trail`,
  `FrameChildren`) is reported as a `'wrapper'` entry; primitives are checked
  first, and a user component named `Sequence` stays `'component'`. The bridge
  reads it as `ElementKind::Wrapper`; older runtimes never send it.
- Timing windows: the `layerSource` reply carries `windows` (each timing
  wrapper around the innermost element, nearest first: kind, `from`/`to` in
  composition frames, its source position, and `remapped` inside a `Loop` or
  `FreezeFrame`) and `span`, where all windows overlap, cut to the
  composition. Bridge: `LayerSourceReport::windows` / `span`
  (`crates/react-bridge/src/types.rs`).
- Occurrence sweep: `MountedComposition.collectOccurrences(from, frames)`
  (`packages/react/src/render.ts`), the CLI request
  `collectOccurrences { from, frames, preview }`, and
  `ReactBridge::collect_occurrences`. Each frame is walked as `layerSource`
  walks it and every layer's chain, minus wrappers, is recorded: per element
  (keyed by the first non-wrapper entry), per instance (the whole chain),
  `[from, to)` frame ranges and `max_per_frame`. `OccurrenceIndex::merge`
  joins chunks; `instance_for(report)` finds the selection's element and
  instance; `scenes_of` and `outermost_chapter_at`
  (`crates/react-bridge/src/occurrences.rs`) map ranges to scenes.
- Editor worker (`crates/editor/src/occurrences.rs`): while Select layers
  (interactive mode) is on, `refresh_occurrences` asks a worker thread to
  spawn its own `@celesta/cli` process with source locations and sweep the
  composition in `CHUNK_FRAMES` (240) frame chunks: after every successful
  reload of a React entry, when Select layers is turned on, and when another
  file opens with it on. Turning it off drops the index (`occurrences =
  None`). A result for an older reload, or one that lands after Select layers
  was turned off, is dropped (`accepts`). After the editor's own write the
  last ready index stays in use as `OccurrenceState::Refreshing` until the
  new sweep's result (ready or failed) replaces it (`rescan_state`), so the
  Shared row, the impact line, the bands, and the chapter underlines do not
  blank after each nudge; outside reloads start from `Scanning`. That sweep
  is held back (`occurrence_sweep_after`) until the preview draws the
  reloaded code, so it does not compete with the reload for CPU.
- Impact (`crates/editor/src/impact.rs`): `impact_view` turns the report and
  the index into `ImpactView::Known(Impact)` (scenes it appears in, other
  scenes, instance count, places in the current scene, call sites, filled and
  outlined bands) or `SpanOnly` with a note ("Scanning scenes…",
  "Couldn't scan scenes: …", "Not found in the scene scan") and the
  report's `span`. `impact_line` gives the line above Transform.
- Properties tab (`layer_panel.rs`, `panels.rs`): a header (primitive icon,
  tag, "in `<Component>`", the element's position that opens the file,
  "Appears in" with scene names or timecode ranges without chapters,
  "Shared  also in …(N instances)" in the warning color with a hover list of
  call sites, "Can't move" rows listing locked props and reasons), then the
  impact line, the Transform rows, the props file's Properties, and the Layer
  section with the old Source / Called from rows under a collapsed
  "Source chain" disclosure.
- Timeline (`timeline.rs`): the Composition row draws a filled band for this
  element in the current scene and outlined bands for its other occurrences
  (tooltip: scene or timecodes, frames, call site; a click seeks). Before the
  scan is ready, one filled band shows the report's span. The Chapters row
  underlines the chapters the selection appears in.
- Keeping the selection across own writes (`refresh.rs`, `background.rs`,
  `transform_edit.rs`, `transform_drag.rs`): the kept reload used to set the
  lookup back to Pending and reset the transform state, which blanked the
  tab until the reloaded code answered. Now it leaves `source_lookup` as it
  is, sets `source_lookup_stale`, and calls `TransformEdit::expire`, which
  keeps the resolution shown but not current (so the Transform fields are
  read-only) and resolves the next lookup answer even when it equals the
  last. `sync_transform_targets` starts nothing while the lookup is stale, so
  the old report is never resolved against the written file. The first
  lookup of the reloaded code (`LookupFate::Show`) clears the flag; the
  resolution started from it is current again. The header shows a small
  spinner while the flag is set. A successful commit records the written
  values (`TransformEdit::record_written`), and the stale fields show them
  until the reloaded code resolves, so the numbers update when the write
  lands rather than after the reload. A successful drag commit keeps the released
  drag (and its override) until the kept reload, which drops it before
  redrawing, so a redraw in between still shows the dragged position.
  Playback asks for a lookup while it is stale, as it does while one is
  pending. Reloads from outside edits clear the selection as before.

## Cost of the sweep

One tree walk per frame of the composition per reload while Select layers is
on (none while it is off): each frame is mounted and its layers walked as a
`layerSource` lookup does, and the scene is discarded. It runs on its own thread and its own Node process, so the preview
and its runtime are not blocked, but a long composition keeps one core busy
for a while after each save. A reload during a sweep abandons it at the next
chunk boundary, so at most 240 frames of stale work run after a save.

## Decisions

- **Chunked sweep instead of killing the process.** The spec cancelled a
  running sweep by killing its bridge process. `ReactBridge` owns its `Child`
  and exposes no handle another thread could kill, so the sweep runs as
  240-frame `collectOccurrences` requests, and the worker abandons it between
  chunks when a newer request is queued.
- **No per-fiber chain cache.** Fibers alternate between renders and the
  committed set exists to pick the live one; caching chains by fiber risks
  stale chains. The sweep computes the committed set once per frame and walks
  chains from it.
- **"Changes N places in this scene".** The impact line for an element no
  other scene shares says how many places it changes when it is drawn more
  than once in the current scene (the sum of each overlapping instance's
  `max_per_frame`), so copies in the same scene are not silent.
- **Window placement from each `Sequence`'s own `from`.** A window is placed
  from the `Sequence`'s own `from` against the window above it, where the
  `Sequence` gates its children, not from the frame its children see: a
  frame-shifting provider such as a `MotionBlur` moment moved windows by a
  fraction of a frame, and a fractional `from` was rounded.
- **Element key = first non-wrapper chain entry.** Two layers are the same
  element when the first entry of their chains that is not a timing wrapper
  is the same position; the whole chain without wrappers is the instance.
- **A playhead in a gap between chapters counts as no current scene.**
  Nothing is then in the current scene: every scene the element is in is an
  "other" scene, and every band is outlined. Without chapters of any length
  the whole timeline is one scene.
- **Call sites = where instance chains diverge.** Each instance's call site
  is its chain entry at the first index where the element's instance chains
  differ (its last entry when they never do), so the hover list names the
  call that makes each instance, not the element itself.
- **The header row is labeled "Can't move".** One row per lock reason,
  listing the props it locks, in target order.
- **Chapters are marked by an underline.** The chapters the selection
  appears in get an accent underline in the Chapters row; the current
  chapter keeps its own highlight.
- `TransformEdit::expire` also clears the last commit's error, as `reset`
  did on the kept reload.

## Verifying by hand

Manual checks are pending the app check:

- Drag a layer and release: the Properties tab keeps its rows (read-only, with
  a spinner in the header) and the layer does not jump back before the
  reload.
- A component used in two chapters: "Shared also in …" and "Also changes …",
  the hover list of call sites, outlined bands in the other chapter, and the
  underlines in the Chapters row.
- A composition without chapters: "Appears in" shows timecode ranges and
  "Changes N places in this scene" counts copies.
- Save from outside Celesta during a long sweep: "Scanning scenes…" until
  the new sweep finishes, never the old result.
- Nudge a layer with the Transform rows: the Shared row, "Also changes …",
  the outlined bands, and the chapter underlines stay shown through the
  reload and the re-sweep.
- With Select layers off, saving starts no sweep (no extra Node process);
  turning it on shows "Scanning scenes…" and then the scan.
