# P7a: the editor's visual foundation (2026-11-10)

First sub-project of P7, the editor's UI refinement. It sets the tokens, the
shared panel header and the window chrome that the later sub-projects build
on. Decided with the user one question at a time; the reference study of
DaVinci Resolve 21 and Premiere Pro 2026 that informed it was done hands-on
the same day (screenshots and notes outside the repository).

## Why

The editor does not look like a professional tool. Measured against Resolve
and Premiere on the same kind of screen:

- **Density is low.** The editor uses gpui-kit's default base font size,
  16px, as its `rem`. Body text renders at 14px (`text_sm`), panel rows at
  about 40px and Inspector blocks at about 70px. Resolve and Premiere sit at
  12–13px text and 20–24px rows.
- **Surfaces mix two hues.** The viewer and panels are a blue-black, the
  timeline a neutral grey.
- **Panel headers disagree.** "Assets" is a plain header; the right dock is
  an underlined tab bar; neither has a panel menu.
- **It reads as shadcn/ui.** Outlined buttons, underlined tabs, label-over-
  value blocks with generous padding, inputs with outer focus rings and card
  toasts make it look like a web component library.

The timeline is the exception, and the user likes it. It is hand-built from
flat bands and hairlines, lays rows out in fixed lanes, shows numbers and
identifiers in monospace, uses color only where it is data, and shows state
with small muted icons and tooltips. **P7 derives the rest of the editor
from the timeline**, not from stock gpui-kit component styling.

## Who it is for

Today mostly developers who write compositions in TSX and use the editor to
preview, check and fine-tune values, switching between it and their code.
The editor is to grow toward people who stay in it for long editing
sessions. The density target is therefore close to Resolve and Premiere,
without making value inspection and fine-tuning harder.

## Success criteria

1. The tokens and the rules below are written down, and every panel follows
   them (checkable).
2. At the end of P7 (after P7d), screenshots of the same scene beside
   Resolve and Premiere read as the same class of tool (judged by the user).

P7a is done when criterion 1 holds for the chrome it covers. Criterion 2 is
checked once, at the end of P7.

## P7 as a whole

The layout stays: Assets on the left, the preview in the middle, the right
dock (Inspector, Plugins, Problems), the timeline below. There are no pages
or modes, no free docking and no workspaces. Export becomes a page (P7d).
No sub-project adds features: no keyframe diamonds without keyframes, no
track lock buttons the project model does not have. Each sub-project gets
its own spec, plan and pull request.

| Sub-project | Scope | Depends on |
| --- | --- | --- |
| **P7a visual foundation** (this spec) | tokens, `PanelHeader`, right-dock tabs, title bar, status bar, toasts, the `text_xs`/`px()` migration | — |
| P7b Inspector | collapsible sections, label and value side by side, scrubbable values with units | P7a |
| P7c timeline and transport | one timecode, a thinned ruler, track header and clip look, transport buttons ("In"/"Out" text vs icons), timeline geometry constants on tokens | P7a |
| P7d export page | the export dialog becomes a page; the P3 dialog's issues (one long column, "Preset" with two meanings, a repeated "Rate control" heading, plugin ids instead of names, CLI text in warnings) | P7a |

P7b, P7c and P7d can proceed in parallel once P7a has landed.

## Design direction: "like the timeline"

These rules apply to P7a and to every later sub-project.

- **Flat bands and one hairline.** Regions are separated by a background
  step and a single border, never by cards, outlined boxes or doubled
  padding. The boundary owner draws the hairline; neighbors do not repeat
  it.
- **Rows in lanes.** A row is `[key lane][content][fixed-width trailing
  icon lane]`. Optional content never shifts the other lanes.
- **Monospace for numbers and identifiers.** Timecodes, resolutions, frame
  rates, counts, track labels.
- **Chrome buttons have no border.** gpui-kit `Button` in the `ghost`
  variant; with the zero radius below it is a flat cell. Hover lifts the
  band one step; pressed and "on" darken it one step. No `outline` buttons
  in the chrome.
- **Color is data or emphasis, not decoration.** Clip kinds, the playhead,
  the export range, meter levels. `primary` marks selection, focus and the
  primary action.
- **Elevation only for overlays.** Popovers, menus, toasts and dialogs cast
  a shadow; window regions do not.
- **Focus stays visible, without a halo.** Focused controls show a thin
  inner line instead of an outer ring (the theme's `focus_ring` set to
  false).
- **Compose first, hand-build second.** Anything with keyboard or
  accessibility behavior (`Button`, menus, `Tooltip`, `TabBar`) is a gpui-kit
  component styled through the theme. Only when the theme cannot produce
  the look is an element built by hand, as the timeline's rows are.
- **No AI look.** No gradients, no uppercase transforms, corner radius 0,
  lucide icons only (no text glyphs as icons).

## Tokens

All tokens live in `celesta-editor-theme`. Raw color values appear only
there; editor code reads `cx.theme()` roles and the theme crate's tokens.

### Base size and type

- `rem` = **13px**: `celesta_editor_theme::init` sets the theme's
  `font_size` to 13px. gpui-kit component sizes are `rem`-based (a `small`
  `Button` is `size_6` = 1.5rem), so medium controls become about 26px and
  small ones about 20px.
- Type roles (platform UI font):

  | Role | Helper | Size |
  | --- | --- | --- |
  | Body, values, panel titles | `text_base` | 13px |
  | Labels, secondary text, ruler, status bar | `text_sm` | ≈11.4px |
  | Timecode | monospace `text_lg` | ≈14.6px |

- `text_xs` (≈9.75px at this base) is not used.
- Two weights: regular, and medium for panel titles and the selected tab.

### Color

One neutral grey family; the blue-black goes. Values are defined in the
theme crate by overriding gpui-kit's semantic roles (`background`,
`sidebar`, `popover`, `border`, `input`, `muted`, `secondary` and their
foregrounds). The steps, darkest first:

1. window ground and viewer surround;
2. panel body;
3. panel header and toolbar band;
4. input field (one step sunken from the band it sits on);
5. popover (one step lifted, with a shadow).

- One border color.
- Two text levels: `foreground` and `muted_foreground`.
- `primary` is a restrained blue, for selection, focus and the primary
  action.
- The brand orange (`ACCENT`) stays for the wordmark and the playhead only,
  as the theme crate's documentation already says; it is not a button
  color.
- The timeline's data colors (`clip_fill`, `meter`, waveform, export range)
  are unchanged.

The exact values are chosen during implementation against the screenshots
and are expected to be tuned afterwards; the user accepted them as
adjustable later.

### Radius and shadow

- `radius` and `radius_lg` are 0.
- Shadows only on overlay surfaces (see the design direction).

### Dimension tokens

New, `rem`-based, in the theme crate:

- panel header height;
- list row height;
- transport and toolbar row height;
- panel inner padding.

Spacing uses gpui-kit's `rem` helpers in their existing ratios (the 2, 4, 8,
12, 16, 24, 32 relationships). Celesta does not define its own spacing
scale. Tokens that later sub-projects need (track height, Inspector row
height) are added by those sub-projects.

## Components and chrome

### `PanelHeader`

A new shared element in the editor crate, replacing
`widgets::panel_header`. It is built like the timeline's ruler row:

- the panel-header height token, the header band color, one hairline at the
  bottom;
- leading: the title (`text_base`, medium), then an optional count
  (monospace, muted);
- a trailing slot for panel actions;
- a trailing `•••` button (lucide `Ellipsis`, `DropdownMenu`) holding that
  panel's own options. A panel with no options shows no `•••`; there are no
  empty menus. There are no panel-management items (close, undock), since
  there is no docking.

P7a uses it for Assets. P7b–d use it for their panels.

### Right-dock tabs

The underlined `TabBar` is replaced by classic NLE tabs: the selected tab
takes the panel body's color and joins the content below; the others sit on
the header band. The band's height, background, baseline and hairline match
`PanelHeader`, so "Assets" on the left and the tabs on the right share one
horizontal line. The trailing `•••` belongs to the selected tab.

Implementation first tries gpui-kit's `TabBar` with `TabVariant::Tab` under
the zero radius. If that cannot produce the look, the tab strip is built by
hand, keeping keyboard selection and the overflow menu the dock has today.

### Title bar

- Leading: the project name (`text_base`) and "React composition" (muted,
  `text_sm`), as today.
- Trailing: Create New Project…, Open… and Export… as `ghost`, `small`
  buttons on one baseline. Export… loses its outline. It becomes the export
  page's entry in P7d.
- Nothing is added.

### Status bar

Content unchanged (resolution · frame rate · GPU, then error and warning
counts). `text_sm`, muted, numbers in monospace, height on the row token.

### Toasts

- `NotificationSettings::placement` (the theme's global setting) moves
  toasts to the bottom right, just above the status bar, so they no longer
  cover panel headers and tabs.
- Popover surface, radius 0, a hairline border, a shadow.
- Today's error toast wraps its message into a narrow column beside its
  action button. Implementation first finds whether that comes from
  gpui-kit's `Notification` layout or from how the editor builds the toast,
  and fixes it if it is the editor's.

### Icons

- lucide only.
- Every icon-only button has a tooltip with its name.
- The transport's mix of text ("In", "Out") and icons is P7c's decision.

## Migration

### Text sizes

Lowering `rem` from 16px to 13px shrinks every text helper one step, so each
call site moves up one step by its role:

- `text_xs` → `text_sm` (80 call sites): 12px becomes ≈11.4px, almost the
  same.
- `text_sm` used for body text → `text_base`: 14px becomes 13px.
- `text_sm` used for labels and secondary text stays (≈11.4px).
- `text_size(px(...))` (3 call sites) → a type token.

With this mapping the timeline looks almost the same. The tighter look comes
from `rem`-based control heights and spacing.

### `px()`

The editor has 214 `px(...)` calls. Each is one of:

- **Layout** (padding, widths, panel heights) → a `rem` helper or a
  dimension token.
- **Physical** (1px hairlines, the playhead's width, hit-test tolerance,
  coordinate math in the timeline and transform handles) → kept, with a
  comment saying why.

The timeline's geometry constants (`TRACK_HEADER_WIDTH`, `RULER_HEIGHT`,
`LANE_GUTTER` and the like) are left to P7c.

## Testing

Rust, `cargo test`:

- `celesta-editor-theme` unit tests:
  - after `init`, `font_size` is 13px, `radius` and `radius_lg` are 0,
    `focus_ring` is false, and the notification placement is bottom right;
  - the dimension tokens keep their order (the panel header is at least as
    tall as a list row).
- `PanelHeader` UI integration tests (`#[gpui_kit::test]`):
  - with no options there is no `•••`;
  - `•••` opens the menu; Escape closes it and returns focus to the button.
- A source-scan test over `crates/editor/src` that fails on `text_xs(` and
  `text_size(px(`, so they do not come back.

## Verification

- Before and after screenshots of the same scene, as AGENTS.md requires:
  - `examples/greenscreen` with the project actually loaded (not the
    "runtime exited" placeholder);
  - a 1440×900 window;
  - captured with `screencapture -x -o -l <window id>`.
- Interface zoom check from the Design Guides: temporarily set `rem` to 12,
  13 and 16px and confirm the hierarchy, wrapping and truncation hold.
- Side-by-side comparison with Resolve and Premiere: at the end of P7, not
  in P7a.

## Documentation

- `docs/handoff/2026-11-10-p7a-visual-foundation.md` records what was built
  and how it was checked.
- If the user guide (`packages/website/content/en/` and `ja/`) shows editor
  screenshots or describes the chrome, update both languages. Otherwise
  leave it.
- The Celesta Agent Skill (`skills/celesta/`) covers the composition API
  and is not affected.

## Out of scope for P7a

- The Inspector's layout (P7b).
- Timeline geometry, the ruler, track headers, clips and the transport
  (P7c).
- The export page (P7d).
- A light theme. The editor stays dark only.
- User-selectable themes.
