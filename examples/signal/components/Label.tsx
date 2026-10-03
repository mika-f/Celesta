import { Text } from '@celesta/react';
import { BONE } from '../constants';

export function Label({ children, x, y, size, color = BONE, mono = false, weight = 400,
  opacity = 1, anchorX = 0 }: {
  children: string; x: number; y: number; size: number; color?: string;
  mono?: boolean; weight?: number; opacity?: number; anchorX?: number;
}) {
  return <Text x={x} y={y} anchorX={anchorX} opacity={opacity} style={{
    fontFamily: mono ? 'IBM Plex Mono' : 'Helvetica Neue', fontSize: size,
    fontWeight: weight, fill: { type: 'solid', color },
  }}>{children}</Text>;
}
