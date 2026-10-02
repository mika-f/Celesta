import { Assets, Audio, Composition, Font, Series, computeSeries } from '@celesta/react';
import { Hud } from './components/Hud';
import { FPS, H, W } from './constants';
import { SCENES } from './scenes';

// APEX — a one-minute motion film. 1800 frames at 30 fps, 120 BPM (15 frames per beat).
// Every chapter is a pure function of the frame; every cut lands on a beat.
//
//   scenes/        one file per chapter; scenes/index.ts lists them in order
//   components/    Label (text), Disc, Ring, Glitch (chromatic text), Hud (frame overlay)
//   constants.ts   canvas size and palette        math.ts   3D rotate / project / mix

const { durationInFrames } = computeSeries(SCENES);

export default function Root() {
  return <Composition width={W} height={H} fps={FPS} durationInFrames={durationInFrames}>
    <Assets>
      <Font src="../afterimage/assets/fonts/BebasNeue-Regular.ttf" />
      <Font src="../afterimage/assets/fonts/IBMPlexMono-Regular.ttf" />
    </Assets>
    <Audio src="./assets/score.wav" />
    <Series>
      {SCENES.map(({ name, durationInFrames, Scene }) => (
        <Series.Sequence key={name} durationInFrames={durationInFrames}><Scene /></Series.Sequence>
      ))}
    </Series>
    <Hud total={durationInFrames} />
  </Composition>;
}
