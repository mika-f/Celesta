import { Assets, Composition, Font, Sequence, Series } from '@celesta/react';
import { Backdrop } from './components/Chrome';
import { Hud, Wipe } from './components/Hud';
import { FPS, FONT_SRC, H, W } from './constants';
import { CUTS, SCENES, SECTIONS, SERIES } from './timeline';

// VERSUS: the same heavy scene (bench/) built with Remotion, fframes and
// Celesta, then compared on speed and syntax. Run the benchmark first so the
// film can play the three exports and show the measured numbers:
//   node examples/versus/bench/run.mjs && node examples/versus/bench/loop.mjs
// then open this file in Celesta, or export it with:
//   Celesta-export --react examples/versus/film.tsx versus.mp4

export default function Root() {
  return (
    <Composition width={W} height={H} fps={FPS} durationInFrames={SERIES.durationInFrames}>
      <Assets>
        <Font src={FONT_SRC} />
      </Assets>
      <Backdrop />
      <Series>
        {SCENES.map(({ name, durationInFrames, Scene }, i) => (
          <Series.Sequence key={`${name}-${i}`} durationInFrames={durationInFrames}><Scene /></Series.Sequence>
        ))}
      </Series>
      <Hud sections={SECTIONS} />
      {CUTS.map((cut) => (
        <Sequence key={cut} from={cut - 8} durationInFrames={16}><Wipe /></Sequence>
      ))}
    </Composition>
  );
}
