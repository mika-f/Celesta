import { Group, Rect } from '@celesta/react';
import { Ribbon } from '../components/Ribbon';
import { Type } from '../components/Type';
import { FPS, H, INK, RED, W } from '../constants';
import { clamp, ease, mix } from '../math';

export function Outro({ f }: { f: number }) {
  const t = f / FPS;
  return <>
    <Ribbon time={t * 0.6} x={960} y={470} size={260} opacity={0.55 * (1 - ease((f - 652) / 55))} />
    <Group opacity={ease((f - 579) / 24)}>
      <Type x={960} y={545} center size={350}>AFTERIMAGE</Type>
      <Rect x={mix(960, 78, ease((f - 592) / 35))} y={765}
        width={1764 * ease((f - 592) / 35)} height={3} fill={RED} />
      <Type x={960} y={816} center mono size={21}>SOME THINGS STAY.</Type>
      <Type x={960} y={1008} center mono size={16} color="#85837B">AN INDEPENDENT MOTION STUDY / 2026</Type>
    </Group>
    <Rect width={W} height={H} fill={INK} opacity={clamp((f - 699) / 20)} />
  </>;
}
