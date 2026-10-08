# Verify without the GUI

You cannot see the Celesta preview window. Check your work with these tools,
cheapest first:

| Check | Tool | Time | Proves |
| --- | --- | --- | --- |
| The React entry loads; layers, positions, audio, missing files per frame | `scripts/inspect.mjs` | seconds | structure and timing (not pixels, not fonts) |
| Types | `tsc` ([setup.md](setup.md#type-check)) | seconds | props and imports are valid |
| A JSON project is valid | any short export (validation runs on load) | seconds | schema and references |
| Real pixels at chosen frames | PNG export (`--frame`, `--frames`) | seconds | what the export will look like |
| The whole video at a glance | contact sheet (`--every … --contact-sheet`) | seconds–minutes | pacing, layout across scenes |
| Audio and the final file | short MP4 export + `ffprobe` | longer | sound, streams, duration |

Never claim the video "looks right" unless you rendered a frame and looked
at the image.

`Celesta-export` below stands for the exporter path you found in
[setup.md](setup.md#find-the-celesta-tools).

## Contents

- [inspect.mjs](#inspectmjs)
- [Look at real frames](#look-at-real-frames)
- [Contact sheets](#contact-sheets)
- [Check a JSON project](#check-a-json-project)
- [Check audio](#check-audio)

## inspect.mjs

`scripts/inspect.mjs` in this skill loads a React entry with Celesta's own
runtime (same bundler, same `prepare()` call, same renderer protocol as the
app and exporter), evaluates frames, and prints the resulting layer tree.

```sh
node <skill>/scripts/inspect.mjs scene.tsx                  # frames 0, middle, last
node <skill>/scripts/inspect.mjs scene.tsx --frames 0,45,-1 # -1 is the last frame
node <skill>/scripts/inspect.mjs scene.tsx --every 15       # every 15th frame (and the last)
node <skill>/scripts/inspect.mjs scene.tsx --json           # raw Scene JSON
node <skill>/scripts/inspect.mjs --psd-layers hana.psd      # PSD layer paths for portraits
node <skill>/scripts/inspect.mjs card.tsx --props-file variants/spring.json  # with project property values
```

It finds the runtime automatically (a source checkout above the entry or the
current directory, then the default macOS and Windows install locations).
Override with `--runtime <cli.js>` and `--node <node>` (or `CELESTA_REACT_CLI`
/ `CELESTA_NODE`). With no Node.js on PATH, run the script with Celesta's
bundled Node:

```sh
"/Applications/Celesta.app/Contents/Helpers/node" <skill>/scripts/inspect.mjs scene.tsx
```

Sample output:

```text
composition: 1920×1080 @ 30 fps, 150 frames (5 s)

── frame 75 (2.5 s) ──
rect 1920×1080 at (0, 0) fill linear(#101018→#10101800)
text "Hello, Celesta." at (960, 540) anchor (0.5, 0.5) opacity 0.42 [sans-serif 96px #ffffff]
group at (1400, 200) blend add glow #ffcc66/12 clip 600×80 at (0, 0)
  image ./mira/calm.png at (0, 0) size 432×auto  ← MISSING FILE
path 3 commands at (0, 0) stroke #EF402B/3
audio ./voices/line-01.wav from 0 s for 3 s

missing: ./mira/calm.png (relative paths resolve from /Users/me/my-video)
1 problem(s) found
```

Read it as: one line per layer in draw order (later lines draw on top),
nested groups indented, positions in canvas pixels, followed by
non-default opacity, blend mode, effects, and clip. Check that:

- the frames you changed contain the layers you expect, with the text,
  positions, and opacity you intended (an element fully faded out shows
  `opacity 0`; an inactive `Sequence` contributes nothing);
- nothing is `MISSING FILE` and no frame reports `ERROR`;
- audio clips start and last as long as intended.

The exit code is 1 when the entry fails to load, a frame fails, or a local
file is missing.

Limits:

- **It cannot shape text.** Entries that call `useTextMetrics`,
  `measureText`, `TextBox`, `useFitText`, `fitText`, or use
  `@celesta/code` fail with `inspect.mjs cannot shape text…`. Use a PNG
  export for those.
- `preloadMedia()` and `planDialogue()` are answered with `ffprobe` when it
  is installed; otherwise `prepare()` fails with a clear message.
- Entries that render `<ProjectTimeline />`, `<ProjectTrack />`, or
  `useProjectTrack()` need the Rust evaluator and fail here by design;
  verify those with a PNG export using `--project`.
- It does not check fonts, glyph coverage, or how images look.

## Look at real frames

Export lossless PNGs at exact, zero-based composition frames:

```sh
Celesta-export --react scene.tsx --frame 90 /tmp/celesta-frame.png
Celesta-export --react scene.tsx --frames 0,89,90,149 /tmp/celesta-check.png
Celesta-export project.celesta.json --frames 0,30,59 /tmp/celesta-project.png
# Companion projects use the same evaluation as MP4:
Celesta-export --react scene.tsx --project project.celesta.json --frame 90 /tmp/celesta-frame.png
```

Then open the PNG (with your image-reading tool) and look at it.

- `--frame` can be repeated; `--frames` takes comma-separated integers.
  Either selects PNG output (`--output-format png` is optional).
- One selected frame writes the exact output filename. Several produce
  `celesta-check-000000.png`, `celesta-check-000089.png`, … (at least six
  digits).
- The last valid frame is the frame count minus one. Negative, duplicate,
  and out-of-range frames are rejected. All selections and existing files
  are checked before rendering; `--overwrite` permits replacement.
- `--frame`/`--frames` cannot be combined with `--from`/`--to`.

`--every <n>` selects 0, n, 2n, … and always the last frame (`--every 30` on
a 100-frame composition gives 0, 30, 60, 90, 99). It can be narrowed with
`--from`/`--to` (`--to` exclusive); files are still named by composition
frame number. It cannot be combined with `--frame`/`--frames`.

```sh
Celesta-export --react scene.tsx --every 15 --from 4 --to 6 /tmp/celesta-span.png
```

One export writes at most 1000 separate PNGs; use a larger interval, a
shorter span, or a contact sheet.

PNG export uses the same GPU renderer as MP4 in `final` quality, with RGBA
output including transparency, one `prepare()` call for all frames, no
audio, and odd canvas sizes allowed. Debug guides (`DebugOverlay`,
`DebugBounds`) are not drawn. Font fallback and missing-glyph warnings are
printed to stderr: read them.

**Only overwrite a file the user asked for or one you created.** Write
checks to a scratch path such as `/tmp/celesta-*.png`.

## Contact sheets

To check a whole video at once, add `--contact-sheet`: the selected frames
are reduced and laid out on a grid in one PNG, written to the exact output
filename. Under each tile is the frame number and timecode
(`#90 00:00:03:00`); on narrow tiles only the frame number fits.

```sh
# A 2-minute, 30 fps video: one tile every 5 seconds, 25 tiles.
Celesta-export --react scene.tsx --every 150 --contact-sheet /tmp/celesta-sheet.png
# Closer look at one span, bigger tiles:
Celesta-export project.celesta.json --every 10 --from 20 --to 30 \
  --contact-sheet --columns 4 --tile-width 480 /tmp/celesta-sheet.png
```

| Option | Meaning |
| --- | --- |
| `--contact-sheet` | Write one grid image instead of one PNG per frame. Works with `--frame`, `--frames` (tiles in the given order) or `--every`. |
| `--columns <n>` | Tiles per row (default 5; fewer frames use one row). |
| `--tile-width <px>` | Tile width (default 320); the height keeps the composition's aspect ratio. |

Tiles are reduced by averaging, so thin lines and small text blur rather
than vanish; read text and fine details in a full-size `--frame` export. A
sheet holds at most 400 frames and is at most 16384 px on each side.

## Check a JSON project

A `.celesta.json` is validated in full when the exporter loads it, so any
export is also a validation. The quickest:

```sh
Celesta-export project.celesta.json --frame 0 /tmp/celesta-check.png
```

Errors name the failing path, for example
`invalid project: tracks[0].items[1].content.asset: references missing asset "x"`.
See [errors.md](errors.md).

## Check audio

PNG export skips audio. Export a short MP4 span (see
[export.md](export.md)) and, if `ffprobe` is installed, inspect its
streams:

```sh
Celesta-export --json --overwrite --preset ultrafast --from 2 --to 5 --react scene.tsx /tmp/celesta-check.mp4
ffprobe -v error -show_entries format=duration:stream=codec_type -of compact /tmp/celesta-check.mp4
```

`inspect.mjs` already lists when each audio clip starts and how long it
plays, which is usually enough to check timing.
