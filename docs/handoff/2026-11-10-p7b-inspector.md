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

**Tokens.** `metrics::inspector_label_width()` (7rem),
`metrics::inspector_trailing_width()` (1.25rem), and, after the user's
spacing review, `inspector_row_height()` (2rem, rows and section headers)
and `inspector_padding()` (1rem, their leading and trailing padding).
`panel_header_height()` is 2.5rem so gpui-kit's medium `Tab` (32px) fits
the band; that change came from main with P7c (#808), and this branch only
rewords its doc comment. The dock's tabs get `px_2` more each side, and
Reload composition is a medium button with 1.5rem above and below. The
shared `panel_padding()` and `list_row_height()`, which the timeline uses,
are unchanged.

**`inspector.rs`.**
- `InspectorSection`: a band header (`title_bar`, `inspector_row_height()`,
  one hairline) holding a full-width ghost `Button` with a chevron and the title;
  a trailing slot (the props file's spinner). Collapsing focuses the header
  button (its own focus handle), so a field inside blurs and submits.
- `InspectorRow`: label lane (fixed width, ellipsis, tooltip, optional
  `on_label_press`), value lane, trailing lane (always present); sub-lines
  start at the value lane's edge; `indented` (1rem, so the lanes snap to
  the same pixel) and `top_aligned`.
- `fact(id, label, value, mono, cx)`: a read-only row.
- `CollapsedSections` on `EditorView::collapsed_sections`, toggled by
  `EditorView::section(id, title, cx)`. Ids: `inspector/<name>`,
  `react/<name>`, `properties/<group lowercased>`, `properties/component`,
  `properties/properties`, `properties/layer`, `properties/called-from`.
  `replace_contents` (open, reload) carries the state to the new view.

**`scrub_value.rs`.** `ScrubValue` (resting number, then its unit muted
in its own lane so a narrow lane cuts the number, not the unit; a press
reports its x; Enter on the focused resting value asks to type; while typing
it shows the panel's escapable text input), `drag_distance` (3px threshold,
counted from the threshold), `display_number` (integer, step places, else as
written), `ScrubOrigin`, `ScrubEnd`, `RowTyping`, `refused_press` (a value
press the scope refuses opens typing; a refused label drag says why). The resting
value tracks the input's focus handle, so Tab opens a row for typing.

**Layer rows.** `Scrub` has an origin and the threshold; `finish()` decides
`Write`, `Nothing` or `Type`. A row whose bad text was put back on blur
rests with its message as a sub-line (`element_row_open`). A value press while playing, or on a row that
types but does not scrub (input rows), opens typing directly. A release
outside the window never opens typing.

**Props-file rows.** `PropertyScrub` (same helpers) shows the dragged value
and writes once on release through `commit_property`; the preview follows
after the reload. A resting row that still holds its field's focus takes a
loaded value (`replaces_text`). Reset to default is the trailing `RotateCcw` button.

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

- `cargo test -p celesta-editor -p celesta-editor-theme` (803 + 8 at
  `f1042736`; see "Review round 2" for the count after it),
  `cargo test -p celesta-react-bridge --lib`, clippy `-D warnings`,
  `cargo fmt --all --check`, the two Vitest files.
- Screenshots (in the PR, not committed): the editor from the P7a tip and
  from this branch, `examples/greenscreen/composite.tsx`, 1440×900, the React
  Preview tab and the Properties tab with the video layer selected,
  `screencapture -x -o -l <window id>`.
- Zoom: not rerun. Every size P7b adds is on the `rem` (tokens and GPUI
  helpers), so lanes and text scale together; P7a's zoom check covered the
  pixel-sized parts.

## Final review

A fresh reviewer read the whole branch. Fixed: the unit was cut with the
number and not muted; a refused value press showed the refusal as an error
in an unfocused field; a layer row with a stale message stayed a field; a
resting props-file row kept a stale value while holding the focus; the
collapse state was lost on open and reload. Deferred minors are listed in
the PR.

## Known limits

- Dragging a props-file row does not move the preview until release (props
  reach the runtime only through the file).
- Collapse state is per session, not saved.

## Review round 2 (hinaloe on `f1042736`)

Fixed, each with a test that failed first:

- Space on a focused section header started playback: the editor's
  `space` binding ran before the button's own keys. The header carries the
  `InspectorHeader` key context, and the binding is
  `CelestaEditor && !Input && !InspectorHeader`. The bindings moved into
  `key_bindings()` in `main.rs`, so
  `inspector::tests::space_toggles_a_focused_header` binds the editor's own
  keys.
- A failed props-file write after a drag, pick or reset left the row open
  as a field that no longer dragged. `RowState` remembers whether the last
  commit was typed (`TextUpdate::Keep`); only typed text stays as an edit
  for a retry, otherwise the row rests on the file's value
  (`a_failed_drag_write_rests_the_row_on_the_file_value`).
- Drags wrote float noise (`0.5700000000000001`): `scrub_value` rounds to
  the places of the step or the start, whichever has more (at most 12)
  (`a_scrub_lands_on_its_steps_places`).
- The layer panel's scope box, selection header, impact line and notes
  were padded `px_3` beside the label lane, about 3px off the rows' value
  edge: they use `inspector_padding()`, and
  `style_guard::inspector_lines_share_the_row_padding` keeps `px_3` out of
  the Inspector's files. `old_inspector_helpers_are_gone` now matches
  `LABEL_WIDTH` too.
- The window coming back to the front reopened a focused resting value:
  `RowTyping` remembers that Enter or Escape closed the row with the focus
  kept, and only a blur clears that
  (`a_resting_row_with_the_focus_opens_only_on_request`). Round 3 found
  this did not hold in a window; see there.
- Tab into a number row is pinned by
  `scrub_value::tests::tab_opens_a_resting_number_for_typing` (checked to
  fail without the resting value's `track_focus`).
- The component section's "Registered as" row and a prop called `name`
  shared the id `component-name`: the prop rows are `component-prop-<key>`.

Docs: the guide (en, ja) says props-file rows drag by their value only, ↺
removes the key, which rows show units, that the Inspector and React
Preview tabs collapse too, and Space or Enter toggles a focused header;
the ja guide calls the gesture 数値のドラッグ. The skill says the editor
enforces `min`/`max` and the runtime doesn't.

Checked: `cargo test -p celesta-editor -p celesta-editor-theme` (808 + 8),
clippy `-D warnings`, `cargo fmt --all --check`.

Not changed (suggestions, recorded on the PR): the duplicated drag and
typing helpers between the layer and props-file panels, `fact`'s bare
`mono` flag, `InspectorSection` built by hand rather than on gpui-kit's
`Collapsible`, the drag threshold compared after rounding (2.5px in
effect), a props-file drag starting from the default when the file holds
a non-number, and the extra unit-path tests.

## Review round 3 (hinaloe on `e6c59d08`)

Fixed, each with a test that failed first:

- Going to the back blurs the focused field (gpui blurs it on the
  inactive frame), and that blur cleared `RowTyping`'s resting flag, so
  the focus sent on coming back reopened the row. `RowTyping::on` takes
  whether the window is active; a blur in an inactive window leaves the
  flag. The panels pass `window.is_window_active()` from the control
  handler. `scrub_value::tests::coming_back_to_the_window_leaves_a_resting_number_resting`
  presses Enter, deactivates and re-activates the window; the unit test
  covers the inactive blur too.
- Reset to default lost its accessible name with `compact_label`; it,
  the row's Remove and the keyframe jumps set `accessibility_label`.
  `style_guard::inspector_icon_buttons_have_accessible_names` checks
  them, on a helper shared with P7c's transport guard.

Also changed: `ScrubValue::field` takes a closure that dresses an `Input`
over the value's own state, so the field is the one whose focus the
resting value tracks; `InspectorSection::trailing` is `push_trailing`
(it appends, `InspectorRow::trailing` replaces); the duplicate 32px
bound in `the_inspector_is_roomier_than_a_list` is gone (P7c's
`the_panel_header_holds_a_whole_tab` keeps it).

Known, not changed:

- A mouse collapse leaves the focus on the header, so Space toggles the
  section again rather than playing; a click on the canvas or the
  timeline gives Space back to playback.
- Collapsing a section whose field holds text that does not parse
  submits it, and the message shows in the hidden body; the header does
  not show it.
- Tests still missing: `on_label_press`, the props-file drag clamp, the
  `sub_line` lane.

Checked: `cargo test -p celesta-editor -p celesta-editor-theme` (824 +
11), clippy `-D warnings`, `cargo fmt --all --check`.

## Review round 4 (hinaloe on `2633345b`)

Fixed, each with a test that failed first:

- How a row came back to the window depended on how it was opened: a
  row reopened by a click or by Enter on the resting value kept the
  resting flag and came back closed, one opened by Tab came back open.
  `RowTyping::request` clears the flag, so a reopened row is typing
  again and comes back open
  (`a_row_reopened_by_request_comes_back_to_the_window_open`).
- The flag could stick: with the window at the back, or before the first
  frame back in front, the focus moving to another field sends this one
  neither a focus nor a blur, and a later Tab onto it left it resting.
  `RowTyping::settle` clears the flag once the field is not the window's
  focus (gpui keeps that focus while the window is at the back), and
  `EditorView::render` runs it for every row of both panels
  (`settle_resting_rows`), before the frame's focus events.
  `scrub_value::tests::a_row_left_at_the_back_opens_on_tab_once_the_focus_moved_on`
  activates the window and focuses the other field in one update. The
  first try, a check on the first frame through `on_next_frame`, could not
  be tested: gpui's test window draws without running next-frame
  callbacks.
- The window-active check had no test through the panels' wiring.
  `subscribe_text` and `text_input` are generic over the view, the active
  check moved into `RowTyping::follow(&event, window)` (the panels pass
  the window), and the `Tabbed` test view uses all of them, so
  `coming_back_after_enter_…` and `coming_back_after_escape_…` drive the
  production mapping. Making `follow` ignore the window fails both. A
  real `EditorView` in a test window needs the GPU renderer and the
  workers, so the panels' two call sites and the render call are not
  covered.

Checked: `cargo test -p celesta-editor -p celesta-editor-theme` (828 +
11), clippy `-D warnings`, `cargo fmt --all --check`.

## Rebase onto main at `2ece40f8` (P7d, #813)

`main.rs`: main added `export_page::view::key_bindings()` next to the
editor's list, which this branch had moved into `key_bindings()` (with
`!InspectorHeader` on Space); both are bound. `metrics.rs`: P7d added the
same `inspector_label_width()` (7rem) for the export page. The rebase did
not resolve this: `ce06ebb0` ("inspector lane tokens") applied without a
conflict, because the two definitions sit in different parts of the file,
so every rebased commit from `ce06ebb0` through `7c2e9e22` defines the
token twice and does not build (`E0428`). The fix is the later commit
`3d90705d`: the branch's copy stays, and P7d's
`the_inspector_label_lane_is_7rem` is dropped, as
`inspector_lanes_fit_their_contents` asserts the same. Folding the fix
into `ce06ebb0` would rewrite that commit, so it was not done; a
squash-merge, or that edit with the owner's approval, keeps the broken
commits out of `main`'s history for `git bisect`.

After this lands, the export page and the Inspector sit side by side at
different densities: the export page keeps `list_row_height` (1.75rem)
and `panel_padding` (0.75rem) in `export_page/view/layout.rs`, the
Inspector `inspector_row_height` (2rem) and `inspector_padding` (1rem).
P7d's handoff already lists merging the rows as a follow-up.

Known: `a_layer_row_rests_with_its_message_once_its_text_is_restored` and
`a_refused_value_press_opens_typing_and_a_refused_label_drag_says_why` in
`tests/element_rows.rs` restate the `match` of `element_row_open` and
`refused_press` rather than driving a row.

Checked: `cargo test -p celesta-editor -p celesta-editor-theme` (852 +
11), clippy `-D warnings`, `cargo fmt --all --check`.
