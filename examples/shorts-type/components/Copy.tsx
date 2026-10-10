import type { ReactNode } from 'react';
import type { TextStyle } from '@celesta/react';
import { Text } from '@celesta/react';
import { C, FONT } from '../constants';

export const textStyle = (size: number, weight = 900, color: string = C.paper, font: keyof typeof FONT = 'ja'): TextStyle => ({
  fontFamily: FONT[font], fontSize: size, fontWeight: weight, fill: { type: 'solid', color },
});

export type CopyProps = {
  children: ReactNode;
  x?: number;
  y?: number;
  size: number;
  weight?: number;
  color?: string;
  font?: keyof typeof FONT;
  ax?: number;
  ay?: number | 'baseline';
  opacity?: number;
  scale?: number;
  lineHeight?: number;
  // Wraps between Japanese phrases (文節) at this width.
  maxWidth?: number;
};

export function Copy({ children, x = 0, y = 0, size, weight = 900, color = C.paper, font = 'ja',
  ax = 0, ay = 0, opacity = 1, scale, lineHeight, maxWidth }: CopyProps) {
  return (
    <Text x={x} y={y} anchorX={ax} anchorY={ay} opacity={opacity} scale={scale} maxWidth={maxWidth}
      style={{ ...textStyle(size, weight, color, font), lineHeight, lineBreak: 'phrase' }}>
      {children}
    </Text>
  );
}

// A line of code-ish annotation: which API the copy above is showing.
export function Tag({ children, x = 0, y = 0, color = C.grey, opacity = 1 }:
  { children: string; x?: number; y?: number; color?: string; opacity?: number }) {
  return <Copy x={x} y={y} size={26} weight={500} font="mono" color={color} opacity={opacity}>{children}</Copy>;
}
