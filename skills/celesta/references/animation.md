# Time and animation

How a React composition moves: frame hooks, `interpolate`, easings,
springs, `Sequence` scenes, transitions, and the motion helpers that save
hand-written timing math. Imported from `@celesta.studio/react`, except `Transition`,
`TransitionSeries`, `computeTransitionSeries`, `useTransitionSeriesScene`, and
`useTransitionVolume` (`@celesta.studio/transitions`), `Camera` (`@celesta.studio/layout`),
`FrameChrome` and `Motion` (`@celesta.studio/components`), and `TextReveal`, `useTypewriter`, and
`useCountUp` (`@celesta.studio/text`).

Every frame must be a pure function of the frame number: frames are rendered
out of order when scrubbing and exporting. Never use `Math.random()`,
`Date.now()`, timers, `useEffect`, or state that accumulates across renders.
For randomness and noise, use [`@celesta.studio/math`](math.md).

## Contents

- [Pick a helper](#pick-a-helper)
- [Hooks](#hooks)
- [interpolate](#interpolate)
- [interpolateColor](#interpolatecolor)
- [Easings](#easings)
- [spring](#spring)
- [progress](#progress)
- [Sequence](#sequence)
- [Loop](#loop)
- [FreezeFrame](#freezeframe)
- [MotionBlur and Trail](#motionblur-and-trail)
- [Series and computeSeries](#series-and-computeseries)
- [Scenes and useScene](#scenes-and-usescene)
- [Chapters in the preview](#chapters-in-the-preview)
- [FrameChrome](#framechrome)
- [TransitionSeries](#transitionseries)
- [Stagger](#stagger)
- [Motion](#motion)
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
| Named chapters, with a HUD that shows the current one and its progress | `defineScenes` + `Scenes` + `useScene` |
| Chapters the editor preview lists and seeks to | `<Composition chapters>` + `listChapters` |
| Corner brackets, a title, the timecode, the chapter, and a progress bar around the picture | `FrameChrome` |
| Scenes joined by a cut, cross-fade, slide, or wipe | `TransitionSeries` + `computeTransitionSeries` |
| A background animation or a short BGM repeated | `Loop` |
| A past frame redrawn in place (rewind, flashback, thumbnails) | `FreezeFrame` |
| Fast motion (a dash, a whip pan, a spinning wheel) smeared like a camera sees it | `MotionBlur` |
| Ghost copies following a moving object | `Trail` |
| A voiced script back to back | `planDialogue` + `DialogueSeries` ([dialogue.md](dialogue.md#timing-a-script-from-its-voices)) |
| Items entering one after another | `Stagger` (or `progress(frame, i * each, …)`) |
| One element that fades, rises, or scales in, holds, and leaves | `Motion` |
| Fade/slide/scale of one element at only a scene's start or only its end | `Transition` |
| Motion on the music's beat | `useBeat` |
| A video whose last frame runs into its first (Shorts, TikTok) | periodic motion and `loopNoise` ([loops.md](loops.md)) |
| Something that changes at given frames (captions, camera stops, chart callouts) | `useCue` |
| A push-in, pan, or shake over a whole scene | `Camera` |
| A line being drawn | `Polyline progress` ([react-core.md](react-core.md#path-line-polyline)) |
| An arrow growing toward its target | `Arrow` with animated `x2`/`y2` ([react-core.md](react-core.md#circle-ellipse-arrow)) |
| Text revealed line by line, typed, or a counting number | `TextReveal`, `useTypewriter`, `useCountUp` ([text.md](text.md#text-motion)); `Counter`, `StatCard`, and `BarChart` from `@celesta.studio/components` ([react-core.md](react-core.md#counter-statcard-barchart-ranking-numbers-and-bar-charts)) |
| Code being typed | `@celesta.studio/code` with `useTypewriter` ([code.md](code.md)) |

## Hooks

| Hook | Returns |
| --- | --- |
| `useCurrentFrame()` | Integer frame, local to the innermost `<Sequence>` (directly inside a `<FreezeFrame>`, its `frame`, rounded). Inside a `<MotionBlur>` or a `<Trail>` copy, the exact fractional frame of the moment drawn, such as `12.25`. |
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
`outputRange` (at least 2). `input` and every range entry must be finite: a
`NaN` (say, from a misspelled field) throws rather than returning `NaN`,
which `'clamp'` would not catch. **The default extrapolation is `extend`**, so
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
`overshootClamping` (false). Frames before `delay` return `from`. `frame`
must be finite and `fps` finite and above 0, or it throws.

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
parent's end). `from` defaults to 0 and must be finite; `durationInFrames`
must be at least 0, where `0` is an empty window and omitted or `Infinity`
runs to the parent's end. `NaN`, a negative duration, or a non-number throws
(see errors.md), so a misspelled field cannot make a scene cover every later
one. Inside, frames and media restart at 0. Sequences nest and
accept common layer props, plus `lang` (default text language for the
children) and `fontFallback` (`'loaded'` tries `<Font>` fonts before system
fonts for the children's text; see text.md).

Children are unmounted outside this window, so their component code and
hooks do not run. Entering the window again mounts them afresh, resetting
their local React state. Declare fonts or assets needed throughout the
composition in `<Assets>` outside a sequence.

For scenes in a row, prefer `Series` (next) to computing `from` by hand.

## Loop

`<Loop durationInFrames from? times?>` repeats its children: every
`durationInFrames` frames (positive, fractions allowed) from `from`, their
clock starts again at 0, exactly as if each cycle were a `<Sequence>` of its
own. `useCurrentFrame()`, `<Video>`, `<Audio>`, and nested sequences restart
together; `useVideoConfig().durationInFrames` is the cycle's length. Without
`times` it repeats until the parent's end; a fractional `times` (`2.5`) cuts
the last cycle short. It accepts common layer props, `lang`, and
`fontFallback`.

```tsx
<Loop durationInFrames={120} times={3}>
  <Audio src="./phrase.wav" />
</Loop>
```

- Each cycle's audio is a clip of its own, starting at the cycle and cut
  where the loop (or its parent) ends.
- Only the current cycle is mounted, so React state inside resets each
  cycle. Keep state that must survive outside the loop.
- For a seamless whole-video loop, see [loops.md](loops.md); a `<Loop>`
  without `times` that reaches the end keeps cycling past it with
  `inspect.mjs --loop`.

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

- `frame` may lie at or past the end of the composition the element is in:
  the children are drawn as if the video went on, so a short composition
  (a 1-frame poster, say) can draw a frame of a longer video. A `<Sequence>`
  inside that ends with the composition (no `durationInFrames`) stays open,
  one with its own `durationInFrames` keeps its own end, and
  `useVideoConfig()` still reports the composition's real duration (inside
  such an open `<Sequence>`, the frames from its start to the composition's
  end, or 0 when it starts past the end). Children
  that read it there can differ from a longer composition's; give the
  `<Sequence>` its own `durationInFrames` to keep them the same.
- Frozen children are always silent: `<Audio>`, the sound of a `<Video>`,
  and voiced `<Dialogue>` inside are not collected. For delayed playback with sound, use `<Sequence from>`.
- Portraits can be redrawn too. A `<CharacterView>` inside never takes over
  its ref, so the live `<Dialogue>`s keep driving the live view. A
  `<Dialogue>` inside drives only a view drawn inside the same
  `<FreezeFrame>`; pointing it at a view outside throws.
- Authored `id`s inside are prefixed with the freeze layer's id and `/`.
- `<ProjectTimeline />`, `<ProjectTrack />`, and `useProjectTrack()` throw
  inside: project layers are evaluated for the current frame only.

## MotionBlur and Trail

`<MotionBlur shutterAngle samples>` draws its children at `samples` moments
(default 8) while a shutter `shutterAngle` degrees of a frame wide (default
180, from 0 to 720) is open, centred on the frame, and shows their average.
Moments stay within the enclosing window's first and last frames. Each moment
draws the children again, so the blur costs about `samples` times the
children: wrap only what moves fast. Raise `samples` when very fast motion
shows steps.

Inside, `useCurrentFrame()` is each moment's exact frame (`12.25`), so
components that animate from it move between moments; floor it before
indexing arrays. Props computed outside are the same in every moment, so
pass a function child to compute them per moment:

```tsx
<MotionBlur>
  {(frame) => (
    <Camera x={interpolate(frame, [0, 30], [0, 3000], { extrapolateRight: 'clamp' })}>
      <World />
    </Camera>
  )}
</MotionBlur>
```

`<Trail copies lagInFrames trailOpacity>` (defaults 4, 1, 0.5) draws
`copies` earlier copies beneath its children, each `lagInFrames` older; the
newest is at `trailOpacity` and each older one fades by the same step. A
copy fades like a `<Group>` with that opacity (overlapping layers in it show
through each other). Copies from before the enclosing window are not drawn.

Both:

- Accept common layer props, applied to the result (for `MotionBlur`, to
  the average).
- Give the same result for a frame whatever was drawn before; the editor
  preview draws what the export does.
- Keep `<Video>`, animated images, lip sync, and blinking at the frame's
  moment in every copy. Audio and subtitles inside play once (from the
  moment nearest the frame, or `Trail`'s live children).
- Draw each moment as a copy, like `<FreezeFrame>`: a `<CharacterView>`
  inside never takes its ref, so its `<Dialogue>` must be inside too.
  `<ProjectTimeline />` and `<ProjectTrack />` throw inside. `<Static>`
  cannot contain either.
- `samples={1}` or `shutterAngle={0}` draws a plain group.

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

## Scenes and useScene

Chapters declared once, by name, and read from anywhere. `defineScenes(entries)`
places `{ id, name?, durationInFrames, offset?, Scene? }` entries back to back
like `Series` (`offset` overlaps or gaps; `durationInFrames` may be 0, which
keeps the entry in the list but never plays it; ids must be unique) and
returns `{ scenes, durationInFrames }`, each scene with its `index`, `name`
(defaults to `id`) and `from`. Extra fields stay on the entries.
`<Scenes scenes={list} />` plays each `Scene` in a `<Sequence>`.

`useScene(list)` (or `sceneAt(list, frame)`) returns `null` before the first
scene and from the list's end on, else `{ scene, index, id, name, from,
durationInFrames, frame, progress, previous, next, scenes,
totalDurationInFrames }`: `frame` counts from the scene's start, `progress`
runs 0→1 and holds 1 in a gap. Where scenes overlap, the later one is
current. Inside `<Scenes>`, `useScene()` (no argument) reads the list on the
`<Scenes>`' own clock, so nested `<Sequence>`s do not shift it; elsewhere
(a HUD) the caller must sit on the same timeline as `<Scenes>`. Both give
the same result on the same frame. Inside a `<Sequence>` that is not under
the `<Scenes>` (the list only names chapters laid out with `<Sequence>` or
`<TransitionSeries>`), `useScene(list, { clock: 'root' })` reads the list on
the composition's frame instead of the caller's, which would restart at 0.

```tsx
const SCENES = defineScenes([
  { id: 'intro', name: 'COLD OPEN', durationInFrames: 120, Scene: Intro },
  { id: 'words', name: 'MANIFESTO', durationInFrames: 240, Scene: Words },
]);

function Hud() {
  const scene = useScene(SCENES);
  if (!scene) return null;
  return (
    <Group>
      <Text x={60} y={60}>{scene.name}</Text>
      <Rect x={60} y={90} width={300 * scene.progress} height={4} fill="#ffffff" />
    </Group>
  );
}

<Composition width={1920} height={1080} fps={30} durationInFrames={SCENES.durationInFrames}>
  <Scenes scenes={SCENES} />
  <Hud />
</Composition>
```

## Chapters in the preview

`<Composition chapters={source}>` lists named spans in the editor preview: a
Chapters row above the timeline's tracks and a Chapters list in the React
Preview tab, the current one highlighted, a click seeking to its start.
`source` is a `defineScenes` list, a `planDialogue` plan (its `scenes`), or
`{ id, name?, from, durationInFrames }[]` in the composition's frames. Read
once at load (and on every reload) like `width`/`fps`; nothing is inferred
from the tree, and a composition without it previews as before. Chapters are
sorted by `from` and cut at the end; length 0 is a seekable mark; a missing
or duplicate `id`, a negative or fractional frame, or a chapter starting at
or after the end fails the load.

The list is flat, on the composition's frames. `listChapters(source,
{ from })` checks a source and moves it, so a list played inside one scene
joins with that scene's start; overlapping chapters stack on lanes in the
timeline, and the later-starting one is current.

```tsx
const PARTS = defineScenes([
  { id: 'setup', durationInFrames: 45 },
  { id: 'result', durationInFrames: 45 },
]);
const chapters = [...listChapters(SCENES), ...listChapters(PARTS, { from: SCENES.scenes[1].from })];

<Composition width={1920} height={1080} fps={30} durationInFrames={SCENES.durationInFrames} chapters={chapters}>
  <Scenes scenes={SCENES} />
</Composition>

// A dialogue plan, titled:
<Composition ... chapters={plan.scenes.map((scene) => ({ ...scene, name: TITLES[scene.id] }))}>
```

## FrameChrome

Imported from `@celesta.studio/components`. The overlay a film keeps around the picture, laid out inside `margin` from
the canvas size, so it holds its places on landscape, vertical, and square
canvases. Only what is turned on is drawn:

| Prop | Draws |
| --- | --- |
| `corners` | an L bracket in each corner (`cornerLength` 24, `strokeWidth` 2) |
| `title`, `subtitle` | top left: the title at `titleWeight` (700), then the subtitle at `fontWeight` and `subtitleOpacity` (0.6) |
| `timecode` | top right: `frameToTimecode`; `timecodeHours={false}` drops the hours |
| `chapters` + `scenes` | bottom left: the current scene's `name`, or `formatChapter(state)` |
| `caption` | bottom right: any text |
| `progress` | a bar `progressInset` (half the margin) above the bottom edge, from `margin` to `margin`; with `scenes`, one segment per scene, as wide as it is long (`segments={false}` keeps one bar) |

Labels sit beside the brackets (`gap` 14), centred on the arm;
`positions={{ timecode: 'bottom-right' }}` moves one, and two in one corner
throw. `color` and `progressColor` take hex or `<Theme>` color names,
`fontFamily` a `<Theme>` font name. The bar is empty on the first frame and
full on frame `durationInFrames - 1` (defaults to the length of `scenes`,
else of the composition), and scene segments fill as one bar, so it does not
restart at a cut. `trackOpacity` (0.25) shows the unfilled part.

`blendMode="difference"` keeps the overlay readable over dark and light
scenes; `progressBlendMode="normal"` keeps a colored bar out of the blend.
`scenes` is read on the caller's clock: place `<FrameChrome>` beside the
`<Scenes>`, on the same timeline. To hide it over some scenes, wrap it:
`useScene(SCENES)?.id === 'logo' ? null : <FrameChrome … />`.

```tsx
import { FrameChrome } from '@celesta.studio/components';

<Theme fonts={{ mono: 'JetBrains Mono' }} colors={{ paper: '#f4f1ea', accent: '#ff4d1f' }}>
  <Scenes scenes={SCENES} />
  <FrameChrome margin={56} corners timecode chapters progress scenes={SCENES}
    title="CELESTA" subtitle="/ FEATURE TOUR" caption="1920×1080 · 30 FPS"
    color="paper" fontFamily="mono" blendMode="difference" />
</Theme>
```

Runnable: [`packages/cli/examples/with-frame-chrome.tsx`](../../../packages/cli/examples/with-frame-chrome.tsx).

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
(imported from `@celesta.studio/transitions`) for other values:
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
`@celesta.studio/web`, so every built-in type does.

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

## Motion

`<Motion enter? exit? from? durationInFrames?>` from `@celesta.studio/components`
eases one element in at the start of its window, holds it exactly as
written, and eases it out at the end. It is a `Group`: its own `x`, `y`,
`opacity`, `scale`, and the other layer props are what it holds, and the
effects multiply its opacity and scale and offset its position. The
children's props, clock, and media are left alone.

```tsx
import { Motion } from '@celesta.studio/components';

<Motion enter="fade-up" exit={{ type: 'fade-down', durationInFrames: 10 }}>
  <Text x={640} y={360} anchorX={0.5}>Enter, hold, exit</Text>
</Motion>
```

| Prop | Notes |
| --- | --- |
| `enter`, `exit` | A preset name, or `{ type, durationInFrames?, distance?, scaleFrom?, easing? }`. Omitted, that side cuts. |
| `from`, `durationInFrames` | The window, as on `<Sequence>`: the children mount, play media, and count frames from `from`. Omit both to use the enclosing sequence's window and clock as they are. |

| Preset | Effect |
| --- | --- |
| `'fade'` | Opacity only. |
| `'fade-up'`, `'fade-down'`, `'fade-left'`, `'fade-right'` | Fades while moving `distance` px (default 64) in that direction: an entrance `fade-up` rises into place, an exit `fade-up` rises away. |
| `'scale'` | Fades while scaling from `scaleFrom` (default 0.8) to 1, or back on exit. |

- **Timing.** `durationInFrames` of an effect defaults to half a second; 0
  or 1 is a cut. Like `<Transition>`, an n-frame entrance is hidden on the
  window's first frame and at rest on its nth; an exit is at rest n frames
  before the window ends and hidden on its last frame. `easing` defaults to
  `Easings.easeOutCubic`; an exit plays it backwards (as
  `<Transition direction="out">` does), so the element settles in and speeds
  away.
- **Short windows.** When the entrance and exit do not fit the window, both
  shrink in proportion until they meet on one frame, so the element is at
  rest on at least one frame; a one-frame window shows it at rest.
- **Scale pivot.** `scale` turns about the group's origin. Place the
  `Motion` with `x`/`y` and its children around 0, or set `originX`/`originY`.
- **Motion vs TransitionSeries.** `Motion` animates one element inside a
  scene. To join whole scenes with a cut, cross-fade, slide, or wipe, use
  [TransitionSeries](#transitionseries).

Runnable: [`packages/cli/examples/with-motion.tsx`](../../../packages/cli/examples/with-motion.tsx).

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

For an entrance and an exit together, use [Motion](#motion) rather than
nesting two transitions. `applyTransition(props, { type, slideFrom?,
distance?, scaleFrom? }, progress)` returns the group props a transition
draws at `progress` (0 hidden, 1 at rest), for timing of your own.

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
largest drift in world pixels, smoothed by `@celesta.studio/math`'s `noise()`.
Animate the props from the frame for pans and zooms. To blur a fast pan,
wrap the camera in a `<MotionBlur>` with a function child that computes the
props from the frame it is given ([MotionBlur and Trail](#motionblur-and-trail)).

## Timecodes

`timecodeToFrame(timecode, fps)`: `'01:02.500'`, `'1:02:03'`, or `'12.5'` →
frame number. Handy for syncing to timestamps the user gives you.
`frameToTimecode(frame, fps)` goes the other way, formatting `HH:MM:SS:FF`
for an on-screen clock; `frameToTimecode(frame, fps, { hours: false })`
gives `MM:SS:FF`, the minutes counting past 59. For a whole HUD around the
picture, see [FrameChrome](#framechrome).
