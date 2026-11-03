Titles that reveal themselves, a camera that travels, and lines that draw on. Like the helpers on the previous page, these are built on the frame, so scrubbing and exporting reproduce every moment.

## Text effects

**Text helpers**

| API | Use it for |
| --- | --- |
| `TextReveal` | Lines of a string slide up from behind their own masks, one after another. Set `lineHeight`, `from`, `stagger`, and `durationInFrames`; `align` pivots each line. `\n` breaks a line; with `maxWidth`, lines also wrap where `<Text maxWidth>` wraps (`style.lineBreak: 'phrase'` included), and each wrapped line is revealed on its own. |
| `TypewriterText` (`@celesta.studio/components`) | Text typed out with a line or block caret, timed as `useTypewriter` times it. The caret goes where the measured layout puts the next character, so proportional fonts, Japanese, `\n`, and `maxWidth` wrapping need no monospaced font. |
| `useTypewriter(text, options)` | The typed-so-far `text`, its `length`, `done`, and `caretVisible`: steady while typing, blinking while idle. |
| `useCountUp(to, options)` | A number counting to `to`, with `from`, `delay`, `durationInFrames`, `easing`, and `decimals`. |
| `Counter`, `StatCard` (`@celesta.studio/components`) | A `<Text>` drawing a counted number formatted, and a card with a label, the number, and a note. They count with `useCountUp`'s default timing and print with `locale` (`'en-US'` by default, never the machine's), `decimals` (always shown), `prefix`/`suffix`, or a `formatter`. With `anchorX={1}` the last digit and the unit stay in place as digits are added. |

```tsx title="An editorial title reveal"
<TextReveal x={160} y={300} lineHeight={200} stagger={6}
  style={{ fontFamily: 'Archivo Black', fontSize: 210,
    fill: { type: 'solid', color: '#ecefe8' } }}>
  {'36 DAYS\nOF CELESTA'}
</TextReveal>
```

For a quick monospaced caret, use `length × advance` as an estimate. For actual glyph positions, use useTextMetrics() in the desktop runtime; see Text & fonts.

## Camera

`<Camera>` looks at a point of a larger world: lay the world out in its own coordinates, then animate `x`/`y` (the point shown at the center), `zoom`, and `rotation`. `shake` adds a smooth handheld drift of up to that many pixels.

```tsx title="Travel along a long timeline"
<Camera x={interpolate(frame, [0, 120], [0, 4000], { extrapolateRight: 'clamp' })}
  y={540} zoom={1.1} shake={4}>
  <TimelineWorld />
</Camera>
```

To blur a fast camera move, wrap the camera in a `MotionBlur` and compute its props from the frame the blur passes in, so they change between its moments; see [Animation](/en/docs/animation/).

## Lines and charts

`<Line x1 y1 x2 y2>` draws a segment and `<Polyline points>` connects several; both take `stroke`, `strokeWidth`, `cap` (`round` by default), and `join`. Animate a polyline’s `progress` from 0 to 1 to draw it on, and use `pointOnPolyline(points, t)` to put a marker on its tip.

For other shapes, `<Path>` takes `points` (with `closed`) or SVG-like `commands` (`moveTo`, `lineTo`, `quadTo`, `cubicTo`, `close`), a `stroke` and/or `fill`, and `cap`/`join`/`miterLimit`. Each is one layer however many segments it has, stays sharp when scaled, and paints a translucent stroke that crosses itself only once, so draw procedural line art as a few paths rather than many thin rects.

Every line and shape stroke takes `dash`, the lengths of its dashes and gaps in turn (like SVG’s `stroke-dasharray`), and `dashOffset`, which shifts them along the line. `dash={[0, 12]}` with `cap="round"` draws dots, and `dashOffset={-frame}` makes the dashes flow towards the end. The pattern is anchored at the first point, so a dashed polyline drawn on with `progress` keeps its dashes in place. Closed shapes run their dashes clockwise: from the rightmost point of a circle or an ellipse, and from the end of a `Rect`’s top-left corner. An `<Arrow>` dashes only its shaft.

A `<Path>` draws part of its stroke too, with `progress` and `progressStart` (0–1 of the path’s length); curves are split exactly along their length. Dashes stay where they are on the whole path, whichever part is drawn. A `fill` always covers the whole shape.

`BarChart` from `@celesta.studio/components` draws horizontal bars from an array of `{ id, label, value }`: a label column, a bar growing from 0, and the value past the bar's end or lined up at the right edge, each row starting `stagger` frames after the one above. The bar's length and the printed number come from the same progress, so the number always matches the bar; it is formatted as `Counter` formats it. `Ranking` adds a rank column and needs `order`: sorted by value (`'descending'` or `'ascending'`, equal values keeping their order) or kept as given (`'input'`). Values must be finite and 0 or more, and ids unique.

```tsx title="Bars that grow in turn"
<BarChart x={120} y={240} width={1000} stagger={8} decimals={1} suffix=" s" data={[
  { id: 'a', label: 'A', value: 12.4 },
  { id: 'b', label: 'B', value: 8.1 },
]} />
```

`Callout` from `@celesta.studio/components` points at something: a box at `x`/`y`, `width` wide, holding a `title` and `text` and as tall as they measure, and a line from the box to a `target` point ending in an arrowhead. The line leaves the middle of the edge facing the target (or the `side` you name) and runs `straight` or as an `elbow` that turns once; `lineProgress` grows it from the target. `target`, `x`, and `y` are in the same coordinates, so a callout in the same `<Group>` as what it points at stays on it whatever that group's transform, and the line follows an animated target. A target inside the box draws no line.

```tsx title="A note with an arrow"
<Callout target={{ x: 960, y: 540 }} x={1200} y={240} width={480}
  title="Look here" text="What this part does."
  fill="#182030" color="#F4F1EA" lineColor="#FF4D1F" />
```

## Moving along and between paths

Functions that take `commands` return points as `{ x, y }`, the same shape as the commands (`pointOnPolyline` returns `[x, y]`, like its `points`).

- `pointOnPath(commands, t)` and `tangentOnPath(commands, t)` give the point and direction `t` (0–1) of the way along. `pointOnPath(commands, progress)` is the tip of a `<Path progress>`. A tangent’s `angle` is in degrees clockwise, ready for `rotation`.
- `getLength`, `getPointAtLength`, and `getTangentAtLength` do the same by length in pixels. `measurePath(commands)` measures once for many lengths.
- `interpolatePath(a, b, t)` returns the shape between two paths. Their commands may differ in number and type: corners are matched by how far around the outline they are.
- `parseSvgPath(d)` turns an SVG `d` string (every command, arcs `A` included) into commands, and `toSvgPath(commands)` turns them back.

```tsx
const ROUTE = parseSvgPath('M0 0 C 100 -80 200 80 300 0');
const t = progress(frame, 0, 90);
const tip = pointOnPath(ROUTE, t);

<Path commands={ROUTE} stroke="#4DD6FF" strokeWidth={4} progress={t} />
<Path x={tip.x} y={tip.y} rotation={tangentOnPath(ROUTE, t).angle} fill="#FFFFFF"
  points={[[12, 0], [-8, -7], [-8, 7]]} closed />
```

## Circles, ellipses, and arrows

`<Circle radius>` and `<Ellipse width height>` are placed like a `Rect`: `x`/`y` put their box’s anchor (top-left by default), `fill` gradients start from the box’s top-left, and a `stroke` stays inside the box. `<Arrow x1 y1 x2 y2>` points its head’s tip at `x2`/`y2`; set `strokeWidth`, `headLength`, `headWidth`, and `heads` (`end`, `start`, or `both`). An arrow shorter than its heads shrinks them to fit, so it can grow from nothing, and one with no length draws nothing. A circle is drawn as a rounded `Rect` and the other two as paths; all three take gradients, transforms, and effects like any layer.

```tsx title="A labelled callout"
<Circle x={960} y={540} anchorX={0.5} anchorY={0.5} radius={80}
  stroke="#FFD84D" strokeWidth={8} />
<Arrow x1={600} y1={300} x2={890} y2={480} strokeWidth={6} stroke="#FFD84D" />
```

## Triangles, polygons, stars, pies, and hearts

`<Polygon radius sides>`, `<Star radius points innerRatio>`, and `<Pie radius startAngle endAngle>` are sized by a radius and placed like a `<Circle>` of that radius; `<Triangle width height apex>` and `<Heart width height>` fill a box placed like a `Rect`. Polygon corners and star tips start at 12 o’clock, and pie angles are degrees clockwise from 12 o’clock, so animate `endAngle` from `startAngle` to sweep a slice open: equal angles draw nothing, and 360° apart a whole circle. `cornerRadius` rounds the corners of every shape but the heart. Each shape is one path whose `stroke` is centred on the outline, unlike a circle’s or an ellipse’s, and takes `join`/`miterLimit` like a `<Path>`; `cap` only shapes the ends of dashes, since the outline is closed.

`trianglePath`, `polygonPath`, `starPath`, `piePath`, and `heartPath` return the same path commands, measured from the box’s top-left, to pass to a `<Path>` or to reshape first.

```tsx title="A pie chart that sweeps open"
<Pie x={760} y={340} radius={200} endAngle={interpolate(frame, [0, 45], [0, 270], { extrapolateRight: 'clamp' })}
  fill="#4DD8C0" />
<Star x={1200} y={340} radius={120} points={5} innerRatio={0.45} cornerRadius={8} fill="#FFD84D" />
<Path x={1200} y={640} commands={starPath({ radius: 120, points: 5 })} stroke="#FFFFFF" strokeWidth={4} />
```
