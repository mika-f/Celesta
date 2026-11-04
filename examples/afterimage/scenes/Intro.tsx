import { lerp } from '@celesta.studio/math';
import { Easings, Rect, progress } from '@celesta.studio/react';
import { Ribbon } from '../components/Ribbon';
import { Type } from '../components/Type';
import { FPS, H, INK, PAPER } from '../constants';

export function Intro({ f }: { f: number }) {
  const t = f / FPS;
  return <>
    <Ribbon time={t} size={330} opacity={progress(f, 0, 45, Easings.easeOutQuart)} />
    <Rect width={lerp(960, 0, progress(f, 12, 44, Easings.easeOutQuart))} height={H} fill={INK} />
    <Rect x={lerp(960, 1920, progress(f, 12, 44, Easings.easeOutQuart))} width={960} height={H} fill={INK} />
    <Type x={960} y={896} size={19} mono center opacity={progress(f, 12, 20, Easings.easeOutQuart)}>SOME THINGS STAY.</Type>
    <Rect x={959} y={lerp(540, 400, progress(f, 0, FPS, Easings.easeOutQuart))} width={2} height={lerp(0, 280, progress(f, 0, FPS, Easings.easeOutQuart))}
      fill={PAPER} opacity={1 - progress(f, 25, FPS, Easings.easeOutQuart)} />
  </>;
}
