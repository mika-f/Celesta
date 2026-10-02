import { Group, Rect } from '@celesta/react';
import { Ribbon } from '../components/Ribbon';
import { Type } from '../components/Type';
import { Margin } from '../components/Margin';
import { FPS, PAPER, RED } from '../constants';
import { clamp, ease } from '../math';

export function Study({ f }: { f: number }) {
  const t = f / FPS;
  return <>
    <Ribbon time={t} x={1270} y={540} size={430} color={PAPER} turn={0.3} />
    {['LIGHT', 'LEAVES', 'A MARK.'].map((word, i) => <Group key={word}
      opacity={ease((f - 180 - i * 9) / 10)} x={-55 * (1 - ease((f - 180 - i * 9) / 18))}>
      <Type x={76} y={206 + i * 222} size={255} color={i === 2 ? RED : PAPER}>{word}</Type>
    </Group>)}
    <Margin frame={f} color={PAPER} label="RETINAL PERSISTENCE" />
    <Rect x={82} y={936} width={460 * clamp((f - 180) / 120)} height={4} fill={RED} />
  </>;
}
