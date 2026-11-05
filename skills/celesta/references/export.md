# Export to MP4, MOV, or WebM

The command-line exporter renders a React composition or a JSON project to
an H.264 + AAC MP4, a transparent MOV or WebM (see
[Transparent video](#transparent-video)), or still images: PNG, JPEG, or
WebP frames and contact sheets (see [Still images](#still-images) and
[verify.md](verify.md)). Find it
first: [setup.md](setup.md#find-the-celesta-tools). `Celesta-export` below
stands for its path.

## Contents

- [Commands](#commands)
- [Options](#options)
- [Transparent video](#transparent-video)
- [Looping video](#looping-video)
- [Loudness](#loudness)
- [Still images](#still-images)
- [JSON results](#json-results)
- [Progress output](#progress-output)
- [Quality notes](#quality-notes)
- [Without a GPU](#without-a-gpu)

## Commands

```sh
Celesta-export [options] project.celesta.json out.mp4
Celesta-export [options] --react scene.tsx out.mp4
Celesta-export [options] --react scene.tsx --project project.celesta.json out.mp4
Celesta-export [options] --react card.tsx --props-file variants/spring.json spring.mp4
```

The source comes before the output. With `--react`, the source is the
entry file. Relative asset paths resolve from the entry's (or project
file's) folder, not the current directory.

A fast check of one second around the part you changed:

```sh
Celesta-export --json --overwrite --preset ultrafast --from 2 --to 3 \
  --react scene.tsx /tmp/celesta-check.mp4
```

The final export the user asked for:

```sh
Celesta-export --json --react scene.tsx out.mp4
```

**Only overwrite a file the user asked for or one you created.** Without
`--overwrite`, the exporter refuses an existing output file.

## Options

| Option | Meaning |
| --- | --- |
| `--react` | The source is a React entry instead of a JSON project. |
| `--project <file>` | Companion JSON project for `<ProjectTimeline />`/`<ProjectTrack />` (needs `--react`). Its `properties` also count as project property values. |
| `--props-file <file>` | JSON object of project property values for the entry (needs `--react`). Without it, a `props.json` in the entry's folder is used automatically; this option replaces it. See [project-data.md](project-data.md#pass-values-from-the-command-line). |
| `--props <json>` | Inline JSON object of project property values (needs `--react`); wins over `--props-file`. |
| `--from <t>`, `--to <t>` | Export only this span (`--to` exclusive); the output starts at 00:00. Times are seconds, `MM:SS.mmm`, or `HH:MM:SS.mmm`. |
| `--subtitles <file>` | Also write the video's subtitles (`<Subtitles>` cues, `<Dialogue>` lines, a project's dialogue items) as `.srt` or `.vtt`, timed like the video. Repeatable; `{lang}` in the path writes one file per language. Without an output video, writes only the subtitles, without rendering. See [subtitles.md](subtitles.md#write-the-subtitle-file-on-export). |
| `--overwrite` | Replace an existing output file. |
| `--preset <p>` | libx264 preset, `ultrafast` … `veryslow` (default `medium`). Encoding speed against file size only; it does not change what is rendered. MP4 only. |
| `--crf <n>` | 0–51, lower is better quality (default 18). MP4 only. |
| `--color-conversion <w>` | `auto` (default), `gpu`, or `encoder`: where RGB becomes YUV. MP4 only. |
| `--bit-depth <n>` | `8` (default) or `10`. `10` encodes H.264 High 10 (`yuv420p10le`): smoother dark gradients, but fewer players decode it; keep `8` for files meant for phones, browsers, or social media. MP4 only. |
| `--loop` | The video plays in a loop (Shorts, TikTok): the AAC sound is encoded to run from its end into its start. MP4 only. See [Looping video](#looping-video). |
| `--loudness <l>` | Bring the whole mix to an integrated loudness: LUFS from -70 to -5, `streaming` (-14), or `broadcast` (-23). Video only. See [Loudness](#loudness). |
| `--true-peak <db>` | The highest true peak `--loudness` may leave, -9 to 0 dBTP (default -1). |
| `--peak-handling <h>` | `limit` (default): a limiter holds peaks at `--true-peak`. `lower`: the gain stops short of the target instead. |
| `--render-quality <q>` | `final` (default) or `draft`. `draft` draws scaled text from a scale-1 texture, so it is softer; use it only to check timing. |
| `--json` | No progress output; one line of JSON on stdout when the export ends. See [JSON results](#json-results). |
| `--json-progress` | With `--json`, also one JSON line per progress event before the result. See [Progress events and cancellation](#progress-events-and-cancellation). |
| `--stdin-cancel` | Cancel the export when stdin reads a `cancel` line or closes. |
| `--info` | Report the composition's size, frame rate, length, and a React entry's compositions without rendering; takes no output. With `--json`, the result has `outputs: []`. |
| `--no-ui` | Plain text progress instead of the terminal dashboard (automatic when stderr is not a terminal). |
| `--output-format <f>` | `png`, `jpeg`, `webp`, `mp4`, `mov`, or `webm`. Normally implied by the output extension; one that disagrees with it is a usage error. |
| `--frame`, `--frames`, `--every`, `--contact-sheet`, `--columns`, `--tile-width`, `--debug-guides` | Still output; see [Still images](#still-images) and [verify.md](verify.md#look-at-real-frames). |
| `--quality <n>` | 1–100, JPEG and WebP stills only (default 90). Higher is sharper and larger. |

Video export needs non-zero, even `width` and `height`.

## Transparent video

The output extension picks the video format:

| Extension | Video | Audio |
| --- | --- | --- |
| `.mp4` | H.264 (`yuv420p`; `yuv420p10le` with `--bit-depth 10`) | AAC |
| `.mov` | ProRes 4444 with alpha (`yuva444p10le`) | 24-bit PCM |
| `.webm` | VP9 with alpha (`yuva420p`, CRF 20) | Opus, 48 kHz |

```sh
Celesta-export --json --react scene.tsx overlay.mov
```

- Only areas nothing draws on are transparent. A full-frame background
  layer makes the output opaque.
- Alpha is straight (unassociated). Color is converted and tagged BT.709
  (limited range).
- `--preset`, `--crf`, `--bit-depth`, `--color-conversion`, and `--loop`
  apply to MP4 only; with `.mov` or `.webm` they are usage errors.
- The MCP `export_video` tool takes the same three extensions in `output`;
  `preset`, `crf`, `bitDepth`, and `loop` are rejected with `.mov` and
  `.webm`.
- Celesta encodes through its codec plugin (`ffmpeg`, bundled with the app).
  A source build makes it with `node scripts/plugins/build.mjs plugins/ffmpeg`
  and finds it once `CELESTA_PLUGIN_PATH` is the absolute path of
  `plugins/dist` (`export CELESTA_PLUGIN_PATH="$PWD/plugins/dist"` at the
  repository root). A plugin
  whose FFmpeg lacks the needed encoder fails with `encoder_unavailable`; no
  codec plugin at all fails with `no_codec`.

## Looping video

Add `--loop` when the MP4 is made to play on repeat, as Shorts and TikTok
play it, and its last frame runs into its first
([loops.md](loops.md)):

```sh
Celesta-export --json --loop --react film.tsx film.mp4
```

- An AAC encoder starts from silence, so without `--loop` a hit on frame 0
  (a kick, a downbeat) comes out smeared for about 10 ms, and a looping
  player repeats that on every pass. With `--loop`, the encoder first takes
  the end of the soundtrack, so the start comes out as cleanly as any other
  bar line. The extra lead-in is hidden by the MP4's edit list: the file
  still decodes to exactly the soundtrack, as long as the video.
- The soundtrack itself must loop: sound that rings past the end belongs at
  the start ([loops.md](loops.md#sound)). `--loop` does not fold it there.
- With `--from`/`--to`, the span's own end leads into its start.
- MP4 only. A `.mov`'s PCM runs from its end into its start without help,
  and `--loop` with `.mov`, `.webm`, stills, or no output video is a usage
  error. The MCP `export_video` tool takes `loop: true`.
- Without `--loop`, the export is unchanged: right for a video that plays
  once, where silence does come before the start.

## Loudness

Streaming services play videos at about -14 LUFS (YouTube, Spotify): a
louder video is turned down, a quieter one stays quiet. Every video export
measures its mix and reports it in the video output's `loudness`
([JSON results](#json-results)); add `--loudness` when the user wants the
video at a loudness:

```sh
Celesta-export --json --loudness streaming --react film.tsx film.mp4
Celesta-export --json --loudness -16 --true-peak -2 --react film.tsx film.mp4
```

- Measured as ITU-R BS.1770 integrated loudness, as FFmpeg's `ebur128`
  filter measures it (within about 0.05 LU; FFmpeg's `loudnorm` reads the
  same file about 0.2 LU lower). The true peak is read 8x oversampled.
- One gain moves the whole mix, so the balance between voice, music and
  effects stays as mixed: set that with clip volumes and ducking first
  ([dialogue.md](dialogue.md)).
- Where the gain would take a true peak over `--true-peak`, a limiter holds
  the peak there and the target is still reached (within 0.1 LU). With
  `--peak-handling lower`, the gain stops where the loudest peak meets the
  ceiling instead, and a warning says how loud the result is.
- A mix that is silent or shorter than 0.4 s has no integrated loudness:
  `integratedLufs` is `null`, it is written unchanged, and `--loudness`
  warns. A React entry without any audio has no audio stream and no
  `loudness`; `--loudness` warns there too.
- The same input always gives the same audio.
- The values describe the mix before encoding. A `.mov`'s PCM keeps them;
  AAC and Opus move the loudness by about 0.1 LU and the true peak by up to
  about 0.5 dB either way. Give `--true-peak -2` when the file itself must
  read under -1 dBTP.
- The MCP `export_video` tool takes `loudness` (a number or `"streaming"` /
  `"broadcast"`), `truePeak`, and `peakHandling`; `@celesta.studio/exporter` takes
  the same names.

## Still images

A frame selection (`--frame`, `--frames`, or `--every`) writes still images
instead of a video. The output extension picks the image format:

| Extension | Format | `--quality` |
| --- | --- | --- |
| `.png` | Lossless 8-bit RGBA | not accepted |
| `.jpg`, `.jpeg` | Baseline JPEG | 1–100, default 90 |
| `.webp` | Lossy WebP | 1–100, default 90 |

```sh
Celesta-export --json --react scene.tsx --frame 90 --quality 85 thumbnail.jpg
Celesta-export --json --react scene.tsx --every 150 --contact-sheet sheet.webp
```

- Stills are drawn over the same opaque background as an MP4, so they have
  no transparent pixels; a JPEG, which has no alpha channel, loses nothing.
- Several frames are written as `thumbnail-000090.jpg` and so on, keeping
  the output's extension.
- WebP holds at most 16383 pixels on each side; a larger contact sheet is
  rejected before rendering.
- A contact sheet carries on past a frame that fails to evaluate or render:
  that tile is marked `FAILED`, the sheet is written, and the export then
  fails with `frames_failed`, listing each failed frame (see
  [JSON results](#json-results) and
  [verify.md](verify.md#contact-sheets)). Separate stills stop at the first
  failed frame.
- Check frames for yourself as PNG ([verify.md](verify.md)); JPEG and WebP
  compression can blur fine text and edges.
- The MCP `render_frames` tool takes `format` (`png`, `jpeg`, or `webp`) and
  `contact_sheet` takes the same extensions in `output`; both take `quality`
  and return the images in the format they wrote. An image over 1568 pixels
  on a side comes back as a downscaled PNG; the file keeps its size and
  format.

## JSON results

Add `--json` to every exporter call you make. Instead of progress output, the
exporter prints a single line of JSON on stdout when it finishes and writes
nothing to stderr. It lists the composition, every file written with its
frame numbers, the warnings, and on failure an error code:

```json
{"status":"ok","source":"/Users/me/my-video/scene.tsx",
 "composition":{"width":1920,"height":1080,"fps":30.0,"frames":150,"duration":5.0},
 "outputs":[{"type":"frame","path":"/tmp/celesta-check-000000.png","frame":0,"time":0.0},
            {"type":"frame","path":"/tmp/celesta-check-000090.png","frame":90,"time":3.0}],
 "warnings":["font family \"Noto Sans JP\" (weight 400) is not installed or loaded; text layer \"root.0\" uses a fallback font"],
 "stats":{"elapsedSeconds":1.2,"renderFps":58.5}}
```

(Shown wrapped here; the real output is one line.) When the entry used a
props file, the result also has `propsFile` (its path) and `propsFileAuto`
(`true` for the entry's automatic `props.json`, `false` for `--props-file`).

- `composition.frames` is the frame count. Frames are zero-based, so the
  last one is `frames - 1` (an empty composition has none). Times are in seconds, rounded to milliseconds.
- `outputs` lists the files that exist after the export, with absolute
  paths. Read these paths; do not work out file names yourself. Each entry
  has a `type`:
  - `video`: `firstFrame`, `frames`, `start`, `duration`, and `audio`
    (whether the file has an audio stream), `format` (`"mp4"`, `"mov"`, or
    `"webm"`), and `alpha` (`true` when the format carries transparency:
    `mov` and `webm`). With audio, `loudness` has `integratedLufs` and
    `truePeakDbtp` (`null` for silence), and with `--loudness` the `gainDb`
    it applied ([Loudness](#loudness));
  - `frame`: one still image, with `frame` and `time`;
  - `subtitles`: one `--subtitles` file (a `{lang}` path lists one per
    language), with `format` (`"srt"` or `"vtt"`), the number of `cues`,
    and `lang` when the subtitles have a language;
  - `contactSheet`: one image, with `columns` and the tile `frames` in
    reading order.
- `stats.renderFps` is the average speed of the whole export in frames per
  second: rendering and video encoding run side by side, so it is the speed
  of whichever is slower. At large sizes the default `--preset medium` can
  encode slower than Celesta renders, and `renderFps` then shows the encoding
  speed. To compare rendering speeds, export an `.mp4` with `--preset ultrafast`
  ([performance.md](performance.md#find-slow-parts)).
- On failure, `status` is `"error"` and `error` has a stable `code`, the
  `message`, and sometimes a `hint`. `outputs` still lists the images written
  before the failure. `composition` is included when the failure happened
  after the composition was loaded. A `usage` error report has only
  `status` and `error`.
- When one composition frame failed, `error.frame` is its zero-based number
  and `message` starts with `frame <n>: `; `code` says what failed
  (`react`, `render`, `evaluation`, …), as it would for any other failure.
- When a contact sheet had frames that failed, the sheet is still written
  and listed in `outputs`, and `error.code` is `frames_failed`.
  `error.failedFrames` lists each failed tile in sheet order as
  `{frame, code, message}`, with the `code` and `message` that frame's
  failure would have on its own (`error` and `outputs` shown, messages cut):

  ```json
  {"status":"error",
   "error":{"code":"frames_failed",
            "message":"1 of 13 contact sheet frames failed; /tmp/sheet.png shows them as failed tiles:\n  frame 120: could not evaluate React export: …",
            "hint":"the sheet was written with failed tiles; fix the frames in failedFrames and export again",
            "failedFrames":[{"frame":120,"code":"react","message":"could not evaluate React export: …"}]},
   "outputs":[{"type":"contactSheet","path":"/tmp/sheet.png","columns":5,
               "frames":[0,10,20,30,40,50,60,70,80,90,100,110,120]}]}
  ```

| `error.code` | Meaning |
| --- | --- |
| `usage` | The arguments were rejected (exit status 2). |
| `output_exists` | An output file exists; add `--overwrite` only if you may replace it. |
| `invalid_selection` | A frame is out of range or repeated, the selection or contact sheet is too large, or a still output is not `.png`, `.jpg`, `.jpeg`, or `.webp`. |
| `empty_range` | `--from`/`--to` cover no frames of the composition. |
| `unsupported_output` | The output is not a `.mp4`, `.mov`, or `.webm` path and no frame selection (`--frame`, `--frames`, `--every`) makes it a still export. |
| `encoder_unavailable` | The codec plugin's FFmpeg lacks an encoder the format needs (video: H.264, ProRes, or VP9; audio: AAC, PCM, or Opus). |
| `no_codec` | No codec plugin was found, so no video or audio could be decoded or encoded. Reinstall the app, or in a source build run `node scripts/plugins/build.mjs plugins/ffmpeg` and `export CELESTA_PLUGIN_PATH="$PWD/plugins/dist"` at the repository root, in the shell that runs Celesta. |
| `unsupported_dimensions` | Video needs non-zero even width and height. |
| `unsupported_subtitles` | The `--subtitles` path is not a `.srt` or `.vtt` file. |
| `mixed_subtitle_languages` | Subtitles in several languages would share a `--subtitles` path without `{lang}`; see [subtitles.md](subtitles.md#several-languages). |
| `project_io`, `project_json` | The project file could not be read or is not valid JSON. |
| `project_validation` | The project is invalid; `error.issues` lists each `{path, message}`. |
| `react` | The React entry failed to load or render; read `message` and look it up in [errors.md](errors.md). |
| `frames_failed` | A contact sheet was written, but the frames in `error.failedFrames` failed and show as `FAILED` tiles. |
| `evaluation`, `render`, `audio`, `codec`, `io`, `time` | The export failed at that stage (`codec`: the codec plugin failed while checking encoders, encoding, or muxing); read `message`. |
| `cancelled` | The export was cancelled (`--stdin-cancel`); what it was writing was removed. |

The exit status is 0 on success, 1 when the export fails (a contact sheet
with failed frames included, though it is written), and 2 for usage
errors. An installed Celesta older than this option rejects it with
`unexpected argument '--json'`; then use `--no-ui` and read the text output
below.

### Progress events and cancellation

For a script that shows progress, `--json --json-progress` prints one JSON
line per event before the result. Each has an `event`; the result line has
none:

```json
{"event":"compositions","compositionId":"default","compositions":[{"id":"default","width":1920,"height":1080,"frameRate":{"numerator":30,"denominator":1},"durationInFrames":150}]}
{"event":"composition","composition":{"width":1920,"height":1080,"fps":30.0,"frames":150,"duration":5.0}}
{"event":"rendering","frame":1,"total":150}
{"event":"warning","message":"…"}
{"event":"mixingAudio"}
{"event":"muxing"}
{"event":"wrote","output":{"type":"video","path":"/work/out.mp4","firstFrame":0,"frames":150,"start":0.0,"duration":5.0,"audio":true,"loudness":{"integratedLufs":-16.1,"truePeakDbtp":-1.67},"format":"mp4","alpha":false}}
```

`rendering` counts from 1 the frame that has started; `total` is the number
of frames the export renders (the selected stills, for a still export). The
`composition`, `compositions`, `warning`, and `wrote` events carry what the
result reports under the same names.

With `--stdin-cancel`, writing a `cancel` line to the exporter's stdin, or
closing it, stops the export before its next frame: the result is a
`cancelled` error, and the partial video is removed. Stills already written
stay. Since a closed stdin also cancels, an exporter started this way stops
when the process that started it exits.

From Node.js, `@celesta.studio/exporter` (`packages/exporter`) runs the exporter
this way and returns the result as typed objects; see its README.

## Progress output

Without `--json`, an interactive terminal gets a full-screen dashboard.
`--no-ui` makes the output plain text:

```text
rendering frame 300/1530  58.5 fps  elapsed 00:00:05  eta 00:00:21
mixing audio
muxing
```

Warnings (missing fonts, missing glyphs) are printed as `warning: …` lines;
a plugin's informational notice is printed as a `note: …` line.
A failure prints `Celesta export: <message>` and
exits non-zero; see [errors.md](errors.md). The rendering count can reach
the total while audio mixing and muxing are still running; wait for the
process to exit.

## Quality notes

Export renders on the GPU when one is available. Layers that are only moved
are copied pixel for pixel. Scaled and rotated layers are filtered: their
edges are anti-aliased, shrunk images use mipmaps, and in `final` quality
text is rasterized at the size it is drawn at. The app preview draws in
`draft` quality while playing and in `final` quality when paused, so a
paused frame matches the export.

For speed problems, see [performance.md](performance.md).

## Without a GPU

On Linux machines without a GPU (CI, cloud containers), the exporter needs
Mesa's software Vulkan driver (`sudo apt-get install -y mesa-vulkan-drivers`)
and renders on the CPU, roughly 10–60 fps at 1080p on 4 cores: check a
single PNG frame before a full export. Minimal containers often have no
fonts; install some (`fonts-dejavu-core`, `fonts-noto-cjk` for Japanese) or
load font files with `<Font>`.

The Linux container image does all of this:

```sh
docker run --rm --user "$(id -u):$(id -g)" -v "$PWD:/work" celesta-exporter --react film.tsx film.mp4
```
