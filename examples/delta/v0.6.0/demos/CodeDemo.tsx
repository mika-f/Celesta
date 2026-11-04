import { Group, Rect, Text, useTextMetrics } from '@celesta.studio/react';
import { highlightCode } from '@celesta.studio/code';
import { codeTheme } from '../../components/codeTheme';
import { STAGE, enter } from '../../components/ChapterShell';
import type { DemoProps } from '../../components/ChapterShell';
import { C, FONT, solid } from '../../components/theme';

// highlightCode() hands back <Code>'s lines and colour runs; the staging is
// ours: each line slides in on its own, the newest one gets an accent bar,
// and its runs are listed underneath. Every diff panel in this film is
// drawn the same way.
const SOURCE = `import { highlightCode } from '@celesta.studio/code';

const lines = highlightCode(source, {
  language: 'tsx',
});

lines.map((line, i) => (
  <Text key={i} y={i * 46}
    style={{ colorRuns: line.runs }}>
    {line.text}
  </Text>
));`;
const theme = codeTheme;
const LINES = highlightCode(SOURCE, { language: 'tsx', theme });
const STYLE = { fontFamily: FONT.mono, fontSize: 22, lineHeight: 36 } as const;
const STEP = 7;
/** A line's runs that draw something (the source is ASCII, so columns are indices). */
const tokens = (i: number) => LINES[i].runs.filter((run) => LINES[i].text.slice(run.start, run.end).trim());

export function CodeDemo({ frame }: DemoProps) {
  const { ascent } = useTextMetrics('M', STYLE);
  // The inspector follows the newest line with at least three colours on it.
  const shown = LINES.filter((_, i) => frame >= 4 + i * STEP);
  const newest = [...shown.keys()].reverse().find((i) => new Set(tokens(i).map((run) => run.color)).size >= 3) ?? 0;
  const mono = { fontFamily: FONT.mono, fontSize: 18 };
  const runs = tokens(newest).slice(0, 6);
  return (
    <Group>
      <Rect width={STAGE.width} height={STAGE.height} fill={C.ink} />
      <Group x={30} y={28}>
        {LINES.map((line, i) => {
          const p = enter(frame, 4 + i * STEP, 16);
          if (p <= 0) return null;
          const y = i * STYLE.lineHeight;
          return (
            <Group key={i} y={y} opacity={p}>
              {i === newest ? <Rect x={-30} y={2} width={STAGE.width} height={STYLE.lineHeight - 4} fill="#F26A2E1F" /> : null}
              {i === newest ? <Rect x={-30} y={2} width={5} height={STYLE.lineHeight - 4} fill={C.orange} /> : null}
              <Text x={30} y={ascent + 5} anchorX={1} anchorY="baseline" style={{ ...STYLE, fill: solid(C.dim) }}>
                {String(i + 1)}
              </Text>
              {line.text ? (
                <Text x={60 + 50 * (1 - p)} y={ascent + 5} anchorY="baseline"
                  style={{ ...STYLE, fill: solid(theme.foreground), colorRuns: line.runs }}>{line.text}</Text>
              ) : null}
            </Group>
          );
        })}
      </Group>
      <Rect x={30} y={500} width={STAGE.width - 60} height={2} fill={C.line} />
      <Text x={30} y={540} anchorY={0.5} style={{ ...mono, fill: solid(C.dim) }}>
        {`lines[${newest}].runs`}
      </Text>
      {runs.map((run, i) => (
        <Group key={`${newest}-${i}`} x={30 + (i % 3) * 270} y={580 + Math.floor(i / 3) * 62} opacity={enter(frame, 4 + newest * STEP + i * 2, 10)}>
          <Rect width={250} height={48} cornerRadius={8} fill={C.raised} stroke={C.line} strokeWidth={2} />
          <Rect x={14} y={16} width={16} height={16} cornerRadius={4} fill={run.color} />
          <Text x={42} y={24} anchorY={0.5} style={{ ...mono, fill: solid(run.color) }}>{run.type}</Text>
          <Text x={236} y={24} anchorX={1} anchorY={0.5} style={{ ...mono, fill: solid(C.dim) }}>{`${run.start}–${run.end}`}</Text>
        </Group>
      ))}
    </Group>
  );
}
