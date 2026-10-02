import { Assets, Audio, Composition, Font, Rect, useCurrentFrame } from '@celesta/react';
import { DURATION, FPS, H, INK, PAPER, RED, W } from './constants';
import { Echo } from './scenes/Echo';
import { Impact } from './scenes/Impact';
import { Intro } from './scenes/Intro';
import { Outro } from './scenes/Outro';
import { Study } from './scenes/Study';
import { Title } from './scenes/Title';

// AFTERIMAGE — a 24-second optical study. All cuts sit on a 120 BPM grid.
//
//   scenes/        one file per scene, in playback order
//   components/    Type (text), Line, Ribbon (3D half-twist ribbon), Margin (page chrome)
//   constants.ts   canvas size and palette        math.ts   clamp / ease / mix

function Film() {
  const f = useCurrentFrame();
  const intro = f < 60;
  const title = f >= 60 && f < 180;
  const study = f >= 180 && f < 300;
  const impact = f >= 300 && f < 420;
  const echo = f >= 420 && f < 570;
  const outro = f >= 570;
  const bg = title || echo ? PAPER : impact ? RED : INK;
  return <>
    <Rect width={W} height={H} fill={bg} />
    {intro && <Intro f={f} />}
    {title && <Title f={f} />}
    {study && <Study f={f} />}
    {impact && <Impact f={f} />}
    {echo && <Echo f={f} />}
    {outro && <Outro f={f} />}
  </>;
}

export default function Afterimage() {
  return <Composition width={W} height={H} fps={FPS} durationInFrames={DURATION}>
    <Assets>
      <Font src="./assets/fonts/BebasNeue-Regular.ttf" />
      <Font src="./assets/fonts/IBMPlexMono-Regular.ttf" />
    </Assets>
    <Audio src="./assets/score.wav" />
    <Film />
  </Composition>;
}
