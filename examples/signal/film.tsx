import { Assets, Audio, Composition, Font, Rect, useCurrentFrame } from '@celesta/react';
import { BONE, END, FPS, H, W } from './constants';
import { Counter } from './scenes/Counter';
import { Kinetic } from './scenes/Kinetic';
import { Scrub } from './scenes/Scrub';
import { Source } from './scenes/Source';
import { Title } from './scenes/Title';

// CELESTA / SIGNAL — an editorial motion piece, 16 seconds at 30 fps.
// Cuts are on the half-second grid of the 120 BPM score.
//
//   scenes/        one file per scene, in playback order
//   components/    Label (text), Signal and Rotor (animated generative forms)
//   constants.ts   canvas size and palette        math.ts   clamp / ease / mix

// First frame of each scene (frames at 30 fps). A bone flash marks every cut.
const SOURCE = 60, KINETIC = 150, SCRUB = 300, TITLE = 390;
const CUTS = [SOURCE, KINETIC, 210, 255, SCRUB, TITLE];

function Film() {
  const f = useCurrentFrame();
  return <>
    {f < SOURCE ? <Counter f={f} /> : f < KINETIC ? <Source f={f} /> :
      f < SCRUB ? <Kinetic f={f} /> : f < TITLE ? <Scrub f={f} /> : <Title f={f} />}
    {CUTS.includes(f) &&
      <Rect width={W} height={H} fill={BONE} opacity={0.48} />}
  </>;
}

export default function CelestaSignal() {
  return <Composition width={W} height={H} fps={FPS} durationInFrames={END}>
    <Assets><Font src="../afterimage/assets/fonts/IBMPlexMono-Regular.ttf" /></Assets>
    <Audio src="./assets/score.wav" />
    <Film />
  </Composition>;
}
