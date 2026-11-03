import type { ReactNode } from 'react';
import { Theme } from '@celesta.studio/react';
import { C, FONT } from '../components/theme';

// The DELTA look as a <Theme> (#421): the film's font stacks and colors by
// name, so 0.8.0's own code and the @celesta.studio/components it uses write
// fontFamily="mono" and color="orange" instead of the stacks and hexes.
export const DELTA_FONTS = { display: FONT.display, sans: FONT.sans, mono: FONT.mono };

export function DeltaTheme({ children }: { children?: ReactNode }) {
  return <Theme fonts={DELTA_FONTS} colors={C}>{children}</Theme>;
}
