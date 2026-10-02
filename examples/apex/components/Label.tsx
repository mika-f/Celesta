import { Text } from '@celesta/react';
import { BONE } from '../constants';

export type LabelProps = {
  children: string; x: number; y: number; size: number; color?: string; mono?: boolean;
  anchorX?: number; anchorY?: number; opacity?: number; outline?: boolean; blendMode?: 'add' | 'normal';
  scale?: number; letterSpacing?: number;
};
export function Label({ children, x, y, size, color = BONE, mono = false, anchorX = 0, anchorY = 0,
  opacity = 1, outline = false, blendMode = 'normal', scale = 1, letterSpacing = 0 }: LabelProps) {
  return <Text x={x} y={y} anchorX={anchorX} anchorY={anchorY} opacity={opacity}
    blendMode={blendMode} scale={scale} style={{
      fontFamily: mono ? 'IBM Plex Mono' : 'Bebas Neue', fontSize: size, letterSpacing,
      ...(outline ? { stroke: { paint: { type: 'solid', color }, width: 3 } }
        : { fill: { type: 'solid', color } }),
    }}>{children}</Text>;
}
