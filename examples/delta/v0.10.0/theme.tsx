import type { ReactNode } from 'react';
import { Theme } from '@celesta.studio/react';
import { C, FONT } from '../components/theme';

// The DELTA look as a <Theme>: the film's font stacks and colors by name,
// with the rack's metal and the LED's lit and unlit greens on top.
export const DELTA_FONTS = { display: FONT.display, sans: FONT.sans, mono: FONT.mono };
export const DELTA_COLORS = { ...C, metal: '#1B1D22', rail: '#2C2F36', hole: '#08090B', led: '#3DDC84', unlit: '#1E2A22' };

export function DeltaTheme({ children }: { children?: ReactNode }) {
  return <Theme fonts={DELTA_FONTS} colors={DELTA_COLORS}>{children}</Theme>;
}

/** A cable color name's hex, for paints that do not take theme names (bands with alpha). */
export const CABLE: Record<string, string> = { orange: C.orange, blue: C.blue, green: C.green, violet: C.violet };
