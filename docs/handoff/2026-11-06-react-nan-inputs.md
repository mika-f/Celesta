# React rejects NaN timing, media, and animation inputs (2026-11-06)

`<Sequence>`, `<Video>`, `<Audio>`, `interpolate()`, and `spring()` turned a
`NaN` (usually computed from a misspelled field, an input prop, or data from
`prepare()`) into a default in TypeScript, so the native side received a
valid value and could not report anything (mika-f/Celesta#708). They now
throw an error naming the component and the prop, as `<Loop>`, `<Image>`,
`computeSeries`, `interpolateColor()`, and `frameKeyframes()` already did.

This is a breaking change: a composition that relied on the silent default
now throws at render time.

## Specification

- **`<Sequence>`**
  - `from`: `undefined` is 0; otherwise a finite number. `NaN`, `±Infinity`,
    and non-numbers throw
    ``<Sequence> `from` must be a finite number of frames, not …``.
  - `durationInFrames`: `undefined` and `Infinity` run to the end of the
    parent, as before; `0` is an empty window. `NaN`, a negative number, and
    non-numbers throw
    ``<Sequence> `durationInFrames` must be a number of frames of at least 0, or Infinity, not …``.
    A negative duration used to be clamped to 0.
- **`<Video>` / `<Audio>`**
  - `startFrom` (both) and `<Video>`'s `playbackRate`: unset is the default,
    otherwise a finite number (`<Video> startFrom must be a finite number`),
    as `<Image>` already checked.
  - `volume`, `playbackRate` (`<Audio>`), and `pitch`: unset is the default;
    otherwise a finite number or keyframes that pass `isKeyframeAnimation`.
    Anything else, including `null` and keyframes with a non-finite `time` or
    `value`, throws `<Audio> volume must be a finite number or keyframes with
    finite times and values` (`<Video>` for a video's soundtrack).
  - `collectAudioClip` checks these before it returns early for a clip
    whose window is empty, so a bad clip throws even where none of it is
    heard. `<Dialogue>` voices go through the same path and
    report as `<Audio>`.
- **`interpolate()`**: requires a finite `input` and finite `inputRange` and
  `outputRange` entries, read with indexed loops so sparse-array holes count
  as missing, matching `interpolateColor()`. `extrapolate*: 'clamp'` never
  covered a `NaN` input (both comparisons are false).
- **`spring()`**: requires `Number.isFinite(fps) && fps > 0` (the message is
  unchanged, `spring() requires a positive fps`) and a finite `frame`.
- Transform props and `opacity` keep dropping a non-finite value
  (`numberOr`); the opacity case is fixed at its usual source,
  `interpolate()`.

## Implementation

- `sequenceWindow(props, parent)` in `packages/react/src/time.ts` validates
  the props and returns the window: `from`, `durationInFrames` (undefined when
  open), its start and end in the parent's local seconds, its end in
  composition seconds, and `openEnd`. `<Sequence>` (components.ts) and the
  walker's `childSequenceContext` (render.ts) both call it, so the window
  rule, including the `openEnd` / `endsWithComposition` decision, is written
  once. Each site passes its own view of the parent: `<Sequence>` its
  context's `durationInFrames` and the root's clock, the walker its
  `originSec` / `windowEndSec`. `<Sequence>` still derives the children's
  `durationInFrames` from its own context when the window is open, since the
  walker does not track frames.
- `render.ts` adds `finiteNumber()` for plain numeric media props and gives
  `animatedNumber()` the element and prop names; it falls back only for
  `undefined`.

## Checks

- `pnpm run build:runtime`
- `pnpm run test:runtime`: every runtime package passes, including the
  `Loop`, `Motion`, `Scenes`, `Series`, `TransitionSeries`, caption-track,
  and dialogue-series tests built on `<Sequence>`.
- New tests in `packages/react/test/sequence.test.mts`,
  `video-audio.test.mts`, and `animation.test.mts` (the first behavioural
  tests of `interpolate` and `spring`). With the four source files reverted
  to `main`, the 7 rejection tests fail and the tests pinning the accepted
  values (`durationInFrames` undefined / `Infinity` / `0`, `interpolate`
  and `spring` results) pass.
- User guide (en/ja): `01-animation.md`, `06-media.md`, and the `Sequence`
  row of the reference. Skill: `animation.md`, `react-core.md`, and a row in
  `errors.md` for each new message.
