import { Text } from '@celesta/react';
import { C, FONT } from '../constants';

export type LabelProps = {
  children: string;
  x: number;
  y: number;
  size: number;
  font?: keyof typeof FONT;
  weight?: number;
  color?: string;
  ax?: number;
  ay?: number | 'baseline';
  opacity?: number;
  scale?: number;
  align?: 'left' | 'center' | 'right';
};

// One line of text in one of the two fonts.
export function Label({
  children, x, y, size, font = 'mono', weight = 400, color = C.fg,
  ax = 0, ay = 0, opacity = 1, scale = 1, align = 'left',
}: LabelProps) {
  return (
    <Text x={x} y={y} anchorX={ax} anchorY={ay} opacity={opacity} scale={scale}
      style={{ fontFamily: FONT[font], fontSize: size, fontWeight: weight, align, fill: { type: 'solid', color } }}>
      {children}
    </Text>
  );
}
