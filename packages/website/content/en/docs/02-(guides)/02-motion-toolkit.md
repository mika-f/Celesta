These helpers cover the patterns that come up in almost every video: scenes in a row (with transitions between them), lists that cascade in, and cuts on the beat. Each is built on `Sequence`, `Group`, and the frame, so everything stays scrubbable and deterministic. Text effects, the camera, and line drawing are on [the next page](/en/docs/text-camera-lines/); seeded randomness and noise are in [`@celesta.studio/math`](/en/docs/math/).

## Scenes back to back

`<Series>` plays its `<Series.Sequence>` children one after another, so you write each scene’s length instead of its start frame. A negative `offset` overlaps a scene with the one before it. `computeSeries()` does the same arithmetic without rendering, which is handy for the composition’s total length:

```tsx title="Three scenes, sized from their lengths"
const SCENES = [
  { name: 'intro', durationInFrames: 90, Scene: Intro },
  { name: 'body', durationInFrames: 240, Scene: Body },
  { name: 'outro', durationInFrames: 60, Scene: Outro },
];
const { durationInFrames } = computeSeries(SCENES);

export default function Root() {
  return (
    <Composition width={1920} height={1080} fps={30} durationInFrames={durationInFrames}>
      <Series>
        {SCENES.map(({ name, durationInFrames, Scene }) => (
          <Series.Sequence key={name} durationInFrames={durationInFrames}>
            <Scene />
          </Series.Sequence>
        ))}
      </Series>
    </Composition>
  );
}
```

## Named chapters

When a HUD or a chapter list needs to know where the video is, declare the scenes once with `defineScenes()`: each entry has an `id`, an optional `name`, a length, and the `Scene` to draw. `<Scenes>` plays them like `<Series>`, and `useScene(list)` returns the current one — its `name`, the `frame` since it started, its `progress` from 0 to 1, and the whole list — or `null` before the first and after the last. Inside a scene, `useScene()` with no argument gives the same answer, even under nested `<Sequence>`s. `sceneAt(list, frame)` does the same outside a component.

```tsx title="A HUD that follows the chapters"
const SCENES = defineScenes([
  { id: 'intro', name: 'COLD OPEN', durationInFrames: 90, Scene: Intro },
  { id: 'body', name: 'MANIFESTO', durationInFrames: 240, Scene: Body },
]);

function Hud() {
  const scene = useScene(SCENES);
  if (!scene) return null;
  return <Text x={60} y={60}>{`${scene.name} ${Math.round(scene.progress * 100)}%`}</Text>;
}

export default function Root() {
  return (
    <Composition width={1920} height={1080} fps={30} durationInFrames={SCENES.durationInFrames}>
      <Scenes scenes={SCENES} />
      <Hud />
    </Composition>
  );
}
```

Outside a `<Scenes>`, `useScene(list)` reads the list on the caller's clock, which a `<Sequence>` restarts at 0. If the video is laid out with `<Sequence>`s or `<TransitionSeries>` and the list only names its chapters, pass `{ clock: 'root' }` to read it on the composition's frame instead:

```tsx title="The video's chapter, from inside a chapter"
function Body() {
  const scene = useScene(SCENES, { clock: 'root' }); // 'body' at frame 100, not 'intro'
  return <Text>{scene?.name}</Text>;
}

<Sequence from={90} durationInFrames={240}><Body /></Sequence>
```

## Chapters in the preview

`chapters` on `<Composition>` lists named spans of the video in the editor: the timeline gets a Chapters row above the tracks, and the React Preview tab a Chapters list with each start and length. The current chapter is highlighted, and a click on either seeks to its start. Pass a `defineScenes` list, a dialogue plan (its `scenes`), or spans of `{ id, name?, from, durationInFrames }` in the composition's frames. Nothing is inferred from the tree: a composition without `chapters` previews as before.

```tsx title="The scenes as the preview's chapters"
<Composition width={1920} height={1080} fps={30} durationInFrames={SCENES.durationInFrames} chapters={SCENES}>
  <Scenes scenes={SCENES} />
</Composition>
```

Chapters are one flat list on the composition's timeline. A list played inside one scene, such as the parts of a demo, joins it with `listChapters(source, { from })`, which moves its spans to where that scene starts; the editor stacks chapters that overlap on lanes of their own.

```tsx title="A scene's parts, listed below it"
const chapters = [
  ...listChapters(SCENES),
  ...listChapters(PARTS, { from: SCENES.scenes[1].from }),
];
```

Like the size and clock, `chapters` is read once when the entry loads, and again on every reload. Chapters are ordered by their start and cut at the composition's end; a chapter of length 0 is a seekable mark. A missing or duplicate `id`, a negative or fractional frame, or a chapter starting at or after the end fails the load with an error.

## Transitions between scenes

`<TransitionSeries>` lays scenes out like `Series`, with a `<TransitionSeries.Transition>` between two of them. A transition overlaps the scenes on either side by its `durationInFrames`, so `computeTransitionSeries()` returns each scene’s start and the total length, shortened by every overlap:

```tsx file=examples/transition-series.tsx title="Scenes joined by a cross-fade and a slide"
```

Each scene is an ordinary Sequence whose frame 0 is the first frame of its transition in, and the next scene is drawn above. A transition longer than either scene, or a scene too short for its transitions in and out together, throws instead of leaving a gap or showing three scenes at once. Overlapping scenes play their sound together: `useTransitionVolume(volume)` gives an Audio in the scene a volume that fades with its picture, and `useTransitionSeriesScene()` reports the scene’s transitions for any other timing.

`type` names one of these. A name uses the defaults in brackets; call the function of the same name from `@celesta.studio/transitions` for other values, as in `type={iris({ center: { x: 0.3, y: 0.5 }, feather: 40 })}`. `from` (`'left'` by default) is the edge `'slide'`, `'wipe'`, and `'cube'` come from, and the axis `'flip'` turns about. `easing` shapes every type’s progress.

| `type` | What happens | Options |
| --- | --- | --- |
| `'cut'` | The next scene replaces the previous one, with no overlap. | |
| `'crossfade'` | The next scene fades in over the previous one. | |
| `'slide'` | The next scene pushes the previous one out. | |
| `'wipe'` | A straight edge reveals the next scene. | |
| `'iris'` | The next scene opens out of a circle. | `center` (`{ x: 0.5, y: 0.5 }`, fractions of the size), `direction` (`'open'`, or `'close'` to shrink the previous scene into a circle), `feather` (0 px) |
| `'clock-wipe'` | A hand sweeps once around a point. | `center`, `startAngle` (0, degrees clockwise from 12 o’clock), `direction` (`'clockwise'` or `'counterclockwise'`) |
| `'zoom'` | Both scenes zoom about a point while the next fades in. | `direction` (`'in'` or `'out'`), `scale` (1.5), `center` |
| `'dissolve'` | The next scene shows through square by square. | `cellSize` (16 px), `seed` (0) |
| `'blur'` | The previous scene blurs away as the next comes into focus. | `amount` (32 px, up to 64) |
| `'flip'` | The picture turns over like a card. | `shade` (0.4): how dark a face gets as it turns; `perspective` (none): turns it in 3D about the centre, seen from this many pixels |
| `'cube'` | Two faces of a cube turn a quarter turn. | `shade` (0.4); `perspective` (none): turns the faces in 3D about the edge they share |

By default `'flip'` and `'cube'` are drawn without perspective: the faces narrow and darken, and the cube is scaled down mid-turn so it always spans the series. With a `perspective`, as in `flip({ perspective: 2000 })`, they turn in 3D instead: the flip about the series' centre, the cube about the edge its faces share, which crosses the series and is also where the viewer looks from. Every type draws the same in preview, export, and the browser editor.

Transitions move and cut across the series’ size: how far a slide pushes, where a wipe ends, where an iris centres. That is the composition’s size, or the area of an enclosing `<SafeArea>` or `<Fit>`. For a series in a smaller frame, give `<TransitionSeries>` the frame’s `width` and `height`, and place it with a `Group` (with a `clip` if the scenes must not slide out of the frame):

```tsx title="A series in a 560 px frame"
<Group x={260} y={400} clip={{ width: 560, height: 315 }}>
  <TransitionSeries width={560} height={315}>…</TransitionSeries>
</Group>
```

### Your own transitions

`defineTransition({ name, present })` makes a transition to pass as `type`. On every overlapped frame, `present` gets the eased `progress`, the `frame` and `durationInFrames` of the overlap, `from`, and the series’ `width` and `height`, and returns `Group` props for the scene coming in (`enter`) and the one going out (`exit`): a position, scale, opacity, `clip`, `blur`, color filter, `shader`, or `mask`. A scene's group turns about the series' top-left corner; return `originX: width / 2, originY: height / 2` with a `rotationX` or `rotationY` and a `perspective` to turn it in 3D about the centre (see `examples/with-3d-transition.tsx`). The built-in types are written the same way, and `useTransitionSeriesScene()` reports a defined transition by its `name`. Custom WGSL shaders run in preview and export, not in the browser editor.

```tsx file=examples/custom-transition.tsx title="Blinds defined with a mask, then an off-centre iris"
```

## Cascades and entrances

`progress(frame, start, durationInFrames, easing?)` is a clamped 0–1 value for “how far through this span are we”: the building block of most entrances. `<Stagger each={n}>` starts each child `n` frames after the previous one, so an entrance written for frame 0 cascades down a list:

```tsx title="Rows that arrive one after another"
function Row({ label, y }: { label: string; y: number }) {
  const frame = useCurrentFrame(); // 0 when this row starts
  const p = progress(frame, 0, 20, Easings.easeOutExpo);
  return <Text x={120 + 40 * (1 - p)} y={y} opacity={p}>{label}</Text>;
}

<Stagger each={4}>
  {items.map((item, i) => <Row key={item} label={item} y={200 + i * 64} />)}
</Stagger>
```

## On the beat

`useBeat({ bpm })` returns the current `beat`, `bar`, `beatInBar`, the `progress` through the beat, and a `pulse` that is 1 on every beat and decays until the next. Use it to flash, bump, or cut in time with the soundtrack; `offset` moves the first beat.

```tsx title="A dot that pulses with the music"
const { pulse } = useBeat({ bpm: 120 });
return <Rect width={40} height={40} cornerRadius={20} scale={1 + 0.3 * pulse} fill="#7cf29c" />;
```

## Cues

`useCue(cues)` takes a list of `{ at, …data }` objects sorted by `at` and returns the one in effect, its `index`, the `frame` since it started, and the `previous` and `next` cues. Swapping captions, a camera moving between stops, or a chart callout that changes are all a list of cues. `cueAt(cues, frame)` is the same outside a component.

```tsx title="Captions that swap and fade in"
const active = useCue([
  { at: 0, text: 'Write it.' },
  { at: 45, text: 'Preview it.' },
  { at: 90, text: 'Ship it.' },
]);
if (!active) return null;
return <Text opacity={progress(active.frame, 0, 10)}>{active.cue.text}</Text>;
```

## Timecodes

`frameToTimecode(frame, fps)`, from `@celesta.studio/react`, formats `HH:MM:SS:FF` for an on-screen clock.
