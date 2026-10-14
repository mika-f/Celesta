# Photo slideshow template

A title card, then one photo per slide with a slow zoom and its caption along
the bottom, joined by transitions, over music that fades out at the end.
`film.tsx` declares its inputs with `defineProjectProperties()` and
`props.json` holds the values this project renders with. `prepare()` reads
the slides file named by the `slides` property, and the composition's length
follows the number of slides. 1920 × 1080 (16:9) or 1080 × 1920 (9:16),
30 fps.

Create a project from it:

```sh
celesta-editor --init my-trip --template slideshow
cd my-trip
celesta-editor film.tsx
celesta-exporter --react film.tsx trip.mp4
```

## What to replace

- `assets/` — the sample photos (`photo-01.png` … `photo-05.png`) and music
  (`music.wav`). Put your own photos here, or anywhere `slides.json` can
  point at. Photos fill the frame and are cropped to it, so landscape photos
  lose their sides in 9:16.
- `slides.json` — the slides, in order: an array of
  `{ "image": ..., "caption": ... }`. `image` is required; a relative path
  resolves from the folder holding `slides.json`. `caption` is optional, and
  long captions shrink to fit two lines. Each slide adds `secondsPerSlide` to
  the video.
- `props.json` — the title, colors, format, transition, timing, and the
  slides and music files. Preview and export load it automatically. Set
  `"music": ""` for a silent slideshow; music shorter than the slideshow
  repeats.
- The layout numbers, colors, and motion timings (title card length,
  transition length, zoom, music fade) are in the constants block at the
  top of `film.tsx`.

## Properties

To change these without editing JSON, open `film.tsx` in Celesta and use the preview panel's **Properties** section; it saves to the props file in use (`props.json`, or the `--props-file` the editor was opened with).

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `title` | string | `旅の記録` | The title card's heading. Long titles wrap onto two lines and shrink to fit. |
| `subtitle` | string | `2026 秋` | The line under the heading, in the accent color. |
| `format` | `16:9` / `9:16` | `16:9` | 1920 × 1080, or 1080 × 1920 with captions kept higher. |
| `accent` | color | `#f2c14e` | The rule and subtitle on the title card. |
| `transition` | `crossfade` / `slide` / `wipe` / `iris` / `clock-wipe` / `zoom` / `dissolve` / `blur` / `flip` / `cube` | `crossfade` | How each scene gives way to the next. |
| `secondsPerSlide` | number | `4` | How long each slide stays, including its transitions; at least 1.33. |
| `slides` | path | `./slides.json` | The slides to show. An http(s) URL is fetched instead. |
| `music` | path | `./assets/music.wav` | The music; `""` for none. |

## Another set of slides

`--props-file` renders with another set of values instead of `props.json`.
A file such as `kyoto.json` beside `film.tsx`:

```json
{
  "title": "京都 冬の旅",
  "subtitle": "2027 冬",
  "format": "9:16",
  "transition": "slide",
  "secondsPerSlide": 5,
  "slides": "kyoto/slides.json",
  "music": ""
}
```

```sh
celesta-exporter --react film.tsx kyoto.mp4 --props-file kyoto.json
```

Relative paths in it resolve from its own folder, and properties left out
of it use the defaults in `film.tsx`. `--props` overrides single values
without another file:

```sh
celesta-exporter --react film.tsx trip-vertical.mp4 --props '{"format":"9:16"}'
```

## In the Celesta repository

The same commands work from a checkout of the Celesta repository, with the
paths starting at `templates/slideshow/`:

```sh
cargo run -p celesta-exporter --release -- --react templates/slideshow/film.tsx trip.mp4
```

To regenerate the sample media: the photos are drawn with Celesta and the
music is synthesized, by scripts in `source/` (not copied into projects).
`source/generate.sh` writes all six files into `assets/`:

```sh
CELESTA_EXPORTER="cargo run -p celesta-exporter --release --" sh templates/slideshow/source/generate.sh
```

## License

The sample photos and music are made for this template and licensed like the
Celesta repository (MIT OR Apache-2.0). Noto Sans JP is loaded from Google
Fonts (SIL Open Font License).
