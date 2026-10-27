import { Group, Rect, Text, useTheme } from '@celesta/react';
import { emphasisContent, graphemes, resolveTimedRanges, timedRangeAt } from '@celesta/text';
import type { TextEmphasis } from '@celesta/text';
import { C, FONT, solid } from '../../components/theme';
import type { DemoProps } from '../program';
import { STAGE } from '../program';
import { BAR, CHAPTER_LENGTH } from '../timeline';
import { mono } from './common';

// EMPHASIS: emphasis as data, not as <Span>s. Every two beats another range
// of grapheme clusters is marked on the same string, and emphasisContent()
// turns them into spans inside one <Text>. Below, three items as timed
// ranges, and timedRangeAt() finding the one on this frame.

export const EMPHASIS_TEXT = 'HPが1です。回復しましょう。';

const STYLES = {
  danger: { color: C.red, fontWeight: 700 },
  key: { color: C.orange, fontWeight: 700 },
} as const;

const STEPS: readonly (readonly TextEmphasis[])[] = [
  [],
  [{ start: 3, end: 4, style: 'danger' }],
  [{ start: 3, end: 4, style: 'danger' }, { start: 7, end: 9, style: 'key' }],
  [{ start: 0, end: 2, style: 'key' }, { start: 3, end: 4, style: 'danger' }, { start: 7, end: 9, style: 'key' }],
];

const ITEMS = [{ from: 0, durationInFrames: 40 }, { from: 52 }, { from: 100 }];
const RANGES = resolveTimedRanges(ITEMS, { end: CHAPTER_LENGTH, what: 'pages' });

export function EmphasisDemo({ frame }: DemoProps) {
  const theme = useTheme();
  const step = Math.min(STEPS.length - 1, Math.max(0, Math.floor(frame / BAR)));
  const emphasis = STEPS[step];
  const clusters = graphemes(EMPHASIS_TEXT);
  const cell = 56;
  const now = timedRangeAt(RANGES, Math.max(0, frame));
  const nowIndex = now ? RANGES.indexOf(now) : -1;
  const lane = STAGE.width - 120;
  const code = emphasis.length === 0 ? 'emphasis: []'
    : `emphasis: [${emphasis.map((e) => `{ start: ${e.start}, end: ${e.end}, style: '${e.style}' }`).join(', ')}]`;
  return (
    <Group>
      <Text x={60} y={60} style={{ fontFamily: FONT.sans, fontSize: 66, fontWeight: 500, fill: solid(C.paper) }}>
        {emphasisContent(EMPHASIS_TEXT, emphasis, { styles: STYLES, theme })}
      </Text>
      <Group x={60} y={176}>
        {clusters.map((ch, i) => {
          const on = emphasis.find((e) => i >= e.start && i < e.end);
          const color = on ? (on.style === 'danger' ? C.red : C.orange) : C.grey;
          return (
            <Group key={i} x={i * cell}>
              <Rect width={cell - 6} height={56} cornerRadius={3} fill={on ? '#FFFFFF10' : 'panel'} stroke={on ? color : 'line'} strokeWidth={2} />
              <Text x={(cell - 6) / 2} y={28} anchorX={0.5} anchorY={0.5} style={mono(26, on ? color : C.paper, 700)}>{ch}</Text>
              <Text x={(cell - 6) / 2} y={74} anchorX={0.5} style={mono(15, C.dim)}>{String(i)}</Text>
            </Group>
          );
        })}
        {emphasis.map((e, k) => (
          <Rect key={k} x={e.start * cell} y={-12} width={(e.end - e.start) * cell - 6} height={5}
            fill={e.style === 'danger' ? 'red' : 'orange'} />
        ))}
      </Group>
      <Text x={60} y={300} maxWidth={STAGE.width - 120} style={mono(19, C.paper)}>{code}</Text>

      <Group x={60} y={410}>
        <Text y={-14} anchorY={1} style={mono(16, C.dim)}>{'resolveTimedRanges(items, { end: 144 })'}</Text>
        {RANGES.map((r, i) => (
          <Group key={i} x={(r.from / CHAPTER_LENGTH) * lane}>
            <Rect width={((r.end - r.from) / CHAPTER_LENGTH) * lane - 4} height={44} cornerRadius={3}
              fill={i === nowIndex ? 'orange' : 'raised'} stroke={i === nowIndex ? 'orange' : 'line'} strokeWidth={2} />
            <Text x={12} y={22} anchorY={0.5} style={mono(17, i === nowIndex ? C.ink : C.grey, 700)}>
              {`[${r.from}, ${r.end})`}
            </Text>
          </Group>
        ))}
        <Rect x={(Math.max(0, frame) / CHAPTER_LENGTH) * lane - 1} y={-6} width={3} height={56} fill="paper" />
        <Text y={78} style={mono(19, C.paper)}>
          {`timedRangeAt(ranges, ${Math.max(0, frame)}) → ${now ? `[${now.from}, ${now.end})` : 'null'}`}
        </Text>
      </Group>
    </Group>
  );
}
