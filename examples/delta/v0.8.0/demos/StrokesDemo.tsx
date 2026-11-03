import { Group, Rect, Text } from '@celesta.studio/react';
import type { Stroke } from '@celesta.studio/react';
import { Circle } from '@celesta.studio/shapes';
import type { DemoProps } from '../../components/ChapterShell';
import { STAGE, enter } from '../../components/ChapterShell';
import { C, FONT, solid } from '../../components/theme';
import { SUBTITLE_JA } from '../strings';
import { BAR, BEAT } from '../timeline';

// One subtitle line over a busy picture, outlined once, twice and three
// times. Each is a single <Text>: style.stroke takes a list, every width
// measured from the glyph outline, so the order does not matter. The
// widths breathe on the beat.
const mono = (size: number, color: string, weight = 400) =>
  ({ fontFamily: FONT.mono, fontSize: size, fontWeight: weight, fill: solid(color) }) as const;

function Picture({ frame }: { frame: number }) {
  // Stripes and a sun, so a plain subtitle would get lost.
  return (
    <Group>
      <Rect width={STAGE.width} height={STAGE.height} fill={C.blue} />
      {Array.from({ length: 14 }, (_, i) => (
        <Group key={i} x={-200 + i * 90 + (frame * 1.5) % 90} skewX={-24}>
          <Rect width={44} height={STAGE.height} fill="#FFFFFF" opacity={0.28} />
        </Group>
      ))}
      <Circle x={640} y={170} radius={120} anchorX={0.5} anchorY={0.5} fill={C.amber} />
      <Rect y={STAGE.height - 220} width={STAGE.width} height={220} fill={C.paper} opacity={0.85} />
    </Group>
  );
}

const ROWS: readonly { label: string; strokes: (pulse: number) => readonly Stroke[] }[] = [
  { label: "stroke={{ width: 8 }}", strokes: (p) => [{ paint: solid(C.ink), width: 6 + 3 * p }] },
  {
    label: 'stroke={[inner, outer]}',
    strokes: (p) => [{ paint: solid(C.orange), width: 8 + 3 * p }, { paint: solid(C.paper), width: 18 + 5 * p }],
  },
  {
    label: 'stroke={[a, b, c]}',
    strokes: (p) => [
      { paint: solid(C.paper), width: 6 + 2 * p },
      { paint: solid(C.orange), width: 14 + 4 * p },
      { paint: solid(C.ink), width: 24 + 6 * p },
    ],
  },
];

export function StrokesDemo({ frame }: DemoProps) {
  const pulse = Math.max(0, Math.cos((Math.PI * 2 * Math.max(0, frame)) / (2 * BEAT)));
  return (
    <Group>
      <Picture frame={frame} />
      {ROWS.map((row, i) => {
        const p = enter(frame, i * BAR, 14);
        const y = 120 + i * 200;
        return (
          <Group key={row.label} opacity={p} y={20 * (1 - p)}>
            <Text x={STAGE.width / 2} y={y} anchorX={0.5} anchorY={0.5}
              style={{ fontFamily: FONT.sans, fontSize: 84, fontWeight: 700, fill: solid(i === 0 ? C.paper : C.ink), stroke: row.strokes(pulse) }}>
              {i === 1 ? SUBTITLE_JA : i === 0 ? 'One outline.' : 'Three, one layer.'}
            </Text>
            <Text x={STAGE.width / 2} y={y + 78} anchorX={0.5} anchorY={0.5} style={mono(19, C.ink, 700)}>{row.label}</Text>
          </Group>
        );
      })}
    </Group>
  );
}
