# @celesta/react API reference

Everything is imported from `@celesta/react`. The runtime bundled with
Celesta provides both `react` (18.x) and `@celesta/react`; do not install
them from npm.

## Contents

- [Entry file shape](#entry-file-shape)
- [Common layer props](#common-layer-props)
- [Layers](#layers): Composition, Rect, Text, Group, Image, Video, Audio, Font, Assets
- [Text styles and fonts](#text-styles-and-fonts)
- [Time and animation](#time-and-animation): hooks, interpolate, Easings, spring, Sequence, Transition, timecodeToFrame
- [Layout helpers](#layout-helpers): Center, SafeArea, Stack, Grid, Fit
- [Media helpers](#media-helpers): preloadMedia, mediaDurationInFrames
- [Project data](#project-data): ProjectProvider, useProjectProperty, defineProjectProperties, ProjectTimeline, ProjectTrack, registerComponent
- [Preview-only debug guides](#preview-only-debug-guides)
- [Keyframe values](#keyframe-values)

## Entry file shape

```tsx
// Optional. Runs once, before the first frame, in preview and export.
export async function prepare(): Promise<void> { /* fetch, load, probe */ }

// Required. Returns exactly one <Composition>.
export default function Root() {
  return <Composition width={1920} height={1080} fps={30} durationInFrames={300}>…</Composition>;
}
```

- `durationInFrames / fps` is the length in seconds. Frames run from `0` to
  `durationInFrames - 1`.
- `durationInFrames` may be computed from values that `prepare()` stored
  (for example, a voice or clip length).
- Module-level code runs once. `registerComponent()` and
  `defineProjectProperties()` must be called at module level.
- `console.log` output goes to the terminal (stderr), which is useful when
  running `scripts/inspect.mjs`.

## Common layer props

Every visual layer (`Rect`, `Text`, `Group`, `Image`, `Video`,
`CharacterView`, `Dialogue`, `Sequence`, layout helpers, `Transition`)
accepts:

| Prop | Default | Meaning |
| --- | --- | --- |
| `x`, `y` | `0` | Position of the anchor point in the parent's coordinates, in pixels. Origin is the top-left of the canvas; y grows downward. |
| `anchorX`, `anchorY` | `0` | Normalized pivot: `0` left/top, `0.5` center, `1` right/bottom. Positioning, rotation, and scale all use it. |
| `rotation` | `0` | Degrees, clockwise. |
| `scale` | `1` | Uniform scale. `scaleX`/`scaleY` override one axis. |
| `opacity` | `1` | `0`–`1`; multiplies down through groups. |
| `id` | generated | Optional stable layer id. |

To center something at a point, set `anchorX={0.5} anchorY={0.5}` and put
the point in `x`/`y`.

## Layers

### `<Composition>`

`width`, `height`, `fps`, `durationInFrames`: all required positive
integers. Use even `width`/`height` for MP4 export. Exactly one, at the root.
These props are read once when the entry loads, so they must not depend on
the current frame.

### `<Rect>`

| Prop | Notes |
| --- | --- |
| `width`, `height` | Required, pixels. |
| `fill` | Hex string (`"#RRGGBB"` / `"#RRGGBBAA"`). Omit for no fill. |
| `stroke`, `strokeWidth` | Inner stroke; needs both to be visible. |
| `cornerRadius` | Pixels. A square with `cornerRadius = size / 2` is a circle. |

A full-canvas background: `<Rect width={1920} height={1080} fill="#101018" />`
as the first child.

### `<Text>`

| Prop | Notes |
| --- | --- |
| children | Strings or numbers only (arrays of them are joined). Use template literals to combine values. `\n` breaks a line. |
| `style` | A `TextStyle`, see below. |
| `maxWidth` | Wrap at word boundaries within this width; `style.align` positions each line inside it. |

Single-line text is anchored by its visible glyph bounds, so
`anchorY={0.5}` centers the letters themselves.

### `<Group>`

No size or appearance of its own. Children are positioned relative to the
group's `x`/`y`, and its rotation, scale, and opacity apply to all of them.

### `<Image>`

`src` (path or URL). Drawn at its natural pixel size; resize with `scale`
or a `<Fit>`. PNG, JPEG, WebP, and PNM are supported.

### `<Video>`

| Prop | Notes |
| --- | --- |
| `src` | Path or URL. Drawn at natural pixel size. |
| `startFrom` | Seconds into the file at the enclosing sequence's frame 0. Default `0`. |
| `playbackRate` | Static number only. Default `1`. |

**A React `<Video>` is picture only; it makes no sound.** To hear the clip's
soundtrack, add an `<Audio>` with the same `src`, `startFrom`, and
`playbackRate` next to it, inside the same `<Sequence>`.

### `<Audio>`

| Prop | Notes |
| --- | --- |
| `src` | Path or URL. Any file FFmpeg can decode, including a video file's audio. |
| `startFrom` | Seconds into the file. Default `0`. |
| `playbackRate` | Number or [keyframes](#keyframe-values). Default `1`. |
| `volume` | Linear gain, number or [keyframes](#keyframe-values). Default `1`. |
| `muted` | Default `false`. |

Bare, an `<Audio>` spans the whole composition. Inside a `<Sequence>` it
starts when the sequence starts and stops when it ends. An `<Audio>` behind a
conditional plays only on frames where it is rendered.

### `<Font>` and `<Assets>`

```tsx
<Assets>
  <Font src="./fonts/MPLUSRounded1c-Bold.ttf" />
  <Font src="https://fonts.googleapis.com/css2?family=M+PLUS+Rounded+1c:wght@400;700" />
</Assets>
```

- `<Font src>` loads TTF, OTF, WOFF, WOFF2, or a web-font CSS stylesheet
  (every `@font-face` in it is loaded). Refer to it by the **family name
  stored inside the font**, not the file name.
- `<Assets>` holds declarations that draw nothing: `Font`, `Character`, and
  `Image`/`Video`/`Audio` used only to create refs.
- Refs: `const logo = React.createRef<AssetReference>()`, then
  `<Assets><Image ref={logo} src="./logo.png" /></Assets>` and
  `<Image src={logo} />` elsewhere. Plain string paths are usually simpler.

## Text styles and fonts

```ts
type TextStyle = {
  fontFamily?: string;     // installed family, or one loaded with <Font>
  fontSize?: number;       // px, > 0
  fontWeight?: number;     // 100–900
  fill?: { type: 'solid'; color: string };                        // hex
  stroke?: { paint: { type: 'solid'; color: string }; width: number }; // outline
  align?: 'left' | 'center' | 'right';
  lineHeight?: number;     // px, > 0
};
```

- An unknown `fontFamily` silently falls back to another font. For
  reproducible output, ship the font file next to the entry and load it with
  `<Font>`. Characters the font lacks (emoji) fall back per glyph.
- With Google Fonts, list every weight you use in the URL.

## Time and animation

### Hooks

| Hook | Returns |
| --- | --- |
| `useCurrentFrame()` | Integer frame, local to the innermost `<Sequence>`. |
| `useCurrentTime()` | Exact `{ value, timescale }` time, local to the innermost `<Sequence>`. |
| `useVideoConfig()` | `{ width, height, fps, durationInFrames }`; inside a `Sequence`, `durationInFrames` is the sequence's. |
| `useIsPreview()` | `true` only in the Celesta app preview, `false` in exports. |

Hooks work in any component, including `Root`. However, `<Composition>`'s
own props are read once, when the entry is loaded: compute them from
constants or `prepare()` results, never from `useCurrentFrame()` or
`useVideoConfig()`.

### `interpolate(input, inputRange, outputRange, options?)`

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

### `Easings`

`linear`, `easeIn`, `easeOut`, `easeInOut` (quadratic), and
`easeIn…`/`easeOut…`/`easeInOut…` for `Sine`, `Quad`, `Cubic`, `Quart`,
`Quint`, `Expo`, `Circ`, `Back`, `Elastic`, `Bounce` (for example
`Easings.easeOutBack`). Any `(t: number) => number` works too.

### `spring({ frame, fps, config?, from?, to?, delay?, durationInFrames? })`

Damped spring from `from` (default 0) to `to` (default 1), which may
overshoot. `config`: `stiffness` (100), `damping` (10), `mass` (1),
`overshootClamping` (false). Frames before `delay` return `from`.

```tsx
const frame = useCurrentFrame();
const { fps } = useVideoConfig();
const scale = spring({ frame, fps, delay: 10, config: { damping: 12 } });
```

### `<Sequence from? durationInFrames?>`

Shows its children only during `[from, from + durationInFrames)` in the
parent's frames (default: until the parent's end). Inside, frames and media
restart at 0. Sequences nest, and accept common layer props. Use them to
lay out scenes back to back:

```tsx
const scenes = [{ C: Intro, len: 90 }, { C: Body, len: 240 }, { C: Outro, len: 60 }];
let at = 0;
return <Composition width={1920} height={1080} fps={30} durationInFrames={390}>
  {scenes.map(({ C, len }, i) => {
    const from = at; at += len;
    return <Sequence key={i} from={from} durationInFrames={len}><C /></Sequence>;
  })}
</Composition>;
```

### `<Transition type durationInFrames …>`

Animates its children at the start (`direction="in"`, default) or end
(`direction="out"`) of the enclosing sequence (or composition).

| Prop | Notes |
| --- | --- |
| `type` | `'fade'`, `'slide'`, or `'scale'`. |
| `durationInFrames` | Positive integer. |
| `direction` | `'in'` (default) or `'out'`. |
| `slideFrom` | `'left'` (default), `'right'`, `'top'`, `'bottom'`. |
| `distance` | Slide distance in px, default `64`. |
| `scaleFrom` | Starting scale, default `0.8`. |
| `easing` | `(t) => number`, for example `Easings.easeOutCubic`. |

Nest transitions to combine an entrance and an exit.

### `timecodeToFrame(timecode, fps)`

`'01:02.500'`, `'1:02:03'`, or `'12.5'` → frame number. Handy for syncing to
timestamps the user gives you.

## Layout helpers

Helpers position child **origins**; they do not measure what children draw.
Anchor children at `0.5` to center them on those points.

| Component | Props | Behavior |
| --- | --- | --- |
| `Center` | common props | Moves the origin to the center of the canvas or the enclosing `SafeArea`/`Fit`. `x`/`y` offset from there. |
| `SafeArea` | `padding`: number or `{ top, right, bottom, left }` | Insets children; nested helpers see the smaller area. |
| `Stack` | `spacing` (required), `direction`: `'vertical'` (default) or `'horizontal'` | Child *i* is placed at `i * spacing`. |
| `Grid` | `columns`, `columnWidth`, `rowHeight`, `columnGap?`, `rowGap?` | Fills cells row by row; each child's origin is its cell's top-left. |
| `Fit` | `sourceWidth`, `sourceHeight`, `mode`: `'contain'` (default) or `'cover'` | Scales content designed at the source size to the current area and centers it. |

`useLayoutBounds()` returns the current area's `{ width, height }`.

```tsx
<SafeArea padding={96}>
  <Center>
    <Stack spacing={96} y={-96}>
      {['Write', 'Preview', 'Export'].map((s) => (
        <Text key={s} anchorX={0.5} anchorY={0.5} style={{ fontSize: 64 }}>{s}</Text>
      ))}
    </Stack>
  </Center>
</SafeArea>
```

To show a 1280×720 video full-screen in a 1920×1080 composition:
`<Fit sourceWidth={1280} sourceHeight={720}><Video src="./clip.mp4" /></Fit>`.

## Media helpers

Call these in `prepare()` only.

```tsx
let clipFrames = 150; // fallback
export async function prepare() {
  const info = await preloadMedia('./media/walk.mp4');
  clipFrames = mediaDurationInFrames(info, 30) ?? clipFrames;
}
```

`preloadMedia(src)` resolves to
`{ src, durationSeconds?, video?: { width, height, codec?, frameRate?, durationSeconds? }, audio: [{ codec?, sampleRate?, channels?, durationSeconds? }] }`.
`mediaDurationInFrames(info, fps)` rounds up to whole frames.

For anything else asynchronous (fetching JSON, reading files with
`node:fs`), use `prepare()` the same way and keep a fallback so the scene
still renders offline.

## Project data

### Read values from a `.celesta.json`

```tsx
import type { Project } from '@celesta/react';
import projectFile from './project.celesta.json';
const project = projectFile as Project;

function Title() {
  const title = useProjectProperty('title', 'Chapter 1'); // key, fallback
  return <Text style={{ fontSize: 96 }}>{title}</Text>;
}

export default function Root() {
  return <Composition width={1920} height={1080} fps={30} durationInFrames={90}>
    <ProjectProvider project={project}><Title /></ProjectProvider>
  </Composition>;
}
```

`useProject()` returns the whole project. `loadProject(path)` and
`loadProjectFromString(json)` also create a `Project`.

### `defineProjectProperties(schema)`

Declares the Inspector fields for `properties` (module level):

```ts
defineProjectProperties({
  title:  { type: 'string',  label: 'Title',  defaultValue: 'Celesta' },
  accent: { type: 'color',   label: 'Accent', defaultValue: '#ff8800' },
  size:   { type: 'number',  label: 'Size',   defaultValue: 48, min: 8, max: 200, step: 2 },
  sub:    { type: 'boolean', label: 'Subtitle', defaultValue: false },
  weight: { type: 'select',  label: 'Weight', defaultValue: 'bold', options: ['bold', 'light'] },
});
```

### Draw a JSON timeline inside React

- `<ProjectTimeline />` draws the companion project's whole timeline;
  `<ProjectTrack id="titles" />` draws one track; `useProjectTrack(id)`
  returns its evaluated layers.
- These only work when the entry is exported with
  `--react entry.tsx --project project.celesta.json`. Without a companion
  project they throw.

### `registerComponent(name, Component, schema?)`

Lets a JSON item `{ "type": "component", "component": "LowerThird", "props": { … } }`
render a React component through `<ProjectTimeline />`. Call it at module
level. Props must be plain JSON values; declare them with a `type` alias,
not an `interface` (interfaces do not satisfy the JSON constraint). The
item's `range`, `transform`, and `opacity` still come from the JSON.

```tsx
type LowerThirdProps = { name: string; role: string };
function LowerThird({ name, role }: LowerThirdProps) {
  return <Text style={{ fontSize: 48 }}>{`${name} · ${role}`}</Text>;
}
registerComponent<LowerThirdProps>('LowerThird', LowerThird, {
  name: { type: 'string', defaultValue: 'Mira' },
  role: { type: 'string', defaultValue: 'Host' },
});
```

## Preview-only debug guides

`<DebugOverlay safeArea? color? showCenter? showFrame? />` draws the safe
area, center cross, and frame counter. `<DebugBounds width height label? color? showOrigin?>`
wraps children and outlines a box. Both render only in the app preview
(never in exports), so they are safe to leave in.

## Keyframe values

`Audio` `volume`/`playbackRate` and `Dialogue` `volume`/`playbackRate` accept
a number or keyframes. Keyframe times are **seconds from the start of the
enclosing sequence**, written as rationals:

```tsx
volume={{
  type: 'keyframes',
  keyframes: [
    { time: { value: 0, timescale: 1 }, value: 0 },
    { time: { value: 2, timescale: 1 }, value: 0.8, easing: 'ease-out' },
  ],
}}
```

Keyframe `easing` uses the kebab-case names listed in
[project-json.md](project-json.md#easing-names). Visual props (`x`,
`opacity`, …) do not take keyframes in React; compute them from the frame
with `interpolate`.
