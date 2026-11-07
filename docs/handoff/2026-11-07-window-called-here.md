# A band drag's label names the call in a scene calling twice (2026-11-07)

Follow-up to mika-f/Celesta#560. Paths are under `crates/editor/src/`.

## Specification

When the current scene calls the component a timing window is written in
(`C`) more than once, the window's scope options now count those calls
(`ScopeOptions::called_here`), as an element's do. While **This scene** is
in effect, the band drag's label then reads "Edits change this `<C>` only"
in place of "Edits change this scene". What the drag writes is unchanged:
**This scene** already wrote the selected call `S` only.

So with a Price scene calling `PriceTag` twice and the `<Text>` in
`PriceTag` wrapped in its own `<Sequence>`, a band drag of the Text under
**This scene** reads "Edits change this `<PriceTag>` only". A scene that
calls `PriceTag` once still reads "Edits change this scene", and so does a
composition without chapters, where nothing is counted (as for an
element).

## Implementation

- `scope::window_calls_here` counts the distinct entries at `S`'s place
  over the window's instances (`window_instances`) shown in the current
  scene. A window's instances belong to every element it wraps, so their
  chains have their own length below the window: the place is counted from
  the outer end of the selected chain, for instances whose chain ends with
  the selected chain above `S`. 0 without a current scene.
- `options_over` uses it for a window in place of the fixed `(0, true)`;
  `drawn_alone` stays true for a window.
- `BandDrag` keeps `called_here` from the window's scope state at the
  press, and `band_grips` builds the label's second line with
  `scope_in_effect_here` (the canvas drag's rule).

### Limits

- A call of `C` drawn at another depth in the scene than `S` (one inside a
  `<Group>`, say) has a chain that doesn't end as the selected one does
  above `S`, so it isn't counted. The label then reads "this scene", as
  before, though only `S` is written.
- The window's scope options are shown only by the band drag, so the
  options' `this_scene_tooltip` and `scope_line` read for a window are not
  drawn anywhere.

## Tests

In `tests/timeline_timing.rs`:

- `a_window_in_a_component_a_scene_calls_twice_counts_the_calls` (new): the
  issue's example. Two calls of `PriceTag` in Price give `called_here` 2 and
  the label "Edits change this `<PriceTag>` only"; with the second call drawn
  in another scene only, `called_here` is `None` and the label reads "Edits
  change this scene".
- `a_window_a_scene_calls_twice_writes_this_call_under_this_scene`: now
  expects `called_here` 2 (it asserted `None`), and `None` without chapters.
- `a_window_two_scenes_call_is_shared_by_its_own_instances`: Intro calls
  `Delayed` once (two elements in one call): `called_here` is `None`.

## Checks

```sh
cargo test -p celesta-editor
cargo clippy -p celesta-editor --all-targets -- -D warnings
cargo fmt --all -- --check
```

All pass: editor 699 tests.

## Not checked on screen

The app was not run. To check: in a scene calling `PriceTag` twice, where
the element inside `PriceTag` has its own `<Sequence>`, select that element
in one call, keep **This scene**, and drag the middle of its band: the
label's second line should read "Edits change this `<PriceTag>` only". In a
scene calling it once, it should read "Edits change this scene".
