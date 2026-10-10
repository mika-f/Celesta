import { Text } from '@celesta/react';
import { CYAN, SAFE, W, WHITE } from '../constants';

// The only words: a line that reads the same from any point in the loop, and
// the mark. Centred and narrow, they keep clear of the platforms' UI (SAFE)
// while the picture fills the frame. They never move, so their rasters are
// made once; the lens ring still bends them as it passes.

function Type({ children, y, size, color, mono = false, spacing = 0, opacity = 1 }: {
  children: string; y: number; size: number; color: string; mono?: boolean; spacing?: number; opacity?: number;
}) {
  return <Text x={W / 2} y={y} anchorX={0.5} anchorY={0.5} opacity={opacity} style={{
    fontFamily: mono ? 'IBM Plex Mono' : 'Bebas Neue', fontSize: size, letterSpacing: spacing,
    align: 'center', fill: { type: 'solid', color },
  }}>{children}</Text>;
}

export function Copy() {
  return <>
    <Type y={SAFE.top + 110} size={150} color={WHITE} spacing={6}>NO START.</Type>
    <Type y={SAFE.top + 240} size={150} color={WHITE} spacing={6}>NO END.</Type>
    <Type y={SAFE.bottom - 88} size={60} color={WHITE} spacing={22} opacity={0.92}>CELESTA</Type>
    <Type y={SAFE.bottom - 36} size={24} color={CYAN} mono spacing={2} opacity={0.8}>@celesta/shader · WGSL</Type>
  </>;
}
