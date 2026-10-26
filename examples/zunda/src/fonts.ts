// 書体の読み込み。<Assets> に <Font src={…}> として置く。
//
// 和文フォントはファイルが大きいので、Google Fonts の `text=` で、この動画が
// 描く文字だけに絞って読み込む（googleFont の `text`）。絞り込みに漏れた文字は、
// 別の書体で描かれ、書き出し時に警告が出る。和文を描くときは、その文字列を
// 各シーンの `strings`（または台本）に入れておくこと。

import { googleFont } from '@celesta/react';

import { FONT } from './theme.ts';

const ASCII = Array.from({ length: 0x7f - 0x20 }, (_, i) => String.fromCharCode(0x20 + i)).join('');
/** カタカナ全部。声のシーンに出す VOICEVOX の読み（AudioQuery のモーラ）は prepare() まで分からないため。 */
const KATAKANA = Array.from({ length: 0x30fc - 0x30a1 + 1 }, (_, i) => String.fromCharCode(0x30a1 + i)).join('');

/** 和文の 2 書体を、`strings` に出てくる文字・カタカナ・ASCII だけで読み込む URL。 */
export function japaneseFontUrls(strings: readonly string[]): string[] {
  const text = strings.join('') + KATAKANA + ASCII;
  return [
    googleFont(FONT.ja, { weights: [500, 800], text }),
    googleFont(FONT.title, { text }),
  ];
}

/** コードとターミナル用の等幅フォント（ラテン文字だけなので絞らない）。 */
export const MONO_FONT_URL = googleFont(FONT.mono, { weights: [400, 500, 700] });
