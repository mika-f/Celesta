import { Text } from '@celesta/react';
import { C, FONT } from '../constants';

export type LabelProps = {
  children: string;
  x?: number;
  y?: number;
  size: number;
  font?: keyof typeof FONT;
  weight?: number;
  color?: string;
  ax?: number;
  ay?: number | 'baseline';
  opacity?: number;
  scale?: number;
  align?: 'left' | 'center' | 'right';
  lineHeight?: number;
  spacing?: number;
};

export function Label({
  children, x = 0, y = 0, size, font = 'sans', weight = 700, color = C.ink,
  ax = 0, ay = 0, opacity = 1, scale = 1, align = 'left', lineHeight, spacing,
}: LabelProps) {
  return (
    <Text x={x} y={y} anchorX={ax} anchorY={ay} opacity={opacity} scale={scale}
      style={{
        fontFamily: FONT[font], fontSize: size, fontWeight: weight, align, lineHeight,
        letterSpacing: spacing, fill: { type: 'solid', color },
      }}>
      {children}
    </Text>
  );
}
