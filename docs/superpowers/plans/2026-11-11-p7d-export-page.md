# P7d export page Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the editor's modal export dialog with an export page: a settings panel in the left slot beside the viewer, the right dock and the timeline, worded for the page (plugin display names, "Speed preset", "Mode", warnings without CLI text).

**Architecture:** `ExportDialog` becomes `ExportPage` (module `export_page/`), rendered by `EditorView::workspace()` in the left resizable panel while an `ExportPageSlot` says it is open; the model keeps its logic. The exporter's resolve warnings become a `ResolveWarning` enum whose `Display` is today's CLI text, so the editor can word them itself. `plugin.json` gains an optional `name`, carried to `MediaComponentInfo::plugin_name`.

**Tech Stack:** Rust, gpui-kit 0.9 (`gpui-component` 0.9.0, GPUI from `gpui-pre` 0.3.11), `cargo test`, serde.

**Spec:** `docs/superpowers/specs/2026-11-11-p7d-export-page-design.md`

## Global Constraints

- Build environment: `export PKG_CONFIG_PATH="$(brew --prefix ffmpeg@8)/lib/pkgconfig"` and `export PATH="/Users/natsuneko/Library/Application Support/mbx/bin:$HOME/.cargo/bin:$HOME/.local/share/mise/installs/node/26/bin:$HOME/.local/share/mise/installs/pnpm/12/bin:$PATH"`. The first-party plugins are a separate workspace: `cargo test --manifest-path plugins/Cargo.toml -p celesta-codec-ffmpeg`.
- No new export features: no render queue, no file name or location fields, no size estimate, no output summary, no per-field reset. `ExportModel`'s behavior (presets, the container rule, `last`, Save as preset…, describe and resolve in the background) is unchanged.
- The CLI's stderr, the JSON report's `warnings` and MCP keep today's text byte for byte. Setting keys (`preset`, `rateControl`) do not change.
- Tokens only: `cx.theme()` roles and `celesta_editor_theme::metrics`; no raw colors, no layout `px(...)` (a `physical:` comment for any pixel value kept).
- Type roles: labels and secondary text `text_sm`, values `text_base`, timecodes monospace. Weights regular and medium only.
- Chrome buttons `ghost`; the one `primary` is the panel's Export…. No gradients, no uppercase, radius 0, lucide `IconName` only, every icon-only button has a tooltip.
- User-guide changes go to both `packages/website/content/en/` and `packages/website/content/ja/`.
- Never amend commits. Commits are signed (1Password; "failed to fill whole buffer" means it is locked: ask the user to unlock). Every commit message ends with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. **Reload Plugins or another project while the page is open.** The page closes, Assets is back, no empty panel is left (Task 7: `ExportPageSlot::clear` test; Task 8 by hand).
2. **The narrowest Export panel (22rem).** Long component labels ("ProRes (prores_ks) · FFmpeg") truncate inside their control; no control pokes past the panel's edge (Task 6: `controls_fit_the_narrowest_panel`).
3. **A preset naming something this registry does not have** (a plugin not installed, an unknown key). The warning still reads as a sentence, using the reference or key as written (Task 5: `an_unknown_reference_is_worded_as_written`).
4. **The save panel cancelled.** The page stays as it was; only a started export closes it (Task 7: `ExportPageSlot` test for `export_started`; Task 8 by hand).
5. **Escape with a `Select` menu open on the page.** Only the menu closes; a second Escape closes the page (Task 8 by hand: the menu lives in the overlay layer, outside the editor's capture handler).

---

### Before you start: before screenshots

The "before" screenshots must show today's dialog, so take them first, on this branch before Task 1.

- [ ] Build: `cargo build -p celesta-editor`.
- [ ] Run `target/debug/celesta-editor examples/greenscreen/composite.tsx` (in the background). Ask the user to click "Trust" in the "Trust this project?" dialog.
- [ ] Size the window to 1440×900 and capture with `screencapture -x -o -l <window id>` into the session's scratchpad: `p7d-before-1-default.png` (Export… opened, after "Checking…" ends), `p7d-before-2-advanced.png` (Video's Advanced open), `p7d-before-3-warnings.png` (a temporary project `export-presets.json` with a preset `Web` holding `"crf": 99` and `"speed": 1`, chosen), `p7d-before-4-save.png` (Save as preset… open). Remove the temporary presets file afterwards.

---

### Task 1: Plugin display names

**Files:**
- Modify: `crates/plugin-host/src/manifest.rs` (`Manifest`, tests)
- Modify: `crates/media/src/registry.rs` (`RegisteredPlugin`, `MediaComponentInfo`, `plugin_components`, tests)
- Modify: every `RegisteredPlugin { … }` and `Manifest { … }` literal (`grep -rn "RegisteredPlugin {$\|[^a]Manifest {$" crates plugins`): `crates/exporter/src/media.rs`, `crates/exporter/src/{encode,resolve}.rs` tests, `crates/codec-test/src/lib.rs`, `crates/media-image/src/lib.rs`, `crates/react-bridge/examples/audio-bench.rs`, `crates/exporter/tests/integration/export.rs`, and the plugin-host tests
- Modify: `plugins/ffmpeg/plugin.json`
- Modify: `packages/website/content/{en,ja}/docs/05-(more)/05-plugins.md`, `skills/celesta/references/plugins.md`

**Interfaces:**
- Produces:
  - `celesta_plugin_host::Manifest::name: Option<String>` (`#[serde(default)]`), doc "The plugin's display name, as the editor shows it; the `id` when absent."
  - `impl Manifest { pub fn display_name(&self) -> &str }`: `name`, else `id`.
  - `celesta_media::RegisteredPlugin::name: String`: the display name.
  - `celesta_media::MediaComponentInfo::plugin_name: String`: its plugin's `name`.

- [ ] **Step 1: Write the failing tests.**
  - `manifest.rs`: `a_manifest_reads_its_display_name` (`"name": "FFmpeg"` → `display_name() == "FFmpeg"`); `a_manifest_without_a_name_shows_its_id` (no `name` → `name == None`, `display_name() == id`).
  - `registry.rs`: `components_carry_their_plugins_display_name`: a registry of one plugin with `name: "Test codec"` gives `component(..).unwrap().plugin_name == "Test codec"`.
- [ ] **Step 2: Run** `cargo test -p celesta-plugin-host manifest && cargo test -p celesta-media registry`. Expected: FAIL (no field `name` / `plugin_name`).
- [ ] **Step 3: Implement.** Add the fields; `plugin_components` copies `plugin.name`; `crates/exporter/src/media.rs` sets `name: plugin.manifest.display_name().to_owned()` (likewise `audio-bench.rs`). Test and fixture literals: `name: None` for `Manifest`; for `RegisteredPlugin` the id, except `celesta_codec_test::registered()` → `"Test codec"` and `celesta_media_image::registered()` → `"Built-in"`. `plugins/ffmpeg/plugin.json`: `"name": "FFmpeg"` after `"id"`.
- [ ] **Step 4: Run** `cargo test -p celesta-plugin-host -p celesta-media -p celesta-exporter -p celesta-editor` and `cargo test --manifest-path plugins/Cargo.toml -p celesta-codec-ffmpeg`. Expected: PASS (editor label tests still pass: labels still read the id until Task 5).
- [ ] **Step 5: Docs.** In `05-plugins.md` (en and ja) add `name` to the manifest's fields: optional, the display name the editor's export page shows beside a component, the `id` when absent. Same in `skills/celesta/references/plugins.md` if it lists manifest fields.
- [ ] **Step 6: Commit** `feat(plugin-host): a plugin's display name in plugin.json`.

### Task 2: The ffmpeg plugin's labels

**Files:**
- Modify: `plugins/ffmpeg/src/components.rs` (`x264_settings`, tests)

**Interfaces:**
- Produces: x264's `preset` field labelled `"Speed preset"`, `rateControl` labelled `"Mode"` (group `"Rate control"` unchanged).

- [ ] **Step 1: Write the failing tests** in `components.rs`:
  - `x264_names_speed_preset_and_mode`: `x264_settings()` has `preset` with label `"Speed preset"` and `rateControl` with label `"Mode"` and group `Some("Rate control")`.
  - `no_field_is_labelled_as_its_group`: for every `VideoCodec::ALL` and `AudioCodec::ALL` `settings()`, no field has `Some(&field.label) == field.group.as_ref()`.
- [ ] **Step 2: Run** `cargo test --manifest-path plugins/Cargo.toml -p celesta-codec-ffmpeg components`. Expected: FAIL on both.
- [ ] **Step 3: Implement:** change the two labels in `x264_settings`.
- [ ] **Step 4: Run** the same command. Expected: PASS.
- [ ] **Step 5: Commit** `fix(ffmpeg): x264's "Speed preset" and rate control "Mode"`.

### Task 3: `ResolveWarning`

**Files:**
- Create: `crates/exporter/src/resolve_warning.rs`
- Modify: `crates/exporter/src/resolve.rs` (`ResolvedOutput::warnings`, every `warnings.push`, `fallback`, `extension_warning`, `settings`, `resolved_encoder`), `crates/exporter/src/lib.rs` (re-export), `crates/exporter/src/encode.rs:709`, `crates/exporter/src/still_output.rs:294`, tests that compare `resolved.warnings`

**Interfaces:**
- Produces, exported as `celesta_exporter::ResolveWarning`:

```rust
#[derive(Clone, Debug, PartialEq)]
pub enum ResolveWarning {
    /// A base's component that cannot be used; the default is used.
    Unavailable { base: String, kind: ComponentKind, text: String, reason: String },
    /// A base's audio encoder with a muxer that takes no audio.
    NoAudio { base: String, text: String, muxer: ComponentRef },
    /// Audio options given with a base that leaves the audio out.
    AudioOptionsIgnored { base: String },
    /// A base's value the schema does not take; the default is used.
    StaleValue { base: String, component: ComponentRef, key: String, label: String, message: String },
    /// A base's value for a key the schema does not have.
    UnknownSetting { base: String, component: ComponentRef, key: String },
    /// A named muxer that does not list the output's extension.
    Extension { muxer: ComponentRef, muxer_name: String, extension: String },
    /// The preset lookup's and the project default's warnings, in the CLI's words.
    Lookup(String),
}
```

  `base` is what the CLI's text calls the base (``preset `Web` ``). `ResolvedOutput::warnings: Vec<ResolveWarning>`.

- [ ] **Step 1: Write the failing test** `resolve_warnings_read_as_before` in `resolve_warning.rs`, one `assert_eq!(warning.to_string(), …)` per variant with today's text:
  - `Unavailable` → ``preset `Web`: ffmpeg/libx264 is unavailable (no libx264); using the default instead``
  - `NoAudio` → ``preset `Web`: ffmpeg/aac is unavailable (ffmpeg/ivf takes no audio); the audio is left out``
  - `AudioOptionsIgnored` → ``audio options are ignored: preset `Web` leaves the audio out``
  - `StaleValue` → ``preset `Web`: ffmpeg/libx264 crf must be at most 51; using its default``
  - `UnknownSetting` → ``preset `Web`: ffmpeg/libx264 has no setting `speed`; it is left out``
  - `Extension` → `ffmpeg/mp4 does not write .mov files; the file is written as MP4 anyway`
  - `Lookup("x")` → `x`
- [ ] **Step 2: Run** `cargo test -p celesta-exporter resolve_warning`. Expected: FAIL (no `ResolveWarning`).
- [ ] **Step 3: Implement** the enum and `Display`, then build each warning in `resolve.rs` as its variant (`StaleValue::label` from the schema field's `label`); `FoundPreset::warnings` and the project-default warning become `ResolveWarning::Lookup`. Callers that print (`encode.rs`, `still_output.rs`) map with `to_string()`. Existing tests keep their expected strings and compare `warnings.iter().map(ToString::to_string)`.
- [ ] **Step 4: Run** `cargo test -p celesta-exporter` and `cargo test -p celesta-mcp` (MCP's export tests read the report's warnings). Expected: PASS, with no expected string changed.
- [ ] **Step 5: Commit** `refactor(exporter): resolve warnings as ResolveWarning, the same text`.

### Task 4: Rename the dialog to the page

**Files:**
- Move: `crates/editor/src/export_dialog/` → `crates/editor/src/export_page/` (`git mv`)
- Modify: `crates/editor/src/main.rs` (`mod export_page;`), `view.rs`, `export_control.rs`, `plugins.rs`

**Interfaces:**
- Produces: `crate::export_page::view::{ExportPage, ExportPageEvent}`, `crate::export_page::model::ExportModel`; `EditorView::export_page` (still `Option<(Entity<ExportPage>, Subscription)>` until Task 7). Nothing else is renamed.

- [ ] **Step 1:** `git mv crates/editor/src/export_dialog crates/editor/src/export_page`; rename `ExportDialog` → `ExportPage`, `ExportDialogEvent` → `ExportPageEvent`, `ExportDialogModel` → `ExportModel`, the field `export_dialog` → `export_page`, and doc comments that say "dialog" where they mean this type.
- [ ] **Step 2: Run** `cargo test -p celesta-editor export_page`. Expected: PASS, the same tests as before the move.
- [ ] **Step 3: Commit** `refactor(editor): the export dialog's module becomes export_page`.

### Task 5: The model's labels and warnings

**Files:**
- Create: `crates/editor/src/export_page/wording.rs`
- Modify: `crates/editor/src/export_page/model.rs` (`label`, `warnings`, `initial_base`, `read`, `apply`), `crates/editor/src/export_page/mod.rs`, `crates/editor/src/export_page/view.rs` (where warnings are shown)

**Interfaces:**
- Consumes: `MediaComponentInfo::plugin_name` (Task 1), `ResolveWarning` (Task 3).
- Produces:

```rust
// wording.rs
pub(crate) enum PageWarning {
    Resolve(ResolveWarning),
    UnreadableFile { source: PresetSource, message: String },
    UnknownDefault { name: String },
}
impl ExportModel {
    /// `warning` as the page shows it under the Preset or Container row.
    pub(crate) fn warning_text(&self, warning: &PageWarning) -> String;
}
```

  `ExportModel::warnings: Vec<PageWarning>`; `initial_base(&self) -> (InitialBase, Vec<PageWarning>)`; component labels `"{name} · {plugin_name}"`, plus `" (project)"`.

- [ ] **Step 1: Write the failing tests** in `wording.rs` against a model over `celesta_codec_test::registered()` (component names from that fixture, plugin name "Test codec"); each asserts the exact text:
  - `StaleValue { label: "CRF", message: "must be at most 51", .. }` → `CRF must be at most 51. The default is used.`
  - `UnknownSetting { component: <video>, key: "speed", .. }` → `<video name> has no setting “speed”. It is left out.`
  - `Unavailable { kind: VideoEncoder, text: "<plugin>/<video>", reason: "no libx264", .. }` → `<video name> · Test codec is unavailable: no libx264. The default encoder is used.`; with `kind: Muxer` it ends `The default container is used.`
  - `NoAudio { text: <audio ref>, muxer: <muxer ref>, .. }` → `<audio label> is not used: <muxer label> takes no audio. The audio is left out.`
  - `AudioOptionsIgnored` → `Audio settings are ignored: the preset leaves the audio out.`
  - `Extension { muxer_name: "MP4", extension: "mov", .. }` → `MP4 doesn’t write .mov files. The file is written as MP4 anyway.`
  - `Lookup("the project's preset `web` hides …")` → the text with typographic quotes (`plain`).
  - `UnreadableFile { source: Project, message: "m" }` → `This project’s presets file couldn’t be read, so its presets aren’t listed. m`; `User` → `Your presets file couldn’t be read, so your presets aren’t listed. m`
  - `UnknownDefault { name: "Web" }` → `The project’s default preset “Web” isn’t in any presets file.`
  - `an_unknown_reference_is_worded_as_written`: `Unavailable { text: "gone/x264", .. }` → `gone/x264 is unavailable: … The default encoder is used.`; `UnknownSetting` for a component not in the registry uses its reference.
  - In `model.rs`: the existing label assertions become `"<name> · Test codec"` (and `" (project)"`).
- [ ] **Step 2: Run** `cargo test -p celesta-editor export_page`. Expected: FAIL (no `wording`, labels still use the id).
- [ ] **Step 3: Implement.** `label` uses `info.plugin_name`. `read` takes the file's `PresetSource` and pushes `UnreadableFile`; `initial_base` pushes `UnknownDefault`; `apply` maps `resolved.warnings` into `PageWarning::Resolve`. Component lookups by reference go through the model's existing options (label for `Unavailable`/`NoAudio`, name for `UnknownSetting`); `text` that does not parse or is not found is shown as written. Move `plain` into `wording.rs`; failures (`ExportError` text) keep going through `plain` unchanged. The view shows `model.warning_text(w)` where it showed `plain(w)`; `LAST_EXPORT` and `preset_label` stay as the `base` passed to the resolve.
- [ ] **Step 4: Run** `cargo test -p celesta-editor export_page`. Expected: PASS. Update view tests that asserted CLI-worded warnings to the new sentences (`warnings_about_last_call_it_your_last_export` becomes `warnings_name_no_preset`: no shown warning starts with `preset` or contains `your last export`).
- [ ] **Step 5: Commit** `feat(editor): export warnings and labels worded for the page`.

### Task 6: The Export panel

**Files:**
- Create: `crates/editor/src/export_page/view/layout.rs`
- Modify: `crates/editor/src/export_page/view.rs` (render, footer, events, focus, tests), `view/fields.rs`, `view/save.rs`, `crates/editor/src/actions.rs` (`ConfirmExport`), `crates/editor/src/main.rs` (key binding), `crates/editor/src/export_page/model.rs` (`exports_audio`), `crates/editor-theme/src/metrics.rs`

**Interfaces:**
- Produces:
  - `celesta_editor_theme::metrics::inspector_label_width() -> Rems` = `rems(7.0)`, with P7b's doc comment word for word ("Width of an inspector row's label lane, the same for every row so labels and values share one edge; wide enough for \"Corner radius\".").
  - `actions::ConfirmExport`, bound `KeyBinding::new("enter", ConfirmExport, Some("ExportPage"))`.
  - `ExportPageEvent::Close` (beside `Export`, `Loudness`, `ClearRange`).
  - `impl Focusable for ExportPage`; the root element has `key_context("ExportPage")` and handles `ConfirmExport` with `export`.
  - `ExportPage::set_range(&mut self, range: Option<SharedString>, settable: bool, cx: &mut Context<Self>)`: `settable` false for a React entry (no note).
  - `ExportModel::exports_audio(&self) -> bool`: false when the audio choice is None or the muxer takes no audio.
  - `layout.rs`: `row(id: SharedString, label: SharedString, control: AnyElement, lines: Vec<AnyElement>, cx: &App) -> AnyElement`, `section_header(title: &'static str, cx: &App) -> AnyElement`, `group_line(id: SharedString, title: SharedString, cx: &App) -> AnyElement`. Every row's label carries `debug_selector` `export-label-{id}` and its control `export-control-{id}`.
  - Removed: `open_export_dialog`, `EXPORT_DIALOG_WIDTH`, the `DialogFooter`/`DialogAction`/`DialogClose` footer.

- [ ] **Step 1: Write the failing tests** (`#[gpui_kit::test]`, the page as `Root`'s child in place of `Blank`; port the existing tests by dropping `open_export_dialog` and `has_active_dialog`):
  - `enter_in_the_page_exports` (the old `export_dialog_emits_the_chosen_output`): with the page focused, `simulate_keystrokes("enter")` emits one `Export` with the chosen choice; while "Checking…" it emits none.
  - `export_exports_while_the_preset_name_has_the_focus`: kept; the click target is `export-page-export`; Enter in the name saves and does not export.
  - `the_close_button_emits_close`: a click on `export-page-close` emits `Close`.
  - `loudness_is_disabled_without_audio`: audio "None" → the loudness `Select` is disabled and "No audio is exported." is rendered; choosing an audio encoder enables it.
  - `labels_share_one_edge`: with Video's Advanced open, `export-label-video-encoder`, `export-group-video-Rate control`, `export-video-advanced`'s icon and `export-label-video-<advanced field>` have the same `left()`.
  - `controls_fit_the_narrowest_panel`: window `22 × 13 = 286px` wide, every `export-control-*` bound's `right()` ≤ the window's width.
  - `metrics.rs`: `inspector_label_width() == rems(7.0)`.
- [ ] **Step 2: Run** `cargo test -p celesta-editor export_page && cargo test -p celesta-editor-theme`. Expected: FAIL.
- [ ] **Step 3: Implement** the panel as the spec's "The Export panel" lays it out: `PanelHeader::new("export-page", "Export")` with a ghost xsmall `IconName::Close` button (tooltip "Back to editing") as its action; a scrolling body (`overflow_y_scroll`, `flex_1`, `min_h_0`) with Preset, Container, Range rows, then `section_header("Video")` and `section_header("Audio")`, each with the Encoder row, the model's fields (groups as `group_line`, no indent), the Advanced disclosure (ghost xsmall button, `pl_0`, on the label edge), and Loudness at the Audio section's end; the footer pinned (band color, hairline above, `panel_padding()`), "Save as preset…" ghost leading, "Export…" primary trailing (`debug_selector` `export-page-export`), the blocked line above. Controls `small`. The Save as preset… form keeps its place above the footer's buttons and its behavior, laid out with `row` (Name, Save to) and its Save/Replace and close buttons after them; its Name wrapper stops `ConfirmExport` (as it stopped `Confirm`). The panel takes focus when shown (`focus_handle` held by `ExportPage`).
- [ ] **Step 4: Run** the same commands. Expected: PASS.
- [ ] **Step 5: Commit** `feat(editor): the export page's settings panel`.

### Task 7: The page in the editor

**Files:**
- Modify: `crates/editor/src/export_control.rs` (`ExportPageSlot`, `open_export_page`, `close_export_page`, `toggle_export_page`, `sync_export_range`, `choose_export_path`, `export_dialog_event`), `crates/editor/src/view.rs` (fields), `crates/editor/src/render.rs` (`workspace`, Escape chain), `crates/editor/src/panels.rs` (title bar), `crates/editor/src/plugins.rs` (`set_plugins`), `crates/editor/src/main.rs` and `crates/editor/src/lifecycle.rs` (`export_split`)

**Interfaces:**
- Consumes: `ExportPage`, `ExportPageEvent::Close`, `ExportPage::set_range` (Task 6).
- Produces:

```rust
#[derive(Default)]
pub(crate) struct ExportPageSlot { /* page: Option<(Entity<ExportPage>, Subscription)>, open: bool */ }
impl ExportPageSlot {
    pub(crate) fn is_open(&self) -> bool;
    pub(crate) fn page(&self) -> Option<&Entity<ExportPage>>;
    pub(crate) fn set(&mut self, page: Entity<ExportPage>, events: Subscription);
    pub(crate) fn show(&mut self);           // open only with a page set
    pub(crate) fn hide(&mut self);           // keeps the page
    pub(crate) fn export_started(&mut self); // hide, for an export that started
    pub(crate) fn clear(&mut self);          // drops the page, closed
}
pub(crate) enum LeftPanel { Hidden, Assets, Export }
pub(crate) fn left_panel(react: bool, export_open: bool) -> LeftPanel;
```

  `EditorView::export_page: ExportPageSlot`; `EditorView::export_split: Option<Entity<ResizableState>>`. `request_export` is renamed `open_export_page(&mut self, window, cx)`; `close_export_page(&mut self, window, cx)` focuses the editor's `focus_handle`.

- [ ] **Step 1: Write the failing tests** in `export_control.rs`:
  - `left_panel_follows_the_page`: `(false,false)→Assets`, `(false,true)→Export`, `(true,false)→Hidden`, `(true,true)→Export`.
  - `the_slot_opens_only_with_a_page`, `hiding_keeps_the_page`, `clearing_the_slot_closes_the_page`, `a_started_export_closes_the_page` (`#[gpui_kit::test]`, a page built as Task 6's tests build it).
- [ ] **Step 2: Run** `cargo test -p celesta-editor export_control`. Expected: FAIL.
- [ ] **Step 3: Implement.**
  - `open_export_page`: today's `request_export` guards and reuse rule, without `pause()`; `sync_export_range`; `show()`; focus the page. `ExportProject` and the File menu's Export… call it (no toggle).
  - Title bar: label "Export", `.selected(open).toggled(open)`, click → `toggle_export_page`. Exporting still shows Cancel export / Cancelling….
  - `ExportPageEvent::Close` and Escape → `close_export_page`. Escape: in `editor_shell`'s `capture_key_down`, after the drag/scrub/add checks, when `export_page.is_open()`.
  - `choose_export_path`: on a chosen path call `export_started()` before `start_export`; a cancelled panel leaves the slot alone.
  - `set_plugins`: `self.export_page.clear()`.
  - `sync_export_range(&self, cx)`: `set_range(self.export_range_label(), !self.is_react_preview(), cx)` on the page; called from `open_export_page`, `set_export_in`, `set_export_out`, `clear_export_range`.
  - `workspace(&self, window, cx)`: per `left_panel`, the first resizable panel shows the Export panel (visible) or Assets; while the page is open the row is `h_resizable("celesta-export-row")` with `export_split` and a first panel of `rems(26.)..` sized in pixels from `window.rem_size()` (default 26rem, range 22–34rem; a `physical:` comment as on the dock row); otherwise today's row with `dock_split`. `export_split` is created in `main.rs` beside `dock_split` and carried in `lifecycle.rs` as `dock_split` is.
- [ ] **Step 4: Run** `cargo test -p celesta-editor` and `cargo clippy -p celesta-editor --all-targets -- -D warnings`. Expected: PASS, no warnings.
- [ ] **Step 5: Commit** `feat(editor): export from a page beside the viewer and the timeline`.

### Task 8: Verification and documentation

**Files:**
- Modify: `packages/website/content/{en,ja}/docs/02-(guides)/09-export.md`, `packages/website/content/{en,ja}/docs/01-(getting-started)/04-react-compositions.md`, `skills/celesta/references/export.md`, `docs/superpowers/specs/2026-11-08-media-components-design.md` (§ Editor)
- Create: `docs/handoff/2026-11-11-p7d-export-page.md`

- [ ] **Step 1: After screenshots.** Same scene and window as the before shots: `p7d-after-1-default.png` … `p7d-after-4-save.png`, the same four states on the page. Compare each pair; nothing from the dialog is missing.
- [ ] **Step 2: By hand**, each with the result written down for the handoff note:
  - Export → save panel cancelled → the page as it was; Export → a path → the export starts, the editor is back in editing, progress in the status bar, the file plays.
  - Escape with the Container `Select` open closes the menu only; a second Escape closes the page; reopening keeps an edited CRF and the open Advanced.
  - `i`/`o` on the timeline while the page is open update its Range row; Clear empties it.
  - Reload Plugins with the page open: the page closes and Assets is back.
  - A React entry (`examples/greenscreen/composite.tsx` is one): the left panel appears only with the page; Range shows "Whole composition" with no note.
  - The Export panel dragged to 22rem and 34rem: labels truncate with tooltips, nothing overlaps.
- [ ] **Step 3: Interface zoom.** Temporarily set `metrics::BASE_FONT_SIZE` to 12 and 16, run, screenshot the page, set it back. The footer stays pinned and the body scrolls at 16.
- [ ] **Step 4: Side by side.** One image of the page beside the reference study's Resolve Deliver and Premiere Export screenshots, for the pull request.
- [ ] **Step 5: Docs.** `09-export.md` (en, ja): the export page (Export in the title bar or File → Export…, leaving with Export, the X or Escape, the panel's rows, Range set with In and Out on the timeline, Export… → save panel → back to editing), "Speed preset" and "Mode" in the x264 text, labels "· FFmpeg". `04-react-compositions.md` (en, ja): its Export… sentence. `skills/celesta/references/export.md`: where it describes the dialog. The media-components spec § Editor: one line "P7d (`docs/superpowers/specs/2026-11-11-p7d-export-page-design.md`) made the dialog a page."
- [ ] **Step 6: Handoff note** `docs/handoff/2026-11-11-p7d-export-page.md`: Specification (what was built, by file), Deviations from the spec, Checks (commands and results, the screenshots' names, the by-hand results), Not done (the `InspectorRow` merge with P7b, the Plugins panel's display name, P3's remaining Not done items).
- [ ] **Step 7: Run** `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo test --manifest-path plugins/Cargo.toml -p celesta-codec-ffmpeg`, and `pnpm --filter @celesta.studio/website check` (the docs' link and content check). Expected: all pass.
- [ ] **Step 8: Commit** `docs: the export page in the guide, the skill and the handoff note`.
