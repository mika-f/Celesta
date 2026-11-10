# P7c Timeline and Transport Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One timecode, a thinned three-tier ruler, icon-only transport and
zoom buttons, fixed-lane track headers, clips with a name strip, and the
timeline's and meter's geometry on `rem` tokens.

**Architecture:** `celesta_editor_theme::metrics` gains the timeline's `Rems`
tokens and the theme gains `clip_label_fill`. A new `TimelineGeometry` value
(built from `window.rem_size()` each render, last one kept on `EditorView`)
carries the same sizes in `Pixels` for hit tests, lane math and painting.
The track header and the clip block become `RenderOnce` components in their
own modules so they can be UI-tested without an `EditorView`.

**Tech Stack:** Rust, gpui-kit 0.9 / gpui-pre 0.3.11, `cargo test`
(`#[gpui_kit::test]`, `VisualTestContext`, `debug_selector`).

**Spec:** `docs/superpowers/specs/2026-11-10-p7c-timeline-design.md`

## Global Constraints

- Design direction (P7a): flat bands and one hairline; rows in fixed lanes;
  monospace for numbers and identifiers; borderless `ghost` chrome buttons;
  color only for data or emphasis; radius 0; no gradients; no uppercase
  transforms; lucide icons only (no glyph arrows).
- Raw color values only in `crates/editor-theme`; editor code reads
  `cx.theme()` roles and theme-crate functions.
- `style_guard.rs` already forbids `text_xs(` and `text_size(px(` without
  `physical:` and labelled `.xsmall()` buttons (use
  `widgets::CompactLabel::compact_label`). Task 3 adds: no `px(` in
  `timeline.rs`, `timeline_header.rs` or `timeline_clip.rs` unless the line
  says `physical:`; Task 8 adds `meter.rs` to that list.
- Text roles: `text_base` body/values/names, `text_sm` labels, monospace
  `text_lg` the timecode.
- No new features and no behavior changes: scrub, zoom, pan, band and scene
  drags, In/Out, mute/solo, selection behave as today.
- Build env: `export PKG_CONFIG_PATH="$(brew --prefix ffmpeg@8)/lib/pkgconfig"`
  and `PATH="/Users/natsuneko/Library/Application Support/mbx/bin:$HOME/.cargo/bin:$HOME/.local/share/mise/installs/node/26/bin:$HOME/.local/share/mise/installs/pnpm/12/bin:$PATH"`;
  builds and launches run with the sandbox off.
- Commits are signed through 1Password; on a signing failure stop and ask
  the user to unlock it. Never amend. Every commit message ends with
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Test command for the crates touched:
  `cargo test -p celesta-editor-theme -p celesta-editor` (the editor's
  unit tests live in `crates/editor/src/tests.rs`, `src/tests/*.rs` and
  per-module `#[cfg(test)]` blocks).

## Review Focus

1. **A timeline squeezed to its minimum height (the body split's 180px
   floor):** the meter's dB labels drop to a sparser set instead of
   overlapping; the track area scrolls. → Task 8 tests `scale_marks` at
   small heights.
2. **A clip narrower than its name:** the name is clipped by the clip, the
   clip keeps its 2px minimum width, and the strip never draws outside it.
   → Task 6 UI test with a 40px-wide clip.
3. **Mixed track kinds in one project (overlay above audio, a disabled and
   locked track):** names and M/S line up across rows. → Task 5 UI test.
4. **Before the lanes are painted once:** `frame_x` and
   `timeline_fraction_at` fall back to the stored geometry at the base
   font size, not to a stale 220px. → Task 2 test of
   `TimelineGeometry::default()`.
5. **A composition over an hour long or zoomed to single frames:** ruler
   labels keep at least 12rem apart and `mid` is absent or valid. → Task 4
   tests at 1 hour and at 40px per frame.

---

### Task 1: Timeline tokens and the clip label color

**Files:**
- Modify: `crates/editor-theme/src/metrics.rs`
- Modify: `crates/editor-theme/src/lib.rs` (beside `clip_fill`)

**Interfaces:**
- Produces (`celesta_editor_theme::metrics`, all `-> Rems`):
  `track_header_width()` 17rem, `ruler_height()` 2.25rem,
  `video_track_height()` 3rem, `audio_track_height()` 4rem,
  `clip_inset()` 0.25rem, `clip_label_height()` 1.25rem,
  `chapter_lane_height()` 1.75rem, `meter_column_width()` 8.75rem,
  `volume_slider_width()` 6.75rem.
- Produces (`celesta_editor_theme`): `pub fn clip_label_fill(kind: TrackKind) -> Hsla`.

- [ ] **Step 1: Write the failing tests** in `metrics.rs`'s `tests`:

```rust
#[test]
fn timeline_tokens_keep_their_order() {
    let px = |r: Rems| r.0 * BASE_FONT_SIZE;
    // The strip and at least a strip-high body fit a video clip.
    assert!(px(clip_label_height()) * 2.0 <= px(video_track_height()) - 2.0 * px(clip_inset()) + 0.01);
    assert!(px(audio_track_height()) > px(video_track_height()));
    assert!(px(ruler_height()) >= px(list_row_height()));
    assert!(px(meter_column_width()) >= px(volume_slider_width()) + 2.0 * px(panel_padding()));
}
```

and in `lib.rs`'s `tests`:

```rust
#[test]
fn clip_labels_are_darker_than_their_clips() {
    for kind in [TrackKind::Video, TrackKind::Audio, TrackKind::Overlay, TrackKind::Dialogue] {
        let fill: Hsla = rgb(clip_fill(kind)).into();
        assert!(clip_label_fill(kind).l < fill.l, "{kind:?}");
    }
}
```

- [ ] **Step 2: Run** `cargo test -p celesta-editor-theme` — expect compile errors (functions missing).
- [ ] **Step 3: Implement** the nine token functions (doc comment each, naming what it sizes) and `clip_label_fill`: one `const fn clip_label_fill_hex(kind) -> u32` table (the kind's color one step darker, about 25% lower lightness; e.g. video `0x3a_60_b8`, audio `0x1d_7d_51`, overlay `0x77_44_8c`, dialogue `0xb0_6c_1d`), wrapped as `Hsla`. Values may be tuned at the screenshot step (Task 9).
- [ ] **Step 4: Run** `cargo test -p celesta-editor-theme` — PASS.
- [ ] **Step 5: Commit** `feat(editor-theme): timeline tokens and the clip label color`.

### Task 2: `TimelineGeometry`

**Files:**
- Create: `crates/editor/src/timeline_geometry.rs` (register `mod timeline_geometry;` in `main.rs`)
- Modify: `crates/editor/src/view.rs` (field + initializer near `timeline_lane_bounds`, line ~295/584)
- Modify: `crates/editor/src/timeline.rs:304-330` (`frame_x`, `timeline_fraction_at` fallbacks)

**Interfaces:**
- Consumes: Task 1 tokens.
- Produces:

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TimelineGeometry {
    pub rem: Pixels,
    pub track_header_width: Pixels,
    pub ruler_height: Pixels,
    pub video_track_height: Pixels,
    pub audio_track_height: Pixels,
    pub clip_inset: Pixels,
    pub clip_label_height: Pixels,
    pub chapter_lane_height: Pixels,
    pub footer_height: Pixels,      // metrics::toolbar_row_height()
    pub section_divider: Pixels,    // 0.25rem, video/audio divider
    pub label_spacing: Pixels,      // 12rem, ruler labels
    pub minor_tick_spacing: Pixels, // 0.75rem
    pub minor_tick: Pixels,         // 0.375rem
    pub playhead_cap_half: Pixels,  // 0.375rem (today 6px at 16)
    pub playhead_cap_height: Pixels,// 0.75rem (today 12px)
    pub playhead_cap_shoulder: Pixels, // 0.3125rem (today 5px)
}
impl TimelineGeometry { pub fn new(rem: Pixels) -> Self }
impl Default for TimelineGeometry { /* new(px(metrics::BASE_FONT_SIZE)) */ }
```

  `EditorView.timeline_geometry: Cell<TimelineGeometry>` (default), written at the top of `timeline()` from `window.rem_size()`.

- [ ] **Step 1: Write the failing tests** in `timeline_geometry.rs`:
  - `sizes_are_their_tokens_times_the_rem`: for `rem` in `[13.0, 16.0]`, `new(px(rem)).track_header_width == px(17.0 * rem)`, `ruler_height == px(2.25 * rem)`, `video_track_height == px(3.0 * rem)`, `label_spacing == px(12.0 * rem)`.
  - `the_default_is_the_base_font_size`: `TimelineGeometry::default() == TimelineGeometry::new(px(metrics::BASE_FONT_SIZE))` and its `track_header_width` is `px(221.0)`.
- [ ] **Step 2: Run** `cargo test -p celesta-editor timeline_geometry` — FAIL (module missing).
- [ ] **Step 3: Implement** the struct; add the `EditorView` field; `timeline()` needs `window` — change its signature to `timeline(&self, window: &Window, cx: &mut Context<Self>)` and update the caller in `render.rs:240`. Replace `TRACK_HEADER_WIDTH` in the two fallbacks with `self.timeline_geometry.get().track_header_width`.
- [ ] **Step 4: Run** the tests — PASS; `cargo build -p celesta-editor` compiles.
- [ ] **Step 5: Commit** `feat(editor): TimelineGeometry from the window's rem`.

### Task 3: Timeline layout on tokens, and the pixel guard

**Files:**
- Modify: `crates/editor/src/style_guard.rs`
- Modify: `crates/editor/src/timeline.rs` (all of `timeline()`, `ruler_row`, `track_row`, `react_composition_row`, `chapter_row`, `timeline_footer`, `selection_band`, `band_grip_overlay`, `set_timing_overlay`, `chapter_end_grip`; module docs)
- Modify: `crates/editor/src/viewer.rs` (`audio_panel` header height)

**Interfaces:**
- Consumes: Task 1 tokens, Task 2 `TimelineGeometry` (pass `geometry: TimelineGeometry` into the row builders that place things by pixels).
- Produces: the constants `TRACK_HEADER_WIDTH`, `RULER_HEIGHT`, `VIDEO_TRACK_HEIGHT`, `AUDIO_TRACK_HEIGHT`, `CHAPTER_LANE_HEIGHT`, `FOOTER_HEIGHT` are gone; `LANE_GUTTER` stays `pub(crate) const` with a `physical:` doc line.

- [ ] **Step 1: Write the failing test** `pixel_layout_stays_out_of_the_timeline` in `style_guard.rs`: for each file in `const PIXEL_FREE: &[&str] = &["timeline.rs", "timeline_header.rs", "timeline_clip.rs"]` that exists under `src/`, every line containing `px(` must contain `physical:`; collect `file:line` offenders and assert empty (same reporting as `text_sizes_stay_on_the_type_roles`). Skip `#[cfg(test)]` modules: stop scanning a file at its first line equal to `#[cfg(test)]`.
- [ ] **Step 2: Run** `cargo test -p celesta-editor style_guard` — FAIL listing the timeline's `px(` lines.
- [ ] **Step 3: Migrate** `timeline.rs`: layout widths/heights through `metrics` `Rems` (`.w(metrics::track_header_width())`, `.h(metrics::video_track_height())`, `.h(metrics::toolbar_row_height())` for the footer, `.top(metrics::clip_inset())`/`.bottom(...)`); pixel-placed overlays (`track header width + x`, chapter lanes `inset + lane × lane height`, the composition label's `bottom`) from `geometry`; the video/audio divider `geometry.section_divider`. Lines that stay pixels get a trailing `// physical: <why>`: hairlines and 1px lines, `min_w(px(2.0))` (smallest visible clip), `LANE_GUTTER` (scrollbar), `EDGE_GRIP`-derived grip widths (hit tolerance), `scroll_tracks_pixels` (scroll offset math), the lane-width spacers in `set_timing_overlay` (measured lane pixels). Replace the `//! physical:` module paragraph with one sentence: sizes come from `metrics` and `TimelineGeometry`; pixel lines say why. `audio_panel`'s header: `.h(metrics::ruler_height())`.
- [ ] **Step 4: Run** `cargo test -p celesta-editor` — PASS, including `pixel_layout_stays_out_of_the_timeline`.
- [ ] **Step 5: Commit** `refactor(editor): timeline geometry on rem tokens`.

### Task 4: The ruler: one timecode, three tiers, wider labels

**Files:**
- Modify: `crates/editor/src/timecode.rs` (`RulerScale`, `ruler_scale`, tests)
- Modify: `crates/editor/src/timeline.rs` (`ruler_row`, `paint_ruler`)

**Interfaces:**
- Produces: `RulerScale { major: i64, mid: Option<i64>, minor: i64 }`;
  `ruler_scale(pixels_per_frame: f64, fps: i64, min_label_px: f64, min_tick_px: f64) -> RulerScale` (signature unchanged).

- [ ] **Step 1: Write the failing tests** in `timecode.rs`:
  - `mid_ticks_split_the_major`: with `fps = 60`, `min_label_px = 156.0`, `min_tick_px = 9.75`:
    `ruler_scale(1000.0 / 600.0, …) == RulerScale { major: 120, mid: Some(60), minor: 10 }`;
    `ruler_scale(0.3, …) == RulerScale { major: 600, mid: Some(300), minor: 60 }`;
    `ruler_scale(3.0, …) == RulerScale { major: 60, mid: Some(30), minor: 5 }`.
  - `a_ten_second_composition_labels_every_two_seconds`: the first case above (a 10 s, 60 fps composition across 1000px): `major == 120`.
  - `no_mid_when_minor_is_the_major`: `ruler_scale(3.0, 60, 156.0, 156.0) == RulerScale { major: 60, mid: None, minor: 60 }`.
  - `long_and_single_frame_rulers_stay_valid` (Review Focus 5): at 1 hour across 1000px (`pixels_per_frame = 1000.0 / 108_000.0`, fps 30) `major as f64 * ppf >= 156.0` and `mid.map_or(true, |m| major % m == 0 && m % minor == 0)`; at 40px per frame the same invariants hold.
  - Update the existing `RulerScale { major, minor }` literals to include `mid`.
- [ ] **Step 2: Run** `cargo test -p celesta-editor timecode` — FAIL.
- [ ] **Step 3: Implement** `mid`: the largest step in the ladder with `minor < step < major`, `major % step == 0` and `step % minor == 0`; `None` if none.
- [ ] **Step 4: Update `ruler_row` and `paint_ruler`:**
  - delete the corner's timecode child; the corner keeps width, `border_r_1` and the band color (`debug_selector(|| "timeline-ruler-corner".into())`);
  - call `ruler_scale(ppf, fps, geometry.label_spacing.into(), geometry.minor_tick_spacing.into())`;
  - ticks become `(x, Tier)` with `enum Tier { Major, Mid, Minor }`; painted heights: Major = ruler height, Mid = half, Minor = `geometry.minor_tick`;
  - labels: `.font_family(mono)`, `.text_sm()`, muted, `.top(rems(0.125))`, `.ml(rems(0.25))`;
  - playhead cap from `geometry.playhead_cap_*`; the ruler's In/Out band `.h(rems(0.375))`.
- [ ] **Step 5: Run** `cargo test -p celesta-editor` — the timecode tests PASS.
- [ ] **Step 6: Commit** `feat(editor): one timecode and a three-tier ruler`.

### Task 5: Track headers in fixed lanes

**Files:**
- Create: `crates/editor/src/timeline_header.rs` (register in `main.rs`)
- Modify: `crates/editor/src/timeline.rs` (`track_header`, the header halves of `react_composition_row` and `chapter_row` use it)

**Interfaces:**
- Produces:

```rust
#[derive(IntoElement)]
pub(crate) struct TrackHeader { /* private */ }
impl TrackHeader {
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>, name: impl Into<SharedString>, swatch: Hsla) -> Self;
    pub fn count(self, count: usize) -> Self;          // Chapters
    pub fn disabled(self, disabled: bool) -> Self;     // EyeOff slot + muted name
    pub fn locked(self, locked: bool) -> Self;         // Lock slot
    pub fn level(self, level: Option<f32>) -> Self;    // None = empty level slot
    pub fn mix(self, mix: Option<MixButtons>) -> Self; // None = empty M/S slots
    pub fn mix_lane(self, shown: bool) -> Self;        // false = no M/S lane at all (React preview)
}
pub(crate) struct MixButtons { pub muted: bool, pub solo: bool, pub on_mute: ClickHandler, pub on_solo: ClickHandler }
type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
```

  Lanes and widths per the spec's table: leading `metrics::panel_padding()`, label `rems(2.0)` monospace `text_sm` muted, swatch `rems(0.5)` square radius 0, name `flex_1 min_w_0` `text_base` ellipsized, state two `size_3` slots each `rems(0.75)` wide, level `rems(0.25) × rems(1.25)`, M and S `xsmall ghost compact_label` buttons (each in a fixed `size_5` slot), trailing `rems(0.5)`. Debug selectors: `{id}-name`, `{id}-mute`, `{id}-solo`, `{id}-level-slot`. Element ids for the buttons keep today's `track-mute-{track}`/`track-solo-{track}`.

- [ ] **Step 1: Write the failing UI test** (`#[gpui_kit::test]`, host window like `panel_header.rs`'s tests, 400×200): render a column of two headers, `TrackHeader::new("a", "V2", "Titles", red).disabled(true).locked(true).level(Some(0.5)).mix(Some(..))` and `TrackHeader::new("b", "A1", "Music", green).level(None).mix(None)`; test `lanes_line_up_across_rows`: `debug_bounds("a-name").origin.x == debug_bounds("b-name").origin.x`, `a-mute` exists, and `debug_bounds("a-name").size.width == debug_bounds("b-name").size.width` (the empty slots keep the name lane equal). Second test `without_a_mix_lane_the_name_widens`: a header with `.mix_lane(false)` has a wider `-name` than one with `.mix(None)`.
- [ ] **Step 2: Run** `cargo test -p celesta-editor timeline_header` — FAIL (module missing).
- [ ] **Step 3: Implement** `TrackHeader` as `RenderOnce`; tooltips on the state icons ("Disabled in the project", "Locked in the project") and the M/S buttons ("Mute in the preview", "Solo in the preview") as today. Replace `track_header`'s body with it (listener closures call `toggle_track_mute`/`toggle_track_solo` through `cx.listener` wrapped into `Rc`), `mix_lane(!self.is_react_preview())`, `level(None)` for overlay tracks and for React videos (today's condition), `mix(None)` for overlay tracks. Chapters row: `TrackHeader::new("chapters", "", "Chapters", accent).count(n).mix(None)`; Composition row: `TrackHeader::new("composition", "V1", "Composition", overlay color).mix(None)`.
- [ ] **Step 4: Run** `cargo test -p celesta-editor` — PASS.
- [ ] **Step 5: Commit** `feat(editor): track headers in fixed lanes`.

### Task 6: Clips with a name strip

**Files:**
- Create: `crates/editor/src/timeline_clip.rs` (register in `main.rs`)
- Modify: `crates/editor/src/timeline.rs` (`clip_block`, the composition block in `react_composition_row`, chapter blocks' radius, `paint_waveform` moves to the new module)

**Interfaces:**
- Produces:

```rust
#[derive(IntoElement)]
pub(crate) struct ClipBlock { /* private */ }
impl ClipBlock {
    pub fn new(id: impl Into<ElementId>, name: impl Into<SharedString>, kind: TrackKind) -> Self;
    pub fn label_offset(self, fraction: f32) -> Self;   // today's label_offset
    pub fn selected(self, selected: bool) -> Self;
    pub fn enabled(self, enabled: bool) -> Self;
    pub fn waveform(self, bars: Vec<f32>) -> Self;
    pub fn on_click(self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self;
}
```

  The block fills its parent (`size_full`, `relative`, `overflow_hidden`, no radius); the caller positions it (`absolute`, `left/w relative`, `top/bottom metrics::clip_inset()`, `min_w(px(2.0)) // physical`). Inside: the strip `w_full`, `h(metrics::clip_label_height())`, `bg(theme::clip_label_fill(kind))`, `overflow_hidden`, `debug_selector(|| format!("{id}-label"))`, holding the name (`text_sm`, `theme::waveform()` color, `whitespace_nowrap`); the body below in `rgb(theme::clip_fill(kind))` with the waveform canvas inset `rems(0.125)` top and bottom; the selection border over everything (`border_2`, `theme::clip_selected_border()`); `opacity(0.4)` when disabled.

- [ ] **Step 1: Write the failing UI tests**:
  - `the_name_strip_is_its_token_tall`: a 300×40 parent holding `ClipBlock::new("c", "score.wav", TrackKind::Audio)`; `debug_bounds("c-label").size.height == px(metrics::clip_label_height().0 * BASE_FONT_SIZE)`.
  - `a_narrow_clip_clips_its_name` (Review Focus 2): a 40px-wide parent and a long name; the strip stays inside the clip — `debug_bounds("c-label").size.width <= px(40.0)`.
- [ ] **Step 2: Run** `cargo test -p celesta-editor timeline_clip` — FAIL.
- [ ] **Step 3: Implement** `ClipBlock`; use it in `clip_block` and for the composition block (name = entry file name; the bands, grips and Set timing overlay stay siblings in the lane as today). Chapter blocks: drop `rounded_sm`, heights from `metrics::chapter_lane_height()`.
- [ ] **Step 4: Run** `cargo test -p celesta-editor` — PASS.
- [ ] **Step 5: Commit** `feat(editor): clips with a name strip`.

### Task 7: Transport and footer icons, rem breakpoints

**Files:**
- Modify: `crates/editor/src/icons.rs` (`LucideIcons` list)
- Modify: `crates/editor/src/viewer.rs` (`transport_layout`, constants, `transport_bar`, tests at ~1972)
- Modify: `crates/editor/src/timeline.rs` (`timeline_footer`)

**Interfaces:**
- Produces: `fn transport_layout(width: Pixels, rem: Pixels) -> (bool, bool)`; constants `TRANSPORT_FULL_WIDTH: f32 = 51.25` and `TRANSPORT_MARKS_WIDTH: f32 = 37.5` (in `rem`s).

- [ ] **Step 1: Update the failing test** `the_transport_bar_collapses_at_its_own_widths`: at `rem = px(13.0)`: `transport_layout(px(666.25), rem) == (true, true)`, `px(666.0)` → `(false, true)`, `px(487.5)` → `(false, true)`, `px(487.0)` → `(false, false)`; at `rem = px(16.0)`: `px(820.0)` → `(true, true)`, `px(819.0)` → `(false, true)`.
- [ ] **Step 2: Run** `cargo test -p celesta-editor transport` — FAIL.
- [ ] **Step 3: Implement**: `transport_layout` divides `width` by `rem`; `transport_bar` gets `window` (pass `window.rem_size()`; `viewer_panel` takes `window` from `render.rs`). Mark In: `.icon(IconName::ArrowRightFromLine)`, Mark Out: `.icon(IconName::ArrowRightToLine)` (both stay `small ghost`, `selected`, same tooltips). Add `ArrowRightFromLine`, `ArrowRightToLine`, `UnfoldHorizontal` to `LucideIcons`. Footer: zoom to fit becomes `.icon(IconName::UnfoldHorizontal)` with the same `tooltip_with_action("Zoom to fit", …)`.
- [ ] **Step 4: Run** `cargo test -p celesta-editor` — PASS.
- [ ] **Step 5: Commit** `feat(editor): icon In, Out and Fit; transport breakpoints in rem`.

### Task 8: The meter column on rem

**Files:**
- Modify: `crates/editor/src/meter.rs`
- Modify: `crates/editor/src/viewer.rs` (`audio_panel`, `volume_slider`, drop `AUDIO_PANEL_WIDTH`, `VOLUME_SLIDER_WIDTH`)
- Modify: `crates/editor/src/view.rs` (`meter_height: Rc<Cell<Option<Pixels>>>` beside `transport_width`)
- Modify: `crates/editor/src/audio.rs` (`MasterVolumeDrag` gains `slider_width: f64`), `audio_control.rs`, `waveform.rs` (`master_volume_from_drag(start_volume: f64, delta_pixels: f64, slider_width: f64) -> f64`), `tests.rs:673`

**Interfaces:**
- Produces: `pub fn scale_marks(height: Pixels, line_height: Pixels) -> &'static [f32]` returning one of `DENSE = [0.0, -6.0, -12.0, -18.0, -24.0, -36.0, -48.0, -60.0]`, `TWELVE = [0.0, -12.0, -24.0, -36.0, -48.0, -60.0]`, `SPARSE = [0.0, -30.0, -60.0]`: the first whose smallest adjacent gap (`height × gap_db / 60`) is `>= line_height`, else `SPARSE`.
  `audio_meter(reading: Option<MeterReading>, height: Option<Pixels>, line_height: Pixels, cx: &App)`; `audio_panel(&self, window: &Window, cx)` (caller in `render.rs` passes `window`).

- [ ] **Step 1: Write the failing tests** in `meter.rs`: `scale_marks_thin_with_the_column` — line height `px(16.0)`: height `px(200.0)` → `DENSE` (6 dB gap = 20px); `px(120.0)` → `TWELVE` (6 dB = 12px, 12 dB = 24px); `px(60.0)` → `SPARSE`; `px(10.0)` → `SPARSE` (Review Focus 1). Update `tests.rs` `master_volume_from_drag` cases to pass `88.0` and keep the same expected values.
- [ ] **Step 2: Run** `cargo test -p celesta-editor meter` — FAIL.
- [ ] **Step 3: Implement**: `scale_marks`; scale labels and peak readouts monospace `text_sm` muted (drop `text_size(px(9.0))` and the `physical:` markers); the scale column width `rems(1.75)`; each label `line_height(line_height)` and `mt(-line_height / 2)` so it centers on its mark; `audio_panel` computes `line_height = window.rem_size() * 1.25` and passes `self.meter_height.get()`; add `"meter.rs"` to `PIXEL_FREE` in `style_guard.rs`; the volume readout ("100%") monospace; measure the bars' height into `EditorView.meter_height` with a `canvas` prepaint; `audio_panel` width `metrics::meter_column_width()`; `volume_slider` width `metrics::volume_slider_width()` and its track/thumb insets in `rems` (thumb `rems(0.625)`, track `rems(0.25)` tall, thumb left `relative(volume / 2)` with `-ml` half the thumb); `begin_master_volume_drag` stores `slider_width = f64::from(window.rem_size()) * f64::from(metrics::volume_slider_width().0)`.
- [ ] **Step 4: Run** `cargo test -p celesta-editor -p celesta-editor-theme` — ALL PASS, including `pixel_layout_stays_out_of_the_timeline` and `text_sizes_stay_on_the_type_roles`.
- [ ] **Step 5: Commit** `feat(editor): the meter column on rem, labels at text_sm`.

### Task 9: Verification and documentation

**Files:**
- Create: `docs/handoff/2026-11-10-p7c-timeline.md`

- [ ] **Step 1: Lint**: `cargo clippy -p celesta-editor-theme -p celesta-editor -- -D warnings` and `cargo fmt --all --check` — both clean.
- [ ] **Step 2: Zoom check**: set `BASE_FONT_SIZE` to 12.0, build, run greenscreen and look; then 16.0; restore 13.0 (do not commit the change). Ruler, header lanes, strips, transport breakpoints and meter labels hold; record findings in the handoff.
- [ ] **Step 3: After screenshots**: build the editor, copy the binary out of `$(readlink target)/debug/` to the scratchpad, run from the repo root with `CELESTA_PLUGIN_PATH="$PWD/plugins/dist"` on `examples/greenscreen/composite.tsx` and on `examples/36-days/film.tsx` (its `score.wav` is already generated), ask the user to click "Trust and Open", capture `screencapture -x -o -l <id>` (id from the `CGWindowListCopyWindowInfo` script). Compare with the "before" captures (`scratchpad/before/before-greenscreen.png`, `before-36days.png`); tune `clip_label_fill` if the strip does not read as one step darker, committing that as its own `fix(editor-theme)` commit.
- [ ] **Step 4: User guide**: `grep -rn "\*\*In\*\*\|\*\*Out\*\*\|\*\*Fit\*\*" packages/website/content/{en,ja}`; change only passages that name them as text buttons (none expected).
- [ ] **Step 5: Handoff note** in the P7a note's shape: what changed (tokens table, `TimelineGeometry`, ruler, transport, headers, clips, meter), the pixel rules, tests, how it was checked (commands, screenshots not committed, zoom findings). Remove `examples/36-days/score.wav` (gitignored, generated) and check `git status` shows nothing under `examples/`.
- [ ] **Step 6: Commit** `docs: P7c handoff`.
