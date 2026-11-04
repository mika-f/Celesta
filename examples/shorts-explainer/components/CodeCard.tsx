import { Group, Rect, Text } from '@celesta.studio/react';
import { highlightCode } from '@celesta.studio/code';
import type { CodeTheme, HighlightedCodeLine } from '@celesta.studio/code';
import { C, FONT } from '../constants';
import { Label } from './Label';

// The video's own colors for @celesta.studio/code's token types: tags and calls
// in blue, keywords in pink, attributes in yellow, values in mint.
const THEME: CodeTheme = {
  foreground: C.codeText,
  highlightLine: '#FFC93C30',
  tokens: {
    keyword: '#FF8FB1', builtin: '#FF8FB1',
    tag_name: '#8FB8FF', function: '#8FB8FF', type: '#8FB8FF', namespace: '#8FB8FF',
    attr_name: '#FFD27A', property: '#FFD27A', parameter: '#FFD27A',
    string: '#7FE0C6', template: '#7FE0C6', regex: '#7FE0C6', number: '#7FE0C6', constant: '#7FE0C6', boolean: '#7FE0C6',
    punctuation: C.codeDim, operator: C.codeDim, comment: C.codeDim,
  },
};

// A TSX snippet split into lines and colored the way <Code> colors it.
export function highlightTsx(source: string): HighlightedCodeLine[] {
  return highlightCode(source, { language: 'tsx', theme: THEME });
}

// Source code points up to the end of line `i`: typing has reached the end
// of that line once `typed` is this many.
export function lineEnd(lines: HighlightedCodeLine[], i: number) {
  return lines[i].start + lines[i].columns.length - 1;
}

type CodeCardProps = {
  x: number;
  y: number;
  width: number;
  file: string;
  // From highlightTsx(), computed once outside the component.
  lines: HighlightedCodeLine[];
  size?: number;
  // How many source code points are typed so far, line breaks included (all
  // when omitted). The caret shows until typing passes the end.
  typed?: number;
  // Lines drawn with a highlight bar behind them.
  highlight?: number[];
};

// A dark editor card with a file tab. One <Text> per line, colored with the
// line's runs; typing reveals code points with visibleCharacters.
export function CodeCard({ x, y, width, file, lines, size = 30, typed, highlight = [] }: CodeCardProps) {
  const lineHeight = Math.round(size * 1.55);
  const height = codeCardHeight(lines.length, size);
  const visible = typed ?? Infinity;
  // JetBrains Mono advances every glyph by 0.6 em, so a column is a fixed
  // width and the caret needs no text measuring.
  const advance = size * 0.6;
  let caretLine = -1;
  let caretColumn = 0;
  if (typed !== undefined && typed <= lineEnd(lines, lines.length - 1)) {
    caretLine = 0;
    while (caretLine + 1 < lines.length && lines[caretLine + 1].start <= typed) caretLine++;
    const line = lines[caretLine];
    caretColumn = line.columns[Math.min(typed - line.start, line.columns.length - 1)];
  }
  return (
    <Group x={x} y={y}>
      <Rect width={width} height={height} cornerRadius={28} fill={C.code} />
      <Rect x={24} y={20} width={Math.max(160, file.length * size * 0.62 + 48)} height={46} cornerRadius={14} fill={C.codeLine} />
      <Label x={48} y={43} ay={0.5} size={size * 0.8} font="mono" weight={700} color={C.codeDim}>{file}</Label>
      {lines.map((_, i) => highlight.includes(i) && (
        <Rect key={`hl-${i}`} x={14} y={84 + i * lineHeight - 4} width={width - 28} height={lineHeight}
          cornerRadius={10} fill={THEME.highlightLine} />
      ))}
      {lines.map((line, i) => {
        if (!line.text || visible <= line.start) return null;
        const shown = Math.min(visible - line.start, line.columns.length - 1);
        return (
          <Text key={i} x={32} y={84 + i * lineHeight + lineHeight / 2 - 4} anchorY={0.5}
            style={{
              fontFamily: FONT.mono, fontSize: size, fontWeight: 500, fill: { type: 'solid', color: THEME.foreground },
              colorRuns: line.runs, visibleCharacters: line.columns[shown],
            }}>
            {line.text}
          </Text>
        );
      })}
      {caretLine >= 0 && (
        <Rect x={32 + caretColumn * advance} y={84 + caretLine * lineHeight + 4}
          width={4} height={lineHeight - 14} fill={C.yellow} />
      )}
    </Group>
  );
}

export function codeCardHeight(lines: number, size = 30) {
  return 84 + lines * Math.round(size * 1.55) + 28;
}
