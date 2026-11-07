# A timing window reads its own scene calls (2026-11-07)

Follow-up to `docs/handoff/2026-11-07-timeline-timing-gaps.md` ("Kept, with
the reason", the scene calls item) and mika-f/Celesta#566. Paths are under
`crates/editor/src/`.

## Specification

A timing window's scope (the band drag's **Edits change** options) counts a
scene as setting its own value when that scene's call of the component the
window is written in (`C`) passes a parameter the window's rows read. The
window's instances are every index instance whose `wrappers` hold the
window's source, of any element (`scope::window_instances`). Each of them
now has its call of `C` read, not only those whose chain ends as one of the
selected element's instances does from the call.

So for a `Delayed` that wraps its children in `<Sequence from={sequenceFrom}>`
and that Intro calls around a `<Rect>` and Main around a `<Text>`, with the
`<Rect>` selected:

- Main's `<Delayed sequenceFrom={60}>` is read: Main is listed as setting its
  own (`sets_own`), not under **Shared value is used in**.
- When Intro passes `sequenceFrom` too and the parameter has no default,
  the `from` row under **Shared value** is locked as "Every scene sets
  `sequenceFrom` itself". The grips and the release gate refuse a **Shared
  value** write of it with that reason. With a default, **Shared value**
  writes the default and every scene keeps its own value, as for any
  shared element. The option itself stays open, since the window's other
  rows (`durationInFrames`) can still be written.

## Implementation

- `scope::window_calls_read` builds what the read takes (`WindowCallsRead`):
  the window's source, the index generation, the tag at the selected `S`
  (`ComponentTargets::name`), the selected chain from `S` outward without
  wrappers, and each window instance's chain.
- `scope::read_window_calls` reads the calls in that request, one per window
  instance, in `window_instances` order. To find an instance's call of `C`:
  - when the instance's chain ends with the selected chain from `S`, the call
    sits at that place (the selected instance, and another element written
    in the same call);
  - otherwise it is the innermost chain entry above the element whose JSX tag
    reads (`tag_name`) as the tag at `S` (another scene's call).
  An instance with neither is `None`, as before. Files are read once per
  request.
- `SceneCalls` for a window is keyed by window instance (`element` is the
  window's source). `window_scope_state` takes calls only when they are for
  the window and match its instance count, and returns them unchanged. The
  earlier mapping from the element's calls (`scope::window_calls`) is gone.
  `impact_filter`, `gesture_gate` and `grip_refusal` already took the
  window's `SceneCalls` from `window_scope_state`, so they are unchanged.
- `ElementEdit::window_calls` holds `(WindowCallsRead, SceneCalls)`. The
  window's calls are always read for the window now. Before, they came from
  the element's calls when `C` was the element's component.
  `EditorView::window_scene_calls` returns them while the current request
  equals the one they answer: same index generation, window, `S` chain and
  instance chains. `sync_window_scene_calls` starts the background read
  otherwise, deduplicated by `window_calls_wanted`. It still bumps
  `scene_calls_version`, which the window's scope cache keys on.

### Release gate

The press keeps the scope and the calls from `band_scope`. The release
checks `gesture_gate` with the calls current then. Only the calls' content
changed. Until the window's read lands, the options show `own_pending` and
the gate behaves as it did when calls were unread.

### Limits

- A scene that calls `C` under another name (an import alias, a namespace
  member whose last segment differs) is not matched by tag. Its call is
  `None`, so it counts as not setting its own value, as every such instance
  did before.
- With `C` nested in itself around the element, the innermost matching call
  is taken, which is the call of the window nearest the element.

## Tests

In `tests/timeline_timing.rs`:

- `a_window_two_scenes_call_is_shared_by_its_own_instances` now reads the
  calls from the files: the Rect, the Circle (same call) and Main's Text each
  get their own call.
- `a_window_reads_the_call_of_another_elements_scene`: Main passing
  `sequenceFrom` is under `sets_own`, not `shared_reaches`, and a **Shared
  value** write goes. With Intro passing it too and no default, the `from`
  row is `EverySceneSets` and `gesture_gate` refuses **Shared value**.
- `a_window_ignores_calls_read_for_the_element`: the element's calls, keyed
  by its own instances, don't count for the window (`own_pending`).

## Checks

```sh
cargo test -p celesta-editor
cargo clippy -p celesta-editor --all-targets -- -D warnings
cargo fmt --all -- --check
```

All pass: editor 697 tests.

## Not checked on screen

The app was not run. To check: in a project where `Delayed` (wrapping its
children in `<Sequence from={sequenceFrom}>`) is called in two scenes around
different elements, pass `sequenceFrom` in one scene's call, select an
element in the other scene, choose **Shared value**, and drag the band. The
passing scene's bands should stay where they are while the drag runs. With
both scenes passing it and no default for `sequenceFrom`, the grips that
write `from` should be locked with "Every scene sets
`sequenceFrom` itself".
