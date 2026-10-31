# React compositions: entry, layers, and layout

The building blocks of a `@celesta/react` composition: the entry file, the
props every layer takes, each layer type, layout helpers, media loading, and
preview-only debug guides. For motion see [animation.md](animation.md); for
text see [text.md](text.md).

Everything here is imported from `@celesta/react` unless its section names
another package: shapes from `@celesta/shapes`, layout helpers from
`@celesta/layout`, media info from `@celesta/media-utils`, and debug guides
from `@celesta/debug`. Components only mentioned here come from their own
packages too: `CharacterView` and `Dialogue` from `@celesta/character`,
`Transition` from `@celesta/transitions`, and `Code` from `@celesta/code`
(see [SKILL.md](../SKILL.md#packages)). The runtime bundled with Celesta
provides `react` (18.x) and those packages; never install them from npm.

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
- [Lottie animations](#lottie-animations)
- [Instances (particles)](#instances-particles)
- [PatternBackground (dots, grid, stripes)](#patternbackground-dots-grid-stripes)
- [Panel (cards and boxes with padding)](#panel-cards-and-boxes-with-padding)
- [Badge (labels sized to their text)](#badge-labels-sized-to-their-text)
- [Counter, StatCard, BarChart, Ranking (numbers and bar charts)](#counter-statcard-barchart-ranking-numbers-and-bar-charts)
- [Callout (a note with a line to its target)](#callout-a-note-with-a-line-to-its-target)
- [Static (still subtrees)](#static-still-subtrees)
- [Video](#video)
- [MediaFrame (images and videos in a frame)](#mediaframe-images-and-videos-in-a-frame)
- [Audio](#audio)
- [Font and Assets](#font-and-assets)
- [Layout helpers](#layout-helpers)
- [Vertical video (Shorts, TikTok, Reels)](#vertical-video-shorts-tiktok-reels)
- [Media info](#media-info)
- [Keyframe values](#keyframe-values)
- [Debug guides](#debug-guides)

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
- Imports: `react` and the Celesta packages in [SKILL.md](../SKILL.md#packages) come from
  Celesta's runtime. Relative files (including
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
| `opacity` | `1` | `0`–`1`; multiplies down through groups. A `Group` below 1 fades as one picture. |
| `blendMode` | `'normal'` | How the layer combines with what is beneath it, as in CSS `mix-blend-mode`: `'normal'`, `'multiply'`, `'screen'`, `'overlay'`, `'add'`, `'difference'`, `'darken'`, `'lighten'`, `'colorDodge'`, `'colorBurn'`, `'hardLight'`, `'softLight'`, `'exclusion'`, `'hue'`, `'saturation'`, `'color'`, or `'luminosity'` (camelCase, not CSS's `'color-dodge'`). |
| `blur` | `0` | Gaussian blur radius in output pixels, from 0 to 64. |
| `shadow` | none | `{ color, blur, offsetX, offsetY }`; hex color, 0–64 px blur, output-pixel offsets. |
| `glow` | none | `{ color, blur }`; centered halo with a 0–64 px blur. |
| `brightness` | `1` | Multiplies the color: `0` black, above `1` brighter. |
| `contrast` | `1` | Scales the distance from mid-gray: `0` mid-gray, above `1` more contrast. |
| `saturation` | `1` | `0` grayscale, above `1` more saturated. |
| `hue` | `0` | Rotates the hue, in degrees (CSS `hue-rotate()`). |
| `grayscale` | `0` | `0`–`1`; `1` is fully gray. |
| `invert` | `0` | `0`–`1`; `1` inverts the color. |
| `tint` | none | `{ color, amount? }`; lays a hex color over the pixels at `amount` (0–1, default 1) times the color's alpha, keeping their alpha. |
| `vignette` | none | `0`–`1`, or `{ amount, radius?, softness? }`; darkens toward the edges of the layer's box. Darkening starts at `radius` (0 centre, 1 corners; default 0.5) and reaches `amount` `softness` further (default 0.5). |
| `chromaKey` | none | `'#RRGGBB'`, or `{ color, tolerance?, softness?, spill? }`; makes colors near the key transparent (green-screen `<Video>`/`<Image>`). `tolerance` (default 0.2) is the BT.709 CbCr distance keyed fully, `softness` (0.1) the fade beyond it, `spill` (0–1, 0.5) how much key-colored cast to remove. Keep `tolerance + softness` below the key's chroma (0.36 for `#00b140`), or grays turn translucent. |
| `pixelate` | none | Cell size in px, or `{ size }`; `1` or less does nothing. |
| `wave` | none | `{ amplitude, wavelength, phase?, angle? }`; px, px, degrees, degrees. Animate `phase` to make it travel. |
| `zoomBlur` | none | `0`–`1`, or `{ amount, centerX?, centerY? }` (centre in the box, default 0.5). |
| `chromaticAberration` | none | Offset in px, or `{ offset, angle? }`; red moves toward `angle`, blue away. |
| `halftone` | none | Grid size in px, or `{ size, angle? }` (default 45°); dots of each cell's color, area by brightness, transparent between. |
| `noise` | none | `0`–`1`, or `{ amount, seed?, monochrome? }`. Without `seed` the grain follows `useCurrentFrame()`; a `<Static>` needs a `seed`. |
| `scanlines` | none | `0`–`1`, or `{ amount, spacing?, thickness?, offset? }` (4 px, 0.5, 0 px). |
| `id` | generated | Optional stable layer id. |

To center something at a point, set `anchorX={0.5} anchorY={0.5}` and put
the point in `x`/`y`.

Effects apply to the composited pixels of the layer; on a `Group` they apply
to all children together. They combine with `opacity` and `blendMode`.
They run in this order: `shader` (see [shader.md](shader.md)), then
`chromaKey`, `pixelate`, `wave`, `zoomBlur`, `chromaticAberration`, the
color filters in the order of the table (`brightness` first, `vignette`
last), `halftone`, `noise`, `scanlines`, then `shadow` and `glow` from the
result, with `blur` on top. Each color filter works on the straight sRGB
color, keeps alpha, and clamps its result to 0–1; values that change
nothing (`brightness={1}`, `grayscale={0}`, `pixelate={1}`, `noise={0}`)
add no work. Spatial values are output pixels, and grids and centres
follow the layer's box. For another order, pass the filters to `shader` as
an array: `shader={[grayscale(), tint({ color: '#704214', amount: 0.4 })]}`.
All of these built-in filters also run in the browser editor.
Compute any prop from `useCurrentFrame()` to animate it. Visual props do not
take keyframe objects in React. Effects and blend modes cost GPU passes; see
[performance.md](performance.md) before putting them on many layers.

## Composition

`width`, `height`, `fps`, `durationInFrames`: all required positive
integers. Use even `width`/`height` for video export. Exactly one, at the root.
These props are read once when the entry loads, so they must not depend on
the current frame or `useVideoConfig()`. Optional `lang` (for example
`"ja-JP"`) sets the default text language, which picks fallback fonts; see
[text.md](text.md#language-and-fallback-fonts). Optional `fontFallback="loaded"`
tries fonts loaded with `<Font>` before system fonts; see
[text.md](text.md#fonts-fallback-and-emoji).

## Rect and paints (gradients)

| Prop | Notes |
| --- | --- |
| `width`, `height` | Required, pixels. |
| `fill` | Hex string (`"#RRGGBB"` / `"#RRGGBBAA"`) or a gradient `Paint` (see below). Omit for no fill. |
| `stroke`, `strokeWidth` | Inner stroke (hex or `Paint`); needs both to be visible. |
| `cornerRadius` | Pixels. A square with `cornerRadius = size / 2` is a circle. |
| `dash`, `dashOffset`, `cap` | Dash the stroke, as for `<Path>`, clockwise from the end of the top-left corner. A dashed rect is drawn as its fill under a stroked `Path`, in a `Group`. |

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
| `dash`, `dashOffset` | Dash and gap lengths in turn, like SVG's `stroke-dasharray` (an odd count repeats), starting over at each `moveTo`; `dashOffset` shifts the pattern, so animating it makes the dashes flow. `dash={[0, gap]}` with `cap="round"` draws dots. |
| `progress`, `progressStart` | The part of the stroke drawn, 0–1 of the path's length (defaults 1 and 0); curves are split exactly, and dashes stay where they are on the whole path. A `fill` stays whole: with a trimmed stroke the path becomes a `<Group>` of a fill layer and a stroke layer. |

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
`progress` (0–1) of its length, which animates a line being drawn
(`progressStart` trims the other end);
`pointOnPolyline(points, t)` gives the tip as an `[x, y]` pair (not
`{ x, y }`), for a marker or a label that rides the line. Both take
`dash`/`dashOffset`; the dashes are anchored at the first point, so they
stay put while `progress` draws the line on.

Helpers for `commands` return points as `{ x, y }`, like the commands
themselves. Lengths count every drawn segment, curves and `close`
included, but not the jumps of `moveTo`:

- `pointOnPath(commands, t)` / `tangentOnPath(commands, t)`: the point and
  direction `t` (0–1) of the way along; `pointOnPath(commands, progress)` is
  the tip of a `<Path progress>`. A tangent is a unit `{ x, y }` plus
  `angle` in degrees clockwise, ready for `rotation`. At a corner it is the
  direction of the segment that starts there.
- `getLength(commands)`, `getPointAtLength(commands, s)`,
  `getTangentAtLength(commands, s)`: the same by length in pixels (clamped).
  `measurePath(commands)` measures once and returns `{ length,
  getPointAtLength, getTangentAtLength }`. Results are cached per commands
  array, so define it once, not inline.
- `interpolatePath(a, b, t)`: the shape between two paths. Any commands work:
  segments become cubics, each subpath is split where the other has a
  corner (by fraction of its length), closed shapes start at the
  best-aligned corner, and an unmatched subpath grows from its middle.
  Paths with the same command types just blend point by point. Keep `a`
  and `b` the same arrays across frames (the matching is cached).
- `parseSvgPath(d)` turns SVG path data (all of `M L H V C S Q T A Z`,
  relative too; arcs become cubics) into commands; `toSvgPath(commands)`
  writes absolute `M L Q C Z`.

```tsx
const ROUTE = parseSvgPath('M0 0 C 100 -80 200 80 300 0');
const t = progress(frame, 0, 90);
const tip = pointOnPath(ROUTE, t);
<Path commands={ROUTE} stroke="#4DD6FF" strokeWidth={4} progress={t} />
<Path x={tip.x} y={tip.y} rotation={tangentOnPath(ROUTE, t).angle} fill="#FFFFFF"
  points={[[12, 0], [-8, -7], [-8, 7]]} closed />
```

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
| `dash`, `dashOffset`, `cap` | As for `<Path>`; the dashes run clockwise from the rightmost point. A dashed `Circle` is drawn as an `Ellipse`, and a dashed shape with a `fill` as a fill and a stroke in a `Group`, so the fill reaches the edge between the dashes. `<Rect>` takes them too, its dashes starting at the end of the top-left corner. |

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
| `dash`, `dashOffset` | Cut the shaft into flat-ended dashes, measured from `x1`/`y1` as for a `<Line>`; the heads stay solid, and the arrow is still one filled outline. |

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

## Triangle, Polygon, Star, Pie, Heart

Imported from `@celesta/shapes`.

Closed shapes, each one `<Path>` layer, so they take every common layer
prop. `x`/`y` place the box's `anchorX`/`anchorY` point (top-left by
default), which `rotation`/`scale` turn about, as for `<Rect>`. Polygons,
stars, and pies are sized by `radius` in a `2 * radius` square box, like
`<Circle>`; triangles and hearts fill a `width` × `height` box.

| Component | Props |
| --- | --- |
| `<Triangle width height>` | `apex` (0–1, default 0.5): where the top corner is along the top edge; 0 is a right angle on the left. The base is the bottom edge. |
| `<Polygon radius sides>` | `sides`: whole number ≥ 3. Corners on the circle, the first at 12 o'clock. |
| `<Star radius points>` | `points`: whole number ≥ 3 tips on the circle, the first at 12 o'clock; `innerRatio` (0–1, default 0.5): inner corners at that fraction of `radius`. |
| `<Pie radius endAngle>` | `startAngle` (default 0), `endAngle`: degrees clockwise from 12 o'clock. Below `startAngle` it runs counter-clockwise; equal angles draw nothing; 360° or more apart is a whole circle. |
| `<Heart width height>` | Fills the box; the tip is at the bottom middle. |

All but `Heart` take `cornerRadius` (pixels, default 0): each corner
becomes an arc of that radius tangent to both sides, shrunk where it would
reach more than halfway along a side (pie corners shrink with the slice, so
animating `endAngle` stays smooth). `fill` and `stroke` are hex or `Paint`
in box-local pixels. **The stroke is centred on the outline** (as for
`<Path>`, unlike `<Ellipse>`/`<Circle>`/`<Rect>`, whose stroke lies inside
the box), so half of it lies outside the box; `join` and `miterLimit` work
as for `<Path>` (a sharp star tip bevels past `miterLimit`), and `cap`
shapes only the ends of dashes, since the outline is closed. A 0 size draws
nothing; negative or non-finite sizes, a `sides`/`points` that is not a
whole number ≥ 3, or `apex`/`innerRatio` outside 0–1 fail the render.

`trianglePath`, `polygonPath`, `starPath`, `piePath`, and `heartPath` take
the same options as an object and return the `PathCommand[]` the component
draws, measured from the box's top-left. Pass them to `<Path commands>` (at
the same `x`/`y`, that matches the component with its default anchor), or
map over them first to reshape the outline.

```tsx
<Pie x={960} y={540} anchorX={0.5} anchorY={0.5} radius={200}
  endAngle={interpolate(frame, [0, 45], [0, 360], { extrapolateRight: 'clamp' })} fill="#4DD8C0" />
<Star x={300} y={300} radius={80} points={5} innerRatio={0.45} cornerRadius={6} fill="#FFD84D" />
<Path x={600} y={300} commands={polygonPath({ radius: 80, sides: 6, cornerRadius: 12 })} stroke="#FFFFFF" />
```

See `packages/cli/examples/with-more-shapes.tsx` in the Celesta repository for every shape and generator.

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
| `fontFallback` | `'loaded'` or `'system'`: where the children's text finds characters its `fontFamily` lacks (inherited, overridable). |
| `clip` | `{ x, y, width, height, cornerRadius }`: draw the children only inside this rectangle. `x`/`y` (default `0`) are its top-left corner and `cornerRadius` (default `0`) rounds it, both in the group's own coordinates, so the clip moves, scales, and rotates with the group. |
| `mask` | A drawing (or `<Mask mode invert>`) that decides where the children show; see "Masks" below. |

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

An `opacity` below 1 isolates the group too, as CSS `opacity` does: a scene
fading in over another fades as one picture, and its card does not let its
own backdrop show through. Children's blend modes then mix only with each
other. A group whose only child is one shape (not a `Group` or
`<Instances>`) blended `'normal'` just passes its opacity to that child.

### Masks

`<Group mask={…}>` shows the children only where the mask is drawn: text,
a path, an image, or any subtree, positioned in the group's own coordinates.
The mask itself is never shown. `<Mask>` sets how it is read, and works as
the prop's value or as a direct child of the `<Group>`:

```tsx
<Group mask={<Text style={{ fontSize: 360, fontWeight: 900 }}>CELESTA</Text>}>
  <Video src="./clip.mp4" />
</Group>

<Group>
  <Mask mode="luminance" invert>
    <Circle x={960} y={540} anchorX={0.5} anchorY={0.5} radius={r} fill="#ffffff" blur={24} />
  </Mask>
  <SceneB />
</Group>
```

- `mode="alpha"` (default) uses the mask's alpha. `mode="luminance"` uses
  `(0.2126 R + 0.7152 G + 0.0722 B) × A` of its sRGB values. `invert` flips
  the mask.
- Where the mask draws nothing, nothing shows. `mask={null}` or
  `mask={false}` turns the mask off, and an empty `<Mask />` hides the group.
  Toggling the mask keeps the children's layer ids and state.
- A masked group is isolated, like one with a blend mode: children's blend
  modes mix only with each other. Its blur, shadow, and glow apply after the
  mask, so they follow the cut-out shape. Its `opacity` and `blendMode`
  apply once to the result.
- The group's `clip` limits the children, not the mask.
- `<Mask>` only works directly in a `<Group>` (not in `<Sequence>` or
  `<FreezeFrame>`), and a group takes one mask.
- Audio inside a mask plays normally.
- Masks draw the same in preview, export, and `@celesta/web`.

## Image

`src` (path or URL). PNG, JPEG, WebP, GIF, PNM, and SVG are supported.
Without dimensions, the image uses its natural size. `width` or `height`
alone preserves its aspect ratio. Both dimensions stretch to the display box;
`fit="contain"` centers the whole image with transparent letterboxing, while
`fit="cover"` centers and crops it to fill the box. `scale` applies afterwards.

```tsx
<Image src="./logo.svg" width={432} />
<Image src="./photo.png" width={800} height={450} fit="cover" />
```

An animated GIF, APNG, or WebP plays like a `<Video>`: synced to the
enclosing sequence's clock from its frame 0, each frame shown for its own
delay (delays of 10 ms or less show for 100 ms, as in browsers), and the same
frame for the same time however the timeline is scrubbed. `startFrom`
(seconds into the animation, default 0) and `playbackRate` (default 1) work
as on `<Video>`; it loops unless `loop={false}`, which holds the last frame.
Inside `<Static>` it draws its first frame. `preloadMedia()` reports its
length (see [Media info](#media-info)).

```tsx
<Image src="./spinner.gif" width={64} />
<Image src="./intro.webp" startFrom={0.5} playbackRate={2} loop={false} />
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

## Lottie animations

`<Lottie>` from `@celesta/lottie` plays a Lottie (Bodymovin) JSON export in
step with the composition. Import the file and pass its contents as `data`:

```tsx
import { Lottie } from '@celesta/lottie';
import confetti from './confetti.json';

<Lottie data={confetti} x={960} y={540} anchorX={0.5} anchorY={0.5} width={800} />
<Lottie data={confetti} startFrame={0} endFrame={30} playbackRate={2} loop={false} />
```

- The box is the file's `w` x `h`, clipped like lottie-web clips it.
  `width` or `height` alone keeps the aspect ratio; `x`/`y`/`anchorX`/
  `anchorY` and the other common props place the box.
- Playback starts at the enclosing sequence's frame 0. `playbackRate`
  (default 1, negative plays backwards) and `loop` (default true) work as
  on `<Image>`; without `loop` it holds the frame before `endFrame`.
  `startFrame`/`endFrame` are in the file's own frames (defaults: its
  `ip`/`op`). Between the file's frames the values interpolate, so a 30 fps
  file stays smooth at 60 fps, and every frame depends only on its time:
  scrubbing draws the same picture as playing.
- Drawn: shape, precomposition (with time remap and stretch), solid, image,
  and null layers; parenting; masks (add, subtract, intersect, inverted,
  opacity); alpha and luma track mattes (inverted too); groups, paths,
  rectangles, ellipses, stars, polygons, fills, strokes (dashes, caps,
  joins), linear and radial gradients, trim paths; layer blend modes.
- Not drawn, with a warning on stderr (or `onWarning`) once per file: text
  layers, expressions, effects and layer styles, 3D layers, auto-orient,
  merge paths, repeaters, round corners, the even-odd fill rule, radial
  gradient highlights, mask expansion, and embedded (base64) images.
  `inspectLottie(data)` returns the same messages without drawing.
- Image layers load `assetsPath` + the asset's file name (default: the
  asset's own `u` folder), relative to the entry like an `<Image>` `src`.
  Export images as separate files, not embedded.

## Instances (particles)

`<Instances>` draws many particles of one shape as one layer: rects,
circles, or one image (`src`) for every particle. Use it for sparks, snow,
star fields, and anything else with hundreds to hundreds of thousands of
elements; 100,000 animated particles cost about as much per frame as a few
hundred `Rect` layers. Particles draw in array order, later ones on top.

```tsx
import { Composition, Instances, useCurrentFrame } from '@celesta/react';

const N = 20_000;
const xs = new Float32Array(N);
const ys = new Float32Array(N);
const colors = new Uint8Array(N * 4);

function Snow() {
  const t = useCurrentFrame() / 30;
  for (let i = 0; i < N; i++) {
    xs[i] = (i * 97.3 + Math.sin(t + i) * 20) % 1920;
    ys[i] = (i * 13.7 + t * (40 + (i % 60))) % 1080;
    colors.set([255, 255, 255, 120 + (i % 100)], i * 4);
  }
  return <Instances count={N} x={xs} y={ys} color={colors} shape="circle" width={3} height={3} />;
}
```

| Prop | Value |
| --- | --- |
| `count` | Number of particles; every column holds this many values. |
| `x`, `y` | Particle **centres**, one per particle (`Float32Array` or array of numbers). Required. |
| `width`, `height` | A number for every particle, or one per particle. Required for rects and circles. An image given neither draws at its natural size, and given one keeps its aspect ratio, as `<Image>` does. |
| `rotation` | Degrees per particle, about its centre. |
| `color` | `Uint8Array` (or array) of straight RGBA 0-255, four per particle. Fills a rect or circle; multiplies an image's texels (white leaves the image as it is). |
| `fill` | One hex colour for every particle when `color` is absent. Defaults to white. |
| `shape` | `'rect'` (default) or `'circle'`; ignored with `src`. `cornerRadius` rounds rects. |
| `src` | An image every particle draws, as for `<Image src>`. |

- **`x`, `y`, and `rotation` always belong to the particles.** Move or turn
  the whole set with a `<Group x y rotation>` around it. `scale`,
  `opacity`, `blendMode`, `blur`, `glow`, `shadow`, and `shader` apply to
  the layer as a whole.
- **A `blendMode` blends the particles as one layer**, as on a `<Group>`:
  they composite normally with each other, then the result blends with the
  backdrop. Overlapping particles do not add up through `add` or `screen`.
- **Fill the columns in place.** Allocate the typed arrays once at module
  scope and overwrite them each frame; every frame sends their current
  contents. `Float32Array`/`Uint8Array` columns are fastest; plain arrays
  are converted every frame.
- **A wrong length fails the frame** with an error naming the prop
  (`<Instances> x has 3 values; count is 4`). A particle with a non-finite
  value is not drawn.
- The CPU reference renderer cannot rotate particles; export uses the GPU.

## PatternBackground (dots, grid, stripes)

Imported from `@celesta/components`. A repeating background filling an
area (the canvas by default; `x`, `y`, `width`, `height`, `cornerRadius`
pick another) and clipped to it, without building arrays of `<Rect>`s.
Every mark is a particle of one `<Instances>` layer, so the dots of a
1920×1080 canvas every 80 px take 4 layers: the clip group, the
`background`, the marks group, and the instances.

| `pattern` | Draws | `size` (default) |
| --- | --- | --- |
| `'dots'` | a dot in the middle of each `spacing` cell; `dotShape="square"` for squares | diameter (4) |
| `'grid'` | lines along the cell edges, across the whole area | line width (1) |
| `'stripes'` | bands `spacing` apart at `angle` degrees clockwise from horizontal (45) | band thickness (half of `spacing`) |
| `{ marks }` | your own marks in every cell (below) | — |

`spacing` (40) is a number or `[across, down]`. `color` (white) and
`background` (none) take hex or `<Theme>` color names. `opacity` and the
color's alpha fade the marks as one layer, so crossing grid lines do not
darken; `blendMode` blends them. `offsetX`/`offsetY` shift the pattern
(negative is fine) and `speedX`/`speedY` move it in pixels per second from
the caller's frame 0; the area stays covered to its edges. `variation={{
opacity, scale, seed }}` lowers each mark's alpha and size by up to that
fraction, the same on every frame for the same `seed`, and a mark keeps its
value as the pattern moves. More than `maxMarks` (100,000) marks throw.

```tsx
import { PatternBackground } from '@celesta/components';

<PatternBackground pattern="dots" spacing={80} size={3}
  background="#101820" color="#FFFFFF" opacity={0.12} />
<PatternBackground pattern="stripes" spacing={28} size={10} angle={-30} speedX={28} color="accent" />
```

A pattern of your own is `{ marks }`: a list of marks, or a function of the
cell (`{ column, row, x, y }`) returning them. Each mark is a `'rect'`
(default) or `'circle'` of `width` × `height` (defaults to `width`), centred
at `x`, `y` from the cell's top-left (default: the cell's centre), with
optional `rotation`, `color`, and `opacity`. Marks may spill up to one cell
into their neighbours. Cells keep their `column` and `row` as the pattern
moves.

```tsx
const crosses: PatternTile = {
  marks: ({ column, row }) => (column + row) % 2 === 0
    ? [{ width: 18, height: 3, rotation: 45 }, { width: 18, height: 3, rotation: -45 }]
    : [{ shape: 'circle', width: 6, color: 'accent' }],
};
<PatternBackground pattern={crosses} spacing={36} speedY={18} color="ink" />
```

Runnable: [`packages/cli/examples/with-pattern-background.tsx`](../../../packages/cli/examples/with-pattern-background.tsx).

## Panel (cards and boxes with padding)

Imported from `@celesta/components`. A box of a given size with a
background, a border, rounded corners, and a shadow, whose children lay out
inside its padding. Use it for caption cards, callouts, and code or stat
panels instead of a `<Group>` with a `<Rect>`, a hand-set `x={PAD}`, and a
`clip` repeating the same size.

| Prop | Default | Notes |
| --- | --- | --- |
| `width`, `height` | required | Outer size, padding included. Not measured from the content. |
| `padding` | `0` | A number, or `{ top, right, bottom, left }` (missing edges are 0). |
| `fill` | none | Hex, `<Theme>` color name, or gradient `Paint` (in the panel's local pixels). |
| `stroke`, `strokeWidth` | none, `2` | The border, drawn inside the edge like `Rect`'s. |
| `cornerRadius` | `0` | Rounds the background, the border, and the clip alike. |
| `shadow` | none | `{ color, blur, offsetX, offsetY }`, cast by the background only; needs a `fill`. `color` may be a theme name. |
| `clip` | `false` | Draws the content only inside the panel's rounded rectangle. |
| `anchorX`, `anchorY` | `0` | The point of the panel at `x`/`y`, and the point `rotation` and `scale` turn about. |

Other common props (`x`, `y`, `rotation`, `scale`, `opacity`, effects) apply
to the whole panel.

- The children's origin is the top-left of the area inside the padding.
  `<Center>`, `<Fit>`, `<SafeArea>`, and `useLayoutBounds()` inside see that
  area, so `<Center>` centres in it and text can wrap to its width.
- Bottom to top it draws the shadow, the `fill`, the children, then the
  `stroke`, so content reaching the edge never covers the border. `clip`
  cuts the children to the whole rounded box (padding included, as CSS
  `overflow: hidden`) and never cuts the shadow.
- `width`, `height`, `padding`, `cornerRadius`, and `strokeWidth` must be
  finite and not negative, or it throws. Padding wider than the panel leaves
  a content area of 0 starting at the left and top padding. A `shadow`
  without a `fill` throws.

```tsx
import { Panel } from '@celesta/components';
import { Center } from '@celesta/layout';

<Panel x={120} y={600} width={720} height={320} padding={32}
  fill="#182030" stroke="#2C3A50" cornerRadius={20} clip
  shadow={{ color: '#00000099', blur: 32, offsetX: 0, offsetY: 16 }}>
  <Text maxWidth={656} style={textStyle}>What this step does.</Text>
</Panel>

// Centred on a point and scaled about it.
<Panel x={960} y={540} anchorX={0.5} anchorY={0.5} scale={pop}
  width={220} height={64} fill="#F4F1EA" cornerRadius={32}>
  <Center><Text anchorX={0.5} anchorY={0.5}>NEW</Text></Center>
</Panel>
```

Runnable: [`packages/cli/examples/with-panel.tsx`](../../../packages/cli/examples/with-panel.tsx).

## Badge (labels sized to their text)

Imported from `@celesta/components`. A label on a [`Panel`](#panel-cards-and-boxes-with-padding)
whose size comes from the measured text: tags, chapter labels, chips, and
pills. It passes the same text and style to `useTextMetrics()` and `<Text>`,
so the background fits Japanese, Latin, and proportional fonts without
estimating widths from the number of characters, and follows the text when
it or the font changes. There is no separate `Chip`: a chip is a badge with
`cornerRadius="pill"`.

| Prop | Default | Notes |
| --- | --- | --- |
| `children` | required | The label: text, which may hold `<Span>`. |
| `style`, `fontFamily`, `fontSize`, `fontWeight`, `color` | none | The label's `TextStyle` and the same shorthand as `<Text>`; theme names work. |
| `padding` | `{ top: 8, right: 16, bottom: 8, left: 16 }` | As `Panel` takes it. |
| `cornerRadius` | `8` | A radius, or `'pill'` for half the badge's height. |
| `leading`, `leadingWidth` | none | A decoration before the text (a dot, an icon) and the width it lays out in; `leading` without `leadingWidth` throws. |
| `gap` | `8` | Space between `leading` and the text. |
| `maxWidth` | none | The widest the badge may be, padding included. |
| `fill`, `stroke`, `strokeWidth`, `shadow`, `clip`, `anchorX`, `anchorY` | as `Panel` | Passed to the `Panel`. |

- Size: the text's advance width plus `leadingWidth + gap` and the padding
  across; its line boxes plus the padding down (`useTextMetrics()`'s
  `height`, so one line and several alike). The text sits on its baseline at
  `padding.top + ascent`. Empty text keeps one line box, so the badge does
  not collapse while text is typed in.
- `leading` lays out in a box `leadingWidth` wide and one line tall at the
  start of the content, so `<Center>` in it centres on the first line.
- `maxWidth`: longer text wraps onto more lines and the badge grows taller.
  A single word wider than the room is cut off at the edge, as with
  `<Text maxWidth>`; pass `clip` to cut it at the rounded box. Truncating
  with an ellipsis is not supported.
- `anchorX`/`anchorY`, `x`/`y`, `rotation`, `scale`, and `opacity` apply to
  the whole badge, so the background and the text move together.
- `padding`, `cornerRadius`, `gap`, `leadingWidth`, and `maxWidth` must be
  finite and not negative, or it throws.

```tsx
import { Badge } from '@celesta/components';
import { Center } from '@celesta/layout';

<Badge x={120} y={80} fill="#204060" fontFamily="ja" fontSize={28} color="#FFFFFF">第1章</Badge>

// A chip with a dot, centred on a point and scaled about it.
<Badge x={960} y={840} anchorX={0.5} anchorY={0.5} scale={pop}
  fill="#10121A" stroke="#2C3A50" strokeWidth={1} cornerRadius="pill"
  padding={{ top: 14, right: 24, bottom: 14, left: 20 }}
  fontFamily="mono" fontSize={22} color="#5BC0FF"
  leading={<Center><Rect anchorX={0.5} anchorY={0.5} width={8} height={8} cornerRadius={4} fill="#3D5BFF" /></Center>}
  leadingWidth={8} gap={12}>
  {'<Composition fps={30}>'}
</Badge>
```

Runnable: [`packages/cli/examples/with-badge.tsx`](../../../packages/cli/examples/with-badge.tsx).

## Counter, StatCard, BarChart, Ranking (numbers and bar charts)

Imported from `@celesta/components`. Components that count numbers and
print them one way, with `formatNumber(value, format)`, also exported:

| Format prop | Default | Notes |
| --- | --- | --- |
| `locale` | `'en-US'` | Digit grouping and the decimal mark (`1,234.5`; `'de-DE'` gives `1.234,5`). Fixed, so the machine's locale never changes the output. |
| `decimals` | `0` | Decimal places, always shown (`12.0`); rounded half up. 0 to 20. |
| `prefix`, `suffix` | none | Text around the number: `suffix=" 件"`. |
| `formatter` | none | `(value) => string`, given the value rounded to `decimals`, instead of the locale's formatting; `prefix` and `suffix` still go around it. |

A value that rounds to 0 prints without a minus sign. Values must be finite,
or it throws.

The count timing is `delay` (default 0), `durationInFrames` (default 30; 0
jumps to the value at `delay`), and `easing` (default `easeOutExpo`), the
same defaults as `useCountUp`.

- **`<Counter value from?>`**: a `<Text>` (every other prop is `Text`'s)
  printing a number that counts from `from` (default 0) to `value`, down as
  well as up and through negative numbers. The text grows from its anchor:
  `anchorX={1}` keeps the last digit and the suffix in place as digits are
  added. There is no tabular-figures option; a proportional font's digits
  differ in width, so use a monospaced font when no digit may move.
- **`<StatCard value label? note?>`**: a `Panel` (its props, `padding`
  default 32, `cornerRadius` default 16) with the label, the counting
  number, and the note stacked with `gap` (default 8). `labelStyle`,
  `valueStyle`, `noteStyle`, and `color` style them; `align` (`'left'`,
  `'center'`, `'right'`) places the lines and the edge the number grows
  from. It is sized once, from the widest line and the number at `from` and
  at `value`, so it does not grow mid-count; `width`/`height` size it
  yourself, centring the lines.
- **`<BarChart data width>`**: horizontal bars, one row per
  `{ id, label?, value, color?, textColor?, note? }`. A label column (as wide
  as the widest label or note, or `labelWidth`, cutting longer labels off),
  then a bar from 0 that is `value / max` of the room (`max` defaults to the
  largest value; larger values stop at full length), then the value: past
  the bar's end (`valuePosition="end"`, the default) or lined up at the
  right edge (`"right"`, last digits in place). Room for the values is the
  widest final value, or `valueWidth`.
  - Each row starts `stagger` frames after the one above, from `delay`,
    fades in over `fadeInFrames` (default 8), and over `durationInFrames`
    its bar grows and its value counts up from 0 **with the same
    progress**, so the number always matches the bar.
  - Sizes: `rowHeight` (default the taller of `barHeight` and the label with
    its note), `rowGap` 16, `barHeight` 28, `cornerRadius` 4, `labelGap` 24,
    `valueGap` 16. Colors: `barColor` (`#4F8CFF`), `trackColor` (a
    full-length track behind each bar; none by default), and per item
    `color` (the bar) and `textColor` (label and value, over the styles'
    fill); hex or theme names.
  - `order`: `'input'` (default) keeps the given order; `'descending'` and
    `'ascending'` sort by value, equal values keeping their given order.
    Rows are keyed by `id`, which must be unique.
  - Values must be finite and 0 or more (bars grow from 0 to the right;
    diverging bars are not supported). A value of 0 draws no bar; when every
    value (or `max`) is 0 no bar has length and the values print 0. Empty
    `data` draws nothing.
- **`<Ranking order data width>`**: a `BarChart` with a rank column. `order`
  is required, so whether rows are sorted is always explicit. The rank is
  the row's position from the top (1 first; equal values get consecutive
  ranks in their given order), printed by `formatRank` (default
  `String(rank)`) in `rankStyle` (default `labelStyle`), right-aligned in
  `rankWidth` (default the widest rank) before `rankGap` (24).

```tsx
import { BarChart, Counter, Ranking, StatCard } from '@celesta/components';

<Counter x={1800} y={120} anchorX={1} value={1280} suffix=" 件" fontSize={96} fontWeight={700} />
<StatCard x={120} y={600} width={420} height={240} fill="#182030" color="#F4F1EA"
  label="総ダウンロード" value={1280} suffix=" 件" note="2026 年 10 月" />
<BarChart x={120} y={240} width={1000} stagger={8} decimals={1} suffix=" s" data={[
  { id: 'a', label: 'A', value: 12.4 },
  { id: 'b', label: 'B', value: 8.1 },
]} />
<Ranking x={120} y={240} width={1600} order="descending" valuePosition="right"
  formatRank={(rank) => String(rank).padStart(2, '0')}
  data={rows.map((row) => ({ id: row.id, label: row.name, value: row.score }))} />
```

Runnable: [`packages/cli/examples/with-counter.tsx`](../../../packages/cli/examples/with-counter.tsx),
[`packages/cli/examples/with-bar-chart.tsx`](../../../packages/cli/examples/with-bar-chart.tsx).

## Callout (a note with a line to its target)

Imported from `@celesta/components`. A box with a title and text, sized
from the measured text like [`Badge`](#badge-labels-sized-to-their-text),
and a line from the box to a `target` point ending in an arrowhead. Use it
instead of a `<Rect>`, a `<Text>`, and a `<Line>` or `<Arrow>` placed by
hand.

| Prop | Default | Notes |
| --- | --- | --- |
| `target` | required | `{ x, y }`, the point the line ends at. |
| `x`, `y` | required | Where the box goes, in the same coordinates as `target`. |
| `anchorX`, `anchorY` | `0` | Which point of the box is at `x`/`y`, as fractions of its width and measured height (`anchorY={0.5}` centres it on `y`). It only places the box. |
| `width` | required | The box, padding included. Its height is the measured title and text plus the padding. |
| `title`, `text` | none | Text, which may hold `<Span>`, wrapped inside the padding. Either may be left out. |
| `titleStyle`, `textStyle`, `color` | `{ fontSize: 28, fontWeight: 700 }`, `{ fontSize: 22 }`, none | `color` is a hex or theme name for both, unless a style sets a fill. |
| `gap` | `8` | Between the title and the text. |
| `padding`, `cornerRadius`, `fill`, `stroke`, `strokeWidth`, `shadow` | `20`, `12`, none | The box, as `Panel` takes them. |
| `side` | `'auto'` | The edge the line leaves from, at its middle: `'auto'` takes the edge facing the target (of how far it lies outside across and down, the larger), or `'top'`, `'right'`, `'bottom'`, `'left'`. |
| `route` | `'straight'` | `'elbow'` goes straight out of the edge, then turns once to the target. |
| `lineColor`, `lineWidth`, `dash`, `cap` | `#FFFFFF`, `2`, none, `'round'` | The line; `lineColor` takes theme names. The arrowhead stays solid. |
| `head`, `headLength`, `headWidth` | `'arrow'`, as `Arrow` | `'none'` ends the line at the target without a head. |
| `lineProgress` | `1` | How much of the line is drawn, 0–1 of its length from the target; animate it to grow the line from the target to the box. |
| `opacity`, `lang`, `fontFallback`, `id` | | For the whole callout. |

- `target`, `x`, and `y` are in the Callout's parent's coordinates, and the
  Callout has no transform of its own (no `scale` or `rotation`): it would
  move the line's end off the target. Put it in the same `<Group>` as what
  it points at, and the line stays on the target whatever that group's
  transform. Animate `target`, `x`, and `y` and the line follows.
- A target inside the box draws no line.
- The line is drawn under the box, so the box's border covers its end.
- To fade it in and out, wrap it in `<Motion enter="fade">`. The moving
  presets (`fade-up`, `scale`, …) move the whole group, the line's target
  end included, off the target.
- Following a moving object automatically, reading another layer's
  position, and keeping several callouts from overlapping are not provided.

```tsx
import { Callout } from '@celesta/components';

<Callout target={{ x: 960, y: 540 }} x={1200} y={240} width={480}
  title="ここに注目" text="対象の特徴を説明します"
  fill="#182030" color="#F4F1EA" lineColor="#FF4D1F" />

// From a named edge, turning once; growing from the target.
<Callout target={{ x: 400, y: 800 }} x={600} y={420} width={360} side="bottom" route="elbow"
  title="Elbow" lineProgress={progress(frame, 0, 20)} />
```

Runnable: [`packages/cli/examples/with-callout.tsx`](../../../packages/cli/examples/with-callout.tsx).

## Static (still subtrees)

`<Static>` marks content that stays the same from frame to frame. React
renders it only when `deps` change, and its layers are walked, sent to the
renderer, and parsed once and then reused. 100,000 still rects inside one
`<Static>` cost about 7 ms a frame, against 85 ms in a `React.memo`
component and more without one.

```tsx
import { Static } from '@celesta/react';

<Static deps={[theme, rows]}>
  <Background />
  {rows.map((row) => <Row key={row.id} {...row} />)}
</Static>
<Cursor />
```

- **`deps`** works like `useMemo`'s: the content is rebuilt when one of
  them changes (`Object.is`). Leave it out for content fixed for as long as
  it is mounted. `children` is not compared.
- **Keep the frame out of it.** A component inside that calls
  `useCurrentFrame()` still draws correctly, but the subtree is then
  rebuilt and sent again on every frame it changes, and saves nothing.
- **It cannot contain** `<Video>`, `<Audio>`, `<Sequence>`,
  `<FreezeFrame>`, or elements from other packages that register their own
  host elements (characters, dialogue): the frame fails with
  `<Static> cannot contain <video>` and similar. Rects, paths, text,
  images, `<Instances>`, groups, clips, masks, and effects are fine.
- It is drawn as a group with no transform of its own. Move or fade the
  content with a `<Group>` around it; that does not rebuild it.

## Video

| Prop | Notes |
| --- | --- |
| `src` | Path or URL. Without `width` or `height`, drawn at natural pixel size. |
| `width`, `height` | Display size in pixels, as on `<Image>`: one alone keeps the frame's aspect ratio; both stretch the frame to that box. Finite and positive. No `fit`. `scale` applies afterwards. |
| `startFrom` | Seconds into the file at the enclosing sequence's frame 0. Default `0`. |
| `playbackRate` | Static number only. Default `1`. |
| `volume` | Linear gain of the soundtrack, number or [keyframes](#keyframe-values). Default `1`. |
| `muted` | Silences the soundtrack. Default `false`. |
| `duckGroup` / `duck` | As on `<Audio>`: see [Ducking](#ducking). |
| `loop` | Past the end of the file, starts the picture and soundtrack over from the file's beginning. Default `false`: the last frame holds, in silence. |

A `<Video>` plays its soundtrack with its picture, on the same clock:
`startFrom`, `playbackRate`, and the enclosing sequences apply to both, and
the sound follows the same rules as an [`<Audio>`](#audio). A file without
an audio stream is silent. Set `muted` to keep only the picture.

**Do not add an `<Audio>` with the same `src` to hear a `<Video>`**; the
sound would play twice. Compositions written for earlier versions, where a
`<Video>` was silent, did this: remove that `<Audio>`, or mute the `<Video>`.

To show a 1280×720 clip full-screen in a 1920×1080 composition, give it
`width={1920}`. To contain or cover footage of another aspect ratio, wrap it
in `<Fit sourceWidth={…} sourceHeight={…}>`, which contains it by default, or
add `mode="cover"` to fill the area and crop (see
[Layout helpers](#layout-helpers)). To fit it into a framed box with rounded
corners and a caption, use [`MediaFrame`](#mediaframe-images-and-videos-in-a-frame).

HDR video (PQ or HLG, such as iPhone footage) and BT.2020 SDR video are
converted to SDR the way Chrome draws them onto a canvas, so the native
preview, the export, and `@celesta/web` agree. The HDR reference white
(203 nits, HLG's 75% signal) lands at half of SDR white's light, so HDR
footage looks darker than a white `<Rect>` beside it; highlights roll off up
to the file's MaxCLL (1000 nits without one). Export is SDR.
`preloadMedia()` reports `video.hdr` (`'pq'` or `'hlg'`) for such a file.

## MediaFrame (images and videos in a frame)

Imported from `@celesta/components`. An image or a video fitted into a
`width`×`height` frame drawn by [`Panel`](#panel-cards-and-boxes-with-padding),
with an optional caption under it. Use it instead of a `<Rect>` behind a
`<Group clip>` around a scaled `<Video>` with a `<Text>` placed under it.

| Prop | Default | Notes |
| --- | --- | --- |
| `kind` | required | `'image'` or `'video'`. Never guessed from the file name. |
| `src`, `name` | required, none | As `<Image>` / `<Video>` take them (paths, URLs, asset refs). |
| `width`, `height` | required | The frame, padding included. |
| `fit` | `'contain'` | `'contain'` shows all of the media, centred, on the `fill`; `'cover'` fills the area, cropped, centred. |
| `sourceWidth`, `sourceHeight` | required for video | The video's size: `<Video>` has no `fit`, so `MediaFrame` scales it to fit and needs its size. Read it once with `preloadMedia()` in `prepare()`. |
| `padding` | `0` | A mat between the frame's edge and the media, as `Panel` takes it. |
| `fill`, `stroke`, `strokeWidth`, `cornerRadius`, `shadow`, `anchorX`, `anchorY` | as `Panel` | The border draws over the media. |
| `caption`, `captionStyle`, `captionGap` | none, `{ fontSize: 20 }`, `12` | Text under the frame, wrapped to its width (`captionStyle.align` aligns it), its line box `captionGap` below the frame. |
| `variant` | `'plain'` | The decoration; only `'plain'` exists (browser and device chrome are not provided yet). |
| `startFrom`, `playbackRate`, `loop` | as `Image` / `Video` | For an animated image or a video. |
| `volume`, `muted` | as `Video` | Video only. |

- The media is clipped to the area inside the padding, its corners rounded
  by `cornerRadius` less the narrowest padding edge (never below 0).
- An image is an `<Image>` given that area and `fit`, so it needs no size.
- A video is a `<Video>` scaled and centred in the area, with every playback
  prop passed through unchanged: it starts with the enclosing `<Sequence>`,
  and its soundtrack is collected exactly as a bare `<Video>`'s.
- The anchor and the transforms apply to the frame, the media, and the
  caption together.
- An unknown `kind`, `fit`, or `variant`, a video without a positive source
  size, and negative or non-finite sizes throw.

```tsx
import { MediaFrame } from '@celesta/components';
import { preloadMedia } from '@celesta/media-utils';
import type { MediaInfo } from '@celesta/media-utils';

let clip: MediaInfo;
export async function prepare() { clip = await preloadMedia('./demo.mp4'); }

<MediaFrame kind="video" src={clip.src} sourceWidth={clip.video!.width} sourceHeight={clip.video!.height}
  x={120} y={120} width={960} height={540} fit="cover" cornerRadius={24} startFrom={2}
  caption="アプリのプレビュー" captionStyle={{ fontSize: 24, fill: { type: 'solid', color: '#8A97A8' } }} />
<MediaFrame kind="image" src="./shot.png" width={480} height={640} fit="contain"
  padding={12} fill="#182030" cornerRadius={20} />
```

Runnable: [`packages/cli/examples/with-media-frame.tsx`](../../../packages/cli/examples/with-media-frame.tsx).

## Audio

| Prop | Notes |
| --- | --- |
| `src` | Path or URL. Any file FFmpeg can decode, including a video file's audio. |
| `startFrom` | Seconds into the file. Default `0`. |
| `playbackRate` | Number or [keyframes](#keyframe-values). Default `1`. |
| `pitch` | Semitones, number or [keyframes](#keyframe-values). Default `0`, range −24…24; fractions are cents (`0.5` = 50 cents). Values outside ±24 are clamped. |
| `preservePitch` | Default `false`: `playbackRate` moves speed and pitch together. `true`: `playbackRate` changes only the duration and `pitch` alone sets the pitch. Heard transposition is `2^(pitch/12) × (preservePitch ? 1 : playbackRate)`. |
| `volume` | Linear gain, number or [keyframes](#keyframe-values). Default `1`. |
| `muted` | Default `false`. |
| `loop` | Past the end of the file, starts over from its beginning. Default `false`: silence. |
| `fadeIn` | Seconds to rise from silence where the clip starts being heard. Default `0`. |
| `fadeOut` | Seconds to fall to silence where the clip stops being heard. Default `0`. |
| `duckGroup` | The [duck group](#ducking) the clip's sound belongs to. |
| `duck` | Turns the clip down while clips of other groups play. See [Ducking](#ducking). |

`pitch` and `preservePitch` apply to `<Audio>` only, not to a `<Video>`'s
soundtrack or `<Dialogue>` voices. Preview and export sound the same;
`@celesta/web`'s `exportMp4` rejects a clip that uses them ("Audio pitch and
preservePitch are not supported by browser export yet.").

`fadeIn` / `fadeOut` are `<Audio>` only too, and `exportMp4` applies them.

- **Edges of what is heard.** The fades sit at the edges of the clip's
  audible window: its innermost `<Sequence>`, or an outer one that cuts it
  shorter. The fade-out follows the sequence's end when its length changes;
  nothing needs recomputing. Use them instead of `frameKeyframes` for a
  plain fade in or out.
- They multiply `volume` (number or keyframes), so a fade and a ducking
  curve combine.
- Linear in amplitude; there is no curve option yet.
- Fades that add up to more than the clip overlap and multiply: the clip
  never reaches full volume.
- Timed on the timeline, not the file: `loop` does not repeat them, and
  `playbackRate` / `pitch` do not stretch them. A file that ends before the
  clip does is already silent when the fade-out falls.
- A negative or non-finite value throws.

```tsx
<Sequence durationInFrames={300}>
  <Audio src="./bgm.wav" volume={0.6} fadeIn={1.5} fadeOut={3} />
</Sequence>
```

Runnable: [`packages/cli/examples/with-audio-fade.tsx`](../../../packages/cli/examples/with-audio-fade.tsx).

### Ducking

Mark the clips that should be heard over others with `duckGroup`, and give
the others `duck`: their gain dips while a clip of a group in `under`
plays. Dialogue voices are in `"voice"` already
([dialogue.md](dialogue.md#ducking-bgm-and-game-audio-under-speech)).

```tsx
<Audio src="./narration.wav" duckGroup="voice" />
<Audio src="./bgm.wav" volume={0.6} duck={{ under: 'voice' }} />
<Video src="./game.mp4" duck={{ under: ['voice', 'sfx'], to: { db: -12 } }} />
```

| `duck` field | Notes |
| --- | --- |
| `under` | A group name or a list of them. A group no clip has ducks nothing. |
| `to` | The gain under them: a multiplier from 0 to 1, or `{ db }` at or below 0. Default `0.3`. |
| `attack` | Seconds the gain takes to come down, ending as the other clip starts. Default `0.15`. |
| `release` | Seconds it takes to come back up after the other clip ends. Default `0.4`. |
| `holdGap` | Silences shorter than this many seconds stay ducked. Default `0.6`. |

- A marked clip counts from its start to where its file runs out, or to the
  end of its sequence if that comes first; a looping clip counts for its
  whole sequence. Muted clips do not count, and a clip never ducks under
  itself.
- Runs of speech closer together than `holdGap`, or than
  `attack + release`, are joined, so the gain does not bob up between them.
  The ramps ease in and out (`ease-in-out` on the gain).
- The dip multiplies `volume` and the fades. It is timed on the timeline:
  `playbackRate`, `loop`, and `pitch` of the ducked clip do not move it.
- `attack: 0` or `release: 0` switches the gain instantly, which can click
  on some material. Values are used as written.
- Out-of-range values (`to: 1.5`, a negative `attack`) fail the export and
  preview with the clip's `src` and the field; a non-finite number throws in
  React already.
- Preview, export, and `@celesta/web`'s `exportMp4` play the same curve. To
  build it yourself, `duckEnvelope(intervals, duck, { origin })` from
  `@celesta/media-utils` returns it as keyframes (intervals and `origin`
  in seconds); from a dialogue plan, where `origin` is in frames,
  use `duckingVolume()` ([dialogue.md](dialogue.md#ducking-bgm-and-game-audio-under-speech)).

Bare, an `<Audio>` spans the whole composition. Inside a `<Sequence>` it
starts when the sequence starts and stops when it ends. An `<Audio>` behind a
conditional plays only on frames where it is rendered.

## Font and Assets

```tsx
<Assets>
  <Font src="./fonts/MPLUSRounded1c-Bold.ttf" />
  <Font google="M PLUS Rounded 1c" weights={[400, 700]} subsets={['japanese']} />
</Assets>
```

- `<Font src>` loads TTF, OTF, WOFF, WOFF2, or a web-font CSS stylesheet
  (each `@font-face` loads at most one file: in its last `src` with a
  supported entry, the first `url()` whose `format()` is TTF/OTF/WOFF/WOFF2
  or absent; EOT, SVG fonts, collections, and `tech()` such as COLRv1 are
  skipped, and a file that fails to load is not retried with the next
  entry; a `src` with no such `url()` draws the first installed font its
  `local()` entries name by full or PostScript name, case-insensitively,
  in preview and export alike — a `url()` wins even after a `local()`, and
  a rule whose fonts are not installed draws nothing, so prefer files for
  reproducible exports). Refer to it by the **family name
  stored inside the font**, not the file name. See
  [text.md](text.md#fonts-fallback-and-emoji) for weights and fallback.
- `<Font google>` requests a family by name instead of `src`. `weights`
  defaults to `[400]` (100–900 in steps of 100). Omit `subsets` to load
  every script the family has; listing scripts such as `['japanese']`
  loads only those plus Latin. Every returned face is loaded. Native
  rendering caches CSS and files for later offline use. Unknown names or
  unavailable weights report an error. `googleFont(family, { weights,
  subsets, text })` returns the same URL for `measureText`'s `fonts` option.
- `text` loads only the characters a video draws, one small file per weight
  instead of the many files a Japanese family is split into:
  `<Font google="Noto Sans JP" weights={[500, 700]} text={strings.join('')} />`.
  Characters are deduplicated and sorted, so the same set shares one cache
  entry; changing the set makes a new request. A character missing from
  `text` is drawn by the next family in the `fontFamily` stack or a system
  fallback font, and the export warns that the family has no glyph for it.
  `text` cannot be combined with `subsets`, and must not be empty.
  Fonts retain their own licenses; include license and copyright notices
  when redistributing files. The cache does not fetch license files.
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
| `Center` | common props | Moves the origin to the center of the canvas or the enclosing `SafeArea`/`Fit`/`LayoutBounds` (and so `Panel`). `x`/`y` offset from there. |
| `SafeArea` | `padding`: number, `{ top, right, bottom, left }` (missing edges are 0), or `'vertical-social'` | Insets children; nested helpers see the smaller area. A preset is sized from the canvas ([vertical video](#vertical-video-shorts-tiktok-reels)). |
| `Stack` | `spacing` (required), `direction`: `'vertical'` (default) or `'horizontal'` | Child *i* is placed at `i * spacing`. |
| `Grid` | `columns`, `columnWidth`, `rowHeight`, `columnGap?`, `rowGap?` | Fills cells row by row; each child's origin is its cell's top-left. |
| `Fit` | `sourceWidth`, `sourceHeight`, `mode`: `'contain'` (default) or `'cover'` | Scales content designed at the source size to the current area and centers it. |
| `LayoutBounds` | `width`, `height` | Makes that the area nested helpers see, without moving or drawing anything. For components with a size of their own; `Panel` uses it for its content area. |

`useLayoutBounds()` returns the current area's `{ width, height }`. For a
box with a background and padding around its content, use
[`Panel`](#panel-cards-and-boxes-with-padding).

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

`safeAreaInsets(padding, width, height)` resolves any `padding` value to
`{ top, right, bottom, left }` pixels, for placing things at the safe area's
edges yourself.

## Vertical video (Shorts, TikTok, Reels)

Use a 1080×1920 canvas (`width={1080} height={1920}`). The players draw
their own UI over the video, and not evenly: the caption, channel name, and
progress bar cover about the bottom 20%, the like/comment/share buttons run
down the right edge, and search or tabs sit at the top. Split the frame in
two:

- **Inside the safe area**: everything meant to be read or tapped
  (headlines, body text, subtitles, code, numbers, logos, calls to action),
  and faces or the subject a shot is about.
- **Edge to edge**: backgrounds, footage, images, gradients, textures, and
  decorative motion. They may run under the UI; only don't let the meaning
  depend on the part that is covered.

`'vertical-social'` is a preset for that area: top 10%, right 18.5%, bottom
20%, left 1/15 of the canvas, which is top 192, right 200, bottom 384, left
72 px at 1080×1920. It is a rough guide covering the three platforms, not
any platform's own figures, and it does not follow their UI changes; pass
per-edge numbers when you know better. The safe area is off-center (wider
margin on the right, more at the bottom), so center copy inside it with
`<Center>`, not on the canvas.

```tsx
<Composition width={1080} height={1920} fps={30} durationInFrames={300}>
  <Image src="./bg.jpg" width={1080} height={1920} />  {/* edge to edge */}
  <SafeArea padding="vertical-social">
    <Center>
      <Text anchorX={0.5} anchorY={0.5} maxWidth={760}
        style={{ fontSize: 96, align: 'center' }}>Readable copy</Text>
    </Center>
  </SafeArea>
  <DebugOverlay safeArea="vertical-social" />
</Composition>
```

Leave some room inside the safe area for motion: a zoom or slide that
starts inside it can carry copy out. Check the result with the guides drawn
into PNG frames (`--debug-guides`, see
[verify.md](verify.md#check-a-vertical-videos-safe-area)).

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
`{ src, durationSeconds?, video?: { width, height, codec?, frameRate?, durationSeconds?, color?: { primaries?, transfer?, matrix? }, hdr?: 'pq' | 'hlg' }, audio: [{ codec?, sampleRate?, channels?, durationSeconds? }], animation?: { frameCount, durationSeconds } }`.
`color` holds FFmpeg's names for the stream's tags (`bt2020`,
`smpte2084`, `bt2020nc`, …); `hdr` is set for HDR video (see [Video](#video)).
`animation` is there for an animated GIF, APNG, or WebP, whose
`durationSeconds` is then one pass through its frames, as `<Image>` plays it.
`mediaDurationInFrames(info, fps)` rounds up to whole frames. To lay out a
whole voiced script this way, use `planDialogue()` (see
[dialogue.md](dialogue.md#timing-a-script-from-its-voices)).

## Audio spectrum and loudness

Also from `@celesta/media-utils`. `loadAudioSpectrum(src, options?)`
decodes an audio file (or a video's soundtrack) in `prepare()` and analyzes
all of it; `useAudioSpectrum(spectrum, { startFrom? })` returns
`{ bands: number[], rms: number }` for the current frame:

```tsx
let music: AudioSpectrum;
export async function prepare() {
  music = await loadAudioSpectrum('./score.mp3', { bands: 32, smoothing: { attack: 0.01, release: 0.2 } });
}
function Bars() {
  const { bands, rms } = useAudioSpectrum(music);
  return <>{bands.map((level, i) => <Rect key={i} x={i * 20} y={400} width={16} height={4 + level * 300} anchorY={1} fill="#ffffff" />)}</>;
}
// <Audio src={music.src!} /> plays the same file.
```

- `bands` are 0–1: each band's mean FFT power in dB relative to a
  full-scale sine, mapped from `minDecibels` (default −80) to `maxDecibels`
  and clamped. `maxDecibels` defaults to `'auto'`, the loudest band level
  anywhere in the file before smoothing, so only the file's peaks reach 1
  however loud it was mastered. Pass a number for a fixed ceiling, e.g. to
  give several files the same scale (`maxDecibels: -6`); a ceiling below
  the music's peaks pins its loudest bands at 1. `rms` is linear, about
  0.707 for a full-scale sine.
- Options: `fftSize` (power of two, 32–32768, default 2048), `bands`
  (default 32), `scale` (`'log'` default, `'linear'`, `'mel'`),
  `minFrequency` (20 Hz) / `maxFrequency` (16 kHz, capped at Nyquist),
  `smoothing` (seconds, or `{ attack, release }`; default 0), `hopSeconds`
  (default 1/240).
- The window is Hann, centered on the frame's time; the channels are
  averaged at the file's own sample rate. The file is analyzed in order once,
  every `hopSeconds`, and frames interpolate between hops, so values do not
  depend on fps or on the order frames are rendered in.
- The hook reads the enclosing `<Sequence>`'s clock, like `<Audio>`; pass
  `startFrom` to match an `<Audio startFrom>`. `playbackRate` is not
  followed. Outside the file every value is 0.
- `spectrumAt(spectrum, seconds)` is the same lookup without a hook;
  `analyzeAudioSpectrum({ sampleRate, samples }, options)` analyzes mono
  samples you already have.

## Keyframe values

`Audio` `volume`/`playbackRate`/`pitch` and `Dialogue` `volume`/`playbackRate`
accept a number or keyframes. Keyframe times are **seconds from the start of the
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
key and defaults to `'linear'`. For a plain fade in or out, use
[`fadeIn` / `fadeOut`](#audio) instead, and to dip under voices that play,
[`duck`](#ducking). Dip the BGM by hand from frame 30 to 60, on top of its
fades:

```tsx
const { fps } = useVideoConfig();
<Audio src="./bgm.wav" fadeIn={0.5} fadeOut={0.5} volume={frameKeyframes([
  { frame: 25, value: 0.8 },
  { frame: 30, value: 0.3, easing: 'ease-out' },
  { frame: 60, value: 0.3 },
  { frame: 65, value: 0.8, easing: 'ease-in' },
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

## Debug guides

Imported from `@celesta/debug`.

`<DebugOverlay safeArea? shade? color? showCenter? showFrame? />` outlines
the safe area, shades the canvas outside it, and draws a center cross and
frame counter. `safeArea` takes the same values as `<SafeArea padding>`: a
number, `{ top, right, bottom, left }`, or `'vertical-social'` (default: 5%
of the shorter side on every edge). `shade` is the fill outside the safe
area, `false` for none. `<DebugBounds width height label? color?
showOrigin?>` wraps children and outlines a box.

Both draw in the app preview and in PNG frames and contact sheets exported
with `--debug-guides` ([verify.md](verify.md#look-at-real-frames)), and
never in an MP4, so they are safe to leave in. `useDebugGuides()` returns
`true` where guides are drawn, for your own guides. `useIsPreview()` returns
`true` only in the app preview.
