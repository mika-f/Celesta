import { Rect } from '@celesta/react';
import { TAU } from '../constants';

// Equal-angle, unequal-length spokes make the form breathe rather than spin as a disc.
export function Rotor({ frame, x, y, color, size = 1 }: {
  frame: number; x: number; y: number; color: string; size?: number;
}) {
  return <>{Array.from({ length: 72 }, (_, i) => {
    const a = i / 72 * TAU + frame * 0.019;
    const radius = size * (125 + 81 * Math.sin(i * 0.52 - frame * 0.12));
    const length = size * (110 + 90 * Math.sin(i * 0.27 + frame * 0.11) ** 2);
    return <Rect key={i} x={x + Math.cos(a) * radius}
      y={y + Math.sin(a) * radius} anchorY={0.5}
      rotation={a * 180 / Math.PI} width={length} height={i % 6 === 0 ? 7 : 2}
      fill={color} opacity={i % 6 === 0 ? 1 : 0.54} />;
  })}
    <Rect x={x} y={y} anchorX={0.5} anchorY={0.5}
      width={112 * size} height={112 * size} fill={color}
      rotation={-frame * 2.7} />
  </>;
}
