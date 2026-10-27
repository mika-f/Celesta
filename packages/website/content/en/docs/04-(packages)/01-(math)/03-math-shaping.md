The remaining helpers in [`@celesta/math`](/en/docs/math/) reshape numbers, build repeating motion, and do the angle and point arithmetic that layout and orbits need. They are plain functions, so you can use them anywhere: in a component, in `prepare()`, or in a data file.

## Shaping numbers

**Scalars**

| API | Use it for |
| --- | --- |
| `clamp(value, min, max)` | Limits `value` to `[min, max]`. `clamp01(value)` limits it to `[0, 1]`. |
| `lerp(a, b, t)` | The value `t` of the way from `a` to `b`. `t` is not clamped, so values outside 0–1 extrapolate. |
| `inverseLerp(a, b, value)` | The opposite: how far `value` is from `a` to `b`, as a `t`. Returns `0` when `a === b`. |
| `remap(value, inMin, inMax, outMin, outMax)` | Moves a value from one range to another, without clamping. `remapClamped` keeps the result inside the output range. |
| `step(edge, x)` | `0` below `edge`, `1` from `edge` on. |
| `smoothstep(edge0, edge1, x)` | A smooth 0–1 ramp as `x` goes from `edge0` to `edge1`. `smootherstep` has an even gentler start and finish. |
| `fract(x)` | The fractional part, always in `[0, 1)`, also for negative numbers. |
| `mod(value, divisor)` | Modulo with the sign of the divisor: `mod(-1, 4)` is `3`, not `-1`. |
| `wrap(value, min, max)` | Wraps into `[min, max)`, for looping positions and hues. |
| `pingPong(value, length)` | Bounces a steadily increasing value back and forth between `0` and `length`. |
| `snap(value, increment)` | Rounds to the nearest multiple of `increment`, for stepped or grid-locked motion. |
| `roundTo(value, decimals?)` | Rounds to a number of decimals, for tidy on-screen readouts. |
| `approxEqual(a, b, epsilon?)` | Whether two numbers differ by at most `epsilon` (`1e-6` by default). |

> [!NOTE]
> **remap or interpolate?**
> `remap` is a plain linear conversion with no easing and, unless you use `remapClamped`, no clamping. For motion over frames, with easing and `extrapolateLeft`/`extrapolateRight`, use [`interpolate`](/en/docs/animation/) from `@celesta/react`. `remap` shines when converting other kinds of values, such as turning a noise result into an opacity.

```tsx title="Loops and bounces"
import { pingPong, snap, wrap } from '@celesta/math';

const frame = useCurrentFrame();
const ticker = wrap(frame * 4, -200, 1920);   // scrolls right, re-enters at the left
const sweep = pingPong(frame * 8, 1600);      // 0 → 1600 → 0, repeating
const stepped = snap(sweep, 100);             // the same sweep, in 100 px jumps
```

## Waves

The four wave functions repeat with a **period of 1** and return values in `[-1, 1]`, so you pass `frame / framesPerCycle`. They are in phase: each is positive for the first half of a cycle and negative for the second, so you can swap one for another.

**Waves**

| API | Use it for |
| --- | --- |
| `sineWave(t)` | A smooth sine. Gentle pulses, bobbing, breathing. |
| `triangleWave(t)` | Straight ramps up and down. Constant-speed back-and-forth. |
| `squareWave(t)` | `1` for the first half of each cycle, `-1` for the second. On/off blinking. |
| `sawtoothWave(t)` | Ramps from 0 up to 1, drops to -1 halfway, and ramps back to 0. Repeating sweeps. |

```tsx title="A breathing circle on a 2-second cycle"
import { sineWave } from '@celesta/math';

const { fps } = useVideoConfig();
const frame = useCurrentFrame();
const breath = sineWave(frame / (fps * 2)); // one full cycle every two seconds

<Rect x={960} y={540} anchorX={0.5} anchorY={0.5} width={200} height={200}
  cornerRadius={100} scale={1 + 0.08 * breath} fill="#a68bbf" />
```

## Angles

Functions that take or return angles use **radians**. A layer’s `rotation` prop is in **degrees**, so convert with `radToDeg` when you hand it a computed angle.

**Angles**

| API | Use it for |
| --- | --- |
| `TAU` | A full turn in radians (2π). |
| `degToRad(degrees)`, `radToDeg(radians)` | Convert between degrees and radians. |
| `normalizeAngle(angle)` | Wraps an angle into `[-π, π)`. |
| `angleDifference(from, to)` | The shortest signed turn from one angle to another. |
| `lerpAngle(a, b, t)` | Interpolates between angles the short way round, so 350° to 10° passes through 0° instead of sweeping back through 180°. |

## Points and geometry

Points are plain `{ x, y }` objects, typed as `Vec2`. Celesta’s y axis points down, so a positive angle turns **clockwise** on screen, the same as `rotation`.

**2D points**

| API | Use it for |
| --- | --- |
| `distance(a, b)` | The straight-line distance between two points. |
| `angleBetween(from, to)` | The direction from one point to another, in radians; `0` points along +x. |
| `lerpPoint(a, b, t)`, `midpoint(a, b)` | A point along a segment, and the point halfway. |
| `rotatePoint(point, angle, origin?)` | Turns a point about an origin (the coordinate origin by default). |
| `polarToCartesian(angle, radius, center?)`, `cartesianToPolar(point, center?)` | Convert between a direction-and-distance and an `{ x, y }`. The second returns `{ angle, radius }`. |
| `quadraticBezierPoint(p0, p1, p2, t)`, `cubicBezierPoint(p0, p1, p2, p3, t)` | The point at `t` (0–1) along a Bézier curve, as drawn by a path’s `quadTo` and `cubicTo`. Use them to move an object along a curve you also draw. |

```tsx title="Dots on a rotating ring"
import { TAU, polarToCartesian } from '@celesta/math';

const frame = useCurrentFrame();
const dots = Array.from({ length: 12 }, (_, i) => {
  const angle = (i / 12) * TAU + frame / 60; // evenly spaced, slowly turning
  const p = polarToCartesian(angle, 300, { x: 960, y: 540 });
  return <Rect key={i} x={p.x} y={p.y} anchorX={0.5} anchorY={0.5}
    width={24} height={24} cornerRadius={12} fill="#a68bbf" />;
});
```

```tsx title="A marker riding a curve"
import { cubicBezierPoint } from '@celesta/math';

const p0 = { x: 200, y: 800 }, p1 = { x: 500, y: 200 };
const p2 = { x: 1400, y: 200 }, p3 = { x: 1700, y: 800 };
const t = progress(useCurrentFrame(), 0, 90, Easings.easeInOutCubic);
const marker = cubicBezierPoint(p0, p1, p2, p3, t);

<Path commands={[
  { type: 'moveTo', ...p0 },
  { type: 'cubicTo', x1: p1.x, y1: p1.y, x2: p2.x, y2: p2.y, x: p3.x, y: p3.y },
]} stroke="#57456c" strokeWidth={4} />
<Rect x={marker.x} y={marker.y} anchorX={0.5} anchorY={0.5} width={32} height={32}
  cornerRadius={16} fill="#a68bbf" />
```

Both `quadraticBezierPoint` and `cubicBezierPoint` use the same control points as a `Path`’s `quadTo` and `cubicTo` commands, so the marker stays on the drawn curve. See [Text effects, camera & lines](/en/docs/text-camera-lines/) for `Path`.

## Colors

Color props take `#RRGGBB` or `#RRGGBBAA`, in either case. These two helpers read that format exactly as the renderer does, so you can check or take apart a color that comes from data or a property input before you use it.

**Colors**

| API | Use it for |
| --- | --- |
| `isHexColor(value)` | Whether `value` is a `#RRGGBB` or `#RRGGBBAA` string. |
| `parseHexColor(color)` | The color as `[r, g, b, a]`, each 0–255 (`#RRGGBB` has an alpha of 255), or `undefined` when it is not one. |

```tsx title="A data color with a fallback, at half its alpha"
import { isHexColor, parseHexColor } from '@celesta/math';

const fill = isHexColor(row.color) ? row.color : '#a68bbf';
const [r, g, b, a] = parseHexColor(fill)!;
const overlay = `#${[r, g, b, Math.round(a / 2)].map((c) => c.toString(16).padStart(2, '0')).join('')}`;
```
