import { Easings, Group, Rect, Text, interpolate, progress } from '@celesta.studio/react';
import { Arrow, Circle, Heart, Pie, Polygon, Star, Triangle } from '@celesta.studio/shapes';
import type { DemoProps } from '../../components/ChapterShell';
import { STAGE, enter } from '../../components/ChapterShell';
import { C, FONT, solid } from '../../components/theme';
import { BAR, BEAT } from '../timeline';

// Top: the five new closed shapes, their corners rounding and squaring on
// the beat, the pie filling, the polygon and star gaining sides.
// Bottom: dashes and dots flowing along strokes with dashOffset.
const R = 72;
const SLOT = STAGE.width / 5;

function Slot({ i, label, frame, children }: { i: number; label: string; frame: number; children: React.ReactNode }) {
  const p = enter(frame, i * 4, 14);
  return (
    <Group x={SLOT * i + SLOT / 2} y={150} opacity={p} scale={0.6 + 0.4 * p}>
      {children}
      <Text y={R + 54} anchorX={0.5} anchorY={0.5} style={{ fontFamily: FONT.mono, fontSize: 18, fill: solid(C.grey) }}>{label}</Text>
    </Group>
  );
}

export function ShapesDemo({ frame }: DemoProps) {
  const beat = Math.floor(Math.max(0, frame) / BEAT);
  // Corners round on one beat and square on the next, eased.
  const round = 22 * Math.abs(Math.sin((Math.PI / 2) * (frame / BEAT)));
  const sides = 3 + (Math.floor(beat / 2) % 6);
  const points = 5 + (Math.floor(beat / 4) % 4);
  const end = interpolate(frame, [0, 2 * BAR], [0, 360], { easing: Easings.easeInOutCubic, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });
  const flow = -frame * 3;
  const dashIn = progress(frame, BAR, 16, Easings.easeOutCubic);
  return (
    <Group>
      <Slot i={0} label={`apex ${(0.5 + 0.4 * Math.sin(frame / 12)).toFixed(2)}`} frame={frame}>
        <Triangle width={2 * R} height={2 * R * 0.87} anchorX={0.5} anchorY={0.5} apex={0.5 + 0.4 * Math.sin(frame / 12)}
          cornerRadius={round} fill={C.orange} />
      </Slot>
      <Slot i={1} label={`sides={${sides}}`} frame={frame}>
        <Polygon radius={R} sides={sides} anchorX={0.5} anchorY={0.5} cornerRadius={round} fill={C.blue} />
      </Slot>
      <Slot i={2} label={`points={${points}}`} frame={frame}>
        <Star radius={R + 6} points={points} innerRatio={0.48} anchorX={0.5} anchorY={0.5} rotation={frame * 1.5}
          cornerRadius={round / 3} fill={C.amber} />
      </Slot>
      <Slot i={3} label={`endAngle ${Math.round(end)}°`} frame={frame}>
        <Circle radius={R} anchorX={0.5} anchorY={0.5} fill={C.raised} />
        <Pie radius={R} endAngle={end} anchorX={0.5} anchorY={0.5} cornerRadius={round / 2} fill={C.green} />
      </Slot>
      <Slot i={4} label="<Heart>" frame={frame}>
        <Heart width={2 * R} height={2 * R * 0.9} anchorX={0.5} anchorY={0.5} scale={1 + 0.08 * Math.max(0, Math.cos((Math.PI * 2 * frame) / BEAT))}
          fill={C.red} />
      </Slot>
      <Group y={340} opacity={dashIn}>
        <Rect x={0} width={STAGE.width} height={2} fill={C.line} />
        <Text y={36} style={{ fontFamily: FONT.mono, fontSize: 20, fill: solid(C.grey) }}>{`dash={[24, 12]}  dashOffset={${Math.round(flow)}}`}</Text>
        <Rect x={10} y={80} width={250} height={220} cornerRadius={24} stroke={C.orange} strokeWidth={6}
          dash={[24, 12]} dashOffset={flow} />
        <Circle x={430} y={190} anchorX={0.5} anchorY={0.5} radius={110} stroke={C.paper} strokeWidth={10}
          dash={[0, 22]} cap="round" dashOffset={flow * 0.5} />
        <Star x={710} y={190} anchorX={0.5} anchorY={0.5} radius={120} points={6} innerRatio={0.55}
          stroke={C.blue} strokeWidth={6} dash={[40, 14, 6, 14]} dashOffset={flow} cornerRadius={6} />
        <Arrow x1={20} y1={360} x2={STAGE.width - 30} y2={360} strokeWidth={8} stroke={C.green}
          dash={[30, 16]} dashOffset={flow} />
        <Text x={430} y={190} anchorX={0.5} anchorY={0.5} style={{ fontFamily: FONT.mono, fontSize: 18, fill: solid(C.grey) }}>dash={'{[0, 22]}'}</Text>
      </Group>
    </Group>
  );
}
