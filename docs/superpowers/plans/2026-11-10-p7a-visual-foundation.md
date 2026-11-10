# P7a visual foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the editor a dense, neutral, "like the timeline" visual base: a 13px `rem`, one grey family, radius 0, a shared `PanelHeader`, classic right-dock tabs, borderless chrome, toasts at the bottom right, and text/`px()` call sites moved onto that scale.

**Architecture:** `celesta-editor-theme` owns every raw value: `init` edits gpui-kit's global `Theme` (font size, radius, focus ring, notification placement, colors) and a new `metrics` module exposes `rem`-based dimension tokens. The editor crate reads only `cx.theme()` roles and `metrics`, gains a `PanelHeader` element, and its call sites are migrated by fixed rules.

**Tech Stack:** Rust, gpui-kit 0.9 (`gpui-component` 0.9.0, GPUI from `gpui-pre` 0.3.11), `cargo test`.

**Spec:** `docs/superpowers/specs/2026-11-10-p7a-visual-foundation-design.md`

## Global Constraints

- Build environment: `export PKG_CONFIG_PATH="$(brew --prefix ffmpeg@8)/lib/pkgconfig"` and `export PATH="/Users/natsuneko/Library/Application Support/mbx/bin:$HOME/.cargo/bin:$HOME/.local/share/mise/installs/node/26/bin:$HOME/.local/share/mise/installs/pnpm/12/bin:$PATH"`.
- `rem` = 13px (`Theme::font_size`); `radius` and `radius_lg` = 0; `focus_ring` = false.
- Raw colors (`rgb`, `rgba`, hex) only in `crates/editor-theme`. Editor code reads `cx.theme()` roles and `celesta_editor_theme::metrics`.
- Type roles: body, values and panel titles `text_base`; labels and secondary text `text_sm`; timecode monospace `text_lg`. No `text_xs`, no `text_size(px(...))` in `crates/editor/src`.
- Two weights only: regular and medium (`font_medium`) for panel titles and the selected tab.
- Chrome buttons are `ghost`; no `outline` buttons in the title bar, panel headers or status bar. The danger "Cancel export" button stays `danger`.
- No gradients, no uppercase transforms, lucide icons only (`IconName`), every icon-only button has a tooltip.
- The timeline's geometry constants (`TRACK_HEADER_WIDTH`, `RULER_HEIGHT`, `LANE_GUTTER` and the like) and the data colors (`clip_fill`, `meter`, waveform, export range) do not change in P7a.
- No new features: no options are invented to fill a `•••` menu.
- Never amend commits. Commits are signed (1Password; "failed to fill whole buffer" means it is locked: ask the user to unlock). Every commit message ends with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. **gpui-kit's own `text_xs`.** `StatusBar` sets `text_xs` on its container, and menus, tooltips and tab suffixes may too; at a 13px `rem` that is ≈9.75px. Anything the editor puts in those containers must set its own type role (Task 5 pins the status bar; Task 7 looks at tooltips and menus in the screenshots).
2. **The theme being reloaded.** `Theme::change` reloads the dark config and drops `font_size`, `radius` and color edits. `init` must apply the edits in a `Theme::update` after `change`, through one `apply` function that can be called again (Task 1 tests that a second `change` followed by `apply` restores them).
3. **A count of zero or none.** `PanelHeader` with `count(0)` shows no count, and a tab with no problems shows no suffix (Task 3 and Task 4 tests).
4. **A panel with no options.** No `•••` and no empty menu (Task 3 test). Today no panel has options, so the shipped editor shows no `•••` at all; that is correct.
5. **Long titles in a narrow dock.** A long project name in the title bar and the right dock's tab strip at its minimum width must truncate or overflow into the tab menu, not push buttons off the bar (Task 5 and Task 7 screenshots at the dock's minimum width).

---

### Task 1: Theme settings and dimension tokens

**Files:**
- Modify: `crates/editor-theme/src/lib.rs` (`init`)
- Create: `crates/editor-theme/src/metrics.rs`
- Modify: `crates/editor-theme/Cargo.toml` (dev-dependency for tests)

**Interfaces:**
- Produces:
  - `pub fn init(cx: &mut App)`: `Theme::change(ThemeMode::Dark, None, cx)` then `apply(cx)`.
  - `pub fn apply(cx: &mut App)`: one `Theme::update` that sets the values below and the palette (Task 2 fills the palette in).
  - `pub mod metrics` with `pub const BASE_FONT_SIZE: f32 = 13.0;` and
    `pub fn panel_header_height() -> Rems` (`rems(1.875)`),
    `pub fn list_row_height() -> Rems` (`rems(1.75)`),
    `pub fn toolbar_row_height() -> Rems` (`rems(2.0)`),
    `pub fn panel_padding() -> Rems` (`rems(0.75)`).

- [ ] **Step 1: Add the test dependency.** In `crates/editor-theme/Cargo.toml` add `[dev-dependencies] gpui-kit = { version = "0.9", features = ["test-support"] }` (same version as the normal dependency).

- [ ] **Step 2: Write the failing tests** in `crates/editor-theme/src/lib.rs` (`#[cfg(test)] mod tests`):

```rust
#[gpui_kit::test]
fn init_sets_the_editor_scale(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        init(cx);
        let theme = Theme::global(cx);
        assert_eq!(theme.font_size, px(metrics::BASE_FONT_SIZE));
        assert_eq!(theme.radius, px(0.));
        assert_eq!(theme.radius_lg, px(0.));
        assert!(!theme.focus_ring);
        assert_eq!(theme.notification.placement, Anchor::BottomRight);
        assert!(theme.is_dark());
    });
}

#[gpui_kit::test]
fn apply_restores_the_scale_after_a_reload(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
        apply(cx);
        assert_eq!(Theme::global(cx).font_size, px(metrics::BASE_FONT_SIZE));
        assert_eq!(Theme::global(cx).radius, px(0.));
    });
}
```

And in `metrics.rs`:

```rust
#[test]
fn tokens_keep_their_order() {
    let px = |r: Rems| r.0 * BASE_FONT_SIZE;
    assert!(px(panel_header_height()) >= px(list_row_height()));
    assert!(px(toolbar_row_height()) >= px(list_row_height()));
    assert!(px(panel_padding()) < px(list_row_height()));
}
```

- [ ] **Step 3: Run them and see them fail.** Run: `cargo test -p celesta-editor-theme`. Expected: compile errors for `apply` and `metrics`.

- [ ] **Step 4: Implement `metrics.rs` and `apply`.** In `apply`'s `Theme::update` closure set `font_size = px(metrics::BASE_FONT_SIZE)`, `radius = px(0.)`, `radius_lg = px(0.)`, `focus_ring = false`, `notification.placement = Anchor::BottomRight`, and `notification.margins.bottom = px(metrics::BASE_FONT_SIZE * (2.0 + 0.75))` (the status bar's toolbar row plus panel padding, so toasts sit just above it). Leave `mono_font_size` at gpui-kit's 13px. Leave `shadow` on: it drives the overlay elevations the spec keeps, and `ghost` buttons cast none. `init` keeps its doc comment and calls `change` then `apply`.

- [ ] **Step 5: Run the tests and see them pass.** Run: `cargo test -p celesta-editor-theme`. Expected: 3 passed.

- [ ] **Step 6: Commit.** `git commit -S -m "feat(editor-theme): 13px rem, square corners, bottom-right toasts and dimension tokens"` (with the Co-Authored-By line).

### Task 2: One neutral grey palette

**Files:**
- Modify: `crates/editor-theme/src/lib.rs` (palette inside `apply`)

**Interfaces:**
- Consumes: `apply` from Task 1.
- Produces: no new names; `cx.theme()` roles carry the palette.

Starting values (the user expects to tune them later; record final values in the handoff note):

| Step | Value | Roles |
| --- | --- | --- |
| 1 ground, viewer surround, input field | `#161616` | `background`, `input` stays the border |
| 2 panel body | `#1c1c1c` | `sidebar`, `tab_active`, `list`, `table`, `group_box` |
| 3 header and toolbar band | `#232323` | `title_bar`, `tab_bar`, `tab`, `status_bar`, `secondary`, `muted`, `list_head`, `table_head` |
| band hover / pressed | `#2c2c2c` / `#1a1a1a` | `secondary_hover`, `list_hover` / `secondary_active`, `button_active` |
| 5 popover | `#2a2a2a` | `popover` |
| border | `#2e2e2e` | `border`, `input`, `title_bar_border`, `status_bar_border`, `sidebar_border`, `table_row_border`, `window_border` |
| text | `#e6e6e6` / `#9a9a9a` | `foreground`, `popover_foreground`, `sidebar_foreground`, `tab_active_foreground`, `secondary_foreground` / `muted_foreground`, `tab_foreground` |
| primary | `#4a7fd6`, hover `#5a8be0`, active `#3f72c4`, fg `#ffffff` | `primary*`, `ring`, `list_active_border`, `table_active_border` |

- [ ] **Step 1: Write the failing test** in `lib.rs`:

```rust
#[gpui_kit::test]
fn surfaces_step_from_ground_to_popover(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        init(cx);
        let theme = Theme::global(cx);
        let l = |c: Hsla| c.l;
        assert!(l(theme.background) < l(theme.sidebar));
        assert!(l(theme.sidebar) < l(theme.title_bar));
        assert!(l(theme.title_bar) < l(theme.popover));
        assert_eq!(theme.tab_active, theme.sidebar);
        assert_eq!(theme.tab_bar, theme.title_bar);
        assert_eq!(theme.status_bar, theme.title_bar);
        // One neutral family: no hue drift between steps.
        for c in [theme.background, theme.sidebar, theme.title_bar, theme.popover] {
            assert!(c.s < 0.02, "{c:?} is not neutral");
        }
    });
}
```

- [ ] **Step 2: Run it and see it fail.** Run: `cargo test -p celesta-editor-theme surfaces_step`. Expected: FAIL (gpui-kit's default dark palette).

- [ ] **Step 3: Set the palette in `apply`** from the table, through `rgb(...)` values assigned to `theme.colors.<role>` in the same `Theme::update` closure.

- [ ] **Step 4: Run the tests and see them pass.** Run: `cargo test -p celesta-editor-theme`. Expected: 4 passed.

- [ ] **Step 5: Commit.** `feat(editor-theme): one neutral grey family, a restrained blue primary`.

### Task 3: `PanelHeader`

**Files:**
- Create: `crates/editor/src/panel_header.rs`
- Modify: `crates/editor/src/main.rs` (module), `crates/editor/src/panels.rs:259` (Assets uses it), `crates/editor/src/widgets.rs` (remove `panel_header`)
- Modify: `crates/editor/Cargo.toml` (`[dev-dependencies] gpui-kit = { version = "0.9", features = ["test-support"] }`)

**Interfaces:**
- Consumes: `metrics::panel_header_height()`, `metrics::panel_padding()`.
- Produces:

```rust
#[derive(IntoElement)]
pub(crate) struct PanelHeader { /* id, title, count, actions, options */ }
impl PanelHeader {
    pub(crate) fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self;
    /// Shown, monospace and muted, only when greater than zero.
    pub(crate) fn count(self, count: usize) -> Self;
    /// Appended to the trailing slot, before `•••`.
    pub(crate) fn action(self, action: impl IntoElement) -> Self;
    /// Adds the `•••` button (id `"{id}-options"`, `IconName::Ellipsis`,
    /// tooltip "Options") opening this menu. Without it there is no button.
    pub(crate) fn options(
        self,
        menu: impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static,
    ) -> Self;
}
```

Look: `.h(panel_header_height())`, `.px(panel_padding())` (the horizontal-padding method taking the token), `bg(theme.title_bar)`, `border_b_1()` in `theme.border`, title `text_base` + `font_medium` + `foreground`, count `text_sm` + mono family + `muted_foreground`, `•••` a `ghost` `xsmall` `Button` in a fixed-width trailing lane.

- [ ] **Step 1: Write the failing UI tests** in `panel_header.rs` (`#[cfg(test)]`), rendering a header inside `Root` in a headless window and using `gpui_kit::test::TestWindowExt`:
  - `no_options_means_no_menu_button`: `PanelHeader::new("h", "Assets")` → `window.try_find("h-options").is_none()`.
  - `a_zero_count_is_not_shown`: `.count(0)` → `window.try_find("h-count").is_none()`; `.count(3)` → `window.find("h-count").visible()` (give the count element the id `"{id}-count"`).
  - `options_open_and_escape_returns_focus`: `.options(|menu, _, _| menu.item(PopupMenuItem::new("Reveal")))`; `click("h-options")` → `window.try_find("popup-menu").is_some()`; `press("escape")` → `try_find("popup-menu").is_none()` and `find("h-options").focused() == Some(true)`.

- [ ] **Step 2: Run them and see them fail.** Run: `cargo test -p celesta-editor panel_header`. Expected: compile error, `PanelHeader` not found.

- [ ] **Step 3: Implement `PanelHeader`** (`RenderOnce`), using `Button::dropdown_menu` for `•••` as `panels.rs` does for the composition picker.

- [ ] **Step 4: Use it for Assets** (`PanelHeader::new("assets-header", "Assets").count(self.assets.len())`) and delete `widgets::panel_header`.

- [ ] **Step 5: Run the tests and see them pass.** Run: `cargo test -p celesta-editor panel_header`. Expected: 3 passed. If `focused()` returns `None` for the button, register the menu's focus binding as the test reference says rather than dropping the assertion.

- [ ] **Step 6: Commit.** `feat(editor): a shared PanelHeader`.

### Task 4: Classic right-dock tabs

**Files:**
- Modify: `crates/editor/src/problems_panel.rs:16-60` (`right_dock`)

**Interfaces:**
- Consumes: `metrics::panel_header_height()`; theme roles from Task 2 (`tab_bar`, `tab`, `tab_active`).

- [ ] **Step 1: Try the gpui-kit tab.** Remove `.underline()` (the default `TabVariant::Tab`), drop `.px_3()`, set `.h(panel_header_height())`, keep `.menu(true)` and `on_click`. Tab suffix counts become `text_sm`, mono family, `muted_foreground`; the selected tab's label `font_medium`.

- [ ] **Step 2: Check the look against the spec.** Launch the editor (Task 7's command) and capture the window. Pass when: the selected tab is `tab_active` (= panel body) with no line between it and the content; other tabs sit on the `tab_bar` band; the band's bottom hairline lines up with the Assets header's on the left.

- [ ] **Step 3: Only if Step 2 fails,** build the strip by hand in a new `crates/editor/src/dock_tabs.rs`: an `h_flex` band of `ghost` `Button`s (id `"right-dock-tab-{RightTab:?}"`, `.selected(..)`), the overflow `DropdownMenu` listing all tabs, and the same `on_click` behavior. Add a UI test that clicking `"right-dock-tab-Problems"` shows the Problems panel.

- [ ] **Step 4: Run the editor tests.** Run: `cargo test -p celesta-editor`. Expected: all pass.

- [ ] **Step 5: Commit.** `feat(editor): classic tabs on the right dock`.

### Task 5: Title bar, status bar and toasts

**Files:**
- Modify: `crates/editor/src/panels.rs:28-153` (`title_bar`), `crates/editor/src/render.rs:260-295` (`status_bar`), the toast construction in `crates/editor/src/problems_panel.rs:309-360`

**Interfaces:**
- Consumes: `metrics::toolbar_row_height()`.

- [ ] **Step 1: Title bar.** The composition picker and Export… change from `.outline()` to `.ghost()`; the project name `text_base`; "React composition" `text_sm`. Nothing added or removed.

- [ ] **Step 2: Status bar.** `.h(toolbar_row_height())` on `StatusBar`; every child sets `text_sm` (Review Focus 1); the "dimensions · frame rate · GPU" text and the counts in `problems_indicator` and `export_progress_label` use the mono family.

- [ ] **Step 3: Toast wrapping.** Push the error toast from `problems_panel.rs` (title, message, "Show in Problems") in the running editor and capture it. If the message wraps into a narrow column because of how the editor builds it (for example the action placed as a sibling of the message instead of through `Notification::action`), rebuild it so the message takes the full width. If it is gpui-kit's own layout, leave it and record that in the handoff note.

- [ ] **Step 4: Run the editor tests.** Run: `cargo test -p celesta-editor`. Expected: all pass.

- [ ] **Step 5: Commit.** `feat(editor): borderless title bar, monospace status bar, readable toasts`.

### Task 6: Text sizes

**Files:**
- Modify: every file with `text_xs()` (80 sites: `panels.rs` 13, `viewer.rs` 10, `timeline.rs` 10, `layer_panel.rs` 10, `plugins_panel.rs` 9, `problems_panel.rs` 8, `element_rows_view.rs` 7, `property_panel_view.rs` 4, `widgets.rs` 3, `render.rs` 2, `export_dialog/view.rs` 2, `property_controls.rs` 1, `export_dialog/view/fields.rs` 1), every `text_sm()` (27 sites) and `text_size(px(...))` (3 sites)
- Create: `crates/editor/src/style_guard.rs` (test-only module)

**Interfaces:**
- Consumes: the type roles in Global Constraints.

- [ ] **Step 1: Write the failing guard test** in `style_guard.rs`: walk `concat!(env!("CARGO_MANIFEST_DIR"), "/src")` recursively, read every `.rs` file except `style_guard.rs`, and `assert!` that none contains `"text_xs("` or `"text_size(px("`, listing each offending `path:line` in the failure message.

- [ ] **Step 2: Run it and see it fail.** Run: `cargo test -p celesta-editor style_guard`. Expected: FAIL listing 83 sites.

- [ ] **Step 3: Migrate by role.** `text_xs()` → `text_sm()`. Each `text_sm()` → `text_base()` where it is body text, a value or a title; it stays `text_sm()` where it is a label or secondary text. Each `text_size(px(n))` → the role it plays (the ruler's 10px labels → `text_sm`).

- [ ] **Step 4: Run all editor tests.** Run: `cargo test -p celesta-editor`. Expected: all pass, including `style_guard`.

- [ ] **Step 5: Commit.** `refactor(editor): text sizes by role on the 13px scale`.

### Task 7: Layout `px()` onto `rem`

**Files:**
- Modify: the editor's `px(...)` call sites outside the timeline geometry constants (214 in total; `grep -rn 'px(' crates/editor/src`)

**Interfaces:**
- Consumes: `metrics::*`.

- [ ] **Step 1: Classify and convert.** For each `px(n)`:
  - **Layout** (padding, gaps, widths, fixed heights of panels, rows and controls): use the matching `metrics` token if one fits, else the GPUI helper for `n / 4` (`h_10` for 40, `w_64` for 256), else `rems(n / 16.0)`. This keeps today's proportions on the 13px scale.
  - **Physical** (1px lines, the playhead's width, hit-test tolerance, pointer and coordinate math in `timeline*.rs`, `transform_*.rs`, `viewer.rs` canvas code, the timeline geometry constants): leave it, and add `// physical: <why>` on the line or once above a block of them.

- [ ] **Step 2: Run all editor tests.** Run: `cargo test -p celesta-editor`. Expected: all pass.

- [ ] **Step 3: Check that nothing unexplained is left.** Run: `grep -rn 'px(' crates/editor/src | grep -v 'physical:' | grep -v '^crates/editor/src/.*tests'`; every remaining line is inside a block whose `// physical:` comment covers it.

- [ ] **Step 4: Commit.** `refactor(editor): layout sizes on rem, physical pixels marked`.

### Task 8: Screenshots, zoom check, handoff note

**Files:**
- Create: `docs/handoff/2026-11-10-p7a-visual-foundation.md`
- Possibly modify: `packages/website/content/en/` and `packages/website/content/ja/` pages that show the editor

- [ ] **Step 1: Before screenshot.** On the commit before Task 1 (`git worktree add` a scratch checkout of it), build with `cargo build -p celesta-editor`, stage plugins with `node scripts/plugins/build.mts plugins/ffmpeg`, and run `target/debug/celesta-editor examples/greenscreen/composite.tsx` from the repository root with `CELESTA_PLUGIN_PATH="$PWD/plugins/dist"`, so the React runtime starts and the project loads (not the "runtime exited" placeholder). Resize the window to 1440×900, find its id with `CGWindowListCopyWindowInfo`, and capture `screencapture -x -o -l <id> before.png`.

- [ ] **Step 2: After screenshot.** Same scene, same size, on the branch tip: `after.png`. Also capture the right dock at its minimum width and a long project name (Review Focus 5), and an open `•••`-less header, a tooltip and a menu to check gpui-kit's `text_xs` (Review Focus 1).

- [ ] **Step 3: Zoom check.** Temporarily set `BASE_FONT_SIZE` to 12.0 and 16.0 (not committed), rebuild, capture each, and confirm the hierarchy, wrapping and truncation hold. Revert with `git checkout crates/editor-theme/src/metrics.rs`.

- [ ] **Step 4: Website.** `grep -rln -i 'editor' packages/website/content/en packages/website/content/ja` and check for editor screenshots or descriptions of the chrome (panel headers, tabs, buttons). Update both languages if any exist; otherwise note that none did.

- [ ] **Step 5: Handoff note.** Write `docs/handoff/2026-11-10-p7a-visual-foundation.md`: what changed (tokens with final color values, `PanelHeader`, tabs path taken in Task 4, title/status bar, toast finding from Task 5 Step 3), the migration rules, the tests, and how the screenshots were taken. Screenshots are attached to the PR, not committed.

- [ ] **Step 6: Full check.** Run: `cargo test -p celesta-editor-theme -p celesta-editor` and `cargo clippy -p celesta-editor-theme -p celesta-editor -- -D warnings`. Expected: all pass, no warnings.

- [ ] **Step 7: Commit.** `docs: P7a handoff note`.
