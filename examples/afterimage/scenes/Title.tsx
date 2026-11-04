import { Easings, Rect, progress } from '@celesta.studio/react';
import { Ribbon } from '../components/Ribbon';
import { Type } from '../components/Type';
import { Margin } from '../components/Margin';
import { FPS, H, INK, RED, W } from '../constants';

export function Title({ f }: { f: number }) {
  const t = f / FPS;
  return <>
    <Ribbon time={t} x={1370} y={510} size={420} turn={0.5} />
    <Type x={65 - 110 * (1 - progress(f, 60, 22, Easings.easeOutQuart))} y={180} size={435} color={INK}>AFTER</Type>
    <Type x={510 + 280 * (1 - progress(f, 68, 25, Easings.easeOutQuart))} y={560} size={435} color={INK}>IMAGE</Type>
    <Rect x={72} y={560} width={340 * progress(f, 78, 28, Easings.easeOutQuart)} height={8} fill={RED} />
    <Type x={78} y={618} size={20} mono color={INK}>THE SHAPE OF</Type>
    <Type x={78} y={648} size={20} mono color={INK}>WHAT REMAINS.</Type>
    <Margin frame={f} />
    <Rect x={0} y={H * progress(f, 60, 14, Easings.easeOutQuart)} width={W} height={H * (1 - progress(f, 60, 14, Easings.easeOutQuart))} fill={INK} />
  </>;
}
