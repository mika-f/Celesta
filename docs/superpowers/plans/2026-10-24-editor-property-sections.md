# Properties Sections (Design 2b) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every prop a primitive declares in its property table is shown and edited in the Properties tab, one section per group (text, font, colors, size, corner radius, opacity, image source, not only the transform), with a control per type; a value computed from others lists its inputs as editable sub-rows; a component call's own props get a "Component" section; `Text`'s font shorthands edit `style` when `style` sets them.

**Architecture:** The runtime declares a property table per primitive (`packages/react/src/property-tables.ts`, `@celesta/shapes` through `@celesta/react/internal`) and sends them in the ready message. The editor generalizes 2a's transform targets to prop-name keys and `EditValue`s, resolves every table row (plus inputs and the call site's props) off the UI thread, and renders the rows with controls shared with the project Properties panel. Writes keep 2a's path: re-resolve, apply, `write_all_recorded`, one history entry per committed field.

**Tech Stack:** TypeScript runtime (Vitest, `tsc -b test` for type tests), Rust (`celesta-react-bridge` serde, `celesta-source-edit` oxc, `celesta-editor` GPUI via gpui-kit 0.6).

**Spec:** `docs/superpowers/specs/2026-10-23-editor-property-editing-design.md` — sections "Property tables", "Properties tab", "Inputs", and the `Text` shorthand paragraph of "Typed edit values". Builds on 2a (`docs/superpowers/plans/2026-10-23-editor-property-editing-engine.md`, handoff `docs/handoff/2026-10-23-property-editing-engine.md`).

| Part | Plan | Spec sections |
| --- | --- | --- |
| 2a | `2026-10-23-editor-property-editing-engine.md` (PR #516) | engine |
| 2b | this plan | Property tables, Properties tab, Component section, Inputs (rows), `Text` shorthand vs `style` |
| 2c | later | Canvas: Pivot, Resize (consumes the tables' `size`) |
| 2d | later | Inline text editing |

## Global Constraints

- `crates/source-edit` stays GPUI-free and never re-prints the AST: byte-span replacements and insertions only.
- Numbers are written with `format_number` as today.
- One committed field (or one scrub) is one history entry, whatever number of files it touches.
- A commit re-resolves every target it writes against every file it read; if any target moved, nothing is written and the error is "The source changed while editing".
- Rows are listed in the table's order: groups in declaration order, rows in declaration order. No `BTreeMap` may reorder them anywhere between `property-tables.ts` and the panel.
- UI copy is English, inspector style (radius 0, no gradients, no uppercase labels, lucide icons via `CelestaIcon`/`IconName`).
- Tests: Rust integration tests in `crates/source-edit/tests/*.rs`; editor unit tests in `crates/editor/src/tests.rs` (or the module's `#[cfg(test)]` block, as the module already does); Vitest `.mts` under `packages/*/test/` importing `../src`; type tests are `.test.mts` files that `tsc -b test` compiles (`// @ts-expect-error` lines).
- Red/green runs only the new tests; the crate's or package's full suite once per task.
- No `git commit --amend`; commit and test commands are separate calls. Leave the uncommitted `examples/feature-tour/` edits and `docs/superpowers/.DS_Store` alone; stage paths explicitly.

## Rulings (decided here; the spec leaves them open)

- **Field types.** Table fields are `ComponentPropertyField`'s types (`number`, `string`, `boolean`, `color`, `select`, `path`) plus `text`, with `defaultValue` optional (`Rect`'s `width` has none) and no `label`. They are their own type, `PrimitivePropertyField`, since `ComponentPropertyField` requires a default.
- **Text shorthands.** The four shorthand rows map to these `style` keys: `fontFamily` → `style.fontFamily`, `fontSize` → `style.fontSize`, `fontWeight` → `style.fontWeight`, `color` → `style.fill` (`themedTextStyle` fills `fill` from `color`). The row edits `style.<key>` when `style` is an object literal whose own properties set that key; the shorthand attribute otherwise (also when there is no `style`); and is locked `StyleMaySet` ("May be set by `style`") when `style` is not an object literal or the object has a spread.
- **No `style`-only rows in 2b.** The `Text` table carries only props with a shorthand home, so `NoStyleObject` (2a's lock for `style.<key>` without a `style` attribute) never shows in 2b and stays as it is. Adding a `style={{…}}` attribute is not needed.
- **Self-closing `<Text />` children stay locked.** `TextProps.children` is required, so `<Text />` does not type-check; 2a's `Expression("/>")` lock stays (the 2a handoff's "2b opens the tag" is withdrawn; the 2b handoff says so).
- **Colors.** A color field accepts `#RRGGBB`, `#RRGGBBAA` (as `parse_color_input` does today), or a `<Theme>` color name: a word matching `^[A-Za-z][A-Za-z0-9_-]*$`, written as typed. The editor has no list of theme names; the picker shows no color for a name. Anything else: "Enter a color: #RRGGBB, #RRGGBBAA, or a theme color name".
- **Kind mismatch.** A row whose literal is of another kind than the field type (`anchorY="baseline"` on a number row) is locked as `Expression { source }` with the literal's source text, as 2a locks kind mismatches.
- **Placeholder of an absent prop:** `Missing.default` (the source's) if any, else the received value, else the table's `defaultValue`, else empty. A missing `scaleX`/`scaleY` written on the element itself shows the received `scale` (as today); a `PassThrough` to a `Missing` with a default shows that default (fixes 2a's `({ sx = 3 }) => <Rect scale={2} scaleX={sx} />` showing 2).
- **Numbers with `min`/`max`** (`opacity` 0–1): typed values outside are refused with "Enter a number from {min} to {max}"; scrubbing clamps.
- **Impact scoping (`written_at`) stays transform-only**: it looks at `x`, `y`, `rotation`, `scale`, `scaleX`, `scaleY` targets as in 2a, so selecting a layer keeps the impact the user checked in #516. `locked_summary` stays transform-only too.
- **No table** (an unknown primitive, or a runtime that sends none): the Properties tab shows the Transform group from the transform props, as 2a does.
- **`path` rows** write the value `import_path(entry_dir, entry_dir, chosen)` returns: a relative `src` resolves against the entry's directory (`entry-dir.ts`).
- **Scrubbing** a number label previews through the runtime like a canvas drag (prop overrides, positions fixed for the scrub) and writes once on release; Escape cancels.
- **Component section** lists the attributes written at the owner call site (the call site 2a's pass-through uses), in source order, skipping `key`, `ref`, spreads, and attributes a primitive row of this element already shows as "Set at …". Typed from the registered schema when `registerComponent()` registered the tag's name, else from the literal's kind (number → number, string → string, boolean → boolean); non-literal values are locked rows. The call site's `children` are not listed.
- **Cost.** Each row resolves with its own parse (`resolve_through`, `resolve_inputs`, `text_style_path`); about twenty parses per selection on the background executor. Accepted; caching parses is not in 2b.

## Review Focus

- **A row whose literal kind differs from the table type** (`<Text anchorY="baseline">`). The Anchor Y row is locked showing `"baseline"`, never a number field holding `NaN`. Test in Task 6.
- **A `select` value not among the options** (`<Image fit="fill" />` in an old file). The row shows `fill` as its value and offers `contain`/`cover`; picking one writes it. Test in Task 8 (`select_value_outside_the_options_is_shown`).
- **A `text` value with leading or trailing spaces** (`" Sale "`). Written as `{" Sale "}`, not JSXText that React would trim. Test in Task 6 (commit through `commit_files`).
- **A Component-section prop that is also a pass-through target** (`<PriceTag x={40} label="Sale" />`, the `Rect` reads `x`). `x` shows once, in the Transform row with "Set at …", not again under Component. Test in Task 10.
- **A font row next to a `style` object that does not set it** (`<Text style={{ fontWeight: 700 }}>`, edit Font Size). The write inserts the shorthand `fontSize={48}` (the object is left alone), and one undo restores the file byte for byte. Test in Task 6.

---

### Task 1: Property tables in the runtime

**Files:**
- Create: `packages/react/src/property-tables.ts`
- Modify: `packages/react/src/internal.ts` (export `registerPrimitiveProperties`, `propertyTable` and the field helpers), `packages/react/src/components.ts` (register `Group`, `Image`, `Text`, `Rect`, `Video` next to their definitions, or in `property-tables.ts` importing the props types — implementer's choice), `packages/shapes/src/index.ts` (register `Ellipse`, `Circle`, `Triangle`, `Polygon`, `Star`, `Pie`, `Heart` beside the `markPrimitive` loop), `packages/cli/src/cli.ts` (`ReadyMessage.primitiveProperties`)
- Test: `packages/react/test/property-tables.test.mts` (new), `packages/shapes/test/property-tables.test.mts` (new), `packages/cli/test/primitive-properties.test.mts` (new)

**Interfaces:**
- Produces (TS, `packages/react/src/property-tables.ts`):

```ts
export type PrimitivePropertyField =
  | { type: 'number'; defaultValue?: number; min?: number; max?: number }
  | { type: 'string'; defaultValue?: string }
  | { type: 'text'; defaultValue?: string }
  | { type: 'boolean'; defaultValue?: boolean }
  | { type: 'color'; defaultValue?: string }
  | { type: 'select'; options: readonly string[]; defaultValue?: string }
  | { type: 'path' };
type PropKey<P> = Extract<keyof P, string>;
/** The props that hold the box, for 2c's resize handles. */
export interface PrimitiveSize<P> { width?: PropKey<P>; height?: PropKey<P>; radius?: PropKey<P> }
export interface PrimitivePropertyTable<P> {
  size?: PrimitiveSize<P>;
  groups: Record<string, Partial<Record<PropKey<P>, PrimitivePropertyField>>>;
}
export function propertyTable<P>(table: PrimitivePropertyTable<P>): PrimitivePropertyTable<P>;
export function num(defaultValue?: number, min?: number, max?: number): PrimitivePropertyField;
export function str(defaultValue?: string): PrimitivePropertyField;
export function text(): PrimitivePropertyField;
export function bool(defaultValue?: boolean): PrimitivePropertyField;
export function color(defaultValue?: string): PrimitivePropertyField;
export function select(options: readonly string[], defaultValue?: string): PrimitivePropertyField;
export function path(): PrimitivePropertyField;
export function registerPrimitiveProperties(name: string, table: PrimitivePropertyTable<any>): void;
/** Ordered for the wire: object key order is lost in Rust's maps. */
export interface WirePropertyTable {
  size?: { width?: string; height?: string; radius?: string };
  groups: { name: string; fields: { key: string; field: PrimitivePropertyField }[] }[];
}
export function listPrimitiveProperties(): Record<string, WirePropertyTable>;
```

- `ReadyMessage.primitiveProperties: Record<string, WirePropertyTable>` (always sent, `{}` when nothing registered).
- The tables (keys are the props types' names; `Transform` is the same for every primitive: `x: num(0), y: num(0), rotation: num(0), scale: num(1), anchorX: num(0), anchorY: num(0), opacity: num(1, 0, 1)`), registered under the component's name as a source chain reports it:

| Name | `size` | Groups after `Transform` |
| --- | --- | --- |
| `Rect` | `width`, `height` | Size `{ width: num(), height: num(), cornerRadius: num(0) }`; Appearance `{ fill: color(), stroke: color(), strokeWidth: num(0) }` |
| `Group` | — | none |
| `Image` | `width`, `height` | Source `{ src: path() }`; Size `{ width: num(), height: num(), fit: select(['contain', 'cover']) }` |
| `Video` | — | Source `{ src: path(), startFrom: num(0), playbackRate: num(1), loop: bool(false) }`; Audio `{ volume: num(1), muted: bool(false) }` |
| `Text` | `width: 'maxWidth'` | Text `{ children: text(), fontFamily: str(), fontSize: num(), fontWeight: num(), color: color() }`; Layout `{ maxWidth: num() }` |
| `Ellipse` | `width`, `height` | Size `{ width: num(), height: num() }`; Appearance `{ fill: color(), stroke: color(), strokeWidth: num(2) }` |
| `Circle` | `radius` | Size `{ radius: num() }`; Appearance as `Ellipse` |
| `Triangle` | `width`, `height` | Size `{ width: num(), height: num(), apex: num(0.5, 0, 1), cornerRadius: num(0) }`; Appearance as `Ellipse` |
| `Polygon` | `radius` | Size `{ radius: num(), sides: num(), cornerRadius: num(0) }`; Appearance as `Ellipse` |
| `Star` | `radius` | Size `{ radius: num(), points: num(), innerRatio: num(0.5, 0, 1), cornerRadius: num(0) }`; Appearance as `Ellipse` |
| `Pie` | `radius` | Size `{ radius: num(), startAngle: num(0), endAngle: num(), cornerRadius: num(0) }`; Appearance as `Ellipse` |
| `Heart` | `width`, `height` | Size `{ width: num(), height: num() }`; Appearance as `Ellipse` |

`Text`'s `Transform` group omits nothing (`anchorY` is `number | 'baseline'`; a `"baseline"` literal locks, see Rulings). If a props type lacks a key in this table (the type test says so), drop that row and report it; do not widen the props type.

- [ ] **Step 1: Write the failing tests**

`packages/react/test/property-tables.test.mts`:
- `every Celesta primitive registers a table`: `Object.keys(listPrimitiveProperties())` includes `Group`, `Image`, `Rect`, `Text`, `Video`.
- `tables keep their declared order`: `listPrimitiveProperties().Rect.groups.map(g => g.name)` equals `['Transform', 'Size', 'Appearance']`; the `Size` fields' keys equal `['width', 'height', 'cornerRadius']`; `Text`'s `Text` group keys equal `['children', 'fontFamily', 'fontSize', 'fontWeight', 'color']`.
- `size names the props that hold the box`: `Rect.size` equals `{ width: 'width', height: 'height' }`, `Text.size` equals `{ width: 'maxWidth' }`, `Group.size` is `undefined`.
- `fields carry their defaults and bounds`: `Rect` Transform `opacity` equals `{ type: 'number', defaultValue: 1, min: 0, max: 1 }`; `Image` Size `fit` equals `{ type: 'select', options: ['contain', 'cover'] }`.
- Type test in the same file: `// @ts-expect-error` above `propertyTable<RectProps>({ groups: { Size: { widht: num() } } })` and above `propertyTable<RectProps>({ size: { radius: 'radius' }, groups: {} })`. Every registration (here and in `@celesta/shapes`) is written `propertyTable<XProps>({ … })` with that primitive's own props type, so a renamed prop fails `tsc -b` for every primitive, not only `Rect`.

`packages/shapes/test/property-tables.test.mts`: importing `../src` registers `Ellipse`, `Circle`, `Triangle`, `Polygon`, `Star`, `Pie`, `Heart`; `Circle.size` equals `{ radius: 'radius' }`; `Star` Size keys equal `['radius', 'points', 'innerRatio', 'cornerRadius']`.

`packages/cli/test/primitive-properties.test.mts` (spawn the CLI as `prop-overrides.test.mts` does): the first message's `primitiveProperties.Rect.groups[0].name` is `'Transform'`, and an entry importing `@celesta/shapes` also lists `Star`.

- [ ] **Step 2: Run them to see them fail**

Run: `pnpm --filter @celesta/react exec vitest run test/property-tables.test.mts` (and the shapes and cli files likewise)
Expected: FAIL, `listPrimitiveProperties` is not exported.

- [ ] **Step 3: Implement** `property-tables.ts` (a module-level `Map<string, PrimitivePropertyTable<any>>`; `listPrimitiveProperties` turns each into `WirePropertyTable` with `Object.entries`, which keeps insertion order), the registrations, the `internal.ts` exports, and `primitiveProperties: listPrimitiveProperties()` in the CLI's ready message.

- [ ] **Step 4: Run the new tests, then each package's suite**

Run: `pnpm --filter @celesta/react test`, `pnpm --filter @celesta/shapes test`, `pnpm --filter @celesta/cli test`
Expected: PASS (the type test compiles only because the `@ts-expect-error` lines error).

- [ ] **Step 5: Commit** — `feat(react): declare a property table for every primitive`

---

### Task 2: Tables through the bridge into the editor

**Files:**
- Modify: `crates/react-bridge/src/types.rs` (types, `ReactCompositionMetadata.primitive_properties`), `crates/react-bridge/src/protocol.rs` (`ReadyMessage::Ready.primitive_properties`), `crates/react-bridge/src/bridge.rs` (copy into metadata), `crates/react-bridge/src/lib.rs` (exports), `crates/editor/src/preview.rs` (`ReactPreview.primitive_properties`), `crates/editor/src/refresh.rs` (set it where `chapters` is set on a successful load; keep it across failed loads like `chapters`), `crates/editor/src/view.rs` or `layer_select.rs` (accessor)
- Test: `crates/react-bridge/src/tests.rs`, `crates/editor/src/tests.rs`

**Interfaces:**
- Produces (Rust, `celesta_react_bridge`):

```rust
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum PrimitivePropertyField {
    Number { default_value: Option<f64>, min: Option<f64>, max: Option<f64> },
    String { default_value: Option<String> },
    Text { default_value: Option<String> },
    Boolean { default_value: Option<bool> },
    Color { default_value: Option<String> },
    Select { options: Vec<String>, default_value: Option<String> },
    Path,
}
pub struct PrimitiveSize { pub width: Option<String>, pub height: Option<String>, pub radius: Option<String> }
pub struct PrimitivePropertyRow { pub key: String, pub field: PrimitivePropertyField }
pub struct PrimitivePropertyGroup { pub name: String, pub fields: Vec<PrimitivePropertyRow> }
pub struct PrimitivePropertyTable { pub size: Option<PrimitiveSize>, pub groups: Vec<PrimitivePropertyGroup> }
// ReactCompositionMetadata:
pub primitive_properties: BTreeMap<String, PrimitivePropertyTable>, // keyed by primitive name; the order inside each table is the Vecs'
```
(every `Option` field `#[serde(default)]`; all four structs `Clone, Debug, PartialEq, Deserialize`, camelCase).
- `PrimitivePropertyField::default_value(&self) -> Option<EditValue>` is NOT here (react-bridge does not depend on source-edit); the editor converts (Task 6).
- Editor: `EditorView::primitive_table(&self, name: &str) -> Option<&PrimitivePropertyTable>` reading the React preview's kept tables.

- [ ] **Step 1: Write the failing tests**: in `crates/react-bridge/src/tests.rs`, `ready_message_carries_primitive_tables_in_order` parses a ready line with `"primitiveProperties": {"Rect": {"size": {"width": "width", "height": "height"}, "groups": [{"name": "Transform", "fields": [{"key": "x", "field": {"type": "number", "defaultValue": 0}}, {"key": "opacity", "field": {"type": "number", "defaultValue": 1, "min": 0, "max": 1}}]}, {"name": "Size", "fields": [{"key": "width", "field": {"type": "number"}}]}]}}` and asserts group names `["Transform", "Size"]`, row keys `["x", "opacity"]`, `opacity`'s `max == Some(1.0)`, and `width`'s `default_value == None`; `ready_message_without_tables_has_none` asserts an empty map. Use the helper the existing `componentSchemas` ready tests use. In the editor, a test that a load's metadata with a `Rect` table makes `primitive_table("Rect")` return it and a failed reload keeps it (follow the existing `chapters` keep test in `refresh.rs`/`tests.rs`).
- [ ] **Step 2: Run them** (`cargo test -p celesta-react-bridge ready_message_`) — FAIL: no field.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p celesta-react-bridge` and `cargo test -p celesta-editor` — PASS.
- [ ] **Step 5: Commit** — `feat(editor): read the primitives' property tables from the runtime`

---

### Task 3: Source queries for `Text` shorthands and call-site attributes

**Files:**
- Modify: `crates/source-edit/src/resolve.rs` (`LockReason::StyleMaySet`), `crates/source-edit/src/shape.rs` or a new `crates/source-edit/src/attributes.rs` (the two functions), `crates/source-edit/src/lib.rs` (exports), `crates/editor/src/transform_edit.rs` (`reason_text` arm)
- Test: `crates/source-edit/tests/attributes.rs` (new)

**Interfaces:**
- Produces:

```rust
/// Which prop a `Text` shorthand row edits: `style.<style_key>` when
/// `style` is an object literal whose own properties set `style_key`,
/// `shorthand` otherwise (also without a `style` attribute). Locked
/// `StyleMaySet` when `style` is written but is not an object literal, or
/// the object has a spread; `SyntaxError` / `ElementNotFound` as `resolve`.
pub fn text_style_path(text: &str, element: &ElementRef, shorthand: &str, style_key: &str) -> Result<String, LockReason>;
/// The attribute names written on `element`, in source order, without
/// spreads, `key`, and `ref`. A name written twice is listed once, at its
/// last position. Empty when the text does not parse or no element starts there.
pub fn written_attributes(text: &str, element: &ElementRef) -> Vec<String>;
// LockReason:
StyleMaySet,
```
- `reason_text(&LockReason::StyleMaySet)` is `"May be set by \`style\`"`.

- [ ] **Step 1: Write the failing tests** in `tests/attributes.rs`:
  - `shorthand_without_style`: `<Text fontSize={48}>A</Text>` → `Ok("fontSize")`.
  - `style_literal_sets_the_key`: `<Text style={{ fontSize: 48 }}>A</Text>` with (`fontSize`, `fontSize`) → `Ok("style.fontSize")`; with (`color`, `fill`) on `style={{ fill: '#fff' }}` → `Ok("style.fill")`.
  - `style_literal_without_the_key`: `<Text style={{ fontWeight: 700 }} fontSize={48}>A</Text>` → `Ok("fontSize")`.
  - `style_not_a_literal_locks`: `style={mono(20)}` → `Err(LockReason::StyleMaySet)`; `style={{ ...base, fontWeight: 700 }}` for `fontSize` → `Err(StyleMaySet)`.
  - `quoted_style_keys_count`: `style={{ 'fontSize': 48 }}` → `Ok("style.fontSize")`.
  - `attributes_in_source_order`: `<PriceTag key="a" x={40} {...rest} label="Sale" ref={r} x={41} />` → `["label", "x"]`.
- [ ] **Step 2: Run** `cargo test -p celesta-source-edit --test attributes` — FAIL (no functions).
- [ ] **Step 3: Implement** with `Gather` (the visitor `resolve` uses) or a small visitor over the element's opening tag; reuse `StyleProperty` from `shape.rs` for the object-literal reading so the rules match `resolve`'s `style.<key>` reading.
- [ ] **Step 4: Run** `cargo test -p celesta-source-edit` — PASS.
- [ ] **Step 5: Commit** — `feat(source-edit): tell a Text shorthand from its style property and list a call's attributes`

---

### Task 4: Inputs resolved at the call site

2a's `resolve_inputs` returns parameter inputs as `CallSiteNotFound` and has known gaps the rows would show. This task fixes them.

**Files:**
- Modify: `crates/source-edit/src/inputs.rs`, `crates/source-edit/src/pass_through.rs` (a name-based entry), `crates/source-edit/src/resolve.rs` (only if the locked span moves into the `Read`/lock data)
- Test: `crates/source-edit/tests/inputs.rs`

**Interfaces:**
- Changes:

```rust
/// As in 2a, plus: an input that is a parameter of the component enclosing
/// `element` resolves at `call_site` (the same text/element pair
/// `resolve_through` takes) as `PassThrough { call_site, param, target }`,
/// following 2a's pass-through rules (literal, `Missing` with the
/// parameter's default, `ParameterOfCaller` for a second hop); without a
/// call site it stays `CallSiteNotFound`.
pub fn resolve_inputs(
    text: &str,
    element: &ElementRef,
    prop: &str,
    ctx: &ResolveContext,
    call_site: Option<(&str, &ElementRef)>,
) -> Vec<(String, EditTarget)>;
```
- `pass_through.rs` gains `pub(crate) fn resolve_parameter_through(text, element, name: &str, ctx, call_site) -> Option<EditTarget>` (or an equivalent the implementer names): the pass-through for identifier `name` read inside `element`'s enclosing component, `None` when `name` is not one of its parameters. `resolve_through` and `resolve_inputs` both use it, so the parameter rules live in one place.

- [ ] **Step 1: Write the failing tests** in `tests/inputs.rs` (keep the existing ones passing):
  - `parameter_input_resolves_at_the_call_site`: element text `function Tag({ padding = 8 }) { const w = 100; return <Rect width={w + padding * 2} height={10} />; }`, call site text `<Tag padding={16} />` → inputs `[("w", Literal 100 ...), ("padding", PassThrough { param: "padding", target: Literal 16 })]`.
  - `parameter_input_missing_at_the_call_site`: call site `<Tag />` → `padding` is `PassThrough(Missing { default: Some(Number(8)) })`.
  - `parameter_input_without_a_call_site`: `None` → `CallSiteNotFound { component: "Tag" }` (2a behavior).
  - `an_input_used_twice_in_one_expression_is_one_place`: `const padding = 16;` and `width={padding + padding}` → `padding` is `Literal 16`, not `SharedBinding`. (Uses inside the locked expression count as one place; `SharedBinding` still applies when `padding` is also read elsewhere.)
  - `a_lambda_parameter_only_hides_its_own_body`: `width={items.map((padding) => padding).length + padding}` with `const padding = 16` outside → `padding` is listed (`Literal 16`). Today any binding of the name anywhere in the expression hides it.
  - `every_callee_form_skips_only_the_function_name`: `width={new Box(size).w + tag\`${gap}\`.length + fns[i](depth)}` with `const size = 1, gap = 2, depth = 3, i = 0` → the input names are exactly `["size", "gap", "fns", "i", "depth"]`. `Box` (a `new` callee) and `tag` (a tag) name functions and are not listed; a callee that is not a plain name (`fns[i]`) is walked like any expression, so its member object `fns` and its index `i` are listed (today the whole callee is dropped). `fns` then locks with its own reason, which is fine.
- [ ] **Step 2: Run** `cargo test -p celesta-source-edit --test inputs` — the new tests FAIL.
- [ ] **Step 3: Implement.** The span the inputs are read from is the one `resolve` locked on: derive it from the same `Read` data `resolve` uses instead of re-deriving the Expression rules in `locked_span` (2a review carry-over); scope bound names to the binding's own function body; skip only the callee position of `new` and tagged templates as of plain calls.
- [ ] **Step 4: Run** `cargo test -p celesta-source-edit` — PASS. Update the editor call site of `resolve_inputs` if any (there is none in 2a).
- [ ] **Step 5: Commit** — `feat(source-edit): resolve an expression's parameter inputs at the call site`

---

### Task 5: Element targets keyed by prop name

2a's `TransformTargets`, `commit_files`, and `TransformEdit` are keyed by `TransformProp` and write `f64`. Generalize them in place; behavior stays the same in this task (still only the transform props are resolved).

**Files:**
- Rename: `crates/editor/src/transform_edit.rs` → `crates/editor/src/element_edit.rs` (`git mv`; update `mod` and `use` lines)
- Modify: `element_edit.rs`, `transform_rows.rs`, `transform_drag.rs`, `transform_handles.rs`, `layer_panel.rs`, `impact.rs` if it names the types, `tests.rs`
- Test: `crates/editor/src/tests.rs`

**Interfaces:**
- Produces (in `element_edit.rs`):

```rust
/// A row the editor writes: a table prop by its table key (`fontSize`, even
/// when it writes `style.fontSize`). Task 6 adds `Input`, Task 10 `CallSite`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum PropKey { Prop(String) }

pub(crate) struct Target {
    pub(crate) file: PathBuf,
    /// The prop path resolved on the element (`fontSize`, `style.fontSize`, `children`).
    pub(crate) path: String,
    pub(crate) target: EditTarget,
    pub(crate) call_site: Option<SourcePos>,
}
pub(crate) struct ElementTargets {          // was TransformTargets
    pub(crate) file: PathBuf,
    pub(crate) files: Vec<PathBuf>,
    pub(crate) element: ElementRef,
    pub(crate) targets: Vec<(PropKey, Target)>,
}
impl ElementTargets {
    pub(crate) fn target(&self, key: &PropKey) -> Option<&EditTarget>;
    pub(crate) fn file_of(&self, key: &PropKey) -> &Path;
    pub(crate) fn transform(&self, prop: TransformProp) -> Option<&EditTarget>; // adapter for drag/handles
    pub(crate) fn written_at(&self) -> Option<&SourcePos>; // transform keys only (Rulings)
}
impl TransformProp { pub(crate) fn key(self) -> PropKey; } // PropKey::Prop(self.name())
pub(crate) fn commit_files(texts: &Texts, expected: &ElementTargets, report: &LayerSourceReport,
    values: &[(PropKey, EditValue)]) -> Result<Vec<(PathBuf, String)>, CommitError>;
pub(crate) enum CommitError { SourceChanged, NotEditable(String /* row label */), NotANumber, Apply(ApplyError) }
pub(crate) struct ElementEdit { /* was TransformEdit; same fields */ }
impl ElementEdit {
    pub(crate) fn record_written(&mut self, layer: Option<&str>, values: &[(PropKey, EditValue)]);
    pub(crate) fn written_value(&self, selected: Option<&str>, key: &PropKey) -> Option<&EditValue>;
    // begin / finish / invalidate / expire / reset / unavailable / resolved / targets / error / set_error unchanged
}
impl EditorView {
    pub(crate) fn commit_element(&mut self, values: Vec<(PropKey, EditValue)>,
        done: impl FnOnce(&mut Self, Result<(), String>, &mut Context<Self>) + 'static, cx: &mut Context<Self>); // was commit_transform
    pub(crate) fn element_targets(&self) -> Option<(&ElementTargets, &LayerSourceReport)>; // was transform_targets
}
```
- `TransformProp::received` is removed (2a carry-over: duplicated reader); callers use `report.props.get(prop.name())` and `ReceivedValue` → number.
- `EditorView.transform` field becomes `element_edit: ElementEdit`; `sync_transform_targets` becomes `sync_element_targets`.
- `commit_files` refuses a non-finite `EditValue::Number` with `NotANumber`; a `Keyframe` target given a non-number is `NotEditable` (`to_edit` returns `None`).

- [ ] **Step 1: Write the failing tests** (beside the existing `commit_files` tests): `commit_files_writes_a_string` (a hand-built `ElementTargets` for `<Text>Sale</Text>` with `PropKey::Prop("children")` → `Literal { JsxText }`, value `String("Sold out")` → text `<Text>Sold out</Text>`), `commit_files_writes_a_bool` (`<Video loop />`, `loop` → `Literal { BareTrue }`, `Bool(false)` → `<Video loop={false} />`), and `written_values_keep_their_type` (`record_written` with a `String` value, `written_value` returns it). Convert the existing `TransformProp` tests to `PropKey` (`TransformProp::X.key()`) without changing their expectations.
- [ ] **Step 2: Run** `cargo test -p celesta-editor commit_files_writes_a` — FAIL to compile.
- [ ] **Step 3: Implement** the rename and generalization; drag, handles, and rows go through `ElementTargets::transform(prop)` and `TransformProp::key()`.
- [ ] **Step 4: Run** `cargo test -p celesta-editor` — PASS, every existing test unchanged in meaning; `cargo clippy -p celesta-editor --all-targets` clean.
- [ ] **Step 5: Commit** — `refactor(editor): key element targets by prop name and write typed values`

---

### Task 6: Resolve the whole table, inputs included

**Files:**
- Modify: `crates/editor/src/element_edit.rs` (`resolve_targets`, `Target.inputs`, `PropKey::Input`, `commit_files`'s comparison), `crates/editor/src/transform_rows.rs` (only what the new signature forces; the rows UI is Task 8)
- Test: `crates/editor/src/tests.rs`

**Interfaces:**
- Consumes: `PrimitivePropertyTable` (Task 2), `text_style_path` / `StyleMaySet` (Task 3), `resolve_inputs(…, call_site)` (Task 4), `ElementTargets` / `PropKey` / `commit_files` (Task 5).
- Produces:

```rust
pub(crate) enum PropKey {
    Prop(String),
    /// Input `name` of the expression row `row` (a table key).
    Input { row: String, name: String },
}
pub(crate) struct Target { /* Task 5 fields */ pub(crate) inputs: Vec<(String, Target)> } // inputs only on an Expression-locked row
/// Every row of `table` (or the transform props when `table` is `None`),
/// then their inputs; the call site is read once, when any row or input
/// passes through.
pub(crate) fn resolve_targets(
    read: impl Fn(&Path) -> Result<String, String>,
    report: &LayerSourceReport,
    table: Option<&PrimitivePropertyTable>,
) -> Result<ElementTargets, String>;
/// The table field of `key` as a placeholder-able default.
pub(crate) fn table_default(field: &PrimitivePropertyField) -> Option<EditValue>;
```
- Row order in `ElementTargets.targets`: the table's order; within `Transform`, `scale` is replaced by `scaleX`, `scaleY` (in that order, at `scale`'s place) when either is written as an attribute (2a rule).
- `Text` rows `fontFamily`/`fontSize`/`fontWeight`/`color` resolve the path `text_style_path` returns (`color` with style key `fill`); `Err(reason)` makes the row `Locked(reason)` with `path` = the shorthand.
- Kind check after resolving: a `Literal` whose `EditValue` kind does not fit the field (number ↔ `Number`; `boolean` ↔ `Bool`; `string`/`text`/`color`/`select`/`path` ↔ `String`) becomes `Locked(Expression { source })`, `source` the literal's text from the file.
- Inputs: for each row (and only rows) locked `Expression`, `resolve_inputs(text, element, path, ctx, call_site)`; each input becomes a `Target` (file: the call site's for a `PassThrough`, `call_site` set likewise). Inputs are not expanded further.
- `commit_files` compares rows and inputs (same keys, files, places) and writes `PropKey::Input` values to the input's target.
- `sync_element_targets` passes `self.primitive_table(&report.chain[0].name)`; 2a's outside-project lock applies to inputs too.

- [ ] **Step 1: Write the failing tests:**
  - `a_table_resolves_every_row_in_order`: a `Rect` table (Transform: x, y, rotation, scale, opacity; Appearance: fill, cornerRadius) on `<Rect x={10} fill="#fff" width={10} height={10} />` → keys in that order; `fill` is `Literal String "#fff"`, `cornerRadius` is `Missing`, `scale` is `Missing`.
  - `per_axis_scale_replaces_scale_in_place`: `scaleX={2}` written → keys `x, y, rotation, scaleX, scaleY, opacity, …`.
  - `text_shorthand_edits_the_style_property`: `<Text style={{ fontSize: 48 }} fontWeight={700}>A</Text>` → `fontSize` row path `style.fontSize`, `fontWeight` row path `fontWeight`; `style={mono(20)}` → both `Locked(StyleMaySet)`.
  - `a_literal_of_another_kind_locks`: `<Text anchorY="baseline">A</Text>` with Transform `anchorY: number` → `Locked(Expression { source: "\"baseline\"" })`.
  - `an_expression_row_lists_its_inputs`: `function Tag({ padding = 8 }) { const w = 100; return <Rect width={w + padding * 2} height={10} />; }` called as `<Tag padding={16} />` in another file → the `width` row is `Expression`-locked with inputs `w` (`Literal 100`, the element's file) and `padding` (`PassThrough`, the call site's file, `call_site` set); `files` lists both files.
  - `committing_an_input_writes_its_target`: `commit_files` with `PropKey::Input { row: "width", name: "padding" }` and `Number(24)` returns only the call site's file, with `padding={24}`.
  - `a_text_with_outer_spaces_is_written_in_braces` (Review Focus): `children` of `<Text>Sale</Text>` set to `" Sale "` → `<Text>{" Sale "}</Text>`.
  - `a_font_row_beside_a_style_object_inserts_the_shorthand` (Review Focus): `<Text style={{ fontWeight: 700 }}>A</Text>` with a `Text` table → the `fontSize` row's path is `fontSize` (`Missing`, the object does not set it); committing `Number(48)` gives `<Text style={{ fontWeight: 700 }} fontSize={48}>A</Text>`; writing it with `write_all_recorded` and undoing restores the original bytes (2a's multi-file history test helpers). A second case, `style={{ fontSize: 40, fontWeight: 700 }}`, writes `fontSize: 48` inside the object.
- [ ] **Step 2: Run** the new tests — FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p celesta-editor` — PASS.
- [ ] **Step 5: Commit** — `feat(editor): resolve every row of a primitive's property table`

---

### Task 7: Shared property controls

The project Properties panel (`property_panel.rs`, `property_panel_view.rs`) already builds text, switch, select, color, and path controls, wired to `this.property_panel.rows`. Extract them so the element rows (Task 8) use the same controls. No visible change in this task.

**Files:**
- Create: `crates/editor/src/property_controls.rs`
- Modify: `crates/editor/src/property_panel.rs`, `crates/editor/src/property_panel_view.rs`, `crates/editor/src/main.rs` (or wherever `mod` lines live)
- Test: existing `property_panel.rs` tests; one new test in `property_controls.rs`

**Interfaces:**
- Produces:

```rust
/// What a control is for.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ControlKind { Number, String, Text, Boolean, Color, Select(Vec<String>), Path }
/// What the person did in a control; each panel maps it to its own rows.
pub(crate) enum ControlEvent { Edited, Submit { blur: bool }, Escape, Toggled(bool), Chosen(String), ColorPicked, ColorPickerClosed, ChoosePath, ClearPath }
pub(crate) enum Control {
    Text(Entity<InputState>),                 // Number, String, Path; Text is multi-line
    Switch,
    Select(Entity<SelectState<Vec<String>>>),
    Color { text: Entity<InputState>, picker: Entity<ColorPickerState> },
}
pub(crate) struct BuiltControl { pub(crate) control: Control, pub(crate) subscriptions: Vec<Subscription> }
pub(crate) fn build_control(
    kind: &ControlKind,
    window: &mut Window,
    cx: &mut Context<EditorView>,
    on: impl Fn(&mut EditorView, ControlEvent, &mut Window, &mut Context<EditorView>) + Clone + 'static,
) -> BuiltControl;
/// Renders `control`; `shown_bool` is the switch state, `placeholder` the dimmed value.
pub(crate) fn render_control(id: impl Into<ElementId>, kind: &ControlKind, control: &Control, locked: bool,
    shown_bool: bool, hint: Option<String>, cx: &mut Context<EditorView>,
    on: impl Fn(&mut EditorView, ControlEvent, &mut Window, &mut Context<EditorView>) + Clone + 'static) -> AnyElement;
```
- `ControlKind::Text` builds `InputState::new(window, cx).multi_line(true).submit_on_enter(true).auto_grow(1, 6)` (gpui-base 0.6.6: plain Enter emits `PressEnter`, Shift+Enter inserts a newline); its `PressEnter` maps to `Submit { blur: false }` like single-line inputs.
- The project panel keeps `RowState`, `CommitQueue`, and its `Value`-based commit flow; only control construction and rendering move. `RowControl` is replaced by `Control`.

- [ ] **Step 1: Write the failing test** `input_options_per_kind` in `property_controls.rs` for a pure `pub(crate) fn input_options(kind: &ControlKind) -> Option<(bool /* multi_line */, bool /* submit_on_enter */)>` that `build_control` reads: `Text` → `Some((true, true))`; `Number`, `String`, `Path`, `Color` → `Some((false, false))`; `Boolean`, `Select(_)` → `None` (no text input).
- [ ] **Step 2: Run** it — FAIL.
- [ ] **Step 3: Implement** the extraction; the project panel maps `ControlEvent`s to its existing handlers (`commit_property`, `submit_property_text`, `restore_property_text`, `color_picker_notified`, `choose_property_path`).
- [ ] **Step 4: Run** `cargo test -p celesta-editor` — PASS; `cargo clippy -p celesta-editor --all-targets` clean.
- [ ] **Step 5: Commit** — `refactor(editor): share property controls between panels`

---

### Task 8: Element property rows

Replaces the fixed six-field Transform group with a section per table group, a row per resolved target, and a control per type.

**Files:**
- Create: `crates/editor/src/element_rows.rs` (row state, sync, commit), `crates/editor/src/element_rows_view.rs` (render)
- Delete: `crates/editor/src/transform_rows.rs` (move `set_at_text`, the keyframe jumps, and the field text logic into the new modules)
- Modify: `crates/editor/src/layer_panel.rs` (`layer_edit_section` renders the sections), `crates/editor/src/view.rs` (state field), `crates/editor/src/main.rs`
- Test: `crates/editor/src/tests.rs`

**Interfaces:**
- Consumes: `ElementTargets`, `PropKey` (Tasks 5–6), `build_control` / `render_control` / `ControlKind` (Task 7), `PrimitivePropertyTable` (Task 2).
- Produces:

```rust
/// The control kind of a table field.
pub(crate) fn control_kind(field: &PrimitivePropertyField) -> ControlKind;
/// What a row shows for `target`: the value, and the dimmed placeholder of
/// an absent prop (Rulings: source default, received, table default).
pub(crate) fn row_text(key: &str, target: &Target, field: Option<&PrimitivePropertyField>, report: &LayerSourceReport) -> (String, String);
/// Parses typed text for `kind`; the error is the row's message.
pub(crate) fn parse_row_value(kind: &ControlKind, field: Option<&PrimitivePropertyField>, text: &str) -> Result<EditValue, String>;
/// The row label: the table key in words (`cornerRadius` → "Corner radius",
/// `children` → "Text", `scaleX` → "Scale X", `x` → "X", `anchorX` → "Anchor X").
pub(crate) fn row_label(key: &str) -> String;
pub(crate) struct ElementRows { /* per PropKey: BuiltControl, edited, shown, layer, invalid */ }
```
- Sections: under the selection header, one `inspector_section(group name)` per table group, in order, each with its rows; with no table, one "Transform" section from the transform targets. An input sub-row sits under its row, indented one step, labeled with the input name, with its own control (`Number`, or `String` for a string literal), its own "Set at …" line when it passes through, and its lock reason when locked.
- Row states as 2a's Transform rows: `Literal` editable; `Keyframe` editable with the filled diamond; between keyframes with the diamond, reason, and jump buttons; `Missing` editable with the placeholder; `Locked` disabled with the lock icon and `reason_text`; a pass-through shows the inner target's state and "Set at `<file>`:`<line>`" with `CornerDownRight`.
- Typing commits on Enter or blur (multi-line: Enter, not Shift+Enter) through `commit_element(vec![(key, value)])`; a switch, a select choice, a picked color (on picker close), and "Choose…" commit at once. Each commit is one history entry. Typed text that does not parse shows the row's message and is not written; on blur the field shows the source's value again. Written values show until the reload resolves (`written_value`), for every type.
- `parse_row_value`: `Number` → finite `f64` within `min`/`max` ("Enter a number", "Enter a number from 0 to 1"); `String`/`Text`/`Path` → the text as typed (`Text` keeps newlines); `Color` → the Rulings' rule; `Select` → any of the options.
- Path: "Choose…" opens the file dialog as the project panel does and writes `import_path(entry_dir, entry_dir, chosen)`.

- [ ] **Step 1: Write the failing tests** (pure helpers):
  - `row_labels`: `cornerRadius` → `Corner radius`, `children` → `Text`, `scaleX` → `Scale X`, `anchorY` → `Anchor Y`, `strokeWidth` → `Stroke width`, `x` → `X`, `opacity` → `Opacity`.
  - `control_kinds`: each `PrimitivePropertyField` variant to its `ControlKind`.
  - `parse_row_values`: `"12.5"` number → `Number(12.5)`; `"1.5"` with `max: 1` → `Err("Enter a number from 0 to 1")`; `"#ff8800"` color → `String("#ff8800")`; `"accent"` color → `String("accent")`; `"#ff88"` color → error; `"fill"` select with options `contain`, `cover` → error; `"A\nB"` text → `String("A\nB")`.
  - `select_value_outside_the_options_is_shown` (Review Focus): `row_text` of `Literal String "fill"` on a `fit` select row → `("fill", "")`.
  - `missing_rows_show_the_table_default`: `Missing { default: None }`, nothing received, field `num(2)` → `("", "2")`; received `5` → `("", "5")`.
  - `the_scale_axis_placeholder_of_a_parameter_default` (2a carry-over): `PassThrough(Missing { default: Some(Number(3)) })` for `scaleX`, received `scale` 2 → `("", "3")`; an element-level `Missing { default: Some(1) }` for `scaleX` with received `scale` 2 → `("", "2")`.
  - `locked_rows_show_the_received_value`: `Locked(Expression)` with received `String("Sale")` on a text row → `("Sale", "")`.
- [ ] **Step 2: Run** them — FAIL.
- [ ] **Step 3: Implement**, then build and launch the editor on `examples/feature-tour/film.tsx` (`pnpm --filter "@celesta/cli..." build`, `cargo build -p celesta-editor`, run `./target/debug/celesta-editor examples/feature-tour/film.tsx` in the background) and confirm it starts; the GUI check itself is manual (handoff).
- [ ] **Step 4: Run** `cargo test -p celesta-editor` — PASS; clippy clean.
- [ ] **Step 5: Commit** — `feat(editor): edit every table prop in the Properties tab`

---

### Task 9: Scrub a number by dragging its label

**Files:**
- Modify: `crates/editor/src/element_rows_view.rs` (label drag), `crates/editor/src/element_rows.rs` (scrub state, commit on release), `crates/editor/src/transform_drag.rs` (`override_sites` takes `PropKey`s and the row's resolved `path`, so drags and scrubs share it), `packages/react/src/jsx-dev-runtime.ts` (`overridden` sets `style.<key>` inside a copy of `style`: a `Text` font row scrubs `style.fontSize`)
- Test: `crates/editor/src/tests.rs`, `packages/react/test/prop-overrides.test.mts`

**Interfaces:**
- Produces:

```rust
/// A scrub in progress on one number row.
pub(crate) struct Scrub { key: PropKey, start_x: f32, start_value: f64, value: f64, sites: Vec<PropOverride> /* positions, fixed for the scrub */ }
/// The value a scrub `dx` pixels from its start gives: `start + dx * step`,
/// `step` 1 per pixel, 0.01 for a field with `max <= 1` (opacity, ratios),
/// clamped to `min`/`max`; Shift multiplies the step by 10.
pub(crate) fn scrub_value(start: f64, dx: f32, field: Option<&PrimitivePropertyField>, shift: bool) -> f64;
```
- Mouse down on an editable number row's label starts a scrub (cursor `ew-resize` on hover); moving sends the frame with a prop override at the row's write position (the element's prop, or the call site's `param` for a pass-through; an input's const is not overridable, so input rows do not scrub); release writes once via `commit_element` (one history entry); Escape or a release without movement writes nothing and drops the overrides. Locked, keyframe, and between-keyframes rows do not scrub (a keyframe has no single prop value to override).
- A scrub and a canvas drag never run together: starting either ends the other without writing.

- [ ] **Step 1: Write the failing tests:** `scrub_steps` (`scrub_value(10.0, 5.0, num(), false) == 15.0`; with `num(1, 0, 1)` start 0.5, dx 10 → 0.6; dx 100 → 1.0 clamped; Shift with `num()` dx 5 → 60.0); `scrub_overrides_the_row_prop` (a `Literal` `cornerRadius` row → one override at the element's position with prop `cornerRadius`; a `Text` `fontSize` row whose path is `style.fontSize` → prop `style.fontSize`; a `PassThrough` row → at the call site with `param`; positions equal on every frame of the scrub — reuse 2a's drag override test helpers). In `prop-overrides.test.mts`: `a style key override replaces that key only` (`<Text style={{ fontSize: 40, fontWeight: 700 }}>` with override `style.fontSize` = 48 draws `fontSize` 48 and keeps `fontWeight` 700; the written `style` object is not mutated; without `style` written, the override creates `{ fontSize: 48 }`).
- [ ] **Step 2: Run** — FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p celesta-editor` — PASS; clippy clean.
- [ ] **Step 5: Commit** — `feat(editor): scrub number props from their labels`

---

### Task 10: The Component section

**Files:**
- Modify: `crates/editor/src/element_edit.rs` (`PropKey::CallSite`, `ElementTargets.component`, resolution in `resolve_targets`, `commit_files` comparison), `crates/editor/src/element_rows.rs` / `element_rows_view.rs` (the section)
- Test: `crates/editor/src/tests.rs`

**Interfaces:**
- Consumes: `written_attributes` (Task 3), `component_schemas` (the editor's existing `BTreeMap<String, ComponentPropertySchema>`), rows (Task 8).
- Produces:

```rust
pub(crate) enum PropKey { Prop(String), Input { row: String, name: String }, CallSite(String) }
pub(crate) struct ComponentTargets {
    /// The tag at the call site (`PriceTag`), as `tag_name` reads it.
    pub(crate) name: String,
    pub(crate) call_site: SourcePos,
    /// In source order; `kind` from the registered schema or the literal.
    pub(crate) props: Vec<(String, Target, ControlKind)>,
}
// ElementTargets:
pub(crate) component: Option<ComponentTargets>,
```
- Shown when the owner call site (2a's `caller`) is found and readable. Each attribute from `written_attributes(call_site_text, call)` is resolved with `resolve(call_site_text, call, name, ctx)` where `ctx` has `primitive: false, component_props: true, render_count: 1, received: None` and the report's candidate frames. Skipped: names that a primitive row of this element passes through to (its `PassThrough.param` at this call site) — they already show "Set at …".
- Kind: `component_schemas[name]`'s field for the attribute when registered (`ComponentPropertyField` → `ControlKind` by type), else `Literal`'s value kind (`Number` → `Number`, `String` → `String`, `Bool` → `Boolean`), else (locked, missing) `String` (the row is disabled anyway).
- Section title: "Component" with the tag name as the first row's context ("PriceTag" shown in the section header, e.g. "Component · PriceTag"). Rows use Task 8's rendering; the outside-project lock applies.

- [ ] **Step 1: Write the failing tests:** `component_section_lists_the_call_sites_props` (`function PriceTag({ x = 0, label }) { return <Rect x={x} width={10} height={10} />; }` called as `<PriceTag x={40} label="Sale" tone={2} on />` → `component.props` names `["label", "tone", "on"]` with kinds `String`, `Number`, `Boolean`; `x` absent — Review Focus); `component_props_use_the_registered_schema` (schema `PriceTag: { label: color }` → `label` kind `Color`); `component_prop_commit_writes_the_call_site` (`PropKey::CallSite("label")`, `String("Sold")` → only the call site's file changes, `label="Sold"`); `no_component_section_without_a_call_site` (an element written directly in the scene → `component == None`).
- [ ] **Step 2: Run** — FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p celesta-editor` — PASS; clippy clean.
- [ ] **Step 5: Commit** — `feat(editor): edit a component call's props in a Component section`

---

### Task 11: One temp-file writer

2a carry-over: `edit_history.rs`'s `write_temp` repeats the temp-file logic of `celesta_react_bridge::replace_file`.

**Files:**
- Modify: `crates/react-bridge/src/props_edit.rs` (split `replace_file` into a public `write_temp(file, text) -> io::Result<PathBuf>` and the rename), `crates/react-bridge/src/lib.rs`, `crates/editor/src/edit_history.rs` (use it; delete the copy)
- Test: existing `edit_history` tests (incl. `a_failed_temp_write_renames_nothing`) and `replace_file` tests

- [ ] **Step 1:** Run `cargo test -p celesta-editor edit_history` and `cargo test -p celesta-react-bridge replace_file` — PASS before the change (record the counts).
- [ ] **Step 2: Implement**: one temp name scheme (the one `a_failed_temp_write_renames_nothing` blocks: `.<name>.tmp-<pid>` in the same directory) used by both.
- [ ] **Step 3: Run** both crates' suites — PASS, same counts.
- [ ] **Step 4: Commit** — `refactor: write temp files in one place`

---

### Task 12: Docs and handoff

**Files:**
- Modify: `packages/website/content/en/docs/01-(getting-started)/03-preview.md`, `packages/website/content/ja/docs/01-(getting-started)/03-preview.md`
- Create: `docs/handoff/2026-10-24-property-sections.md`
- Modify: `docs/superpowers/specs/2026-10-23-editor-property-editing-design.md` only where this plan's Rulings settle what the spec left open (field types without `label`, the color-name rule, the shorthand → `style.fill` mapping, the scrub preview)

- [ ] **Step 1: Write the docs**: the preview page's Properties tab part explains the sections per primitive, the controls (Enter / Shift+Enter in a text row, label scrub with Shift for ×10, Escape), "Set at …" rows, inputs under a computed value, the Component section, and that a `Text` font row edits `style` when `style` sets it. Japanese page mirrors the English one.
- [ ] **Step 2: Write the handoff**: what changed per module, the Rulings that differ from the spec, the 2a carry-overs closed (inputs gaps, scale-axis placeholder, duplicated readers, `write_temp`) and withdrawn (self-closing children), what 2c/2d build on (`size` in the tables, `PropKey`, shared controls, multi-line text control), and "Verify by hand": edit a `Rect`'s fill with the picker and its corner radius by scrubbing; edit a `Text`'s content with Shift+Enter, and its font size where `style` sets it; edit an input of a computed width; edit a Component-section prop; undo each in one step. No example scene changes: the feature tour has no editor-only content these rows supersede.
- [ ] **Step 3: Run** the website's checks if the docs package has them (`pnpm --filter @celesta/website build` or its lint), else none.
- [ ] **Step 4: Commit** — `docs(editor): describe the Properties sections`
