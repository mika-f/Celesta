# Selection and impact design

Part 1 of `2026-10-23-editor-element-editing-overview-design.md`; the terms
(scene, element, instance, occurrence, shared) are defined there.

## Goal

When a layer is selected in the React preview, the user can tell at a glance:

- what it is: the element's tag, the component that wrote it, and where;
- when it is on screen: its occurrences, highlighted on the timeline;
- what an edit to it would change: the scenes and instances that share the
  element, before anything is written;
- why a value or a drag is not available, without hovering.

The selection and the Properties tab stay in place while the editor's own
write reloads the code.

## Non-goals

- Choosing between "this scene" and "all scenes" (design 3). This design only
  shows the impact; edits still write where they write today.
- Editing anything new (design 2) or timing (design 4).
- Selecting a layer by clicking an occurrence on the timeline. Clicking seeks
  only.
- Conditional rendering inside a window (`frame > 30 && <X />`) in the
  instant span; the occurrence index covers it once the sweep finishes.

## Overview

```text
select layer ─> layerSource (chain, windows, span) ─> header, span highlight
reload ──────> occurrence sweep (own bridge process, background)
            ─> OccurrenceIndex ─> scenes list, shared highlight, impact line
own write ───> keep last report and targets visible (stale) until re-lookup
```

## Runtime (`packages/react`, `packages/cli`)

### Wrapper entries in the source chain

The source chain lists every fiber above the layer whose props carry a JSX
position, walking parent fibers (`sourceFibers`). So a `<Sequence>` the user
wrote appears as a `component` entry, and an element passed as `children`
reports the component it was passed to as its next entry, not the component
that wrote it. Later designs need to tell these apart:

- `LayerSourceEntry.kind` gains `'wrapper'` for components marked transparent
  (`Composition`, `Sequence`, `FreezeFrame`, the motion-blur wrappers).
  `Loop` is marked transparent too; it is not today.
- The header's "in `<Component>`" (below) and the instance key skip
  `'wrapper'` entries.
- Finding the call site of the component that *wrote* an element (not its
  parent) is design 2's owner call site rule.

The existing `layer-source` tests change with the new kind.

### Timing windows in the report

`framesAbove` returns the frames the providers above a layer hand down,
without their windows. The `layerSource` report gains:

```ts
interface TimingWindow {
  /** The wrapper's component name: 'Sequence', 'Loop', 'FreezeFrame'. */
  kind: string;
  /** First frame and end (exclusive), in composition frames. */
  from: number;
  to: number;
  /** The JSX position the window's props were written at, if any. */
  source?: { file: string; line: number; column: number };
  /**
   * The window's frames are not the composition's one to one: a
   * `FreezeFrame`, a `Loop`, or any window inside one. Design 4 does not
   * drag these.
   */
  remapped?: boolean;
}

interface LayerSourceReport {
  // existing fields …
  /** Enclosing timing windows, nearest first. */
  windows: TimingWindow[];
  /** The intersection of `windows`, clipped to the composition. */
  span: { from: number; to: number };
}
```

The windows are read from the fibers on the element's way to the root, the
same walk `framesAbove` does, using each wrapper's resolved window (the one
`Sequence` computes for gating). `<Series>` and `<Scenes>` place their items
with `Sequence`:

- a `<Series.Sequence>` item's window has the item's position as `source`,
  since design 4 edits the item's `durationInFrames` and `offset`. The item
  element never mounts, so `SeriesRoot` must pass the item's JSX position on
  to the `Sequence` it renders (a runtime change in this design);
- a `<Scenes>` scene's window has no `source` (`<Scenes>` builds it with
  `createElement`); design 4 finds the `defineScenes` entry instead.

A `Loop` reports its outer window, marked `remapped`; its repeats are not
listed.

### Occurrence sweep

A new request, `{ "collectOccurrences": { "from": 0, "frames": 240 } }`,
walks the frames `[from, from + frames)` as `collectAudio` walks every frame (settle the tree, walk it with a layer-node map, no scene
JSON, no audio decoding) and records, for every layer node:

- the element key: the `file:line:column` of the first chain entry that is
  not a `'wrapper'`;
- the instance key: the positions of the chain's entries other than
  `'wrapper'` ones, innermost first.

Copies with the same instance key in different scenes (one `Scene` component
used by two `defineScenes` entries, `.map()` over scenes) are one instance
whose ranges fall in several scenes; the editor counts impact in scenes, so
they still show as shared.

It answers with frame ranges run-length encoded per instance:

```ts
interface OccurrenceIndex {
  elements: Array<{
    element: { file: string; line: number; column: number };
    instances: Array<{
      chain: Array<{ file: string; line: number; column: number }>;
      ranges: Array<[from: number, to: number]>;
      /** Most copies of this instance in any one frame (`.map()`). */
      maxPerFrame: number;
    }>;
  }>;
}
```

Only layers whose chain has a non-wrapper entry with a source position are
recorded. Layers without one (an entry built without positions, or library
code called straight from a wrapper) are skipped. A position-less layer
drawn by a library component the user called is recorded under that call,
as `layerSource` reports it.

Ranges are clipped to the requested frames; the bridge joins chunks.

Cost: one tree walk per frame, like `collectAudio`, plus a chain walk per
layer from a committed-fiber set computed once per frame. Chains are not
cached across frames: fibers alternate between renders, and a cached chain
could be stale. The sweep must not slow the preview, so it runs in a separate
bridge process (see below).

## Bridge (`crates/react-bridge`)

- `LayerSourceReport` gains `windows` and `span`, with serde defaults so an
  older runtime still parses.
- `ReactBridge::collect_occurrences() -> Result<OccurrenceIndex, _>`.
- `OccurrenceIndex` with lookups:
  - `instances_of(element) -> &[Instance]`;
  - `scenes_of(ranges, chapters) -> Vec<ChapterId>`: chapters a set of ranges
    overlaps, used per instance and for a whole element;
  - `instance_for(report) -> Option<&Instance>`: the instance whose key equals
    the report's chain positions (wrappers skipped).

## Editor (`crates/editor`)

### Occurrence worker

An `OccurrenceWorker`, shaped like `ComponentSchemaWorker`: its own thread and
its own `ReactBridge` spawned with source locations. The sweep is requested in
chunks of 240 frames (`collectOccurrences { from, frames }`); on every React
reload the worker abandons the running sweep at its next chunk and starts
another. The sweep runs only while Select layers (the interactive mode) is
on, since nothing can be selected outside it: turning it on starts a sweep,
and turning it off drops the index. The view
keeps `occurrences: OccurrenceState` = `Scanning` | `Ready(index)` |
`Refreshing(index)` | `Failed(message)`, tagged with the reload generation it
belongs to. A result for an older generation is dropped, and so is one that
lands after Select layers was turned off.

Until the index is ready, the impact is shown from the report's `span` only,
with "Scanning scenes…" in place of the scene list.

### Header

The top of the Properties tab, above the Layer section, shows the selection in
one block:

```text
▢ Rect                      in <PriceTag>
  components/PriceTag.tsx:42  ↗
  Appears in  Price
  Shared      also in Intro, Outro (3 instances)   [warning color]
```

- First line: a lucide icon for the primitive (square, circle, type, image,
  film, group), the element's tag, and "in `<Component>`" for the nearest
  `component` entry of the chain (wrappers skipped; omitted when there is
  none).
- Second line: the element's position, opening the file like the existing
  source rows.
- "Appears in": the scenes this instance's occurrences overlap.
- "Shared": shown when the element's occurrences (all instances) overlap
  more than one scene; it names the other scenes and the instance count.
  Hovering lists the call sites.
- The existing "Source" and "Called from" rows move under a collapsed "Source
  chain" disclosure.

### Edit impact line

Above the Transform rows (and every editable group design 2 adds), one line
states what a commit changes:

- not shared: "Changes this scene only", or "Changes 3 places in this scene"
  when the element renders more than once in it;
- shared: "Also changes Intro, Outro", in the warning color.

For a pass-through target (design 2) the scenes are those of the instances
that the written call site produces.

This design does not stop shared edits (the user sometimes wants them); design
3 adds the choice. The per-frame `Repeated` lock stays: copies made by
`.map()` in one frame can't be told apart by the source edit.

### Why it can't move

Today a locked layer shows a dashed outline and its lock reason only as a
tooltip. The header gains "Can't move" rows listing the locked props with
their reasons, one reason per row ("X, Y — Written as an expression: `W / 2`"),
so the reason is visible without hovering. Props whose targets are editable
are not listed.

### Timeline highlight

On the React composition row (V1):

- this instance's occurrences in the current scene: a filled accent band;
- every other occurrence of the element (other instances, and this instance
  in other scenes): an outlined band in the same accent;
- before the index is ready: the report's `span` as the filled band.

Bands draw beneath the playhead and above the row background, are hit-tested
for a tooltip (scene, frames, the instance's call site), and a click seeks to
the band's first frame.

On the chapter row, the chapters this instance appears in are underlined with
a 2 px accent bottom border; the current chapter keeps its own highlight.

The bands and underlines follow the selection and clear with it.

### Keeping the selection across the editor's own writes

`refresh.rs` resets the lookup to `Pending` and clears the transform targets on
every reload, so the Layer section blanks until the reloaded code answers.
After a reload caused by the editor's own write (the existing
`selection_keep`):

- the last `SourceLookup::Found` report and the resolved targets stay
  displayed, marked stale: fields read-only, a small spinner in the header.
  The fields of the props the write changed show the written values at once,
  so the numbers do not lag behind the reload;
- the canvas keeps the drag's overridden scene until the reloaded frame is
  shown, so the layer does not jump back for a frame;
- the new lookup's answer replaces them when it arrives. If the layer no
  longer exists, the selection clears as today;
- the last scene scan stays in use (`Refreshing(index)`) until the reload's
  new sweep lands, so the "Shared" row, the impact line, the bands, and the
  chapter underlines stay shown; the new sweep's result, ready or failed,
  replaces it. That sweep starts only once the preview has drawn the reloaded
  code, so it does not compete with the reload.

Reloads not caused by the editor (an outside edit) clear the selection as
today, and the scan starts over from `Scanning`.

## Testing

TypeScript (Vitest, `packages/react`, `packages/cli`):

- `kind: 'wrapper'` for a written `<Sequence>`, `<FreezeFrame>`, and
  `<Loop>`; a `component` for a user component.
- `windows` and `span` for nested `Sequence`s, a `Series` item (source at the
  `<Series.Sequence>`), a `Scenes` scene (no source), a `Loop` and a
  `Sequence` inside a `FreezeFrame` (`remapped`), and an element with no
  wrapper.
- `collectOccurrences`: one component used in two scenes gives one element
  with two instances; one `Scene` component in two `defineScenes` entries
  gives one instance with ranges in both; `.map()` gives `maxPerFrame` > 1; a conditionally
  rendered element's ranges stop where the condition does; layers without
  positions are skipped.
- Export scenes are unchanged with and without source locations.

Rust:

- `OccurrenceIndex` lookups: `scenes_of` at chapter edges (end exclusive),
  `instance_for` by chain.
- `OccurrenceState` drops a result from an older generation.
- Keeping: after an own write the last report stays until the new answer; an
  outside reload clears it.
- Timeline band geometry for ranges clipped by the visible window.

Manual, with screenshots of the same scene before and after for the PR:
select an element of a component used in two scenes, see both bands and the
"Shared" line; drag a layer and check the Properties tab does not blank.

## Documentation

- The React preview user docs: the header, the bands, the impact line.
- A handoff note `docs/handoff/<date>-selection-impact.md`.
