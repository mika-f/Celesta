# .celesta.json project reference

A project file is a JSON timeline: settings, assets, characters, tracks of
timed items, and free-form properties. Celesta validates the whole file on
load and refuses it with `path: message` errors (for example
`tracks[0].items[2].content.asset: references missing asset "bgm"`).

## Contents

- [Skeleton](#skeleton)
- [Time](#time)
- [settings](#settings)
- [assets](#assets)
- [tracks and items](#tracks-and-items)
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
| `width`, `height` | yes | Pixels, > 0. Even numbers for MP4 export. |
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
- URL sources are downloaded once and cached; change the URL to refresh.
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

Same fields as `video`, referencing an `audio` asset. No picture.

### image

```json
{ "type": "image", "asset": "logo" }
```

Drawn at its natural size, centered on `transform.position`.

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

`style` has the same fields as React's `TextStyle`. `\n` breaks lines.
There is no `maxWidth` on text items (only on character subtitles).

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
[verify-and-export.md](verify-and-export.md#export)). Exporting the JSON
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
  `playbackRate`) can be a plain number or keyframes.

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
- `opacity` values within 0–1; `volume` ≥ 0; `playbackRate` > 0; transform
  numbers finite.
- Keyframe lists non-empty, times ≥ 0 and ascending.
- Text styles: `fontSize` and `lineHeight` > 0, stroke `width` ≥ 0, colors
  exactly `#RRGGBB` or `#RRGGBBAA`.
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
