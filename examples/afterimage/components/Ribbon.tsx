import { Group } from '@celesta/react';
import { RED } from '../constants';
import { clamp } from '../math';
import { Line } from './Line';

// A half-twist ribbon, projected from 3D. Each strand runs twice around
// the surface, closing without a seam. Depth controls ink density.
export function Ribbon({ time, x = 960, y = 540, size = 350, color = RED,
  turn = 0, opacity = 1, strands = 13 }: {
  time: number; x?: number; y?: number; size?: number; color?: string;
  turn?: number; opacity?: number; strands?: number;
}) {
  const yaw = time * 0.23 + turn;
  const pitch = 0.85 + Math.sin(time * 0.19) * 0.35;
  const project = (u: number, v: number) => {
    const radius = 1 + v * Math.cos(u / 2);
    const px = radius * Math.cos(u), py = radius * Math.sin(u), pz = v * Math.sin(u / 2);
    const xx = px * Math.cos(yaw) + pz * Math.sin(yaw);
    const zz = -px * Math.sin(yaw) + pz * Math.cos(yaw);
    const yy = py * Math.cos(pitch) - zz * Math.sin(pitch);
    const depth = py * Math.sin(pitch) + zz * Math.cos(pitch);
    const perspective = 3.6 / (3.6 - depth);
    return [x + size * xx * perspective, y + size * yy * perspective, depth];
  };
  const segments = [];
  for (let strand = 0; strand < strands; strand++) {
    const v = 0.035 + strand / (strands - 1) * 0.47;
    for (let i = 0; i < 112; i++) {
      const a = project(i / 112 * Math.PI * 4, v);
      const b = project((i + 1) / 112 * Math.PI * 4, v);
      segments.push({ a, b, depth: (a[2] + b[2]) / 2, key: `${strand}-${i}` });
    }
  }
  segments.sort((a, b) => a.depth - b.depth);
  return <Group opacity={opacity}>{segments.map(s => <Line key={s.key}
    a={s.a} b={s.b} width={1.5 + clamp((s.depth + 1) / 2) * 0.8}
    color={color} opacity={0.25 + clamp((s.depth + 1.4) / 2.8) * 0.75} />)}</Group>;
}
