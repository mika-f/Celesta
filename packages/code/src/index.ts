import * as React from 'react';
import { Group, Rect, Text, textCaret, useTextMetrics } from '@celesta/react';
import type { CommonProps, TextColorRun, TextStyle } from '@celesta/react';
import { codeLines, tokenizeCode, validateTabSize } from './syntax.js';
import type { CodeLanguage } from './syntax.js';

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
    const runs: TextColorRun[] = [];
    for (const run of line.runs) {
      const color = theme.tokens[run.type] ?? theme.foreground;
      const end = run.column + Array.from(run.text).length;
      const previous = runs[runs.length - 1];
      if (previous?.color === color) previous.end = end;
      else runs.push({ start: run.column, end, color });
    }
    return runs;
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
    lines[index].text && visible > lines[index].start ? React.createElement(Text, {
      y: baseline, anchorY: 'baseline', children: lines[index].text,
      style: {
        ...resolvedStyle, fill: { type: 'solid', color: theme.foreground }, colorRuns: runs,
        visibleCharacters: visible < lines[index].start + lines[index].columns.length - 1
          ? lines[index].columns[Math.max(0, visible - lines[index].start)] : undefined,
      },
    }) : null,
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

function codePointLength(text: string): number {
  let length = 0;
  for (const _ of text) length++;
  return length;
}

function codePositionParts(source: string, { line, column }: CodePosition) {
  if (typeof source !== 'string') throw new Error('Code source must be a string');
  // Keep separators so CRLF still counts as two original source code points.
  const parts = source.split(/(\r\n|\r|\n)/);
  const index = (line - 1) * 2;
  if (!Number.isSafeInteger(line) || line < 1 || index >= parts.length) throw new Error('Code line is out of range');
  if (!Number.isSafeInteger(column) || column < 1 || column > codePointLength(parts[index]) + 1) throw new Error('Code column is out of range');
  return { parts, index };
}

/** Original source code points before a position, for Code.visibleCharacters. */
export function codeCharacterCount(source: string, position: CodePosition): number {
  const { parts, index } = codePositionParts(source, position);
  // Count in one walk; this may run every frame for staged reveals.
  let count = position.column - 1;
  for (let part = 0; part < index; part++) count += codePointLength(parts[part]);
  return count;
}

/**
 * Measures a caret/annotation using native shaped glyph metrics and Code's style/tabs.
 * Unsupported in the browser runtime, which cannot supply shaped glyph metrics.
 */
export function useCodePoint(
  source: string, position: CodePosition, style?: TextStyle, tabSize = 2,
): CodePoint {
  validateTabSize(tabSize);
  codePositionParts(source, position);
  const { line, column } = position;
  const resolvedStyle = codeStyle(style);
  const layout = React.useMemo(() => {
    const lines = codeLines(tokenizeCode(source), tabSize);
    let start = 0;
    const positions = lines.map(line => {
      const position = { start, columns: line.columns };
      start += line.columns[line.columns.length - 1] + 1;
      return position;
    });
    return { text: lines.map(line => line.text).join('\n'), positions };
  }, [source, tabSize]);
  const metrics = useTextMetrics(layout.text, resolvedStyle);
  const { ascent } = useTextMetrics('M', resolvedStyle);
  const target = layout.positions[line - 1];
  const offset = target.start + target.columns[column - 1];
  let glyphs;
  try { glyphs = metrics.glyphs; }
  catch { throw new Error('useCodePoint() requires native shaped glyph metrics and is unsupported in browser renders'); }
  const caret = textCaret(metrics, offset);
  const x = glyphs.some(glyph => glyph.line === line - 1) ? caret.x : 0;
  const y = (line - 1) * resolvedStyle.lineHeight!;
  return { x, y, baseline: y + ascent, lineHeight: resolvedStyle.lineHeight! };
}
