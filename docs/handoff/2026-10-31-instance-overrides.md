# Instance-scoped preview overrides (2026-10-31)

Issue #557. A drag or a row scrub of an element in a component the current
scene calls more than once, under **This scene**, moved every copy in the
frame until the release, though only the selected call was written. The
preview overrides now take the copies the write changes: under **This
scene**, a value written on the element moves the copies under the selected
call only; under **Shared value**, a parameter's default moves the copies of
every call that leaves the parameter out. It builds on
`docs/handoff/2026-10-26-edit-scope.md` (whose Known gaps listed the first
case) and `docs/handoff/2026-10-28-drag-session.md`.
User docs: `packages/website/content/{en,ja}/docs/01-(getting-started)/03-preview.md`
("Edit one scene or every scene").

## What changed

- Bridge (`crates/react-bridge/src/types.rs`): `PropOverride.within:
  Option<SourcePos>`, serialized only when set; `SourcePos` is `Serialize`.
  The protocol carries it as `within: { file, line, column }`.
- Editor (`crates/editor/src/transform_drag.rs`): `OverrideSite.within`.
  `override_sites(targets, report, scope, calls)` reads `scoped_row` per
  row: a `Lift` row's site (the element's position) gets `within: S`
  (`targets.component.call_site`, where the lift writes); a `Default` row's
  site is the parameter's prop at each distinct call of `calls` that leaves
  it out (`calls_leaving_out`), falling back to `S` when there are none. A
  row may now have several sites: `prop_overrides` and `Scrub::start` take
  all of them. `begin_transform_drag` and `begin_row_scrub`
  (`element_rows.rs`) pass `kept_scope()` and the scope context's scene
  calls; `Scrub::start` takes them as `(scope, calls)` and keeps the scope.
- Runtime (`packages/react/src/jsx-dev-runtime.ts`): `PropOverride.within`
  (`OverridePosition`). While a frame has overrides, `jsxDEV` gives a
  component element created at a `within` position a boundary type
  (`boundary(type)`, cached per type) that renders the original type with
  the same props under `WithinContext`, extended by its position
  (`file:line:column`; the extended arrays are cached so the value stays
  the same object across frames). `overridden` applies an override with
  `within` only when `use(WithinContext)` holds that position. Overrides
  without `within` behave as before. `render.ts`'s remount key
  (`updateOverrides`' positions) includes `within`, since it changes an
  element's type.

## Decisions

- **`within` is one call, not a chain.** The issue proposed matching the
  selected instance's whole chain. A **This scene** lift writes the new
  prop at `S`, so after the reload every copy under `S` changes and no
  other does; "under the element written at `S`" is exactly that set. A
  chain would also be wrong when `S` itself renders several copies (they
  all take the write).
- **A boundary type rather than the React owner.** The runtime uses React's
  production build, which records no owner, and context is the only way a
  render can tell which call it is under. The boundary is given only to
  the element at a `within` position, only on frames with such an
  override; its fiber and the original's share the props object
  (`jsx` keeps a key-less props object), so `sourceOfProps` and the source
  chain read the same position, and the chain's dedupe of repeated
  positions drops the boundary's entry. The boundary's single child adds no
  `useId` fork. A parent that checks its children's types (`<Series>`,
  rich text's `<Span>`) would see the boundary, but `S` is a call of the
  user's own component, which those never take.
- **"Shared value" previews a default where it applies.** A default is not
  a prop, but overriding the parameter's prop at a call that leaves it out
  is what the new default does there, so the preview reaches each call the
  scene calls list as leaving it out.

## Checks

- `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo fmt --all -- --check`, `git diff --check`.
- New Rust tests: `frame_requests_carry_prop_overrides_only_when_there_are_some`
  (the `within` field), `this_scene_previews_a_lifted_value_on_this_call_s_copies_only`,
  `shared_value_previews_a_default_at_every_call_it_reaches`
  (`crates/editor/src/tests/scope.rs`).
- `pnpm run build:runtime`, `pnpm run test:runtime`. New Vitest cases in
  `packages/react/test/prop-overrides.test.mts` (an override within a call
  reaches that call's copies only, also for a `memo` component, matches the
  file, and leaves the source chain naming the call) and
  `packages/cli/test/prop-overrides.test.mts` (the same through the built
  CLI and esbuild's `jsxDEV`).
- Not checked in the running editor: the drag and the scrub were not
  performed by hand in the GPUI app.

## Known gaps

- A `within` override needs the element at `S` to be created through
  `jsxDEV` on a frame with the override; an element at `S` cached with
  `useMemo` across the drag's start keeps its type (the entry remount at
  the start makes this unlikely, as for the existing `useMemo` limitation).
