import type { ReactNode } from 'react';
import { Text } from '@celesta/react';
import { C, FONT } from '../constants';

export type LabelProps = {
  children: ReactNode;
  x: number;
  y: number;
  size: number;
  font?: keyof typeof FONT;
  weight?: number;
  color?: string;
  stroke?: { color: string; width: number };
  ax?: number;
  ay?: number | 'baseline';
  opacity?: number;
  scale?: number;
  rotation?: number;
  align?: 'left' | 'center' | 'right';
  lineHeight?: number;
  maxWidth?: number;
  letterSpacing?: number;
};

// One styled Text layer. Children are strings or <Span>s.
export function Label({
  children, x, y, size, font = 'ja', weight = 800, color = C.ink, stroke,
  ax = 0, ay = 0, opacity = 1, scale = 1, rotation = 0, align = 'left', lineHeight, maxWidth, letterSpacing,
}: LabelProps) {
  return (
    <Text x={x} y={y} anchorX={ax} anchorY={ay} opacity={opacity} scale={scale} rotation={rotation} maxWidth={maxWidth}
      style={{
        fontFamily: FONT[font], fontSize: size, fontWeight: weight, align, lineHeight, letterSpacing,
        lineBreak: 'phrase',
        fill: { type: 'solid', color },
        stroke: stroke && { paint: { type: 'solid', color: stroke.color }, width: stroke.width },
      }}>
      {children}
    </Text>
  );
}
