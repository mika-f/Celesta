# Property Editing Engine (Design 2a) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The source editor can read and write typed values (numbers, strings, booleans, text children, `style` object properties), follow a prop to the component's call site, list an expression's editable inputs, and commit one edit across several files with one undo step — and the transform write-back uses it, so `x`/`y`/`rotation`/`scale` passed at a component's call site become draggable.

**Architecture:** `crates/source-edit` gains `EditValue`, literal forms that remember how to write a value back, `PassThrough` targets, and an inputs resolver; it stays GPUI-free and works on source texts. The runtime reports every scalar prop the innermost primitive received. The editor finds the owner call site from the source chain, resolves targets per file, writes several files atomically, and keeps one history entry per commit.

**Tech Stack:** Rust (`oxc_parser`/`oxc_ast` in `celesta-source-edit`, GPUI via gpui-kit in `celesta-editor`, serde in `celesta-react-bridge`), TypeScript runtime (Vitest).

**Spec:** `docs/superpowers/specs/2026-10-23-editor-property-editing-design.md` (part 2 of `docs/superpowers/specs/2026-10-23-editor-element-editing-overview-design.md`). Builds on design 1 (branch `claude/celesta-editor-element-controls-29f8f6`).

Design 2 is planned in four parts, each its own PR stacked on the previous one:

| Part | Plan | Spec sections |
| --- | --- | --- |
| 2a | this plan | Typed edit values, Pass-through to the call site, Non-primitive elements, Multi-file edits, Inputs (resolution only) |
| 2b | later | Property tables, Properties tab, Component section, Inputs (rows), `Text` shorthand vs `style` |
| 2c | later | Canvas: Pivot, Resize |
| 2d | later | Inline text editing |

## Global Constraints

- `crates/source-edit` has no GPUI dependency and never re-prints the AST: edits are byte-span replacements and insertions; formatting and comments survive.
- Numbers are written as today (`format_number`: integer within 0.005, else up to 2 decimals, trailing zeros trimmed, `-0` as `0`).
- One gesture or one committed field is one history entry, whatever number of files it touches. The history holds at most 100 entries.
- A commit re-resolves every target against every file it writes; if any target moved, nothing is written and the error is "The source changed while editing".
- Pass-through follows one hop. Renamed destructuring (`{ label: text }`) and `props.label` (a parameter named `props`) are followed; anything else stays `UnresolvedIdentifier`.
- Tests: Rust integration tests in `crates/source-edit/tests/*.rs`, unit tests in `crates/*`; Vitest `.mts` under `packages/*/test/` importing `../src`.
- UI copy is English, inspector style (radius 0, no gradients, no uppercase labels, lucide icons).
- No `git commit --amend`; separate commit and test commands.

## Review Focus

- **A call site in another file whose text changed between resolve and commit** (the user edited the scene file in another editor). Nothing is written to either file. Test in Task 7.
- **A string value with the literal's own quote or a newline** (`label="It's"` typed as `Say "hi"`). Written as `{"Say \"hi\""}`, never a broken attribute. Test in Task 1.
- **A component whose parameter has a default and whose call site omits the prop** (`function Tag({ x = 40 })`, `<Tag />`). The target is `PassThrough(Missing)` at the call site with default 40; writing inserts `x={…}` at the call site, not in the component. Test in Task 3.
- **An undo after a two-file commit where one file was edited outside Celesta.** The whole entry is refused and neither file changes. Test in Task 6.
- **A temp write failing for the second file of a commit** (read-only directory). The first file is not renamed either. Test in Task 6.

---

### Task 1: Typed values and literal forms for attributes

**Files:**
- Modify: `crates/source-edit/src/resolve.rs` (`EditTarget`, `Shape`, `shape_of`, `resolve_identifier`, `check_received`, `default_for`)
- Modify: `crates/source-edit/src/apply.rs` (`Edit`, `apply`)
- Modify: `crates/source-edit/src/lib.rs` (exports)
- Modify: `crates/editor/src/transform_edit.rs`, `transform_rows.rs`, `transform_handles.rs` (adapt to `EditValue`; numbers only for now)
- Test: `crates/source-edit/tests/values.rs` (new), existing `tests/resolve.rs`, `tests/locate_apply.rs`

**Interfaces:**
- Produces:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum EditValue { Number(f64), String(String), Bool(bool) }
impl EditValue { pub fn as_number(&self) -> Option<f64>; }

/// How a literal is written, so a write keeps its style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiteralForm {
    Number,                       // 12, -12
    Quoted(char),                 // name="…" / name='…' (attribute string, no container)
    ContainerQuoted(char),        // name={'…'} / name={"…"}
    Template,                     // name={`…`} without ${}
    BareTrue,                     // `name` alone
    Bool,                         // name={true} / {false}
}

pub enum EditTarget {
    Literal { span: Span, value: EditValue, form: LiteralForm },
    Keyframe { span: Span, value: f64, frame: f64 },
    Missing { insert_at: u32, default: Option<EditValue> },
    Locked(LockReason),
}
impl EditTarget { pub fn to_edit(&self, prop: &str, value: &EditValue) -> Option<Edit>; }

pub enum Edit {
    /// Replaces `start..end` with `value` written in `form`.
    Replace { start: u32, end: u32, value: EditValue, form: LiteralForm },
    /// Inserts ` name=…` at `at`.
    Insert { at: u32, name: String, value: EditValue },
}
pub fn write_value(value: &EditValue, form: LiteralForm) -> String;   // the replacement text
pub fn default_for(prop: &str) -> Option<EditValue>;                // Number(1) for scale props, Number(0) for x/y/rotation, None otherwise
pub struct ResolveContext { /* existing fields */ pub received: Option<EditValue>, /* … */ }
```

Writing rules (exact):

| Value | Form in | Written |
| --- | --- | --- |
| Number | any | `format_number` (a `Replace` of a `Number` literal keeps the span, so `-12` is replaced whole) |
| String | `Quoted(q)` | `q…q` when the value contains neither `q`, `&`, nor a newline (JSX attribute strings have no escapes); otherwise the whole attribute value becomes `{"…"}` (JSON-escaped). For `Quoted` the `Replace` span covers the quotes. |
| String | `ContainerQuoted(q)` | `q…q` with `\` and `q` escaped; newline as `\n` |
| String | `Template` | `` `…` `` with `` ` ``, `\`, `${` escaped |
| Bool true | `BareTrue` | nothing (the attribute stays bare) |
| Bool false | `BareTrue` | `={false}` (the literal's `span` is the empty point right after the attribute name, so the result is `name={false}`) |
| Bool | `Bool` | `true` / `false` |
| Insert Number | — | ` name={12}` |
| Insert String | — | ` name="…"` when it has no `"` and no newline, else ` name={"…"}` |
| Insert Bool | — | ` name` for true, ` name={false}` for false |

A `const LABEL = 'Sale'` initializer resolves like a numeric const today, with the same `SharedBinding` lock and its own form (`ContainerQuoted`). `check_received` compares `EditValue`s (numbers within the existing tolerance, strings and bools exactly).

- [ ] **Step 1: Write the failing tests** (`tests/values.rs`)

```rust
#[test] fn string_attribute_keeps_its_quotes()      // <Text label="Sale" /> → Literal{String("Sale"), Quoted('"')}; write "New" → label="New"
#[test] fn single_quoted_attribute()                // label='Sale' → write "New" → label='New'
#[test] fn container_string_escapes_its_quote()     // label={'Sale'} → write "It's" → label={'It\'s'}
#[test] fn quoted_attribute_with_its_quote_becomes_a_container() // label="Sale" → write `Say "hi"` → label={"Say \"hi\""}
#[test] fn newline_becomes_a_container()            // label="a" → write "a\nb" → label={"a\nb"}
#[test] fn template_without_substitutions()         // label={`Sale`} → write "x`y" → label={`x\`y`}
#[test] fn template_with_substitutions_is_an_expression() // label={`${n} off`} → Locked(Expression)
#[test] fn bare_boolean_written_false()             // <Video loop /> → Literal{Bool(true), BareTrue}; write false → loop={false}
#[test] fn bool_container()                         // loop={false} → write true → loop={true}
#[test] fn const_string_resolves_and_shares()       // const L='a'; <Text label={L}/> → Literal on 'a'; used twice → SharedBinding
#[test] fn insert_string_and_bool()                 // Missing; insert String("a") → ` label="a"`; Bool(false) → ` loop={false}`
#[test] fn received_string_mismatch_is_overridden() // label="a", received String("b") → OverriddenByParent
```

Update the existing numeric tests in `tests/resolve.rs` and `tests/locate_apply.rs` for the new shapes (`value: EditValue::Number(…)`, `form: LiteralForm::Number`, `default: Some(EditValue::Number(…))`).

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p celesta-source-edit`
Expected: FAIL to compile (`EditValue` not found).

- [ ] **Step 3: Implement** in `resolve.rs` and `apply.rs`; adapt the editor call sites (`EditTarget::Literal { value, .. }` now holds `EditValue`; transform code reads `as_number()`; `to_edit` takes `&EditValue::Number(v)`; `ResolveContext.received` is `prop.received(..).map(EditValue::Number)`). JSX attribute strings (`JSXAttributeValue::StringLiteral`) become `Shape::String(span_with_quotes, value, Quoted(q))`; `Expression::StringLiteral` → `ContainerQuoted(q)`; `TemplateLiteral` with no expressions → `Template`; a bare attribute (`value: None`) → `BareTrue` with an empty span at the end of the attribute name; `BooleanLiteral` → `Bool`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p celesta-source-edit && cargo test -p celesta-editor`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/source-edit crates/editor/src
git commit -m "feat(source-edit): resolve and write string and boolean literals"
```

---

### Task 2: Text children and `style` object properties

**Files:**
- Modify: `crates/source-edit/src/resolve.rs` (prop paths), `crates/source-edit/src/locate.rs` (`ElementRef` gains the children range)
- Test: `crates/source-edit/tests/values.rs`

**Interfaces:**
- Consumes: Task 1's `EditValue`, `LiteralForm`, `Edit`.
- Produces: `resolve(text, element, prop, ctx)` accepts two prop paths besides attribute names:
  - `"children"`: the element's children;
  - `"style.<key>"`: property `<key>` of a `style={{ … }}` object literal.
- `LiteralForm` gains `JsxText` (written as JSXText) and `ChildContainer(char)` (a `{'…'}` / `{"…"}` child).
- `ElementRef` gains `children: Option<Span>` (between the opening and closing tags; `None` when self-closing).

Rules (exact, from the spec):
- children: one JSXText child → `Literal{String(collapsed), JsxText}` where the value is the text as React renders it (JSX whitespace collapsing: lines trimmed, blank lines dropped, joined with single spaces); one `{'…'}`/`{"…"}` child → `ChildContainer(q)`; anything else (several children, `<Span>`, an expression) → `Expression` with the children's source. Self-closing or no children → `Missing { insert_at: <end of opening tag>, default: Some(String("")) }` only when the element is not self-closing; a self-closing element's children are `Locked(Expression)` with `"/>"` for now (2b opens the tag).
- Writing children replaces the whole children span: as JSXText when the value has no `{ } < >`, no leading or trailing whitespace, and no newline; otherwise `{"…"}` (JSON-escaped).
- `style.<key>`: the attribute must be `style={{ … }}` (an object literal, no spread); the key a plain or string-literal key. Its value resolves by the same shape rules (number, string, bool, const, `interpolate`). A `style` that is not an object literal → `Locked(Expression)` with its source; a missing key in an object literal → `Missing { insert_at: <before the closing brace, after a trailing comma if any> }`, inserted as `key: value` with `, ` as needed.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn jsx_text_child()                 // <Text>Sale now</Text> → String("Sale now"), JsxText; write "Big sale" → <Text>Big sale</Text>
#[test] fn multiline_jsx_text_collapses()   // "<Text>\n  Sale\n  now\n</Text>" → String("Sale now")
#[test] fn jsx_text_needing_braces()        // write "a {b}" → <Text>{"a {b}"}</Text>
#[test] fn string_child_container()         // <Text>{'Sale'}</Text> → ChildContainer('\''); write "x" → {'x'}
#[test] fn span_children_are_locked()       // <Text>a<Span>b</Span></Text> → Locked(Expression)
#[test] fn style_number_property()          // style={{ fontSize: 48 }} → "style.fontSize" Literal Number(48)
#[test] fn style_missing_property_inserts() // style={{ fontSize: 48 }} → "style.color" Missing; insert String("#fff") → style={{ fontSize: 48, color: "#fff" }}
#[test] fn style_call_is_locked()           // style={mono(20)} → "style.fontSize" Locked(Expression{"mono(20)"})
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p celesta-source-edit --test values`
Expected: FAIL.

- [ ] **Step 3: Implement** the two prop paths in `Gather` (find the element's children / the `style` attribute's object property instead of an attribute) and the `JsxText`/`ChildContainer` writers in `write_value`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p celesta-source-edit`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/source-edit
git commit -m "feat(source-edit): edit text children and style object properties"
```

---

### Task 3: Pass-through to the call site

**Files:**
- Create: `crates/source-edit/src/pass_through.rs`
- Modify: `crates/source-edit/src/resolve.rs` (parameter bindings; component props), `src/lib.rs`
- Test: `crates/source-edit/tests/pass_through.rs` (new)

**Interfaces:**
- Consumes: Tasks 1–2.
- Produces:

```rust
/// The function that encloses `element` in `text`, as a call site would name it.
pub fn enclosing_component(text: &str, element: &ElementRef) -> Option<String>;
// `function PriceTag(…)`, `const PriceTag = (…) => …`, `const PriceTag = memo((…) => …)`, `forwardRef(…)`; None when anonymous.

pub enum EditTarget {
    /* Task 1 variants */
    /// The prop comes from parameter `param` of the enclosing component;
    /// `target` is the prop at the call site `call_site` (in another text).
    PassThrough { call_site: ElementRef, param: String, target: Box<EditTarget> },
}
pub enum LockReason {
    /* existing */
    /// The value is a parameter of `component` that its caller passes on
    /// again (a second hop).
    ParameterOfCaller { component: String },
    /// No chain entry calls the enclosing component.
    CallSiteNotFound { component: String },
}

/// What `resolve` found when the value is a parameter of the enclosing
/// component: the parameter's name at the call site and its literal default.
pub fn parameter_of(text: &str, element: &ElementRef, prop: &str) -> Option<Parameter>;
pub struct Parameter { pub component: String, pub param: String, pub default: Option<EditValue> }

/// Resolves `prop` on `element` (in `element_text`) and, when it is a
/// parameter of the enclosing component, continues at `call_site` in
/// `call_site_text`.
pub fn resolve_through(
    element_text: &str, element: &ElementRef, prop: &str, ctx: &ResolveContext,
    call_site: Option<(&str, &ElementRef)>,
) -> EditTarget;
```

- `ResolveContext` gains `component_props: bool`: true lets `resolve` read props written on a component element (the call site); `primitive: false` with `component_props: false` stays `NotPrimitive`.
- Rules: the value is an identifier bound by the enclosing function's first parameter when that is an object pattern (`{ label }`, `{ label = 'Sale' }`, `{ label: text }` reading `text`), or `props.label` when the parameter is an identifier named `props`. At the call site, `param` resolves with the call site's own rules: literal → `PassThrough(Literal)`; absent and the parameter has a literal default → `PassThrough(Missing { default })`; absent without default → `PassThrough(Missing { default: None })`; a call site value that is itself a parameter of its enclosing function → `Locked(ParameterOfCaller { component })`; other locks (spread, expression) come back as `PassThrough(Locked(reason))` so the UI can say where. `call_site: None` → `Locked(CallSiteNotFound)`.
- The received value check runs on the call site's literal against the primitive's received value.

- [ ] **Step 1: Write the failing tests** (fixtures as two TSX strings: `component.tsx`, `scene.tsx`)

```rust
#[test] fn enclosing_component_names()        // function / const arrow / memo / forwardRef / anonymous → Some("PriceTag")… / None
#[test] fn literal_at_the_call_site()         // function PriceTag({label}) {<Text label={label}/>} + <PriceTag label="Sale"/> → PassThrough{param:"label", target: Literal String("Sale")}
#[test] fn missing_with_default()             // ({ x = 40 }) + <Tag/> → PassThrough{ target: Missing{default: Some(Number(40))} }
#[test] fn renamed_destructuring()            // ({ label: text }) reads text → param "label"
#[test] fn props_member()                     // (props) reads props.label → param "label"
#[test] fn second_hop_is_locked()             // Card passes {label} to <PriceTag label={label}/> → Locked(ParameterOfCaller{component:"Card"})
#[test] fn spread_at_the_call_site()          // <PriceTag {...rest}/> → PassThrough{ target: Locked(Spread) }
#[test] fn no_call_site()                     // call_site None → Locked(CallSiteNotFound{component:"PriceTag"})
#[test] fn write_through_inserts_at_the_call_site() // to_edit on PassThrough(Missing) inserts ` x={12}` into scene.tsx's <Tag
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p celesta-source-edit --test pass_through`
Expected: FAIL.

- [ ] **Step 3: Implement** `pass_through.rs`. `PassThrough::to_edit` delegates to the inner target (the edit applies to the call site's text; callers know which file).

- [ ] **Step 4: Run the tests**

Run: `cargo test -p celesta-source-edit`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/source-edit
git commit -m "feat(source-edit): follow a prop to the component's call site"
```

---

### Task 4: Inputs of an expression

**Files:**
- Create: `crates/source-edit/src/inputs.rs`
- Modify: `crates/source-edit/src/lib.rs`
- Test: `crates/source-edit/tests/inputs.rs` (new)

**Interfaces:**
- Consumes: Tasks 1, 3.
- Produces:

```rust
/// For a prop locked as `Expression`, each identifier the expression reads
/// (in order of first appearance, member objects only: `C.x` gives `C`),
/// resolved by the same rules as a prop value. One level: an input that is
/// itself an expression stays `Locked(Expression)`.
pub fn resolve_inputs(text: &str, element: &ElementRef, prop: &str, ctx: &ResolveContext) -> Vec<(String, EditTarget)>;
```

An identifier that is a parameter of the enclosing component comes back as `Locked(CallSiteNotFound { component })` (no call site is given here); the editor re-resolves such inputs with `resolve_through` and the call site when 2b shows them. In 2a nothing displays inputs yet. `ctx.received` is ignored for inputs (the runtime reports the result, not the inputs).

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn literal_inputs()          // const padding = 16; width={textWidth + padding * 2} → [("textWidth", Locked(UnresolvedIdentifier)), ("padding", Literal Number(16))]
#[test] fn member_object_only()      // x={C.x + 4} → [("C", Locked(UnresolvedIdentifier))]
#[test] fn one_level()               // const a = b + 1; x={a * 2} → [("a", Locked(UnresolvedIdentifier))] (a const that is an expression is not expanded)
#[test] fn not_an_expression()       // x={12} → []
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p celesta-source-edit --test inputs`
Expected: FAIL.

- [ ] **Step 3: Implement** with an `oxc_ast_visit::Visit` over the attribute's expression collecting `IdentifierReference`s (skipping property names of static member expressions and call callees), then resolving each name through the existing identifier rule.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p celesta-source-edit`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/source-edit
git commit -m "feat(source-edit): list an expression's editable inputs"
```

---

### Task 5: Report every scalar prop the primitive received

**Files:**
- Modify: `packages/react/src/source-chain.ts` (`ReceivedProps`, `layerSourceReport`)
- Modify: `crates/react-bridge/src/types.rs` (`ReceivedProps`)
- Test: `packages/react/test/layer-source.test.mts`, `crates/react-bridge/src/tests.rs`

**Interfaces:**
- Produces:
  - TS: `ReceivedProps` becomes `Record<string, number | string | boolean>`: every own prop of the innermost primitive whose value is a finite number, a string, or a boolean, plus `children` when it is a string. Functions, objects, arrays, `undefined`, and `null` are left out.
  - Rust:

```rust
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum ReceivedValue { Number(f64), String(String), Bool(bool) }
#[derive(Clone, Debug, Default, PartialEq, Deserialize)] // no longer `Copy`
pub struct ReceivedProps {
    pub x: Option<f64>, pub y: Option<f64>, pub rotation: Option<f64>,
    pub scale: Option<f64>, pub scale_x: Option<f64>, pub scale_y: Option<f64>,
    /// Every other scalar prop, keyed as written (`fontSize`, `children`).
    #[serde(flatten)] pub others: BTreeMap<String, ReceivedValue>,
}
impl ReceivedProps { pub fn get(&self, prop: &str) -> Option<ReceivedValue>; } // reads the named fields for the transform props, `others` for the rest
```

- [ ] **Step 1: Write the failing tests**

```ts
test('props carry every scalar prop the primitive received', () => {
  // <Text color="#fff" fontSize={48} wrap>{'Sale'}</Text> → props includes { color: '#fff', fontSize: 48, wrap: true, children: 'Sale' }; no function/object keys
});
```

```rust
#[test] fn received_props_read_scalars() // JSON {"x":1,"color":"#fff","wrap":true,"children":"Sale"} → x Some(1.0); get("color") == Some(String("#fff")); get("wrap") == Some(Bool(true)); get("x") == Some(Number(1.0))
```

- [ ] **Step 2: Run them to verify they fail**

Run: `pnpm --filter @celesta/react exec vitest run test/layer-source.test.mts` and `cargo test -p celesta-react-bridge received_props`
Expected: FAIL.

- [ ] **Step 3: Implement** both sides. `ReceivedProps` loses `Copy` (it now holds a map): `crates/editor/src/transform_drag.rs:153` takes `report.props.clone()`, and other by-value uses follow the compiler.

- [ ] **Step 4: Run the tests**

Run: `pnpm --filter @celesta/react test && cargo test -p celesta-react-bridge && cargo test -p celesta-editor`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add packages/react crates/react-bridge
git commit -m "feat(react): report every scalar prop a primitive received"
```

---

### Task 6: Multi-file writes and history entries

**Files:**
- Modify: `crates/editor/src/edit_history.rs` (`HistoryEntry`, `EditHistory`, `Pending`, `write_recorded`, `record_own_write`)
- Modify: `transform_edit.rs` (the one `record_own_write` caller, still one file in this task) and `property_panel.rs:768-798` (`write_recorded` + `edit_history.push(edit)`: push a one-file `HistoryEntry`)
- Test: `crates/editor/src/edit_history.rs` (`#[cfg(test)]`)

**Interfaces:**
- Produces:

```rust
/// One undo step: every file one edit wrote.
pub(crate) struct HistoryEntry { pub(crate) files: Vec<FileEdit> }
impl From<FileEdit> for HistoryEntry { /* one file */ }
// EditHistory::push takes `HistoryEntry`; `Pending` holds a `HistoryEntry`.

/// The texts of a set of files, keyed by path (a missing file reads `""`).
pub(crate) type Texts = BTreeMap<PathBuf, String>;

/// Reads every path in `paths`, lets `edit` compute the new texts from all
/// of them at once (so it can check the set together), writes every changed
/// file to a temp file first and renames them only when all temp writes
/// succeeded. Paths `edit` leaves out, or returns unchanged, are not written.
pub(crate) fn write_all_recorded(
    paths: Vec<PathBuf>,
    edit: impl FnOnce(&Texts) -> Result<Vec<(PathBuf, String)>, String>,
) -> Result<Vec<FileEdit>, String>;

impl EditorView {
    /// `record_own_write` for several files; one history entry.
    pub(crate) fn record_own_writes(
        &mut self,
        paths: Vec<PathBuf>,
        edit: impl FnOnce(&Texts) -> Result<Vec<(PathBuf, String)>, String> + Send + 'static,
        done: impl FnOnce(&mut Self, Result<(), String>, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    );
}
```

`record_own_write(path, edit, …)` becomes `record_own_writes(vec![path], |texts| Ok(vec![(path, edit(&texts[&path])?)]), …)`. `Pending::apply` checks every file against `after`/`before` first and refuses the whole entry (`ChangedOutside` naming the first changed file) if any differs; then writes all files through the same temp-then-rename routine. `settle` on a refusal drops the entry and every older entry that touches any of its files. Watched mtimes are sampled once after all files are written.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn a_two_file_entry_undoes_both()           // push entry {a: x→y, b: p→q}; undo → a == "x", b == "p"; redo → y, q
#[test] fn an_outside_change_refuses_the_whole_entry() // b changed to "r" outside; undo → Err(ChangedOutside); a still "y", b "r"; entry dropped
#[test] fn a_failed_temp_write_renames_nothing()    // write_all_recorded([a (writable), b in a read-only dir], edit both) → Err; a unchanged
#[test] fn unchanged_files_are_not_written()        // edit returns the same text for b → FileEdit list has only a
#[test] fn edit_sees_every_text()                   // edit gets {a: "x", b: "p"} and may refuse with Err("…") → nothing written
#[test] fn the_cap_counts_entries_not_files()       // push 101 two-file entries → 100 kept
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p celesta-editor edit_history`
Expected: FAIL.

- [ ] **Step 3: Implement.** Temp names follow `replace_file`'s (`.{name}.tmp-{pid}`); on any failure remove the temps already written.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p celesta-editor`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/editor/src
git commit -m "feat(editor): write and undo one edit across several files"
```

---

### Task 7: Transform targets through the call site

**Files:**
- Modify: `crates/editor/src/transform_edit.rs` (`TransformTargets`, `resolve_targets`, `commit_text` → `commit_files`, `sync_transform_targets`, `commit_transform`)
- Modify: `crates/editor/src/layer_select.rs` (owner call site)
- Test: `crates/editor/src/tests.rs`

**Interfaces:**
- Consumes: Task 3 (`resolve_through`, `enclosing_component`, `PassThrough`), Task 6 (`record_own_writes`).
- Produces:

```rust
/// The chain entry that calls the component enclosing the selected element:
/// the nearest `Component` entry above it whose `name` equals `component`.
pub(crate) fn owner_call_site<'a>(report: &'a LayerSourceReport, component: &str) -> Option<&'a LayerSource>;

pub(crate) struct Target { pub(crate) file: PathBuf, pub(crate) target: EditTarget }
pub(crate) struct TransformTargets {
    pub(crate) file: PathBuf,                 // the element's file
    pub(crate) element: ElementRef,
    pub(crate) targets: Vec<(TransformProp, Target)>,
}
/// Reads the element's file and, when a target passes through, the call
/// site's file; resolves every transform prop.
pub(crate) fn resolve_targets(read: impl Fn(&Path) -> Result<String, String>, report: &LayerSourceReport) -> Result<TransformTargets, String>;
/// The new text of every file the values touch, after re-resolving against
/// `texts`; `SourceChanged` if any target moved.
pub(crate) fn commit_files(texts: &Texts, expected: &TransformTargets, report: &LayerSourceReport, values: &[(TransformProp, f64)]) -> Result<Vec<(PathBuf, String)>, CommitError>;
```

- A target resolves at the call site only when `parameter_of` (Task 3) says the element's value is a parameter of the enclosing component; only then is the call site's file read.
- The call site's `ElementRef` comes from `element_at(call_site_text, entry.line, entry.column)` for the `owner_call_site` entry.
- `ResolveContext.received` comes from `report.props.get(prop)` converted to `EditValue` (`ReceivedValue::Number(n)` → `EditValue::Number(n)`, and so on).
- `commit_transform` calls `record_own_writes(paths, move |texts| commit_files(texts, &targets, &report, &values).map_err(|e| e.to_string()), …)` with the element's file and, when any target passes through, the call site's file.
- `CommitError::SourceChanged`'s text becomes "The source changed while editing".

- [ ] **Step 1: Write the failing tests** (two fixture files in a temp dir)

```rust
#[test] fn owner_call_site_skips_wrappers_and_children_hosts() // chain [Rect, Sequence(wrapper), Later(component, element passed as children), PriceTag(component)] with component "PriceTag" → the PriceTag entry
#[test] fn x_passed_at_the_call_site_is_a_pass_through_target() // PriceTag({x}) <Rect x={x}/> + <PriceTag x={40}/> → X target file == scene.tsx, PassThrough(Literal 40)
#[test] fn a_drag_writes_two_files()       // x pass-through, rotation local → commit_files gives [(component.tsx, rotation written), (scene.tsx, x written)]
#[test] fn a_changed_call_site_aborts()    // scene.tsx edited so <PriceTag moved → Err(SourceChanged); nothing written
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p celesta-editor -- owner_call_site pass_through two_files changed_call_site`
Expected: FAIL.

- [ ] **Step 3: Implement.** Keep `TransformTargets::target(prop)` returning `&EditTarget` (the inner target for display), and add `TransformTargets::file_of(prop) -> &Path`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p celesta-editor`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/editor/src
git commit -m "feat(editor): drag transform props set at a component's call site"
```

---

### Task 8: Show pass-through rows and their impact; docs

**Files:**
- Modify: `crates/editor/src/transform_rows.rs` (row state), `crates/editor/src/transform_edit.rs` (`reason_text` for the new reasons), `crates/editor/src/impact.rs` (scope by call site), `crates/editor/src/icons.rs` (`CornerDownRight`)
- Modify: `packages/website/content/{en,ja}/docs/01-(getting-started)/03-preview.md`
- Create: `docs/handoff/2026-10-23-property-editing-engine.md`
- Test: `crates/editor/src/tests.rs`, `crates/editor/src/impact.rs`

**Interfaces:**
- Consumes: Task 7.
- Produces:
  - Row for a `PassThrough` target: the inner target's state and field, a `CornerDownRight` icon, and the detail line `Set at <file>:<line>` (file relative to the entry, as `source_position` formats it).
  - Reason texts: `ParameterOfCaller` → "Comes from `<Card>`'s caller"; `CallSiteNotFound` → "Can't find where `<PriceTag>` is called".
  - `pub(crate) fn impact_view(report, occurrences, chapters, frame, written_at: Option<&SourcePos>) -> ImpactView`: when every editable transform target passes through to the same call site, `written_at` is that call site, and the instances counted are those whose chain contains it. `selection_impact` in `layer_panel.rs` passes it; every other caller (tests, the timeline) passes `None` unless it reads `selection_impact`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn pass_through_reason_texts()   // reason_text(ParameterOfCaller{component:"Card"}) == "Comes from `<Card>`'s caller"; CallSiteNotFound{"PriceTag"} == "Can't find where `<PriceTag>` is called"
#[test] fn impact_scoped_to_a_call_site() // two instances (calls in Intro and Price), written_at = the Price call → other_scenes == [] and line "Changes this scene only"
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p celesta-editor -- pass_through_reason impact_scoped`
Expected: FAIL.

- [ ] **Step 3: Implement**, then the docs: the preview page explains rows marked "Set at …" and that dragging them writes the call site; the handoff note lists what 2a added and what 2b–2d will build on.

- [ ] **Step 4: Run all tests**

Run: `cargo test -p celesta-source-edit -p celesta-react-bridge -p celesta-editor && pnpm --filter @celesta/react test`
Expected: PASS.

- [ ] **Step 5: Check it in the app (the user runs this)**

In a scene that calls a component with `x={40}`, select a layer inside it: X shows "Set at scene.tsx:…" with the arrow icon; dragging writes the call site; undo restores both files after a drag that also rotated.

- [ ] **Step 6: Commit**

```bash
git add crates/editor/src packages/website/content docs/handoff
git commit -m "feat(editor): show where pass-through transform props are set"
```
