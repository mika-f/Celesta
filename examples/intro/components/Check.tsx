import { Path } from '@celesta/shapes';
import { C } from '../constants';

// A check mark drawn as a stroke, `size` px square from (x, y).
export function Check({ x, y, size = 48, color = C.accent }: { x: number; y: number; size?: number; color?: string }) {
  return (
    <Path x={x} y={y} points={[[size * 0.08, size * 0.55], [size * 0.38, size * 0.85], [size * 0.92, size * 0.15]]}
      stroke={color} strokeWidth={Math.max(2, size / 9)} cap="square" join="miter" />
  );
}
