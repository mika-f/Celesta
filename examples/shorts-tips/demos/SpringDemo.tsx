import { Easings, Group, Rect, Text, interpolate, progress, spring } from '@celesta/react';
import { Circle, Polyline } from '@celesta/shapes';
import { useAccent, useBarLoop, useStage } from '../components/DemoClock';
import { BAR, C, DEMO, FONT, FPS, W } from '../constants';

// interpolate vs spring: the same 0 → 1 move, once with interpolate and once
// with spring, on the downbeat of every bar (forth, then back). Under the
// balls, both curves over one bar, with a dot riding each.
// Stages (variants/spring.json): 0 `const a`, 1 `const b`, 2 and 3 the two <Ball>s.
export const SPRING_STAGES = 4;

// Keep these equal to the snippet in variants/spring.json.
const TRAVEL = 560;
const LINEAR_FRAMES = 20;
const DAMPING = 8;

const START_X = 160;
const RADIUS = 40;
const BLUE = '#7FD1FF';

type Ease = (t: number) => number;
const linear: Ease = (t) => interpolate(t, [0, LINEAR_FRAMES], [0, 1], { extrapolateRight: 'clamp' });
const springy: Ease = (t) => spring({ frame: t, fps: FPS, config: { damping: DAMPING } });

const mono = (size: number, fill: string) =>
  ({ fontFamily: FONT.mono, fontSize: size, fontWeight: 700, fill: { type: 'solid', color: fill } }) as const;

function Lane({ y, name, label, color, ease, enterStage, playStage }: {
  y: number;
  name: string;
  label: string;
  color: string;
  ease: Ease;
  enterStage: number;
  playStage: number;
}) {
  const enter = progress(useStage(enterStage), 0, 12, Easings.easeOutCubic);
  const loop = useBarLoop(playStage);
  // Even bars go forth, odd bars come back, so the ball never jumps.
  const at = (t: number) => {
    if (!loop) return 0;
    const v = ease(Math.max(0, t));
    return loop.bar % 2 === 0 ? v : 1 - v;
  };
  const value = at(loop?.t ?? 0);
  const x = (v: number) => START_X + v * TRAVEL;
  return (
    <Group y={y + 24 * (1 - enter)} opacity={enter}>
      <Text x={DEMO.safe.left} y={0} style={mono(40, color)}>{label}</Text>
      <Text x={DEMO.safe.right} y={0} anchorX={1} style={mono(40, C.paper)}>{`${name} = ${value.toFixed(2)}`}</Text>
      <Rect x={START_X} y={137} width={TRAVEL} height={6} cornerRadius={3} fill={C.dim} />
      {/* The target: a spring passes it before settling. */}
      <Rect x={START_X + TRAVEL - 3} y={100} width={6} height={80} cornerRadius={3} fill={C.grey} />
      {loop && [8, 6, 4, 2].map((back, i) => (
        <Circle key={back} x={x(at(loop.t - back))} y={140} anchorX={0.5} anchorY={0.5} radius={RADIUS}
          fill={color} opacity={0.08 + i * 0.06} />
      ))}
      <Circle x={x(value)} y={140} anchorX={0.5} anchorY={0.5} radius={RADIUS} fill={color} />
    </Group>
  );
}

// Value over one bar, edge to edge: 0 at the bottom line, 1 at the dashed one.
const GRAPH = { y: 900, unit: 250 } as const;
const curve = (ease: Ease) =>
  Array.from({ length: BAR + 1 }, (_, t) => [(t / BAR) * W, GRAPH.y - ease(t) * GRAPH.unit] as [number, number]);

function Graph({ color, ease, stage }: { color: string; ease: Ease; stage: number }) {
  const draw = progress(useStage(stage), 0, 20, Easings.easeOutCubic);
  const loop = useBarLoop(stage);
  if (draw <= 0) return null;
  const t = loop?.t ?? 0;
  return (
    <>
      <Polyline points={curve(ease)} progress={draw} stroke={color} strokeWidth={6} opacity={0.85} />
      {loop && <Circle x={(t / BAR) * W} y={GRAPH.y - ease(t) * GRAPH.unit} anchorX={0.5} anchorY={0.5} radius={14}
        fill={color} glow={{ color, blur: 16 }} />}
    </>
  );
}

export function SpringDemo() {
  const accent = useAccent();
  const axes = progress(useStage(2), 0, 12);
  return (
    <>
      <Lane y={56} name="a" label="interpolate" color={BLUE} ease={linear} enterStage={0} playStage={2} />
      <Lane y={300} name="b" label="spring" color={accent} ease={springy} enterStage={1} playStage={3} />
      <Group opacity={axes}>
        <Rect y={GRAPH.y} width={W} height={2} fill={C.dim} />
        <Rect y={GRAPH.y - GRAPH.unit} width={W} height={2} fill={C.grey} opacity={0.5} />
      </Group>
      <Graph color={BLUE} ease={linear} stage={2} />
      <Graph color={accent} ease={springy} stage={3} />
    </>
  );
}
