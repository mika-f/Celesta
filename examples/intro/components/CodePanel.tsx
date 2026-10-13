import type { ReactNode } from 'react';
import { Group, Rect } from '@celesta/react';
import { Code, codeThemes, type CodeTheme } from '@celesta/code';
import { C, FONT } from '../constants';
import { Label } from './Label';

// codeThemes.dark recolored to the Noir palette: the accent for keywords and
// literals, the foreground for names and strings, muted for punctuation.
export const noirTheme: CodeTheme = {
  ...codeThemes.dark,
  foreground: C.fg,
  highlightLine: `${C.accent}33`,
  tokens: {
    ...codeThemes.dark.tokens,
    keyword: C.accent, builtin: C.accent, number: C.accent, constant: C.accent, boolean: C.accent,
    function: C.fg, tag_name: C.fg, type: C.fg, namespace: C.fg,
    string: C.fg, template: C.fg, regex: C.fg,
    attr_name: `${C.fg}B3`, property: `${C.fg}B3`, parameter: `${C.fg}B3`,
    punctuation: C.muted, operator: C.muted, comment: C.muted,
  },
};

// Space between the panel edge and the code; the header row above the code.
export const INSET = 32;
export const HEADER = 56;

export function codeStyle(fontSize: number) {
  return { fontFamily: FONT.mono, fontSize, lineHeight: fontSize * 1.5 };
}

export type CodePanelProps = {
  x: number;
  y: number;
  width: number;
  height: number;
  source: string;
  language?: 'tsx' | 'ts' | 'json' | 'bash' | 'text';
  fontSize?: number;
  highlightLines?: number[];
  visibleCharacters?: number;
  title?: string;
  theme?: CodeTheme;
  opacity?: number;
  children?: ReactNode;
};

// A square editor panel with a file-name header. Children are drawn in the
// code's coordinate space (for carets and annotations).
export function CodePanel({
  x, y, width, height, source, language = 'tsx', fontSize = 26, highlightLines,
  visibleCharacters, title, theme = noirTheme, opacity = 1, children,
}: CodePanelProps) {
  const top = title ? HEADER : 0;
  return (
    <Group x={x} y={y} opacity={opacity}>
      <Rect width={width} height={height} fill={C.panel} stroke={C.line} strokeWidth={1} />
      {title && (
        <>
          <Label x={INSET} y={HEADER / 2} size={18} color={C.muted} ay={0.5}>{title}</Label>
          <Rect y={HEADER} width={width} height={1} fill={C.line} />
        </>
      )}
      <Group x={INSET} y={top + INSET}>
        <Code language={language} style={codeStyle(fontSize)} theme={theme}
          highlightLines={highlightLines} highlightWidth={width - INSET * 2}
          visibleCharacters={visibleCharacters}>
          {source}
        </Code>
        {children}
      </Group>
    </Group>
  );
}
