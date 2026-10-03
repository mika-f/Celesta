import * as React from 'react';
import { Group, Rect, Text, useTextMetrics } from '@celesta/react';
import type { CommonProps, TextStyle } from '@celesta/react';
import { codeLines, expandTabs, tokenizeCode, validateTabSize } from './syntax.js';
import type { CodeLanguage, CodeRun } from './syntax.js';

export { tokenizeCode } from './syntax.js';
export type { CodeLanguage, CodeToken } from './syntax.js';

export interface CodeTheme {
  foreground: string;
  highlightLine: string;
  /** Token names map to solid colors; missing names use foreground. */
  tokens: Readonly<Record<string, string>>;
}

export const codeThemes: Readonly<Record<'dark' | 'light', CodeTheme>> = {
  dark: {
    foreground: '#c9c9c5', highlightLine: '#ffffff18',
    tokens: {
      keyword: '#ffffff', builtin: '#ffffff', function: '#f2f2ee', tag_name: '#f2f2ee',
      type: '#f2f2ee', namespace: '#f2f2ee', string: '#a68bbf', template: '#a68bbf',
      regex: '#a68bbf', number: '#d9cff7', constant: '#d9cff7', boolean: '#d9cff7',
      attr_name: '#a4a4a0', property: '#a4a4a0', parameter: '#a4a4a0',
      punctuation: '#6e6e6a', operator: '#6e6e6a', comment: '#787873',
    },
  },
  light: {
    foreground: '#332f3b', highlightLine: '#78618e18',
    tokens: {
      keyword: '#6e467d', builtin: '#6e467d', function: '#3b577a', tag_name: '#3b577a',
      type: '#3b577a', namespace: '#3b577a', string: '#557149', template: '#557149',
      regex: '#557149', number: '#865a35', constant: '#865a35', boolean: '#865a35',
      attr_name: '#65516d', property: '#65516d', parameter: '#65516d',
      punctuation: '#777078', operator: '#777078', comment: '#777078',
    },
  },
};

export interface CodeProps extends CommonProps {
  children: string;
  language?: CodeLanguage;
  /** Use a monospaced font. Alignment is always left; wrapping is disabled. */
  style?: TextStyle;
  theme?: CodeTheme;
  /** Code points from the original source, including newlines and tabs. */
  visibleCharacters?: number;
  /** One-based line numbers. Bands stay at full width during typing. */
  highlightLines?: readonly number[];
  /** Line highlight width in pixels. Defaults to the full source width. */
  highlightWidth?: number;
  /** Width of a tab stop in code points. Defaults to 2. */
  tabSize?: number;
}

function codeStyle(style: TextStyle = {}): TextStyle {
  if (style.align !== undefined && style.align !== 'left') throw new Error('Code only supports left alignment');
  const fontSize = style.fontSize ?? 24;
  const lineHeight = style.lineHeight ?? fontSize * 1.5;
  if (!Number.isFinite(fontSize) || fontSize <= 0 || !Number.isFinite(lineHeight) || lineHeight <= 0) {
    throw new Error('Code requires positive fontSize and lineHeight');
  }
  return { ...style, fontFamily: style.fontFamily ?? 'JetBrains Mono', fontSize, lineHeight, align: 'left' };
}

function TokenRun({ run, visible, style, color, tabSize }: {
  run: CodeRun; visible: number; style: TextStyle; color: string; tabSize: number;
}): React.ReactElement | null {
  // ponytail: prefix shaping is cached per run, but long lines still need many
  // measurements and Text layers. Shared styled-text shaping is the upgrade path.
  const { width } = useTextMetrics(run.prefix, style);
  const count = Math.max(0, Math.min(run.characters.length, visible - run.start));
  const text = count === run.characters.length ? run.text
    : expandTabs(run.characters.slice(0, count).join(''), tabSize, run.column);
  if (!text) return null;
  return React.createElement(Text, {
    x: width, anchorY: 'baseline', style: { ...style, fill: { type: 'solid', color } }, children: text,
  });
}

/** Syntax-highlighted code built from Celesta's Text and Rect primitives. */
export function Code({
  children, language = 'text', style, theme = codeThemes.dark,
  visibleCharacters = Infinity, highlightLines = [], highlightWidth, tabSize = 2, ...groupProps
}: CodeProps): React.ReactElement {
  validateTabSize(tabSize);
  if (typeof visibleCharacters !== 'number' || Number.isNaN(visibleCharacters)) throw new Error('Code visibleCharacters must be a number');
  if (highlightWidth !== undefined && (!Number.isFinite(highlightWidth) || highlightWidth < 0)) {
    throw new Error('Code highlightWidth must be a finite non-negative number');
  }
  const resolvedStyle = codeStyle(style);
  const tokens = React.useMemo(() => tokenizeCode(children, language), [children, language]);
  const lines = React.useMemo(() => codeLines(tokens, tabSize), [tokens, tabSize]);
  const coloredLines = React.useMemo(() => lines.map(line => {
    const runs: (CodeRun & { color: string })[] = [];
    for (const run of line.runs) {
      const color = theme.tokens[run.type] ?? theme.foreground;
      const previous = runs[runs.length - 1];
      if (previous?.color === color) {
        for (const character of run.characters) previous.characters.push(character);
        previous.text += run.text;
      } else {
        runs.push({ ...run, characters: [...run.characters], color });
      }
    }
    return runs.filter(run => run.text.trim().length > 0);
  }), [lines, theme]);
  const metrics = useTextMetrics(lines.map(line => line.text).join('\n'), resolvedStyle);
  // Empty text has no glyph runs in some fonts, so measure a baseline explicitly.
  const baseline = useTextMetrics('M', resolvedStyle).ascent;
  const lineHeight = resolvedStyle.lineHeight!;
  const visible = Math.max(0, Math.floor(visibleCharacters));
  const highlighted = new Set(highlightLines);
  return React.createElement(Group, groupProps, coloredLines.map((runs, index) => React.createElement(
    Group, { key: index, y: index * lineHeight },
    highlighted.has(index + 1) ? React.createElement(Rect, {
      width: highlightWidth ?? metrics.width, height: lineHeight, fill: theme.highlightLine,
    }) : null,
    React.createElement(Group, { y: baseline }, runs.map(run => React.createElement(TokenRun, {
      key: run.start, run, visible, style: resolvedStyle, tabSize,
      color: run.color,
    }))),
  )));
}

export interface CodePosition {
  /** One-based source line and code-point column, before tab expansion. */
  line: number;
  column: number;
}

export interface CodePoint {
  /** Position relative to Code's top-left, before group transforms. */
  x: number;
  y: number;
  baseline: number;
  lineHeight: number;
}

function codePositionParts(source: string, { line, column }: CodePosition) {
  if (typeof source !== 'string') throw new Error('Code source must be a string');
  // Keep separators so CRLF still counts as two original source code points.
  const parts = source.split(/(\r\n|\r|\n)/);
  const index = (line - 1) * 2;
  if (!Number.isSafeInteger(line) || line < 1 || index >= parts.length) throw new Error('Code line is out of range');
  const characters = Array.from(parts[index]);
  if (!Number.isSafeInteger(column) || column < 1 || column > characters.length + 1) throw new Error('Code column is out of range');
  return { parts, index, characters };
}

/** Original source code points before a position, for Code.visibleCharacters. */
export function codeCharacterCount(source: string, position: CodePosition): number {
  const { parts, index } = codePositionParts(source, position);
  return Array.from(parts.slice(0, index).join('')).length + position.column - 1;
}

/** Measures a caret/annotation position using the same style and tabs as Code. */
export function useCodePoint(
  source: string, position: CodePosition, style?: TextStyle, tabSize = 2,
): CodePoint {
  validateTabSize(tabSize);
  const { characters } = codePositionParts(source, position);
  const { line, column } = position;
  const resolvedStyle = codeStyle(style);
  const prefix = expandTabs(characters.slice(0, column - 1).join(''), tabSize);
  const { width } = useTextMetrics(prefix, resolvedStyle);
  const { ascent } = useTextMetrics('M', resolvedStyle);
  const y = (line - 1) * resolvedStyle.lineHeight!;
  return { x: width, y, baseline: y + ascent, lineHeight: resolvedStyle.lineHeight! };
}
