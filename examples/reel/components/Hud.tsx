import { Group, Rect, useCurrentFrame } from '@celesta/react';
import { Label } from './Label';
import { C, DURATION, FPS, H, W } from '../constants';
import { timecode } from '../helpers';
import { S } from '../timeline';

const SECTIONS: [number, string][] = [
  [S.intro, 'COLD OPEN'],
  [S.words, 'MANIFESTO'],
  [S.field, 'DETERMINISTIC'],
  [S.code, 'CODE'],
  [S.timeline, 'TIMELINE'],
  [S.exportAt, 'EXPORT'],
];

// Drawn in paper and blended with `difference`, so it reads light over the
// dark scenes and dark over the paper ones without tracking which is which.
export function Hud() {
  const f = useCurrentFrame();
  const fg = C.paper;
  const section = [...SECTIONS].reverse().find(([at]) => f >= at)?.[1] ?? '';
  const m = 56;
  const arm = 26;
  const corner = (x: number, y: number, sx: number, sy: number) => (
    <Group key={`${x}-${y}`} x={x} y={y}>
      <Rect x={sx < 0 ? -arm : 0} width={arm} height={2} fill={fg} />
      <Rect y={sy < 0 ? -arm : 0} width={2} height={arm} fill={fg} />
    </Group>
  );
  return (
    <>
      <Group opacity={0.75} blendMode="difference">
        {corner(m, m, 1, 1)}
        {corner(W - m, m, -1, 1)}
        {corner(m, H - m, 1, -1)}
        {corner(W - m, H - m, -1, -1)}
        <Label x={m + 40} y={m + 12} size={18} font="mono" weight={700} color={fg} ay={0.5}>Celesta</Label>
        <Label x={m + 136} y={m + 12} size={18} font="mono" weight={400} color={fg} ay={0.5} opacity={0.6}>/ Reel 01</Label>
        <Label x={W - m - 40} y={m + 12} size={18} font="mono" weight={400} color={fg} ax={1} ay={0.5}>{timecode(f)}</Label>
        <Label x={m + 40} y={H - m - 12} size={18} font="mono" weight={400} color={fg} ay={0.5}>{section}</Label>
        <Label x={W - m - 40} y={H - m - 12} size={18} font="mono" weight={400} color={fg} ax={1} ay={0.5}>
          {`${W}×${H} · ${FPS} FPS · 120 BPM`}
        </Label>
      </Group>
      <Rect x={m} y={H - 26} width={(W - m * 2) * (f / (DURATION - 1))} height={2} fill={C.accent} opacity={0.75} />
    </>
  );
}
