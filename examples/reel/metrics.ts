import { measureText } from '@celesta.studio/react';
import { FONT, FONT_SRC } from './constants';

// Measured in prepare(): JetBrains Mono's per-glyph advance in em (0.6), and
// the logo's letter positions. The fallbacks keep the reel rendering when the
// fonts can't be loaded (e.g. offline).
export let monoAdvance = 0.6;
export const LOGO_SIZE = 250;
export let logo: { width: number; letters: { text: string; x: number }[] } | null = null;

export async function prepare() {
  try {
    // prepare() runs outside the film's <Theme>, so the names come along.
    const options = { fonts: [FONT_SRC], theme: { fonts: FONT } };
    const mono = await measureText('M', { fontFamily: 'mono', fontSize: 100 }, options);
    monoAdvance = mono.width / 100;
    const word = await measureText('Celesta',
      { fontFamily: 'display', fontSize: LOGO_SIZE, fontWeight: 700 }, options);
    logo = { width: word.width, letters: word.glyphs.map(({ text, x }) => ({ text, x })) };
  } catch (error) {
    console.warn(`text measurement unavailable, using fallback metrics: ${error}`);
  }
}
