import { measureText } from '@celesta/react';
import { FONT, FONT_SRC } from './constants';

// Measured in prepare(): JetBrains Mono's per-glyph advance in em (0.6), and
// the logo's letter positions. The fallbacks keep the reel rendering when the
// fonts can't be loaded (e.g. offline).
export let monoAdvance = 0.6;
export const LOGO_SIZE = 250;
export let logo: { width: number; letters: { text: string; x: number }[] } | null = null;

export async function prepare() {
  try {
    const fonts = [FONT_SRC];
    const mono = await measureText('M', { fontFamily: FONT.mono, fontSize: 100 }, { fonts });
    monoAdvance = mono.width / 100;
    const word = await measureText('Celesta',
      { fontFamily: FONT.display, fontSize: LOGO_SIZE, fontWeight: 700 }, { fonts });
    logo = { width: word.width, letters: word.glyphs.map(({ text, x }) => ({ text, x })) };
  } catch (error) {
    console.warn(`text measurement unavailable, using fallback metrics: ${error}`);
  }
}
