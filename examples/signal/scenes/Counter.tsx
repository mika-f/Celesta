import { Rect } from '@celesta/react';
import { Label } from '../components/Label';
import { ACID, BONE, GREY, H, INK, W } from '../constants';
import { ease } from '../math';

export function Counter({ f }: { f: number }) {
  const local = f;
  return <>
    <Rect width={W} height={H} fill={INK} />
    <Label x={74} y={58} size={21} mono color={GREY}>CELESTA / FRAME 000</Label>
    <Label x={1555} y={58} size={21} mono color={GREY}>30 FPS / 16 S</Label>
    <Rect x={72} y={153} width={1776} height={3} fill={BONE} />
    <Label x={60} y={165} size={401} mono color={BONE}>
      {String(Math.min(f * 7, 9999)).padStart(4, '0')}
    </Label>
    {Array.from({ length: 44 }, (_, i) => {
      const h = 110 + Math.abs(Math.sin(i * 0.4 + local * 0.14)) * 230;
      return <Rect key={i} x={72 + i * 40} y={750 - h / 2}
        width={i % 4 === 0 ? 14 : 6} height={h} fill={i % 4 === 0 ? ACID : GREY}
        opacity={ease((f - i * 0.4) / 24)} />;
    })}
    <Rect x={72} y={962} width={1776} height={2} fill={GREY} />
    <Label x={74} y={982} size={23} mono color={ACID}>SOURCE → FRAME</Label>
    <Label x={1589} y={982} size={23} mono color={GREY}>00:{String(f).padStart(2, '0')}</Label>
  </>;
}
