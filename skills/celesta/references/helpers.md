# Before writing a helper

Celesta already ships the small helpers a composition tends to grow: clamps,
eased progress, seeded randomness, timecodes, text widths, and "which item is
current". Use these instead of writing your own. They behave the same in
preview and export, and other people (and agents) reading the file know them.

| Instead of writing… | Use | From |
| --- | --- | --- |
| `Math.min(1, Math.max(0, x))` | `clamp01(x)`, `clamp(x, min, max)` | `@celesta/math` |
| `a + (b - a) * t`, mapping one range onto another | `lerp(a, b, t)`, `remap(v, inMin, inMax, outMin, outMax)`, `remapClamped(...)` | `@celesta/math` |
| `easing(clamp((f - start) / frames))` | `progress(f, start, frames, easing?)` | `@celesta/react` |
| A frame range mapped to values, with easing | `interpolate(f, [a, b], [from, to], { easing, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' })` | `@celesta/react` |
| A hex color blended by hand | `interpolateColor(f, [a, b], ['#…', '#…'])` | `@celesta/react` |
| `fract(Math.sin(i * 12.9898 + …) * 43758.5453)` | `random(seed)`, `randomRange(seed, min, max)`, `randomInt(...)` | `@celesta/math` |
| Wobble from summed sines | `noise(seed, t)`, `loopNoise(seed, t, period)` for loops | `@celesta/math` |
| `pad2(minutes) + ':' + pad2(seconds) + …` | `frameToTimecode(f, fps)` (`HH:MM:SS:FF`, or `MM:SS:FF` with `{ hours: false }`), `timecodeToFrame('01:02.5', fps)` | `@celesta/react` |
| SRT/WebVTT times written by hand | `formatSubtitles(cues, { fps, format })` | `@celesta/react` |
| `text.length * fontSize * 0.6` | `useTextMetrics(text, style).width`, `textCaret(metrics, offset)` | `@celesta/react` |
| A text width measured during `prepare()` | `measureText(text, style, { fonts })` | `@celesta/react` |
| `[...items].reverse().find((s) => f >= s.at)` | `useCue(cues)`, `cueAt(cues, f)` | `@celesta/react` |
| A `Rect` sized to `useTextMetrics(text).width` plus padding behind a label | `<Badge padding={…} fill={…}>{text}</Badge>` | `@celesta/components` |
| Corner brackets, a timecode, and a progress bar drawn by hand in a HUD | `<FrameChrome corners timecode progress />` | `@celesta/components` |
| `Array.from({ length: cols * rows }, …)` of `<Rect>` dots or grid lines | `<PatternBackground pattern="dots" spacing={80} size={3} />` | `@celesta/components` |
| `from: i * 6` on each item | `<Stagger each={6}>` | `@celesta/react` |
| Scene start frames summed by hand | `<Series>` + `computeSeries` | `@celesta/react` |
| A beat counter from the BPM | `useBeat({ bpm })` | `@celesta/react` |

## Numbers

```tsx
import { clamp, clamp01, lerp, remapClamped } from '@celesta/math';

const t = clamp01((frame - 30) / 20);
const x = lerp(-200, 960, t);
const size = clamp(fontSize, 24, 96);
const alpha = remapClamped(distance, 0, 400, 1, 0);
```

## Eased progress

`progress` clamps to 0–1 and applies the easing (linear when none is
given). It replaces `ease(x)` wrappers and local `progress` helpers with a
house easing: pass the easing at each call.

```tsx
import { Easings, progress } from '@celesta/react';

// Was: ease((frame - 12) / 20), with ease = (n) => 1 - (1 - clamp(n)) ** 4
const enter = progress(frame, 12, 20, Easings.easeOutQuart);
const fadeOut = 1 - progress(frame, durationInFrames - 15, 15);
```

More: [animation.md](animation.md#progress), [interpolate](animation.md#interpolate).

## Seeded random

Give every property its own string seed. The values differ from a `sin`
hash, so a scene converted from one keeps its density but not its exact
layout.

```tsx
import { random, randomRange } from '@celesta/math';

// Was: hash(i, 1) * W, hash(i, 2) * H
const stars = Array.from({ length: 120 }, (_, i) => ({
  x: randomRange(`star-${i}-x`, 0, 1920),
  y: random(`star-${i}-y`) * 1080,
}));
```

More: [math.md](math.md).

## Timecodes

```tsx
import { frameToTimecode } from '@celesta/react';

frameToTimecode(1234, 30);           // '00:00:41:04'
frameToTimecode(1234, 30, { hours: false });  // '00:41:04', minutes:seconds:frames
```

For subtitle files, build the cues and let `formatSubtitles` write the
times ([subtitles.md](subtitles.md)):

```tsx
const srt = formatSubtitles([{ from: 0, durationInFrames: 60, text: 'Hello.' }], { fps: 30, format: 'srt' });
```

## Text widths

Measure with the same style the `<Text>` uses; never multiply a character
count by an advance width. `width` includes spaces.

```tsx
import { Group, Rect, Text, textCaret, useTextMetrics } from '@celesta/react';

function Chip({ label }: { label: string }) {
  const style = { fontFamily: 'JetBrains Mono', fontSize: 22, fontWeight: 500 };
  const { width } = useTextMetrics(label, style);
  return <Group>
    <Rect y={-28} width={width + 56} height={56} cornerRadius={28} fill="#1A1C22" />
    <Text x={44} anchorY={0.5} style={{ ...style, fill: { type: 'solid', color: '#9EC9FF' } }}>{label}</Text>
  </Group>;
}

// A caret after the first n characters:
const caretX = textCaret(useTextMetrics(line, style), n).x;
```

A render-time measurement needs the exporter to evaluate the frame in
`inspect.mjs` ([verify.md](verify.md#measured-text---native)). Keep a fixed
advance only where that matters more than exact widths, and say so in a
comment. More: [text.md](text.md#measure-text).

## The current item

Cues are `{ at, …data }` sorted by `at`. `cueAt` returns the cue in effect
(or `null` before the first), its `index`, `frame` (frames since it
started), and its `previous` and `next` cues.

```tsx
import { cueAt, progress, useCue } from '@celesta/react';

const SECTIONS = [{ at: 0, name: 'INTRO' }, { at: 120, name: 'DEMO' }, { at: 480, name: 'OUTRO' }];

// Was: [...SECTIONS].reverse().find((s) => frame >= s.at)?.name
const section = cueAt(SECTIONS, frame)?.cue.name ?? '';

// In a component, with a slide-in on every change:
const active = useCue(SECTIONS);
const slide = active ? progress(active.frame, 0, 10) : 0;
```

Start frames kept as a plain array convert once:
`const CUES = STARTS.map((at) => ({ at }))`. More:
[animation.md](animation.md#usecue--cueat).
