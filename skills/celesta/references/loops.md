# Seamless loops

YouTube Shorts, TikTok, and Reels replay a video as soon as it ends. In a
seamless loop the last frame runs into the first as smoothly as any two
neighboring frames, in both picture and sound, so viewers cannot see where
it restarts.

The rule behind everything below: **frame N (one past the last, where N is
`durationInFrames`) must look exactly like frame 0.** A value that differs
between them jumps at the seam.

## Contents

- [Pick the length first](#pick-the-length-first)
- [Make every motion periodic](#make-every-motion-periodic)
- [Nothing marks the start](#nothing-marks-the-start)
- [Noise that loops](#noise-that-loops)
- [Sound](#sound)
- [Check the loop](#check-the-loop)

## Pick the length first

Fix the loop length as a module constant, made of whole beats and bars, and
derive all timing from it:

```ts
// loop.ts
export const FPS = 30;
export const BPM = 75;
export const BEAT = (FPS * 60) / BPM;  // 24 frames
export const BAR = BEAT * 4;           // 96 frames
export const LOOP = BAR * 4;           // 384 frames, 12.8 s
if (!Number.isInteger(BEAT)) throw new Error(`${BPM} BPM is not a whole number of frames at ${FPS} fps`);
```

Pick a BPM that gives whole frames per beat (at 30 fps: 60, 75, 90, 100,
120, 150; at 60 fps also 72, 80, 144). With a fractional beat, beats drift
across frames and the bar lines do not meet the seam. Use the constant for
`<Composition durationInFrames={LOOP}>` and in loop math: inside a
`<Sequence>`, `useVideoConfig().durationInFrames` is the sequence's length,
not the loop's.

Loop math also needs the composition's frame, and inside a `<Sequence>` (or
a `Series` scene) `useCurrentFrame()` counts from the sequence's start. Draw
loop-wide motion outside sequences, or add the sequence's `from` back.

## Make every motion periodic

Every moving value must return to its frame-0 value at frame `LOOP`. Drive
motion with one of these:

| Motion | Write | Repeats when |
| --- | --- | --- |
| Once around the loop | `theta = TAU * frame / LOOP`, then `Math.sin(k * theta)`, `Math.cos(k * theta)` | `k` is a whole number (`k = 1.5` jumps at the seam) |
| A wave | `sineWave(frame / period)` (and triangle, square, sawtooth; [math.md](math.md#waves)) | `period` divides `LOOP` |
| On the beat | `frame % BEAT`, or `useBeat({ bpm: BPM })`'s `progress`, `pulse`, `beatInBar`, `barProgress` | `BEAT` and `BAR` divide `LOOP` |
| A full turn | `rotation={(360 * k * frame / LOOP) % 360}` | `k` is a whole number |
| Scrolling a tiled background | `x={-((speed * frame) % tileWidth)}` | `speed * LOOP` is a multiple of `tileWidth` |
| Drift and wobble | `loopNoise(seed, frame / 20, LOOP / 20)` | always ([below](#noise-that-loops)) |

`useBeat`'s `beat` and `bar` count up forever; use `beatInBar`, or
`bar % BARS`, instead. Wrap angles and offsets with `%` as in the table: 360°
looks the same as 0°, but the loop check compares numbers and reports it.

```tsx
import { useCurrentFrame } from '@celesta.studio/react';
import { Circle } from '@celesta.studio/shapes';
import { TAU } from '@celesta.studio/math';
import { BEAT, LOOP } from './loop';

function Orbit() {
  const frame = useCurrentFrame();
  const theta = (TAU * frame) / LOOP;
  const kick = 1 - (frame % BEAT) / BEAT;   // 1 on each beat, falling to 0
  return (
    <Circle x={540 + 300 * Math.cos(theta)} y={960 + 300 * Math.sin(2 * theta)}
      radius={40 + 20 * kick} fill="#ff3366" />
  );
}
```

Avoid `interpolate` and `spring` over the whole loop: they start and end at
different values. Use them only inside a beat or bar (`frame % BAR`), where
they start again at every bar line, and keep their start and end values
equal if the motion crosses one.

## Nothing marks the start

- **No fades.** Frame 0 is already at full strength; nothing fades in at the
  start or out at the end.
- **No one-off events.** An intro, a title that enters once, or a count-up
  happens at a visible moment that a viewer recognizes on the next pass.
  Repeat events every bar (the same on each bar) or once per loop at any
  frame.
- **Events that cross the end** must also cross the start: draw them twice,
  the second time `LOOP` frames earlier, so their tail plays at frame 0.

  ```tsx
  function AcrossTheSeam({ from, durationInFrames, children }) {
    return <>
      <Sequence from={from} durationInFrames={durationInFrames}>{children}</Sequence>
      <Sequence from={from - LOOP} durationInFrames={durationInFrames}>{children}</Sequence>
    </>;
  }
  ```

- **Video clips** keep playing forward: `<Video>` at frame `LOOP` shows a
  different source frame from frame 0. Use only clips that themselves loop
  and last exactly `LOOP` frames, or `LOOP` divided by a whole number with
  `<Video loop>`, which starts the clip over at its end. A `<Video>` also
  plays its soundtrack, which must loop the same way as the music below;
  mute it if it does not.

## Noise that loops

`noise(seed, frame / 20)` and `fbm` never repeat, so a wobble driven by them
jumps at the seam. Use `loopNoise` and `loopFbm` from `@celesta.studio/math`, which
take the loop length in the same units as `t`:

```tsx
import { loopNoise } from '@celesta.studio/math';
// Was: noise('sway', frame / 20) * 12
const sway = loopNoise('sway', frame / 20, LOOP / 20) * 12;
```

For a 2D field that changes over the loop (clouds, a breathing grid), move
the third coordinate around a circle:

```tsx
const angle = (TAU * frame) / LOOP;
const n = noise3D('field', col / 6 + 0.8 * Math.cos(angle), row / 6, 0.8 * Math.sin(angle));
```

The radius (`0.8`) sets how much the field changes over one loop. See
[math.md](math.md#noise).

## Sound

- **Make the music exactly `LOOP` frames long:** whole bars at the BPM above.
  In samples, `LOOP / FPS * sampleRate` must be a whole number
  (12.8 s × 48 000 = 614 400).
- **No fade-in or fade-out** at the ends, and no volume keyframes that
  differ between the start and the end.
- **Fold the overhang back to the start.** Notes held over the last bar line,
  reverb tails, and delay echoes run past the end. Render the music with its
  tail, then play it twice: once from frame 0 (the composition cuts it at the
  end) and once `LOOP` frames earlier, so the part past the end plays at the
  start, mixed with the downbeat:

  ```tsx
  <Audio src="./score.wav" />
  <Sequence from={-LOOP}><Audio src="./score.wav" /></Sequence>
  ```

  The file must be at least `LOOP` frames long, and its tail shorter than the
  loop. Folding the overhang into the file itself (in a DAW or a script) works
  too; then one `<Audio>` is enough.
- **A short loop file** (ambience, a one-bar beat) repeats with
  `<Audio loop>`. Its length times a whole number must be `LOOP` frames.
- **Export with `--loop`.** An AAC encoder starts from silence, so a hit
  exactly on frame 0 comes out smeared for about 10 ms, and a looping player
  repeats that on every pass. `--loop` encodes the end of the soundtrack
  ahead of its start and hides it, so the restart sounds like any other bar
  line ([export.md](export.md#looping-video)). It does not fold the overhang
  for you; do that as above.

## Check the loop

1. **Compare frame `LOOP` with frame 0** without the GUI:

   ```sh
   node <skill>/scripts/inspect.mjs film.tsx --loop
   ```

   It evaluates frame N (and N + 1) as if the video went on, then compares
   their Scenes with frames 0 and 1, so both the picture and its motion must
   carry across the seam. A `<Sequence>` (or `Series` scene) that ends with
   the video stays open at N, and `useVideoConfig()` still reports the real
   durations. That includes one whose `durationInFrames` ends exactly on the
   last frame: an event meant to be over by then is still drawn at N and
   reported, so end it earlier or carry it across the seam as above.

   ```text
   frame 384 vs frame 0: 1 difference(s)
     layers[2](root.3).content.layers[0](root.3.0).transform.rotation: 22.093227 at frame 384, 12.453926 at frame 0
   frame 385 vs frame 1: 1 difference(s)
     …
   ```

   Each line names the layer path and the two values; find that layer in
   `inspect.mjs film.tsx --frames 0 --json` and fix its motion. A line like
   `layers: "3 layers: …" at frame 384, "4 layers: …" at frame 0` means a
   layer appears or disappears at the seam (a `Sequence` that ends early or
   starts late). Numbers may differ by one part in a million and colors by one
   step per channel; layer ids are not compared. The exit code is 1 when
   anything differs. `--json` lists every difference.

   It compares the picture only, not sound. An entry that measures text
   (`useTextMetrics`, `TextBox`, `@celesta.studio/code`, …) is checked through the
   exporter, as in [verify.md](verify.md#measured-text---native); `--loop`
   only uses an exporter recent enough to evaluate past the end. Without one
   such an entry stops with exit code 2: pass a newer build with
   `--native <Celesta-export>`, or compare the seam in the contact sheet
   below.

2. **Look at the seam.** Put the last and first frames side by side, next to
   an ordinary bar line for comparison:

   ```sh
   Celesta-export --react film.tsx --frames 95,96,383,0 --contact-sheet --columns 4 /tmp/celesta-seam.png
   ```

   The jump from 383 to 0 should look like the jump from 95 to 96. Tiles
   follow the listed order, so `383,0` puts the last frame first.

3. **Listen to the restart.** Export the MP4 with `--loop`
   ([export.md](export.md#looping-video)), check its length with `ffprobe`
   (`duration` equals `LOOP / FPS`), and play it on repeat around the seam.
