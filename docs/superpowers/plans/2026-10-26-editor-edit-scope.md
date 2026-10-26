# Edit Scope (Design 3) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When the selected element is shared, the user chooses "This scene" or "Shared value" before committing. "This scene" writes at the scene's call of the component (lifting the literal into a new prop of the component when needed); "Shared value" writes the value every scene uses unless it sets its own. Every edit kind (Properties rows, scrubs, removal, canvas drags, inline text) honors the choice.

**Architecture:** `crates/source-edit` gains a verbatim `Edit::Splice`, the lift planner (`plan_lift`: a literal or absent prop at the element becomes a new parameter of its component, with the props type extended) and `parameter_default` (where a parameter's default is written, so "Shared value" can write it). `crates/editor` gains a `scope` module of pure rules: the per-element choice and its memory, the scope state computed from the occurrence index and the scene calls' props (confinement and locks), and `scoped_row`, the one per-row rule that both the rows (shown value, locks, removal) and the commit read. The commit path becomes `commit_scoped`, generic over the element it writes (its targets and a re-resolve step), so the selected element's `commit_files` and design 4's timing windows share it; it stays inside `record_own_writes`, the single writer.

**Tech Stack:** Rust (`celesta-source-edit` on oxc 0.154, `celesta-editor` with GPUI via gpui-kit 0.6, `celesta-react-bridge` occurrence index), docs in the website package.

**Spec:** `docs/superpowers/specs/2026-10-23-editor-edit-scope-design.md`. Builds on design 1 (occurrence index, impact line, timeline bands) and design 2 (pass-through and the owner call site rule, `EditValue`, multi-file history, `commit_element`).

## Global Constraints

- One edit is one history entry, through `commit_element` → `record_own_writes` → `commit_scoped` (re-resolve, "The source changed while editing", `apply_checked`). A lift that touches two files is still one entry. Nothing else writes the source.
- Default scope is **This scene**; the choice is remembered per element (its `SourcePos`) for the session. An element that is not shared (one instance, or not written inside a called component) has no control: design 1's impact line stays exactly as `impact_line` writes it ("Changes this scene only", or "Also changes …" when its one instance spans scenes).
- "This scene" requires `S` (the scene's call of `C`, the component enclosing `E`, by the owner call site rule) to be confined: every occurrence of every instance whose chain contains `S` lies in this scene, and every instance overlapping this scene goes through `S` ("This scene" means every instance in the current scene).
- The new prop's name: the element's tag in lower camel case plus the prop's name with a capital (`rectWidth`, `textFontSize`, `rectX`, `textChildren`); taken names get 2, 3, … appended (`rectWidth2`). Taken = every name declared or read anywhere in `C`'s file, the pattern's keys, and the props type's members.
- The default is the current literal in the same form it had (table in Task 1); the literal at `E` becomes the identifier. An absent prop is lifted with its known default (`Missing.default`, else the table's declared default) or none.
- Props type: add `name?: T` (T from the value: `number`, `string`, `boolean`) to an inline type literal, a same-file `interface`, or a same-file `type` alias that is (or intersects) a type literal; also when the type comes from the declarator (`FC<P>`, `React.FC<P>`, `FunctionComponent<P>`) or a `memo<P>`/`forwardRef<R, P>` type argument. Imported → locked; anything else → locked; no annotation anywhere → nothing added.
- Lock texts are verbatim from the spec's Locks table; texts the spec doesn't give are the Rulings' and are added to the spec in Task 5.
- UI copy English, inspector style (radius 0, no gradients, no uppercase labels, lucide icons via `CelestaIcon`/`IconName`); the scope control uses the 2c tabs look (shadcn Tabs: inset pill with a gap) with labels saying what is chosen; nothing is drawn over the canvas.
- Tests call pure functions only (no GPUI test context): view paths name the pure helper that carries their rule, and multi-file writes are tested with `write_all_recorded` and `EditHistory` on temp files as `tests/element_edit.rs` does.
- TDD per task; red/green runs only the new tests (`cargo test -p <crate> -- <names>`); each crate's suite once per task; `cargo clippy -p celesta-source-edit -p celesta-editor --all-targets` clean (the pre-existing `large_enum_variant` on `SourceLookup` excepted); `cargo fmt --check`.
- No `git commit --amend`; commit and test commands are separate calls; stage paths explicitly.

## Rulings (decided here; the spec leaves them open)

- **One per-row rule.** `scoped_row(scope, targets, key, calls)` decides what a row writes under a scope (table in Task 2); the row's shown value, its locks, its remove button and the commit all read it, so what the panel shows is what gets written.
- **Scope applies at commit time; the confinement is the caller's.** `commit_element` asks `commit_gate` (state from the index, the remembered choice) and passes the scope to `commit_scoped`, which enforces only what the text tells (`scoped_row`'s locks, the lift's locks). A remembered "This scene" that a new lock now disables is refused with the lock's text, never widened; the control then shows neither option chosen until the user picks one.
- **When the index can't answer** (scanning, failed, or a `Refreshing` index that no longer finds `E` because a lift moved it) and `E` is inside a called component: a value written at `S` as resolved (a pass-through or Component-section row, unless "Shared value" is remembered) goes as today; any other row commit is held (`HeldCommits`) with the note "Scanning scenes…" and drained when the sweep lands; a drag is refused with that note; a failed scan refuses with "Couldn't scan scenes: …". A spread to other scenes is never written without asking.
- **Scene calls are read in the background.** The occurrence chains are bare positions, so a job beside `sync_element_targets` (keyed by `E` and the index generation) reads each instance's call of `C` — the chain entry at the index `S` has in the selected instance's chain — and the attributes it writes (`written_attributes`). From these: "· Price sets its own", "Every scene sets `rectWidth` itself", and the "Shared value" band filter.
- **"Sets its own" is per element on the line, per row on the row.** A scene sets its own when its call writes any parameter `E`'s rows read; the row whose parameter this scene passes is the precise one: under "Shared value" it shows and edits the default, and drag, scrub and inline text lock it ("This scene sets its own `rectWidth`"), since the change would not show in this scene.
- **"Shared value" on a parameter.** It writes the default's own span (`parameter_default`); a parameter without a default that some scene leaves out gets one (`{ rectWidth }` → `{ rectWidth = 240 }`); without a default and passed at every call, the row locks "Every scene sets `rectWidth` itself", and the option is disabled when that holds for every editable row; a default written as an expression locks "`rectWidth`'s shared value is written as an expression"; `props.rectWidth` (no pattern to hold a default) locks with the not-an-object-pattern text.
- **Removal follows the scope.** "This scene": the remove button of a row passed at `S` deletes it there; it is "Use the shared value" (the existing button, tooltip "Use the shared value"; no second button). A literal at `E` can't be removed under "This scene" (that would reach every scene): no button, an emptied field restores. "Shared value": a literal at `E` is removed as today; a default is never removed. Component-section rows are outside the scope: written and removed at `S` as today.
- **Inputs.** A pass-through input follows the row rules; a literal input (a `const` an expression reads) locks "This scene" with "Can't be set per scene yet".
- **Which lock "This scene" shows**, in this order: no call found ("Can't find where `<PriceTag>` is called", design 2's text); a component entry after `S` in the report chain ("Passes through `<Card>`": a `<Scenes>` scene function adds no entry, so a component there sits between the scene and `C`; scenes written as hand-placed components count too, a known gap); one chain through `S` overlapping another scene with a `Scenes` wrapper after `S` (the reused-call text); any other instance through `S` in another scene (the other-scenes text); an instance in this scene not through `S` ("`<PriceTag>` is called more than once in this scene"); then text-level locks when no row can be written under it (the first row's reason).
- **Props types the lift can't extend** lock with "Can't add `rectWidth` to `PriceTag`'s props type" (an alias without a type-literal member, a mapped type, another declarator annotation).
- **The memory survives a lift.** On a kept reload the view remembers the selected element's old `SourcePos`; when the first lookup of the reloaded code answers for the kept layer, `ScopeChoices::carry(old, new)` moves the choice to the new chain head (an interface field above `E` shifts it).
- **Held commits keep their scope**: `HeldCommit` stores the scope it was made with; draining commits with it.
- **Edits change, in effect.** Without hover the impact (bands, header) is filtered by the chosen scope (`ImpactFilter`); hovering an option previews that option's filter. While a drag runs the Properties header and a second line of the drag label say "Edits change this scene" / "Edits change the shared value".
- **The lift is planned per commit** against the texts being written: `plan_lift` takes every row the commit lifts at once (a move lifts `x` and `y` with names chosen together); the call-site attribute is `Edit::Insert` at `S`.

## Review Focus

- **A component used by two `defineScenes` entries through one `<Scenes>` call** (one chain spanning both scenes): "This scene" disabled with "The scenes reuse the same call (`.map()` or `defineScenes`)", the choice falls to "Shared value". Test in Task 2 (`one_chain_spanning_scenes_locks_as_reused`).
- **A lift whose obvious name is already used** — a body `const rectWidth`, an imported `rectWidth`, `E` inside `.map((rectWidth) => …)`, a pattern key `{ rectWidth: w }`: the parameter is `rectWidth2` and the result parses. Test in Task 1 (`taken_names_include_every_binding_and_read_in_the_file`).
- **An edit right after a lift that moved `E`** (stale `Refreshing` index misses it): a pass-through row writes at `S`, a literal row is held with "Scanning scenes…" and written when the sweep lands; nothing is written unscoped. Test in Task 3 (`commit_gate_holds_until_the_index_answers`).
- **"Shared value" while this scene sets its own**: the row shows and writes the default, Price's attribute stays, drag/scrub/inline lock with "This scene sets its own `rectWidth`". Tests in Task 2 (`scoped_row_follows_the_table`) and Task 3 (`shared_value_on_a_lifted_prop_writes_the_default_and_keeps_call_site_attributes`).
- **A move with "This scene" on `<Rect width={200} />` (no `x`/`y`) in `({ label, ...rest })`**: one entry inserts `rectX = 0, rectY = 0` before `...rest`, `x={rectX} y={rectY}` at `E`, and both attributes at `S`; undo restores both files. Test in Task 3 (`a_move_lifts_x_and_y_in_one_entry`).

---

### Task 1: Splice, parameter defaults and the lift planner (`crates/source-edit`)

**Files:**
- Create: `crates/source-edit/src/lift.rs`
- Modify: `crates/source-edit/src/{apply.rs,scope.rs,pass_through.rs,lib.rs}`
- Test: `crates/source-edit/tests/{splice,lift}.rs`

**Interfaces:**
- Produces (`ParamDefault` stays `pub(crate)` in the private `scope` module; `DefaultTarget` is its public view):

```rust
// apply.rs — verbatim text, for identifiers, pattern entries, type fields, JSX
pub enum Edit { /* existing */ Splice { start: u32, end: u32, text: String } }
// scope.rs
pub(crate) enum ParamDefault { None { binding_end: u32 }, Literal { value: EditValue, span: Span, form: LiteralForm }, Other, NoPattern }
// pass_through.rs
pub enum DefaultTarget { Literal { span: Span, value: EditValue, form: LiteralForm }, Absent { insert_at: u32 }, Expression, NoPattern }
/// Where the default of parameter `param` of the component enclosing `element` is written.
pub fn parameter_default(text: &str, element: &ElementRef, param: &str) -> Option<DefaultTarget>;
// lift.rs
pub struct LiftProp<'a> { pub prop: &'a str, pub target: &'a EditTarget /* Literal or Missing at the element */, pub value: &'a EditValue /* types the field without a default */ }
pub struct Lift { pub params: Vec<String> /* in `props` order */, pub edits: Vec<Edit> /* the component's file */ }
pub enum LiftLock { NoComponent, NotObjectPattern { component: String }, ImportedPropsType { component: String }, PropsTypeNotEditable { component: String, param: String }, NotALiteral }
pub fn lift_name(tag: &str, prop: &str, taken: &[String]) -> String;
pub fn plan_lift(text: &str, element: &ElementRef, props: &[LiftProp<'_>]) -> Result<Lift, LiftLock>;
```

What the lift writes, by the target's `LiteralForm` (the default keeps the literal's style as an expression):

| At `E` | `E` becomes | Default |
| --- | --- | --- |
| `Quoted(q)` `width="200"` | `width={rectWidth}` | `"200"` in `q` |
| `Number`, `Bool`, `ContainerQuoted`, `Template`, a `const` initializer | the bare identifier in the span | the literal as written |
| `BareTrue` `loop` | `loop={rectLoop}` (at the empty span) | `true` |
| `JsxText` / `ChildContainer(q)` | `{textChildren}` between the tags | `"Sale"` / in `q` |
| `Missing` | ` x={rectX}` at `insert_at` (`{textChildren}` for empty children) | known default, else none |

- [ ] **Step 1: Write the failing Splice test** — `splice_replaces_verbatim` (bytes `start..end` become `text`; a Splice overlapping a Replace is `Overlap`).
- [ ] **Step 2: Run it, expect FAIL** — `cargo test -p celesta-source-edit -- splice`; implement; run, expect PASS; `cargo test -p celesta-source-edit`.
- [ ] **Step 3: Commit** — `feat(source-edit): add a verbatim splice edit` (lands first; design 4 builds on it).
- [ ] **Step 4: Write the failing lift tests** (`tests/lift.rs`, TSX fixtures, asserting the edited text after `apply`, which must parse):
  - `lift_name_camel_cases_the_tag_and_appends_on_a_clash` — `Rect`+`width` → `rectWidth`, `Text`+`fontSize` → `textFontSize`, `Rect`+`x` → `rectX`; taken `rectWidth` → `rectWidth2`, taken both → `rectWidth3`.
  - `lifts_into_an_object_pattern` — the spec's `PriceTag` exactly: `function PriceTag({ label }: Props)` → `function PriceTag({ label, rectWidth = 200 }: Props)`, `<Rect width={200} … />` → `<Rect width={rectWidth} … />`, and the local `interface Props` gains `rectWidth?: number;`.
  - `lifts_with_no_parameter` — `function C() {` → `function C({ rectWidth = 200 }) {`; `const C = () =>` → `const C = ({ rectWidth = 200 }) =>`.
  - `inserts_before_a_rest_element` — `{ label, ...rest }` → `{ label, rectWidth = 200, ...rest }`.
  - `lifts_into_an_inline_type_literal`, `lifts_into_a_local_interface`, `lifts_into_a_local_type_alias` — the field matches the members' separator (`;` or `,`).
  - `lifts_into_an_intersection_alias_s_type_literal` (`type Props = { label: string } & Base`); `an_alias_without_a_type_literal_locks` (`type Props = Base & Other` → `PropsTypeNotEditable`).
  - `reads_the_props_type_from_the_declarator_or_wrapper` — `const C: FC<Props> = ({ label }) =>`, `React.FC<Props>`, `memo<Props>(…)`, `forwardRef<HTMLDivElement, Props>(…)` each extend the local `Props`; `const C: Other = …` → `PropsTypeNotEditable`.
  - `an_imported_props_type_locks` (`ImportedPropsType { component: "PriceTag" }`); `a_non_object_parameter_locks` (`(props: Props)`, `(...args)` → `NotObjectPattern`).
  - `writes_the_identifier_by_literal_form` — every row of the table above, `E`'s text and the default each asserted.
  - `an_absent_prop_is_inserted_at_the_element` — `Missing { default: Some(0) }` for `x` → `x={rectX}` and `rectX = 0`; `Missing { default: None }` for `fill` with value `"#f00"` → `{ label, rectFill }`, field `rectFill?: string;`.
  - `taken_names_include_every_binding_and_read_in_the_file` — a body `const rectWidth`, `import { rectWidth } from './sizes'`, `E` inside `.map((rectWidth) => …)`, a key `{ rectWidth: w }`, a type member `rectWidth` → `rectWidth2` each time.
  - `a_move_lifts_x_and_y_together` — `params == ["rectX", "rectY"]`, pattern `{ label, rectX = 10, rectY = 20 }`.
  - `a_keyframe_is_not_a_literal` — a `Keyframe` target → `NotALiteral`.
  - `parameter_default_finds_the_default` — `{ rectWidth = 200 }` → `Literal` at `200` (`Number`); `{ rectWidth: w = 200 }` (by key) → `Literal`; `{ rectWidth }` → `Absent` right after the binding; `{ rectWidth = W * 2 }` → `Expression`; `(props)` → `NoPattern`.
- [ ] **Step 5: Run them, expect FAIL** — `cargo test -p celesta-source-edit -- lift parameter_default taken_names writes_the_identifier an_absent_prop inserts_before reads_the_props_type an_imported an_alias a_non_object a_keyframe`
- [ ] **Step 6: Implement.** `plan_lift` reads the tag at `element` (last `.` segment), finds the enclosing component through `component_function` (`memo`/`forwardRef` too), its first formal parameter, and its type: the parameter's annotation, else the declarator's `FC`-family type argument, else the wrapper call's type argument; a `TSTypeReference` resolves against same-file `TSInterfaceDeclaration`/`TSTypeAliasDeclaration`, an import binding locks. Taken names come from `Scopes.declarations` and `Scopes.refs` (the whole file) plus pattern keys and type members. All edits are `Splice`s except where the table keeps a form (`write_value`).
- [ ] **Step 7: Run them, expect PASS**; then `cargo test -p celesta-source-edit`.
- [ ] **Step 8: Commit** — `feat(source-edit): plan lifting a literal into a component prop`

### Task 2: Scope rules (`crates/editor`, pure)

**Files:**
- Create: `crates/editor/src/scope.rs`
- Modify: `crates/editor/src/element_edit.rs` (`ElementTargets` fields), `crates/editor/src/element_edit/resolve.rs` (fill them in `resolve_rows`), `crates/editor/src/main.rs`
- Test: `crates/editor/src/tests/scope.rs` (declared in `tests.rs`)

**Interfaces:**
- Consumes: Task 1 (`parameter_default`, `DefaultTarget`, `plan_lift`, `LiftLock`); `OccurrenceIndex::instance_for`, `scenes_of`, `outermost_chapter_at`; `OccurrenceState`.
- Produces:

```rust
// element_edit.rs — filled by resolve_rows, so a commit's re-resolve fills them from the texts it writes
pub(crate) struct ElementTargets { /* existing */ pub(crate) enclosing: Option<String> /* C */, pub(crate) defaults: Vec<(String, DefaultTarget)> /* per parameter a row reads */, pub(crate) lift_lock: Option<LiftLock> /* dry run over the rows a lift would take */ }
// scope.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Scope { ThisScene, Shared }
pub(crate) enum ScopeLock { NoCall { component: String }, PassesThrough { component: String }, ReusedCall, OtherScenes { component: String }, CalledTwiceHere { component: String }, NotObjectPattern { component: String }, ImportedPropsType { component: String }, PropsTypeNotEditable { component: String, param: String }, PerSceneUnsupported, EverySceneSets { param: String }, SetsOwn { param: String }, DefaultIsExpression { param: String }, Unknown { note: String } }
impl ScopeLock { pub(crate) fn text(&self) -> String; }
/// Each instance's call of C (instance order) and the props it writes.
pub(crate) struct SceneCalls { pub(crate) element: SourcePos, pub(crate) generation: u64, pub(crate) calls: Vec<Option<(SourcePos, Vec<String>)>> }
pub(crate) fn calls_of(report: &LayerSourceReport, element: &ElementOccurrences, call: &SourcePos) -> Vec<Option<SourcePos>>;
pub(crate) struct ScopeOptions { pub(crate) this_scene: Result<(), ScopeLock>, pub(crate) shared: Result<(), ScopeLock>, pub(crate) shared_reaches: Vec<String>, pub(crate) sets_own: Vec<String> }
pub(crate) enum ScopeState { NotShared, Unknown { note: String }, Options(ScopeOptions) }
pub(crate) fn scope_state(report: &LayerSourceReport, targets: &ElementTargets, occurrences: Option<&OccurrenceState>, calls: Option<&SceneCalls>, chapters: &[ReactChapter], frame: u64) -> ScopeState;
pub(crate) fn chosen_scope(remembered: Option<Scope>, options: &ScopeOptions) -> Result<Scope, ScopeLock>;
pub(crate) fn scope_line(options: &ScopeOptions, chosen: Option<Scope>) -> String;
pub(crate) enum ScopedRow { AsResolved, Lift, Default { shown: Option<EditValue>, own: bool }, Locked(ScopeLock) }
pub(crate) fn scoped_row(scope: Scope, targets: &ElementTargets, key: &PropKey, calls: Option<&SceneCalls>) -> ScopedRow;
/// `removable` under `scope`: only an `AsResolved` row.
pub(crate) fn scoped_removable(scope: Scope, targets: &ElementTargets, key: &PropKey, calls: Option<&SceneCalls>) -> bool;
#[derive(Default)]
pub(crate) struct ScopeChoices { /* HashMap<SourcePos, Scope> */ }
impl ScopeChoices { pub(crate) fn get(&self, element: &SourcePos) -> Option<Scope>; pub(crate) fn set(&mut self, element: SourcePos, scope: Scope); pub(crate) fn carry(&mut self, from: &SourcePos, to: &SourcePos); }
```

`scoped_row` (`E` = a target in `targets.file` that is not a pass-through):

| Row's target | This scene | Shared value |
| --- | --- | --- |
| Literal or `Missing` at `E`, attribute or children | `Lift`, or `Locked` by `lift_lock` | `AsResolved` |
| Keyframe; a path under `style.`; a literal input | `Locked(PerSceneUnsupported)` | `AsResolved` |
| Pass-through (row or input), `S` passes it | `AsResolved` | `Default { own: true }`; drag/scrub/inline lock `SetsOwn` |
| Pass-through, `S` leaves it out | `AsResolved` | `Default { own: false }` |
| Pass-through, no default, every call passes it | `AsResolved` | `Locked(EverySceneSets)` |
| Pass-through, default an expression / `props.x` | `AsResolved` | `Locked(DefaultIsExpression)` / `Locked(NotObjectPattern)` |
| Component-section row | `AsResolved` | `AsResolved` |
| Locked target | as today | as today |

- [ ] **Step 1: Write the failing tests** (`tests/scope.rs`; reports, indexes and chapters built by hand as in `impact.rs`'s tests; `SceneCalls` by hand):
  - `an_element_with_one_instance_is_not_shared`, `an_element_outside_a_component_is_not_shared` (`NotShared`; `impact_line` unchanged).
  - `an_unanswered_index_is_unknown` — `Scanning` → note "Scanning scenes…"; `Failed("x")` → "Couldn't scan scenes: x"; a `Refreshing` index that misses `E` → "Scanning scenes…".
  - `this_scene_is_confined_when_every_instance_through_s_is_in_this_scene`.
  - `no_call_locks_this_scene` ("Can't find where `<PriceTag>` is called"); `a_component_after_s_locks_as_passes_through` ("Passes through `<Card>`"); `one_chain_spanning_scenes_locks_as_reused` ("The scenes reuse the same call (`.map()` or `defineScenes`)"); `s_reaching_another_scene_locks_with_its_component` ("`<PriceTag>` is called here for other scenes too"); `two_calls_in_this_scene_lock` ("`<PriceTag>` is called more than once in this scene").
  - `the_default_is_this_scene_and_falls_back_to_shared_when_locked`; `a_remembered_choice_now_locked_is_refused_not_widened` (`Err` with the lock).
  - `the_line_names_reaching_and_own_scenes` — "Shared value is used in Intro, Outro · Price sets its own"; with the option locked the line is its reason.
  - `shared_is_disabled_when_every_row_is_set_by_every_scene` ("Every scene sets `rectWidth` itself").
  - `scoped_row_follows_the_table` — one case per table row; `lift_lock = ImportedPropsType` → "`PriceTag`'s props type is in another file"; `SetsOwn` text "This scene sets its own `rectWidth`".
  - `removal_follows_the_scope` — "This scene": the row passed at `S` removable, a literal at `E` not; "Shared value": a literal at `E` removable, a `Default` row not.
  - `a_choice_moves_with_the_element_after_a_kept_reload` (`carry`; carrying from an unknown position changes nothing).
  - `calls_of_takes_each_instance_s_entry_at_s_s_index` (an instance whose chain is shorter → `None`).
- [ ] **Step 2: Run them, expect FAIL** — `cargo test -p celesta-editor -- tests::scope::`
- [ ] **Step 3: Implement.** `resolve_rows` sets `enclosing` from `caller`, `defaults` with `parameter_default` for each pass-through parameter (rows and inputs), and `lift_lock` by a dry `plan_lift` over the rows the table lifts. `scope_state` is `NotShared` when `enclosing` is `None` or the index has one instance, `Unknown` when the index can't answer, else the options with the lock order of the Rulings.
- [ ] **Step 4: Run them, expect PASS**; then `cargo test -p celesta-editor`.
- [ ] **Step 5: Commit** — `feat(editor): decide what an edit writes for this scene or the shared value`

### Task 3: Scoped commits

**Files:**
- Modify: `crates/editor/src/element_edit/commit.rs` (`commit_scoped`, `scoped_edits`, `CommitError::Scope`), `crates/editor/src/element_edit.rs` (`commit_element` takes the scope; the scene-calls job beside `sync_element_targets`), `crates/editor/src/element_rows.rs` (`HeldCommit.scope`, gate, scoped removal), `crates/editor/src/transform_drag.rs` (`end_transform_drag` passes the scope, refuses on `Unknown`), `crates/editor/src/inline_text.rs`, `crates/editor/src/refresh.rs` / `view.rs` (`scope_choices: ScopeChoices`, `scene_calls: Option<SceneCalls>`, the kept reload's old position; draining held rows when a sweep lands)
- Test: `crates/editor/src/tests/scope_commit.rs` (declared in `tests.rs`)

**Interfaces:**
- Consumes: Tasks 1–2.
- Produces — the entry point design 4 calls with targets built for any element (a `Sequence` window: `PropKey::Prop("from" | "durationInFrames" | "offset")`, `component` = the `ComponentTargets` of its enclosing component's call, `enclosing` = that component):

```rust
// element_edit/commit.rs
pub(crate) enum CommitError { /* existing */ Scope(ScopeLock) }
/// The new texts for `values` on the element `expected` describes: checks `expected`'s editability, re-resolves with `resolve_again` against `texts`, refuses unless every target is at the same place, then writes as resolved (`scope: None`) or under `scope`.
pub(crate) fn commit_scoped(texts: &Texts, expected: &ElementTargets, resolve_again: impl FnOnce(&Texts) -> Result<ElementTargets, CommitError>, scope: Option<Scope>, calls: Option<&SceneCalls>, values: &[(PropKey, Option<EditValue>)]) -> Result<Vec<(PathBuf, String)>, CommitError>;
/// Per file, the edits for `values` under `scope` on freshly resolved targets: `scoped_row` for each, one `plan_lift` for every `Lift` row plus an `Edit::Insert` per lifted param at `S`, the default's edit for `Default`.
pub(crate) fn scoped_edits(texts: &Texts, fresh: &ElementTargets, scope: Scope, calls: Option<&SceneCalls>, values: &[(PropKey, Option<EditValue>)]) -> Result<BTreeMap<PathBuf, Vec<Edit>>, CommitError>;
/// The selected element's commit: `commit_scoped` re-resolving with `resolve_rows` for `report`.
pub(crate) fn commit_files(texts: &Texts, expected: &ElementTargets, report: &LayerSourceReport, scope: Option<Scope>, calls: Option<&SceneCalls>, values: &[(PropKey, Option<EditValue>)]) -> Result<Vec<(PathBuf, String)>, CommitError>;
// scope.rs
pub(crate) enum CommitGate { Go(Option<Scope>), Hold(String), Refuse(String) }
pub(crate) fn commit_gate(state: &ScopeState, remembered: Option<Scope>, targets: &ElementTargets, keys: &[PropKey]) -> CommitGate;
// element_edit.rs
pub(crate) fn commit_element(&mut self, values: Vec<(PropKey, Option<EditValue>)>, scope: Option<Scope>, done: impl FnOnce(&mut Self, Result<(), String>, &mut Context<Self>) + 'static, cx: &mut Context<Self>);
// element_rows.rs
pub(crate) struct HeldCommit { /* existing */ pub(crate) scope: Option<Scope> }
```

- [ ] **Step 1: Write the failing tests** (fixtures `price_tag.tsx` + `price.tsx` + `intro.tsx` on temp files; `commit_files` inside `write_all_recorded`, history through `EditHistory`):
  - `this_scene_on_a_pass_through_writes_the_call_site_only` — only `price.tsx` changes.
  - `this_scene_lifts_a_literal_and_passes_it` — the spec's diff exactly (`{ label, rectWidth = 200 }`, `width={rectWidth}`, `<PriceTag label="Sale" rectWidth={260} />`); one `HistoryEntry` with two files; `undo` restores both.
  - `a_second_scene_after_a_lift_resolves_as_pass_through` — editing in Intro inserts `rectWidth={180}` at Intro's call; `price_tag.tsx` unchanged.
  - `shared_value_on_a_lifted_prop_writes_the_default_and_keeps_call_site_attributes` — `rectWidth = 200` → `rectWidth = 240`; `rectWidth={260}` stays.
  - `shared_value_adds_a_default_when_there_is_none` — `{ label, rectWidth }` → `{ label, rectWidth = 240 }`.
  - `a_move_lifts_x_and_y_in_one_entry` — the Review Focus case, one entry, undo restores both.
  - `removal_under_this_scene_deletes_at_s` — "Use the shared value": `rectWidth={260}` removed, `price_tag.tsx` unchanged; a literal at `E` under "This scene" is `NotRemovable`.
  - `per_scene_unsupported_values_are_refused` — a keyframe and `style.fontSize` under "This scene" → `CommitError::Scope(PerSceneUnsupported)`, nothing written.
  - `a_locked_lift_is_refused_with_its_text` — imported props type → "`PriceTag`'s props type is in another file".
  - `commit_scoped_takes_any_element_s_targets` — targets built by hand for `<Sequence from={30}>` inside `PriceTag` with a `resolve_again` closure: "This scene" writes `from={sequenceFrom}`, `sequenceFrom = 30`, `sequenceFrom={45}` at `S`; a `resolve_again` that finds another span → `SourceChanged`.
  - `commit_gate_holds_until_the_index_answers` — `Unknown`: a pass-through row → `Go(None)`; with "Shared value" remembered → `Hold("Scanning scenes…")`; a literal row → `Hold`; a failed scan → `Refuse("Couldn't scan scenes: x")`; `NotShared` → `Go(None)`; a remembered "This scene" now locked → `Refuse` with the lock's text.
  - `a_held_commit_keeps_the_scope_it_was_made_with` — `HeldCommits::push` of the same row replaces value and scope; `pop` returns it.
- [ ] **Step 2: Run them, expect FAIL** — `cargo test -p celesta-editor -- tests::scope_commit::`
- [ ] **Step 3: Implement.** `commit_files`' body becomes `commit_scoped` (the check against `expected` is unchanged); `scope: None` keeps today's writes. Callers ask `commit_gate` with the element's state: `commit_element_row` (rows, scrubs, removal, inline text: `Hold` goes to `HeldCommits` with the note shown on the row), `end_transform_drag` (`Hold`/`Refuse` → the drag's notice). The scene-calls job reads each call's file with `written_attributes`; a landed sweep restarts it and drains held rows. The kept reload stores the selected `SourcePos`; the first `LookupFate::Show` for it calls `carry`.
- [ ] **Step 4: Run them, expect PASS**; then `cargo test -p celesta-editor`.
- [ ] **Step 5: Commit** — `feat(editor): write an edit for this scene or for the shared value`

### Task 4: The scope control

**Files:**
- Modify: `crates/editor/src/layer_panel.rs` (impact line → control; `selection_impact` filtered by scope), `crates/editor/src/impact.rs` (`ImpactFilter` replaces `written_at`), `crates/editor/src/view.rs` (`scope_hover: Option<Scope>`), `crates/editor/src/element_rows_view.rs` (shown value and locks from `scoped_row`; remove button tooltip), `crates/editor/src/transform_drag.rs` (label line, header), `crates/editor/src/inline_text_view.rs` (lock)
- Test: `crates/editor/src/impact.rs` tests, `crates/editor/src/tests/scope.rs`

**Interfaces:**
- Consumes: Tasks 2–3.
- Produces:

```rust
// impact.rs
pub(crate) enum ImpactFilter { All, Through(SourcePos), Except(Vec<SourcePos>) /* calls that set their own */ }
pub(crate) fn impact_view(report: &LayerSourceReport, occurrences: Option<&OccurrenceState>, chapters: &[ReactChapter], frame: u64, filter: &ImpactFilter) -> ImpactView;
// scope.rs
/// The filter for `scope` (hovered, else chosen); `None` scope keeps design 2's `written_at` rule.
pub(crate) fn impact_filter(scope: Option<Scope>, targets: &ElementTargets, calls: Option<&SceneCalls>) -> ImpactFilter;
pub(crate) fn scope_in_effect(scope: Scope) -> &'static str; // "Edits change this scene" / "Edits change the shared value"
pub(crate) fn remove_tooltip(scope: Option<Scope>, row: &ScopedRow) -> &'static str; // "Use the shared value" / "Remove"
```

- [ ] **Step 1: Write the failing tests** — `impact_view_filters_through_and_except`; `impact_filter_follows_the_chosen_scope` (This scene → `Through(S)`; Shared → `Except` the calls that set their own; no scope → `written_at`'s rule); `hovering_previews_the_other_option`; `scope_in_effect_names_the_scope`; `the_remove_button_says_use_the_shared_value_at_s`; `a_default_row_shows_the_default` (`ScopedRow::Default { shown }` is the row's value).
- [ ] **Step 2: Run them, expect FAIL** — `cargo test -p celesta-editor -- impact_view_filters impact_filter hovering_previews scope_in_effect the_remove_button a_default_row`
- [ ] **Step 3: Implement.** With `ScopeState::Options`, the impact line becomes "Edits change" + a two-tab control ([This scene] [Shared value], the 2c tabs look) with `scope_line` under it; a locked option is disabled, its reason the line's text and its tooltip; clicking sets `ScopeChoices`. `Unknown` shows its note in place of the control. Rows show and lock by `scoped_row`; drag, scrub and inline text lock `SetsOwn` rows; the drag label's second line and the header use `scope_in_effect`.
- [ ] **Step 4: Run them, expect PASS**; then `cargo test -p celesta-editor`, clippy, fmt.
- [ ] **Step 5: Commit** — `feat(editor): choose whether an edit changes this scene or the shared value`

### Task 5: Docs, example and handoff

**Files:**
- Modify: `packages/website/content/{en,ja}/docs/01-(getting-started)/03-preview.md` (the control, what each option writes, the locks, "Use the shared value", rows under "Shared value" while a scene sets its own)
- Modify: `docs/superpowers/specs/2026-10-23-editor-edit-scope-design.md` (only what the build settled: the lock texts this plan adds, the per-row "Shared value" rule, removal per scope, "Use the shared value" as the row's remove button, the called-twice lock, holding while scanning, memory carried across a lift, held commits keeping their scope, the impact line kept for unshared elements)
- Check: `examples/delta/v0.7.0/demos/EditorDemo.tsx` — update if it shows the impact line this design replaces
- Create: `docs/handoff/2026-10-26-edit-scope.md` (decisions, known gaps — hand-placed scene components lock as "Passes through", several calls in one scene lock instead of writing each — and "Verify by hand": the spec's manual list with before/after screenshots of the same scene)

- [ ] **Step 1: Write the docs**; `pnpm --filter @celesta/website check`.
- [ ] **Step 2: Commit** — `docs(editor): describe edit scope`

## Order

Task 1 → Task 2 → Task 3 → Task 4 → Task 5. Task 1's Splice commit (Step 3) lands first and is all design 4's Tasks 2–3 need; design 4's Task 4 waits for Task 3 here (`commit_scoped`, `SceneCalls`, `commit_gate`).
