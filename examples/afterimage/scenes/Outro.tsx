import { lerp } from '@celesta.studio/math';
import { Easings, Group, Rect, progress } from '@celesta.studio/react';
import { Ribbon } from '../components/Ribbon';
import { Type } from '../components/Type';
import { FPS, H, INK, RED, W } from '../constants';

export function Outro({ f }: { f: number }) {
  const t = f / FPS;
  return <>
    <Ribbon time={t * 0.6} x={960} y={470} size={260} opacity={0.55 * (1 - progress(f, 652, 55, Easings.easeOutQuart))} />
    <Group opacity={progress(f, 579, 24, Easings.easeOutQuart)}>
      <Type x={960} y={545} center size={350}>AFTERIMAGE</Type>
      <Rect x={lerp(960, 78, progress(f, 592, 35, Easings.easeOutQuart))} y={765}
        width={1764 * progress(f, 592, 35, Easings.easeOutQuart)} height={3} fill={RED} />
      <Type x={960} y={816} center mono size={21}>SOME THINGS STAY.</Type>
      <Type x={960} y={1008} center mono size={16} color="#85837B">AN INDEPENDENT MOTION STUDY / 2026</Type>
    </Group>
    <Rect width={W} height={H} fill={INK} opacity={progress(f, 699, 20)} />
  </>;
}
