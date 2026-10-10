import { Group, Rect, Span, Text } from '@celesta/react';
import { C, FONT } from '../constants';
import { Label } from './Label';

// A line of code as colored tokens: [text, color?].
export type CodeLine = [string, string?][];

export const SYNTAX = {
  tag: '#FF8FB1',
  attr: '#FFD27A',
  value: '#7FE0C6',
  fn: '#8FB8FF',
  plain: C.codeText,
  dim: C.codeDim,
} as const;

type CodeCardProps = {
  x: number;
  y: number;
  width: number;
  file: string;
  lines: CodeLine[];
  size?: number;
  // How many characters are typed so far (all when omitted).
  typed?: number;
  // Lines drawn with a highlight bar behind them.
  highlight?: number[];
};

// A dark editor card with a file tab. Typing counts characters across lines.
export function CodeCard({ x, y, width, file, lines, size = 30, typed, highlight = [] }: CodeCardProps) {
  const lineHeight = Math.round(size * 1.55);
  const height = 84 + lines.length * lineHeight + 28;
  let budget = typed ?? Infinity;
  let caretLine = -1;
  let caretColumn = 0;
  const shown = lines.map((tokens, i) => {
    const out: [string, string?][] = [];
    let column = 0;
    for (const [text, color] of tokens) {
      const chars = [...text];
      const take = Math.max(0, Math.min(chars.length, budget));
      if (take > 0) out.push([chars.slice(0, take).join(''), color]);
      budget -= take;
      column += take;
      if (take < chars.length) break;
    }
    if (typed !== undefined && caretLine < 0 && budget <= 0) {
      caretLine = i;
      caretColumn = column;
    }
    return out;
  });
  return (
    <Group x={x} y={y}>
      <Rect width={width} height={height} cornerRadius={28} fill={C.code} />
      <Rect x={24} y={20} width={Math.max(160, file.length * size * 0.62 + 48)} height={46} cornerRadius={14} fill={C.codeLine} />
      <Label x={48} y={43} ay={0.5} size={size * 0.8} font="mono" weight={700} color={SYNTAX.dim}>{file}</Label>
      {lines.map((_, i) => highlight.includes(i) && (
        <Rect key={`hl-${i}`} x={14} y={84 + i * lineHeight - 4} width={width - 28} height={lineHeight}
          cornerRadius={10} fill="#FFC93C30" />
      ))}
      {shown.map((tokens, i) => tokens.length > 0 && (
        <Text key={i} x={32} y={84 + i * lineHeight + lineHeight / 2 - 4} anchorY={0.5}
          style={{ fontFamily: FONT.mono, fontSize: size, fontWeight: 500, fill: { type: 'solid', color: SYNTAX.plain } }}>
          {tokens.map(([text, color], k) => (
            <Span key={k} style={{ fill: color ?? SYNTAX.plain }}>{text}</Span>
          ))}
        </Text>
      ))}
      {caretLine >= 0 && (
        <Rect x={32 + caretColumn * size * 0.6} y={84 + caretLine * lineHeight + 4}
          width={4} height={lineHeight - 14} fill={C.yellow} />
      )}
    </Group>
  );
}

export function codeCardHeight(lines: number, size = 30) {
  return 84 + lines * Math.round(size * 1.55) + 28;
}
