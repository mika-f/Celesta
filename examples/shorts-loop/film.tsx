import { Assets, Audio, Composition, Font } from '@celesta/react';
import { DURATION, FPS, H, W } from './constants';
import { Loop } from './scenes/Loop';

// SHORTS LOOP — a 12.8-second vertical loop for Shorts and TikTok. The last
// frame runs into the first, picture and sound alike: 384 frames at 30 fps
// are exactly four bars at 75 BPM (24 frames per beat).
//
//   scenes/        Loop, the one scene
//   components/    Field (background), Bloom (kaleidoscope), Gem, Copy (text)
//   shaders/       the three WGSL filters and their definitions
//   loop.ts        clocks that repeat every loop, beat, or bar
//   constants.ts   canvas, tempo, palette, safe area

export default function ShortsLoop() {
  return <Composition width={W} height={H} fps={FPS} durationInFrames={DURATION}>
    <Assets>
      <Font src="../afterimage/assets/fonts/BebasNeue-Regular.ttf" />
      <Font src="../afterimage/assets/fonts/IBMPlexMono-Regular.ttf" />
    </Assets>
    <Audio src="./assets/score.wav" />
    <Loop />
  </Composition>;
}
