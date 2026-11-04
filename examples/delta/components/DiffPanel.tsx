import { Group, Rect, Text, useTextMetrics } from '@celesta.studio/react';
import { highlightCode } from '@celesta.studio/code';
import { Panel } from '@celesta.studio/components';
import type { CodeLanguage } from '@celesta.studio/code';
import { codeTheme } from './codeTheme';
import { C, FONT, solid } from './theme';

// A "+ only" diff of the file a feature is used from, typed in. The lines
// come from highlightCode(), so they carry exactly <Code>'s colours while the
// panel stages them itself: a green band per line, a gutter, a block caret.
const CODE = { fontFamily: FONT.mono, fontSize: 23, lineHeight: 46 } as const;
const ADVANCE = 0.6 * CODE.fontSize; // JetBrains Mono is 600/1000 em wide
const HEAD = 46;
const PAD = 18;
const theme = codeTheme;

export function DiffPanel({ x, y, width, file, code, language = 'tsx', typed }: {
  x: number; y: number; width: number; file: string; code: string;
  language?: CodeLanguage;
  /** Source code points typed so far. */
  typed: number;
}) {
  const lines = highlightCode(code, { language, theme });
  const { ascent } = useTextMetrics('M', CODE);
  const height = HEAD + lines.length * CODE.lineHeight + 10;
  const label = { fontFamily: FONT.mono, fontSize: 19 };
  // The caret sits on the line being typed.
  const current = lines.findIndex((line) => typed < line.start + line.columns.length);
  return (
    <Panel x={x} y={y} width={width} height={height} fill={C.panel} stroke={C.line} strokeWidth={2} cornerRadius={10} clip>
      <Rect y={HEAD - 2} width={width} height={2} fill={C.line} />
      <Text x={PAD} y={HEAD / 2} anchorY={0.5} style={{ ...label, fill: solid(C.grey) }}>{file}</Text>
      <Text x={width - PAD} y={HEAD / 2} anchorX={1} anchorY={0.5} style={{ ...label, fill: solid(C.red) }}>-0</Text>
      <Text x={width - PAD - 52} y={HEAD / 2} anchorX={1} anchorY={0.5}
        style={{ ...label, fill: solid(C.green) }}>{`+${lines.length}`}</Text>
      {lines.map((line, i) => {
        const shown = Math.max(0, Math.min(typed - line.start, line.columns.length - 1));
        if (typed < line.start) return null;
        const top = HEAD + 5 + i * CODE.lineHeight;
        const visible = line.columns[shown];
        return (
          <Group key={i} y={top}>
            <Rect x={2} width={width - 4} height={CODE.lineHeight - 4} y={2} fill="#3DDC8414" />
            <Rect x={2} width={4} height={CODE.lineHeight - 4} y={2} fill={C.green} />
            <Text x={PAD} y={ascent + 6} anchorY="baseline" style={{ ...CODE, fill: solid(C.green) }}>+</Text>
            {line.text ? (
              <Text x={PAD + 34} y={ascent + 6} anchorY="baseline" style={{
                ...CODE, fill: solid(theme.foreground), colorRuns: line.runs, visibleCharacters: visible,
              }}>{line.text}</Text>
            ) : null}
            {i === current || (current < 0 && i === lines.length - 1) ? (
              <Rect x={PAD + 34 + visible * ADVANCE + 2} y={9} width={ADVANCE} height={CODE.lineHeight - 18} fill={C.orange} />
            ) : null}
          </Group>
        );
      })}
    </Panel>
  );
}
