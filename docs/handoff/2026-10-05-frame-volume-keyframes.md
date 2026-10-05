# Frame-based volume keyframes (2026-10-05)

`frameKeyframes(keys, { fps, origin? })` (`packages/react/src/keyframes.ts`)
turns `{ frame, value, easing? }` keys into the `Animatable<number>` that
`<Audio>` / `<Dialogue>` `volume` and `playbackRate` take (issue #137).

- Keyframe times are seconds from the `<Audio>`'s innermost sequence start
  (`collectAudioClip` in `render.ts` measures from `context.originSec`).
  `origin` is that start on the keys' clock, so keys can be written in
  composition frames; keys before it give negative times, which the React
  path already produces when it shifts clipped heads.
- Times are `(frame - origin) / fps` through `secondsToTime`, so they are
  rounded to the nearest microsecond like `<Sequence>` offsets.
- Empty keys, keys out of frame order, non-finite frames/values/origin, and a
  non-positive fps throw. Equal frames are kept: `evaluate_f64` holds the
  first value at that time and the second after it (a cut). The `.celesta.json`
  validator also accepts equal times and rejects descending ones.
- `examples/with-volume-fade.tsx` is mixed and evaluated in
  `crates/react-bridge/tests/node_integration.rs`.
