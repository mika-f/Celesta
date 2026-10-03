import { Rect, useCurrentFrame } from '@celesta/react';
import { Label } from '../components/Label';
import { Prism } from '../components/Prism';
import { H, INK, PAPER, W } from '../constants';
import { clamp } from '../math';

export function Closing() {
  const f = useCurrentFrame();
  return <>
    <Rect width={W} height={H} fill={PAPER} />
    <Prism frame={f + 840} x={1580} y={580} size={500} count={32} />
    <Label x={80} y={65} size={20} mono color={INK}>PRISM / MADE WITH CELESTA</Label>
    <Label x={71} y={267} size={455} color={INK}>CELESTA</Label>
    <Label x={84} y={759} size={94} color={INK}>MAKE YOUR IDEAS MOVE.</Label>
    <Label x={84} y={960} size={22} mono color={INK}>YOUR SOURCE. YOUR VISION. YOUR NEXT FILM.</Label>
    <Rect width={W} height={H} fill={INK} opacity={clamp((f - 102) / 17)} />
  </>;
}
