// 画面の隅に出す「いま足したコード」のパネル。段階（step）ごとに行が増え、
// いちばん新しい段階は 1 文字ずつ打ち込まれて、しばらく緑に光る。
//
// 色分けは見た目のための簡単な字句分割で、構文解析ではない。

import { Easings, Group, Rect, Text, progress } from '@celesta/react';

import { COLOR, FONT, MONO_ADVANCE, solid } from '../theme.ts';

/** `at` フレームから `lines` が打ち込まれる。前の段階の行の下に足される。 */
export type CodeStep = { at: number; lines: string[] };

const FONT_SIZE = 21;
const LINE_HEIGHT = 31;
/** 1 フレームに打ち込む文字数 */
const TYPING_SPEED = 1.8;
/** 足した行を光らせておくフレーム数 */
const HIGHLIGHT_FRAMES = 50;

// 文字列 / タグ / 属性名（直後が = か :）/ 数 / 空白 / 記号 / それ以外の語。
const TOKEN = /("[^"]*"?|'[^']*'?|<\/?[A-Za-z]+|\/?>|[A-Za-z_]+(?=[=:])|-?\d+(?:\.\d+)?|\s+|[{}()[\],;=:]|[^\s"'{}()[\],;=:<>\d]+|.)/g;

function tokenColor(token: string, next: string): string {
  if (/^["']/.test(token)) return COLOR.string;
  if (/^<\/?[A-Za-z]/.test(token) || /^\/?>$/.test(token)) return COLOR.tag;
  if (/^[A-Za-z_]+$/.test(token) && /^[=:]/.test(next)) return COLOR.attribute;
  if (/^-?\d/.test(token)) return COLOR.number;
  if (/^[{}()[\],;=:]$/.test(token)) return COLOR.mute;
  return COLOR.code;
}

function CodeLine({ text }: { text: string }) {
  const tokens = text.match(TOKEN) ?? [];
  let column = 0;
  return (
    <>
      {tokens.map((token, i) => {
        const x = column * FONT_SIZE * MONO_ADVANCE;
        column += [...token].length;
        if (!token.trim()) return null;
        return (
          <Text key={i} x={x} anchorY="baseline"
            style={{ fontFamily: FONT.mono, fontSize: FONT_SIZE, fontWeight: 500, fill: solid(tokenColor(token, tokens[i + 1] ?? '')) }}>
            {token}
          </Text>
        );
      })}
    </>
  );
}

export type CodePanelProps = {
  /** 動画全体のフレーム（`steps` の `at` と同じ基準） */
  frame: number;
  steps: CodeStep[];
  x?: number;
  y?: number;
  width?: number;
  /** パネルの右上に出すファイル名 */
  title?: string;
  /** 行がこれより増えたら、古い行から上に流す */
  maxRows?: number;
};

export function CodePanel({ frame, steps, x = 40, y = 36, width = 660, title = 'film.tsx', maxRows = 9 }: CodePanelProps) {
  const started = steps.filter((step) => frame >= step.at);
  if (started.length === 0) return null;

  const rows: { text: string; fresh: boolean }[] = [];
  started.forEach((step, i) => {
    const latest = i === started.length - 1;
    let typed = latest ? Math.floor((frame - step.at) * TYPING_SPEED) : Number.POSITIVE_INFINITY;
    for (const text of step.lines) {
      if (typed <= 0) break;
      rows.push({ text: text.slice(0, typed), fresh: latest && frame - step.at < HIGHLIGHT_FRAMES });
      typed -= text.length;
    }
  });
  const visible = rows.slice(-maxRows);
  const height = 62 + visible.length * LINE_HEIGHT + 14;
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
      {visible.map((row, i) => (
        <Group key={i} y={56 + i * LINE_HEIGHT}>
          {row.fresh && <Rect width={width} height={LINE_HEIGHT} fill="#7CC24230" />}
          <Group x={24} y={22}>
            <CodeLine text={row.text} />
          </Group>
        </Group>
      ))}
    </Group>
  );
}
