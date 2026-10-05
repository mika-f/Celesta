# Export to MP4

The command-line exporter renders a React composition or a JSON project to
an H.264 + AAC MP4 (or to PNG frames, see [verify.md](verify.md)). Find it
first: [setup.md](setup.md#find-the-celesta-tools). `Celesta-export` below
stands for its path.

## Contents

- [Commands](#commands)
- [Options](#options)
- [JSON results](#json-results)
- [Progress output](#progress-output)
- [Quality notes](#quality-notes)
- [Without a GPU](#without-a-gpu)

## Commands

```sh
Celesta-export [options] project.celesta.json out.mp4
Celesta-export [options] --react scene.tsx out.mp4
Celesta-export [options] --react scene.tsx --project project.celesta.json out.mp4
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
| `--project <file>` | Companion JSON project for `<ProjectTimeline />`/`<ProjectTrack />` (needs `--react`). |
| `--from <t>`, `--to <t>` | Export only this span (`--to` exclusive); the output starts at 00:00. Times are seconds, `MM:SS.mmm`, or `HH:MM:SS.mmm`. |
| `--overwrite` | Replace an existing output file. |
| `--preset <p>` | libx264 preset, `ultrafast` … `veryslow` (default `medium`). Encoding speed against file size only; it does not change what is rendered. |
| `--crf <n>` | 0–51, lower is better quality (default 18). |
| `--color-conversion <w>` | `auto` (default), `gpu`, or `encoder`: where RGB becomes YUV. |
| `--render-quality <q>` | `final` (default) or `draft`. `draft` draws scaled text from a scale-1 texture, so it is softer; use it only to check timing. |
| `--json` | No progress output; one line of JSON on stdout when the export ends. See [JSON results](#json-results). |
| `--no-ui` | Plain text progress instead of the terminal dashboard (automatic when stderr is not a terminal). |
| `--frame`, `--frames`, `--every`, `--contact-sheet`, `--columns`, `--tile-width`, `--output-format` | PNG output; see [verify.md](verify.md#look-at-real-frames). |

MP4 export needs non-zero, even `width` and `height`.

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

(Shown wrapped here; the real output is one line.)

- `composition.frames` is the frame count. Frames are zero-based, so the
  last one is `frames - 1`. Times are in seconds, rounded to milliseconds.
- `outputs` lists the files that exist after the export, with absolute
  paths. Read these paths; do not work out file names yourself. Each entry
  has a `type`:
  - `video`: `firstFrame`, `frames`, `start`, `duration`, and `audio`
    (whether the MP4 has an audio stream);
  - `frame`: one PNG, with `frame` and `time`;
  - `contactSheet`: one PNG, with `columns` and the tile `frames` in
    reading order.
- `stats.renderFps` is the average rendering speed. Compare it between
  spans to find slow parts.
- On failure, `status` is `"error"` and `error` has a stable `code`, the
  `message`, and sometimes a `hint`. `outputs` still lists the PNGs written
  before the failure. `composition` is included when the failure happened
  after the composition was loaded.

| `error.code` | Meaning |
| --- | --- |
| `usage` | The arguments were rejected (exit status 2). |
| `output_exists` | An output file exists; add `--overwrite` only if you may replace it. |
| `invalid_selection` | A frame is out of range or repeated, or the selection or contact sheet is too large. |
| `empty_range` | `--from`/`--to` cover no frames of the composition. |
| `unsupported_output` | An MP4 export needs a `.mp4` path. |
| `unsupported_dimensions` | MP4 needs even width and height. |
| `project_io`, `project_json` | The project file could not be read or is not valid JSON. |
| `project_validation` | The project is invalid; `error.issues` lists each `{path, message}`. |
| `react` | The React entry failed to load or render; read `message` and look it up in [errors.md](errors.md). |
| `evaluation`, `render`, `audio`, `ffmpeg`, `io`, `time` | The export failed at that stage; read `message`. |

The exit status is 0 on success, 1 when the export fails, and 2 for usage
errors. An installed Celesta older than this option rejects it with
`unexpected argument '--json'`; then use `--no-ui` and read the text output
below.

## Progress output

Without `--json`, an interactive terminal gets a full-screen dashboard.
`--no-ui` makes the output plain text:

```text
rendering frame 300/1530  58.5 fps  elapsed 00:00:05  eta 00:00:21
mixing audio
muxing MP4
```

Warnings (missing fonts, missing glyphs) are printed as `warning: …` lines.
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
