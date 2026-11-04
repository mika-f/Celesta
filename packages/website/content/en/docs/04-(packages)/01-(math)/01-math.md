`@celesta.studio/math` is a small, dependency-free toolkit for the numbers behind motion: seeded randomness, smooth noise, number shaping, waves, angles, and 2D points. It is bundled with the runtime, so a composition imports it directly:

```tsx title="Import"
import { random, randomRange, noise } from '@celesta.studio/math';
```

Choose **File → Set Up TypeScript** for editor types (see [Your first React composition](/en/docs/react-compositions/)), or use it in the [web editor](/en/#playground), which accepts `@celesta.studio/react`, `@celesta.studio/math`, and `react`. The package is not published to npm yet.

This chapter covers randomness. [Noise & fbm](/en/docs/math-noise/) covers smooth drift, and [shaping, waves & geometry](/en/docs/math-shaping/) covers the everyday helpers.

## Why not Math.random()?

A composition is a function of the frame. The preview scrubs backward, the exporter renders frames in order, and both must draw the same picture for the same frame. `Math.random()` gives a different answer every call, so stars would jump around each time you scrub. Every function in `@celesta.studio/math` is a pure function of its inputs: the same seed returns the same value in preview, in export, and on every machine.

## Seeds

A `Seed` is a number or a string. `random(seed)` returns a number in `[0, 1)`, and the same seed always returns the same number. Give each thing you randomize its own seed, and vary it by index and by property:

```tsx title="One seed per property"
const x = randomRange(`star-${i}-x`, 0, 1920);
const y = randomRange(`star-${i}-y`, 0, 1080);
const size = randomRange(`star-${i}-size`, 2, 6);
```

- Reusing a seed reuses the value. `random('a')` called twice is the same number, so `x` and `y` need different seeds or the stars line up on a diagonal.
- A fractional number is its own seed: `0.5` does not collide with `0` or `1`. A number and a string that spells it, such as `0.5` and `'0.5'`, are the same seed.
- A seed must be a string or a finite number. `NaN` and `Infinity` throw an error that names the function.
- To make a value change over time, put time in the seed: `random(Math.floor(frame / 4))` jumps to a new value every four frames, which suits flicker and glitch effects. For smooth change, use [`noise`](/en/docs/math-noise/).

## The random functions

**Randomness**

| API | Use it for |
| --- | --- |
| `random(seed)` | A number in `[0, 1)`. |
| `randomRange(seed, min, max)` | A number in `[min, max)`. |
| `randomInt(seed, min, max)` | A whole number from `min` to `max`, **both included**. Throws if no whole number lies between them. |
| `randomBool(seed, probability?)` | `true` with the given probability (`0.5` by default). |
| `randomSign(seed)` | `-1` or `1`, each half the time. Handy for mirroring a direction or spin. |
| `randomPick(seed, items)` | One element of `items`. Throws on an empty list. |
| `shuffle(seed, items)` | A shuffled **copy** of `items`; the original is untouched. |
| `randomGaussian(seed, mean?, stdDev?)` | A bell-curve value: most land within `stdDev` of `mean` (defaults `0` and `1`), a few land much further out. |
| `randomInCircle(seed, radius?, center?)` | A point spread evenly over a disc, as `{ x, y }`. Defaults to radius `1` about the origin. |

## A seeded starfield

Because the seeds are fixed, the field below is identical on every frame; only the drift, from [`noise`](/en/docs/math-noise/), moves:

```tsx title="A starfield that drifts"
import { noise, randomRange } from '@celesta.studio/math';

function Stars() {
  const frame = useCurrentFrame();
  return <>
    {Array.from({ length: 80 }, (_, i) => (
      <Rect key={i} width={4} height={4} cornerRadius={2} fill="#ffffff"
        x={randomRange(`star-${i}-x`, 0, 1920) + noise(i, frame / 40) * 30}
        y={randomRange(`star-${i}-y`, 0, 1080)}
        opacity={randomRange(`star-${i}-o`, 0.2, 0.8)} />
    ))}
  </>;
}
```

## Choosing and ordering things

`randomPick` and `shuffle` pick from lists you already have, such as a palette or a set of captions. Seed them with something stable, like the item’s index:

```tsx title="A palette, and a shuffled order"
const palette = ['#a68bbf', '#7cf29c', '#f2c97c', '#7cc4f2'];

const color = randomPick(`bar-${i}-color`, palette);
const order = shuffle('slides', ['intro', 'demo', 'pricing', 'outro']);
const mirrored = randomBool(`bar-${i}-flip`, 0.3); // flipped about 30% of the time
const spin = 45 * randomSign(`bar-${i}-spin`);     // clockwise or counter-clockwise
```

## Natural-looking scatter

Uniform randomness looks artificial: every value is as likely as any other. `randomGaussian` clusters values around a center, and `randomInCircle` spreads points evenly by area instead of crowding the middle of a disc, so a burst of particles looks round rather than starry-centered:

```tsx title="A burst of particles"
import { randomGaussian, randomInCircle } from '@celesta.studio/math';

const particles = Array.from({ length: 120 }, (_, i) => {
  const p = randomInCircle(`spark-${i}`, 260, { x: 960, y: 540 });
  const size = Math.max(2, randomGaussian(`spark-${i}-size`, 8, 3));
  return <Rect key={i} x={p.x} y={p.y} anchorX={0.5} anchorY={0.5}
    width={size} height={size} cornerRadius={size / 2} fill="#f2c97c" />;
});
```

> [!NOTE]
> **Keep the seed, change the look**
> When a random layout is almost right, change the seed’s prefix (`star-` to `sky-`) and the whole field rearranges. Because every value is repeatable, the version you liked in the preview is the version you export.
