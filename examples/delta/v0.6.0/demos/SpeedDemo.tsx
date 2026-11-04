import { Easings, Group, Rect, Text, interpolate } from '@celesta.studio/react';
import { STAGE, enter } from '../../components/ChapterShell';
import type { DemoProps } from '../../components/ChapterShell';
import { C, FONT, solid } from '../../components/theme';

// Before and after, as each pull request measured it on one machine
// (Apple M4 unless noted). Each pair has its own scale: the workloads differ.
const GROUPS = [
  { pr: 259, title: '100k particles  ·  ms per frame', before: ['100k <Rect>', 180], after: ['<Instances>', 10], decimals: 0, approx: true },
  { pr: 272, title: '100k still layers  ·  React → Rust, ms per frame', before: ['React.memo', 84.6], after: ['<Static>', 6.6], decimals: 1, approx: false },
  { pr: 180, title: 'NEBULA 1080p  ·  GPU ms per frame', before: ['before', 49.42], after: ['after', 6.83], decimals: 2, approx: false },
] as const;
const TILES = [
  { pr: 266, value: '−24%', label: '100k Rect, GPU draw' },
  { pr: 189, value: '−28%', label: 'NEBULA frame (Windows)' },
];
const BAR = { x: 170, width: 500 };

export function SpeedDemo({ frame }: DemoProps) {
  const mono = { fontFamily: FONT.mono, fontSize: 18 };
  return (
    <Group>
      <Rect width={STAGE.width} height={STAGE.height} fill={C.ink} />
      {GROUPS.map((g, i) => {
        const start = 2 + i * 12;
        const p = enter(frame, start, 14);
        const grow = interpolate(frame, [start + 4, start + 34], [0, 1], { easing: Easings.easeOutCubic, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });
        const max = g.before[1];
        const fmt = (v: number) => `${g.approx ? '≈' : ''}${v.toFixed(g.decimals)} ms`;
        return (
          <Group key={g.pr} x={30} y={26 + i * 150} opacity={p}>
            <Text anchorY={0.5} y={14} style={{ ...mono, fill: solid(C.grey) }}>{g.title}</Text>
            <Text x={800} anchorX={1} anchorY={0.5} y={14} style={{ ...mono, fill: solid(C.dim) }}>{`#${g.pr}`}</Text>
            {[g.before, g.after].map(([label, value], j) => (
              <Group key={label} y={46 + j * 44}>
                <Text anchorY={0.5} y={15} style={{ ...mono, fill: solid(j ? C.paper : C.dim) }}>{label}</Text>
                <Rect x={BAR.x} width={Math.max(4, BAR.width * (value / max) * grow)} height={30} cornerRadius={4}
                  fill={j ? C.orange : C.line} />
                <Text x={BAR.x + 14 + Math.max(4, BAR.width * (value / max) * grow)} y={15} anchorY={0.5}
                  style={{ ...mono, fontWeight: j ? 700 : 400, fill: solid(j ? C.orange : C.grey) }}>
                  {fmt(value * grow)}
                </Text>
              </Group>
            ))}
          </Group>
        );
      })}
      {TILES.map((t, i) => (
        <Group key={t.pr} x={30 + i * 410} y={490} opacity={enter(frame, 46 + i * 6)}>
          <Rect width={390} height={150} cornerRadius={12} fill={C.raised} stroke={C.line} strokeWidth={2} />
          <Text x={24} y={22} style={{ fontFamily: FONT.display, fontSize: 84, fill: solid(C.paper) }}>{t.value}</Text>
          <Text x={24} y={124} anchorY={0.5} style={{ ...mono, fontSize: 16, fill: solid(C.grey) }}>{`#${t.pr}  ·  ${t.label}`}</Text>
        </Group>
      ))}
      <Text x={30} y={STAGE.height - 44} anchorY={0.5} opacity={enter(frame, 56)} style={{ ...mono, fontSize: 16, fill: solid(C.dim) }}>
        Before and after on the same machine, from each PR's benchmark.
      </Text>
    </Group>
  );
}
