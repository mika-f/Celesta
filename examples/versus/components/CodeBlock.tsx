import { Group, Rect, Text, useTextMetrics } from '@celesta/react';
import { Code, type CodeTheme } from '@celesta/code';
import { useMemo } from 'react';
import { FONT } from '../constants';

// One palette for both languages, so the three panels read alike.
export const THEME: CodeTheme = {
  foreground: '#E6E9F2',
  highlightLine: '#00000000',
  tokens: {
    keyword: '#FF7AB6', builtin: '#FF7AB6', boolean: '#FFB86C', constant: '#FFB86C', number: '#FFB86C',
    function: '#7FD1FF', tag_name: '#7FD1FF', type: '#7FD1FF', namespace: '#7FD1FF',
    string: '#B5E48C', template: '#B5E48C', regex: '#B5E48C',
    attr_name: '#C9B2FF', property: '#C9B2FF', parameter: '#E6E9F2',
    punctuation: '#8A93AD', operator: '#8A93AD', comment: '#5C6680',
  },
};

const RUST_KEYWORDS = new Set(['let', 'fn', 'if', 'else', 'as', 'for', 'in', 'impl', 'pub', 'use', 'struct',
  'const', 'mut', 'move', 'match', 'return', 'self', 'Self', 'where', 'mod']);
const TOKEN = /(\/\/.*$)|("(?:[^"\\]|\\.)*")|('[a-z_]+\b(?!'))|(\b\d+\.?\d*\b)|([A-Za-z_][A-Za-z0-9_]*!?)|(\s+)|(.)/g;

type Run = { text: string; color: string; column: number };

// A small Rust highlighter: Celesta's Code highlights TSX, TS, JSON and Bash.
function rustLines(source: string): Run[][] {
  const colors = THEME.tokens as Record<string, string>;
  return source.split('\n').map((line) => {
    const runs: Run[] = [];
    let column = 0;
    for (const m of line.matchAll(TOKEN)) {
      const [text, comment, string, lifetime, number, ident] = m;
      const before = line.slice(0, m.index).trimEnd();
      const after = line.slice(m.index! + text.length).trimStart();
      let type = 'punctuation';
      if (comment) type = 'comment';
      else if (string) type = 'string';
      else if (lifetime) type = 'keyword';
      else if (number) type = 'number';
      else if (ident) {
        if (ident.endsWith('!') || after.startsWith('(')) type = 'function';
        else if (RUST_KEYWORDS.has(ident)) type = 'keyword';
        else if (before.endsWith('<') || before.endsWith('</')) type = 'tag_name';
        else if (after.startsWith('=') && !after.startsWith('==')) type = 'attr_name';
        else if (/^[A-Z]/.test(ident)) type = 'type';
        else type = 'foreground';
      } else if (/^\s+$/.test(text)) {
        column += text.length;
        continue;
      }
      const color = colors[type] ?? THEME.foreground;
      const last = runs[runs.length - 1];
      if (last && last.color === color && last.column + last.text.length === column) last.text += text;
      else runs.push({ text, color, column });
      column += text.length;
    }
    return runs;
  });
}

export function CodeBlock({ source, lang, size = 24, lineHeight = size * 1.5, reveal = 1, marks = [], markColor }: {
  source: string;
  lang: 'tsx' | 'rust';
  size?: number;
  lineHeight?: number;
  /** 0–1: how much of the block is uncovered, top to bottom. */
  reveal?: number;
  /** One-based lines to underlay with a band. */
  marks?: number[];
  markColor?: string;
}) {
  const style = { fontFamily: FONT.mono, fontSize: size, lineHeight };
  const advance = useTextMetrics('M', style).width;
  const lines = source.split('\n');
  const width = Math.max(...lines.map((l) => l.length)) * advance;
  const height = lines.length * lineHeight;
  const rust = useMemo(() => (lang === 'rust' ? rustLines(source) : []), [lang, source]);
  return (
    <Group clip={{ x: -24, y: 0, width: width + 48, height: Math.max(0, height * reveal) }}>
      {marks.map((line) => (
        <Rect key={line} x={-16} y={(line - 1) * lineHeight} width={width + 32} height={lineHeight}
          cornerRadius={6} fill={`${markColor ?? '#FFFFFF'}22`} />
      ))}
      {lang === 'tsx'
        ? <Code language="tsx" style={style} theme={THEME}>{source}</Code>
        : rust.map((runs, i) => runs.map((run, j) => (
          <Text key={`${i}-${j}`} x={run.column * advance} y={i * lineHeight + lineHeight * 0.72} anchorY="baseline"
            style={{ fontFamily: FONT.mono, fontSize: size, fill: { type: 'solid', color: run.color } }}>
            {run.text}
          </Text>
        )))}
    </Group>
  );
}

export const codeWidth = (source: string, size: number) =>
  Math.max(...source.split('\n').map((l) => l.length)) * size * 0.6;
