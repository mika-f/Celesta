# @celesta/math: random, noise, and math helpers

```tsx
import { noise, random, randomRange } from '@celesta/math';
```

Pure functions with no React; they work in components, `prepare()`, and
module scope alike. Celesta's runtime supplies the package; never install it
from npm. (Older code imported `random`/`noise` from `@celesta/react`; they
now live only here.)

Everything seeded takes a number or string `seed` and returns the same value
for the same seed on every render. Use these, never `Math.random()`; give
each property its own seed (`` `star-${i}-x` ``).

## Contents

- [Random](#random)
- [Noise](#noise)
- [Numbers](#numbers)
- [Waves](#waves)
- [Angles and points](#angles-and-points)

## Random

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

A starfield, stable across frames:

```tsx
const STARS = Array.from({ length: 120 }, (_, i) => ({
  x: randomRange(`star-${i}-x`, 0, 1920),
  y: randomRange(`star-${i}-y`, 0, 1080),
  phase: random(`star-${i}-phase`) * 100,
}));
// in a component: opacity={0.5 + 0.5 * noise(`twinkle-${i}`, (frame + s.phase) / 20)}
```

## Noise

All return smooth values in `[-1, 1]`; different seeds give unrelated fields.

- `noise(seed, t)`: 1D value noise; feed it `frame / 20` or so for drift and
  wobble.
- `noise2D(seed, x, y)` / `noise3D(seed, x, y, z)`: gradient (Perlin) noise,
  `0` on whole-number coordinates. Scale positions down (`x / 200`); use the
  third coordinate as time to animate a 2D field.
- `fbm(seed, t, opts?)`, `fbm2D(seed, x, y, opts?)`, `fbm3D(seed, x, y, z,
  opts?)`: layered noise with detail at several scales. `opts` are
  `octaves` (4), `lacunarity` (2), and `gain` (0.5).

## Numbers

`clamp(v, min, max)`, `clamp01(v)`, `lerp(a, b, t)`, `inverseLerp(a, b, v)`,
`remap(v, inMin, inMax, outMin, outMax)` (unclamped), `remapClamped(...)`,
`step(edge, x)`, `smoothstep(e0, e1, x)`, `smootherstep(e0, e1, x)`,
`fract(x)`, `mod(v, n)` (never negative for positive `n`), `wrap(v, min,
max)`, `pingPong(v, length)`, `snap(v, increment)`, `roundTo(v, decimals)`,
`approxEqual(a, b, epsilon = 1e-6)`. For frame-to-value mappings with easing,
prefer `interpolate` ([animation.md](animation.md#interpolate)).

## Waves

`sineWave(t)`, `triangleWave(t)`, `squareWave(t)`, `sawtoothWave(t)`: period
1, range `[-1, 1]`, all in phase: positive for the first half of each period
and negative for the second (sine, triangle, and sawtooth start at 0; the
square wave starts at 1). Pass `frame / framesPerCycle`.

## Angles and points

Angles are **radians** (`degToRad`/`radToDeg` convert a layer's `rotation`,
which is in degrees); with y pointing down, a positive angle turns
clockwise. `TAU`, `normalizeAngle(a)` (to `[-π, π)`),
`angleDifference(from, to)`, `lerpAngle(a, b, t)` (the short way round).

Points are `{ x, y }` (`Vec2`): `distance(a, b)`, `angleBetween(from, to)`,
`lerpPoint(a, b, t)`, `midpoint(a, b)`, `rotatePoint(p, angle, origin?)`,
`polarToCartesian(angle, radius, center?)`, `cartesianToPolar(p, center?)`,
`quadraticBezierPoint(p0, p1, p2, t)`, and `cubicBezierPoint(p0, p1, p2, p3,
t)` (the curves a `Path`'s `quadTo`/`cubicTo` draw).
