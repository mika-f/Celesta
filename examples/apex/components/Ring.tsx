import { Rect } from '@celesta/react';
import { H, W } from '../constants';

export function Ring({ r, color, opacity = 1, x = W / 2, y = H / 2, width = 3 }: {
  r: number; color: string; opacity?: number; x?: number; y?: number; width?: number;
}) {
  return <Rect x={x} y={y} anchorX={0.5} anchorY={0.5} width={2 * r} height={2 * r}
    cornerRadius={r} stroke={color} strokeWidth={width} opacity={opacity} />;
}
