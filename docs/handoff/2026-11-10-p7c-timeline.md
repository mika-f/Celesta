# P7c: the timeline and transport (2026-11-10)

The third sub-project of P7, the editor's UI refinement, on top of P7a.
Spec: `docs/superpowers/specs/2026-11-10-p7c-timeline-design.md`. Plan:
`docs/superpowers/plans/2026-11-10-p7c-timeline.md`.

## What changed

**Tokens (`celesta_editor_theme::metrics`).** The timeline's sizes, as `Rems`
(pixels at the 13px base):

| Token | Value |
| --- | --- |
| `track_header_width()` | 18rem ≈ 234px |
| `ruler_height()` | 2.75rem ≈ 36px (also the meter's "Master" header) |
| `video_track_height()` / `audio_track_height()` | 4rem / 5rem |
| `clip_inset()` | 0.375rem, above and below a clip |
| `clip_label_height()` | 1.5rem, a clip's name strip |
| `chapter_lane_height()` | 2rem |
| `meter_column_width()` | 10rem ≈ 130px |
| `volume_slider_width()` | 6.75rem |

The footer uses `toolbar_row_height()`. The theme gains
`clip_label_fill(kind)`, each kind's clip color one step darker.

**`TimelineGeometry` (`crates/editor/src/timeline_geometry.rs`).** The same
sizes in `Pixels` at one `rem`, plus the ruler's label spacing (12rem), minor
tick spacing (0.75rem) and length, and the playhead cap. `timeline()` builds
it from `window.rem_size()` and stores it on `EditorView`
(`timeline_geometry`); chapter lanes, the overlays placed at "header width +
lane x", the painted ruler and the pre-paint fallbacks of `frame_x` and
`timeline_fraction_at` read it. Layout reads the `metrics` tokens directly.

**Ruler.** The corner above the track headers no longer shows the playhead's
timecode (it shows once, under the viewer). Labels are monospace `text_sm`
full timecodes at least 12rem apart. `RulerScale` gained `mid`: the coarsest
ladder step between `minor` and `major` that divides the major and is a
multiple of the minor; ticks are painted full height (major), half (mid)
and short (minor).

**Transport and footer.** Mark In and Mark Out are icon buttons
(`ArrowRightFromLine`, `ArrowRightToLine`), zoom to fit is
`UnfoldHorizontal`; all three are added to `LucideIcons` in `icons.rs`.
Every icon button in the transport bar and the timeline footer sets
`accessibility_label` (a tooltip only describes a button), which
`style_guard::transport_icon_buttons_have_accessible_names` keeps.
`transport_layout(width, rem)` compares in rems: everything at ≥ 51.25rem,
the In/Out buttons at ≥ 37.5rem (820px and 600px at a 16px rem).

**Track headers (`timeline_header.rs`).** `TrackHeader` lays out fixed lanes:
label (2rem, monospace), kind swatch, name (flex, ellipsized, count after it
for Chapters), two state slots (disabled, locked), the level slot, and M/S
slots (`MixButtons`). Empty slots keep their width, so lanes line up across
rows; a React preview drops the M/S lane from every row. The track rows, the
Composition row and the Chapters row all use it.

**Clips (`timeline_clip.rs`).** `ClipBlock` fills the box its caller places:
a name strip (`clip_label_height()`, `clip_label_fill`) holding the name,
the body in the clip color holding the waveform, a 2px selection outline
over both, square corners. The Composition block uses it; with "Set timing"
shown, the overlay draws the entry name on the strip's line beside the
button. Chapter blocks and selection bands lost `rounded_sm`.

**Meter column.** dB labels are monospace `text_sm` and thin with the bars'
height (`meter::scale_marks`: the eight marks, every 12 dB, or 0/30/60); the
height is measured into `EditorView.meter_height` as the transport's width
is. Peak readouts are monospace `text_sm`. The dB scale column is 1.25rem;
the column grew from the spec's 8.75rem to 10rem because the two readouts
(`-23.2 -23.2`) overflowed it. The volume slider is on rems, and a drag maps
across `volume_slider_width()` at the window's rem
(`MasterVolumeDrag::slider_width`).

**Spacing review.** The first build was too cramped; two rounds of inner
padding, each launched for the user's review, set the token values above
plus: 0.75rem between the header's lanes and the panel padding on both its
ends, 0.5rem side padding on clip, chapter and composition names, 0.25rem
above and below the waveform, ruler labels 0.375rem from the top and from
their tick, a 3rem transport row, and 1rem side / 0.75rem vertical padding
in the meter column. The right dock's spacing was passed to P7b (#805).
In the same review, `panel_header_height()` went from P7a's 1.875rem to
2.5rem: gpui-kit draws a medium `TabVariant::Tab` 32px tall, and the shorter
band squeezed the right dock's tabs (`the_panel_header_holds_a_whole_tab`
keeps it). "Reload composition" dropped `.small()` for the default size;
#805 rewrites that panel, so expect a one-line conflict there.

## Pixel rules

`style_guard::pixel_layout_stays_out_of_the_timeline` fails on a `px(` line
in `timeline.rs`, `timeline_header.rs`, `timeline_clip.rs` or `meter.rs`
that does not say `physical:`. The physical ones: hairlines and one-pixel
lines (ticks, playhead, marks, hold line), `LANE_GUTTER` (gpui-base's
scrollbar is pixel-sized), grip widths (hit tolerance), the smallest visible
clip or band (2px), measured lane pixels, the scroll offset, the painted
bars' gap and the half-pixel playhead cap alignment.

## Tests

- `celesta-editor-theme`: `timeline_tokens_keep_their_order`,
  `clip_labels_are_darker_than_their_clips`.
- `celesta-editor`: `timeline_geometry::tests` (sizes at 13 and 16px; the
  default), `timecode::tests` (`mid_ticks_split_the_major`,
  `a_ten_second_composition_labels_every_two_seconds`,
  `no_mid_when_minor_is_the_major`, `long_and_single_frame_rulers_stay_valid`),
  `timeline_header::tests` (`lanes_line_up_across_rows`,
  `without_a_mix_lane_the_name_widens`), `timeline_clip::tests`
  (`the_name_strip_is_its_token_tall`, within 0.5px of layout rounding;
  `a_narrow_clip_clips_its_name`), `meter::tests`
  (`scale_marks_thin_with_the_column`, `peak_readouts_fit_the_column`),
  `viewer::tests::the_transport_bar_collapses_at_its_own_widths`,
  `tests::master_volume_from_drag…`, and the new style guard.
- An `EditorView` cannot be built in a test window, so the header and the
  clip are tested as components; that the timecode shows once is checked by
  the screenshots.
- `transform_drag::tests` flaked twice in full runs (NotFound on its temp
  scene file): `one_file` names its temp directory by process id and
  `SystemTime` nanoseconds, which repeat between parallel tests on macOS.
  It predates P7c and was left alone.

## How it was checked

- `cargo test -p celesta-editor-theme -p celesta-editor` (791 and 8 passed),
  `cargo clippy -p celesta-editor-theme -p celesta-editor --all-targets -- -D warnings`,
  `cargo fmt --all --check`.
- Screenshots (attached to the PR, not committed): the editor built from the
  P7a tip (`34a92a5b`) and from this branch, run from the repository root with
  `CELESTA_PLUGIN_PATH="$PWD/plugins/dist"` on
  `examples/greenscreen/composite.tsx` and `examples/36-days/film.tsx` (its
  `score.wav` generated with `python3 examples/36-days/make-score.py` and
  removed afterwards; 36-days has an audio track but no Chapters row), a
  1440×900 window, `screencapture -x -o -l <window id>`.
- Zoom: `BASE_FONT_SIZE` temporarily 12 and 16 (not committed). The ruler
  keeps its label spacing in rems (labels every second at 16px), header lanes
  and clip strips keep their proportions, the meter drops to the 12 dB marks
  at 16px, and "Volume 100%" fits the column.
- The user guide describes the timeline and transport by their actions and
  shortcuts (`I`/`O`, `Shift Z`), not by "In"/"Out" text buttons or the
  ruler's timecode, so it was not changed. The Agent Skill covers the
  composition API only.
- The worktree's `target` and `plugins/target` symlinks pointed at an mbx
  cache that no longer existed; they were removed so the mbx shim would
  recreate them.
