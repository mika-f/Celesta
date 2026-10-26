# Edit scope (2026-10-26)

When the selected element is shared (written inside a called component, more
than one instance), the Properties tab's impact line becomes a control:
"Edits change" [This scene] [Shared value], with a line naming the scenes the
shared value reaches and those that set their own. "This scene" writes at the
scene's call of the component, lifting a literal into a new prop of the
component (today's value kept as its default) when needed; "Shared value"
writes the value every scene uses unless it sets its own: the literal at the
element, or the parameter's default. Every edit kind honors the choice:
Properties rows, scrubs, removal, canvas drags, and inline text.
Design: `docs/superpowers/specs/2026-10-23-editor-edit-scope-design.md`
(part 3 of `docs/superpowers/specs/2026-10-23-editor-element-editing-overview-design.md`).
It builds on design 1 (`docs/handoff/2026-10-23-selection-impact.md`: the
occurrence index, the impact line, the timeline bands) and design 2
(`docs/handoff/2026-10-23-property-editing-engine.md`,
`docs/handoff/2026-10-24-property-sections.md`,
`docs/handoff/2026-10-24-canvas-tools.md`,
`docs/handoff/2026-10-25-inline-text.md`: pass-through and the owner call
site rule, `EditValue`, multi-file history, `commit_element`, `HeldCommits`).
Plan: `docs/superpowers/plans/2026-10-26-editor-edit-scope.md`.
User docs: `packages/website/content/{en,ja}/docs/01-(getting-started)/03-preview.md`
("Edit one scene or every scene"; "See what an edit changes" and "Edit
properties" point to it).

## What changed

- Source edits (`crates/source-edit/src/{apply,lift,pass_through,scope,lib}.rs`):
  `Edit::Splice { start, end, text }` writes verbatim text (identifiers,
  pattern entries, type fields, JSX), with the same sorting and overlap checks
  as the other edits. `parameter_default(text, element, param) ->
  Option<DefaultTarget>` says where a parameter's default is written:
  `Literal { span, value, form }`, `Absent { insert_at }` (right after the
  binding), `Expression`, or `NoPattern` (`props.x`). `plan_lift(text,
  element, props) -> Result<Lift, LiftLock>` plans moving the rows' values at
  `E` into new parameters of `C`: `C` is the innermost enclosing function a
  call site can name (its id, a `const` declarator, through `memo` or
  `forwardRef`), so `E` inside `.map((rectWidth) => …)` lifts to the outer
  component. The props type is the parameter's annotation, else the
  declarator's `FC<P>` / `React.FC<P>` / `FunctionComponent<P>` argument,
  else the `memo<P>` / `forwardRef<R, P>` argument, resolved against same-file
  `interface` and `type` declarations (a type literal, or the type literal of
  an intersection). Taken names are every declaration and read in the file,
  the pattern's keys, and the props type's members, including those reached
  through `extends` and intersection bases in the same file; names are chosen
  in props order (`rectX`, `rectY` together). `lift_name(tag, prop, taken)`
  builds `rectWidth`, then `rectWidth2`, `rectWidth3`. Locks: `NoComponent`,
  `NotObjectPattern`, `ImportedPropsType`, `PropsTypeNotEditable`,
  `NotALiteral` (a keyframe, a pass-through, a locked target, a `style.*`
  prop, a literal outside the component). A bare attribute's `={name}` edit is
  ordered before a `Missing` insert at the same offset, so the output parses.
  `expression` is public (it writes a default). `plan_lift_avoiding(text,
  element, props, also_taken)` is `plan_lift` with more taken names: a commit
  passes the attributes `S` already writes, so a name forwarded through
  `...rest` is not written twice.
- Scope rules (`crates/editor/src/scope.rs`, new, pure): `Scope { ThisScene,
  Shared }`; `ScopeLock` and its `text()`; `SceneCalls` (each instance's call
  of `C` and the attributes it writes, keyed by element and index
  generation) with `calls_of`, `read_scene_calls` (the background body) and
  `current_calls` (generation, element, and every call position checked
  against the current report); `scope_state` → `ScopeState { NotShared,
  Unknown { note }, Options(ScopeOptions) }`; `chosen_scope`, `commit_scope`,
  `previewed_scope`, `scope_line`; `scoped_row` → `ScopedRow { AsResolved,
  Lift, Default { shown }, Locked(lock) }`, the one per-row rule, with
  `ScopedRow::removes_as_resolved`; `scoped_removable`; `gesture_lock`; `commit_gate`, `drain_gate` (a row
  commit's gate), `gesture_gate` and `held_gate` (a held commit's, by its
  `HeldOrigin`) → `CommitGate { Go(scope), Hold(note),
  Refuse(text) }`; `gesture_start_refusal`; `impact_filter`;
  `scope_in_effect`; `remove_tooltip`; `ScopeChoices { get, set, carry,
  forget_in, clear }`.
- Targets (`crates/editor/src/element_edit.rs`, `element_edit/resolve.rs`):
  `ElementTargets` gains `enclosing` (`C`, set even when no call is found, so
  "Can't find where …" is reachable), `defaults` (`parameter_default` per
  distinct pass-through parameter of the rows and inputs), `lift_lock` (a dry
  `plan_lift` over the rows a lift would take; skipped when none would), and
  `outside` (the files outside the project). `resolve_rows` fills them, so a
  commit's re-resolve does too.
- Commits (`element_edit/commit.rs`): `commit_scoped(texts, expected,
  resolve_again, scope, calls, values)` checks each value against `expected`
  (under a scope: a `Locked` row is refused with its lock, except the removal
  of a `SetsOwn` row, which goes at `S` as resolved; `Lift` and `Default` are
  accepted even when the target at `S` is locked, removal goes through
  `scoped_removable`), re-resolves, re-applies the outside-project
  lock, runs the same-place check, then writes as resolved (`scope: None`) or
  through `scoped_edits`: one `plan_lift` for every `Lift` row plus an
  `Edit::Insert` per new parameter at `S`'s end of attributes, a `Replace` of a
  literal default or a ` = value` `Splice` after the bound name for `Default`.
  `commit_files` wraps it with `resolve_rows`. `CommitError::Scope(ScopeLock)`.
- The view (`element_edit.rs`, `element_rows.rs`, `transform_drag.rs`,
  `inline_text_view.rs`, `occurrences.rs`, `view.rs`, `refresh.rs`,
  `background.rs`, `viewer.rs`, `render.rs`): `sync_scene_calls` runs the
  scene-calls job beside `sync_element_targets`; `ScopeCache` keeps the shown
  scope state per (resolution, occurrence generation, scene calls version,
  which counts up whenever `scene_calls` is set);
  `shown_scope_context` (what the rows show, current or not) and
  `scope_context` (current targets only, for the gates); `kept_scope`,
  `commit_gate_for` (through `drain_gate`), `gesture_gate_for`,
  `gesture_refusals`, `row_removable`. `commit_element` takes the scope.
  `HeldCommit.scope`, `HeldCommit.scanning`, `HeldCommit.gesture` and
  `ElementRow.held_note`
  ("Scanning scenes…" in place of "Saving…"); `commit_row_with` gates each
  commit (Go writes, Hold holds, Refuse fails as a write failure);
  `drain_held_rows` stops while the oldest held commit's gate says Hold,
  marking it `scanning`, unless a selection change waits: then it drops it.
  `select_layer` drops the selected layer's `scanning` commits
  (`HeldCommits::take_scanning`) with "Not saved: the scenes were still being
  scanned" in `inline_notice`.
  `poll_occurrence_results` sets `occurrence_landed` on any change (also on a
  failed sweep and when Select layers clears the occurrences), and
  `settle_row_requests` drains held rows on it. The drag, the scrub
  (`Scrub.scope`) and the inline text session (`InlineTextSession.scope`)
  capture `kept_scope` at start, and start only when `gesture_refusals` (the
  gesture gate under that scope) has nothing against them; `end_transform_drag`
  and the scrub's and inline text's commits go through `gesture_gate`.
  `transform_state` drops the drag props it refuses and, through
  `without_refused`, the size handles when a size prop is refused (all gone:
  the Locked state with the text). A refused inline commit ended by Enter,
  blur or a press outside keeps the session open with the refusal in
  `inline_notice` (`inline_commit_refusal`, which takes a hold as a refusal
  too). A kept reload stores the selected element's old position; the first
  `LookupFate::Show` calls `ScopeChoices::carry` to the new chain head. Own
  writes collect the files they changed in `scope_written`; the reload that
  reads them calls `ScopeChoices::forget_in` with the carried position, and a
  reload of the same composition that is not an own write (a change outside
  the editor) calls `ScopeChoices::clear`.
- The control (`layer_panel.rs`, `impact.rs`, `element_rows_view.rs`,
  `element_rows.rs`): `impact_line` shows the control for `Options`
  ("Edits change", two tabs in the tool row's toggle look, `scope_line` under
  them, in the warning color when no option is in effect), "Edits change" and
  the note for `Unknown`, design 1's line for `NotShared`. A locked tab is
  dimmed, has no click and no hover preview, and its tooltip is the lock
  text. `ImpactFilter { All, Through(call), Except(calls) }` replaces
  `written_at` in `impact_view`; `selection_impact` filters by the hovered or
  chosen option. Rows read `scoped_row`: a `Locked` row shows the lock icon
  and text and doesn't scrub; a `Default` row shows the default
  (`default_row_text`) and has no "Set at" line; a `SetsOwn` row shows as
  resolved, with its "Set at" line. The remove button's tooltip comes from
  `remove_tooltip`. The drag label's second line and the selection header's
  line under the title use `scope_in_effect`. `properties_panel`
  (`panels.rs`) draws `impact_line`'s box above the `properties-scroll` area,
  so it stays at the top of the tab; the selection header scrolls with the
  sections.

## Decisions

These settle what the spec left open, or differ from it; the spec has been
amended to match.

- **One per-row rule.** `scoped_row` decides what a row writes under a
  scope; the row's shown value, its locks, its remove button and the commit
  all read it, so what the panel shows is what gets written.
- **The scope applies at commit time; confinement is the caller's.**
  `commit_element` asks `commit_gate` (state from the index, the remembered
  choice) and passes the scope to `commit_scoped`, which enforces only what
  the text tells (`scoped_row`'s locks, the lift's locks). A remembered "This
  scene" that a new lock disables is refused with the lock's text, never
  widened; the control then shows neither option chosen.
- **When the index can't answer**, only `Scanning`, or a `Refreshing` index
  that no longer finds `E`, holds a commit; a failed scan, a `Ready` index
  missing `E` ("Not found in the scene scan"), and no sweep at all ("The
  scenes are not scanned") refuse, since no sweep would drain the commit.
  Values written at `S` as resolved (a pass-through unless "Shared value" is
  remembered, a Component-section row) go as before. The hold arm never
  writes unscoped. A row commit held with no option shown is written when the
  sweep lands only if the option then in effect is "This scene" (a lift
  confined to the scene); when "This scene" is locked by then, it is refused
  with "The scenes were scanned while editing; edit again" rather than
  written to the shared value (`drain_gate`).
- **A gesture is never held.** A drag, a scrub or an inline text edit
  released while the scenes are scanned is refused with the scan's note
  (`gesture_gate` turns a hold into a refusal); an inline edit stays open
  with it. Only commits held for a reload (another write running, the
  targets being read again) still wait as before; a scrub's or an inline
  edit's (`HeldCommit.gesture`) then drains through `gesture_gate` under the
  scope it captured (`held_gate`), so it meets the same locks as at release.
- **A reload-held commit says what changed.** One held for a reload with no
  option shown and refused at the drain says "The scenes changed while
  editing; edit again" (`CHANGED_MEANWHILE`), not the scan's text.
- **A selection change doesn't wait for the scan.** Selecting another layer,
  or leaving the interactive mode, drops the commits held for the scan with
  "Not saved: the scenes were still being scanned" in the viewer's notice,
  which stays through the change; commits held for a reload are still waited
  for.
- **A gesture starts only when its release would be written.** The drag,
  the scrub and the inline edit check, at start, the gate their release
  would pass, under the scope they will keep (`kept_scope`), not the one the
  control shows in effect: a locked remembered choice, a "sets its own" row,
  or a literal while the scenes are scanned stops them before they start,
  with the refusal text (the drag's Locked state, the row's message, the
  double-click's note). The size handles are not offered when a size prop
  is refused.
- **Scene calls are read in the background**, keyed by element, index
  generation and call positions; a result counts only for the index and calls
  it was read for.
- **"Sets its own" is per element on the line, per row on the row.** The
  line names a scene whose call writes any parameter `E`'s rows read; under
  "Shared value", the row whose parameter this scene passes (an expression
  too) is `Locked(SetsOwn)` in `scoped_row`: it shows this scene's value, the
  one the canvas draws, keeps its "Set at" line, takes no commit, and drag,
  scrub and inline text lock with the same text ("This scene sets its own
  `rectWidth`"), like a settings screen that marks a value as set elsewhere
  and edits it there. Writing the default from that row would change other
  scenes while the scene being looked at stayed as it was. Its remove button
  ("Use the shared value") deletes the attribute at `S`; the row then shows
  and edits the default. A lock of the default itself (`EverySceneSets`, an
  expression, no pattern, not found) takes precedence, since removing the
  value at `S` would not make those editable. Through `row_lock`, "Shared
  value" is disabled with this text when every row in its scope is one this
  scene sets.
- **"Shared value" on a parameter** writes the default's span; a parameter
  without a default that some call leaves out gets one; without a default and
  passed everywhere, the row locks ("Every scene sets `rectWidth` itself"); an
  expression default locks ("`rectWidth`'s shared value is written as an
  expression"); `props.x` locks with the not-an-object-pattern text; a
  parameter with no `defaults` entry locks with "Can't find where
  `rectWidth`'s shared value is written" (its own lock, not a scan note).
- **Removal follows the scope**: the existing remove button is "Use the
  shared value" on a row passed at `S`, under "This scene" and under "Shared
  value" alike; a literal at `E` has no button under "This scene"; a default
  has none under "Shared value". Component-section rows are outside the
  scope.
- **The scope control stays in view.** The box `impact_line` returns (the
  control, its scanning note, or design 1's line) is a non-scrolling child
  above the Properties panel's scroll area, so it can be seen and switched
  while editing rows further down, in every state alike. Pinning it right
  under the selection header would have taken the header out of the scroll
  area too; the header keeps scrolling instead, so the box now sits above it,
  at the top of the tab. A hover that changes the header's "Shared" row no
  longer moves the tabs either way.
- **Lock order** for "This scene": no call; a component entry after `S`;
  a reused chain (only with a `Scenes` wrapper after `S`); other scenes
  through `S`; another call in this scene; then row locks, which disable the
  option only when no row can be written. A literal input locks with "Can't
  be set per scene yet"; a lift or a default writing a file outside the
  project locks with "This file is outside the project"
  (`ScopeLock::OutsideProject`). `LiftLock::NoComponent` shows design 2's
  "Can't find where `<C>` is called".
- **The line's precedence**: a locked option shows its reason (the chosen
  one's first); with "This scene" locked the reach line is not shown, since
  the reason is what blocks the user. Texts the spec didn't give ("Shared
  value is used everywhere" / "in no scene", "X, Y set their own", more than
  three scenes counted as "N scenes", the props-type, outside-project,
  default-not-found, scan and gesture texts) were added to it.
- **The memory survives a lift**: `ScopeChoices::carry` moves the choice to
  the element's new position on the kept reload. Choices are kept by
  position, so the reload of an own write forgets every other choice made in
  the files it wrote (a lift's added lines may move a sibling onto a
  remembered position), and a change from outside the editor forgets them
  all; forgetting falls back to the default (`chosen_scope` with nothing
  remembered: "This scene" unless it is locked).
- **A shared edit's impact is conservative.** Under "Shared value",
  `impact_filter` leaves out the scenes that set their own (`Except`) only
  when every row it writes passes through to a parameter; a value written
  on the element changes every instance (`All`).
- **The element is not shared, nothing is kept.** `commit_scope` returns
  `None` for `NotShared` whatever was remembered, so a stale choice never
  locks a gesture on an element without the control.
- **What the panel shows is what gets written.** The shown options are
  computed at the frame the shown targets were resolved for
  (`resolved.timeline_frame`), not at the playhead, so a seek doesn't show a
  scope computed from stale targets.
- **A commit writes with the scope the user saw.** A held commit stores the
  remembered choice, else the option the control showed then
  (`commit_scope`); a drag, a scrub and an inline text edit capture it when
  they start and are gated by `gesture_gate`: a lock under the captured
  scope refuses with its text, never switching options; one that started with
  no option shown is refused once the options are known ("The scenes were
  scanned while editing; edit again").
- **A refused inline edit stays open** when the person ended it (Enter, blur,
  a press outside); a selection change, a seek, play, and a project command
  can't wait on an open edit, so they end it.
- **The lift** treats a single-use `const` inside `C` as the value (the
  `const` takes the new prop); a literal outside `C`'s function and a
  `style.*` prop are `NotALiteral`; `FC` without a type argument is
  `PropsTypeNotEditable`.
- `ScopeLock::Unknown` was dropped: the index notes live in
  `ScopeState::Unknown` as strings.

## Known gaps

- Scenes written as components and placed by hand
  (`<Sequence><PriceScene /></Sequence>`) lock "This scene" as "Passes through
  `<PriceScene>`": the scene's own component entry sits after `S`.
- Several calls of `C` in one scene lock "This scene" ("`<PriceTag>` is
  called more than once in this scene") instead of writing each call.
- A refused inline text edit ended by a selection change, a seek, play, or a
  project command loses its text. It needs a scan landing or a lock appearing
  during the edit. After a refused blur the person may have to click back into
  the text, and the scope is captured again at the blur, before a clicked tab
  applies.
- A held inline text commit refused at the drain can't reopen the edit: the
  text reverts, and the refusal shows at the top-left of the preview.
- Until the background job reads the scene calls, the line leaves out the
  scenes that set their own, and "Every scene sets `rectWidth` itself" can't
  fire yet.
- A row passed at the call is written at `S` while the index can't answer,
  as without the choice.
- Anonymous and lowercase components count as not shared (no control).
- With every row passing through, the timeline bands under "Shared value"
  are approximate: `Except` drops the calls that pass any parameter the
  element's rows read, per element, not per row being edited.
- `row_removable` falls back to plain removal when the targets aren't
  current: during an own write's reload the remove button shows for a literal
  under "This scene" (the commit refuses it).
- `current_calls` accepts an instance whose call couldn't be read (`None`)
  and doesn't retry until the index generation changes.
- With the Size tool chosen and the size handles hidden by the scope, the
  toggle's warning (`resize_note`) says nothing about why.
- An edit outside the editor (and the manual Reload) forgets every scope
  choice, not only those in the files it changed: the reload watcher knows
  only that some file did. Loading another composition forgets none.
- The dry lift can lock every row as "Can't be set per scene yet" when a
  `const` in an outer function around `C` is read.
- A lifted pattern entry is always written inline (`, rectWidth = 200`), even
  in a multi-line pattern; only type fields copy the newline and indent.
  `PropsTypeNotEditable` names the first lifted prop when several lift.
- `TextOverride` is by layer id: a live inline edit under "Shared value"
  shows on the edited layer only until the commit's reload.
- A `Default` row without a default shows an empty field with the element's
  default dimmed.
- A pass-through whose value at `S` is locked for another reason than an
  expression (`OverriddenByParent`, say) counts as this scene's own value
  under "Shared value" too: anything but an absent attribute at `S` does.
- The scope tabs copy the tool row's toggle look (`mode_toggle` in
  `viewer.rs` is private, with fixed 58 px tabs and no disabled or preview
  state); one tabs widget shared by the scope control and the tool row is
  still to be extracted.
- The drag label's second line relies on `\n` inside a `whitespace_nowrap`
  div; check it on screen.
- Cost: for an element inside a component, `resolve_rows` adds a parse for
  `enclosing`, one per pass-through parameter, and one dry lift, also on
  every commit's re-resolve; `row_removable` recomputes `scope_state` per row
  per render, and `transform_state` once per call.
- Untested: the sweep-landed drain flag (no pure test; a single flag);
  `remembered_scope` reading the lookup answer; a commit with real
  `SceneCalls` refused by `EverySceneSets`; a pass-through and a lift in one
  commit; `HeldCommits::first`; the lift's guard for a module-level `const`;
  `FunctionComponent<P>`, `React.memo`, `React.forwardRef`; mismatched scene
  calls ignored; lifting children; "This scene" on a pass-through left out at
  `S`; a chapter gap.
- Code nits: component detection is written twice (the lift's finder and
  `Scopes`) and must stay in agreement; `Scopes` has a `names` field and a
  `names()` method; `Scene::at` duplicates `impact_view`'s current scene;
  `read_params` duplicates the passed-parameter walk;
  `scoped_row_follows_the_table` is one long test; the `row_lock` and
  `lift_lock` docs are slightly stale.

## What design 4 builds on

- `commit_scoped(texts, expected, resolve_again, scope, calls, values)` is
  generic over the element it writes: design 4's timing windows build
  `ElementTargets` for a `Sequence` (`PropKey::Prop("from" |
  "durationInFrames" | "offset")`, `component` = the enclosing component's
  call, `enclosing` = that component) and pass their own `resolve_again`.
  Hand-built targets must set `outside` (as `lock_outside_project` does).
  `commit_scoped` is re-exported only under `cfg(test)` in
  `element_edit.rs`; lift that when design 4 calls it.
- `SceneCalls`, `commit_gate` and `gesture_gate` decide the scope for any
  element; `Edit::Splice` writes verbatim text.

## Verify by hand

In the editor, with **Select layers** on, on a project with a `PriceTag`
component (a `<Rect width={200} />` inside, a local `interface Props`) called
in an Intro scene and a Price scene. Take the before and after screenshots of
the same scene for each of the first three checks, for the pull request.

- Select the `<Rect>` in Price: "Edits change" shows [This scene] raised and
  "Shared value is used in Intro, Price". Set its width to 260: only Price
  changes; Intro is unchanged; the diff is the spec's (`{ label, rectWidth =
  200 }`, `width={rectWidth}`, `rectWidth?: number;` in `Props`, `<PriceTag
  label="Sale" rectWidth={260} />`); one undo restores both files.
- Then choose [Shared value]. The line reads "Shared value is used in Intro ·
  Price sets its own"; the Width row shows 260 (what the canvas draws) with
  its "Set at" line naming Price's call, is locked with "This scene sets its
  own `rectWidth`", and doesn't take typing; scrubbing the Width label is
  locked with that text; the canvas offers no size handles for the `<Rect>`.
  Hover the row: its remove button's tooltip reads "Use the shared value";
  click it: `rectWidth={260}` goes from Price's call, Price draws 200, and the
  row shows 200, unlocked. Set the width to 240: `rectWidth = 240` in the
  pattern, and Intro and Price both change.
- Scroll the Properties tab down to the last rows: "Edits change" and the
  line under it stay at the top of the tab while the header and the rows
  scroll beneath them; a layer that is not shared keeps its impact line there
  too.
- Select a background drawn by a component in every scene, choose [Shared
  value], and edit its fill: every scene changes. Select it again later: the
  choice is remembered.
- Hover each tab: the timeline bands preview the instances it would change,
  and go back to the chosen option's on leave. A locked tab is dimmed, its
  tooltip gives the reason, and a click does nothing.
- Drag the layer: the drag label shows a second line, "Edits change this
  scene" or "Edits change the shared value", and the header shows the same
  under the title.
- With [This scene] chosen in Price, move the playhead to a scene whose
  `<PriceTag>` call sits inside `.map()` (it renders other scenes too) and
  select the `<Rect>` there: neither tab is raised, the line is
  "`<PriceTag>` is called here for other scenes too" in the warning color, a
  commit is refused with it until you pick [Shared value], and a drag of the
  layer does not start (the canvas shows the reason).
- Add a second `<PriceTag>` to `Price.tsx` in an external editor and select
  the `<Rect>` again: the outside change forgot the choices, [This scene] is
  disabled ("`<PriceTag>` is called more than once in this scene"), and
  [Shared value] is raised.
- Choose [Shared value] for one element of `PriceTag`, then make a [This
  scene] edit of another element of it that lifts a value (the new field in
  `interface Props` moves both elements down a line): the edited element
  keeps its choice; the first one shows [This scene] again.
- Under [This scene], hover the remove button of the Width row passed at
  Price's call: its tooltip reads "Use the shared value", and clicking it
  removes `rectWidth={260}` from the call. A value written on the `<Rect>`
  itself has no remove button under [This scene].
- With `{ label, rectWidth }` (no default) and Intro leaving the prop out,
  choose [Shared value] and set 240: the parameter becomes `{ label,
  rectWidth = 240 }`.
- A move under [This scene] of a `<Rect>` without `x` and `y` in a component
  taking `({ label, ...rest })`: one change adds `rectX = 0, rectY = 0` before
  `...rest`, `x={rectX} y={rectY}` on the `<Rect>`, and both attributes at
  the call; one undo restores both files.
- Right after a "This scene" edit that lifts a value of a component with a
  local `interface Props` (the new field moves the element down a line),
  commit another literal row of the same element while "Edits change" says
  "Scanning scenes…": the row says "Scanning scenes…" and the value is
  written when the scan lands. A drag of a literal prop in that time does not
  start (the canvas shows the note); a row passed at the call is written at
  once.
- In the same window, commit a literal row and click another layer before
  the scan lands: the selection changes at once, the value is not written,
  and "Not saved: the scenes were still being scanned" shows at the top-left
  of the preview.
- Turn **Select layers** on and double-click a shared `<Text>` whose text is
  written on it while "Scanning scenes…" shows: no edit begins, and the note
  says "Scanning scenes…". For a `<Text>` whose text is passed at the call
  (`<Text>{label}</Text>`), the edit begins; press Enter after the scan
  finished: the edit stays open with "The scenes were scanned while editing;
  edit again" at the top-left; press Enter again to write it.
- Undo and redo a lift: the component's file and the call's file change
  together.
