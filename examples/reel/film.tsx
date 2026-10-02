import { Assets, Audio, Composition, Font, Sequence } from '@celesta/react';
import { Hud } from './components/Hud';
import { Wipe } from './components/Wipe';
import { DURATION, FPS, FONT_SRC, H, W } from './constants';
import { S, SCENES } from './timeline';

// "Code is the cut." A 32-second kinetic-type reel for Celesta, cut to a
// 120 BPM score (one beat = 15 frames, one bar = 60 frames at 30 fps).
// Open this file in Celesta, or export it with:
//   Celesta-export --react examples/reel/film.tsx reel.mp4
//
//   timeline.ts    scene boundaries (S) and the scene list
//   scenes/        one file per scene
//   components/    Label, Mono (colored code runs), Tag, Swap, Wipe, Hud
//   metrics.ts     font measurements made in prepare()   constants.ts, helpers.ts   palette, math

// Measures the fonts once before the first frame.
export { prepare } from './metrics';

export default function Root() {
  return (
    <Composition width={W} height={H} fps={FPS} durationInFrames={DURATION}>
      <Assets>
        <Font src={FONT_SRC} />
      </Assets>
      {SCENES.map(({ from, to, Scene }) => (
        <Sequence key={from} from={from} durationInFrames={to - from}><Scene /></Sequence>
      ))}
      {[S.field, S.code, S.timeline, S.exportAt].map((cut) => (
        <Sequence key={`wipe-${cut}`} from={cut - 7} durationInFrames={15}><Wipe /></Sequence>
      ))}
      <Sequence from={0} durationInFrames={S.silence}><Hud /></Sequence>
      <Audio src="./music.wav" />
    </Composition>
  );
}
