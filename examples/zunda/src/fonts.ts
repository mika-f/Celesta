// 書体の読み込み。<Assets> に <Font src={…}> として置く。
//
// 和文フォントはファイルが大きいので、Google Fonts の `text=` で、この動画が
// 描く文字だけに絞って読み込む。絞り込みに漏れた文字は、別の書体で描かれて
// しまう。和文を描くときは、その文字列を各シーンの `strings`（または台本）に
// 入れておくこと。

const ASCII = Array.from({ length: 0x7f - 0x20 }, (_, i) => String.fromCharCode(0x20 + i)).join('');

/** M PLUS Rounded 1c（500, 800）と Dela Gothic One を、`strings` に出てくる文字と ASCII だけで読み込む URL。 */
export function japaneseFontsUrl(strings: readonly string[]): string {
  const glyphs = [...new Set([...strings.join(''), ...ASCII])].sort().join('');
  return 'https://fonts.googleapis.com/css2?family=M+PLUS+Rounded+1c:wght@500;800&family=Dela+Gothic+One'
    + `&text=${encodeURIComponent(glyphs)}`;
}

/** コードとターミナル用の等幅フォント（ラテン文字だけなので絞らない）。 */
export const MONO_FONT_URL = 'https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400;500;700';
