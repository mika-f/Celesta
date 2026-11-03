Noise is smooth randomness: nearby inputs give nearby outputs, so values wander instead of jumping. Use it for drift, wobble, handheld shake, floating particles, and organic textures. Like everything in [`@celesta.studio/math`](/en/docs/math/), it is deterministic: the same seed and coordinates return the same value on every render.

## 1D noise over time

`noise(seed, t)` returns a value in `[-1, 1]`. It picks a random value at each whole number of `t` and blends smoothly between them, so the scale of `t` sets the speed: `frame / 20` changes direction about every 20 frames.

```tsx title="A gentle floating motion"
import { noise } from '@celesta.studio/math';

function Floating({ children }: { children: React.ReactNode }) {
  const frame = useCurrentFrame();
  return (
    <Group x={noise('float-x', frame / 45) * 24} y={noise('float-y', frame / 60) * 16}
      rotation={noise('float-r', frame / 90) * 3}>
      {children}
    </Group>
  );
}
```

- Use a **different seed for each property**. With one seed, `x` and `y` move in lockstep along a diagonal.
- Larger divisors are slower and calmer: `frame / 90` drifts, `frame / 8` trembles.
- Scale by the amount you want: `noise(…) * 24` moves up to 24 pixels either way.
- Add a per-item offset for a crowd: ``noise(`dot-${i}`, frame / 40)`` keeps each dot on its own path.

> [!NOTE]
> **Camera shake**
> The `shake` prop of [`Camera`](/en/docs/text-camera-lines/) is built for handheld drift. Reach for `noise` directly when you want to shake something that is not the camera, or to shape the shake yourself.

## 2D and 3D noise

`noise2D(seed, x, y)` samples a smooth field over a plane. Scale the position down, as in `x / 200`, to stretch the features; the smaller the divisor, the busier the field. `noise3D(seed, x, y, z)` adds a third coordinate, which you can treat as time to animate a field smoothly.

```tsx title="A field of dots that breathe"
import { noise3D } from '@celesta.studio/math';

function Field() {
  const frame = useCurrentFrame();
  const dots = [];
  for (let row = 0; row < 12; row++) {
    for (let col = 0; col < 20; col++) {
      // Two coordinates for position, the third for time.
      const n = noise3D('field', col / 6, row / 6, frame / 60);
      const size = 28 + n * 20;
      dots.push(<Rect key={`${row}-${col}`} x={140 + col * 82} y={100 + row * 80}
        anchorX={0.5} anchorY={0.5} width={size} height={size} cornerRadius={size / 2}
        opacity={0.5 + n * 0.5} fill="#a68bbf" />);
    }
  }
  return <>{dots}</>;
}
```

Every result lies in `[-1, 1]`. To turn it into a size, an opacity, or any other range, use [`remap`](/en/docs/math-shaping/) or the arithmetic above: `0.5 + n * 0.5` maps it to `[0, 1]`.

## Layered noise (fbm)

One layer of noise moves on a single scale, which looks smooth and a little artificial. Fractal Brownian motion, `fbm`, adds several layers, each finer and weaker than the last. The result wanders on a large scale and also has small detail, like clouds, smoke, terrain, or flame. `fbm`, `fbm2D`, and `fbm3D` take the same arguments as the plain versions plus an options object, and still return values in `[-1, 1]`.

**fbm options**

| API | Use it for |
| --- | --- |
| `octaves` | How many layers to add. A whole number of at least 1; the default is `4`. More layers add finer detail and cost a little more per call. |
| `lacunarity` | How much finer each layer is than the one before. The default is `2`, doubling the frequency. |
| `gain` | How much weaker each layer is than the one before. A finite number of at least 0; the default is `0.5`. Lower values keep the result smoother, higher values make it rougher. |

```tsx title="A rough horizon line"
import { fbm } from '@celesta.studio/math';

const points: [number, number][] = Array.from({ length: 97 }, (_, i) => [
  i * 20,
  700 + fbm('ridge', i / 14, { octaves: 5, gain: 0.55 }) * 120,
]);

<Polyline points={points} stroke="#a68bbf" strokeWidth={3} />
```

Invalid options throw an error that names the function, such as `fbm2D() requires a whole number of octaves of at least 1`.

## Noise that loops

`noise` and `fbm` never repeat, so in a video that plays on repeat (Shorts, TikTok), a wobble driven by them jumps where the last frame meets the first. `loopNoise(seed, t, period)` is the same smooth noise, but it repeats every `period`, counted in the units of `t`: pass the length of the loop scaled the same way as the frame. `loopFbm` does the same for layered noise.

```tsx title="A sway that loops"
import { loopNoise } from '@celesta.studio/math';

const LOOP = 384; // the composition's durationInFrames

function Sway({ children }: { children: React.ReactNode }) {
  const frame = useCurrentFrame();
  return <Group x={loopNoise('sway', frame / 20, LOOP / 20) * 24}>{children}</Group>;
}
```

## Which one do I use?

**Choosing a noise function**

| You want | Use |
| --- | --- |
| One value that drifts over time | `noise(seed, frame / n)` |
| A value that wanders on several scales | `fbm(seed, frame / n)` |
| A value that drifts in a looping video | `loopNoise(seed, frame / n, durationInFrames / n)` |
| A pattern across the screen | `noise2D` or `fbm2D` sampled at scaled positions |
| A pattern across the screen that moves | `noise3D` or `fbm3D` with time as the third coordinate |
| A new, unrelated value each frame or beat | [`random`](/en/docs/math/) with the frame in the seed |
