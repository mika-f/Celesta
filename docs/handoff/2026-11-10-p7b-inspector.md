# P7b: the Inspector (2026-11-10)

The second sub-project of P7. Spec:
`docs/superpowers/specs/2026-11-10-p7b-inspector-design.md`. Plan:
`docs/superpowers/plans/2026-11-10-p7b-inspector.md`. Built on P7a (PR #802).

## What changed

**Units on number fields.** `unit?: string` on the `number` variant of
`ComponentPropertyField` (`packages/react/src/registry.ts`) and on `num()`
(`packages/react/src/property-tables.ts`); `unit: Option<String>` on both
`Number` variants in `crates/react-bridge/src/types.rs`; `schema_field`
carries it. Display only: never converted, never checked. Built-in tables
(react and shapes): `px` for x, y, width, height, maxWidth, fontSize,
cornerRadius, strokeWidth and the shapes' sizes; `°` for rotation and a
Pie's start and end angles; `s` for a video's startFrom. Ratios
(scale, anchors, opacity, playbackRate, volume) have none.

**Tokens.** `metrics::inspector_label_width()` (7rem) and
`metrics::inspector_trailing_width()` (1.25rem).

**`inspector.rs`.**
- `InspectorSection`: a band header (`title_bar`, `list_row_height()`, one
  hairline) holding a full-width ghost `Button` with a chevron and the title;
  a trailing slot (the props file's spinner). Collapsing focuses the header
  button (its own focus handle), so a field inside blurs and submits.
- `InspectorRow`: label lane (fixed width, ellipsis, tooltip, optional
  `on_label_press`), value lane, trailing lane (always present); sub-lines
  start at the value lane's edge; `indented` and `top_aligned`.
- `fact(id, label, value, mono, cx)`: a read-only row.
- `CollapsedSections` on `EditorView::collapsed_sections`, toggled by
  `EditorView::section(id, title, cx)`. Ids: `inspector/<name>`,
  `react/<name>`, `properties/<group lowercased>`, `properties/component`,
  `properties/properties`, `properties/layer`, `properties/called-from`.

**`scrub_value.rs`.** `ScrubValue` (resting text with unit; a press
reports its x; Enter on the focused resting value asks to type; while typing
it shows the panel's escapable text input), `drag_distance` (3px threshold,
counted from the threshold), `display_number` (integer, step places, else as
written), `with_unit`, `ScrubOrigin`, `ScrubEnd`, `RowTyping`. The resting
value tracks the input's focus handle, so Tab opens a row for typing.

**Layer rows.** `Scrub` has an origin and the threshold; `finish()` decides
`Write`, `Nothing` or `Type`. A value press while playing, or on a row that
types but does not scrub (input rows), opens typing directly. A release
outside the window never opens typing.

**Props-file rows.** `PropertyScrub` (same helpers) shows the dragged value
and writes once on release through `commit_property`; the preview follows
after the reload. Reset to default is the trailing `RotateCcw` button.

**Read-only rows.** Every tab uses `InspectorSection` + `fact`; the React
Preview facts are a "Composition" section with Entry as a wrapping row.
`widgets::inspector_row`, `inspector_section`, `inspector_note` and
`layer_panel::LABEL_WIDTH` are gone; `style_guard::old_inspector_helpers_are_gone`
keeps them out.

## Tests

- TS (Vitest): `num keeps a unit and omits it when unset`, `built-in tables
  carry their units`; existing exact-object expectations updated (react and
  shapes; the shapes tests read react's `dist`, so build it first).
- Bridge: `a_number_field_keeps_its_unit_and_loads_without_one`.
- Theme: `inspector_lanes_fit_their_contents`.
- `inspector::tests`: toggling by id; header click; Space on a focused
  header (Tab focus, KeyDown+KeyUp); collapsing blurs a field inside; value
  lanes share one edge; a narrow row keeps its trailing lane. The test
  window is activated: focus events reach only an active window.
- `scrub_value::tests`: threshold, display, unit-aware parsing, press,
  locked, Enter.
- `tests/element_rows.rs`: value press without a drag types; label press
  writes nothing; drag from the threshold writes once; drag back writes
  nothing; `RowTyping`. Existing scrub tests moved their pointer 3px.
- `property_panel::tests`: props-file drag writes once; press types; typing
  survives a reload.

## How it was checked

- `cargo test -p celesta-editor -p celesta-editor-theme` (795 + 7),
  `cargo test -p celesta-react-bridge --lib`, clippy `-D warnings`,
  `cargo fmt --all --check`, the two Vitest files.
- Screenshots (in the PR, not committed): the editor from the P7a tip and
  from this branch, `examples/greenscreen/composite.tsx`, 1440×900, the React
  Preview tab and the Properties tab with the video layer selected,
  `screencapture -x -o -l <window id>`.
- Zoom: not rerun. Every size P7b adds is on the `rem` (tokens and GPUI
  helpers), so lanes and text scale together; P7a's zoom check covered the
  pixel-sized parts.

## Known limits

- Dragging a props-file row does not move the preview until release (props
  reach the runtime only through the file).
- Collapse state is per session, not saved.
