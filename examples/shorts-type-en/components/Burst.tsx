import { Easings, progress } from '@celesta/react';
import { random } from '@celesta/math';
import { Path } from '@celesta/shapes';

// Speed lines flying out from a point over one beat: thin wedges, one path.
// `seed` picks a new set of lengths and angles for every beat.
export function Burst({ x, y, t, color, seed }: { x: number; y: number; t: number; color: string; seed: number }) {
  const out = progress(t, 0, 1, Easings.easeOutCubic);
  const commands = Array.from({ length: 30 }, (_, i) => {
    const angle = ((i + random(`${seed}:a:${i}`) * 0.6) / 30) * Math.PI * 2;
    const inner = 260 + 520 * out + 160 * random(`${seed}:r:${i}`);
    const outer = inner + (200 + 420 * random(`${seed}:l:${i}`)) * (1 - out * 0.7);
    const spread = 0.018;
    const at = (r: number, a: number) => ({ x: Math.cos(a) * r, y: Math.sin(a) * r });
    return [
      { type: 'moveTo' as const, ...at(inner, angle) },
      { type: 'lineTo' as const, ...at(outer, angle - spread) },
      { type: 'lineTo' as const, ...at(outer, angle + spread) },
      { type: 'close' as const },
    ];
  }).flat();
  return <Path x={x} y={y} commands={commands} fill={color} opacity={1 - out} />;
}
