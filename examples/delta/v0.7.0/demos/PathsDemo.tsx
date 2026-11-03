import { Easings, Group, Rect, Text, interpolate, progress } from '@celesta.studio/react';
import { Path, getLength, interpolatePath, parseSvgPath, pointOnPath, tangentOnPath, toSvgPath } from '@celesta.studio/shapes';
import type { DemoProps } from '../../components/ChapterShell';
import { STAGE, enter } from '../../components/ChapterShell';
import { C, FONT, solid } from '../../components/theme';
import { outlines } from '../outlines';
import { BAR, BEAT } from '../timeline';

// Top: SVG path data, parsed, drawn on with progress, a marker riding its
// tip along the tangent. Bottom: interpolatePath() between outlines, one
// morph a beat, and the path data it makes, written back with toSvgPath().
const ROUTE = parseSvgPath('M40 300 C 160 40 300 40 360 190 S 520 360 600 200 A 90 90 0 1 1 780 160 L 820 60');
const LENGTH = getLength(ROUTE);
const SHAPE = outlines(250);
const CYCLE = [SHAPE.delta, SHAPE.star, SHAPE.heart, SHAPE.hexagon];

export function PathsDemo({ frame }: DemoProps) {
  const t = progress(frame, 0, 3 * BAR, Easings.easeInOutSine);
  const tip = pointOnPath(ROUTE, t);
  const tangent = tangentOnPath(ROUTE, t);
  const step = Math.max(0, Math.floor((frame - 6) / BEAT));
  const from = CYCLE[step % CYCLE.length];
  const to = CYCLE[(step + 1) % CYCLE.length];
  const m = progress(frame - 6 - step * BEAT, 0, 7, Easings.easeInOutCubic);
  const shape = interpolatePath(from, to, m);
  const svg = toSvgPath(shape);
  const mono = { fontFamily: FONT.mono, fontSize: 19 } as const;
  return (
    <Group>
      <Group opacity={enter(frame, 0)}>
        <Path commands={ROUTE} stroke={C.line} strokeWidth={4} dash={[2, 12]} cap="round" />
        <Path commands={ROUTE} stroke={C.orange} strokeWidth={6} progress={t} cap="round" join="round"
          glow={{ color: '#F26A2E88', blur: 16 }} />
        <Path x={tip.x} y={tip.y} rotation={tangent.angle} points={[[18, 0], [-12, -12], [-6, 0], [-12, 12]]} closed fill={C.paper} />
        <Text x={0} y={380} style={{ ...mono, fill: solid(C.grey) }}>
          {`getLength() ${LENGTH.toFixed(0)} px   ·   progress ${t.toFixed(2)}   ·   tangent ${tangent.angle.toFixed(0).padStart(4)}°`}
        </Text>
      </Group>
      <Group y={430} opacity={enter(frame, 6)}>
        <Rect width={STAGE.width} height={2} fill={C.line} />
        <Path x={150} y={165} commands={shape} fill={interpolate(m, [0, 1], [0, 1]) > 0.5 ? C.violet : C.blue}
          stroke={C.paper} strokeWidth={3} join="round" />
        <Text x={330} y={44} style={{ ...mono, fill: solid(C.orange), fontWeight: 700 }}>
          {`interpolatePath(${['delta', 'star', 'heart', 'hexagon'][step % 4]}, ${['star', 'heart', 'hexagon', 'delta'][step % 4]}, ${m.toFixed(2)})`}
        </Text>
        <Text x={330} y={84} maxWidth={520} style={{ ...mono, fontSize: 16, lineHeight: 26, fill: solid(C.grey) }}>
          {`toSvgPath() → ${svg.slice(0, 260)}…`}
        </Text>
      </Group>
    </Group>
  );
}
