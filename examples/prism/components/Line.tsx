import { Rect } from '@celesta/react';
import { PAPER } from '../constants';

export function Line({ a, b, color = PAPER, width = 2, opacity = 1 }: {
  a: number[]; b: number[]; color?: string; width?: number; opacity?: number;
}) {
  return <Rect x={a[0]} y={a[1]} width={Math.max(0.1, Math.hypot(b[0] - a[0], b[1] - a[1]))}
    height={width} anchorY={0.5} rotation={Math.atan2(b[1] - a[1], b[0] - a[0]) * 180 / Math.PI}
    fill={color} opacity={opacity} />;
}
