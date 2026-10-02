import { Rect, useCurrentFrame } from '@celesta/react';
import { Label } from '../components/Label';
import { Prism } from '../components/Prism';
import { Rails } from '../components/Rails';
import { BLUE, GREY, INK, PAPER, RED } from '../constants';

export function Rhythm() {
  const f = useCurrentFrame();
  // The global score is 120 BPM; local beat zero is also a downbeat.
  const hit = Math.exp(-(f % 15) / 4);
  return <>
    <Rails chapter="03 / SOUND & PICTURE" />
    <Label y={163} size={185}>FIND YOUR FREQUENCY.</Label>
    <Label y={381} size={23} mono color={GREY}>LAYER THE IMAGE. SHAPE THE SOUND.</Label>
    <Rect x={80} y={462} width={555} height={437} fill={BLUE} />
    <Rect x={657} y={462} width={555} height={437} fill={PAPER} />
    <Rect x={1234} y={462} width={606} height={437} fill={RED} />
    <Prism frame={f + 400} x={358} y={675} size={190 + hit * 20} color={PAPER} count={16} />
    <Label x={685} y={493} size={17} mono color={INK}>02 / TYPOGRAPHY</Label>
    {['MAKE', 'SOME', 'NOISE.'].map((s, i) => <Label key={s} x={688 + hit * (i + 1) * 4}
      y={544 + i * 99} size={116} color={INK}>{s}</Label>)}
    {Array.from({ length: 42 }, (_, i) => {
      const height = 15 + (130 + hit * 90) * Math.abs(Math.sin(i * 0.57 + f * 0.13))
        * Math.sin((i + 1) / 43 * Math.PI);
      return <Rect key={i} x={1261 + i * 13} y={681} anchorY={0.5}
        width={6} height={height} fill={INK} />;
    })}
    <Label x={110} y={852} size={17} mono>01 / GENERATIVE IMAGE</Label>
    <Label x={1265} y={852} size={17} mono color={INK}>03 / ORIGINAL AUDIO</Label>
    <Label y={932} size={21} mono color={GREY}>THREE LAYERS. ONE RHYTHM.</Label>
  </>;
}
