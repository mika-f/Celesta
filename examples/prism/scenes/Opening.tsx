import { lerp } from '@celesta.studio/math';
import { Easings, Rect, progress, useCurrentFrame } from '@celesta.studio/react';
import { Label } from '../components/Label';
import { Prism } from '../components/Prism';
import { H, INK, PAPER, RED, W } from '../constants';

export function Opening() {
  const f = useCurrentFrame();
  return <>
    <Prism frame={f} y={480} size={340} color={PAPER} />
    <Rect width={lerp(960, 0, progress(f, 0, 44, Easings.easeOutQuart))} height={H} fill={INK} />
    <Rect x={lerp(960, W, progress(f, 0, 44, Easings.easeOutQuart))} width={960} height={H} fill={INK} />
    <Rect x={958} y={300} width={4} height={480} fill={RED} opacity={1 - progress(f, 0, 50, Easings.easeOutQuart)} />
    <Label x={960} y={925} size={22} mono center opacity={progress(f, 18, 25, Easings.easeOutQuart)}>AN IDEA IS ONLY THE BEGINNING.</Label>
    <Label x={80} y={58} size={20} mono>CELESTA / MOTION STUDY 001</Label>
  </>;
}
