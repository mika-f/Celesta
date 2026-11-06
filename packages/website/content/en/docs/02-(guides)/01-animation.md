A composition’s duration in seconds is `durationInFrames / fps`. At 30 fps, 150 frames make five seconds, with frame indices from 0 through 149. Use `useCurrentFrame()` inside a composition to calculate animated values.

## Interpolate between values

The title example uses `Math.min(frame / 30, 1)` for opacity. For easing and more control, import `interpolate` and `Easings` from `@celesta.studio/react`, then replace the calculation with:

```tsx title="An eased fade (inside a component)"
const opacity = interpolate(frame, [0, 30], [0, 1], {
  easing: Easings.easeOut,
  extrapolateLeft: 'clamp',
  extrapolateRight: 'clamp',
});
```

Pass the result to `<Text opacity={opacity} … />`. Input ranges must be strictly increasing and have the same number of entries as the output range. The input and every range entry must be finite numbers: a `NaN`, such as one from a misspelled field, throws an error instead of returning `NaN`. Extrapolation defaults to `extend`; use `clamp` to hold the first and last values.

`Easings` covers the familiar easings.net curves: `linear`, and the `easeIn`/`easeOut`/`easeInOut` variants of `Sine`, `Quad`, `Cubic`, `Quart`, `Quint`, `Expo`, `Circ`, `Back`, `Elastic`, and `Bounce` (for example `Easings.easeOutBack`).

## Interpolate between colors

Import `interpolateColor` from `@celesta.studio/react` too. It takes the same input range and `easing` as `interpolate`, but maps the frame onto `#RRGGBB` or `#RRGGBBAA` colors and returns `#RRGGBBAA`. Use it wherever a color goes: a `Rect` fill, a text style’s fill, a gradient stop, or a shadow.

```tsx title="Night to day, and a title fading in"
const sky = interpolateColor(frame, [0, 45, 90], ['#101820', '#EF7B45', '#7FC8F8']);
const title = interpolateColor(frame, [0, 20], ['#FFFFFF00', '#FFFFFF']);

<Rect width={1920} height={1080} fill={sky} />
<Text style={{ fontSize: 96, fill: { type: 'solid', color: title } }}>Morning</Text>
```

Unlike `interpolate`, extrapolation defaults to `clamp`, so the first and last colors hold outside the range; `extend` continues the end segments, and `identity` is not available. Colors blend in sRGB with premultiplied alpha, as gradients and CSS do: a color fading to `#00000000` keeps its hue instead of darkening, and channels that `extend` or an overshooting easing push out of range are clamped. CSS color names, `rgb()`, and the short `#RGB` form are not accepted.

## Add a little bounce

`spring()` simulates a damped spring: it starts at 0, settles at 1, and may overshoot on the way. It needs the frame rate, so read it from `useVideoConfig()`:

```tsx title="A springy pop-in"
function Badge() {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const scale = spring({ frame, fps, delay: 10, config: { damping: 12 } });
  return (
    <Rect x={960} y={540} anchorX={0.5} anchorY={0.5} scale={scale}
      width={320} height={120} cornerRadius={60} fill="#78618e" />
  );
}
```

Tune the feel with `config`: `stiffness` (default 100), `damping` (default 10), `mass` (default 1), and `overshootClamping`. Use `from` and `to` to map the result to another range. `frame`, `delay`, `from`, and `to` must be finite numbers, `fps` a finite number above 0, and `durationInFrames`, when set, a number other than `NaN` (`Infinity` is the same as leaving it out); anything else throws an error.

## Place a scene on the timeline

Import `Sequence` to delay or limit a group of layers. This shows the `Title` component from the previous example starting at frame 30 for 90 frames:

```tsx title="Inside your Composition"
<Sequence from={30} durationInFrames={90}>
  <Title />
</Sequence>
```

Within that sequence, `useCurrentFrame()` starts at 0 when the sequence begins. Child video and audio use the same local clock. Children contribute neither layers nor audio outside the sequence’s time window.

`from` defaults to 0 and must otherwise be a finite number; it may be negative. Leave `durationInFrames` out, or set it to `Infinity`, to run until the end of the enclosing sequence or composition; `0` is an empty window. A `from` that is `NaN`, infinite, or not a number, and a `durationInFrames` that is `NaN`, negative, or not a number, throw an error that names the prop, so a timing computed from a misspelled field fails instead of covering every later scene.

## Repeat a scene

`<Loop durationInFrames>` plays its children over and over: every `durationInFrames` frames, their clock starts again at 0. Use it for a background animation or a short piece of music that repeats:

```tsx title="A 2-second pulse and a 4-second BGM phrase, repeated"
<Loop durationInFrames={60}>
  <Pulse />
</Loop>
<Loop durationInFrames={120} times={3}>
  <Audio src="./phrase.wav" />
</Loop>
```

Inside, `useCurrentFrame()`, `<Video>`, `<Audio>`, and nested sequences all restart with each cycle, and `useVideoConfig().durationInFrames` is one cycle’s length. `from` sets where the first cycle starts, like a `Sequence`’s. Without `times`, the loop repeats until the enclosing sequence or composition ends; a fractional `times` such as `2.5` cuts the last cycle short. Each cycle’s audio is a clip of its own, cut where the loop ends. Only the current cycle is mounted, so any frame you scrub to looks the same as in the export.

## Redraw a past frame

Every frame is a function of the frame number, so `<FreezeFrame frame>` can draw any moment of the video again in place, for a rewind, a flashback, or a row of thumbnails. `frame` counts on the whole composition’s clock, even inside a `Sequence`:

```tsx title="Frame 1200, at quarter size"
<Group scale={0.25} x={80} y={80}>
  <FreezeFrame frame={1200}>
    <World />
  </FreezeFrame>
</Group>
```

Directly inside, `useCurrentFrame()` returns `frame`, rounded to a whole frame (a `Sequence` within it still counts from its own start), and sequences, video, lip sync, and blinking all show that moment. A fixed `frame` holds still; with `frame` read from `useCurrentFrame()` in your root component, `frame={frame - 90}` replays 90 frames behind (three seconds at 30 fps). Frozen children are silent. A `CharacterView` drawn inside leaves its ref to the live view, and a `Dialogue` inside drives only a view inside the same `FreezeFrame`. Project layers (`ProjectTimeline`, `ProjectTrack`) are evaluated for the current frame only and cannot be drawn inside. A video may hold a smaller copy of itself, but a `FreezeFrame` whose frame plays that same `FreezeFrame` again would draw itself forever, so it throws an error naming the frames involved, as does nesting `FreezeFrame`s more than 16 deep.

## Blur fast motion

Something that moves far between two frames jumps from one place to the next. `<MotionBlur>` draws its children at several moments while a shutter is open around the frame and shows their average, so the movement smears the way a camera sees it:

```tsx title="A card that dashes across, blurred"
<MotionBlur shutterAngle={180} samples={8}>
  <DashingCard />
</MotionBlur>
```

`shutterAngle` is how long the shutter stays open, in degrees of a frame: 180 (the default) is half a frame, centered on the frame, and 360 a whole frame. `samples` (8 by default) is how many moments are averaged; each one draws the children again, so a blur costs about that many times the children. When very fast motion shows steps instead of a smooth smear, raise `samples`.

Inside, `useCurrentFrame()` returns each moment’s frame, such as `12.25`, so components that animate from it move between the moments. Code that indexes an array by the frame needs `Math.floor` there. Props you compute outside, such as a `Camera`’s `x`, are the same in every moment; pass a function to compute them per moment instead:

```tsx title="Blur a camera move"
<MotionBlur>
  {(frame) => (
    <Camera x={interpolate(frame, [0, 30], [0, 3000], { extrapolateRight: 'clamp' })}>
      <World />
    </Camera>
  )}
</MotionBlur>
```

The result of a frame does not depend on which frames were drawn before it, and the editor preview draws the same blur as the export. Moments stay within the enclosing `Sequence`, so the first and last frames of a shot do not fade. `<Video>`, animated images, lip sync, and blinking keep the frame’s picture in every moment (footage already carries the blur of its own camera), and audio and subtitles inside play once. Every moment is a copy, like a `FreezeFrame`’s: a `CharacterView` inside leaves its ref alone, so put its `Dialogue` inside too. Project layers cannot be drawn inside. The blurred layer takes a group’s props (`x`, `opacity`, `blendMode`, `blur`, …), which apply to the average.

## Leave a trail

`<Trail>` draws earlier copies of its children beneath them, each `lagInFrames` frames older and fainter than the one above:

```tsx title="A ball with four ghosts"
<Trail copies={4} lagInFrames={1.5} trailOpacity={0.5}>
  <Ball />
</Trail>
```

The newest copy is drawn at `trailOpacity` and each older one fades by the same step. A copy fades as a `Group` with that opacity does, so overlapping layers within one copy show through each other. Copies from before the enclosing `Sequence` began are not drawn. Inside a copy, `useCurrentFrame()` and the rules for video, audio, and characters are those of `MotionBlur`; the children on top are the live ones.

## Ready-made entrances and exits

`<Transition>` fades, slides, or scales its children at the start (`direction="in"`, the default) or end (`direction="out"`) of the enclosing sequence:

```tsx title="Slide a caption in, fade it out"
<Sequence from={30} durationInFrames={120}>
  <Transition type="slide" slideFrom="bottom" durationInFrames={15}
    easing={Easings.easeOutCubic}>
    <Transition type="fade" direction="out" durationInFrames={20}>
      <Caption />
    </Transition>
  </Transition>
</Sequence>
```

Slides travel `distance` pixels (64 by default); scales start from `scaleFrom` (0.8 by default). Pass several types to combine them, such as `type={['fade', 'slide']}`.

> [!NOTE]
> **Keep animation tied to the frame**
> Compute visual changes from frame or time values so scrubbing and exporting can reproduce each moment. A browser timer or `Math.random()` is not the composition’s clock; derive “random” values from the frame or an index instead, with [`@celesta.studio/math`](/en/docs/math/)’s seeded `random` and `noise`.

## Hold a scene at one frame

`<FreezeFrame frame={60}>` draws its children at root composition frame 60, even when wrapped in a Sequence. Child hooks, video, lip sync, and blinking follow that time; nested sequences still apply their own offsets. Audio inside FreezeFrame is muted.

```tsx title="FreezeFrame"
<FreezeFrame frame={60}>
  <AnimatedTitle />
</FreezeFrame>
```
