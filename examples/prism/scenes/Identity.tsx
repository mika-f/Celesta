import { Rect, useCurrentFrame } from '@celesta/react';
import { Label } from '../components/Label';
import { Prism } from '../components/Prism';
import { Rails } from '../components/Rails';
import { H, INK, PAPER, RED, W } from '../constants';
import { ease } from '../math';

export function Identity() {
  const f = useCurrentFrame();
  return <>
    <Rect width={W} height={H} fill={PAPER} />
    <Prism frame={f + 60} x={1540} y={528} size={440} />
    <Label x={70 - 180 * (1 - ease(f / 22))} y={185} size={450} color={INK}>CELESTA</Label>
    <Rect x={80} y={659} width={160 * ease((f - 8) / 22)} height={10} fill={RED} />
    <Label x={80} y={724} size={105} color={INK} opacity={ease((f - 14) / 18)}>MAKE YOUR IDEAS MOVE.</Label>
    <Label x={84} y={872} size={23} mono color={INK}>CODE-FIRST VIDEO. FRAME BY FRAME.</Label>
    <Rails chapter="00 / A NEW PERSPECTIVE" light />
  </>;
}
