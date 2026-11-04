import type { CommonProps } from '@celesta.studio/react';
import { Path } from '@celesta.studio/shapes';
import { C } from './theme';

// The Δ mark: an equilateral triangle whose bounding box is centered on x/y.
export function trianglePoints(size: number): [number, number][] {
  const h = size * Math.sqrt(3) / 2;
  return [[0, -h / 2], [size / 2, h / 2], [-size / 2, h / 2]];
}

export function Delta({ size, fill = C.orange, stroke, strokeWidth, ...rest }:
  { size: number; fill?: string; stroke?: string; strokeWidth?: number } & Omit<CommonProps, 'anchorX' | 'anchorY'>) {
  return (
    <Path {...rest} points={trianglePoints(size)} closed join="round"
      fill={stroke ? undefined : fill} stroke={stroke ?? fill} strokeWidth={strokeWidth ?? size * 0.04} />
  );
}
