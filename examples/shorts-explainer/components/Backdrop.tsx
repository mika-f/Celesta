import { Circle } from '@celesta/shapes';
import { Group, Rect, useCurrentFrame } from '@celesta/react';
import { C, H, STAGE_Y, W } from '../constants';

// The whole-video background behind the panel and portraits: a sky-to-cream
// gradient, a dot grid drifting upward, and a soft floor.
export function Backdrop() {
  const frame = useCurrentFrame();
  const step = 72;
  const drift = (frame * 0.8) % step;
  const dots = [];
  for (let row = -1; row < H / step + 1; row++) {
    for (let col = 0; col < W / step + 1; col++) {
      const x = col * step + (row % 2 === 0 ? 0 : step / 2);
      const y = row * step - drift;
      dots.push(<Circle key={`${row}-${col}`} x={x} y={y} anchorX={0.5} anchorY={0.5} radius={4} fill={C.line} />);
    }
  }
  return (
    <>
      <Rect width={W} height={H} fill={{
        type: 'linear', start: { x: 0, y: 0 }, end: { x: 0, y: H },
        stops: [{ offset: 0.4, color: C.sky }, { offset: 0.7, color: '#EEF7FF' }, { offset: 1, color: C.cream }],
      }} />
      <Group>{dots}</Group>
      <Rect y={STAGE_Y + 160} width={W} height={H - STAGE_Y - 160} fill={{
        type: 'linear', start: { x: 0, y: 0 }, end: { x: 0, y: H - STAGE_Y - 160 },
        stops: [{ offset: 0, color: '#FFC93C00' }, { offset: 1, color: '#FFC93C55' }],
      }} />
    </>
  );
}
