# Required component props (2026-10-26)

Issue #527. The Properties tab's Component section offered to remove any
prop written as a literal at a component's call, including one the
component's props type requires: `<Badge caption="Sale" count={3} />` lost
`count={3}`, drew "Sale ×undefined", and no longer type-checked. The editor
does not know the component's TypeScript types; primitives avoid this with
their tables' `required(…)` (#525), components had nothing like it.

Of the issue's options (A: never remove a Component-section prop; B: let
`registerComponent()`'s schema declare `required`; C: write the schema's
`defaultValue` instead of removing), this takes **B**, as primitives do.
User docs: `packages/website/content/{en,ja}/docs/01-(getting-started)/03-preview.md`
("Edit properties"); skill: `skills/celesta/references/project-data.md`.

## What changed

- Schema type (`packages/react/src/registry.ts`, `property-tables.ts`,
  `index.ts`): a schema field is `ComponentSchemaField`
  (`ComponentPropertyField & { required?: boolean }`).
  `ComponentPropertySchema<Props>` types each field by `RequiredKeys<Props>`
  (now exported from `property-tables.ts`): a prop `Props` requires must
  say `required: true`, and an optional one must not, so a schema that
  misses one fails `tsc -b`, as a primitive table does. An untyped schema
  (`ComponentPropertySchema`, `Record<string, JsonValue>`) may declare
  either. `ComponentPropertyField` itself, which project properties share,
  has no `required`.
- Wire and Rust (`crates/react-bridge/src/types.rs`, `lib.rs`): a schema
  is `BTreeMap<String, ComponentSchemaField { field, required }>`, with
  `required` read from inside the field (`#[serde(flatten)]`, default
  `false`) and serialized only when true. Readers of the field
  (`schema_field`, `component_row_label`, the timeline Inspector's
  `render_component_props`) take `.field`.
- Removal (`crates/editor/src/element_rows.rs`): `removable` refuses a
  `CallSite` prop the component's schema declares required, and a table
  row whose target is a `PassThrough` to such a parameter (its removal
  deletes the attribute at the call). A component without a schema, or a
  prop its schema leaves out, stays removable. The commit refuses the same
  removals ("count can't be removed here"), and an emptied field writes
  nothing (`TypedText::Restore`), as for a primitive's required prop.
- Examples: `packages/cli/examples/with-registered-component.tsx` and
  `with-text-box.tsx` mark their required props.

## Decisions

- **Breaking for typed schemas.** An existing
  `ComponentPropertySchema<Props>` (or an inline schema whose `Props` is
  inferred) that declares a required prop without `required: true` no
  longer compiles. That is the point: it is how the primitive tables keep
  `required` honest, and the editor cannot check the call's types itself.
- **Pass-throughs follow the component's schema.** A row above the
  Component section that is set at the call (`<Rect x={x} />` in a
  component called with `x={40}`) removes the call's attribute, so it is
  checked against the component's schema too, by the parameter's name.
  That is the call the removal edits, also when the value passes through
  a further component above it.
- **`required` does not change rendering or `--props`.** Like `min`,
  `max`, and `integer`, the runtime does not check it; it is metadata for
  the editor.

## Checks

- `pnpm run build:runtime`, `pnpm run test:runtime` (the new
  `packages/react/test/component-schema.test.mts` checks the field keeps
  `required` and, with `tsc -b test`, that a missing or extra `required`
  does not compile).
- `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo fmt --all -- --check` (new tests:
  `a_prop_the_component_schema_requires_is_not_removable`,
  `a_required_component_prop_keeps_its_value_when_emptied`, and the ready
  message's `required` in `deserializes_component_schemas_from_the_ready_message`).
  Clippy fails with rustc 1.99.1 on `large_enum_variant` for
  `SourceLookup` (`crates/editor/src/layer_select.rs`), on the base commit
  as well; this change does not touch it.

## Verify by hand

With `registerComponent('Badge', Badge, { caption: { …, required: true },
count: { type: 'number', defaultValue: 0, required: true }, wide: { type:
'boolean', defaultValue: false } })` and `<Badge caption="Sale" count={3}
tone="#ff4444" wide />`, select a layer inside `Badge`: the Count row shows
no ×, and emptying it and pressing Enter puts `3` back without writing.
Wide shows the × and removes `wide`; Tone, which the schema leaves out,
does too.
