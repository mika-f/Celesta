import { Camera, Group, Line, Rect, progress, useBeat, useCurrentFrame } from '@celesta/react';
import { Disc } from '../components/Disc';
import { Label } from '../components/Label';
import { ACID, BONE, CYAN, GREY, H, INK, MAG, W } from '../constants';
import { proj, rot } from '../math';

const CORNERS = [[-1, -1, -1], [1, -1, -1], [1, 1, -1], [-1, 1, -1], [-1, -1, 1], [1, -1, 1], [1, 1, 1], [-1, 1, 1]];
const EDGES = [[0, 1], [1, 2], [2, 3], [3, 0], [4, 5], [5, 6], [6, 7], [7, 4], [0, 4], [1, 5], [2, 6], [3, 7]];
export function Cubes() {
  const f = useCurrentFrame();
  const { pulse } = useBeat({ bpm: 120 });
  const colors = [ACID, CYAN, MAG, BONE, ACID, CYAN];
  return <>
    <Rect width={W} height={H} fill={INK} />
    <Camera shake={pulse * 10} seed="cubes" zoom={1 + pulse * 0.02}>
      {colors.map((color, c) => {
        const size = 80 + c * 46;
        const ax = f * 0.012 * (1 + c * 0.2) + c * 0.5, ay = f * 0.017 * (c % 2 ? -1 : 1) + c * 0.8;
        const pts = CORNERS.map((p) => proj(rot(p.map((v) => v * size), ax, ay), W / 2, H / 2, 2200));
        const on = progress(f, c * 10, 20);
        return <Group key={c} opacity={on}>
          {EDGES.map(([a, b], k) => <Line key={k} x1={pts[a][0]} y1={pts[a][1]} x2={pts[b][0]} y2={pts[b][1]}
            stroke={color} strokeWidth={c === 0 ? 5 : 2.5} />)}
          {pts.map(([x, y], k) => <Disc key={k} x={x} y={y} r={c === 0 ? 8 : 5} color={color} />)}
        </Group>;
      })}
    </Camera>
    <Label x={72} y={64} size={28} mono color={GREY}>{'05 — VOLUME'}</Label>
    <Label x={W - 72} y={H - 100} size={26} mono color={GREY} anchorX={1}>{'6 CUBES · 48 EDGES · PERSPECTIVE IN 6 LINES OF MATH'}</Label>
  </>;
}
