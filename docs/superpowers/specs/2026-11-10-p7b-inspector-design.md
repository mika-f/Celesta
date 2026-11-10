# P7b: the Inspector (2026-11-10)

The second sub-project of P7, the editor's UI refinement. It builds on P7a
(`docs/superpowers/specs/2026-11-10-p7a-visual-foundation-design.md`, PR
#802) and follows that spec's design direction ("like the timeline") and
tokens. Decided with the user one question at a time.

## Why

The right dock shows label and value pairs in five different layouts:

| Where | Layout today |
| --- | --- |
| Inspector tab (non-React) and the Layer, Asset, Track, Clip, Dialogue, Component sections | Label over value, two lines (`widgets::inspector_row`) |
| React Preview tab's facts (Size, Frame rate, …) | Label left, value right-aligned (a local `row` closure) |
| Properties tab, the layer's rows | Already in lanes: label (`LABEL_WIDTH`), control, a 20px icon lane; dragging the label scrubs |
| Properties tab, the props file's rows | Label over control, a "Reset to default" text button |
| Chapters | A name / start / length list |

None of the section headings (`widgets::inspector_section`) collapse, the
label column cuts "Corner radius" since P7a, numbers carry no units, and
only a label (not the value) can be dragged. Resolve and Premiere put the
label beside the value in one line, collapse sections, and drag the value
itself, with its unit.

## Scope

- **In:** every label and value row in the Inspector, React Preview and
  Properties tabs, on one shared section and one shared row; collapsible
  sections; a number control that is dragged and clicked to type; units on
  number fields.
- **Out:** the Chapters, Plugins and Problems lists (their sections get the
  new header; their rows stay); live preview while dragging a props-file
  row; keyframe creation; section menus or resets (only the slot); saving
  the collapse state across launches; the timeline (P7c); the export page
  (P7d).

## Decisions

1. Scope: all key-value rows of the right dock (not only the Properties
   tab).
2. Units come from the schema: an optional `unit` on number fields, in the
   built-in tables and in the public `ComponentPropertyField`.
3. A number value is dragged directly, and a click without a drag types.
   Dragging the label keeps working.
4. Every section starts open; collapsing is remembered by section id for
   the session, not saved.
5. Approach: Celesta's own `InspectorSection`, `InspectorRow` and
   `ScrubValue` in a new `inspector.rs`, built like P7a's `PanelHeader`.
   gpui-kit's `Accordion` and `DescriptionList` were rejected (the shadcn
   look, and editable rows do not fit `DescriptionList`), and restyling the
   existing helpers was rejected (five layouts would stay five code paths).

## `InspectorSection`

- Header on the band color (`title_bar`), `list_row_height()` tall, one
  hairline under it.
- Leading: a muted chevron (lucide `ChevronDown` open, `ChevronRight`
  collapsed), then the title, `text_sm`, medium weight.
- A trailing slot for section actions (empty in P7b except the props
  file's spinner, below).
- A click anywhere on the header toggles it. The header is focusable;
  Space and Enter toggle it.
- Collapsed shows the header only.
- Each section has a stable id from its caller, made of the tab and the
  section (`properties/transform`, `properties/component`,
  `inspector/composition`). The id does not include the selected layer or
  component, so a section collapsed once stays collapsed while the
  selection changes. A title may include a name (`Component · Card`); the
  id does not.
- State: `EditorView::collapsed_sections: HashSet<SharedString>`.

## `InspectorRow`

Three lanes, as the timeline lays out its rows:

- **Label lane**, one fixed width for every row:
  `metrics::inspector_label_width()` = 7rem (≈91px at 13px), wide enough
  for "Corner radius". `text_sm`, muted, left-aligned. A label that still
  does not fit ends in an ellipsis and shows its full text in a tooltip.
- **Value lane**, the remaining width. Values `text_base`; numbers, sizes,
  timecodes and identifiers in monospace. Read-only values are
  left-aligned (the React Preview tab's right alignment goes), so label and
  value edges line up across tabs.
- **Trailing lane**, `metrics::inspector_trailing_width()` = 1.25rem: the
  existing state icons (keyframe diamond, lock), the remove button on
  hover, and the props file's reset, now an icon (lucide `RotateCcw`,
  tooltip "Reset to default"). Empty when a row has none, so lanes stay
  aligned.
- Height at least `list_row_height()`. A multi-line value (text, a long
  path) grows the row and the label sits at its top.
- No hairline between rows; sections keep theirs.
- Sub-lines (Set at file:line, the lock reason, the keyframe jumps, the
  input error, Saving…) start at the value lane's edge, `text_sm`, muted,
  errors in `danger`, as today.
- An input sub-row keeps its one-step (0.75rem) indent inside the label
  lane; its value lane stays on the others' edge.

## `ScrubValue`

The value of every number row (layer rows and props-file rows). Text,
color, select, boolean and path rows keep their gpui-kit controls.

- **At rest:** the number in `text_base` monospace, then the unit, muted,
  after one space (`120 px`, `45 °`); no unit, the number only.
  Decimals: none for an integer field; as many as the field's `step` has
  (0.01 → 2); otherwise as today (`format_component_number`). The display
  rounds; editing shows the stored value. Hover lifts the background one
  step (`secondary_hover`) and shows the left-right resize cursor. Text
  stays `foreground` (`primary` marks selection and focus).
- **Drag:** a press becomes a drag after 3px of movement; less is a click.
  The value follows `element_rows::scrub_value` (the field's step per
  pixel, Shift ×10, clamped and rounded as the field says). Escape before
  release puts the value back without writing; a release with the value
  where it started writes nothing.
  - Layer rows go through the existing `Scrub`: the preview follows during
    the drag and the source is written once on release.
  - Props-file rows show the dragged value in the row and write once on
    release through `commit_property`; the composition reloads then. The
    preview does not follow during the drag (props reach the runtime only
    through the file).
- **Typing:** a click without a drag swaps in gpui-kit's `Input`, focused,
  its text selected. Enter or blur commits and Escape restores, as each
  row's commit flow does today. A number followed by the field's own unit
  (`120px`, `120 px`) is read without it; anything else that is not a
  number gets today's error.
- **Keyboard:** Tab focuses it (P7a's thin inner focus line); Enter starts
  typing; Escape while typing returns to the resting display.
- **Locked:** muted, not draggable, not clickable, with today's lock icon
  and reason. Between keyframes and `--props` keys behave the same. No drag
  while playing, as today.

## Units

- TypeScript: `unit?: string` on the `number` variant of
  `ComponentPropertyField` (`packages/react/src/registry.ts`) and on
  `num()`'s options (`packages/react/src/property-tables.ts`), omitted from
  the output when unset, as `step` is. Display only: never converted or
  checked.
- Rust: `unit: Option<String>` (`#[serde(default)]`) on `Number` in both
  `ComponentPropertyField` and `PrimitivePropertyField`
  (`crates/react-bridge/src/types.rs`). A schema without it still loads.
- Built-in units: `px` for x, y, width, height, maxWidth, fontSize,
  cornerRadius, strokeWidth; `°` for rotation (the renderer reads degrees);
  `s` for startFrom where its documentation says seconds (checked per
  component during implementation). None for scale, anchorX/Y, opacity,
  fontWeight, playbackRate, volume: ratios and multipliers stay unitless,
  and nothing is shown as a percentage.

## Migration

- **Inspector tab:** Canvas, Frame rate, Duration and React entry go into a
  new "Composition" section. The Asset, Track, Clip, Dialogue, Component
  and Project properties sections move onto `InspectorSection` and
  `InspectorRow`. The schema problem line stays.
- **React Preview tab:** Size, Frame rate, Duration, Renderer and Values
  from go into "Composition"; Entry is a row whose long path wraps. The
  Reload button and the warning box stay. Chapters get the new section
  header; their rows stay.
- **Properties tab:** the pinned scope control and impact line use
  `inspector_label_width()` instead of `LABEL_WIDTH`. The layer header
  stays. The layer's sections (Transform, the table's groups, Component ·
  Name) and the Layer section (ID, Called from) move onto the new pieces;
  numbers use `ScrubValue`. The props file's "Properties" becomes a section
  with its spinner in the trailing slot, its rows one line. "Select a layer
  in the preview" stays as a muted note in the Layer section.
- **Removed:** `widgets::inspector_row`, `inspector_section`,
  `inspector_note`, `layer_panel::LABEL_WIDTH`, the React Preview tab's
  `row` closure, and `property_row`'s label-over-control layout.

## Testing

Rust (`cargo test`):

- `metrics`: `inspector_label_width()` is wider than `list_row_height()`;
  `inspector_trailing_width()` fits an xsmall icon button.
- Pure helpers: the display text (integer, step decimals, no step, unit
  after one space); unit-aware parsing (`120px`, `120 px`, `120` → 120;
  `120 em` → error); the 3px drag threshold.
- Bridge: a `Number` without `unit` deserializes; one with `unit` keeps it.
- UI integration tests (`VisualTestContext`, `debug_selector`, as in P7a):
  - a header click hides the section's rows and a second click shows them;
    Space toggles a focused header; `properties/transform` stays collapsed
    after selecting another layer;
  - the value lane starts at the same x in a plain row, a row with a
    trailing icon and an indented input row;
  - a click on `ScrubValue` opens typing and Escape restores; a drag on a
    props-file row writes once, on release; a press on a layer row's value
    starts the existing scrub.

TypeScript (Vitest): `num()` keeps `unit` and omits it when unset.

## Verification

- Before and after screenshots of the same scenes (AGENTS.md):
  `examples/greenscreen`, 1440×900, `screencapture -x -o -l <window id>`;
  the React Preview tab, and the Properties tab with a layer selected (by a
  click in the preview).
- Zoom check: `BASE_FONT_SIZE` temporarily 12 and 16; "Corner radius" fits
  the label lane and the value lane keeps its edge.

## Documentation

- The user guide, en and ja, `docs/01-(getting-started)/03-preview.md`:
  scrubbing moves from the label to the value as well (drag the value or
  its label; click to type), props-file rows can be dragged, sections
  collapse, and `unit` joins the number field's options in the
  `registerComponent()` schema.
- The Celesta Agent Skill, `skills/celesta/`: `unit` wherever a number
  field's `min`, `max` and `step` are listed (`references/project-data.md`,
  `references/react-core.md`, and `scripts/inspect.mjs` if it prints the
  schema).
- `docs/handoff/2026-11-10-p7b-inspector.md`.
