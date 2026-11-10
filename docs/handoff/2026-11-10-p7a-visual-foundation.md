# P7a: the editor's visual foundation (2026-11-10)

The first sub-project of P7, the editor's UI refinement. Spec:
`docs/superpowers/specs/2026-11-10-p7a-visual-foundation-design.md`. Plan:
`docs/superpowers/plans/2026-11-10-p7a-visual-foundation.md`.

## What changed

**Theme (`crates/editor-theme`).** `init` loads the dark theme, then
`apply` sets the editor's scale and palette in one `Theme::update`. Call
`apply` again after anything that reloads the theme (`Theme::change`
resets it).

- `font_size` 13px (the window's `rem`), `radius` and `radius_lg` 0,
  `focus_ring` off (a thin inner line instead of an outer ring), `shadow`
  left on (it drives the overlay elevations).
- Toasts at the bottom right, `notification.margins.bottom` =
  13px × (2.0 + 0.75), just above the status bar.
- Palette, starting values (expected to be tuned):

  | Step | Value | Roles |
  | --- | --- | --- |
  | ground (window, viewer surround, inputs) | `#161616` | `background` |
  | panel body | `#1c1c1c` | `sidebar`, `tab_active`, `list`, `table`, `group_box` |
  | header and toolbar band | `#232323` | `title_bar`, `tab_bar`, `tab`, `status_bar`, `secondary`, `muted`, `list_head`, `table_head` |
  | band hover / pressed | `#2c2c2c` / `#1a1a1a` | `secondary_hover`, `list_hover` / `secondary_active`, `button_active` |
  | popover | `#2a2a2a` | `popover` |
  | hover and menu highlight | `#3a3a3a` | `accent` (ghost-button hover at half strength, hovered and keyboard-selected menu items) |
  | border | `#2e2e2e` | `border`, `input` and the bar, sidebar, table and window borders |
  | text | `#e6e6e6` / `#9a9a9a` | `foreground` and friends / `muted_foreground`, `tab_foreground` |
  | primary | `#4a7fd6` (hover `#5a8be0`, active `#3f72c4`) | `primary*`, `ring`, active borders |

- `metrics`: `BASE_FONT_SIZE`, `panel_header_height()` (1.875rem),
  `list_row_height()` (1.75rem), `toolbar_row_height()` (2rem),
  `panel_padding()` (0.75rem), `scaled(design_px) -> Rems` and
  `scaled_px(design_px) -> Pixels` for sizes designed against a 16px `rem`
  (`scaled_px` for APIs that take only `Pixels`, such as `Dialog::w`).

**`PanelHeader` (`crates/editor/src/panel_header.rs`).** The header band
every panel shares: band color, one hairline, the title (`text_base`,
medium), an optional monospace count (hidden at 0), a trailing slot for
actions, and `•••` (`IconName::Ellipsis`, tooltip "Options") only when the
panel has options. Assets uses it. No panel has options yet, so the
shipped editor shows no `•••`.

**Right dock.** gpui-kit's default `TabVariant::Tab` (was `underline`), at
the panel header's height, so its band lines up with Assets. Counts are
monospace. A hand-built strip was not needed.

**Title bar.** The composition picker and Export… are `ghost` (were
`outline`); the project name is `text_base` and ellipsizes, so a long name
no longer pushes the composition picker out of the bar.

**Status bar.** A toolbar row tall; every child sets `text_sm` (gpui-kit's
`StatusBar` defaults to `text_xs`, ≈9.75px at this base); numbers and
problem counts in monospace.

**Toasts.** The narrow message column beside "Show in Problems" is
gpui-kit's own `Notification` layout (a `flex_1` column beside the action);
the editor already passes the button through `Notification::action`, so it
was left as is.

## Migration rules

- Text: `text_xs` → `text_sm`; `text_sm` for body text, values and titles →
  `text_base`; labels and secondary text stay `text_sm`. The ruler's 10px
  labels → `text_sm`. `style_guard.rs` fails on `text_xs(` or
  `text_size(px(` unless the line says `physical:`.
- The meter's dB labels keep `text_size(px(9.0))`, marked `physical:`: at
  `text_sm` they overlapped on the pixel-spaced painted scale.
- `px()`: layout sizes → `metrics::scaled` / `scaled_px` / GPUI helpers.
  Physical ones stay with a `physical:` note: the timeline (module docs),
  the meter and its column, the volume slider, the canvas painting in
  `viewer.rs` and `inline_text_view.rs`, the asset thumbnail, the platform
  window sizes, and the docks' and timeline's resizable split sizes (the
  window does not shrink with the rem, so the regions keep their size and
  the denser type shows more in them). The timeline's geometry moves onto
  tokens in P7c.

## Tests

- `celesta-editor-theme`: `init_sets_the_editor_scale`,
  `apply_restores_the_scale_after_a_reload`,
  `surfaces_step_from_ground_to_popover`, `tokens_keep_their_order`,
  `scaled_keeps_a_16px_design_in_proportion`,
  `scaled_px_is_scaled_at_the_base_font_size`.
- `celesta-editor`: `panel_header::tests` (no options → no `•••`; a zero
  count hidden; a count shown; `•••` opens the menu and Escape closes it)
  and `style_guard::text_sizes_stay_on_the_type_roles`.
- The dev-dependency is `gpui-pre =0.3.11` with `test-support`, as the
  editor already had: gpui-kit's own `test-support` feature pulls a taffy
  that conflicts with gpui-pre's, so `gpui_kit::test::TestWindowExt` is not
  available. The `PanelHeader` tests use `VisualTestContext` and
  `debug_selector`; whether Escape returns focus to `•••` is gpui-kit's
  `DropdownMenu` behavior and is not asserted here.

## How it was checked

- `cargo test -p celesta-editor-theme -p celesta-editor` and
  `cargo clippy -p celesta-editor-theme -p celesta-editor -- -D warnings`.
- Screenshots (attached to the PR, not committed): the editor built from
  the commit before P7a and from the branch tip, both run on
  `examples/greenscreen/composite.tsx` from the repository root with
  `CELESTA_PLUGIN_PATH="$PWD/plugins/dist"`, a 1440×900 window, captured
  with `screencapture -x -o -l <window id>` (the id from
  `CGWindowListCopyWindowInfo`). The JS packages had to be rebuilt with
  `turbo run build --filter "@celesta.studio/cli..." --force` first: a turbo
  cache hit had not restored `packages/react/dist`, and the React runtime
  exited on start.
- Zoom: `BASE_FONT_SIZE` temporarily 12 and 16 (not committed). The
  hierarchy, wrapping and truncation held; at 16 the meter column (pixel
  sized) gets tight around "Volume 100%", which P7c takes with the meter.
- The user guide names the title bar, Export…, the Problems tab and the
  status bar but shows no screenshots and does not describe their look, so
  it was not changed. The Agent Skill covers the composition API only.
