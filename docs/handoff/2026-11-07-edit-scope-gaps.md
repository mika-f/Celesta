# Edit scope: known gaps (2026-11-07)

Issue #565 collected seven small gaps left by the editor's edit scope
("Edits change" [This scene] / [Shared value],
`docs/handoff/2026-10-26-edit-scope.md`). Each is fixed, verified not to be
a gap, or kept with the reason below. The old note's Known gaps list points
here for each.

## Fixed

- **A held keyframe commit read the shown scope's targets.**
  `drain_held_rows` (`crates/editor/src/element_rows.rs`) checked whether
  the playhead moved off a held commit's keyframe (`moved_off_keyframe`) on
  `element_targets()`, the targets of the scope the control shows. It now
  reads `targets_under(held.scope)` (made `pub(crate)` in
  `element_edit.rs`), the targets the commit is written to by
  `commit_element`. The case: a commit held under "This scene" for an
  element its component draws more than once, where the one-instance
  targets (`ElementTargets::one`) hold a keyframe and the targets as
  resolved hold `Repeated`; with "Shared value" shown by the drain (a tab
  switch or a seek in between), the old check saw `Repeated`, never moved,
  and wrote the keyframe at a frame the person no longer looked at. Test:
  `a_held_keyframe_commit_is_checked_on_the_targets_of_its_scope`.
- **The Size tool's warning said nothing when the scope hid the handles.**
  `resize_note` (`transform_drag.rs`) returned `resize_fallback`'s reason
  only; when the layer can be resized but `without_refused` dropped the
  handles for a refused size prop, it returned `None`. It now gives
  `resize_refusal`: the first refused size prop and its refusal, in the
  form of a locked size row ("Width: This scene sets its own
  `rectWidth`"), the same refusal `transform_state` read. Test: in
  `a_drag_offers_no_size_handles_for_a_refused_size_prop`.
- **An absent attribute locked as `OverriddenByParent` counted as "sets its
  own".** `shared_default` (`scope.rs`) counted anything but `Missing` at
  `S` as this scene's own value. `pass_through::missing_parameter` returns
  `OverriddenByParent` for an absent attribute whose parameter default is
  not what the runtime saw (something above the element overrides it), so
  such a row was locked as "This scene sets its own `rectCornerRadius`"
  with a **×** ("Use the shared value") that had no attribute to remove.
  `sets_own_value` now counts `OverriddenByParent` only when `S` writes the
  attribute; `ComponentTargets::written` (new) holds every attribute
  `written_attributes` finds at `S`, filled in `resolve_with`. Other locks
  at `S` (an expression, a spread that may hold the value) still count, as
  the scene line does for written attributes. Test:
  `an_overridden_value_counts_as_this_scenes_own_only_when_s_writes_it`.
- **The line named scenes before the scene calls were read.** Until
  `read_scene_calls` lands, `scope_state` had no calls, so every scene was
  in `shared_reaches` (those that set their own too) and the line said
  "Shared value is used in Intro, Price, Outro". `ScopeOptions::own_pending`
  (new: no calls and the rows read a parameter) makes `scope_line` say
  "Checking which scenes set their own…" instead, after the lock texts,
  which keep precedence. An element whose rows read no parameter has
  nothing to wait for and shows the reach at once. "Every scene sets
  `rectWidth` itself" still needs the calls: until then that row is locked
  as "This scene sets its own `rectWidth`", true of this scene and refused
  the same way, and the text changes when the calls land. Test:
  `the_line_waits_for_the_scene_calls_before_naming_scenes`.

## Verified, not a gap

- **A scope switched during a drag.** The drag's handles (`drag.editable`,
  `drag.resize`) come from `drag_state` at its start, which reads the
  shown targets, those of `chosen_scope(remembered, options)`; its release
  is gated and written on `ScopeContext::under(drag.scope)`, where
  `drag.scope` is `kept_scope()`, `commit_scope(remembered, state)`. These
  differ only when the remembered option is locked (the shown targets fall
  back to the targets as resolved, the kept scope stays the locked one),
  and then `gesture_start_refusal` refuses every key, so no drag starts.
  `ScopeState::Unknown` and `NotShared` read the targets as resolved
  whatever the scope. A choice that changes during the drag (a reload that
  carries or forgets it, say) changes neither the handles nor the scope
  the release uses: both are the start's, and the release reads the
  targets of that scope as they are then. Test pinning the invariant:
  `a_gesture_starts_only_on_the_targets_of_the_scope_it_keeps`.

## Kept, with the reason

- **An element repeated in a component is resolved twice.** `resolve_with`
  resolves it again as one instance (`render_count: 1`) into
  `ElementTargets::one`, on every resolution and every commit's re-resolve.
  Not changed: the work runs on the background executor, and each step is
  an oxc parse of one source file, well under a millisecond for a
  component file. Deriving the main targets from the one-instance ones
  instead would have to repeat `read_written`'s lock order (`SyntaxError`
  and `NotPrimitive` before `Repeated`, a `<Text>` style path's locks
  before both), and the two differ beyond the locks: an expression row has
  inputs only as one instance, so the passed parameters, the Component
  section's props, the defaults and the dry lift all differ. A shared parse
  cache in `celesta-source-edit` would cut this cost for every resolution,
  and is the place to do it if it shows up in a profile.
- **Anonymous and lowercase components count as not shared.** A lowercase
  name can't be a JSX tag (React reads `<priceTag>` as an intrinsic
  element), so such a function is called as a function, `{priceTag(props)}`,
  where "This scene" has no attributes to write. An anonymous component
  (`export default function ({ label })`) has a JSX call under the name it
  is imported as, but that name is written nowhere in the component's
  file, the lift has no declaration to name, and `owner_call_site`,
  `enclosing_component`, the lift's finder and `Scopes` all match by name.
  Supporting it means giving `C` one name every one of them agrees on (the
  default export's import name at the call), which is a design of its own.

## Checks

```sh
cargo test -p celesta-editor
cargo clippy -p celesta-editor --all-targets -- -D warnings
cargo fmt --all -- --check
```

## Verify by hand

On the project of `docs/handoff/2026-10-26-edit-scope.md`'s "Verify by
hand" (a `PriceTag` with a `<Rect>` called by Intro and Price), with
**Select layers** on:

- Select the `<Rect>` in Price right after opening the project: the line
  under "Edits change" reads "Checking which scenes set their own…" for a
  moment, then "Shared value is used in Intro · Price sets its own".
- Choose [Shared value] with Price passing `rectWidth={260}`, and choose
  the **Size** tool: the canvas shows no size handles, and the toggle's
  warning reads "Width: This scene sets its own `rectWidth`".
