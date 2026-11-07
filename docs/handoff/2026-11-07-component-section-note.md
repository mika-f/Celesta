# Component section: "this call only" note (2026-11-07)

Issue #564. For a shared element, the Properties panel shows "Edits change"
[This scene] [Shared value] at the top. The **Component · `<Tag>`** section
below it always writes the component's call (`PropKey::CallSite` rows are
outside the scope: `commit_gate`, `scoped_row`), but nothing on screen said
so; under "Shared value" its rows read as if they changed every scene.

## Specification

- While the panel shows "Edits change" (the scope state is `Options`, or
  `Unknown` while whether the element is shared is not known yet: the
  scenes are being scanned, are not scanned, the scan failed, or the index
  does not find the element), the Component
  section shows, under its "Set at `<file>`:`<line>`" line, an `info` icon
  and **Writes this call only, whatever Edits change says**, muted, wrapping
  in a narrow panel.
- Without the control (`NotShared`, or no scope context), the line is not
  shown: nothing above says otherwise.
- What the rows write is unchanged.

## Implementation

- `component_section_note` (`crates/editor/src/scope.rs`) gives the text
  for a scope state, or `None`.
- `element_sections` (`crates/editor/src/element_rows_view.rs`) reads
  `shown_scope_context()`'s state and adds the line in the box under the
  section heading, after `set_at_line`. `set_at_line` and the new
  `component_note_line` share `icon_line` (the muted row with a leading
  icon, centered); the note aligns its icon to the top instead, since it
  wraps, and "Set at" still stays on one line with an ellipsis.
- User guide: the Component section paragraph in
  `packages/website/content/{en,ja}/docs/01-(getting-started)/03-preview.md`
  names the line and says the rows write the call under either option. The
  Celesta skill does not cover the editor panel, so it is untouched.

## Checks

- `cargo test -p celesta-editor`: new test
  `tests::scope::the_component_section_says_it_writes_the_call_while_edits_change_shows`.
- `cargo fmt --all -- --check`, `cargo clippy -p celesta-editor --all-targets -- -D warnings`.
- Not checked in the app: the session could not bring the editor window up
  to capture it. To check by hand: open `examples/scene-timing/film.tsx`,
  turn on interactive mode, seek into Main, select the price tag's `<Rect>`;
  the line shows under **Component · PriceTag**'s "Set at" under either tab.
