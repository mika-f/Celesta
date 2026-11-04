import { createContext, useContext } from 'react';
import { safeAreaInsets } from '@celesta.studio/layout';
import { H, W } from '../components/theme';

// The canvas a scene is laid out for: the 16:9 film, or the 9:16 short.
// The short keeps copy meant to be read inside the platforms' safe area
// ('vertical-social'); backgrounds and visuals fill the frame, and centred
// things are centred on the frame, not on the off-centre safe area.
export interface Layout {
  width: number;
  height: number;
  portrait: boolean;
  safe: { top: number; right: number; bottom: number; left: number };
}

export const LANDSCAPE: Layout = { width: W, height: H, portrait: false, safe: { top: 0, right: 0, bottom: 0, left: 0 } };

export const SHORT = { width: 1080, height: 1920 } as const;
export const PORTRAIT: Layout = {
  ...SHORT,
  portrait: true,
  safe: safeAreaInsets('vertical-social', SHORT.width, SHORT.height),
};

export const LayoutContext = createContext<Layout>(LANDSCAPE);
export const useLayout = () => useContext(LayoutContext);
