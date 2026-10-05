# Export to MP4

The command-line exporter renders a React composition or a JSON project to
an H.264 + AAC MP4 (or to PNG frames, see [verify.md](verify.md)). Find it
first: [setup.md](setup.md#find-the-celesta-tools). `Celesta-export` below
stands for its path.

## Contents

- [Commands](#commands)
- [Options](#options)
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
Celesta-export --no-ui --overwrite --preset ultrafast --from 2 --to 3 \
  --react scene.tsx /tmp/celesta-check.mp4
```

The final export the user asked for:

```sh
Celesta-export --no-ui --react scene.tsx out.mp4
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
| `--no-ui` | Plain text progress instead of the terminal dashboard (automatic when stderr is not a terminal). |
| `--frame`, `--frames`, `--every`, `--contact-sheet`, `--columns`, `--tile-width`, `--output-format` | PNG output; see [verify.md](verify.md#look-at-real-frames). |

MP4 export needs non-zero, even `width` and `height`.

## Progress output

In an interactive terminal the exporter draws a full-screen dashboard. Pass
`--no-ui` when you run it so the output is plain text that you can read:

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
