import type { TextStyle } from '@celesta.studio/react';
import { FONT } from './constants';

// The two tapes under every scene: a line of the manifesto, and the API the
// scene shows. They are listed here so prepare() can measure each one
// (measure.ts) and a tape can repeat its text just enough times.
export const TAPES = {
  hook: ['動画を、コードで書く。', 'React + TypeScript → MP4'],
  frame: ['すべてのフレームは、フレーム番号の関数。', 'useCurrentFrame()'],
  phrase: ['文節で、折り返す。', "lineBreak: 'phrase'"],
  fit: ['箱に、収める。', 'fitText()'],
  span: ['光らせる言葉で、意味が変わる。', '<Span>'],
  beat: ['拍に合わせて、言葉を落とす。', 'useBeat()'],
  outro: ['動画を、コードで書く。', 'Celesta'],
} as const satisfies Record<string, readonly [string, string]>;

export type TapeKind = 'main' | 'code';

// One repeat of a tape's text, separator included.
export const tapeRun = (_kind: TapeKind, text: string) => `${text}  ●  `;

export const tapeStyle = (kind: TapeKind): TextStyle => kind === 'main'
  ? { fontFamily: FONT.ja, fontSize: 54, fontWeight: 900 }
  : { fontFamily: FONT.mono, fontSize: 40, fontWeight: 700 };
