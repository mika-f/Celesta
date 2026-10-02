import { Group } from '@celesta/react';
import { Line } from './Line';
import { RED } from '../constants';

// A moving stack of triangular apertures, projected with perspective.
// The same motif travels from the opening through code, motion, and export.
export function Prism({ frame, x = 960, y = 540, size = 320, color = RED, count = 28 }: {
  frame: number; x?: number; y?: number; size?: number; color?: string; count?: number;
}) {
  return <>{Array.from({ length: count }, (_, i) => {
    const depth = i / count;
    const angle = frame * 0.009 + depth * 1.2;
    const r = size * (0.22 + depth * 0.78);
    const points = Array.from({ length: 3 }, (_, j) => {
      const a = angle + j * Math.PI * 2 / 3 - Math.PI / 2;
      const z = Math.sin(a) * Math.sin(frame * 0.006) * 0.4;
      return [x + Math.cos(a) * r / (1 - z),
        y + Math.sin(a) * r * 0.84 / (1 - z) + (depth - 0.5) * size * 0.25];
    });
    return <Group key={i}>{points.map((a, j) => <Line key={j} a={a} b={points[(j + 1) % 3]}
      color={color} width={i % 7 === 0 ? 4 : 1.6} opacity={0.25 + depth * 0.75} />)}</Group>;
  })}</>;
}
