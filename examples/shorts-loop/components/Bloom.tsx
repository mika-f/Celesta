import { Group } from '@celesta/react';
import { Circle, Ellipse, Path } from '@celesta/shapes';
import { AMBER, CX, CY, CYAN, MAGENTA, RADIUS, VIOLET, WHITE } from '../constants';
import { TAU, theta, wave } from '../loop';
import { kaleido } from '../shaders';
import { Gem } from './Gem';

// The kaleidoscope. Plain Celesta shapes (a core, ellipses, petals, orbiting
// gems) turn around the centre; `kaleido` mirrors one slice of them into a
// sixteen- or twenty-four-fold flower. Every turn and wobble is a whole
// number of cycles per loop.

const deg = (radians: number) => (radians * 180) / Math.PI;

const ELLIPSES = [
  { width: 300, height: 170, color: CYAN, stroke: 5, cycles: 1 },
  { width: 520, height: 300, color: MAGENTA, stroke: 9, cycles: -1 },
  { width: 700, height: 460, color: AMBER, stroke: 3, cycles: 2 },
  { width: 800, height: 620, color: CYAN, stroke: 6, cycles: -2 },
];

const ORBITS = [
  { count: 5, radius: 150, cycles: 1, size: 30, color: CYAN },
  { count: 7, radius: 255, cycles: -2, size: 24, color: AMBER },
  { count: 9, radius: 350, cycles: 3, size: 18, color: MAGENTA },
];

const PETAL = [
  { type: 'moveTo' as const, x: 0, y: 0 },
  { type: 'cubicTo' as const, x1: 70, y1: -54, x2: 260, y2: -70, x: 400, y: 0 },
  { type: 'cubicTo' as const, x1: 260, y1: 70, x2: 70, y2: 54, x: 0, y: 0 },
  { type: 'close' as const },
];

export function Bloom({ frame, pulse }: { frame: number; pulse: number }) {
  const t = theta(frame);
  // Two foldings, eight and twelve wedges, trade places twice per loop.
  const morph = 0.5 - 0.5 * Math.cos(2 * t);
  const blend = morph * morph * (3 - 2 * morph);
  return <Group blendMode="screen" shader={kaleido({
    center: [CX, CY],
    sectors: [8, 12],
    blend,
    spin: t,
    slice: -2 * t,
    twist: 0.4 * wave(frame, 1),
    split: 1.5 + 5 * pulse,
  })}>
    {/* Holds the group's box steady, so the filter always covers the whole disc. */}
    <Circle x={CX} y={CY} radius={RADIUS} anchorX={0.5} anchorY={0.5} stroke={`${WHITE}40`} strokeWidth={2} />
    {[0, 1, 2, 3, 4].map((i) => (
      <Path key={`petal-${i}`} x={CX} y={CY} commands={PETAL} opacity={0.5}
        rotation={72 * i - deg(t)} scale={0.85 + 0.12 * wave(frame, 2, i)}
        fill={{ type: 'linear', start: { x: 0, y: 0 }, end: { x: 400, y: 0 },
          stops: [{ offset: 0, color: `${VIOLET}00` }, { offset: 0.5, color: MAGENTA }, { offset: 1, color: `${CYAN}00` }] }} />
    ))}
    {ELLIPSES.map((e, i) => (
      <Ellipse key={`ellipse-${i}`} x={CX} y={CY} width={e.width} height={e.height} anchorX={0.5} anchorY={0.5}
        rotation={deg(e.cycles * t) + 45 * i} scale={1 + 0.05 * wave(frame, 3, i)}
        stroke={e.color} strokeWidth={e.stroke * (1 + 0.6 * pulse)} />
    ))}
    {ORBITS.flatMap((o, j) => Array.from({ length: o.count }, (_, i) => {
      const a = (TAU * i) / o.count + o.cycles * t;
      const r = o.radius + 26 * wave(frame, 2, i + j);
      return <Gem key={`gem-${j}-${i}`} x={CX + r * Math.cos(a)} y={CY + r * Math.sin(a)}
        radius={o.size * (1 + 0.5 * pulse)} color={o.color} />;
    }))}
    <Gem x={CX} y={CY} radius={150 * (1 + 0.15 * pulse)} color={MAGENTA} opacity={0.9} />
  </Group>;
}
