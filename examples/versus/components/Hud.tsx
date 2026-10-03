import { Easings, Group, Rect, useCurrentFrame, useVideoConfig } from '@celesta/react';
import { C, ORDER, TOOL, W, H } from '../constants';
import { progress } from '../helpers';
import { Label } from './Label';

// A panel led by the three tool colors sweeps left to right; place it so its
// eighth frame (fully covering the screen) lands on the cut.
export function Wipe() {
  const f = useCurrentFrame();
  const p = progress(f, 0, 16, Easings.easeInOutQuart);
  const x = -W - 180 + p * (2 * W + 360);
  return (
    <Group x={x}>
      <Rect width={W} height={H} fill={C.panel2} />
      {ORDER.map((id, i) => (
        <Rect key={id} x={W + i * 60} width={60} height={H} fill={TOOL[id].color} />
      ))}
      {ORDER.map((id, i) => (
        <Rect key={`t-${id}`} x={-60 - i * 60} width={60} height={H} fill={TOOL[ORDER[2 - i]].color} />
      ))}
    </Group>
  );
}

// Thin frame around the whole film: title, section and a progress line.
export function Hud({ sections }: { sections: { name: string; from: number }[] }) {
  const f = useCurrentFrame();
  const { durationInFrames } = useVideoConfig();
  const index = sections.reduce((found, s, i) => (f >= s.from ? i : found), 0);
  const on = progress(f, 20, 20) * (1 - progress(f, durationInFrames - 40, 20));
  return (
    <Group opacity={on}>
      <Label x={W - 96} y={36} ax={1} size={14} font="mono" weight={700} color={C.grey} spacing={3}>
        {`CELESTA × REMOTION × FFRAMES  ·  ${sections[index].name.toUpperCase()}`}
      </Label>
      <Rect x={0} y={H - 4} width={W} height={4} fill="#FFFFFF10" />
      <Rect x={0} y={H - 4} width={Math.max(1, (W * f) / durationInFrames)} height={4} fill={TOOL.celesta.color} />
    </Group>
  );
}
