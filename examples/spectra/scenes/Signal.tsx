// Chapter 5: a dashboard. Gradient bars, a line chart that draws on, a
// donut of path arcs, an orbiting particle swarm, and a closing logo that
// focuses out of a blur.
import { Circle, Easings, Group, Path, Polyline, Rect, Text, progress, spring, useCountUp, useCurrentFrame } from '@celesta/react';

import { DIM, DISPLAY, FPS, INK, MONO, PALETTE, arc, rand, style } from '../shared';

function Bars({ t, frame }: { t: number; frame: number }) {
  const grow = progress(frame, 0, 40, Easings.easeOutCubic);
  return (
    <Group x={80} y={120}>
      <Text style={style(MONO, 20, DIM, { letterSpacing: 4 })}>THROUGHPUT / CHANNEL</Text>
      {Array.from({ length: 48 }, (_, k) => {
        const h = grow * (30 + 300 * Math.abs(Math.sin(t * 1.7 + k * 0.31) * Math.cos(t * 0.9 + k * 0.07)));
        return (
          <Rect key={k} x={k * 17} y={380 - h} width={12} height={h} cornerRadius={3} fill={{
            type: 'linear', start: { x: 0, y: h }, end: { x: 0, y: 0 },
            stops: [{ offset: 0, color: '#3DA5FF' }, { offset: 1, color: '#FF3D7F' }],
          }} />
        );
      })}
    </Group>
  );
}

function Chart({ t, frame }: { t: number; frame: number }) {
  const series = [0, 1, 2].map((s) =>
    Array.from({ length: 120 }, (_, i) => [i * 7, 150 - 110 * (0.5 + 0.5 * Math.sin(i * 0.11 + s * 1.7 + t * (1 + s * 0.4)))] as [number, number]),
  );
  return (
    <Group x={80} y={600}>
      <Text style={style(MONO, 20, DIM, { letterSpacing: 4 })}>LATENCY (ms)</Text>
      <Rect y={40} width={840} height={180} cornerRadius={10} fill="#FFFFFF08" stroke="#FFFFFF18" strokeWidth={1} />
      {series.map((points, s) => (
        <Polyline key={s} y={40} points={points} progress={progress(frame, s * 8, 70, Easings.easeOutCubic)}
          stroke={PALETTE[s * 2]} strokeWidth={3} join="round" cap="round" />
      ))}
    </Group>
  );
}

function Donut({ frame }: { frame: number }) {
  const shares = [0.34, 0.22, 0.18, 0.14, 0.12];
  const total = progress(frame, 10, 60, Easings.easeOutCubic);
  let at = -Math.PI / 2;
  return (
    <Group x={1240} y={330} rotation={frame * 0.3}>
      {shares.map((share, i) => {
        const span = share * total * Math.PI * 2;
        const commands = arc(110, 190 + (i === 0 ? 14 : 0), at + 0.02, at + Math.max(0.03, span) - 0.02);
        at += span;
        return <Path key={i} commands={commands} fill={PALETTE[i]} shadow={i === 0 ? { color: '#00000080', blur: 12, offsetX: 0, offsetY: 6 } : undefined} />;
      })}
    </Group>
  );
}

function Swarm({ t }: { t: number }) {
  return (
    <Group x={1240} y={330}>
      {Array.from({ length: 900 }, (_, i) => {
        const r = 220 + rand(i, 1) * 360;
        const a = rand(i, 2) * Math.PI * 2 + t * (0.2 + rand(i, 3) * 0.6) * (i % 2 ? 1 : -1);
        return (
          <Rect key={i} x={Math.cos(a) * r} y={Math.sin(a) * r * 0.55} anchorX={0.5} anchorY={0.5}
            width={2 + rand(i, 4) * 4} height={2 + rand(i, 4) * 4} rotation={t * 90 + i}
            fill={PALETTE[i % 6]} opacity={0.25 + 0.5 * (0.5 + 0.5 * Math.sin(t * 4 + i))} />
        );
      })}
    </Group>
  );
}

function Metrics({ frame }: { frame: number }) {
  const fps = useCountUp(60, { delay: 20, durationInFrames: 50 });
  const layers = useCountUp(2480, { delay: 26, durationInFrames: 50 });
  const effects = useCountUp(31, { delay: 32, durationInFrames: 50 });
  return (
    <Group x={1060} y={720}>
      {[
        ['FPS', fps.toString()],
        ['LAYERS', layers.toLocaleString('en-US')],
        ['EFFECTS', effects.toString()],
      ].map(([label, value], i) => (
        <Group key={label} x={i * 270} opacity={progress(frame, 20 + i * 6, 20)}>
          <Rect width={250} height={150} cornerRadius={16} fill="#11163A" stroke={PALETTE[i + 1]} strokeWidth={2} />
          <Text x={20} y={40} anchorY="baseline" style={style(MONO, 18, DIM, { letterSpacing: 3 })}>{label}</Text>
          <Text x={20} y={124} anchorY="baseline" style={style(DISPLAY, 84, INK)}>{value}</Text>
        </Group>
      ))}
    </Group>
  );
}

function Logo({ frame }: { frame: number }) {
  const settle = spring({ frame: frame - 80, fps: FPS, config: { damping: 14 } });
  if (settle <= 0) return null;
  return (
    <Group x={960} y={540} scale={1.4 - 0.4 * settle} opacity={Math.min(1, settle)} blur={Math.max(0, 24 * (1 - settle))}>
      <Rect x={-1000} y={-560} width={2000} height={1120} fill="#05060FE0" />
      <Circle anchorX={0.5} anchorY={0.5} radius={120} stroke={{
        type: 'linear', start: { x: 0, y: 0 }, end: { x: 240, y: 240 },
        stops: [{ offset: 0, color: '#3DA5FF' }, { offset: 1, color: '#FF3D7F' }],
      }} strokeWidth={10} glow={{ color: '#7B5CFF', blur: 30 }} />
      <Text y={10} anchorX={0.5} anchorY={0.5} style={style(DISPLAY, 150, '#FFFFFF', { letterSpacing: 24 })}>CELESTA</Text>
      <Text y={190} anchorX={0.5} anchorY={0.5} style={style(MONO, 24, DIM, { letterSpacing: 8 })}>CODE IS THE CUT</Text>
    </Group>
  );
}

export function Signal() {
  const frame = useCurrentFrame();
  const t = frame / FPS;
  return (
    <Group>
      <Swarm t={t} />
      <Bars t={t} frame={frame} />
      <Chart t={t} frame={frame} />
      <Donut frame={frame} />
      <Metrics frame={frame} />
      <Logo frame={frame} />
    </Group>
  );
}
