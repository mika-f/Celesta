# Editor toasts: a visible edge and a centered icon (2026-11-11)

## Problem

In the P7d hand check (1440×900, 13px `rem`) the "Export finished" toast
looked broken:

- No visible edge. It melted into the panels it covered: the timeline's
  scrollbar ran into it, and the "%" of the mixer's "100%" showed past its
  right edge.
- A title-only toast's icon sat below its title instead of on its line.

P7a's spec (`docs/superpowers/specs/2026-11-10-p7a-visual-foundation-design.md`
§ Toasts) asks for the popover surface, radius 0, a hairline border and a
shadow, at the bottom right above the status bar.

## Cause

All of it is gpui-kit 0.9's `Notification` (`gpui-component`
`src/notification.rs`), not the theme or the editor:

- The surface is `border_1` in `theme.border` over `tokens.popover`. In the
  editor's palette those are `#2e2e2e` over `#2a2a2a`: the border is one
  step from the fill, so the edge is invisible.
- The shadow is `toast_shadow`, black at 10%. It is invisible on the
  editor's near-black greys, and the toast's enter animation sets it on
  every frame (GPUI applies an animator after the animation ends too), so a
  `Styled` override on the notification cannot replace it.
- A typed notification's icon is `absolute().top(px(18.))` beside the text
  column. The 18px is designed for gpui-kit's 16px `rem`; at 13px the title
  line's center is higher, so the icon sits low.
- The action button is a sibling of the text column, so a long title and
  message wrap into a narrow column beside it (the P7a handoff note saw this
  on the error toast).
- The toast's right margin (16px) is wider than a panel's content inset
  (0.75rem), so a right-aligned value in the panel under it shows a sliver
  past the toast's edge.

## Change

- `crates/editor/src/toast.rs`: `Toast` builds every editor toast.
  - `Toast::new(kind, title)`, `.message(...)`, `.action(label, on_click)`,
    `.build(cx) -> Notification`; `Toast::note(message, cx)` for a
    one-sentence info toast with no title. `ToastKind` is `Info`, `Success`,
    `Warning`, `Error` with gpui-kit's lucide icons and status colors.
  - The body is the notification's `content`, not its `title`, `message`,
    `with_type` or `action`: the icon sits in a row as tall as the title's
    line (1.25rem), so it centers on that line whatever the `rem`; the
    message (`text_sm`, muted) and the action (an xsmall outline button,
    `compact_label`) are indented under the title. A toast with an action
    sets `autohide(false)` itself, as gpui-kit's `action` did.
  - The edge is `border_color(foreground at 10%)`, the ink gpui-kit gives the
    hairline ring of its menus and popovers (`popover_ring`), so toasts and
    the other popups share one edge.
  - gpui-kit's own shadow and close button stay. The first row keeps `pr_4`
    clear for the close button shown on hover.
- `problems_panel.rs` (`announce_problems`) builds its toasts with `Toast`.
  The three plain-string toasts (`lifecycle.rs` ×2, `plugins_panel.rs`) are
  `Toast::note`, so they get the info icon and the same edge.
- `celesta-editor-theme`: `notification.margins.right` = 0.75rem (13px ×
  0.75), a panel's content inset, so the toast covers a right-aligned value
  in the panel below whole.

## Checks

- `cargo test -p celesta-editor` (new UI integration tests in `toast.rs`:
  a title-only toast's icon row is level with its one-line title and one
  line tall, so the icon centers on it; the message
  aligns with the title and sits under it; an action takes its own row
  under the message, the title stays on one line, and the toast is still
  there after 10s; a note's icon sits on its first line; a toast without an
  action autohides) and `cargo test -p celesta-editor-theme` (the right
  margin).
- `cargo clippy -p celesta-editor -p celesta-editor-theme --all-targets --
  -D warnings`.
- Screenshots of the same scene before and after (attached to the PR, not
  committed): `examples/greenscreen/composite.tsx` at 1440×900, with a
  temporary, uncommitted hook in `announce_problems` that raises one toast
  of each kind (and all of them stacked) eight seconds after the first
  render, through the same paths as the real toasts.
  - To skip the "Trust this project?" dialog without touching the user's
    settings, the editor ran with `HOME` set to a scratch folder whose
    `Library/Application Support/Celesta/settings.json` lists the example
    folder in `trustedProjects`.
  - GPUI stops drawing a window that other windows cover, so
    `screencapture -l` of a covered editor returns a stale frame. The
    window was brought to the front (System Events `frontmost`) just for
    the captures.
  - A fresh worktree needed `turbo run build --filter
    "@celesta.studio/cli..." --force` (a cache hit had not restored
    `packages/react/dist`), as the P7a note says.

## Follow-up

- The shadow cannot be changed from the editor. A gpui-kit option to scale
  the toast's shadow ink (for dark themes) or to keep a `Styled` shadow
  through the enter animation would let the editor match the spec's
  shadow; until then the edge carries the lift.
