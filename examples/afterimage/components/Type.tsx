import { Text } from '@celesta/react';
import { PAPER } from '../constants';

export function Type({ children, x = 0, y = 0, size = 100, color = PAPER,
  mono = false, center = false, opacity = 1, scaleX = 1,
}: { children: string; x?: number; y?: number; size?: number; color?: string;
  mono?: boolean; center?: boolean; opacity?: number; scaleX?: number }) {
  return <Text x={x} y={y} anchorX={center ? 0.5 : 0} anchorY={center ? 0.5 : 0}
    opacity={opacity} scaleX={scaleX} style={{
      fontFamily: mono ? 'IBM Plex Mono' : 'Bebas Neue', fontSize: size,
      fill: { type: 'solid', color },
    }}>{children}</Text>;
}
