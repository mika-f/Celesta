import { Assets, Audio, Composition, Font, Series, defineProjectProperties, getProjectProperty } from '@celesta/react';
import { Fx } from './components/Fx';
import { Hud } from './components/Hud';
import { SafeGuide } from './components/SafeGuide';
import { FONT_SRC, FPS, H, W } from './constants';
import { measureAll } from './measure';
import { SCENES, TIMING } from './timeline';

// "Shorts Type" in English. A 28-second vertical (1080×1920) piece of
// kinetic typography, the companion of examples/shorts-type: headlines set
// line by line at the size that fills the narrow column, glyphs drawn over
// their own measurements, copy fitted to changing boxes, <Span> emphasis and
// beat sync. 120 BPM, 30 fps: one beat is 15 frames, one bar is 60, and
// every scene is two bars. Copy meant to be read stays inside the safe area
// (constants.ts) that Shorts and TikTok leave free of their UI; backgrounds
// and decoration fill the whole frame.
// Open this file in Celesta, or export it with:
//   Celesta-export --react examples/shorts-type-en/film.tsx shorts-type-en.mp4
//
//   timeline.ts    the scene list         scenes/       one file per scene
//   measure.ts     headline sizes, glyph metrics and fits, measured in prepare()
//   halftone.ts    background shader      fx.ts         finishing shader (RGB split, glitch, flash)
//   components/    Copy/Tag (text), FitStack (fitted headline lines),
//                  Stage (background, tapes, safe area, camera),
//                  Fx (drives fx.ts), Tape, Burst (speed lines), Hud (scene and beat),
//                  SafeGuide (platform UI overlay)
//   constants.ts   canvas, timing, safe area, palette, fonts

defineProjectProperties({
  guides: { type: 'boolean', label: 'Safe-area guides', defaultValue: false },
});

let guides = false;

export async function prepare() {
  guides = getProjectProperty<boolean>('guides');
  await measureAll();
}

export default function Film() {
  return (
    <Composition width={W} height={H} fps={FPS} durationInFrames={TIMING.durationInFrames} lang="en-US">
      <Assets>
        <Font src={FONT_SRC} />
      </Assets>
      <Fx>
        <Series>
          {SCENES.map(({ name, durationInFrames, Scene }) => (
            <Series.Sequence key={name} durationInFrames={durationInFrames}><Scene /></Series.Sequence>
          ))}
        </Series>
        <Hud />
      </Fx>
      {guides && <SafeGuide />}
      <Audio src="./score.wav" />
    </Composition>
  );
}
