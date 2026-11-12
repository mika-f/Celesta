# P7d: the export page (2026-11-11)

The fourth sub-project of P7, the editor's UI refinement. It builds on P7a
(`docs/superpowers/specs/2026-11-10-p7a-visual-foundation-design.md`, PR
#802) and follows that spec's design direction ("like the timeline") and
tokens. It turns P3's export dialog
(`docs/handoff/2026-11-09-export-presets-dialog.md`) into an export page.
Decided with the user one question at a time, against the reference study
of DaVinci Resolve 21's Deliver page and Premiere Pro 2026's Export mode
(notes outside the repository).

## Why

P3's dialog does what the media-components design asked of it, but it reads
as a long web form, and P7a's review of the editor listed its rough edges:

- **One long column.** Preset, Container, the video encoder and its
  fields, the audio encoder and its fields, Range and Loudness stack in a
  dialog almost the window's height; Range and Loudness are below the fold.
- **"Preset" means two things.** The export preset and x264's `preset`
  field share the label.
- **"Rate control" twice.** The x264 group heading "Rate control" sits
  right above the field "Rate control".
- **Plugin ids, not names.** Components read "MP4 · ffmpeg": the plugin's
  `id`, since `plugin.json` has no display name.
- **CLI text in warnings.** A stale preset warns
  ``preset “Web”: ffmpeg/libx264 crf must be at most 51; using its default``:
  the CLI's sentence with its quotes made typographic, naming the preset
  the warning already sits under, a component reference and a setting key.
- **"Advanced" is indented** differently from the fields (a ghost button's
  padding).

Resolve and Premiere both export from a page of their own, with the
settings in a column beside the viewer, and the GPUI Kit Design Guides
reserve dialogs for short decisions ("use a full page or sheet for
workflows that need exploration or many fields").

## Scope

- **In:** the export dialog becomes a page; the six issues above; a
  display name for plugins (`plugin.json`'s `name`); the resolve warnings
  as a structured type so the editor can word them.
- **Out:** any new export feature (see [Out of scope](#out-of-scope)).
  The export model (`ExportDialogModel`) keeps its behavior: presets, the
  container rule, `last`, Save as preset…, describe and resolve on the
  background executor, and the range set with the timeline's In and Out.

## Decisions

| Question | Decision |
| --- | --- |
| What the page replaces | Resolve's Deliver layout: the settings column takes the left panel (where Assets is); the viewer, the right dock and the timeline stay. |
| The right column | The right dock (Inspector, Plugins, Problems) stays as it is. An export's failures go to Problems, which stays one click away. |
| Entering and leaving | The title bar's Export becomes a toggle: "on" while the page shows; pressing it again, Escape, or the panel's close button returns to editing, keeping the page's state. |
| After Export… | The save panel opens over the page; once a path is chosen and the export starts, the editor returns to editing. Progress stays in the status bar and Cancel export in the title bar, as today. |
| Plugin names | An optional top-level `name` in `plugin.json`, the `id` when absent. |
| Approach | The dialog's view becomes the page's settings panel (`ExportPage`), shown by a flag on `EditorView`. A `Sheet` (an overlay outside the editor's key context, so In and Out would not reach the timeline) and a separate page view owning the whole layout (the viewer and the timeline are `EditorView` methods) were rejected. |
| Rows | The page's own two-lane row, on the same tokens and label width as P7b's `InspectorRow` (PR #805, open). P7d does not depend on P7b's code; merging the two is a follow-up once both have landed. |

## The page

### Layout

While the page is open:

```
+------------------+------------------------+--------------+
| Export         X | viewer                 | Inspector |  |
|------------------|                        | Plugins   |  |
| settings         |                        | Problems     |
| (scrolls)        |------------------------|              |
|                  | transport              |              |
|------------------|                        |              |
| footer           |                        |              |
+------------------+------------------------+--------------+
| timeline (its In and Out are the export range)           |
+----------------------------------------------------------+
| status bar                                               |
```

- The left resizable panel shows the **Export panel** instead of Assets.
  The viewer, the right dock, the timeline and the status bar are the same
  elements as in editing, and work as they do there.
- The Export panel has its own width, 26rem by default within 22–34rem,
  apart from the Assets width: returning to editing restores Assets as it
  was. In a React preview, where the left panel is hidden, it shows while
  the page is open.
- The panel's top is P7a's `PanelHeader`: the title "Export" and, in the
  trailing slot, a ghost icon button (lucide `X`, tooltip "Back to
  editing"). There is no `•••`: the panel has no options.
- The page lives inside the editor's `CelestaEditor` key context, so the
  editor's shortcuts work on it as in editing (`i` and `o` set In and Out
  unless a text field has the focus).

### Entering

- **The title bar's Export.** A ghost `small` button labelled "Export"
  (no ellipsis: it shows a page, it does not ask for input), drawn "on"
  (`selected`, one step darker, as P7a's chrome buttons) while the page
  shows; pressing it again returns to editing. While an export runs it is
  replaced by Cancel export (or Cancelling…), as today.
- **File → Export… and ⌘⇧E** (`ExportProject`) open the page and keep their
  ellipsis: more choices follow before anything is exported. With the page
  already open they leave it open.
- The page cannot open while an export runs, while the save panel is up, or
  while a project is opening, as the dialog could not.
- Opening the page **no longer pauses playback**: nothing is modal, and
  playing the In–Out range is a way to check it.

### Leaving

- The Export toggle, the panel's `X`, or **Escape**. Escape comes last in
  the editor's existing chain (a canvas drag, a band or scene drag, a row
  scrub, the add mode), so it closes the page only when nothing else took
  it. An open `Select` menu lives in the window's overlay layer and takes
  Escape first; Escape with the focus in one of the panel's text fields
  (a number field, the save form's Name) closes the page too, keeping
  what was typed.
- Leaving keeps the page's state (the choice, edits, the open "Advanced",
  the save form), as the dialog's Cancel did.
- Reload Plugins and opening another project drop the page's state as they
  dropped the dialog's; an open page closes with it (which also ends P3's
  "the open overlay shows an empty dialog until it is closed").

### Export…

- `Export…` in the panel's footer writes `last` and opens the save panel,
  as the dialog's did.
- **The page stays while the save panel is up.** Cancelling the save panel
  leaves the page as it was. Once a path is chosen and the export starts,
  the page closes and the editor returns to editing.

## The Export panel

One column under the header that scrolls as a whole, and a footer pinned
under it. At a 13px `rem` with "Advanced" closed the column fits a
1440×900 window (about 500px for x264 and AAC), so nothing is below the
fold; sections do not collapse.

```
Export                                       X
──────────────────────────────────────────────
Preset        [None                       v]
Container     [MP4 · FFmpeg               v]
Range         Whole composition
              Set In and Out on the timeline.
Video ───────────────────────────────────────
Encoder       [H.264 (x264) · FFmpeg      v]
Speed preset  [Medium                     v]
              Slower presets make smaller files at the same quality.
Bit depth     [8-bit                      v]
              10-bit H.264 plays in fewer players.
Rate control
Mode          [Constant quality (CRF)     v]
CRF           [18                          ]
              Lower is higher quality and larger.
› Advanced
Audio ───────────────────────────────────────
Encoder       [AAC · FFmpeg               v]
Bitrate       [128000                ] bit/s
Loudness      [Off                        v]
──────────────────────────────────────────────
Save as preset…                     [Export…]
```

### Rows

- Two lanes, as the timeline lays out its rows: a **label lane** of
  `7rem` (P7b's `inspector_label_width`, a token of the theme crate's
  `metrics`, added by whichever of P7b and P7d lands first) and a **value
  lane** with the rest. There is no trailing lane: nothing would go in it
  (a per-field reset would be a new feature).
- Labels `text_sm`, `muted_foreground`, left-aligned, one line; a label that
  does not fit ends in an ellipsis and shows its full text in a tooltip.
- The value lane holds the control in its `small` size: `Select`, `Input`
  (a number field's unit stays the input's suffix), `Checkbox`.
- A row is at least `list_row_height()` tall. The panel's inset is
  `panel_padding()`; rows have no hairlines between them.
- **Under a control**, starting at the value lane's edge, `text_sm`: the
  field's description (muted); an error (`danger`) or a warning
  (`warning`), each with the `TriangleAlert` icon and the text, never the
  color alone. The preset's and the container's warnings and failures sit
  under their rows, as they sat under their sections.

### Sections and groups

- Preset, Container and Range come first, with no heading.
- **"Video" and "Audio"** each start with a section header: the band color
  (`title_bar`), `list_row_height()` tall, one hairline under it (the
  boundary owner draws it), the title `text_sm` medium. No chevron, no
  collapse.
- Each section's first row is **Encoder**: the video encoders, or the audio
  encoders and "None". Its fields follow: ungrouped first, then each group
  in schema order, as the model gives them.
- **A group** shows its name on a line of its own, starting at the label
  lane's edge, `text_sm` medium, `foreground`, with a section gap above it.
  Its fields keep the same lanes: nothing is indented.
- **Advanced** is a disclosure at the label lane's edge: a ghost `xsmall`
  `Button` with `ChevronRight` (closed) or `ChevronDown` (open) and
  "Advanced", its leading padding removed so its icon starts on the labels'
  edge. Its fields, once open, use the same lanes. Each section has its
  own, closed at first, as today.

### Range and Loudness

- **Range**: the in- and out-points as `00:00:01:00 – 00:00:02:00` in
  monospace with a ghost `small` `Clear` after them, or "Whole composition"
  with the muted note "Set In and Out on the timeline." A React entry
  always exports all of it and shows "Whole composition" without the note.
  The row follows the timeline live: setting In or Out on the page updates
  it.
- **Loudness** moves to the Audio section's end. With audio "None" it is
  disabled, with "No audio is exported." under it; its value is kept and
  the export does what it does today.

### Footer

- Pinned under the column: the band color, one hairline above it,
  `panel_padding()` inset.
- Leading: **Save as preset…** (ghost). Trailing: **Export…** (primary),
  disabled while blocked. The reason Export… is blocked ("Checking…" muted,
  anything else as an error) shows on one line above the buttons, as
  today.
- **Enter** runs Export… from anywhere in the panel, through an action in
  the panel's key context. In the save form's Name field Enter saves the
  preset instead, as today.
- **Save as preset…** opens the inline form above the footer's buttons, as
  today, laid out in the same lanes: Name (`Input`), Save to (`Select`:
  This project, My presets), then `Save` (or `Replace`) and the close icon.
  Its behavior is P3's.

## Wording

### "Preset"

- The page's **Preset** (the export preset) keeps its name, as Resolve's
  Deliver page uses it.
- The ffmpeg plugin's x264 field `preset` is labelled **"Speed preset"**
  (`plugins/ffmpeg/src/components.rs`, `x264_settings`). Its key stays
  `preset`, so presets files, `--video-option preset=slow`, MCP and
  `@celesta.studio/exporter` are unchanged. Its description stays.

### "Rate control"

- The ffmpeg plugin's `rateControl` field is labelled **"Mode"**; its group
  stays "Rate control". The page reads "Rate control", then Mode, CRF or
  Bitrate.
- A test in the ffmpeg plugin fails when a field's label equals its
  group's name. The editor adds no rule of its own for other plugins.

### Plugin names

- `plugin.json` takes an optional top-level `name`, the plugin's display
  name (`"name": "FFmpeg"`); `celesta_plugin_host::Manifest` reads it as
  `name: Option<String>`. A manifest without it loads as before, and its
  `id` is shown.
- The bundled ffmpeg plugin's manifest gets `"name": "FFmpeg"`. Plugins on
  open branches (P5's AVFoundation, P6's Media Foundation) add theirs in
  their own pull requests.
- `MediaComponentInfo` carries the plugin's display name (`plugin_name`),
  resolved by the registry from the manifest.
- The export page's component labels become `"{component} · {plugin
  name}"`: "H.264 (x264) · FFmpeg", "MP4 · FFmpeg"; a project plugin's
  keep " (project)", an unavailable one "{label} — {reason}", a checking
  one "Checking…".
- The CLI, MCP and the JSON report keep references (`ffmpeg/libx264`):
  they are identifiers there. The Plugins panel keeps showing the `id`
  with the version: it is where a plugin's identity and approval are
  managed.

### Warnings

- `ResolvedOutput::warnings` becomes `Vec<ResolveWarning>`, an enum with
  one variant per warning `resolve_output` and `resolve_preset_output`
  give today (the shadowing and unreadable-file warnings of the lookup, a
  stale value reset to its default, an unknown key left out, an
  unavailable preset component falling back, audio left out by the preset
  or by a muxer without audio, audio options ignored, a project's default
  using a project plugin, a muxer that does not write the output's
  extension). Each carries what the text needs: the preset's name, the
  component's reference and display name, the field's key and label.
- `impl Display for ResolveWarning` writes **today's text, byte for
  byte**. The CLI's stderr, the JSON report's `warnings` and MCP do not
  change; the existing tests that assert these strings keep passing
  unchanged.
- The editor words them for the page, where they sit under the Preset or
  Container row:
  - no prefix naming the preset: the row already names it;
  - the component's display name and the field's label, not the reference
    and the key; a key the schema no longer has is quoted as the plugin
    wrote it;
  - sentences, each ending in a period.

  For example:

  | Today | On the page |
  | --- | --- |
  | ``preset “Web”: ffmpeg/libx264 crf must be at most 51; using its default`` | CRF must be at most 51. The default is used. |
  | ``preset “Web”: ffmpeg/libx264 has no setting “speed”; it is left out`` | H.264 (x264) has no setting “speed”. It is left out. |
  | ``preset “Web”: ffmpeg/libx264 is unavailable (this FFmpeg build has no libx264); using the default instead`` | H.264 (x264) · FFmpeg is unavailable: this FFmpeg build has no libx264. The default encoder is used. |

  The exact sentences are written per variant during implementation, in
  this tone.
- The model's own warnings are worded the same way: a presets file that
  cannot be read ("Your presets file couldn’t be read, so its presets are
  not listed: {message}"), a `default` naming no preset, and
  `your last export` where a warning concerns `last`.

## Code

- `crates/editor/src/export_dialog/` → `crates/editor/src/export_page/`:
  `ExportDialog` → `ExportPage`, `ExportDialogEvent` → `ExportPageEvent`,
  `ExportDialogModel` → `ExportModel`. The model's logic is unchanged.
- `open_export_dialog` and the dialog footer (`DialogFooter`,
  `DialogAction`, `DialogClose`) go; `ExportPage` renders the whole panel,
  header and footer included.
- `EditorView` keeps the page entity as it kept the dialog
  (`export_page: Option<(Entity<ExportPage>, Subscription)>`, rebuilt when
  the project presets file differs) and adds `export_page_open: bool`.
  `workspace()` puts the Export panel in the left resizable panel while it
  is set. `request_export` becomes `open_export_page`; the title bar's
  toggle, the panel's `X` and Escape call `close_export_page`.
- The page's row, section header and group line are small private helpers
  in `export_page/view/`; they use only the theme crate's tokens.
- The exporter's `ResolveWarning` lives in `crates/exporter/src/resolve.rs`
  (or a `warning.rs` beside it) and is exported from `celesta_exporter`.

## Testing

Rust, `cargo test`:

- `celesta-plugin-host`: a manifest with `name` reads it; one without it
  loads, and the display name falls back to the `id`.
- `celesta-exporter`: the existing tests that assert warning strings pass
  unchanged (the `Display` is today's text); each `ResolveWarning` variant
  is constructed where it is today.
- `celesta-codec-ffmpeg`: no field's label equals its group's name; the
  x264 labels "Speed preset" and "Mode".
- Editor model: the existing tests, renamed; labels with the plugin's
  display name and the `id` fallback.
- Editor page (`#[gpui_kit::test]`, the dialog's tests moved to the
  page):
  - Enter in the panel emits `Export`; Enter in the save form's Name saves;
  - closing (Escape, `X`, the toggle) and reopening keeps an edited field
    and an open "Advanced";
  - Loudness is disabled with audio "None";
  - each warning variant's wording on the page: no preset prefix, display
    names and labels;
  - a group's fields and the "Advanced" fields start their labels at the
    same x as the Encoder row's label (bounds, not a screenshot).

## Verification

- Before and after screenshots of the same scene, as AGENTS.md requires:
  `examples/greenscreen/composite.tsx`, a 1440×900 window,
  `screencapture -x -o -l <window id>`. Before: today's dialog by default,
  with "Advanced" open, with a stale preset's warnings, and with the
  Save as preset… form. After: the page in the same four states.
- Interface zoom: `rem` set to 12, 13 and 16px for a run; labels truncate
  with their tooltip, nothing overlaps, the footer stays pinned.
- One real export from the page: Export… → the save panel → the export
  starts → the editor returns to editing → progress in the status bar.
  Also a cancelled save panel leaves the page as it was.
- The export page beside Resolve's Deliver page and Premiere's Export mode
  in the pull request. P7a's criterion 2 (the whole editor beside them) is
  judged once P7b, P7c and P7d have all landed.

## Documentation

- `packages/website/content/{en,ja}/docs/02-(guides)/09-export.md`: the
  export page (entering, leaving, the panel, Speed preset and Mode in its
  x264 paragraphs).
- `packages/website/content/{en,ja}/docs/01-(getting-started)/04-react-compositions.md`:
  its mention of Export….
- `packages/website/content/{en,ja}/docs/05-(more)/05-plugins.md`:
  `plugin.json`'s `name`.
- `skills/celesta/references/export.md` and `plugins.md`, where they
  describe the dialog or the manifest.
- The media-components spec, § Editor: a note that P7d made the dialog a
  page.
- `docs/handoff/2026-11-11-p7d-export-page.md`: what was built and how it
  was checked.

## Out of scope

- New export features: a render queue, file name and location fields on
  the page, an estimated file size, an output summary, a per-field reset.
- The plugin display name in the Plugins panel.
- Merging the page's row with P7b's `InspectorRow`.
- P3's "Not done" items other than the six issues above (presets files not
  re-read on reopen, edits overwritten by a resolve that lands, and the
  rest).
