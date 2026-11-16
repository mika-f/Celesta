# `keyframes()` (2026-11-16)

Design: [`docs/superpowers/specs/2026-11-16-animation-text-helpers-design.md`](../superpowers/specs/2026-11-16-animation-text-helpers-design.md), part 2.

## Specification

`keyframes(frame, keys)` in `@celesta.studio/react` returns the value at
`frame` of a list of keys `{ frame, value, easing? }` (`ValueKeyframe<T>`).

- `value` is a number or an object of numbers; all keys share one shape and
  the result has it. An object result is a new object on every call.
- `easing` shapes the segment that ends at the key, as in
  `frameKeyframes()` and JSON keyframes. It is a function or a kebab-case
  `Easing` name, which resolves to the `Easings` curve of the same name.
  The four factory names (`cubic-bezier`, …) are not easing names.
- The value holds at the first key's before the first key and at the last
  key's after the last.
- Two keys on one frame `f` cut: frames below `f` end on the first key's
  value, and `f` and later use the second. This differs from the Rust
  evaluator at the single instant `t = f` (which returns the first value);
  for a frame, the new value being on screen on `f` matches
  `<Sequence from={f}>`.
- It throws, with the key's index, for: no keys, a non-finite `frame`
  argument, key frame, or value, keys out of order, a third key on one
  frame, values of different shapes, and an unknown easing name.

`frameKeyframes()` is unchanged: it still builds an `Animatable<number>`
for props that take keyframes (audio volume, playback rate, pitch), with
named easings only.

## Implementation

`packages/react/src/keyframes.ts`. All keys are validated on every call,
then the segment is chosen as the last key at or before `frame` and the key
after it, which gives the cut rule without a special case. Exported from
`index.ts` and `browser.ts` beside `frameKeyframes`.

## Checks

- `pnpm --filter @celesta.studio/react exec vitest run test/keyframes-value.test.mts`:
  per-field interpolation, the easing of the ending key, holds, cuts on the
  first, a middle, and the last frame, a fractional frame just below a cut,
  overshoot, every `Easing` name against `Easings`, and each validation
  message.
- `pnpm --filter @celesta.studio/react test`.
- `node skills/celesta/scripts/inspect.mjs packages/cli/examples/with-keyframes.tsx --frames 0,60,119,120,179`:
  the camera group is at scale 2.2 looking at (557.4, 380) on frame 119 and
  at scale 1.8 looking at (1300, 700) on frame 120.
- `celesta-exporter --react packages/cli/examples/with-keyframes.tsx --frames 60,119,120 frame.png`,
  looked at: frame 119 shows the harbor, frame 120 the observatory.
