# .celesta.json project reference

A project file is a JSON timeline: settings, assets, characters, tracks of
timed items, and free-form properties. Use it for explicit, hand-placed
timelines; for anything computed, write React instead (or combine both, see
[project-data.md](project-data.md)). To check a project, export one PNG frame
([verify.md](verify.md#check-a-json-project)). Celesta validates the whole file on
load and refuses it with `path: message` errors (for example
`tracks[0].items[2].content.asset: references missing asset "bgm"`).

## Contents

- [Skeleton](#skeleton)
- [Time](#time)
- [settings](#settings)
- [assets](#assets)
- [tracks and items](#tracks-and-items)
- [Color filters](#color-filters)
- [Item content types](#item-content-types)
- [transform and opacity](#transform-and-opacity)
- [Keyframes](#keyframes)
- [Easing names](#easing-names)
- [Validation rules](#validation-rules)
- [Complete example](#complete-example)

## Skeleton

All six top-level keys are required, even when empty.

```json
{
  "version": 0,
  "settings": {
    "width": 1920,
    "height": 1080,
    "frameRate": { "numerator": 30, "denominator": 1 },
    "sampleRate": 48000
  },
  "assets": {},
  "characters": {},
  "tracks": [],
  "properties": {}
}
```

## Time

Every time is a rational `{ "value": n, "timescale": d }` meaning `n / d`
seconds; `timescale` must be > 0.

- `{ "value": 10, "timescale": 1 }` is 10 s.
- `{ "value": 15, "timescale": 30 }` is frame 15 at 30 fps (0.5 s).
- Use the frame rate as the timescale when thinking in frames, and `1` when
  thinking in whole seconds. `{ "value": 2500, "timescale": 1000 }` is 2.5 s.

A **range** is `{ "start": Time, "duration": Time }` on the project timeline.

## settings

| Key | Required | Notes |
| --- | --- | --- |
| `width`, `height` | yes | Pixels, > 0. Even numbers for video export. |
| `frameRate` | yes | `{ "numerator": 30, "denominator": 1 }`. 29.97 fps is `30000/1001`. |
| `sampleRate` | yes | Audio mix rate, usually `48000`. |
| `duration` | no | Time. When omitted, the project ends at the latest item end. |
| `masterVolume` | no | Linear gain ≥ 0. |
| `reactEntry` | no | Path to a `.tsx` whose `registerComponent` calls define `component` items (used by the app to find property schemas). |

## assets

A map from asset id to a declaration. Items and characters refer to assets
by id, and the kind must match the use.

```json
"assets": {
  "gameplay": { "type": "video", "source": { "type": "file", "path": "./media/gameplay.mp4" } },
  "bgm":      { "type": "audio", "source": { "type": "file", "path": "./media/bgm.wav" } },
  "logo":     { "type": "image", "name": "Logo", "source": { "type": "file", "path": "./media/logo.png" } },
  "rounded":  { "type": "font",  "source": { "type": "file", "path": "./fonts/MPLUSRounded1c-Bold.ttf" } },
  "remote":   { "type": "image", "source": { "type": "url", "url": "https://example.com/card.png" } }
}
```

- `type`: `video`, `audio`, `image`, or `font`. `name` is an optional label.
- File paths are relative to the project file's folder.
- URL sources are downloaded once and cached; change the path or query (for
  example add `?v=2`) to refresh. A `data:` URL is decoded instead.
- Font assets are loaded for every text item; refer to them by the font's
  internal family name in `style.fontFamily`.

## tracks and items

```json
{
  "id": "titles",
  "name": "Titles",
  "kind": "overlay",
  "items": [ /* TimelineItem */ ]
}
```

| Track key | Notes |
| --- | --- |
| `id` | Non-empty, unique among tracks. |
| `name` | Label. |
| `kind` | `video`, `audio`, `overlay`, or `dialogue`. Match it to the content (dialogue lines on a `dialogue` track, and so on). |
| `enabled` | `false` hides and silences the track. |
| `muted`, `solo` | Audio only. If any track is solo, only solo tracks are heard. |
| `locked` | Editor hint; no effect on output. |

Tracks are drawn in array order: **later tracks draw on top.** Put
backgrounds and footage first, titles and subtitles last.

| Item key | Notes |
| --- | --- |
| `id` | Non-empty, unique across the **whole project**, not just the track. |
| `name` | Optional label. |
| `range` | When the item is on screen / audible. |
| `content` | See below. |
| `enabled` | `false` skips the item. |
| `transform` | Optional; see below. |
| `opacity` | `0`–`1`, number or keyframes. |
| `blendMode` | `normal` (default), `multiply`, `screen`, `overlay`, `add`, `difference`, `darken`, `lighten`, `colorDodge`, `colorBurn`, `hardLight`, `softLight`, `exclusion`, `hue`, `saturation`, `color`, or `luminosity`. |
| `effects` | Optional `{ "blur": numberOrKeyframes, "shadow": { "color": colorOrKeyframes, "blur": numberOrKeyframes, "offsetX": numberOrKeyframes, "offsetY": numberOrKeyframes }, "glow": { "color": colorOrKeyframes, "blur": numberOrKeyframes } }`, plus the color filters below. Colors are `#RRGGBB` or `#RRGGBBAA`; color keyframes interpolate RGBA channels. Blur radii are 0–64 output pixels. |

### Color filters

`effects` also takes the color filters React layers take as props (see
the layer props in [react-core.md](react-core.md)), with the same names,
ranges, and order: they apply in the order of this list, before `shadow`,
`glow`, and `blur`.

```json
"effects": {
  "brightness": 1.1,
  "contrast": 1.2,
  "saturation": { "type": "keyframes", "keyframes": [
    { "time": { "value": 0, "timescale": 1 }, "value": 1 },
    { "time": { "value": 2, "timescale": 1 }, "value": 0 }
  ] },
  "hue": 30,
  "grayscale": 0.5,
  "invert": 0,
  "tint": { "color": "#704214", "amount": 0.35 },
  "vignette": { "amount": 0.6, "radius": 0.5, "softness": 0.5 }
}
```

| Key | Value |
| --- | --- |
| `brightness`, `contrast`, `saturation` | numberOrKeyframes, at least 0; 1 changes nothing |
| `hue` | numberOrKeyframes, degrees |
| `grayscale`, `invert` | numberOrKeyframes, 0–1; 0 changes nothing |
| `tint` | `{ "color": colorOrKeyframes, "amount"?: numberOrKeyframes }`; amount 0–1, default 1 |
| `vignette` | `{ "amount": numberOrKeyframes, "radius"?: numberOrKeyframes, "softness"?: numberOrKeyframes }`; amount 0–1, radius and softness at least 0, default 0.5 |

Keyframe times are relative to the item, as for `opacity`. Values that
change nothing at a frame add no work there. A `component` item drawn by
`<ProjectTimeline />` keeps its effects, color filters included.

## Item content types

The `type` field selects the variant.

### video

```json
{ "type": "video", "asset": "gameplay",
  "sourceRange": { "start": { "value": 12, "timescale": 1 } },
  "playbackRate": 1, "volume": 0.6, "muted": false }
```

Plays the picture **and** its soundtrack. `sourceRange.start` trims the
head (seconds into the file); optional `sourceRange.duration` limits it.
`playbackRate` (> 0) and `volume` (≥ 0) accept keyframes. Set
`"muted": true` for footage without sound.

### audio

Same fields as `video`, referencing an `audio` asset. No picture. Also takes
optional `pitch` (semitones, number or keyframes, default 0, −24…24; fractions
are cents) and `preservePitch` (boolean, default false: `playbackRate` moves
speed and pitch together; true: it changes only the duration and `pitch` alone
sets the pitch). Heard transposition is
`2^(pitch/12) × (preservePitch ? 1 : playbackRate)`. `video` items do not take
them, and `@celesta/web`'s `exportMp4` rejects them.

`fadeIn` / `fadeOut` (times, `{ "value": 3, "timescale": 2 }`, ≥ 0, default
0) ramp the item linearly from silence at its start and to silence at its
end, multiplying `volume`. They are timed on the item's `range`, so `loop`,
`playbackRate`, and `pitch` do not move them; fades longer than the item
overlap and multiply. `video` items and dialogue voices do not take them.

```json
{ "type": "audio", "asset": "bgm", "volume": 0.6,
  "fadeIn": { "value": 3, "timescale": 2 }, "fadeOut": { "value": 3, "timescale": 1 } }
```

`duckGroup` (a non-empty string) and `duck` duck an item under others, as
`<Audio duck>` does (see react-core.md, Ducking). `audio` and `video` items
take both; `dialogue` items take `duckGroup` only, and their voices belong to
`"voice"` unless it says otherwise (`null` for no group). `duck` has `under`
(a group or a list of them), `to` (a gain from 0 to 1, or `{ "db": n }` at or
below 0; default 0.3), and `attack`, `release`, `holdGap` as times (defaults
0.15 s, 0.4 s, 0.6 s). Each broken rule is reported at its own path, such as
`tracks[0].items[0].content.duck.to`.

```json
{ "type": "audio", "asset": "bgm", "volume": 0.6,
  "duck": { "under": "voice", "to": { "db": -10 }, "release": { "value": 1, "timescale": 2 } } }
```

### image

```json
{ "type": "image", "asset": "logo" }
{ "type": "image", "asset": "photo", "width": 800, "height": 450, "fit": "cover" }
```

Centered on `transform.position`. Image assets may be PNG, JPEG, WebP, GIF,
PNM, or SVG. An animated GIF, APNG, or WebP plays from the item's start,
looping, each frame for its own delay. Without `width`/`height` (finite, positive) the image is drawn at its
natural size; one of them preserves the aspect ratio; both stretch to that
box unless `fit` is `"contain"` (centered, transparent letterboxing) or
`"cover"` (centered, cropped). SVGs rasterize at their drawn size, use
`var(--x, fallback)` fallbacks, and keep their own backgrounds.

### text

```json
{ "type": "text", "text": "Chapter 1",
  "style": {
    "fontFamily": "M PLUS Rounded 1c", "fontSize": 96, "fontWeight": 700,
    "fill": { "type": "solid", "color": "#FFFFFFFF" },
    "stroke": { "paint": { "type": "solid", "color": "#3A2D52FF" }, "width": 6 },
    "align": "center", "lineHeight": 120
  } }
```

`stroke` may also be a list, such as
`[{ "paint": …, "width": 14 }, { "paint": …, "width": 7 }]`, for a double
outline: each width is from the glyph outline.

`style` has the same fields as React's `TextStyle` (see
[text.md](text.md#textstyle)), including `letterSpacing` (px, or
`{ "em": -0.02 }` for a multiple of `fontSize`), `fontFeatures`
(`{ "liga": false, "calt": false }`), and `lineBreak`.
Paints are objects, and JSON colors are `#RRGGBB` or `#RRGGBBAA`. `\n` breaks
lines. There is no `maxWidth` on text items (only on character subtitles),
so text items never wrap on their own. Fonts come from `font` assets or
installed families.

### dialogue

A character's line: subtitle, optional voice, expression, and lip-sync cues.
See [dialogue.md](dialogue.md#json-projects).

### component

```json
{ "type": "component", "component": "LowerThird", "props": { "name": "Mira", "role": "Host" } }
```

Rendered by a React component registered with `registerComponent`, and
only when the project is exported together with a React entry that
renders `<ProjectTimeline />` (see
[project-data.md](project-data.md#registercomponent) and
[export.md](export.md#commands)). Exporting the JSON
project on its own fails with an `unsupported … missing component` error on
that layer; so does registering a name that does not match `component`
exactly.

## transform and opacity

```json
"transform": {
  "position": { "x": 960, "y": 540 },
  "scale":    { "x": 0.5, "y": 0.5 },
  "rotation": 15,
  "anchor":   { "x": 0.5, "y": 0.5 }
},
"opacity": 0.8
```

- **`position` places the anchor, and the anchor defaults to the center**
  (`0.5, 0.5`). Without a transform an item is centered on the canvas's
  top-left corner, so almost every visual item needs a `position`.
  For a 1920×1080 canvas, the center is `{ "x": 960, "y": 540 }`.
- `scale` defaults to 1, `rotation` is in degrees clockwise.
- Every number (each `x`/`y`, `rotation`, `opacity`, `volume`,
  `playbackRate`, audio `pitch`) can be a plain number or keyframes.

## Keyframes

```json
"opacity": {
  "type": "keyframes",
  "keyframes": [
    { "time": { "value": 0,  "timescale": 30 }, "value": 0 },
    { "time": { "value": 15, "timescale": 30 }, "value": 1, "easing": "ease-out" }
  ]
}
```

- Keyframe times are **relative to the item's `range.start`**.
- Keyframes must be non-empty and in ascending time order.
- `easing` on a keyframe shapes the curve **into** that keyframe. Omitted
  means linear.
- Before the first keyframe and after the last one, the boundary value holds.

A slide-in from the left over half a second:

```json
"transform": {
  "position": {
    "x": { "type": "keyframes", "keyframes": [
      { "time": { "value": 0,  "timescale": 30 }, "value": -400 },
      { "time": { "value": 15, "timescale": 30 }, "value": 360, "easing": "ease-out-cubic" }
    ] },
    "y": 900
  }
}
```

## Easing names

`linear`, `ease-in`, `ease-out`, `ease-in-out` (quadratic), and
`ease-in-X`, `ease-out-X`, `ease-in-out-X` for X in `sine`, `quad`,
`cubic`, `quart`, `quint`, `expo`, `circ`, `back`, `elastic`, `bounce`
(for example `ease-in-out-cubic`, `ease-out-back`). These match React's
`Easings` in kebab case.

## Validation rules

Celesta rejects the project if any of these fail:

- `version` is `0`.
- `settings.width`, `height`, `sampleRate` > 0; `frameRate` numerator and
  denominator > 0; `masterVolume` finite and ≥ 0; `duration` > 0.
- Track ids non-empty and unique; item ids non-empty and unique across the
  project.
- Every `range.start` ≥ 0 and every `range.duration` > 0.
- Every asset reference exists and has the right kind (`video` item → video
  asset, `audio` → audio, `image` → image, portrait expressions and mouths →
  image, dialogue `audio` → audio).
- `opacity` values within 0–1; `volume` ≥ 0; `playbackRate` > 0; audio `pitch`
  finite within −24…24; transform numbers finite.
- Keyframe lists non-empty, times ≥ 0 and ascending.
- Effects: blur radii 0–64; shadow and glow colors and the tint color
  exactly `#RRGGBB` or `#RRGGBBAA`; `brightness`, `contrast`,
  `saturation`, the vignette's `radius` and `softness` ≥ 0; `grayscale`,
  `invert`, the tint's and the vignette's `amount` within 0–1; `hue`
  finite.
- Text styles: `fontSize` and `lineHeight` > 0, each stroke `width` ≥ 0, colors
  exactly `#RRGGBB` or `#RRGGBBAA`, `lineBreak` `"normal"` or `"phrase"`.
- Characters: `defaultExpression` must be a key of `expressions`;
  `subtitle.maxWidth` > 0.
- Dialogue items: `character` exists; `expression` exists on that
  character; `lipSync` cues need an `audio` asset and the character's
  `portrait.lipSync`; cue times strictly ascending and within the item's
  duration.
- Component items: `component` is non-empty.

JSON syntax matters too: no comments and no trailing commas.

## Complete example

A 10-second 1080p video: footage with its own sound, background music that
fades in, a logo, and an animated title.

```json
{
  "version": 0,
  "settings": {
    "width": 1920,
    "height": 1080,
    "frameRate": { "numerator": 30, "denominator": 1 },
    "sampleRate": 48000,
    "duration": { "value": 10, "timescale": 1 }
  },
  "assets": {
    "gameplay": { "type": "video", "source": { "type": "file", "path": "./media/gameplay.mp4" } },
    "bgm": { "type": "audio", "source": { "type": "file", "path": "./media/bgm.wav" } },
    "logo": { "type": "image", "source": { "type": "file", "path": "./media/logo.png" } }
  },
  "characters": {},
  "tracks": [
    {
      "id": "footage", "name": "Footage", "kind": "video",
      "items": [{
        "id": "clip-1",
        "range": { "start": { "value": 0, "timescale": 1 }, "duration": { "value": 10, "timescale": 1 } },
        "content": { "type": "video", "asset": "gameplay", "volume": 0.8 },
        "transform": { "position": { "x": 960, "y": 540 } }
      }]
    },
    {
      "id": "music", "name": "Music", "kind": "audio",
      "items": [{
        "id": "bgm-1",
        "range": { "start": { "value": 0, "timescale": 1 }, "duration": { "value": 10, "timescale": 1 } },
        "content": {
          "type": "audio", "asset": "bgm",
          "volume": { "type": "keyframes", "keyframes": [
            { "time": { "value": 0, "timescale": 1 }, "value": 0 },
            { "time": { "value": 2, "timescale": 1 }, "value": 0.4, "easing": "ease-out" }
          ] }
        }
      }]
    },
    {
      "id": "overlay", "name": "Overlay", "kind": "overlay",
      "items": [
        {
          "id": "logo-1",
          "range": { "start": { "value": 0, "timescale": 1 }, "duration": { "value": 10, "timescale": 1 } },
          "content": { "type": "image", "asset": "logo" },
          "transform": { "position": { "x": 1760, "y": 100 }, "scale": { "x": 0.5, "y": 0.5 } },
          "opacity": 0.9
        },
        {
          "id": "title-1",
          "range": { "start": { "value": 1, "timescale": 1 }, "duration": { "value": 4, "timescale": 1 } },
          "content": {
            "type": "text", "text": "Stage 1",
            "style": {
              "fontSize": 120, "fontWeight": 700, "align": "center",
              "fill": { "type": "solid", "color": "#FFFFFFFF" },
              "stroke": { "paint": { "type": "solid", "color": "#000000CC" }, "width": 8 }
            }
          },
          "transform": { "position": { "x": 960, "y": 540 } },
          "opacity": { "type": "keyframes", "keyframes": [
            { "time": { "value": 0, "timescale": 30 }, "value": 0 },
            { "time": { "value": 15, "timescale": 30 }, "value": 1, "easing": "ease-out" },
            { "time": { "value": 105, "timescale": 30 }, "value": 1 },
            { "time": { "value": 120, "timescale": 30 }, "value": 0, "easing": "ease-in" }
          ] }
        }
      ]
    }
  ],
  "properties": {}
}
```
