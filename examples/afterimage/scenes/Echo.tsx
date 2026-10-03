import { Rect } from '@celesta/react';
import { Ribbon } from '../components/Ribbon';
import { Type } from '../components/Type';
import { Margin } from '../components/Margin';
import { FPS, H, INK, RED, W } from '../constants';
import { ease } from '../math';

export function Echo({ f }: { f: number }) {
  const t = f / FPS;
  return <>
    {[0, 1, 2].map(i => <Ribbon key={i} time={t - i * 0.35}
      x={960 + (i - 1) * (180 + 400 * (1 - ease((f - 420) / 100)))}
      y={510} size={310} color={i === 1 ? RED : INK} opacity={i === 1 ? 1 : 0.15}
      turn={0.7} strands={9} />)}
    <Type x={960} y={875} center size={74} color={INK}>GONE. STILL HERE.</Type>
    <Margin frame={f} label="THE TRACE REMAINS" />
    <Rect x={0} y={0} width={W} height={H * (1 - ease((f - 420) / 15))} fill={RED} />
  </>;
}
