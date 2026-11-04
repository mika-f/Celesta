import { Easings, Rect, progress } from '@celesta.studio/react';
import { Label } from '../components/Label';
import { Signal } from '../components/Signal';
import { ACID, H, INK, W } from '../constants';

export function Title({ f }: { f: number }) {
  const local = f - 390;
  const reveal = progress(local, 0, 18, Easings.easeOutCubic);
  const fade = progress(f, 466, 14);
  return <>
    <Rect width={W} height={H} fill={ACID} />
    <Rect x={0} y={0} width={W * (1 - reveal)} height={H} fill={INK} />
    <Label x={75} y={75} size={23} mono color={INK}>CODE / MOTION / FILM</Label>
    <Label x={61} y={290} size={258} color={INK} weight={700} opacity={progress(local, 6, 15, Easings.easeOutCubic)}>
      CELESTA
    </Label>
    <Rect x={75} y={650} width={1770 * progress(local, 16, 27, Easings.easeOutCubic)} height={12} fill={INK} />
    <Label x={79} y={721} size={37} mono color={INK}
      opacity={progress(local, 25, 15, Easings.easeOutCubic)}>A CODE-FIRST VIDEO TOOL.</Label>
    <Signal frame={f} top={925} color={INK} count={92} amplitude={44} />
    <Rect width={W} height={H} fill={INK} opacity={fade} />
  </>;
}
