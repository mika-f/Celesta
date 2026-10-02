import { Rect, useCurrentFrame } from '@celesta/react';
import { Label } from '../components/Label';
import { Prism } from '../components/Prism';
import { H, INK, PAPER, RED, W } from '../constants';
import { ease, lerp } from '../math';

export function Opening() {
  const f = useCurrentFrame();
  return <>
    <Prism frame={f} y={480} size={340} color={PAPER} />
    <Rect width={lerp(960, 0, ease(f / 44))} height={H} fill={INK} />
    <Rect x={lerp(960, W, ease(f / 44))} width={960} height={H} fill={INK} />
    <Rect x={958} y={300} width={4} height={480} fill={RED} opacity={1 - ease(f / 50)} />
    <Label x={960} y={925} size={22} mono center opacity={ease((f - 18) / 25)}>AN IDEA IS ONLY THE BEGINNING.</Label>
    <Label x={80} y={58} size={20} mono>CELESTA / MOTION STUDY 001</Label>
  </>;
}
