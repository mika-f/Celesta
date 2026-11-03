import type { TextStyle } from '@celesta.studio/react';
import { FONT } from './constants';

// The two tapes under every scene: a line of the manifesto, and the API the
// scene shows. They are listed here so prepare() can measure each one
// (measure.ts) and a tape can repeat its text just enough times.
export const TAPES = {
  hook: ['Write your video in code.', 'React + TypeScript → MP4'],
  frame: ['Every frame is a function of its number.', 'useCurrentFrame()'],
  measure: ['Every glyph, measured.', 'measureText()'],
  fit: ['Fit the box.', 'fitText()'],
  span: ['Light another word, and the meaning moves.', '<Span>'],
  beat: ['Cut on the beat.', 'useBeat()'],
  outro: ['Write your video in code.', 'Celesta'],
} as const satisfies Record<string, readonly [string, string]>;

export type TapeKind = 'main' | 'code';

// One repeat of a tape's text, separator included. Archivo Black has no
// bullet, so the main tape separates with a slash.
export const tapeRun = (kind: TapeKind, text: string) => (kind === 'main' ? `${text}  /  ` : `${text}  ●  `);

export const tapeStyle = (kind: TapeKind): TextStyle => kind === 'main'
  ? { fontFamily: FONT.display, fontSize: 54, fontWeight: 400, letterSpacing: -0.02 * 54 }
  : { fontFamily: FONT.mono, fontSize: 40, fontWeight: 700 };
