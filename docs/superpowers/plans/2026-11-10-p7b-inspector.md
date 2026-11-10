# P7b Inspector Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One collapsible section, one three-lane row and one draggable, click-to-type number control for every label and value pair in the right dock, with units from the schema.

**Architecture:** Units travel as an optional `unit` on number fields from the TypeScript schemas through `celesta-react-bridge` to the editor. A new `crates/editor/src/inspector.rs` holds `InspectorSection` (a gpui-kit ghost `Button` header on the band) and `InspectorRow` (label, value and trailing lanes); `crates/editor/src/scrub_value.rs` holds the number control and its pure helpers. The layer rows keep their existing `Scrub`; the props-file rows get a `PropertyScrub` built on the same helpers. Then every caller of `widgets::inspector_*` moves over and those helpers go.

**Tech Stack:** Rust (gpui-kit 0.9 / gpui-pre 0.3.11, serde), TypeScript (`@celesta.studio/react`, Vitest), Markdown docs.

**Spec:** `docs/superpowers/specs/2026-11-10-p7b-inspector-design.md` (and P7a's, `docs/superpowers/specs/2026-11-10-p7a-visual-foundation-design.md`, for the design direction).

## Global Constraints

- Build env: `PKG_CONFIG_PATH="$(brew --prefix ffmpeg@8)/lib/pkgconfig"`; Rust tools on `PATH` from `~/Library/Application Support/mbx/bin` and `~/.cargo/bin`; builds need the sandbox off.
- Test command for the editor: `cargo test -p celesta-editor -p celesta-editor-theme`; lint: `cargo clippy -p celesta-editor -p celesta-editor-theme -p celesta-react-bridge --all-targets -- -D warnings`; format: `cargo fmt --all --check` (CI runs it).
- TypeScript tests: `pnpm --filter @celesta.studio/react exec vitest run <file>`.
- No raw color values outside `celesta-editor-theme`; text roles only `text_sm`, `text_base` (the `style_guard` test enforces it); lucide icons only; radius 0; no uppercase transforms; no gradients.
- Monospace (`cx.theme().mono_font_family`) for numbers, sizes, timecodes and identifiers.
- Sizes from `celesta_editor_theme::metrics` or GPUI's `rem` helpers; a `px()` that stays says `// physical:` why.
- UI tests use `VisualTestContext` and `debug_selector`, as `panel_header.rs` does (gpui-kit's `test-support` feature conflicts on taffy).
- Every commit is signed (1Password); if signing fails with "failed to fill whole buffer" or "agent returned an error", stop and ask the user to unlock. Never amend. Trailer: `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Copy, verbatim: section ids `inspector/composition`, `react/composition`, `react/chapters`, `properties/<group name lowercased>`, `properties/component`, `properties/properties`, `properties/layer`, `properties/called-from`; tooltip "Reset to default"; section titles "Composition", "Properties", "Layer", "Chapters".
- Display-only units: never convert a value (no %), never validate a unit.

## Review Focus

1. A reload (own write or file change) rebuilding the rows while a number row is open for typing: the row stays open with its caret and typed text; it does not snap back to the resting display.
2. Pressing a value while another row holds typed text: that text stays in its focused field, as a label press does today (`scrub_takes_focus`); pressing the same row's value replaces its typed text.
3. Escape during a value drag: the value goes back, nothing is written, and the row does not open for typing.
4. Collapsing a section while one of its rows holds typed text: the header button takes the focus, the field blurs and submits as any blur does; the text is not silently dropped.
5. A narrow dock (the right dock at its minimum width) with a long label, a long value and a unit: label ellipsizes with its tooltip, the value ellipsizes before the unit, the trailing lane keeps its width.

---

### Task 1: Units on number fields

**Files:**
- Modify: `packages/react/src/registry.ts` (the `number` variant of `ComponentPropertyField`)
- Modify: `packages/react/src/property-tables.ts` (`PrimitiveFieldType` number variant, `NumberRange`, `num()`)
- Modify: `packages/react/src/components.ts:1094-1170` (the built-in tables)
- Modify: `crates/react-bridge/src/types.rs` (`ComponentPropertyField::Number`, `PrimitivePropertyField::Number`)
- Modify: `crates/editor/src/element_edit/resolve.rs:40-60` (`schema_field` carries `unit`)
- Modify: every Rust struct literal of either `Number` variant (compile errors list them: `property_panel.rs`, `transform_drag.rs`, `element_rows.rs`, `refresh.rs`, tests) — add `unit: None`
- Modify: `skills/celesta/references/project-data.md:39,72`
- Test: `packages/react/test/property-tables.test.mts`, `crates/react-bridge/src/types.rs` tests

**Interfaces:**
- Produces: `unit?: string` in TS; `unit: Option<String>` (`#[serde(default)]`) on both Rust `Number` variants; `schema_field` copies it.

- [ ] **Step 1: Write the failing tests**

```ts
// packages/react/test/property-tables.test.mts
test('num keeps a unit and omits it when unset', () => {
  assert.deepEqual(num(0, { unit: 'px' }), { type: 'number', defaultValue: 0, unit: 'px' });
  assert.equal('unit' in num(0), false);
});

test('built-in tables carry their units', () => {
  const field = (table: string, group: string, key: string) =>
    listPrimitiveProperties()[table]?.groups.find((g) => g.name === group)?.fields.find((f) => f.key === key)?.field;
  assert.equal(field('Rect', 'Transform', 'x')?.unit, 'px');
  assert.equal(field('Rect', 'Transform', 'rotation')?.unit, '°');
  assert.equal(field('Rect', 'Size', 'cornerRadius')?.unit, 'px');
  assert.equal(field('Rect', 'Transform', 'scale')?.unit, undefined);
  assert.equal(field('Rect', 'Transform', 'opacity')?.unit, undefined);
});
```

(Adjust the `field` accessor to the table's real row shape if it differs; the assertions stay.)

```rust
// crates/react-bridge/src/types.rs, tests module
#[test]
fn a_number_field_keeps_its_unit_and_loads_without_one() {
    let with: PrimitivePropertyField =
        serde_json::from_str(r#"{"type":"number","unit":"px"}"#).unwrap();
    assert!(matches!(with, PrimitivePropertyField::Number { unit: Some(ref u), .. } if u == "px"));
    let without: ComponentPropertyField =
        serde_json::from_str(r#"{"type":"number","defaultValue":1}"#).unwrap();
    assert!(matches!(without, ComponentPropertyField::Number { unit: None, .. }));
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `pnpm --filter @celesta.studio/react exec vitest run test/property-tables.test.mts` and `cargo test -p celesta-react-bridge a_number_field_keeps`
Expected: FAIL (TS: `unit` not kept / undefined; Rust: no field `unit`).

- [ ] **Step 3: Add `unit` on both sides and the built-in units**

`num()` spreads `unit` only when defined, as it does `step`. Built-in units (spec, Units): `px` for x, y, width, height, maxWidth, fontSize, cornerRadius, strokeWidth; `°` for rotation; `s` for startFrom on each component whose prop docs say seconds (check each `startFrom` doc comment in `components.ts`; leave it unitless where it is not seconds). `schema_field` copies `unit`. Add `unit: None` wherever the compiler asks.

In `project-data.md`, add `unit: 'px'` to the `size` example and change line 72 to: "`min`, `max`, `step`, and `unit` are Inspector hints; they are not enforced, and `unit` is shown after the value, never converted." (drop "The app's Inspector is read-only", which is no longer true).

- [ ] **Step 4: Run the tests and the editor build**

Run: the two commands from Step 2, then `cargo test -p celesta-editor -p celesta-editor-theme`
Expected: PASS, the editor suite unchanged (count as before plus none).

- [ ] **Step 5: Commit**

```bash
git add packages/react crates/react-bridge crates/editor skills/celesta
git commit -m "feat(react, react-bridge): a display unit on number fields"
```

---

### Task 2: Inspector tokens

**Files:**
- Modify: `crates/editor-theme/src/metrics.rs`

**Interfaces:**
- Produces: `pub fn inspector_label_width() -> Rems` (= `rems(7.0)`), `pub fn inspector_trailing_width() -> Rems` (= `rems(1.25)`).

- [ ] **Step 1: Write the failing test** (in `metrics.rs` tests)

```rust
#[test]
fn inspector_lanes_fit_their_contents() {
    assert_eq!(inspector_label_width(), rems(7.0));
    assert!(inspector_label_width().0 > list_row_height().0);
    // An xsmall icon button is 1rem (`size_4`).
    assert!(inspector_trailing_width().0 >= 1.0);
}
```

- [ ] **Step 2: Run it** — `cargo test -p celesta-editor-theme inspector_lanes` → FAIL (not defined).
- [ ] **Step 3: Add the two functions** with doc comments naming their lanes.
- [ ] **Step 4: Run it** → PASS.
- [ ] **Step 5: Commit** — `git commit -am "feat(editor-theme): inspector lane tokens"`

---

### Task 3: `InspectorSection`, `InspectorRow`, collapse state

**Files:**
- Create: `crates/editor/src/inspector.rs`
- Modify: `crates/editor/src/main.rs` (`mod inspector;`), `crates/editor/src/view.rs` (field `collapsed_sections: CollapsedSections` on `EditorView`, default empty)

**Interfaces:**
- Consumes: Task 2's tokens.
- Produces:
  - `pub(crate) struct CollapsedSections` with `fn is_collapsed(&self, id: &str) -> bool` and `fn toggle(&mut self, id: &str)`.
  - `#[derive(IntoElement)] pub(crate) struct InspectorSection` — `new(id: impl Into<SharedString>, title: impl Into<SharedString>)`, `collapsed(bool)`, `on_toggle(impl Fn(&mut Window, &mut App) + 'static)`, `trailing(impl IntoElement)`, `child(impl IntoElement)` / `children(...)`. Header: a band-colored (`title_bar`) div, `list_row_height()` tall, `border_b_1`, holding a full-width gpui-kit `Button` (`ghost`, `xsmall`, `justify_start`) with `IconName::ChevronDown`/`ChevronRight` and the title (`text_sm`, `FontWeight::MEDIUM`); the trailing slot after it. Debug selectors: `{id}-header` (the button), `{id}-body` (the rows' container, absent when collapsed).
  - `#[derive(IntoElement)] pub(crate) struct InspectorRow` — `new(id, label: impl Into<SharedString>)`, `value(impl IntoElement)`, `trailing(impl IntoElement)`, `indented(bool)`, `top_aligned(bool)`, `sub_line(impl IntoElement)`. Lanes: label `w(inspector_label_width())` (minus one `rems(0.75)` indent step when indented, `pl` grows by the same), `text_sm`, muted, ellipsis, tooltip with the full label; value `flex_1 min_w_0`; trailing `w(inspector_trailing_width()) flex_none`, always present. `min_h(list_row_height())`, `px(panel_padding())`, no border. Sub-lines in a column whose left edge is the value lane's. Debug selectors: `{id}-label`, `{id}-value`, `{id}-trailing`.

- [ ] **Step 1: Write the failing tests** (Host pattern from `panel_header.rs`)

```rust
#[test]
fn a_section_toggles_by_id() {
    let mut sections = CollapsedSections::default();
    assert!(!sections.is_collapsed("properties/transform"));
    sections.toggle("properties/transform");
    assert!(sections.is_collapsed("properties/transform"));
    sections.toggle("properties/transform");
    assert!(!sections.is_collapsed("properties/transform"));
}

#[gpui_kit::test]
fn clicking_the_header_hides_and_shows_the_rows(cx: &mut TestAppContext) {
    // Host holds a `CollapsedSections`; builds
    // InspectorSection::new("s", "Transform").collapsed(..).on_toggle(..toggle "s"..)
    //   .child(InspectorRow::new("r", "X").value("0"))
    // assert `s-body` present; click `s-header`; assert `s-body` absent; click again; present.
}

#[gpui_kit::test]
fn space_toggles_a_focused_header(cx: &mut TestAppContext) {
    // click `s-header` once (focuses it, collapses), then simulate_keystrokes("space"):
    // `s-body` shows again.
}

#[gpui_kit::test]
fn collapsing_blurs_a_field_inside_the_section(cx: &mut TestAppContext) {
    // a row whose value is an `Input`; Host subscribes to its InputEvent::Blur.
    // focus the input, click `s-header`: Blur was received (Review Focus 4).
}

#[gpui_kit::test]
fn value_lanes_share_one_edge(cx: &mut TestAppContext) {
    // three rows: plain, `.trailing(Icon::new(IconName::Lock))`, `.indented(true)`;
    // the `-value` bounds' origin.x are equal; the `-trailing` widths are equal.
}

#[gpui_kit::test]
fn a_narrow_row_keeps_its_trailing_lane(cx: &mut TestAppContext) {
    // window 220px wide; a row with a 60-character label and a 60-character value:
    // `-trailing` width == inspector_trailing_width at 13px; `-value` right edge <= `-trailing` left edge.
}
```

- [ ] **Step 2: Run them** — `cargo test -p celesta-editor inspector::` → FAIL (module missing).
- [ ] **Step 3: Implement `inspector.rs`** per the Interfaces block.
- [ ] **Step 4: Run them** → PASS; then the full editor suite → PASS.
- [ ] **Step 5: Commit** — `git commit -m "feat(editor): InspectorSection, InspectorRow, collapse state"`

---

### Task 4: `ScrubValue` and its helpers

**Files:**
- Create: `crates/editor/src/scrub_value.rs`
- Modify: `crates/editor/src/main.rs` (`mod scrub_value;`), `crates/editor/src/property_controls.rs` (`parse_number_input` takes the unit; `ControlEvent::Focused`; `InputEvent::Focus` maps to it instead of returning)

**Interfaces:**
- Consumes: Task 3 (none directly), Task 1's `unit`.
- Produces:
  - `pub(crate) const DRAG_THRESHOLD: f32 = 3.0;`
  - `pub(crate) fn drag_distance(dx: f32) -> Option<f32>` — `None` while `|dx| < DRAG_THRESHOLD`, else `dx - DRAG_THRESHOLD * dx.signum()` (the value moves from the threshold on, without a jump).
  - `pub(crate) fn display_number(value: f64, step: Option<f64>, integer: bool) -> String` — integer: no decimals; a step: its decimal places (0.01 → 2, 0.5 → 1, 2 → 0); otherwise `widgets::format_component_number`.
  - `pub(crate) fn with_unit(number: &str, unit: Option<&str>) -> String` — `"120 px"`, or the number alone.
  - `parse_number_input(text: &str, unit: Option<&str>) -> Result<f64, String>` — trims, strips a trailing `unit` with or without one space before it, then parses as today.
  - `ControlEvent::Focused` (the field gained the focus: a click, Tab, or Enter on the resting display).
  - `#[derive(IntoElement)] pub(crate) struct ScrubValue` — `new(id, input: Entity<InputState>)`, `text(String)` (the resting display, already `with_unit`), `typing(bool)`, `locked(bool)`, `hint(Option<String>)`, `on_press(impl Fn(f32 /*window x*/, &mut Window, &mut App) + 'static)`, `on_type(impl Fn(&mut Window, &mut App) + 'static)`. Typing: renders the existing single-line `text_input` (move it to `pub(crate)` in `property_controls.rs`). Resting: a monospace `text_base` div that `track_focus`es the input's focus handle (so Tab lands on it, `InputEvent::Focus` fires and the row opens for typing), `secondary_hover` background on hover, `CursorStyle::ResizeLeftRight`, `on_mouse_down(Left)` → `window.prevent_default()` then `on_press(x)`, `on_key_down` Enter → `on_type()` (after Escape the handle keeps the focus, so no second `Focus` event comes; the view sets `typing` itself). Locked: muted text, no handlers. Debug selectors `{id}-rest`, `{id}-input`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn a_press_becomes_a_drag_after_three_pixels() {
    assert_eq!(drag_distance(2.9), None);
    assert_eq!(drag_distance(-2.9), None);
    assert_eq!(drag_distance(3.0), Some(0.0));
    assert_eq!(drag_distance(10.0), Some(7.0));
    assert_eq!(drag_distance(-10.0), Some(-7.0));
}

#[test]
fn numbers_show_their_steps_places_and_unit() {
    assert_eq!(display_number(3.0, None, true), "3");
    assert_eq!(display_number(1.234, Some(0.01), false), "1.23");
    assert_eq!(display_number(400.0, Some(100.0), false), "400");
    assert_eq!(display_number(0.125, None, false), "0.125");
    assert_eq!(with_unit("120", Some("px")), "120 px");
    assert_eq!(with_unit("1.00", None), "1.00");
}

#[test]
fn typed_numbers_may_carry_their_own_unit() {
    assert_eq!(parse_number_input("120px", Some("px")), Ok(120.0));
    assert_eq!(parse_number_input(" 120 px ", Some("px")), Ok(120.0));
    assert_eq!(parse_number_input("120", Some("px")), Ok(120.0));
    assert!(parse_number_input("120 em", Some("px")).is_err());
    assert!(parse_number_input("120px", None).is_err());
}

#[gpui_kit::test]
fn the_resting_value_reports_a_press_and_typing_shows_the_field(cx: &mut TestAppContext) {
    // Host with an InputState and a `typing: bool`; ScrubValue::new("v", input).text("120 px")
    //   .typing(typing).on_press(record x).
    // `v-rest` present, `v-input` absent; simulate a mouse down at `v-rest` center: on_press got its x.
    // set typing = true, notify: `v-input` present, `v-rest` absent.
}

#[gpui_kit::test]
fn a_locked_value_ignores_presses(cx: &mut TestAppContext) {
    // .locked(true): mouse down on `v-rest` records nothing.
}

#[gpui_kit::test]
fn enter_on_the_focused_resting_value_opens_typing(cx: &mut TestAppContext) {
    // focus the input's handle while typing = false (the state after Escape);
    // simulate_keystrokes("enter"): on_type was called.
}
```

Update the existing `number_input_accepts_finite_numbers_only` test's calls to pass `None`.

- [ ] **Step 2: Run them** — `cargo test -p celesta-editor scrub_value:: property_controls::` → FAIL (missing items).
- [ ] **Step 3: Implement** per the Interfaces block. Callers of `parse_number_input` pass the row's field unit (`None` where there is no field).
- [ ] **Step 4: Run them** → PASS; full editor suite → PASS.
- [ ] **Step 5: Commit** — `git commit -m "feat(editor): ScrubValue, the draggable number control"`

---

### Task 5: Layer rows on the new pieces

**Files:**
- Modify: `crates/editor/src/element_rows.rs` (`Scrub`: `origin: ScrubOrigin`, threshold in `follow`, a pure `finish`; `ElementRow`: `typing: bool`; `begin_row_scrub` takes the origin; `end_row_scrub` takes `window`; `ControlEvent::Focused` sets `typing`, `Submit`/`Escape` clear it)
- Modify: `crates/editor/src/element_rows_view.rs` (`element_sections` and `element_row` on `InspectorSection`/`InspectorRow`/`ScrubValue`)
- Modify: `crates/editor/src/playback.rs:357-372` (pass `window` to `end_row_scrub`)
- Modify: `crates/editor/src/layer_panel.rs` (`labelled` uses `inspector_label_width()`; remove `LABEL_WIDTH`)

**Interfaces:**
- Consumes: Tasks 2–4.
- Produces:
  - `pub(crate) enum ScrubOrigin { Label, Value }`
  - `#[derive(Clone, Copy, Debug, PartialEq)] pub(crate) enum ScrubEnd { Write(f64), Nothing, Type }` and `Scrub::finish(&self) -> ScrubEnd`: `Type` when the origin is `Value` and the pointer never passed the threshold; `Nothing` when the value is back at its start (or a label press never moved); else `Write(value)`.
  - `Scrub::follow(x, shift)` changes nothing until `drag_distance(x - start_x)` is `Some`, then uses that distance.
  - `EditorView::focus_element_row_input(&mut self, key: &PropKey, window: &mut Window, cx: &mut Context<Self>)` — focuses the row's input and selects its text (`InputState::focus`, `select_all`), which sets `typing` through `ControlEvent::Focused`.
  - Section ids `properties/<group name lowercased>` and `properties/component`.

- [ ] **Step 1: Write the failing tests** (pure, on `Scrub` in `element_rows.rs` tests; build a `Scrub` with the existing test helpers there, or add a `#[cfg(test)] fn for_test(origin, start_x, start_value, field)` constructor)

```rust
#[test]
fn a_value_press_without_a_drag_opens_typing() {
    let scrub = Scrub::for_test(ScrubOrigin::Value, 100.0, 5.0, None);
    assert_eq!(scrub.finish(), ScrubEnd::Type);
}

#[test]
fn a_label_press_without_a_drag_writes_nothing() {
    let scrub = Scrub::for_test(ScrubOrigin::Label, 100.0, 5.0, None);
    assert_eq!(scrub.finish(), ScrubEnd::Nothing);
}

#[test]
fn a_drag_moves_from_the_threshold_and_writes_once() {
    let mut scrub = Scrub::for_test(ScrubOrigin::Value, 100.0, 5.0, None);
    assert!(!scrub.follow(102.0, false), "inside the threshold");
    assert!(scrub.follow(110.0, false));
    assert_eq!(scrub.finish(), ScrubEnd::Write(12.0)); // 5 + (10 - 3) * 1
}

#[test]
fn a_drag_back_to_the_start_writes_nothing() {
    let mut scrub = Scrub::for_test(ScrubOrigin::Value, 100.0, 5.0, None);
    scrub.follow(110.0, false);
    scrub.follow(103.0, false);
    assert_eq!(scrub.finish(), ScrubEnd::Nothing);
}

#[test]
fn typing_follows_the_fields_focus() {
    // An ElementRow-level pure check, or on a small `TypingState` the row holds:
    // Focused → typing; Submit { blur: true } → not typing; Escape → not typing;
    // a reload that rebuilds `shown` while typing keeps typing (Review Focus 1).
}
```

Existing scrub tests that assumed movement from the first pixel get their pointer positions moved by the threshold; their expected values stay.

- [ ] **Step 2: Run them** — `cargo test -p celesta-editor element_rows::` → FAIL.
- [ ] **Step 3: Implement.** `end_row_scrub` on `ScrubEnd::Type` cancels the scrub (no override, no write) and calls `focus_element_row_input`. `escape_row_scrub` is unchanged (cancel, no typing: Review Focus 3). A value press keeps `begin_row_scrub`'s focus rule (`scrub_takes_focus`, Review Focus 2). A row that does not scrub but is editable (input rows) opens typing straight from the press. In `element_row`: number rows render `ScrubValue` (text `with_unit(display_number(...), unit)`, `typing` from the row, `locked` as today, `on_press` → `begin_row_scrub(key, x, ScrubOrigin::Value, ..)`); other kinds keep `render_control`; the label drag passes `ScrubOrigin::Label`; the state icon or remove button goes in `trailing`; Set at, reason, keyframe jumps and messages become `sub_line`s; `INDENT` becomes the row's `indented(true)`. Sections use `InspectorSection` with `collapsed`/`on_toggle` against `self.collapsed_sections` (Component section: title `Component · {name}`, id `properties/component`).
- [ ] **Step 4: Run** the module tests, then the full editor suite and clippy → PASS, clean.
- [ ] **Step 5: Commit** — `git commit -m "feat(editor): layer rows drag their value and open for typing"`

---

### Task 6: Props-file rows on the new pieces

**Files:**
- Modify: `crates/editor/src/property_panel.rs` (`PropertyScrub`, `PropertyPanelState::scrub`, `RowState`/`PropertyRow` `typing`, event mapping)
- Modify: `crates/editor/src/property_panel_view.rs` (`property_section` → `InspectorSection` `properties/properties`, spinner in `trailing`; `property_row` → `InspectorRow`, reset → `trailing` icon button `RotateCcw`, tooltip "Reset to default"; "Set by --props" and messages → `sub_line`s; numbers → `ScrubValue`)
- Modify: `crates/editor/src/playback.rs` (`drag_move` → `continue_property_scrub`, `drag_end` → `end_property_scrub(window, cx)`), `crates/editor/src/render.rs:108` (Escape → `escape_property_scrub()` too)

**Interfaces:**
- Consumes: Tasks 3–5 (`ScrubEnd`, `drag_distance`, `display_number`, `with_unit`, `ScrubValue`), `element_rows::scrub_value`, `element_edit::resolve::schema_field` (to hand `scrub_value` a `PrimitivePropertyField`).
- Produces:
  - `pub(crate) struct PropertyScrub` with `start(key: &str, field: &ComponentPropertyField, start_x: f32, start_value: f64) -> Self`, `follow(&mut self, x: f32, shift: bool) -> bool`, `finish(&self) -> ScrubEnd`, `shown(&self, key: &str) -> Option<f64>`.
  - `EditorView::{begin_property_scrub, continue_property_scrub, end_property_scrub, escape_property_scrub, focus_property_input}`; `end_property_scrub` on `Write(v)` calls `commit_property(key, Some(number_value(v)), TextUpdate::Always, cx)` once; on `Type` focuses the input.
  - A locked row (`ValueSource::Props`) does not start a scrub.

- [ ] **Step 1: Write the failing tests** (`property_panel.rs` tests)

```rust
#[test]
fn a_props_file_drag_writes_once_on_release() {
    let field = ComponentPropertyField::Number {
        label: None, default_value: 10.0, min: Some(0.0), max: None,
        step: None, integer: None, unit: Some("px".into()),
    };
    let mut scrub = PropertyScrub::start("gap", &field, 100.0, 10.0);
    assert_eq!(scrub.shown("gap"), None, "nothing shown before the threshold");
    scrub.follow(90.0, false);
    assert_eq!(scrub.shown("gap"), Some(3.0)); // 10 + (-10 + 3)
    scrub.follow(50.0, false);
    assert_eq!(scrub.shown("gap"), Some(0.0), "clamped to min");
    assert_eq!(scrub.finish(), ScrubEnd::Write(0.0));
}

#[test]
fn a_props_file_press_without_a_drag_opens_typing() {
    let field = /* as above */;
    assert_eq!(PropertyScrub::start("gap", &field, 100.0, 10.0).finish(), ScrubEnd::Type);
}

#[test]
fn typing_survives_a_reload_that_rebuilds_the_rows() {
    // RowState with typing = true; offer_loaded(...) from a later reload: typing stays true
    // and the typed text is not replaced while `edited` (Review Focus 1).
}
```

- [ ] **Step 2: Run them** — `cargo test -p celesta-editor property_panel::` → FAIL.
- [ ] **Step 3: Implement** per the Interfaces block; `rebuild_property_rows` keeps an existing row's `typing` when it keeps the row.
- [ ] **Step 4: Run** the module tests, the full editor suite, clippy → PASS, clean.
- [ ] **Step 5: Commit** — `git commit -m "feat(editor): props-file rows drag their value, one line each"`

---

### Task 7: Read-only rows and the old helpers

**Files:**
- Modify: `crates/editor/src/panels.rs` (`inspector_panel`: "Composition" section `inspector/composition` for Canvas, Frame rate, Duration, React entry; Asset/Track/Clip/Dialogue/Component/Project properties sections; `react_preview_facts`: "Composition" `react/composition` with Size, Frame rate, Duration, Renderer, Values from and Entry as a `top_aligned` wrapping row, Reload button and warning box below; `react_chapter_list`: `InspectorSection` `react/chapters`, rows unchanged; `properties_panel`: the empty "Layer" note inside `InspectorSection` `properties/layer`)
- Modify: `crates/editor/src/layer_panel.rs` (`layer_section`: "Layer" `properties/layer`, "Called from" `properties/called-from`)
- Modify: `crates/editor/src/widgets.rs` (remove `inspector_row`, `inspector_section`, `inspector_note`)
- Modify: `crates/editor/src/style_guard.rs` (also fail on `inspector_row(` and `inspector_section(` so the old helpers do not return)

**Interfaces:**
- Consumes: Task 3. Section ids from Global Constraints. Values in monospace where they are numbers, sizes, timecodes or identifiers (Canvas, Frame rate, Duration, Size, Start, Duration, IDs, Clips count, Volume, Playback rate).

- [ ] **Step 1: Write the failing test** — extend `style_guard.rs` with `old_inspector_helpers_are_gone`, scanning `src` for `inspector_row(` and `inspector_section(` outside `inspector.rs`.
- [ ] **Step 2: Run it** — `cargo test -p celesta-editor style_guard` → FAIL (listing the call sites).
- [ ] **Step 3: Move each call site** onto `InspectorSection`/`InspectorRow`, then delete the three helpers. Each `InspectorSection` reads and toggles `self.collapsed_sections`.
- [ ] **Step 4: Run** the full editor suite, clippy, `cargo fmt --all --check` → PASS, clean.
- [ ] **Step 5: Commit** — `git commit -m "feat(editor): every inspector tab on the shared section and row"`

---

### Task 8: Docs, screenshots, zoom check, handoff

**Files:**
- Modify: `packages/website/content/en/docs/01-(getting-started)/03-preview.md` and `packages/website/content/ja/docs/01-(getting-started)/03-preview.md` ("Edit properties": drag a number's value or its label, a click without a drag opens it for typing, a press becomes a drag after a few pixels; values show their units; sections collapse by their header and stay collapsed while the selection changes until the editor quits; the props file's **Properties** rows drag too, written once on release, the preview following after the reload; **Reset to default** is the ↺ button at the row's end)
- Create: `docs/handoff/2026-11-10-p7b-inspector.md`

- [ ] **Step 1: Before screenshots.** Build the editor from the P7a tip (`claude/media-components-p7`) into the scratchpad; run `examples/greenscreen/composite.tsx` from the repo root with `CELESTA_PLUGIN_PATH="$PWD/plugins/dist"`, 1440×900; the user clicks "Trust and Open"; capture the React Preview tab, then select a layer by clicking the preview (computer use) and capture the Properties tab, with `screencapture -x -o -l <window id>`.
- [ ] **Step 2: After screenshots.** The same from the branch tip, same scenes.
- [ ] **Step 3: Zoom check.** `BASE_FONT_SIZE` temporarily 12 and 16 (not committed): "Corner radius" fits the label lane; value lanes keep one edge.
- [ ] **Step 4: Docs.** Update both guide pages as listed (ja in the guide's existing ja voice); write the handoff note (what changed, migration, tests, how it was checked, the props-file preview limitation).
- [ ] **Step 5: Final checks and commit** — full editor suite, the TS test file, clippy, `cargo fmt --all --check`; then `git commit -m "docs: P7b user guide and handoff"`.
