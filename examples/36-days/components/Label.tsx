import type { TextStyle } from '@celesta/react';
import { Text } from '@celesta/react';
import { C, FONT } from '../constants';

export const textStyle = (font: keyof typeof FONT, size: number, color: string = C.paper, weight = 400): TextStyle => ({
  fontFamily: FONT[font], fontSize: size, fontWeight: weight, fill: { type: 'solid', color },
});

export type LabelProps = {
  children: string | number;
  x?: number;
  y?: number;
  size: number;
  font?: keyof typeof FONT;
  weight?: number;
  color?: string;
  ax?: number;
  ay?: number | 'baseline';
  opacity?: number;
  lineHeight?: number;
};

export function Label({ children, x = 0, y = 0, size, font = 'display', weight = 400, color = C.paper,
  ax = 0, ay = 0, opacity = 1, lineHeight }: LabelProps) {
  return (
    <Text x={x} y={y} anchorX={ax} anchorY={ay} opacity={opacity}
      style={{ ...textStyle(font, size, color, weight), lineHeight }}>
      {children}
    </Text>
  );
}
