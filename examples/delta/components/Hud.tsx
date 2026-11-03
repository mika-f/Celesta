import { Group, Rect, Text, useCurrentFrame } from '@celesta.studio/react';
import { C, FONT, solid } from './theme';

// Top-right: the series name and one segment per chapter, the current one
// filling up. It sits above the chapter cuts, so it stays put across them.
export function Hud({ version, starts, length }: { version: string; starts: readonly number[]; length: number }) {
  const frame = useCurrentFrame();
  const current = starts.filter((start) => frame >= start).length - 1;
  const seg = 30;
  const gap = 6;
  const right = 1800;
  const total = starts.length * seg + (starts.length - 1) * gap;
  const mono = { fontFamily: FONT.mono, fontSize: 18 };
  return (
    <Group>
      <Text x={right} y={62} anchorX={1} anchorY={0.5} style={{ ...mono, fill: solid(C.grey) }}>
        {`CELESTA  Δ  WHAT'S NEW IN ${version}`}
      </Text>
      {starts.map((start, i) => {
        const x = right - total + i * (seg + gap);
        const fill = i < current ? 1 : i === current ? Math.min(1, (frame - start) / length) : 0;
        return (
          <Group key={start} x={x} y={84}>
            <Rect width={seg} height={4} fill={C.line} />
            {fill > 0 ? <Rect width={seg * fill} height={4} fill={i === current ? C.orange : C.grey} /> : null}
          </Group>
        );
      })}
    </Group>
  );
}
