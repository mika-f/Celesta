import { createContext, useContext } from 'react';
import { safeAreaInsets } from '@celesta/layout';
import { H, W } from '../components/theme';

// The canvas the broadcast is laid out for: the film's 16:9, or the
// short's 9:16. The short keeps copy meant to be read inside the platforms'
// safe area ('vertical-social'); the wall, the monitors and the edges fill
// the frame.
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
