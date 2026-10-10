import type { ReactNode } from 'react';
import type { TextStyle } from '@celesta/react';
import { Text } from '@celesta/react';
import { C, FONT } from '../constants';

export const textStyle = (size: number, font: keyof typeof FONT = 'display', color: string = C.paper, weight = 400): TextStyle => ({
  fontFamily: FONT[font], fontSize: size, fontWeight: weight, fill: { type: 'solid', color },
});

export type CopyProps = {
  children: ReactNode;
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
  lineHeight?: number;
  maxWidth?: number;
  // letterSpacing, in pixels.
  tracking?: number;
};

export function Copy({ children, x = 0, y = 0, size, font = 'body', weight = 400, color = C.paper,
  ax = 0, ay = 0, opacity = 1, scale, lineHeight, maxWidth, tracking }: CopyProps) {
  return (
    <Text x={x} y={y} anchorX={ax} anchorY={ay} opacity={opacity} scale={scale} maxWidth={maxWidth}
      style={{ ...textStyle(size, font, color, weight), lineHeight, letterSpacing: tracking }}>
      {children}
    </Text>
  );
}

// A line of code-ish annotation: which API the copy above is showing.
export function Tag({ children, x = 0, y = 0, color = C.grey, opacity = 1 }:
  { children: string; x?: number; y?: number; color?: string; opacity?: number }) {
  return <Copy x={x} y={y} size={26} weight={500} font="mono" color={color} opacity={opacity}>{children}</Copy>;
}
