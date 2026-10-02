import { Rect } from '@celesta/react';

export function Line({ a, b, color, width = 2, opacity = 1 }: {
  a: number[]; b: number[]; color: string; width?: number; opacity?: number;
}) {
  const dx = b[0] - a[0], dy = b[1] - a[1];
  return <Rect x={a[0]} y={a[1]} width={Math.hypot(dx, dy) + 0.7} height={width}
    anchorY={0.5} rotation={Math.atan2(dy, dx) * 180 / Math.PI}
    fill={color} opacity={opacity} />;
}
