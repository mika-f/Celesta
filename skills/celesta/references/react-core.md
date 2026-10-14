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
- [Instances (particles)](#instances-particles)
- [Static (still subtrees)](#static-still-subtrees)
- [Video](#video)
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
| `opacity` | `1` | `0`–`1`; multiplies down through groups. |
| `blendMode` | `'normal'` | How the layer combines with what is beneath it: `'normal'`, `'multiply'`, `'screen'`, `'overlay'`, `'add'`, or `'difference'`, as in CSS `mix-blend-mode`. |
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
| `id` | generated | Optional stable layer id. |

To center something at a point, set `anchorX={0.5} anchorY={0.5}` and put
the point in `x`/`y`.

Effects apply to the composited pixels of the layer; on a `Group` they apply
to all children together. They combine with `opacity` and `blendMode`.
They run in this order: `shader` (see [shader.md](shader.md)), then the
color filters in the order of the table (`brightness` first, `vignette`
last), then `shadow` and `glow` from the result, with `blur` on top. Each
color filter works on the straight sRGB color, keeps alpha, and clamps its
result to 0–1; values that change nothing (`brightness={1}`,
`grayscale={0}`) add no work. For another order, pass the filters to
`shader` as an array: `shader={[grayscale(), tint({ color: '#704214', amount: 0.4 })]}`.
Color filters also run in the browser editor.
Compute any prop from `useCurrentFrame()` to animate it. Visual props do not
take keyframe objects in React. Effects and blend modes cost GPU passes; see
[performance.md](performance.md) before putting them on many layers.

## Composition

`width`, `height`, `fps`, `durationInFrames`: all required positive
integers. Use even `width`/`height` for video export. Exactly one, at the root.
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
`{ x, y }`), for a marker or a label that rides the line. Both take
`dash`/`dashOffset`; the dashes are anchored at the first point, so they
stay put while `progress` draws the line on.

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
| `src` | Path or URL. Drawn at natural pixel size. |
| `startFrom` | Seconds into the file at the enclosing sequence's frame 0. Default `0`. |
| `playbackRate` | Static number only. Default `1`. |
| `volume` | Linear gain of the soundtrack, number or [keyframes](#keyframe-values). Default `1`. |
| `muted` | Silences the soundtrack. Default `false`. |
| `loop` | Past the end of the file, starts the picture and soundtrack over from the file's beginning. Default `false`: the last frame holds, in silence. |

A `<Video>` plays its soundtrack with its picture, on the same clock:
`startFrom`, `playbackRate`, and the enclosing sequences apply to both, and
the sound follows the same rules as an [`<Audio>`](#audio). A file without
an audio stream is silent. Set `muted` to keep only the picture.

**Do not add an `<Audio>` with the same `src` to hear a `<Video>`**; the
sound would play twice. Compositions written for earlier versions, where a
`<Video>` was silent, did this: remove that `<Audio>`, or mute the `<Video>`.

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
| `loop` | Past the end of the file, starts over from its beginning. Default `false`: silence. |

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
  (every `@font-face` in it is loaded). Refer to it by the **family name
  stored inside the font**, not the file name. See
  [text.md](text.md#fonts-fallback-and-emoji) for weights and fallback.
- `<Font google>` requests a family by name instead of `src`. `weights`
  defaults to `[400]` (100–900 in steps of 100); `subsets` names scripts.
  Google may return all subsets; every returned face is loaded. Native
  rendering caches CSS and files for later offline use. Unknown names or
  unavailable weights report an error. `googleFont(family, { weights,
  subsets })` returns the same URL for `measureText`'s `fonts` option.
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
| `Center` | common props | Moves the origin to the center of the canvas or the enclosing `SafeArea`/`Fit`. `x`/`y` offset from there. |
| `SafeArea` | `padding`: number, `{ top, right, bottom, left }` (missing edges are 0), or `'vertical-social'` | Insets children; nested helpers see the smaller area. A preset is sized from the canvas ([vertical video](#vertical-video-shorts-tiktok-reels)). |
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
`{ src, durationSeconds?, video?: { width, height, codec?, frameRate?, durationSeconds? }, audio: [{ codec?, sampleRate?, channels?, durationSeconds? }], animation?: { frameCount, durationSeconds } }`.
`animation` is there for an animated GIF, APNG, or WebP, whose
`durationSeconds` is then one pass through its frames, as `<Image>` plays it.
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
