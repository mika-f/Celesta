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
  stroke?: { color: string; width: number };
  ax?: number;
  ay?: number | 'baseline';
  opacity?: number;
  scale?: number;
  align?: 'left' | 'center' | 'right';
  lineHeight?: number;
  maxWidth?: number;
};

export function Label({
  children, x, y, size, font = 'display', weight = 800, color = C.paper, stroke,
  ax = 0, ay = 0, opacity = 1, scale = 1, align = 'left', lineHeight, maxWidth,
}: LabelProps) {
  return (
    <Text x={x} y={y} anchorX={ax} anchorY={ay} opacity={opacity} scale={scale} maxWidth={maxWidth}
      style={{
        fontFamily: FONT[font], fontSize: size, fontWeight: weight, align, lineHeight,
        fill: { type: 'solid', color },
        stroke: stroke && { paint: { type: 'solid', color: stroke.color }, width: stroke.width },
      }}>
      {children}
    </Text>
  );
}
