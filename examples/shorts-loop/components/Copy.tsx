import { Group, Text } from '@celesta/react';
import { BAR, CYAN, MAGENTA, SAFE, W, WHITE } from '../constants';
import { within } from '../loop';
import { sheen } from '../shaders';

// The only words: a line that names what the picture does, and the mark.
// Centred and narrow, they keep clear of the platforms' UI (SAFE) while the
// picture fills the frame. The letters never move, so their rasters are made
// once; `sheen` runs light across the line once a bar, and the lens ring
// bends everything as it passes.

function Type({ children, y, size, color, mono = false, spacing = 0, opacity = 1 }: {
  children: string; y: number; size: number; color: string; mono?: boolean; spacing?: number; opacity?: number;
}) {
  return <Text x={W / 2} y={y} anchorX={0.5} anchorY={0.5} opacity={opacity} style={{
    fontFamily: mono ? 'IBM Plex Mono' : 'Bebas Neue', fontSize: size, letterSpacing: spacing,
    align: 'center', fill: { type: 'solid', color },
  }}>{children}</Text>;
}

export function Copy({ frame, pulse }: { frame: number; pulse: number }) {
  // The band starts and ends clear of the letters, so its restart each bar is unseen.
  const sweep = -0.35 + 1.9 * within(frame, BAR);
  return <>
    <Group shader={sheen({ sweep, split: 1 + 4 * pulse, tint: MAGENTA })}>
      <Type y={SAFE.top + 110} size={160} color={WHITE} spacing={8}>LIGHT,</Type>
      <Type y={SAFE.top + 245} size={160} color={WHITE} spacing={8}>FOLDED.</Type>
    </Group>
    <Type y={SAFE.bottom - 88} size={60} color={WHITE} spacing={22} opacity={0.92}>CELESTA</Type>
    <Type y={SAFE.bottom - 36} size={24} color={CYAN} mono spacing={2} opacity={0.8}>@celesta/shader · WGSL</Type>
  </>;
}
