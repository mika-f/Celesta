# P7c: the timeline and transport (2026-11-10)

Third sub-project of P7, the editor's UI refinement. It builds on P7a
(`docs/superpowers/specs/2026-11-10-p7a-visual-foundation-design.md`) and
follows its design direction ("like the timeline"): flat bands and one
hairline, rows in fixed lanes, monospace for numbers and identifiers,
borderless ghost chrome buttons, color only for data or emphasis, radius 0,
no gradients, no uppercase transforms, lucide icons only. Decided with the
user one question at a time.

## Why

Measured against DaVinci Resolve 21 and Premiere Pro 2026 (the P7 reference
study, sections 6 and 10b) and the P7a branch's own screenshot:

- **The timecode shows twice**: large and white under the viewer, orange
  above the track headers. Orange is the brand accent, reserved for the
  playhead and the wordmark.
- **The ruler is crowded**: it takes the finest step whose labels stay 104px
  apart, so a short composition gets a full `HH:MM:SS:FF` every 10 frames or
  every second.
- **The transport mixes text and icons**: "In" and "Out" are text buttons
  beside icon buttons; the timeline footer's "Fit" does the same.
- **Track header lanes shift**: the disabled and locked icons appear only when
  they apply, pushing the level meter and M/S buttons sideways.
- **Clips are a flat block with the name drawn over the waveform**, with
  rounded corners (`rounded_sm` is a fixed radius, not the theme's 0).
- **The timeline's geometry is in pixels**: `TRACK_HEADER_WIDTH`,
  `RULER_HEIGHT`, the track heights and the meter column are `f32` pixel
  constants, the module is marked `//! physical:`, and P7a left them, the
  transport's width breakpoints, the meter column ("Volume 100%" gets tight at
  a 16px base) and the meter's 9px dB labels to P7c.

No features are added. The interaction model (scrub, zoom, pan, drags,
In/Out, mute/solo) is unchanged.

## Decisions

| Question | Decision |
| --- | --- |
| Which timecode stays | The transport's, under the viewer (P7a's timecode role: monospace `text_lg`, `foreground`). The ruler's corner loses its orange one. |
| How the ruler thins | Full `HH:MM:SS:FF` labels, spaced about twice a label's width; three tick tiers. |
| In / Out buttons | lucide icons: `arrow-right-from-line` (In), `arrow-right-to-line` (Out). The footer's "Fit" becomes `unfold-horizontal`. |
| Clip look | A name strip along the clip's top, one step darker than the clip; the body (and an audio clip's waveform) below it. |
| Track heights | About today's: video 3rem, audio 4rem. |
| Geometry on tokens | `rem` tokens in `metrics`, and a `TimelineGeometry` value built from the window's `rem` at render for hit tests and painting. |

## Tokens and geometry

### `celesta_editor_theme::metrics`

New functions, all returning `Rems` (pixel values at the 13px base):

| Token | Value | Today |
| --- | --- | --- |
| `track_header_width()` | 17rem ≈ 221px | 220px |
| `ruler_height()` | 2.25rem ≈ 29px | 30px |
| `video_track_height()` | 3rem ≈ 39px | 40px |
| `audio_track_height()` | 4rem ≈ 52px | 52px |
| `clip_inset()` | 0.25rem ≈ 3px (space above and below a clip) | 3px |
| `clip_label_height()` | 1.25rem ≈ 16px (the name strip) | — |
| `chapter_lane_height()` | 1.75rem ≈ 23px | 22px |
| `meter_column_width()` | 8.75rem ≈ 114px | 112px |
| `volume_slider_width()` | 6.75rem ≈ 88px | 88px |

The timeline footer uses the existing `toolbar_row_height()` (2rem ≈ 26px,
today 26px). Smaller in-row sizes (the label lane, the kind swatch, the
track level bar, the state-icon slots, the playhead cap, the ruler's In/Out
band) are `rem` values inside `TimelineGeometry` or GPUI's `rem` helpers;
they get no `metrics` function of their own unless the meter or viewer
shares them.

### `TimelineGeometry`

A new module, `crates/editor/src/timeline_geometry.rs`.

- `TimelineGeometry::new(rem: Pixels) -> TimelineGeometry` is a `Copy` value
  holding every timeline size above, and the in-row sizes, in `Pixels`.
- The timeline's render builds it once from `window.rem_size()` and passes it
  to whatever needs pixels: chapter lane offsets, the label overlays placed at
  `track header width + x`, the ruler's label and tick spacing, and the
  painted ruler ticks and playhead cap.
- Layout keeps using `Rems` (the `metrics` tokens) so the element tree and the
  pixel math come from the same numbers.
- `EditorView` keeps the last geometry in a `Cell`. The pre-paint fallbacks
  (`frame_x`, `timeline_fraction_at` before the lane has been painted once)
  read it instead of `TRACK_HEADER_WIDTH`.
- The volume slider maps a drag through its measured bounds instead of
  `VOLUME_SLIDER_WIDTH` (which `waveform.rs` reads today).

The `TRACK_HEADER_WIDTH`, `RULER_HEIGHT`, `VIDEO_TRACK_HEIGHT`,
`AUDIO_TRACK_HEIGHT`, `CHAPTER_LANE_HEIGHT`, `FOOTER_HEIGHT`,
`AUDIO_PANEL_WIDTH` and `VOLUME_SLIDER_WIDTH` constants go.

### What stays in pixels

Each such line carries a `physical:` comment saying why:

- one-pixel hairlines, the playhead line, the In/Out mark lines;
- `LANE_GUTTER` (10px): it keeps the lanes clear of gpui-base's scrollbar,
  which is pixel-sized (a 6px thumb inset 4px);
- gesture tolerances: `AXIS_LOCK_PIXELS`, the wheel's lines-to-pixels factor;
- `MAX_PIXELS_PER_FRAME`, the zoom limit (pointer precision);
- `FALLBACK_LANE_WIDTH`, the lane width assumed before the first paint.

The module's `//! physical:` note goes. A new `style_guard.rs` test fails on
any line in `timeline.rs` or `meter.rs` that contains `px(` without
`physical:`, so pixel layout does not come back.

## Timecode

- One timecode: the transport's, under the viewer. Unchanged in look:
  monospace `text_lg` in `foreground`, followed at full width by
  `/ <duration>` in monospace `text_sm`, `muted_foreground`.
- The ruler's corner, the cell above the track headers, becomes an empty band:
  the timeline's band color with the header column's hairline on its trailing
  edge.

## Ruler

- Labels: full `HH:MM:SS:FF` (`format_timecode`), monospace `text_sm`,
  `muted_foreground`, offset from their tick by `rem` spacing (today they use
  the platform font and pixel offsets).
- Label spacing: at least 12rem (≈ 156px at 13px; a label is ≈ 76px wide).
  Example: a 10-second, 60fps composition across a ≈ 1000px lane at zoom 1
  labels every 2 seconds instead of every second.
- Three tick tiers, painted from the ruler's bottom edge:
  - **major**: labelled, the ruler's full height;
  - **mid**: half the ruler's height;
  - **minor**: a short tick (≈ 0.375rem).
- `RulerScale` gains `mid`. `mid` is the coarsest step in the ruler's ladder
  that is smaller than `major`, divides it, and is a multiple of `minor`
  (2s → 1s, 10s → 5s, 1s at 60fps → 30 frames). With no such step there is
  no mid tier. The minor tier's minimum spacing is 0.75rem.
- The playhead cap (the downward pentagon) and the ruler's In/Out band take
  their sizes from `TimelineGeometry` in proportion to `rem`. The playhead and
  mark lines stay one pixel wide.

## Transport

The row's order is unchanged: the timecode on the leading side; In, Out |
go to start, previous frame, play, next frame, go to end | loop in the
center; the In–Out readout with its clear button, Safe areas and Select
layers on the trailing side.

- **In** and **Out** become `small` `ghost` icon buttons: In is
  `arrow-right-from-line` (|→, "from here"), Out is `arrow-right-to-line`
  (→|, "up to here"). They keep their tooltips ("Mark In", "Mark Out") with
  their shortcuts, and show `selected` while that mark is set. Both icons are
  added to `LucideIcons` in `icons.rs`.
- The width breakpoints move to `rem`: everything shows at ≥ 51.25rem; the
  In/Out buttons show at ≥ 37.5rem. These are today's 820px and 600px read as
  a design against a 16px `rem` (`metrics::scaled`), so they keep their
  proportion to the content. `transport_layout` takes the width in `rem`s.
- The row's height stays `metrics::scaled(44.0)`.

## Timeline footer

- Height `toolbar_row_height()`.
- Zoom out (−), zoom in (+) and zoom to fit as `xsmall` `ghost` icon
  buttons; "Fit" becomes lucide `unfold-horizontal` with the tooltip "Zoom to
  fit" and its shortcut. The zoom readout (`1.0×`) stays monospace
  `text_sm`, `muted_foreground`.
- The overview bar's thumb, its In/Out band and its playhead take their
  vertical insets from `TimelineGeometry`.

## Track headers

Every header row is a fixed set of lanes, so optional content never shifts
the others:

| Lane | Content | Width |
| --- | --- | --- |
| leading padding | — | `panel_padding()` |
| label | `V1`, `A1`: monospace `text_sm`, `muted_foreground` | 2rem |
| kind swatch | the clip color, radius 0 | 0.5rem |
| name | `text_base`, ellipsized; `muted_foreground` when the track is disabled | the rest |
| state | two slots: disabled (`EyeOff`), locked (`Lock`); `size_3` muted icons with their tooltips | fixed, reserved when empty |
| level | the track level bar | 0.25rem × 1.25rem |
| M, S | `xsmall` `ghost` buttons, `compact_label` | fixed |
| trailing padding | — | 0.5rem |

- An overlay track has no level bar and no M/S, but keeps their slots empty.
- In a React preview no row has M/S (its tracks are a view of the source), so
  the M/S lane is left out of every row; rows still line up with each other.
- The Chapters row and the React Composition row use the same lanes: Chapters
  shows its count after the name, as today.
- The selected row keeps its `list_active` background.
- The divider between video and audio tracks (today a 3px band) is 0.25rem
  from `TimelineGeometry`.

## Clips

- Radius 0 (no `rounded_sm`). Inset `clip_inset()` from the row's top and
  bottom; horizontal position and width as today.
- **Name strip**: along the top, `clip_label_height()` tall, filled with a new
  theme color `clip_label_fill(kind)`: the kind's clip color one step darker.
  The values are defined in `celesta-editor-theme` beside `clip_fill` and
  chosen during implementation against the screenshots. The name sits in the
  strip on one line, `text_sm`, in the waveform color (white); the existing
  offset that keeps the name readable when the clip starts left of the view
  stays.
- **Body**: below the strip, the kind's `clip_fill`. An audio clip's waveform
  is painted in the body. At the 13px base the video body is ≈ 16px and the
  audio waveform ≈ 29px tall.
- Selection keeps its 2px `clip_selected_border` outline around the whole
  clip; a disabled clip keeps opacity 0.4.
- The React Composition block has the same anatomy, with the entry's file
  name in the strip. Its selection bands, grips and "Set timing" overlay keep
  their current placement and behavior.
- Chapter blocks are not clips: they get radius 0 and the
  `chapter_lane_height()` lanes, no strip.

## Master meter column

- Width `meter_column_width()`; the "Master" header is `ruler_height()` tall,
  so it shares the ruler's line; padding on `rem`.
- dB scale labels: monospace `text_sm`, `muted_foreground` (today 9px,
  marked `physical:`). They thin with the column's height. The candidate sets,
  densest first:
  1. today's eight: 0, 6, 12, 18, 24, 36, 48, 60;
  2. every 12 dB: 0, 12, 24, 36, 48, 60;
  3. 0, 30, 60.

  The first set whose smallest gap between adjacent labels is at least the
  label's line height is drawn. `meter::scale_marks(height, line_height)` is a
  pure function returning that set. The column's height is measured the way
  the transport's width is (a `canvas` that records its bounds into a `Cell`;
  one frame behind, as the transport is).
- The peak readouts under the bars (`−∞`, `-12.3`): monospace `text_sm`,
  `muted_foreground`; the clipping red stays.
- Volume row: "Volume" (`text_sm`) and a monospace percentage, then the
  slider at `volume_slider_width()`. The slider's thumb and track sizes come
  from `rem`.

## Testing

Rust, `cargo test`:

- `celesta-editor-theme`:
  - the new tokens keep their order: the name strip plus a minimum body fits a
    video track inside its insets; the meter column holds the slider and its
    padding; the ruler is at least a list row tall;
  - `clip_label_fill(kind)` is darker than `clip_fill(kind)` for every kind.
- `timeline_geometry`: at a 13px and a 16px `rem`, every size is its token
  times the `rem`.
- `timecode`:
  - `RulerScale::mid` for 2s, 10s and 1s at 60fps majors, and no mid when no
    step qualifies;
  - with 12rem label spacing, a 10-second 60fps composition across ≈ 1000px
    gets a 2-second major.
- `viewer`: `transport_layout` in `rem`s; the breakpoints scale with a 13px
  and a 16px `rem`.
- `meter`: `scale_marks` picks each of the three sets at the heights where it
  should.
- `style_guard`: `px(` in `timeline.rs` and `meter.rs` only on lines that say
  `physical:`.
- UI integration tests (`VisualTestContext`, `debug_selector`; gpui-kit's
  `test-support` feature conflicts on taffy, as P7a found):
  - the playhead timecode is drawn once, under the viewer, and the ruler's
    corner has none;
  - a disabled and locked track and a plain track put their names, M and S
    buttons at the same x (fixed lanes);
  - a clip's name strip is `clip_label_height()` × `rem` tall.

## Verification

- Before and after screenshots of the same scene, as AGENTS.md requires:
  `examples/greenscreen/composite.tsx` with the project loaded, a 1440×900
  window, captured with `screencapture -x -o -l <window id>`. The "before"
  was taken from the P7a branch tip (`34a92a5b`) while this spec was written.
- A second before/after pair on `examples/36-days/film.tsx`, which shows the
  Chapters row and the score's audio track with its waveform. Its
  `score.wav` is generated first with `python3 examples/36-days/make-score.py`
  (standard library only; the file is gitignored).
- Interface zoom: `BASE_FONT_SIZE` temporarily 12 and 16 (not committed).
  The ruler, track header lanes, clip strips, transport breakpoints and meter
  labels hold their proportions; nothing overlaps or clips.
- `cargo clippy -p celesta-editor-theme -p celesta-editor -- -D warnings` and
  `cargo fmt --all --check`.

## Documentation

- `docs/handoff/2026-11-10-p7c-timeline.md` records what was built and how it
  was checked.
- The user guide (`packages/website/content/{en,ja}/`) describes the timeline
  and transport by their actions and shortcuts (`I`/`O`, `Shift Z`, scrub,
  zoom), not their look, and the actions do not change. It is checked and
  left as is unless the implementation finds a passage that names "In", "Out"
  or "Fit" as text buttons or the timecode above the track headers.
- The Celesta Agent Skill (`skills/celesta/`) covers the composition API and
  is not affected.

## Out of scope

- New timeline features: track lock or visibility buttons the project model
  does not have, keyframe diamonds, track height resizing, a two-level
  timeline, snapping or tool buttons.
- The Inspector (P7b) and the export page (P7d).
- The transport's position (it stays under the viewer) and the timeline's
  split sizes (they stay in pixels, as P7a set them).
