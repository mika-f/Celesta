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
| Copy stays clear of platform UI (vertical video) | PNG frames or a contact sheet with `--debug-guides` | seconds | what the safe area covers |
| A seamless loop: the last frame runs into the first | `scripts/inspect.mjs --loop` | seconds | frame N's Scene equals frame 0's |
| Audio and the final file | short MP4 export + `ffprobe` | longer | sound, streams, duration |

Never claim the video "looks right" unless you rendered a frame and looked
at the image.

`Celesta-export` below stands for the exporter path you found in
[setup.md](setup.md#find-the-celesta-tools).

## Contents

- [inspect.mjs](#inspectmjs)
- [Look at real frames](#look-at-real-frames)
- [Contact sheets](#contact-sheets)
- [Check a vertical video's safe area](#check-a-vertical-videos-safe-area)
- [Check a JSON project](#check-a-json-project)
- [Check audio](#check-audio)
- [Check a seamless loop](#check-a-seamless-loop)

## inspect.mjs

`scripts/inspect.mjs` in this skill loads a React entry with Celesta's own
runtime (same bundler, same `prepare()` call, same renderer protocol as the
app and exporter), evaluates frames, and prints the resulting layer tree.

```sh
node <skill>/scripts/inspect.mjs scene.tsx                  # frames 0, middle, last
node <skill>/scripts/inspect.mjs scene.tsx --frames 0,45,-1 # -1 is the last frame
node <skill>/scripts/inspect.mjs scene.tsx --every 15       # every 15th frame (and the last)
node <skill>/scripts/inspect.mjs scene.tsx --every 1        # every frame, before a long export
node <skill>/scripts/inspect.mjs scene.tsx --json           # raw Scene JSON
node <skill>/scripts/inspect.mjs --psd-layers hana.psd      # PSD layer paths for portraits
node <skill>/scripts/inspect.mjs card.tsx --props-file variants/spring.json  # with project property values
node <skill>/scripts/inspect.mjs scene.tsx --native Celesta-export  # pick the exporter (see below)
node <skill>/scripts/inspect.mjs loop.tsx --loop            # frame N against frame 0 (seamless loops)
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
text "CELESTA" at (960, 900) anchor (0.5, 0.5) filters ripple(time=0.4, amplitude=6) pad 8 → rgb-split(amount=10) pad 24 → saturation(1.2) [sans-serif 120px 900 #ffffff]
audio ./voices/line-01.wav from 0 s for 3 s

missing: ./mira/calm.png (relative paths resolve from /Users/me/my-video)
1 problem(s) found
```

Read it as: one line per layer in draw order (later lines draw on top),
nested groups indented, positions in canvas pixels, followed by
non-default opacity, blend mode, effects, and clip. A layer's custom
shaders and built-in filters follow `filters`, in the order they run
(`→`): the `shader` prop's first, then the built-in filter props'. A
custom shader shows its name (the id when it has none), its parameters'
values, and its padding (`pad`); a `color` parameter shows as the four 0–1
numbers the shader receives. A built-in filter shows its values, as
`saturation(1.2)`, `vignette(0.5, radius 0.5, softness 0.5)`,
`pixelate(8 px)`, or `noise(0.2, seed 42)`; a noise's seed is the frame's
unless the composition gave one.
Check that:

- the frames you changed contain the layers you expect, with the text,
  positions, and opacity you intended (an element fully faded out shows
  `opacity 0`; an inactive `Sequence` contributes nothing);
- nothing is `MISSING FILE` and no frame reports `ERROR`;
- audio clips start and last as long as intended.

The exit code is 1 when the entry fails to load, a frame fails, or a local
file is missing, and 2 when the only problem is text it could not measure
(below).

### Measured text: `--native`

Entries that call `useTextMetrics`, `measureText`, `TextBox`,
`useFitText`, `fitText`, use `@celesta.studio/code`, or read `metrics` in a
`subtitle.render` need text shaping, which Node alone cannot do. So
`inspect.mjs` evaluates frames through `Celesta-export --inspect` when it
finds one: the same bridge, fonts, and shaping as an export, and
`preloadMedia()` answered without `ffprobe`, but still no GPU and no pixels.
The first line of the output then reads `native: <path> (found)`.

It looks for a source checkout's build (`target/release` or
`target/debug`, the newer first) above the entry or the current directory,
then the default macOS and Windows installs
([setup.md](setup.md#find-the-celesta-tools)), and skips exporters too old
to support `--inspect` (with `--loop`, too old to evaluate past the end). Pass `--native <Celesta-export>` to choose one
yourself, or `--no-native` to evaluate in Node only. `--props`/
`--props-file` are passed on, and the exporter finds its own runtime
(`--runtime`/`--node` still override it).

```sh
node <skill>/scripts/inspect.mjs scene.tsx --every 15 --native "/Applications/Celesta.app/Contents/MacOS/Celesta-export"
```

In a source checkout, build the exporter once
(`cargo build -p celesta-exporter --release`) and `inspect.mjs` picks it up.
For entries with video or audio, also build the codec plugin and point
`CELESTA_PLUGIN_PATH` at it, from the repository root:

```sh
node scripts/plugins/build.mjs plugins/ffmpeg
export CELESTA_PLUGIN_PATH="$PWD/plugins/dist"   # PowerShell: $env:CELESTA_PLUGIN_PATH = "$PWD\plugins\dist"
```

When no exporter is found (or with `--no-native`), a measurement the entry
does not catch is reported as `UNSUPPORTED inspection: inspect.mjs cannot
shape text…` (with `--json`, `status: "unsupported"` on that frame) rather
than `ERROR`. That is a limit of the inspector, not a broken scene: rerun
with `--native <Celesta-export>`. If the
measurement happens in `prepare()` or in frame 0 (which renders while the
entry loads), no frame is inspected at all. A real `ERROR` elsewhere still
makes the exit code 1. An entry that catches the refusal (in `prepare()`,
say) uses its own fallback and inspects normally, but the placement it
shows is the fallback's.

An exporter given with `--native` that is older than `--inspect` exits
before the handshake; `inspect.mjs` then says so. Update Celesta, or check those frames with PNG
frames or a contact sheet ([below](#look-at-real-frames)).

When frames go through the exporter, with `--json` a failed frame's `status` is `sceneError` (the
scene threw; later frames are still inspected), `requestError`, or
`runtimeError` (the runtime is gone; inspection stops there).

Limits:

- In Node only, `preloadMedia()` and `planDialogue()` are answered
  with `ffprobe` when it is installed; otherwise `prepare()` fails with a
  clear message. `ffprobe` gives no `animation` for an animated image.
  `loadAudioSpectrum()` is answered with the `ffmpeg` command the same way.
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
  Either selects still output; the `.png` extension makes it lossless PNG.
  Keep PNG for checks: `.jpg` and `.webp` outputs are lossy, for
  thumbnails ([export.md](export.md#still-images)).
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

One export writes at most 1000 separate images; use a larger interval, a
shorter span, or a contact sheet.

PNG export uses the same GPU renderer as MP4 in `final` quality, over the
same opaque background, one `prepare()` call for all frames, no
audio, and odd canvas sizes allowed. Debug guides (`DebugOverlay`,
`DebugBounds`, anything gated on `useDebugGuides()`) are drawn only with
`--debug-guides` (React entries; never in an MP4). Font fallback and
missing-glyph warnings are printed to stderr: read them.

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
| `--debug-guides` | Draw the entry's `<DebugOverlay>` and `<DebugBounds>` into the tiles (also works without `--contact-sheet`). |

Tiles are reduced by averaging, so thin lines and small text blur rather
than vanish; read text and fine details in a full-size `--frame` export. A
sheet holds at most 400 frames and is at most 16384 px on each side.

A frame that fails to evaluate or render (a bad color name, an exception in
a component) does not stop the sheet. Its tile is dark red and marked
`FAILED`, the other frames are rendered, and the sheet is written. Then the
export fails with exit status 1 and lists every failed frame with its error:

```text
Celesta export: 1 of 13 contact sheet frames failed; /tmp/celesta-sheet.png shows them as failed tiles:
  frame 120: could not evaluate React export: could not render the composition: <Rect> fill "ink" is neither …
```

With `--json`, the error code is `frames_failed`, `error.failedFrames` lists
each `{frame, code, message}`, and `outputs` still lists the sheet
([export.md](export.md#json-results)). Fix each listed frame, then check it
with `--frame <n>` before exporting the sheet again. One sheet finds every
failing frame it samples; frames between the samples are not checked, so for
a final check of every frame use `inspect.mjs --every 1`, which evaluates
without rendering.

A failure that is not about the frame's content (the GPU or its readback
failing, the React runtime exiting, an output that cannot be written) stops
the sheet at once, as it would any export: the next frame would fail the
same way.

Separate still images and videos stop at the first failed frame. The error
names it (`frame 120: …`), and stills written before it stay.

## Check a vertical video's safe area

For Shorts, TikTok, or Reels, check that everything meant to be read sits
inside the safe area and only backgrounds and decoration run under the
platform UI ([react-core.md](react-core.md#vertical-video-shorts-tiktok-reels)).

1. Put `<DebugOverlay safeArea="vertical-social" />` (or the per-edge
   `{ top, right, bottom, left }` you laid out with) last in the
   `<Composition>`, so it draws on top. It draws nothing in an MP4, so it
   can stay.
2. Export with `--debug-guides`. The area outside the safe area is shaded
   and the safe area is outlined:

   ```sh
   Celesta-export --react film.tsx --every 30 --contact-sheet --debug-guides /tmp/celesta-safe.png
   Celesta-export --react film.tsx --frames 0,150 --debug-guides /tmp/celesta-safe.png
   ```

3. Look at every tile. Text, subtitles, logos, and faces must be inside the
   outline for their whole time on screen, including while they move in or
   out and at the peak of a zoom or shake. Check a doubtful frame at full
   size, since tiles are small.
4. Export without `--debug-guides` for the final check: the guides must be
   gone.

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

## Check a seamless loop

For a video meant to repeat (Shorts, TikTok), check that frame N (one past
the last) equals frame 0, then look at the seam:

```sh
node <skill>/scripts/inspect.mjs film.tsx --loop
Celesta-export --react film.tsx --frames 95,96,383,0 --contact-sheet --columns 4 /tmp/celesta-seam.png
```

`--loop` evaluates frames N and N + 1 as if the video went on and lists every
value that differs from frames 0 and 1; the exit code is 1 when anything
does. An entry that measures text goes through the exporter as in
[Measured text](#measured-text---native). The contact sheet shows the last
and first frames side by side, next to an ordinary pair of frames. How to
read the results and make a video loop: [loops.md](loops.md#check-the-loop).
