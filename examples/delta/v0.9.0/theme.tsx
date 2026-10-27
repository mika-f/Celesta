import type { ReactNode } from 'react';
import { Theme } from '@celesta/react';
import { C, FONT } from '../components/theme';

// The DELTA look as a <Theme>: the film's font stacks and colors by name.
// 0.9.0 looks them up in Rect and Path paints too (#481), so shapes write
// fill="ink" and stroke="line" like the text does.
export const DELTA_FONTS = { display: FONT.display, sans: FONT.sans, mono: FONT.mono };
/** The broadcast's tally colors, on top of the series palette. */
export const DELTA_COLORS = { ...C, tally: '#FF3B30', preview: '#30D158' };

export function DeltaTheme({ children }: { children?: ReactNode }) {
  return <Theme fonts={DELTA_FONTS} colors={DELTA_COLORS}>{children}</Theme>;
}
