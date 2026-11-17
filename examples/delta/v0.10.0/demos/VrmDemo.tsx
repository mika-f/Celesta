import { Group, Rect, Text } from '@celesta.studio/react';
import { Circle, Ellipse, Path } from '@celesta.studio/shapes';
import { C } from '../../components/theme';
import type { DemoProps } from '../program';
import { mono, rise } from './common';

// VRM: the plugin's props on a drawn bust (no VRM model ships with the
// repository). The head nods on a track, the eyes follow the lookAt, the
// mouth reads the vowels, and the hair swings as spring bones do in the
// plugin: stepped on a fixed 60 Hz clock from time 0, so any frame, asked
// for in any order, comes out the same.

const VOWELS = ['a', 'i', 'u', 'e', 'o'] as const;
const MOUTH: Record<(typeof VOWELS)[number], [number, number]> = { a: [34, 30], i: [40, 10], u: [18, 16], e: [36, 18], o: [24, 26] };
const HEAD = { x: 300, y: 210, r: 96 };

/** The head's pitch (a nod from frame 40 to 76) and yaw, in degrees, at a time in seconds. */
function headAt(t: number) {
  const nod = t > 40 / 30 && t < 76 / 30 ? Math.sin(((t - 40 / 30) / (36 / 30)) * Math.PI) * 14 : 0;
  return { pitch: nod, yaw: 9 * Math.sin(t * 1.7) };
}

/** Where a point fixed to the head is, for a pitch and yaw: a small turn about the neck. */
function onHead(dx: number, dy: number, t: number): [number, number] {
  const { pitch, yaw } = headAt(t);
  const a = (yaw * Math.PI) / 180;
  const x = HEAD.x + dx * Math.cos(a) - dy * Math.sin(a);
  const y = HEAD.y + dx * Math.sin(a) + dy * Math.cos(a) + pitch * 1.4;
  return [x, y];
}

const ROOTS = [[-70, -40], [-36, -78], [36, -78], [70, -40]] as const;
const JOINTS = 6;
const LENGTH = 30;

/** Spring-bone strands: verlet chains stepped at 60 Hz from 0 to `t`. */
function strands(t: number): [number, number][][] {
  const steps = Math.floor(t * 60);
  const chains = ROOTS.map(([dx, dy]) => {
    const root = onHead(dx, dy, 0);
    const side = Math.sign(dx);
    return Array.from({ length: JOINTS }, (_, j) => [root[0] + side * j * 6, root[1] + j * LENGTH] as [number, number]);
  });
  const prev = chains.map((c) => c.map((p) => [...p] as [number, number]));
  for (let s = 1; s <= steps; s++) {
    const time = s / 60;
    chains.forEach((chain, c) => {
      chain[0] = onHead(ROOTS[c][0], ROOTS[c][1], time);
      for (let j = 1; j < JOINTS; j++) {
        const p = chain[j];
        const q = prev[c][j];
        const vx = (p[0] - q[0]) * 0.88;
        const vy = (p[1] - q[1]) * 0.88;
        prev[c][j] = [p[0], p[1]];
        chain[j] = [p[0] + vx, p[1] + vy + 0.9];
      }
      for (let j = 1; j < JOINTS; j++) {
        const a = chain[j - 1];
        const b = chain[j];
        const dx = b[0] - a[0];
        const dy = b[1] - a[1];
        const d = Math.hypot(dx, dy) || 1;
        chain[j] = [a[0] + (dx / d) * LENGTH, a[1] + (dy / d) * LENGTH];
      }
    });
  }
  return chains;
}

export function VrmDemo({ frame }: DemoProps) {
  const t = frame / 30;
  const vowel = VOWELS[Math.floor(frame / 10) % VOWELS.length];
  const [mw, mh] = MOUTH[vowel];
  const { pitch, yaw } = headAt(t);
  const look = Math.sin(t * 2.2) * 8;
  const [hx, hy] = onHead(0, 0, t);
  const [ex, ey] = onHead(0, -6, t);
  const [mx, my] = onHead(0, 44, t);
  const rows: [string, string][] = [
    ['src', '"./avatar.vrm"'],
    ['camera', '"bust"'],
    ['mouth', `"${vowel}"`],
    ['lookAt', '"camera"'],
    ['tracks.head', `pitch ${pitch.toFixed(1)}°`],
    ['springs', `step ${Math.floor(t * 60)} @ 60 Hz`],
  ];
  return (
    <Group>
      <Path commands={[
        { type: 'moveTo', x: 90, y: 563 }, { type: 'cubicTo', x1: 110, y1: 400, x2: 210, y2: 380, x: 300, y: 380 },
        { type: 'cubicTo', x1: 390, y1: 380, x2: 490, y2: 400, x: 510, y: 563 }, { type: 'close' },
      ]} fill="#2A2F3A" />
      <Rect x={276} y={290} width={48} height={100} fill="#E9C9B4" />
      {strands(t).map((chain, i) => (
        <Path key={i} points={chain} stroke={C.violet} strokeWidth={22 - i % 2 * 4} cap="round" join="round" />
      ))}
      <Circle x={hx - HEAD.r} y={hy - HEAD.r} radius={HEAD.r} fill="#F2D6C2" />
      <Path commands={[
        { type: 'moveTo', x: hx - 100, y: hy - 10 },
        { type: 'cubicTo', x1: hx - 100, y1: hy - 130, x2: hx + 100, y2: hy - 130, x: hx + 100, y: hy - 10 },
        { type: 'cubicTo', x1: hx + 60, y1: hy - 70, x2: hx - 60, y2: hy - 70, x: hx - 100, y: hy - 10 },
      ]} fill={C.violet} rotation={yaw} originX={hx} originY={hy} />
      {[-34, 34].map((dx) => (
        <Group key={dx}>
          <Ellipse x={ex + dx - 12} y={ey - 16} width={24} height={32} fill={C.paper} />
          <Circle x={ex + dx + look - 7} y={ey - 9 + pitch * 0.2} radius={7} fill={C.ink} />
        </Group>
      ))}
      <Ellipse x={mx - mw / 2} y={my - mh / 2} width={mw} height={mh} fill="#8E3B46" />
      <Group x={600} y={56}>
        <Text style={mono(22, C.paper, 700)}>{'<Vrm'}</Text>
        {rows.map(([key, value], i) => (
          <Group key={key} x={24} y={44 + i * 54} opacity={rise(frame, 6 + i * 6)}>
            <Text style={mono(20, C.grey)}>{key}</Text>
            <Text x={150} style={mono(20, i >= 2 ? C.orange : C.paper, 700)}>{value}</Text>
          </Group>
        ))}
        <Text y={380} style={mono(22, C.paper, 700)}>{'/>'}</Text>
        <Text y={430} maxWidth={380} style={mono(16, C.dim)}>MToon · expressions · VRMA clips · constraints</Text>
      </Group>
    </Group>
  );
}
