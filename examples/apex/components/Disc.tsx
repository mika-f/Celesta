import { Rect } from '@celesta/react';

export function Disc({ x, y, r, color, opacity = 1 }: { x: number; y: number; r: number; color: string; opacity?: number }) {
  return <Rect x={x} y={y} anchorX={0.5} anchorY={0.5} width={2 * r} height={2 * r}
    cornerRadius={r} fill={color} opacity={opacity} />;
}
