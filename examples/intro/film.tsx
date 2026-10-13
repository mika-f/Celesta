import { Assets, Composition, Font, Rect, Sequence } from '@celesta/react';
import { FrameCounter } from './components/FrameCounter';
import { C, DURATION, FONT_SRC, FPS, H, W } from './constants';
import { HOLD } from './shared/cues';
import { SCENES } from './timeline';

// "This video is code." A 60-second introduction to Celesta for YouTube, cut
// to a 120 BPM score (one beat = 15 frames, one bar = 60 frames at 30 fps).
// Open this file in Celesta, or export it with:
//   celesta-exporter --react examples/intro/film.tsx intro.mp4
//
//   shared/cues.ts   scene starts (S) and the end-screen hold
//   timeline.ts      the scene list
//   scenes/          one file per scene
//   components/      shared drawing parts
//   shared/          values that are both drawn and shown as code (tested in test/)

export default function Root() {
  return (
    <Composition width={W} height={H} fps={FPS} durationInFrames={DURATION}>
      <Assets>
        <Font src={FONT_SRC} />
      </Assets>
      <Rect width={W} height={H} fill={C.bg} />
      {SCENES.map(({ from, to, Scene }) => (
        <Sequence key={from} from={from} durationInFrames={to - from}><Scene /></Sequence>
      ))}
      <Sequence from={0} durationInFrames={HOLD}><FrameCounter /></Sequence>
    </Composition>
  );
}
