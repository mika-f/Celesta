import { Path } from '@celesta/shapes';
import { C } from '../constants';

// A keyframe marker: a filled rhombus centered on (x, y).
export function Diamond({ x, y, size = 16, color = C.fg, opacity = 1 }: {
  x: number; y: number; size?: number; color?: string; opacity?: number;
}) {
  const r = size / 2;
  return <Path x={x} y={y} opacity={opacity} points={[[0, -r], [r, 0], [0, r], [-r, 0]]} closed fill={color} />;
}
