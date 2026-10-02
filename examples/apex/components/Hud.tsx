import { Group, Line, Rect, frameToTimecode, useCurrentFrame } from '@celesta/react';
import { Label } from './Label';
import { BONE, FPS, H, W } from '../constants';

// Global frame HUD: corner brackets, timecode, and a film-long progress line.
export function Hud({ total }: { total: number }) {
  const f = useCurrentFrame();
  const m = 40, L = 36;
  const corners: [number, number, number, number][] = [[m, m, 1, 1], [W - m, m, -1, 1], [m, H - m, 1, -1], [W - m, H - m, -1, -1]];
  return <Group blendMode="difference" opacity={0.9}>
    {corners.map(([x, y, dx, dy], i) => <Group key={i}>
      <Line x1={x} y1={y} x2={x + dx * L} y2={y} stroke={BONE} strokeWidth={2} cap="butt" />
      <Line x1={x} y1={y} x2={x} y2={y + dy * L} stroke={BONE} strokeWidth={2} cap="butt" />
    </Group>)}
    <Label x={W - 72} y={H - 62} size={20} mono color={BONE} anchorX={1}>{frameToTimecode(f, FPS)}</Label>
    <Rect x={m} y={H - 8} width={(W - 2 * m) * f / total} height={3} fill={BONE} />
  </Group>;
}
