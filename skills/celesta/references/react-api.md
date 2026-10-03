# @celesta/react API reference

Everything is imported from `@celesta/react`, except the random, noise, and
math helpers, which come from `@celesta/math`. The runtime bundled with
Celesta provides `react` (18.x), `@celesta/react`, and `@celesta/math`; do
not install them from npm.

## Contents

- [Entry file shape](#entry-file-shape)
- [Common layer props](#common-layer-props)
- [Layers](#layers): Composition, Rect, Path, Text, Group, Image, Video, Audio, Font, Assets
- [Text styles and fonts](#text-styles-and-fonts)
- [Time and animation](#time-and-animation): hooks, interpolate, Easings, spring, Sequence, Transition, timecodeToFrame
- [Motion helpers](#motion-helpers): progress, Series, Stagger, planDialogue/DialogueSeries, useBeat, useCue, TextReveal, useTypewriter, useCountUp, Camera, Line, Polyline, frameToTimecode
- [@celesta/math](#celestamath): random, noise, noise2D/3D, fbm, clamp, lerp, remap, smoothstep, waves, angles, points
- [Layout helpers](#layout-helpers): Center, SafeArea, Stack, Grid, Fit
- [Media helpers](#media-helpers): preloadMedia, mediaDurationInFrames, measureText, useTextMetrics
- [Project data](#project-data): ProjectProvider, useProjectProperty, defineProjectProperties, ProjectTimeline, ProjectTrack, registerComponent
- [Rendering cost](#rendering-cost): what makes preview and export slow, and cheaper ways to get the same picture
- [Preview-only debug guides](#preview-only-debug-guides)
- [Keyframe values](#keyframe-values)

## Entry file shape

```tsx
// Optional. Runs once, before the first frame, in preview and export.
export async function prepare(): Promise<void> { /* fetch, load, probe */ }

// Required. Returns exactly one <Composition>.
export default function Root() {
  return <Composition width={1920} height={1080} fps={30} durationInFrames={300}>…</Composition>;
}
```

- `durationInFrames / fps` is the length in seconds. Frames run from `0` to
  `durationInFrames - 1`.
- `durationInFrames` may be computed from values that `prepare()` stored
  (for example, a voice or clip length).
- Module-level code runs once. `registerComponent()` and
  `defineProjectProperties()` must be called at module level.
- `console.log` output goes to the terminal (stderr), which is useful when
  running `scripts/inspect.mjs`.

## Common layer props

Every visual layer (`Rect`, `Text`, `Group`, `Image`, `Video`,
`CharacterView`, `Dialogue`, `Sequence`, layout helpers, `Transition`)
accepts:

| Prop | Default | Meaning |
| --- | --- | --- |
| `x`, `y` | `0` | Position of the anchor point in the parent's coordinates, in pixels. Origin is the top-left of the canvas; y grows downward. |
| `anchorX`, `anchorY` | `0` | Normalized pivot: `0` left/top, `0.5` center, `1` right/bottom. Positioning, rotation, and scale all use it. |
| `rotation` | `0` | Degrees, clockwise. |
| `scale` | `1` | Uniform scale. `scaleX`/`scaleY` override one axis. |
| `opacity` | `1` | `0`–`1`; multiplies down through groups. |
| `blendMode` | `'normal'` | How the layer combines with what is beneath it: `'normal'`, `'multiply'`, `'screen'`, `'overlay'`, `'add'`, or `'difference'`, as in CSS `mix-blend-mode`. |
| `blur` | `0` | Gaussian blur radius in output pixels, from 0 to 64. |
| `shadow` | none | `{ color, blur, offsetX, offsetY }`; hex color, 0–64 px blur, output-pixel offsets. |
| `glow` | none | `{ color, blur }`; centered halo with a 0–64 px blur. |
| `id` | generated | Optional stable layer id. |

To center something at a point, set `anchorX={0.5} anchorY={0.5}` and put
the point in `x`/`y`.

Effects apply to the composited pixels of the layer; on a `Group` they apply
to all children together. They combine with `opacity` and `blendMode`. Compute
any effect prop from `useCurrentFrame()` to animate it.

## Layers

### `<Composition>`

`width`, `height`, `fps`, `durationInFrames`: all required positive
integers. Use even `width`/`height` for MP4 export. Exactly one, at the root.
These props are read once when the entry loads, so they must not depend on
the current frame.

### `<Rect>`

| Prop | Notes |
| --- | --- |
| `width`, `height` | Required, pixels. |
| `fill` | Hex string (`"#RRGGBB"` / `"#RRGGBBAA"`) or a gradient `Paint` (see below). Omit for no fill. |
| `stroke`, `strokeWidth` | Inner stroke (hex or `Paint`); needs both to be visible. |
| `cornerRadius` | Pixels. A square with `cornerRadius = size / 2` is a circle. |

A full-canvas background: `<Rect width={1920} height={1080} fill="#101018" />`
as the first child.

Gradients: pass a `Paint` to `fill`/`stroke` (or a text style's `fill` /
`stroke.paint`). Coordinates are local pixels from the layer's top-left (for
text, its layout box). Stops are `{ offset: 0..1, color }`; colors may carry
alpha, so a gradient can fade to transparent. At least 2 stops.

```tsx
<Rect width={1920} height={1080}
  fill={{ type: 'linear', start: { x: 0, y: 0 }, end: { x: 0, y: 1080 },
          stops: [{ offset: 0, color: '#101018' }, { offset: 1, color: '#10101800' }] }} />
// radial: { type: 'radial', center: { x, y }, radius, stops }
```

### `<Path>`

A vector shape (lines and Bézier curves) as **one layer**, however many
segments it has: a translucent stroke that crosses itself is painted once,
and it stays sharp under `scale`. Prefer it to many thin `Rect`s for
procedural line art; group strands that share a width/opacity into one path
each (tens of paths, not thousands of rects).

| Prop | Notes |
| --- | --- |
| `points` | `[x, y]` pairs joined by straight segments. |
| `closed` | Joins the last point back to the first, without a seam. |
| `commands` | Instead of `points`: `{ type: 'moveTo' \| 'lineTo', x, y }`, `{ type: 'quadTo', x1, y1, x, y }`, `{ type: 'cubicTo', x1, y1, x2, y2, x, y }`, `{ type: 'close' }` (absolute, like SVG `M L Q C Z`). |
| `stroke`, `strokeWidth` | Hex or `Paint`; width defaults to 2. No stroke without `stroke`. |
| `cap` | `'butt'` (default), `'round'`, `'square'`. |
| `join` | `'miter'` (default), `'round'`, `'bevel'`; `miterLimit` (4) bevels sharper miters. |
| `fill` | Hex or `Paint` (non-zero rule); drawn under the stroke. |

Coordinates are the path's own pixels: `x`/`y` move their origin and
`scale`/`rotation` turn about it (there is no `anchorX`/`anchorY`). Gradient
`Paint` coordinates are in the same space.

```tsx
<Path points={[[0, 0], [200, 80], [400, 0]]} stroke="#EF402B" strokeWidth={3} join="round" />
<Path x={960} y={540} fill="#FFD84D" commands={[
  { type: 'moveTo', x: -40, y: 0 }, { type: 'quadTo', x1: 0, y1: -60, x: 40, y: 0 }, { type: 'close' }]} />
```

### `<Text>`

| Prop | Notes |
| --- | --- |
| children | Strings or numbers only (arrays of them are joined). Use template literals to combine values. `\n` breaks a line. |
| `style` | A `TextStyle`, see below. |
| `maxWidth` | Wrap lines to fit this width; `style.align` positions each line inside it. Lines break where Unicode line breaking (UAX #14) allows: at spaces in Latin text, and between most characters in Japanese and Chinese, which keeps punctuation such as `、` and `。` off the start of a line. A word wider than `maxWidth` is not split; the part past `maxWidth` is cut off. With `style.lineBreak: 'phrase'`, Japanese wraps only between phrases instead (see Text styles). |

Single-line text is anchored vertically by its visible glyph bounds, so
`anchorY={0.5}` centers the letters themselves. Horizontally it keeps its
advance width, so leading and trailing spaces take up room.

`anchorY="baseline"` anchors the text on its first line's baseline instead.
Use it to line up separate `Text` layers (colored runs, per-character
animation, mixed font sizes) at one `y`.

### `<Group>`

No size or appearance of its own. Children are positioned relative to the
group's `x`/`y`, and its rotation, scale, and opacity apply to all of them.

| Prop | Notes |
| --- | --- |
| `clip` | `{ x, y, width, height, cornerRadius }`: draw the children only inside this rectangle. `x`/`y` (default `0`) are its top-left corner and `cornerRadius` (default `0`) rounds it, both in the group's own coordinates, so the clip moves, scales, and rotates with the group. |

Use `clip` for a mask reveal (text sliding up from behind an invisible edge),
a wipe, or content scrolling inside a panel. The edge is anti-aliased, and
clips nested inside one another intersect (up to 8 deep on the GPU renderer).
A clip with no area (`width` or `height` of 0 or less) hides the children.

```tsx
<Group x={120} y={200} clip={{ width: 600, height: 80 }}>
  <Text y={80 * (1 - reveal)} style={{ fontSize: 64 }}>Layer it.</Text>
</Group>
```

With a `blendMode` other than `'normal'`, the group is isolated: its children
are drawn together first, and the result blends with what is beneath the
group as one layer, faded by the group's `opacity`. A HUD that must stay
readable over light and dark scenes can be drawn in a light color inside
`<Group blendMode="difference">`.

### `<Image>`

`src` (path or URL). PNG, JPEG, WebP, PNM, and SVG are supported.
Without dimensions, the image uses its natural size. `width` or `height`
alone preserves its aspect ratio. Both dimensions stretch to the display box;
`fit="contain"` centers the whole image with transparent letterboxing, while
`fit="cover"` centers and crops it to fill the box. `scale` applies afterwards.

```tsx
<Image src="./logo.svg" width={432} />
<Image src="./photo.png" width={800} height={450} fit="cover" />
```

SVGs rasterize at their display size and accumulated transform scale in preview
and export. `var(--name, fallback)` uses its fallback (including nested functions).
SVG backgrounds are preserved; remove unwanted backgrounds in the source.
Missing files and invalid SVGs produce asset errors. Rasterization is limited to
16384 pixels per side and 64 million pixels per image.

### `<Video>`

| Prop | Notes |
| --- | --- |
| `src` | Path or URL. Drawn at natural pixel size. |
| `startFrom` | Seconds into the file at the enclosing sequence's frame 0. Default `0`. |
| `playbackRate` | Static number only. Default `1`. |

**A React `<Video>` is picture only; it makes no sound.** To hear the clip's
soundtrack, add an `<Audio>` with the same `src`, `startFrom`, and
`playbackRate` next to it, inside the same `<Sequence>`.

### `<Audio>`

| Prop | Notes |
| --- | --- |
| `src` | Path or URL. Any file FFmpeg can decode, including a video file's audio. |
| `startFrom` | Seconds into the file. Default `0`. |
| `playbackRate` | Number or [keyframes](#keyframe-values). Default `1`. |
| `volume` | Linear gain, number or [keyframes](#keyframe-values). Default `1`. |
| `muted` | Default `false`. |

Bare, an `<Audio>` spans the whole composition. Inside a `<Sequence>` it
starts when the sequence starts and stops when it ends. An `<Audio>` behind a
conditional plays only on frames where it is rendered.

### `<Font>` and `<Assets>`

```tsx
<Assets>
  <Font src="./fonts/MPLUSRounded1c-Bold.ttf" />
  <Font src="https://fonts.googleapis.com/css2?family=M+PLUS+Rounded+1c:wght@400;700" />
</Assets>
```

- `<Font src>` loads TTF, OTF, WOFF, WOFF2, or a web-font CSS stylesheet
  (every `@font-face` in it is loaded). Refer to it by the **family name
  stored inside the font**, not the file name.
- `<Assets>` holds declarations that draw nothing: `Font`, `Character`, and
  `Image`/`Video`/`Audio` used only to create refs.
- Refs: `const logo = React.createRef<AssetReference>()`, then
  `<Assets><Image ref={logo} src="./logo.png" /></Assets>` and
  `<Image src={logo} />` elsewhere. Plain string paths are usually simpler.

## Text styles and fonts

```ts
type TextStyle = {
  fontFamily?: string;     // installed family, or one loaded with <Font>
  fontSize?: number;       // px, > 0
  fontWeight?: number;     // 100–900
  fill?: Paint;                                   // solid or gradient
  stroke?: { paint: Paint; width: number };       // outline
  align?: 'left' | 'center' | 'right';
  lineHeight?: number;     // px, > 0
  letterSpacing?: number;  // px added after each glyph; may be negative
  lineBreak?: 'normal' | 'phrase';  // where maxWidth may wrap; default 'normal'
};
```

- `lineBreak: 'phrase'` wraps Japanese only between phrases (文節), found
  with [BudouX](https://github.com/google/budoux)'s Japanese model, so a
  word such as `フレーム` or a trailing `の。` is not split across lines.
  Lines still break at spaces and at `\n`. A phrase wider than `maxWidth`
  wraps inside itself as `'normal'` text would. `measureText()` and `useTextMetrics()` lay out the same
  way, given the same style. It works the same for `<Text>`, character
  subtitles (`subtitle={{ maxWidth, style: { lineBreak: 'phrase' } }}`),
  and `.celesta.json` text styles. The browser canvas renderer
  (`@celesta/web`) ignores it and wraps only at whitespace.

- Weights match within the family first: when `fontFamily` has no face at
  the requested `fontWeight`, its nearest weight is used, picked as CSS font
  matching does. A family loaded only in Bold therefore draws `fontWeight`
  400, or no weight, in Bold. Bold is never synthesized.
- A `fontFamily` with no installed or loaded face falls back to another
  font. The Celesta app lists it with the preview's warnings, and
  `celesta-export` prints `warning: font family "…" (weight …) is not installed or loaded;
  text layer "…" uses a fallback font` to stderr, once per family and weight.
  `scripts/inspect.mjs` does not check fonts. For reproducible output, ship
  the font file next to the entry and load it with `<Font>`.
- Characters the family lacks fall back per glyph to another font, or are
  drawn as a missing-glyph box (tofu) when no font has them. Apart from
  emoji that a color emoji font draws, whitespace, and invisible
  characters, the app lists them with the preview's warnings, and
  `celesta-export` prints `warning: font family "…" (weight …) has no glyph
  for "…"; text layer "…" draws them with a fallback font` (the first 10
  characters, then `and N more characters`), naming each character once per
  family and weight. This catches a Google Fonts URL whose `text=` subset
  misses characters the video uses: add them to `text=`. A family with no
  face at all only gets the warning above.
- With Google Fonts, list every weight you use in the URL; a missing weight
  uses the family's nearest one.
- Emoji meant to look like emoji (🎉, and a character followed by U+FE0F
  such as ❤️ or 1️⃣, flags, skin tones, ZWJ sequences) are drawn with the
  first installed color emoji font of Apple Color Emoji, Segoe UI Emoji,
  Noto Color Emoji, Twemoji Mozilla, Twemoji, Twitter Color Emoji,
  JoyPixels, and EmojiOne Color (a `<Font>` with one of these families
  counts), even when a text font has a plain glyph for them. A `fontFamily`
  that names one of them draws the emoji itself. U+FE0E keeps a character
  as text.

## Time and animation

### Hooks

| Hook | Returns |
| --- | --- |
| `useCurrentFrame()` | Integer frame, local to the innermost `<Sequence>`. |
| `useCurrentTime()` | Exact `{ value, timescale }` time, local to the innermost `<Sequence>`. |
| `useVideoConfig()` | `{ width, height, fps, durationInFrames }`; inside a `Sequence`, `durationInFrames` is the sequence's. |
| `useIsPreview()` | `true` only in the Celesta app preview, `false` in exports. |

Hooks work in any component, including `Root`. However, `<Composition>`'s
own props are read once, when the entry is loaded: compute them from
constants or `prepare()` results, never from `useCurrentFrame()` or
`useVideoConfig()`.

### `interpolate(input, inputRange, outputRange, options?)`

```ts
interpolate(frame, [0, 20, 40], [0, 1, 0], {
  easing: Easings.easeInOutSine,     // applied inside each segment
  extrapolateLeft: 'clamp',          // 'extend' (default) | 'clamp' | 'identity'
  extrapolateRight: 'clamp',
});
```

`inputRange` must be strictly increasing and the same length as
`outputRange` (at least 2). **The default extrapolation is `extend`**, so
values keep changing past the range; pass `'clamp'` for fades and moves.

### `Easings`

`linear`, `easeIn`, `easeOut`, `easeInOut` (quadratic), and
`easeIn…`/`easeOut…`/`easeInOut…` for `Sine`, `Quad`, `Cubic`, `Quart`,
`Quint`, `Expo`, `Circ`, `Back`, `Elastic`, `Bounce` (for example
`Easings.easeOutBack`). Any `(t: number) => number` works too.

### `spring({ frame, fps, config?, from?, to?, delay?, durationInFrames? })`

Damped spring from `from` (default 0) to `to` (default 1), which may
overshoot. `config`: `stiffness` (100), `damping` (10), `mass` (1),
`overshootClamping` (false). Frames before `delay` return `from`.

```tsx
const frame = useCurrentFrame();
const { fps } = useVideoConfig();
const scale = spring({ frame, fps, delay: 10, config: { damping: 12 } });
```

### `<Sequence from? durationInFrames?>`

Shows its children only during `[from, from + durationInFrames)` in the
parent's frames (default: until the parent's end). Children are unmounted
outside this window, so their component code and hooks do not run. Entering
the window again mounts them afresh, resetting their local React state.
Declare fonts or assets needed throughout the composition in `<Assets>`
outside a sequence.
Inside, frames and media restart at 0. Sequences nest, and accept common
layer props. Use them to lay out scenes back to back:

```tsx
const scenes = [{ C: Intro, len: 90 }, { C: Body, len: 240 }, { C: Outro, len: 60 }];
let at = 0;
return <Composition width={1920} height={1080} fps={30} durationInFrames={390}>
  {scenes.map(({ C, len }, i) => {
    const from = at; at += len;
    return <Sequence key={i} from={from} durationInFrames={len}><C /></Sequence>;
  })}
</Composition>;
```

### `<Transition type durationInFrames …>`

Animates its children at the start (`direction="in"`, default) or end
(`direction="out"`) of the enclosing sequence (or composition).

| Prop | Notes |
| --- | --- |
| `type` | `'fade'`, `'slide'`, or `'scale'`, or an array to combine them (`['fade', 'slide']`). |
| `durationInFrames` | Positive integer. |
| `direction` | `'in'` (default) or `'out'`. |
| `slideFrom` | `'left'` (default), `'right'`, `'top'`, `'bottom'`. |
| `distance` | Slide distance in px, default `64`. |
| `scaleFrom` | Starting scale, default `0.8`. |
| `easing` | `(t) => number`, for example `Easings.easeOutCubic`. |

Nest transitions to combine an entrance and an exit.

### `timecodeToFrame(timecode, fps)` / `frameToTimecode(frame, fps)`

`'01:02.500'`, `'1:02:03'`, or `'12.5'` → frame number. Handy for syncing to
timestamps the user gives you. `frameToTimecode(frame, fps)` goes the other
way, formatting `HH:MM:SS:FF` for an on-screen clock.

## Motion helpers

Reach for these before writing the same math by hand. Each is built on
`Sequence`, `Group`, `Rect`, `Text`, and the frame, so it stays deterministic.

### `progress(frame, start, durationInFrames, easing?)`

Clamped 0–1 position of `frame` in the span, with `easing` applied (linear by
default). The building block of entrances:
`opacity={progress(frame, 10, 20, Easings.easeOutExpo)}`.

### `<Series>` and `computeSeries(items)`

Scenes back to back, by length. Children must be `<Series.Sequence
durationInFrames offset?>`; each is a `<Sequence>` (local frames restart at 0).
A negative `offset` overlaps the previous item. `computeSeries(items)` returns
`{ sequences: [{ from, durationInFrames }], durationInFrames }` without
rendering: use it for the `<Composition>` length and for anything outside the
series that needs a scene's start (a HUD, a wipe on the cut).

```tsx
const SCENES = [
  { name: 'intro', durationInFrames: 90, Scene: Intro },
  { name: 'body', durationInFrames: 240, Scene: Body },
];
const { durationInFrames } = computeSeries(SCENES);
// …
<Series>
  {SCENES.map(({ name, durationInFrames, Scene }) => (
    <Series.Sequence key={name} durationInFrames={durationInFrames}><Scene /></Series.Sequence>
  ))}
</Series>
```

### `planDialogue(lines, { fps, gap?, sceneLeadIn? })` and `<DialogueSeries>`

A voiced script laid out back to back from its recordings. In `prepare()`,
`planDialogue()` measures each line's `audio` and returns a plan: every
line's `from`, `durationInFrames` (the voice), `gapInFrames` after it
(default 0.25 s), and the total `durationInFrames` for the `<Composition>`.
`<DialogueSeries plan views>` renders one `<Sequence>` + `<Dialogue>` per
line, and `plan.startOf(id)`, `plan.range(a, b)`, `plan.scene(id)` time scene
cuts and camera moves to the lines. See
[dialogue.md](dialogue.md#timing-a-script-from-its-voices).

### `<Stagger each from? durationInFrames?>`

Wraps child *i* in a `<Sequence from={from + i * each}>`, so a component that
animates from its own frame 0 cascades down a list. A child is hidden until it
starts. Children carry their own positions (`Stagger` is not a layout helper;
it cannot sit inside `Grid`/`Stack` as one cell per child).

```tsx
function Row({ y, label }: { y: number; label: string }) {
  const p = progress(useCurrentFrame(), 0, 20, Easings.easeOutExpo);
  return <Text x={120 + 40 * (1 - p)} y={y} opacity={p}>{label}</Text>;
}
<Stagger each={4}>{items.map((s, i) => <Row key={s} y={200 + i * 64} label={s} />)}</Stagger>
```

### `useBeat({ bpm, beatsPerBar?, offset?, decay? })` / `beatAt(frame, fps, options)`

Returns `{ framesPerBeat, beat, bar, beatInBar, progress, barProgress, pulse }`.
`pulse` is 1 on each beat and decays exponentially (`decay` frames to 1/e,
default a quarter beat). Inside a `Sequence` the grid starts with the
sequence; use `offset` (frames) to align with music that started elsewhere.

### `useCue(cues)` / `cueAt(cues, frame)`

`cues` is an array of `{ at, …data }` sorted by `at`. Returns `null` before the
first cue, else `{ cue, index, frame, previous, next }` where `frame` counts
from the cue's start. Use for swapping captions, a camera moving between
stops, chart callouts, or "which scene is this" in a HUD.

```tsx
const stop = useCue(STOPS); // [{ at: 30, x: 0 }, { at: 75, x: 1000 }, …]
const from = stop?.previous?.x ?? 0;
const x = stop ? from + (stop.cue.x - from) * progress(stop.frame, 0, 24, Easings.easeInOutCubic) : 0;
```

### `<TextReveal>`

Each line of a string slides up from behind its own mask (`Group clip`),
staggered. Props: `children` (string, `\n` separates lines), `style`,
`lineHeight` (mask height and line spacing; defaults to `style.lineHeight`,
then `fontSize`), `baseline` (0.8: baseline position inside each line box),
`align` (0/0.5/1 pivot of each line), `from`, `stagger` (4), `durationInFrames`
(20), `easing` (`easeOutExpo`), `direction` (`'in'` or `'out'`), plus common
props. `x`/`y` are the top-left of the first line box.

### `useTypewriter(text, { from?, framesPerChar?, blinkFrames? })`

Returns `{ text, length, done, caretVisible }`. Counts by code point, so kana
and emoji count once. `framesPerChar` below 1 types several characters per
frame. The caret is steady while typing and blinks (period `blinkFrames`,
default one second) otherwise. Text is never measured, so place a caret with a
monospaced font: `x = length * fontSize * 0.6` for JetBrains Mono.

### `useCountUp(to, { from?, delay?, durationInFrames?, easing?, decimals? })`

A number that counts to `to` (default 30 frames, `easeOutExpo`), rounded to
`decimals`. Format it yourself, e.g. `value.toLocaleString('en-US')`.

### `<Camera x? y? zoom? rotation? shake? shakeFrequency? seed?>`

Shows the world point (`x`, `y`) at the center of the current area (canvas,
`SafeArea`, or `Fit`), magnified by `zoom` about that point. Defaults look at
the center, so `<Camera zoom={1.05}>` is a slow push-in. `shake` is the
largest drift in world pixels, smoothed by `@celesta/math`'s `noise()`.

### `<Line x1 y1 x2 y2>` and `<Polyline points progress?>`

`<Path>`s with friendlier defaults: `stroke` (default white), `strokeWidth`
(2), `cap` (`'round'` default, `'butt'`, `'square'`), `join` (round with
round caps, else miter), plus `Path`'s transform, `opacity`, and
`blendMode` props. `Polyline` takes `[x, y]` pairs (`closed` to loop) and
draws the first `progress` (0–1) of its length; `pointOnPolyline(points, t)`
gives the tip, for a marker or a label that rides the line.

## @celesta/math

```tsx
import { noise, random, randomRange } from '@celesta/math';
```

Pure functions with no React; they work in components, `prepare()`, and
module scope alike. Everything seeded takes a number or string `seed` and
returns the same value for the same seed on every render. Use these, never
`Math.random()`; give each property its own seed (`` `star-${i}-x` ``).

### Random

| Function | Returns |
| --- | --- |
| `random(seed)` | `[0, 1)` |
| `randomRange(seed, min, max)` | `[min, max)` |
| `randomInt(seed, min, max)` | a whole number, both ends included |
| `randomBool(seed, probability = 0.5)` | `true` with that probability |
| `randomSign(seed)` | `-1` or `1` |
| `randomPick(seed, items)` | one element (throws on an empty array) |
| `shuffle(seed, items)` | a reordered copy |
| `randomGaussian(seed, mean = 0, stdDev = 1)` | a normally distributed number |
| `randomInCircle(seed, radius = 1, center = {x: 0, y: 0})` | an evenly spread point in a disc |

### Noise

All return smooth values in `[-1, 1]`; different seeds give unrelated fields.

- `noise(seed, t)`: 1D value noise; feed it `frame / 20` or so for drift and
  wobble.
- `noise2D(seed, x, y)` / `noise3D(seed, x, y, z)`: gradient (Perlin) noise,
  `0` on whole-number coordinates. Scale positions down (`x / 200`); use the
  third coordinate as time to animate a 2D field.
- `fbm(seed, t, opts?)`, `fbm2D(seed, x, y, opts?)`, `fbm3D(seed, x, y, z,
  opts?)`: layered noise with detail at several scales. `opts` are
  `octaves` (4), `lacunarity` (2), and `gain` (0.5).

### Numbers

`clamp(v, min, max)`, `clamp01(v)`, `lerp(a, b, t)`, `inverseLerp(a, b, v)`,
`remap(v, inMin, inMax, outMin, outMax)` (unclamped), `remapClamped(...)`,
`step(edge, x)`, `smoothstep(e0, e1, x)`, `smootherstep(e0, e1, x)`,
`fract(x)`, `mod(v, n)` (never negative for positive `n`), `wrap(v, min,
max)`, `pingPong(v, length)`, `snap(v, increment)`, `roundTo(v, decimals)`,
`approxEqual(a, b, epsilon = 1e-6)`. For frame-to-value mappings with easing,
prefer `interpolate`.

### Waves

`sineWave(t)`, `triangleWave(t)`, `squareWave(t)`, `sawtoothWave(t)`: period
1, range `[-1, 1]`, all in phase: positive for the first half of each period
and negative for the second (sine, triangle, and sawtooth start at 0; the
square wave starts at 1). Pass
`frame / framesPerCycle`.

### Angles and points

Angles are **radians** (`degToRad`/`radToDeg` convert a layer's `rotation`);
with y pointing down, a positive angle turns clockwise. `TAU`,
`normalizeAngle(a)` (to `[-π, π)`), `angleDifference(from, to)`,
`lerpAngle(a, b, t)` (the short way round).

Points are `{ x, y }` (`Vec2`): `distance(a, b)`, `angleBetween(from, to)`,
`lerpPoint(a, b, t)`, `midpoint(a, b)`, `rotatePoint(p, angle, origin?)`,
`polarToCartesian(angle, radius, center?)`, `cartesianToPolar(p, center?)`,
`quadraticBezierPoint(p0, p1, p2, t)`, and `cubicBezierPoint(p0, p1, p2, p3,
t)` (the curves a `Path`'s `quadTo`/`cubicTo` draw).

## Layout helpers

Helpers position child **origins**; they do not measure what children draw.
Anchor children at `0.5` to center them on those points.

| Component | Props | Behavior |
| --- | --- | --- |
| `Center` | common props | Moves the origin to the center of the canvas or the enclosing `SafeArea`/`Fit`. `x`/`y` offset from there. |
| `SafeArea` | `padding`: number or `{ top, right, bottom, left }` | Insets children; nested helpers see the smaller area. |
| `Stack` | `spacing` (required), `direction`: `'vertical'` (default) or `'horizontal'` | Child *i* is placed at `i * spacing`. |
| `Grid` | `columns`, `columnWidth`, `rowHeight`, `columnGap?`, `rowGap?` | Fills cells row by row; each child's origin is its cell's top-left. |
| `Fit` | `sourceWidth`, `sourceHeight`, `mode`: `'contain'` (default) or `'cover'` | Scales content designed at the source size to the current area and centers it. |

`useLayoutBounds()` returns the current area's `{ width, height }`.

```tsx
<SafeArea padding={96}>
  <Center>
    <Stack spacing={96} y={-96}>
      {['Write', 'Preview', 'Export'].map((s) => (
        <Text key={s} anchorX={0.5} anchorY={0.5} style={{ fontSize: 64 }}>{s}</Text>
      ))}
    </Stack>
  </Center>
</SafeArea>
```

To show a 1280×720 video full-screen in a 1920×1080 composition:
`<Fit sourceWidth={1280} sourceHeight={720}><Video src="./clip.mp4" /></Fit>`.

## Media helpers

Call `preloadMedia()` and asynchronous `measureText()` in `prepare()`.
Use `useTextMetrics()` during render, as described below.

```tsx
let clipFrames = 150; // fallback
export async function prepare() {
  const info = await preloadMedia('./media/walk.mp4');
  clipFrames = mediaDurationInFrames(info, 30) ?? clipFrames;
}
```

`preloadMedia(src)` resolves to
`{ src, durationSeconds?, video?: { width, height, codec?, frameRate?, durationSeconds? }, audio: [{ codec?, sampleRate?, channels?, durationSeconds? }] }`.
`mediaDurationInFrames(info, fps)` rounds up to whole frames. To lay out a
whole voiced script this way, use `planDialogue()` (see
[dialogue.md](dialogue.md#timing-a-script-from-its-voices)).

### Measure text

`measureText(text, style, { maxWidth?, fonts? })` shapes text exactly as
`<Text>` does and resolves to
`{ width, height, ascent, descent, lineHeight, lines, glyphs: [{ text, x, width, line }] }`
in composition pixels (`ascent` runs from the first line's top to its
baseline). `glyphs` has one entry per cluster, so per-letter layout is
`glyphs[i].x`. `<Font>` files are not loaded during `prepare()`; pass their
`src` in `fonts` to measure with them.

```tsx
let caretX = 0;
export async function prepare() {
  const m = await measureText('Celesta', { fontFamily: 'Inter', fontSize: 96 }, { fonts: ['./Inter.ttf'] });
  caretX = m.width;
}
```

### Measure text during render

`useTextMetrics(text, style?, { maxWidth?, fonts? }?)` returns the same
`TextMetrics` synchronously inside a React component. Use it for counters,
typed text and project properties without listing strings in `prepare()`.
It uses the renderer's shaping engine in both preview and export; unchanged
text, style, wrapping width and font declarations reuse the hook's last
result. `<Font>` declarations in the composition are included automatically,
even when they appear after the component that measures text. Relative font
paths resolve against the entry file. Optional `fonts` are extra files to
load for measurement; also declare them with `<Font>` so drawing uses them.

`width` is the advance width, including spaces. `height` is the line-box
height, while single-line `<Text>` normally trims empty rows above and below
its visible glyphs. Place text at `y={padding + metrics.ascent}` with
`anchorY="baseline"` to fit it consistently inside a measured background.
Pass the same `style` and `maxWidth` to the hook and the `<Text>`.

A heading can use independently colored runs without estimated widths:

```tsx
const style = { fontFamily: 'sans-serif', fontSize: 64 };
function Heading() {
  const first = useTextMetrics('速い、', style);
  const accent = useTextMetrics('Easy', style);
  return <Group y={100}>
    <Text anchorY="baseline" style={style}>速い、</Text>
    <Text x={first.width} anchorY="baseline"
      style={{ ...style, fill: { type: 'solid', color: '#28A34A' } }}>Easy</Text>
    <Text x={first.width + accent.width} anchorY="baseline" style={style}>、頼もしい。</Text>
  </Group>;
}
```

Each `<Text>` shapes separately. Keep letters that need shared kerning or a
ligature in the same run; this hook does not add rich-text paragraph shaping.

A fitted pill can measure its computed label in the component that draws it:

```tsx
function Pill({ label }) {
  const m = useTextMetrics(label, style);
  const height = m.height + 24;
  return <Group>
    <Rect width={m.width + 48} height={height} cornerRadius={height / 2} fill="#28A34A" />
    <Text x={24} y={12 + m.ascent} anchorY="baseline"
      style={{ ...style, fill: { type: 'solid', color: '#ffffff' } }}>{label}</Text>
  </Group>;
}
```

For a centered row, add each measured item's padding and a fixed gap:

```tsx
function Row() {
  const { width } = useVideoConfig();
  const frame = useCurrentFrame();
  const first = `SFP+ 10GbE ×${frame + 1}`;
  const second = 'RJ45 2.5GbE ×4';
  const third = 'USB-C';
  const a = useTextMetrics(first, style);
  const b = useTextMetrics(second, style);
  const c = useTextMetrics(third, style);
  const gap = 16;
  const wa = a.width + 48, wb = b.width + 48, wc = c.width + 48;
  return <Group x={(width - wa - wb - wc - 2 * gap) / 2}>
    <Pill label={first} />
    <Group x={wa + gap}><Pill label={second} /></Group>
    <Group x={wa + wb + 2 * gap}><Pill label={third} /></Group>
  </Group>;
}
```

Like other React hooks, call `useTextMetrics` at the component's top level,
with a stable number of calls. The full example is
`packages/react/examples/with-text-metrics.tsx`.

For anything else asynchronous (fetching JSON, reading files with
`node:fs`), use `prepare()` the same way and keep a fallback so the scene
still renders offline.

## Project data

### Read values from a `.celesta.json`

```tsx
import type { Project } from '@celesta/react';
import projectFile from './project.celesta.json';
const project = projectFile as Project;

function Title() {
  const title = useProjectProperty('title', 'Chapter 1'); // key, fallback
  return <Text style={{ fontSize: 96 }}>{title}</Text>;
}

export default function Root() {
  return <Composition width={1920} height={1080} fps={30} durationInFrames={90}>
    <ProjectProvider project={project}><Title /></ProjectProvider>
  </Composition>;
}
```

`useProject()` returns the whole project. `loadProject(path)` and
`loadProjectFromString(json)` also create a `Project`.

### `defineProjectProperties(schema)`

Declares the Inspector fields for `properties` (module level):

```ts
defineProjectProperties({
  title:  { type: 'string',  label: 'Title',  defaultValue: 'Celesta' },
  accent: { type: 'color',   label: 'Accent', defaultValue: '#ff8800' },
  size:   { type: 'number',  label: 'Size',   defaultValue: 48, min: 8, max: 200, step: 2 },
  sub:    { type: 'boolean', label: 'Subtitle', defaultValue: false },
  weight: { type: 'select',  label: 'Weight', defaultValue: 'bold', options: ['bold', 'light'] },
});
```

### Draw a JSON timeline inside React

- `<ProjectTimeline />` draws the companion project's whole timeline;
  `<ProjectTrack id="titles" />` draws one track; `useProjectTrack(id)`
  returns its evaluated layers.
- These only work when the entry is exported with
  `--react entry.tsx --project project.celesta.json`. Without a companion
  project they throw.

### `registerComponent(name, Component, schema?)`

Lets a JSON item `{ "type": "component", "component": "LowerThird", "props": { … } }`
render a React component through `<ProjectTimeline />`. Call it at module
level. Props must be plain JSON values; declare them with a `type` alias,
not an `interface` (interfaces do not satisfy the JSON constraint). The
item's `range`, `transform`, and `opacity` still come from the JSON.

```tsx
type LowerThirdProps = { name: string; role: string };
function LowerThird({ name, role }: LowerThirdProps) {
  return <Text style={{ fontSize: 48 }}>{`${name} · ${role}`}</Text>;
}
registerComponent<LowerThirdProps>('LowerThird', LowerThird, {
  name: { type: 'string', defaultValue: 'Mira' },
  role: { type: 'string', defaultValue: 'Host' },
});
```

## Rendering cost

Preview and export draw every frame on the GPU. A frame's cost depends on
how many layers there are, and far more on what they ask the renderer to
do. Thousands of flat `Rect`s are cheap; a few dozen effects are not. Export
prints its speed as it goes (`rendering frame 300/1530  58.5 fps`). To find
the slow part of a video, see
[verify-and-export.md](verify-and-export.md#find-slow-parts).

| Cheap | Costs more |
| --- | --- |
| `Rect`, flat or gradient, including a gradient whose colors change every frame | `blur`, `glow`, `shadow`: each layer with an effect is drawn onto a canvas of its own and filtered in several extra GPU passes over the area it covers |
| `x`/`y`, `scale`, `rotation`, `opacity` | A `blendMode` other than `'normal'`: every blended layer reads what is beneath it, which takes a copy and a GPU pass of its own |
| Text, images and SVGs whose content and drawn size stay the same: rasterized once and reused | `Path`, `Line`, `Polyline`: rasterized on the CPU every frame at their drawn size, so a large filled path costs per pixel |
| | Text whose string or style (including a gradient `fill`'s colors) changes every frame: rasterized again each frame. Text drawn at a changing scale is rasterized again in steps of about 9% |

To get the same picture for less:

- **Put one effect on a `Group`, not one on each layer.** A `Group`'s effect
  filters all its children together. Letters that fade one by one can
  share one `glow` on their `Group`; it follows each letter's opacity.
- **Draw many small lights as gradients.** For dozens of glowing points
  (stars, windows, sparks), draw a radial-gradient `Rect` that fades to
  transparent instead of using `glow`. Animate it with `opacity` and
  `scale`:

  ```tsx
  <Rect x={x} y={y} width={40} height={40} anchorX={0.5} anchorY={0.5} opacity={brightness}
    fill={{ type: 'radial', center: { x: 20, y: 20 }, radius: 20,
      stops: [{ offset: 0, color: '#FFE8C0' }, { offset: 1, color: '#FFE8C000' }] }} />
  ```

- **Blend a few large layers rather than many small ones.** Layers that use
  the same mode and do not overlap one another can go in one
  `<Group blendMode="add">`. The group then blends once.

## Preview-only debug guides

`<DebugOverlay safeArea? color? showCenter? showFrame? />` draws the safe
area, center cross, and frame counter. `<DebugBounds width height label? color? showOrigin?>`
wraps children and outlines a box. Both render only in the app preview
(never in exports), so they are safe to leave in.

## Keyframe values

`Audio` `volume`/`playbackRate` and `Dialogue` `volume`/`playbackRate` accept
a number or keyframes. Keyframe times are **seconds from the start of the
enclosing sequence**, written as rationals:

```tsx
volume={{
  type: 'keyframes',
  keyframes: [
    { time: { value: 0, timescale: 1 }, value: 0 },
    { time: { value: 2, timescale: 1 }, value: 0.8, easing: 'ease-out' },
  ],
}}
```

Keyframe `easing` uses the kebab-case names listed in
[project-json.md](project-json.md#easing-names). Visual props (`x`,
`opacity`, …) do not take keyframes in React; compute them from the frame
with `interpolate`.
