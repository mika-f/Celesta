// 画面の隅に出す「いま足したコード」のパネル。段階（step）ごとに行が増え、
// いちばん新しい段階は 1 文字ずつ打ち込まれて、しばらく緑に光る。
//
// 色分けは @celesta/code の <Code>。打ち込みの途中でも、完成したコードを一度だけ
// 字句分割した色のまま、先頭から `visibleCharacters` 文字だけを見せる。

import { Easings, Group, Rect, Text, progress } from '@celesta/react';
import { Code, codeCharacterCount, codeThemes } from '@celesta/code';
import type { CodeLanguage, CodeTheme } from '@celesta/code';

import { COLOR, FONT, solid } from '../theme.ts';

/** `at` フレームから `lines` が打ち込まれる。前の段階の行の下に足される。 */
export type CodeStep = { at: number; lines: string[] };

const STYLE = { fontFamily: FONT.mono, fontSize: 21, fontWeight: 500, lineHeight: 31 } as const;
/** コードの左右の余白 */
const INSET = 24;
/** 1 フレームに打ち込む文字数 */
const TYPING_SPEED = 1.8;
/** 足した行を光らせておくフレーム数 */
const HIGHLIGHT_FRAMES = 50;

/** 動画全体の色に合わせたテーマ。トークンの名前は @celesta/code（twinkleplop）のもの。 */
const THEME: CodeTheme = {
  foreground: COLOR.code,
  highlightLine: '#7CC24230',
  tokens: {
    ...codeThemes.dark.tokens,
    string: COLOR.string, template: COLOR.string,
    number: COLOR.number, boolean: COLOR.number, constant: COLOR.number,
    tag_name: COLOR.tag, keyword: COLOR.tag,
    attr_name: COLOR.attribute, property: COLOR.attribute,
    punctuation: COLOR.mute, operator: COLOR.mute,
  },
};

export type CodePanelProps = {
  /** 動画全体のフレーム（`steps` の `at` と同じ基準） */
  frame: number;
  steps: CodeStep[];
  language?: CodeLanguage;
  x?: number;
  y?: number;
  width?: number;
  /** パネルの右上に出すファイル名 */
  title?: string;
  /** 行がこれより増えたら、古い行から上に流す */
  maxRows?: number;
};

export function CodePanel({
  frame, steps, language = 'tsx', x = 40, y = 36, width = 660, title = 'film.tsx', maxRows = 9,
}: CodePanelProps) {
  const started = steps.filter((step) => frame >= step.at);
  if (started.length === 0) return null;

  // 始まった段階の行をすべて 1 つのソースにする。前の段階は打ち終わっていて、
  // いちばん新しい段階だけが途中まで打たれている。
  const latest = started[started.length - 1];
  const firstNew = started.slice(0, -1).reduce((count, step) => count + step.lines.length, 0) + 1;
  const lines = started.flatMap((step) => step.lines);
  const source = lines.join('\n');
  const startOf = (lineNumber: number) => codeCharacterCount(source, { line: lineNumber, column: 1 });
  const visibleCharacters = startOf(firstNew) + Math.floor((frame - latest.at) * TYPING_SPEED);

  // 打ち始めた行（行の先頭が見えている行）の数。これを超えたぶんだけ上に流す。
  let rows = firstNew - 1;
  while (rows < lines.length && startOf(rows + 1) < visibleCharacters) rows++;
  const scrolled = Math.max(0, rows - maxRows);
  const fresh = frame - latest.at < HIGHLIGHT_FRAMES
    ? Array.from({ length: rows - firstNew + 1 }, (_, i) => firstNew + i)
    : [];

  const height = 62 + Math.min(rows, maxRows) * STYLE.lineHeight + 14;
  const appear = progress(frame, steps[0].at, 14, Easings.easeOutCubic);

  return (
    <Group x={x} y={y + 16 * (1 - appear)} opacity={appear}>
      <Rect width={width} height={height} cornerRadius={18} fill={COLOR.panel} stroke="#FFFFFF26" strokeWidth={2}
        shadow={{ color: '#00000080', blur: 24, offsetX: 0, offsetY: 10 }} />
      {['#FF5F57', '#FEBC2E', '#28C840'].map((color, i) => (
        <Rect key={color} x={22 + i * 22} y={20} width={12} height={12} cornerRadius={6} fill={color} />
      ))}
      <Text x={width - 22} y={33} anchorX={1} anchorY="baseline"
        style={{ fontFamily: FONT.mono, fontSize: 16, fill: solid(COLOR.mute) }}>
        {title}
      </Text>
      {/* 古い行は上に流す。パネルの外に出たぶんは切り取る。光らせる帯はパネルの右端まで */}
      <Group y={56} clip={{ width, height: Math.min(rows, maxRows) * STYLE.lineHeight }}>
        <Code x={INSET} y={-scrolled * STYLE.lineHeight} language={language} style={STYLE} theme={THEME}
          visibleCharacters={visibleCharacters} highlightLines={fresh} highlightWidth={width - INSET}>
          {source}
        </Code>
      </Group>
    </Group>
  );
}
