# Time and animation

How a React composition moves: frame hooks, `interpolate`, easings,
springs, `Sequence` scenes, transitions, and the motion helpers that save
hand-written timing math. All imported from `@celesta/react`.

Every frame must be a pure function of the frame number: frames are rendered
out of order when scrubbing and exporting. Never use `Math.random()`,
`Date.now()`, timers, `useEffect`, or state that accumulates across renders.
For randomness and noise, use [`@celesta/math`](math.md).

## Contents

- [Pick a helper](#pick-a-helper)
- [Hooks](#hooks)
- [interpolate](#interpolate)
- [Easings](#easings)
- [spring](#spring)
- [progress](#progress)
- [Sequence](#sequence)
- [Series and computeSeries](#series-and-computeseries)
- [Stagger](#stagger)
- [Transition](#transition)
- [useBeat / beatAt](#usebeat--beatat)
- [useCue / cueAt](#usecue--cueat)
- [Camera](#camera)
- [Timecodes](#timecodes)

## Pick a helper

| You want… | Use |
| --- | --- |
| A value from A to B over some frames | `interpolate` with `'clamp'`, or `progress` |
| A bouncy pop-in | `spring` |
| Scenes back to back | `Series` + `computeSeries` |
| A voiced script back to back | `planDialogue` + `DialogueSeries` ([dialogue.md](dialogue.md#timing-a-script-from-its-voices)) |
| Items entering one after another | `Stagger` (or `progress(frame, i * each, …)`) |
| Fade/slide/scale at a scene's start or end | `Transition` |
| Motion on the music's beat | `useBeat` |
| Something that changes at given frames (captions, camera stops, chart callouts) | `useCue` |
| A push-in, pan, or shake over a whole scene | `Camera` |
| A line being drawn | `Polyline progress` ([react-core.md](react-core.md#path-line-polyline)) |
| An arrow growing toward its target | `Arrow` with animated `x2`/`y2` ([react-core.md](react-core.md#circle-ellipse-arrow)) |
| Text revealed line by line, typed, or a counting number | `TextReveal`, `useTypewriter`, `useCountUp` ([text.md](text.md#text-motion)) |
| Code being typed | `@celesta/code` with `useTypewriter` ([code.md](code.md)) |

## Hooks

| Hook | Returns |
| --- | --- |
| `useCurrentFrame()` | Integer frame, local to the innermost `<Sequence>`. |
| `useCurrentTime()` | Exact `{ value, timescale }` time, local to the innermost `<Sequence>`. |
| `useVideoConfig()` | `{ width, height, fps, durationInFrames }`; inside a `Sequence`, `durationInFrames` is the sequence's. |
| `useIsPreview()` | `true` only in the Celesta app preview, `false` in exports. |

Hooks work in any component Celesta renders, including `Root`, but not in
`prepare()`, at module level, or in plain helper functions.
`<Composition>`'s own props are read once, when the entry is loaded: compute
them from constants or `prepare()` results, never from `useCurrentFrame()`
or `useVideoConfig()`.

## interpolate

`interpolate(input, inputRange, outputRange, options?)`

```ts
interpolate(frame, [0, 20, 40], [0, 1, 0], {
  easing: Easings.easeInOutSine,     // applied inside each segment
  extrapolateLeft: 'clamp',          // 'extend' (default) | 'clamp' | 'identity'
  extrapolateRight: 'clamp',
});
```

`inputRange` must be strictly increasing and the same length as
`outputRange` (at least 2). **The default extrapolation is `extend`**, so
values keep changing past the range; pass `'clamp'` for fades and moves.

## Easings

`linear`, `easeIn`, `easeOut`, `easeInOut` (quadratic), and
`easeIn…`/`easeOut…`/`easeInOut…` for `Sine`, `Quad`, `Cubic`, `Quart`,
`Quint`, `Expo`, `Circ`, `Back`, `Elastic`, `Bounce` (for example
`Easings.easeOutBack`). Every easing returns exactly 0 at 0 and 1 at 1. Any
`(t: number) => number` works too. JSON keyframes use the same names in
kebab case (`ease-out-back`).

## spring

`spring({ frame, fps, config?, from?, to?, delay?, durationInFrames? })`

Damped spring from `from` (default 0) to `to` (default 1), which may
overshoot. `config`: `stiffness` (100), `damping` (10), `mass` (1),
`overshootClamping` (false). Frames before `delay` return `from`.

```tsx
const frame = useCurrentFrame();
const { fps } = useVideoConfig();
const scale = spring({ frame, fps, delay: 10, config: { damping: 12 } });
```

## progress

`progress(frame, start, durationInFrames, easing?)`: clamped 0–1 position of
`frame` in the span, with `easing` applied (linear by default). The building
block of entrances:
`opacity={progress(frame, 10, 20, Easings.easeOutExpo)}`.

## Sequence

`<Sequence from? durationInFrames?>` shows its children only during
`[from, from + durationInFrames)` in the parent's frames (default: until the
parent's end). Inside, frames and media restart at 0. Sequences nest and
accept common layer props, plus `lang` (default text language for the
children).

Children are unmounted outside this window, so their component code and
hooks do not run. Entering the window again mounts them afresh, resetting
their local React state. Declare fonts or assets needed throughout the
composition in `<Assets>` outside a sequence.

For scenes in a row, prefer `Series` (next) to computing `from` by hand.

## Series and computeSeries

Scenes back to back, by length. Children must be `<Series.Sequence
durationInFrames offset?>`; each is a `<Sequence>` (local frames restart at 0).
A negative `offset` overlaps the previous item. `computeSeries(items)` returns
`{ sequences: [{ from, durationInFrames }], durationInFrames }` without
rendering: use it for the `<Composition>` length and for anything outside the
series that needs a scene's start (a HUD, a wipe on the cut).

```tsx
const SCENES = [
  { name: 'intro', durationInFrames: 90, Scene: Intro },
  { name: 'body', durationInFrames: 240, Scene: Body },
];
const { durationInFrames } = computeSeries(SCENES);
// …
<Composition width={1920} height={1080} fps={30} durationInFrames={durationInFrames}>
  <Series>
    {SCENES.map(({ name, durationInFrames, Scene }) => (
      <Series.Sequence key={name} durationInFrames={durationInFrames}><Scene /></Series.Sequence>
    ))}
  </Series>
</Composition>
```

## Stagger

`<Stagger each from? durationInFrames?>` wraps child *i* in a
`<Sequence from={from + i * each}>`, so a component that animates from its
own frame 0 cascades down a list. A child is hidden until it starts.
Children carry their own positions (`Stagger` is not a layout helper; it
cannot sit inside `Grid`/`Stack` as one cell per child).

```tsx
function Row({ y, label }: { y: number; label: string }) {
  const p = progress(useCurrentFrame(), 0, 20, Easings.easeOutExpo);
  return <Text x={120 + 40 * (1 - p)} y={y} opacity={p}>{label}</Text>;
}
<Stagger each={4}>{items.map((s, i) => <Row key={s} y={200 + i * 64} label={s} />)}</Stagger>
```

## Transition

`<Transition type durationInFrames …>` animates its children at the start
(`direction="in"`, default) or end (`direction="out"`) of the enclosing
sequence (or composition).

| Prop | Notes |
| --- | --- |
| `type` | `'fade'`, `'slide'`, or `'scale'`, or an array to combine them (`['fade', 'slide']`). |
| `durationInFrames` | Positive integer. |
| `direction` | `'in'` (default) or `'out'`. |
| `slideFrom` | `'left'` (default), `'right'`, `'top'`, `'bottom'`. |
| `distance` | Slide distance in px, default `64`. |
| `scaleFrom` | Starting scale, default `0.8`. |
| `easing` | `(t) => number`, for example `Easings.easeOutCubic`. |

Nest transitions to combine an entrance and an exit.

## useBeat / beatAt

`useBeat({ bpm, beatsPerBar?, offset?, decay? })` (or
`beatAt(frame, fps, options)` outside a component) returns
`{ framesPerBeat, beat, bar, beatInBar, progress, barProgress, pulse }`.
`pulse` is 1 on each beat and decays exponentially (`decay` frames to 1/e,
default a quarter beat). Inside a `Sequence` the grid starts with the
sequence; use `offset` (frames) to align with music that started elsewhere.

## useCue / cueAt

`cues` is an array of `{ at, …data }` sorted by `at`. `useCue(cues)` (or
`cueAt(cues, frame)`) returns `null` before the first cue, else
`{ cue, index, frame, previous, next }` where `frame` counts from the cue's
start.

```tsx
const stop = useCue(STOPS); // [{ at: 30, x: 0 }, { at: 75, x: 1000 }, …]
const from = stop?.previous?.x ?? 0;
const x = stop ? from + (stop.cue.x - from) * progress(stop.frame, 0, 24, Easings.easeInOutCubic) : 0;
```

## Camera

`<Camera x? y? zoom? rotation? shake? shakeFrequency? seed?>` shows the
world point (`x`, `y`) at the center of the current area (canvas,
`SafeArea`, or `Fit`), magnified by `zoom` about that point. Defaults look at
the center, so `<Camera zoom={1.05}>` is a slow push-in. `shake` is the
largest drift in world pixels, smoothed by `@celesta/math`'s `noise()`.
Animate the props from the frame for pans and zooms.

## Timecodes

`timecodeToFrame(timecode, fps)`: `'01:02.500'`, `'1:02:03'`, or `'12.5'` →
frame number. Handy for syncing to timestamps the user gives you.
`frameToTimecode(frame, fps)` goes the other way, formatting `HH:MM:SS:FF`
for an on-screen clock.
