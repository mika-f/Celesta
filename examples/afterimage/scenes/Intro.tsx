import { Rect } from '@celesta/react';
import { Ribbon } from '../components/Ribbon';
import { Type } from '../components/Type';
import { FPS, H, INK, PAPER } from '../constants';
import { ease, mix } from '../math';

export function Intro({ f }: { f: number }) {
  const t = f / FPS;
  return <>
    <Ribbon time={t} size={330} opacity={ease(f / 45)} />
    <Rect width={mix(960, 0, ease((f - 12) / 44))} height={H} fill={INK} />
    <Rect x={mix(960, 1920, ease((f - 12) / 44))} width={960} height={H} fill={INK} />
    <Type x={960} y={896} size={19} mono center opacity={ease((f - 12) / 20)}>SOME THINGS STAY.</Type>
    <Rect x={959} y={mix(540, 400, ease(f / FPS))} width={2} height={mix(0, 280, ease(f / FPS))}
      fill={PAPER} opacity={1 - ease((f - 25) / FPS)} />
  </>;
}
