import { Assets, Audio, Composition, Font, Series, defineProjectProperties, getProjectProperty } from '@celesta/react';
import { Hud } from './components/Hud';
import { SafeGuide } from './components/SafeGuide';
import { FONT_SRC, FPS, H, W } from './constants';
import { measureAll } from './measure';
import { SCENES, TIMING } from './timeline';

// "Shorts Type." A 28-second vertical (1080×1920) piece of Japanese kinetic
// typography: a short manifesto set in a narrow column, where phrase line
// breaking, measured and fitted text, <Span> emphasis and beat sync matter
// most. 120 BPM, 30 fps: one beat is 15 frames, one bar is 60, and every
// scene is two bars. All copy stays inside the safe area (constants.ts) that
// Shorts and TikTok leave free of their UI.
// Open this file in Celesta, or export it with:
//   Celesta-export --react examples/shorts-type/film.tsx shorts-type.mp4
//
//   timeline.ts    the scene list         scenes/       one file per scene
//   measure.ts     text measured and fitted in prepare()
//   components/    Copy/Tag (text), Stage (background, safe area, camera, exit),
//                  Hud (scene and beat), SafeGuide (platform UI overlay)
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
    <Composition width={W} height={H} fps={FPS} durationInFrames={TIMING.durationInFrames} lang="ja-JP">
      <Assets>
        <Font src={FONT_SRC} />
      </Assets>
      <Series>
        {SCENES.map(({ name, durationInFrames, Scene }) => (
          <Series.Sequence key={name} durationInFrames={durationInFrames}><Scene /></Series.Sequence>
        ))}
      </Series>
      <Hud />
      {guides && <SafeGuide />}
      <Audio src="./score.wav" />
    </Composition>
  );
}
