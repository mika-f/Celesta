// Chapter 2: six clipped tiles, one per blend mode, full of rotating
// gradient shapes; stars and polygons as paths; a sine wave that draws itself
// on; arrows; and a line grid.
import { Easings, Group, Rect, Text, progress, useCurrentFrame } from '@celesta/react';
import { Arrow, Circle, Line, Path, Polyline, pointOnPolyline } from '@celesta/shapes';
import { Grid } from '@celesta/layout';
import type { BlendMode } from '@celesta/react';

import { DIM, FPS, INK, MONO, PALETTE, polygon, style } from '../shared';

const MODES: BlendMode[] = ['normal', 'multiply', 'screen', 'overlay', 'add', 'difference'];

function Backdrop({ t }: { t: number }) {
  return (
    <Group opacity={0.35}>
      {/* One line left of the screen too, so none is missing as they scroll. */}
      {Array.from({ length: 34 }, (_, i) => (
        <Line key={`v${i}`} x1={(i - 1) * 60 + ((t * 30) % 60)} y1={0} x2={(i - 1) * 60 + ((t * 30) % 60)} y2={1080} stroke="#2A3360" strokeWidth={1} />
      ))}
      {Array.from({ length: 19 }, (_, i) => (
        <Line key={`h${i}`} x1={0} y1={i * 60} x2={1920} y2={i * 60} stroke="#2A3360" strokeWidth={1} />
      ))}
    </Group>
  );
}

function Tile({ mode, index, t }: { mode: BlendMode; index: number; t: number }) {
  return (
    <Group blendMode={mode} opacity={0.92} rotation={6 * Math.sin(t + index)} x={210} y={150} anchorX={0.5} anchorY={0.5}
      clip={{ x: -200, y: -140, width: 400, height: 280, cornerRadius: 28 }}>
      <Rect x={-200} y={-140} width={400} height={280} fill={{
        type: 'linear', start: { x: 0, y: 0 }, end: { x: 400, y: 280 },
        stops: [{ offset: 0, color: '#1B2150' }, { offset: 1, color: PALETTE[index] }],
      }} />
      {Array.from({ length: 10 }, (_, k) => {
        const size = 70 + 20 * Math.sin(t * 2 + k);
        return (
          <Rect key={k} x={-150 + (k % 5) * 75} y={-60 + Math.floor(k / 5) * 120} anchorX={0.5} anchorY={0.5}
            width={size} height={size} rotation={t * 50 + k * 18} cornerRadius={12 + 10 * Math.sin(t + k)}
            fill={k % 2 === 0 ? {
              type: 'radial', center: { x: size / 2, y: size / 2 }, radius: size / 2,
              stops: [{ offset: 0, color: '#FFFFFF' }, { offset: 0.6, color: PALETTE[(index + k) % 6] }, { offset: 1, color: '#00000000' }],
            } : PALETTE[(index + k + 3) % 6]}
            stroke="#FFFFFF90" strokeWidth={3} opacity={0.8} />
        );
      })}
      <Text x={-180} y={110} anchorY="baseline" style={style(MONO, 20, INK, { letterSpacing: 3 })}>{mode.toUpperCase()}</Text>
    </Group>
  );
}

function Stars({ t }: { t: number }) {
  return Array.from({ length: 14 }, (_, i) => {
    const points = 5 + (i % 4);
    return (
      <Path key={i} x={120 + i * 130} y={960 + 30 * Math.sin(t * 3 + i)} rotation={t * (i % 2 ? 60 : -60)}
        points={polygon(points, 46, i % 2 ? undefined : 20)} closed
        fill={{ type: 'linear', start: { x: -46, y: -46 }, end: { x: 46, y: 46 },
          stops: [{ offset: 0, color: PALETTE[i % 6] }, { offset: 1, color: PALETTE[(i + 2) % 6] }] }}
        stroke="#FFFFFF" strokeWidth={2} join="round" />
    );
  });
}

function Wave({ frame, t }: { frame: number; t: number }) {
  const points = Array.from({ length: 240 }, (_, i) => {
    const x = i * 8;
    return [x, 70 * Math.sin(i * 0.08 + t * 3) * Math.sin(i * 0.013 + t)] as [number, number];
  });
  const drawn = progress(frame, 10, 90, Easings.easeInOutCubic);
  const [tipX, tipY] = pointOnPolyline(points, drawn);
  return (
    <Group x={0} y={820}>
      <Polyline points={points} progress={drawn} stroke={{
        type: 'linear', start: { x: 0, y: 0 }, end: { x: 1920, y: 0 },
        stops: [{ offset: 0, color: '#3DA5FF' }, { offset: 0.5, color: '#7B5CFF' }, { offset: 1, color: '#FF3D7F' }],
      }} strokeWidth={5} cap="round" join="round" glow={{ color: '#7B5CFF', blur: 10 }} />
      <Circle x={tipX} y={tipY} anchorX={0.5} anchorY={0.5} radius={10} fill="#FFFFFF" glow={{ color: '#FFFFFF', blur: 12 }} />
    </Group>
  );
}

export function Geometry() {
  const frame = useCurrentFrame();
  const t = frame / FPS;
  return (
    <Group>
      <Backdrop t={t} />
      <Grid x={150} y={80} columns={3} columnWidth={420} rowHeight={300} columnGap={120} rowGap={40}>
        {MODES.map((mode, index) => (
          <Tile key={mode} mode={mode} index={index} t={t + index * 0.3} />
        ))}
      </Grid>
      <Wave frame={frame} t={t} />
      <Stars t={t} />
      {Array.from({ length: 8 }, (_, i) => {
        const a = t * 1.5 + (i / 8) * Math.PI * 2;
        return (
          <Arrow key={i} x={960} y={710} x1={Math.cos(a) * 40} y1={Math.sin(a) * 18} x2={Math.cos(a) * 140} y2={Math.sin(a) * 60}
            stroke={PALETTE[i % 6]} strokeWidth={4} heads={i % 3 === 0 ? 'both' : 'end'} />
        );
      })}
      <Text x={1880} y={1050} anchorX={1} anchorY="baseline" style={style(MONO, 18, DIM)}>
        {`blend × ${MODES.length} · clip r=28 · paths ${14 + 8 + 1}`}
      </Text>
    </Group>
  );
}
