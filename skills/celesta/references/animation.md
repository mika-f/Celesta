# Time and animation

How a React composition moves: frame hooks, `interpolate`, easings,
springs, `Sequence` scenes, transitions, and the motion helpers that save
hand-written timing math. Imported from `@celesta/react`, except `Transition`,
`TransitionSeries`, `computeTransitionSeries`, `useTransitionSeriesScene`, and
`useTransitionVolume` (`@celesta/transitions`), `Camera` (`@celesta/layout`),
and `TextReveal`, `useTypewriter`, and `useCountUp` (`@celesta/text`).

Every frame must be a pure function of the frame number: frames are rendered
out of order when scrubbing and exporting. Never use `Math.random()`,
`Date.now()`, timers, `useEffect`, or state that accumulates across renders.
For randomness and noise, use [`@celesta/math`](math.md).

## Contents

- [Pick a helper](#pick-a-helper)
- [Hooks](#hooks)
- [interpolate](#interpolate)
- [interpolateColor](#interpolatecolor)
- [Easings](#easings)
- [spring](#spring)
- [progress](#progress)
- [Sequence](#sequence)
- [FreezeFrame](#freezeframe)
- [Series and computeSeries](#series-and-computeseries)
- [TransitionSeries](#transitionseries)
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
| A color from A to B (fill, gradient stop, shadow) | `interpolateColor` |
| A bouncy pop-in | `spring` |
| Scenes back to back | `Series` + `computeSeries` |
| Scenes joined by a cut, cross-fade, slide, or wipe | `TransitionSeries` + `computeTransitionSeries` |
| A past frame redrawn in place (rewind, flashback, thumbnails) | `FreezeFrame` |
| A voiced script back to back | `planDialogue` + `DialogueSeries` ([dialogue.md](dialogue.md#timing-a-script-from-its-voices)) |
| Items entering one after another | `Stagger` (or `progress(frame, i * each, …)`) |
| Fade/slide/scale of one element at a scene's start or end | `Transition` |
| Motion on the music's beat | `useBeat` |
| A video whose last frame runs into its first (Shorts, TikTok) | periodic motion and `loopNoise` ([loops.md](loops.md)) |
| Something that changes at given frames (captions, camera stops, chart callouts) | `useCue` |
| A push-in, pan, or shake over a whole scene | `Camera` |
| A line being drawn | `Polyline progress` ([react-core.md](react-core.md#path-line-polyline)) |
| An arrow growing toward its target | `Arrow` with animated `x2`/`y2` ([react-core.md](react-core.md#circle-ellipse-arrow)) |
| Text revealed line by line, typed, or a counting number | `TextReveal`, `useTypewriter`, `useCountUp` ([text.md](text.md#text-motion)) |
| Code being typed | `@celesta/code` with `useTypewriter` ([code.md](code.md)) |

## Hooks

| Hook | Returns |
| --- | --- |
| `useCurrentFrame()` | Integer frame, local to the innermost `<Sequence>` (directly inside a `<FreezeFrame>`, its `frame`, rounded). |
| `useCurrentTime()` | Exact `{ value, timescale }` time, local to the innermost `<Sequence>`. |
| `useVideoConfig()` | `{ width, height, fps, durationInFrames }`; inside a `Sequence`, `durationInFrames` is the sequence's. |
| `useIsPreview()` | `true` only in the Celesta app preview, `false` in exports. |
| `useDebugGuides()` | `true` where debug guides are drawn: the app preview, and PNG exports with `--debug-guides`. Never in an MP4. |

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

## interpolateColor

`interpolateColor(input, inputRange, colors, options?)`

```tsx
const fill = interpolateColor(frame, [0, 45, 90], ['#101820', '#EF7B45', '#7FC8F8']);
<Rect width={1280} height={720} fill={fill} />
<Text style={{ fill: { type: 'solid', color: interpolateColor(frame, [0, 20], ['#FFFFFF00', '#FFFFFF']) } }}>…</Text>
<Rect fill={{ type: 'linear', start: { x: 0, y: 0 }, end: { x: 0, y: 400 }, stops: [
  { offset: 0, color: interpolateColor(frame, [0, 60], ['#FFD84D00', '#FFD84D']) },
  { offset: 1, color: '#3366CC' },
] }} … />
```

Same `inputRange` rules and `easing` as `interpolate`, but takes `#RRGGBB` /
`#RRGGBBAA` colors (no CSS names, `rgb()`, or `#RGB`) and returns uppercase
`#RRGGBBAA`.

- **The default extrapolation is `'clamp'`** (unlike `interpolate`): outside
  the range the end color holds. `'extend'` continues the end segment;
  `'identity'` is not accepted.
- Blends in sRGB with **premultiplied alpha**, like gradient stops and CSS:
  `#FF0000` → `#FF000000` stays red while it fades, and a fully transparent
  color's RGB is ignored, so `#00000000` works as "transparent". Channels
  that `'extend'` or an overshooting easing push past 0–255 are clamped.
- At each `inputRange` entry the result is exactly that color (normalized
  to `#RRGGBBAA`), as long as the easing returns 0 at 0 and 1 at 1, as every
  `Easings` curve does.

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

## FreezeFrame

`<FreezeFrame frame>` draws its children as the composition looked at
`frame`, for rewinds, flashbacks, and thumbnail strips. `frame` is an
absolute composition frame wherever the element sits, even inside a
`<Sequence>`. It resets the clock to the root's: directly inside,
`useCurrentFrame()` is `frame` (rounded to the nearest integer when
`frame` is fractional) and `durationInFrames` is the composition's,
while a `<Sequence>` inside still counts from its own start as usual (at
`frame={80}`, `<Sequence from={50}>` sees frame 30). `<Video>`, lip sync, and
blinking follow that frame too. A constant `frame` holds still; a moving one replays at an offset.
It accepts common layer props.

```tsx
// Frame 1200 of the video, at quarter size
<Group scale={0.25} x={80} y={80}>
  <FreezeFrame frame={1200}>
    <World />
  </FreezeFrame>
</Group>
```

- Frozen children are always silent: `<Audio>`, the sound of a `<Video>`,
  and voiced `<Dialogue>` inside are not collected. For delayed playback with sound, use `<Sequence from>`.
- Portraits can be redrawn too. A `<CharacterView>` inside never takes over
  its ref, so the live `<Dialogue>`s keep driving the live view. A
  `<Dialogue>` inside drives only a view drawn inside the same
  `<FreezeFrame>`; pointing it at a view outside throws.
- Authored `id`s inside are prefixed with the freeze layer's id and `/`.
- `<ProjectTimeline />`, `<ProjectTrack />`, and `useProjectTrack()` throw
  inside: project layers are evaluated for the current frame only.

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

## TransitionSeries

Scenes back to back with transitions between them; the overlaps and the
total length are computed for you. Children are `<TransitionSeries.Sequence
durationInFrames>` scenes with an optional `<TransitionSeries.Transition
type durationInFrames from? easing?>` between two of them. Prefer it to a
`Series` with negative `offset`s plus hand-matched `Transition`s.

| `type` | What happens over `durationInFrames` | Options (function of the same name) |
| --- | --- | --- |
| `'cut'` | Nothing: no overlap, omit `durationInFrames` (same as no transition). | — |
| `'crossfade'` | The next scene fades in over the previous one, which stays opaque. | `crossfade()` |
| `'slide'` | The next scene pushes the previous one out, entering from `from`. | `slide()` |
| `'wipe'` | The next scene is revealed by a straight edge moving in from `from`. | `wipe()` |
| `'iris'` | The next scene opens out of a circle that grows past the farthest corner. | `iris({ center, direction: 'open' \| 'close', feather })` |
| `'clock-wipe'` | A hand sweeps once around a point, revealing the next scene. | `clockWipe({ center, startAngle, direction: 'clockwise' \| 'counterclockwise' })` |
| `'zoom'` | Both scenes zoom about a point while the next fades in. | `zoom({ direction: 'in' \| 'out', scale, center })` |
| `'dissolve'` | The next scene shows through square by square, in a seeded order. | `dissolve({ cellSize, seed })` |
| `'blur'` | The previous scene blurs away; the next comes into focus and fades in. | `blur({ amount })` |
| `'flip'` | The series turns over like a card about the axis `from` picks. | `flip({ shade })` |
| `'cube'` | Two faces of a cube turn a quarter turn, the next from `from`. | `cube({ shade })` |

A name uses the defaults: `center` `{ x: 0.5, y: 0.5 }` (fractions of the
series' size), iris `direction: 'open'` and `feather: 0` px, clock-wipe
`startAngle: 0` (12 o'clock, degrees clockwise) and `'clockwise'`, zoom
`'in'` and `scale: 1.5`, dissolve `cellSize: 16` px and `seed: 0`, blur
`amount: 32` px (0–64), flip and cube `shade: 0.4` (0–1). Call the function
(imported from `@celesta/transitions`) for other values:
`type={iris({ center: { x: 0.3, y: 0.5 }, feather: 40 })}`. Bad options
throw when the function is called.

`from` is `'left'` (default), `'right'`, `'top'`, or `'bottom'`; anything
else throws when the series renders. It is the edge `'slide'`, `'wipe'`,
and `'cube'` come from, and the axis `'flip'` turns about (left/right and
top/bottom look the same without perspective). `easing` shapes the progress
(default linear, e.g. `Easings.easeInOutCubic`). `flip` and `cube` are drawn
without perspective: faces narrow and darken (`shade`), and a cube is
scaled down mid-turn so its two faces always span the series.

### Size: a series in a frame

Transitions move and cut across the series' size: how far a slide pushes,
where a wipe ends, where an iris centres. It is `<TransitionSeries width
height>`, or else the enclosing `<SafeArea>`'s or `<Fit>`'s bounds
(`useLayoutBounds()`), or else the composition's size. For a series in a
small frame, position it with a `Group` and give it the frame's size (and a
`clip` if the scenes must not spill out of the frame while they slide):

```tsx
<Group x={260} y={400} clip={{ width: 560, height: 315 }}>
  <TransitionSeries width={560} height={315}>…</TransitionSeries>
</Group>
```

### Defining a transition

`defineTransition({ name, present })` makes a transition for `type`.
`present` gets `{ progress, frame, durationInFrames, from, width, height }`
for each overlapped frame (`progress` already eased) and returns
`{ enter?, exit? }`: `Group` props for each scene's group, such as `x`,
`scale`, `opacity`, `clip`, `blur`, `brightness`, `shader`, or `mask`. The
groups sit at the series' origin; a side left out is drawn as it is. The
built-in transitions other than `'cut'` are written the same way, and
`useTransitionSeriesScene()` reports a defined transition by its `name`.

```tsx
const blinds = defineTransition({
  name: 'blinds',
  present: ({ progress, width, height }) => ({
    enter: {
      mask: <>{Array.from({ length: 8 }, (_, i) => (
        <Rect key={i} y={(i * height) / 8} width={width} height={(height / 8) * progress} fill="#FFFFFF" />
      ))}</>,
    },
    exit: { brightness: 1 - 0.5 * progress },
  }),
});
<TransitionSeries.Transition type={blinds} durationInFrames={24} />
```

A mask or a shader in a style comes and goes with the transition without
changing the scene's layer ids. Custom WGSL shaders run in preview and
export only; masks, clips, blur, and color filters also run in
`@celesta/web`, so every built-in type does.

Timing rules:

- A transition of *n* frames starts the next scene *n* frames before the
  previous one ends, so the series is the scenes' lengths added up minus
  every transition. `computeTransitionSeries(items)` returns
  `{ sequences: [{ from, durationInFrames }], transitions: [{ from,
  durationInFrames }], durationInFrames }`: use it for the `<Composition>`.
  Items are the children's props in order: scenes `{ durationInFrames }`,
  transitions `{ type, durationInFrames? }` (scene items must not have a
  `type` key; `type` may be a name or a defined transition).
- It throws for: no scene, a transition first or last, two transitions in a
  row, an unknown `type`, a cut with frames, a scene or non-cut transition
  whose `durationInFrames` is not a positive integer, and a scene shorter
  than its transitions in and out together (a scene may spend all of its frames in
  transitions, but three scenes never show at once).
- Each scene is a `<Sequence>`: `useCurrentFrame()` is 0 on the first frame of
  its transition in, and `durationInFrames` includes both overlaps. It is
  unmounted after its transition out. The next scene draws above.
- Progress over *n* frames is 1/(n+1) … n/(n+1): every overlapped frame shows
  both scenes; the frames either side show one scene whole.
- Audio of overlapping scenes plays together at its own volume. For an
  audio cross-fade, give an `<Audio>` or `<Video>` placed directly in the scene
  `volume={useTransitionVolume(level)}`: `level`, ramping from/to 0 over the
  scene's transitions in and out. `useTransitionSeriesScene()` returns
  `{ index, enter, exit }` (`{ type, durationInFrames }` or `null`) for other
  timing, such as waiting for the scene to be fully on screen.

```tsx
const FADE = { type: 'crossfade', durationInFrames: 15 } as const;
const { durationInFrames } = computeTransitionSeries([
  { durationInFrames: 90 }, FADE, { durationInFrames: 240 },
]); // 90 + 240 - 15 = 315
// …
<Composition width={1920} height={1080} fps={30} durationInFrames={durationInFrames}>
  <TransitionSeries>
    <TransitionSeries.Sequence durationInFrames={90}><Intro /></TransitionSeries.Sequence>
    <TransitionSeries.Transition {...FADE} />
    <TransitionSeries.Sequence durationInFrames={240}><Body /></TransitionSeries.Sequence>
  </TransitionSeries>
</Composition>

function Body() {
  // The scene's sound fades in under the cross-fade.
  return <Audio src="./body.wav" volume={useTransitionVolume(0.8)} />;
}
```

`packages/cli/examples/with-transition-types.tsx` plays every type in turn,
and `packages/cli/examples/with-custom-transition.tsx` defines one with a
mask and one with a shader.

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
`{ framesPerBeat, beat, bar, beatInBar, progress, barProgress, pulse }`
(`beat` and `bar` count up; the others repeat every beat or bar, which keeps
a [loop](loops.md) seamless).
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
