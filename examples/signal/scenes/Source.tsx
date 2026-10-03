import { Rect } from '@celesta/react';
import { Label } from '../components/Label';
import { Signal } from '../components/Signal';
import { ACID, BLUE, BONE, GREY, H, INK, W } from '../constants';
import { ease } from '../math';

export function Source({ f }: { f: number }) {
  const local = f - 60;
  const enter = ease(local / 14);
  return <>
    <Rect width={W} height={H} fill={BONE} />
    <Rect x={0} y={0} width={W * (1 - enter)} height={H} fill={INK} />
    <Label x={77} y={51} size={22} mono color={INK}>01 / THE SOURCE</Label>
    <Label x={1431} y={51} size={22} mono color={INK}>FILM.TSX ↗</Label>
    <Rect x={75} y={121} width={1770} height={3} fill={INK} />
    <Label x={67} y={186} size={131} mono color={INK} opacity={ease((local - 7) / 14)}>
      useCurrentFrame()
    </Label>
    <Label x={77} y={405} size={26} mono color={INK}>const f = useCurrentFrame();</Label>
    <Label x={77} y={448} size={26} mono color={BLUE}>
      {'y = Math.sin(f * 0.14 + i * 0.21) * amplitude;'}
    </Label>
    <Rect x={74} y={535} width={1772} height={410} fill={INK} />
    <Signal frame={f} top={737} color={ACID} count={96} amplitude={110} />
    <Label x={81} y={895} size={18} mono color={GREY}>RESULT / LIVE PREVIEW</Label>
    <Rect x={74} y={978} width={1772} height={2} fill={INK} />
    <Label x={77} y={995} size={19} mono color={INK}>EDIT THE VALUE. WATCH THE FRAME CHANGE.</Label>
  </>;
}
