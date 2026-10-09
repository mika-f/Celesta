# React compositions: entry, layers, and layout

The building blocks of a `@celesta/react` composition: the entry file, the
props every layer takes, each layer type, layout helpers, media loading, and
preview-only debug guides. For motion see [animation.md](animation.md); for
text see [text.md](text.md).

Everything here is imported from `@celesta/react` unless its section names
another package: shapes from `@celesta/shapes`, layout helpers from
`@celesta/layout`, media info from `@celesta/media-utils`, and debug guides
from `@celesta/debug`. The runtime bundled with Celesta provides `react`
(18.x) and every `@celesta/*` package; never install them from npm.

## Contents

- [Entry file shape](#entry-file-shape)
- [prepare(): async work before the first frame](#prepare-async-work-before-the-first-frame)
- [Common layer props](#common-layer-props)
- [Composition](#composition)
- [Rect and paints (gradients)](#rect-and-paints-gradients)
- [Path, Line, Polyline](#path-line-polyline)
- [Circle, Ellipse, Arrow](#circle-ellipse-arrow)
- [Text (summary)](#text-summary)
- [Group and clipping](#group-and-clipping)
- [Image](#image)
- [Video](#video)
- [Audio](#audio)
- [Font and Assets](#font-and-assets)
- [Layout helpers](#layout-helpers)
- [Media info](#media-info)
- [Keyframe values](#keyframe-values)
- [Preview-only debug guides](#preview-only-debug-guides)

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
- Imports: `react` and every `@celesta/*` package come from Celesta's
  runtime. Relative files (including
  `import data from './data.json'`) are bundled. Other npm packages resolve
  from the project's own `node_modules` (see
  [setup.md](setup.md#npm-dependencies)).

## prepare(): async work before the first frame

Rendering is synchronous. Anything asynchronous (fetching JSON, reading files
with `node:fs`, `preloadMedia()`, `measureText()`, `fitText()`,
`loadLipSync()`, `loadPsdPreset()`, `planDialogue()`) goes in
`export async function prepare()`. Store results in module-level `let`
variables and keep a fallback so the scene still renders offline:

```tsx
let stats = { visitors: 0 }; // fallback
export async function prepare() {
  try {
    stats = await (await fetch('https://example.com/stats.json')).json();
  } catch { /* keep the fallback */ }
}
```

Hooks (`useCurrentFrame()` and the rest) do not work in `prepare()`.

## Common layer props

Every visual layer (`Rect`, `Circle`, `Ellipse`, `Text`, `Group`, `Image`, `Video`,
`CharacterView`, `Dialogue`, `Sequence`, layout helpers, `Transition`,
`Code`) accepts:

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
to all children together. They combine with `opacity` and `blendMode`.
Compute any prop from `useCurrentFrame()` to animate it. Visual props do not
take keyframe objects in React. Effects and blend modes cost GPU passes; see
[performance.md](performance.md) before putting them on many layers.

## Composition

`width`, `height`, `fps`, `durationInFrames`: all required positive
integers. Use even `width`/`height` for MP4 export. Exactly one, at the root.
These props are read once when the entry loads, so they must not depend on
the current frame or `useVideoConfig()`. Optional `lang` (for example
`"ja-JP"`) sets the default text language, which picks fallback fonts; see
[text.md](text.md#language-and-fallback-fonts).

## Rect and paints (gradients)

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
// solid (text styles need this form): { type: 'solid', color: '#ffffff' }
```

## Path, Line, Polyline

Imported from `@celesta/shapes`.

`<Path>` is a vector shape (lines and Bézier curves) as **one layer**,
however many segments it has: a translucent stroke that crosses itself is
painted once, and it stays sharp under `scale`. Prefer it to many thin
`Rect`s for procedural line art; group strands that share a width/opacity
into one path each (tens of paths, not thousands of rects).

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

`<Line x1 y1 x2 y2>` and `<Polyline points progress?>` are `<Path>`s with
friendlier defaults: `stroke` (default white), `strokeWidth` (2), `cap`
(`'round'` default, `'butt'`, `'square'`), `join` (round with round caps,
else miter), plus `Path`'s transform, `opacity`, and `blendMode` props.
`Polyline` takes `[x, y]` pairs (`closed` to loop) and draws the first
`progress` (0–1) of its length, which animates a line being drawn;
`pointOnPolyline(points, t)` gives the tip as an `[x, y]` pair (not
`{ x, y }`), for a marker or a label that rides the line.

## Circle, Ellipse, Arrow

Imported from `@celesta/shapes`.

Diagram shapes, each drawn as one layer, so they take every common layer
prop (transforms, `opacity`, `blendMode`, effects) and stay sharp under
`scale`. A `Circle` is a `Rect` rounded to its radius, as cheap as any
`Rect`; an `Ellipse` or an `Arrow` is a `<Path>`.

`<Ellipse width height>` and `<Circle radius>` (a `2 * radius` square box)
are placed like `<Rect>`: `x`/`y` put the box's `anchorX`/`anchorY` point
(top-left by default), which `rotation`/`scale` turn about.

| Prop | Notes |
| --- | --- |
| `width`, `height` / `radius` | Required, pixels, at least 0. A 0 size draws nothing; a negative or non-finite one fails the render. |
| `fill` | Hex or `Paint`, in local pixels from the box's top-left (as for `Rect`). Omit for no fill. |
| `stroke`, `strokeWidth` | Hex or `Paint`; width defaults to 2. The stroke lies inside the box, like `Rect`'s, and is at most half the shorter side (a thicker one is drawn at that). |

`<Arrow x1 y1 x2 y2>` is a straight arrow whose head's tip is exactly at
`x2`/`y2`. Its shaft and heads are one filled outline, so a translucent or
gradient arrow has no seam. Coordinates are its own pixels, like `Line`'s
(`x`/`y` move their origin; no `anchorX`/`anchorY`).

| Prop | Notes |
| --- | --- |
| `stroke` | Hex or `Paint` (in the arrow's coordinates) of the whole arrow. Default white. |
| `strokeWidth` | Shaft thickness, default 2. The shaft ends flat. |
| `headLength`, `headWidth` | Head size along and across the arrow; default 4 × `strokeWidth` each. A head is never narrower than the shaft. |
| `heads` | `'end'` (default), `'start'`, or `'both'`. |

An arrow shorter than its heads (`headLength`, twice that with `'both'`)
scales them down to fit, keeping their shape, so animating `x2`/`y2` from
`x1`/`y1` grows it smoothly. An arrow with no length draws nothing.
Non-finite points or a head size of 0 or less fail the render.

```tsx
<Circle x={960} y={540} anchorX={0.5} anchorY={0.5} radius={80} stroke="#FFD84D" strokeWidth={8} />
<Ellipse x={200} y={300} width={240} height={120} fill="#3366CC" glow={{ color: '#3366CC', blur: 16 }} />
<Arrow x1={300} y1={600} x2={300 + 400 * progress(frame, 0, 30)} y2={600} strokeWidth={6} heads="both" />
```

See `packages/cli/examples/with-shapes.tsx` in the Celesta repository for all three side by side.

## Text (summary)

`<Text style={…} maxWidth?>` draws strings and numbers only; build combined
strings with template literals. Text styles take paint objects
(`fill: { type: 'solid', color: '#ffffff' }`), not hex strings. Everything
else (styles, fonts, wrapping, anchoring, measuring, fitting to a box) is in
[text.md](text.md).

## Group and clipping

No size or appearance of its own. Children are positioned relative to the
group's `x`/`y`, and its rotation, scale, and opacity apply to all of them.

| Prop | Notes |
| --- | --- |
| `lang` | Default text language for the children (inherited, overridable). |
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

## Image

`src` (path or URL). PNG, JPEG, WebP, PNM, and SVG are supported.
Without dimensions, the image uses its natural size. `width` or `height`
alone preserves its aspect ratio. Both dimensions stretch to the display box;
`fit="contain"` centers the whole image with transparent letterboxing, while
`fit="cover"` centers and crops it to fill the box. `scale` applies afterwards.

```tsx
<Image src="./logo.svg" width={432} />
<Image src="./photo.png" width={800} height={450} fit="cover" />
```

SVGs rasterize at their display size and accumulated transform scale in
preview and export. `var(--name, fallback)` uses its fallback (including
nested functions). SVG backgrounds are preserved; remove unwanted backgrounds
in the source. Missing files and invalid SVGs produce asset errors.
Rasterization is limited to 16384 pixels per side and 64 million pixels per
image.

Paths are relative to the entry file's folder. `http(s)://` URLs are
downloaded once into a per-user cache and never revalidated; change the URL
to pick up a new file.

## Video

| Prop | Notes |
| --- | --- |
| `src` | Path or URL. Drawn at natural pixel size. |
| `startFrom` | Seconds into the file at the enclosing sequence's frame 0. Default `0`. |
| `playbackRate` | Static number only. Default `1`. |

**A React `<Video>` is picture only; it makes no sound.** To hear the clip's
soundtrack, add an `<Audio>` with the same `src`, `startFrom`, and
`playbackRate` next to it, inside the same `<Sequence>`.

To show a 1280×720 clip full-screen in a 1920×1080 composition, wrap it in
`<Fit sourceWidth={1280} sourceHeight={720}>` (see
[Layout helpers](#layout-helpers)).

## Audio

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

## Font and Assets

```tsx
<Assets>
  <Font src="./fonts/MPLUSRounded1c-Bold.ttf" />
  <Font src="https://fonts.googleapis.com/css2?family=M+PLUS+Rounded+1c:wght@400;700" />
</Assets>
```

- `<Font src>` loads TTF, OTF, WOFF, WOFF2, or a web-font CSS stylesheet
  (every `@font-face` in it is loaded). Refer to it by the **family name
  stored inside the font**, not the file name. See
  [text.md](text.md#fonts-fallback-and-emoji) for weights and fallback.
- `<Assets>` holds declarations that draw nothing: `Font`, `Character`, and
  `Image`/`Video`/`Audio` used only to create refs. Put it outside any
  `<Sequence>` when the font or asset is needed throughout.
- Refs: `const logo = React.createRef<AssetReference>()`, then
  `<Assets><Image ref={logo} src="./logo.png" /></Assets>` and
  `<Image src={logo} />` elsewhere. Plain string paths are usually simpler.

## Layout helpers

Imported from `@celesta/layout`.

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

For layouts that depend on text size (pills, rows of labels), measure the
text with `useTextMetrics()`; see [text.md](text.md#measure-text).

## Media info

Imported from `@celesta/media-utils`.

Call `preloadMedia()` in `prepare()` to learn a file's length and size:

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
with `interpolate` ([animation.md](animation.md#interpolate)).

### `frameKeyframes(keys, { fps, origin? })`

Builds the same keyframes from frame numbers. Each key is
`{ frame, value, easing? }`; `easing` shapes the segment that ends at that
key and defaults to `'linear'`. Fade in over 15 frames and out over the
last 15 of a 90-frame sequence:

```tsx
const { fps } = useVideoConfig();
<Audio src="./bgm.wav" volume={frameKeyframes([
  { frame: 0, value: 0 },
  { frame: 15, value: 0.8, easing: 'ease-out' },
  { frame: 75, value: 0.8 },
  { frame: 90, value: 0, easing: 'ease-in' },
], { fps })} />
```

- **Keys count from the start of the `<Audio>`'s innermost `<Sequence>`.**
  To write keys in composition frames (or another outer clock), pass
  `origin`: the frame that sequence starts on, on the keys' clock. With
  `<Sequence from={30}><Sequence from={15}><Audio …/></Sequence></Sequence>`,
  a key at composition frame 45 needs `origin: 45`. Keys before `origin` are
  allowed.
- Times are `(frame - origin) / fps` seconds rounded to the nearest
  microsecond, as `<Sequence>` offsets are. Fractional frames and fps
  (29.97) work.
- Keys must be in order of `frame`. Two keys on the same frame make a cut:
  the first value holds up to that frame, the second applies after it. A
  third key on the same frame throws.
  Before the first key and after the last, the nearest key's value holds.
- An empty list, keys out of order, a non-finite `frame`, `value`, or
  `origin`, and an `fps` that is not positive throw.

See `packages/cli/examples/with-volume-fade.tsx`.

## Preview-only debug guides

Imported from `@celesta/debug`.

`<DebugOverlay safeArea? color? showCenter? showFrame? />` draws the safe
area, center cross, and frame counter. `<DebugBounds width height label?
color? showOrigin?>` wraps children and outlines a box. Both render only in
the app preview (never in exports or PNG frames), so they are safe to leave
in. `useIsPreview()` returns `true` only in the app preview, for your own
preview-only aids.
