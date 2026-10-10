// 3 · Path で描く
// 方眼紙の上に、ずんだ餅とえだまめが一筆書きで描かれていく。線を引き終えた
// 部分から色が塗られ、ペン先の丸が線の先端を追いかける。

import { Easings, Group, Line, Path, Polyline, Rect, pointOnPolyline, progress, spring } from '@celesta/react';

import { ellipse, blob, pod } from '../lib/shapes.ts';
import type { Point } from '../lib/shapes.ts';
import { line, useFilmFrame } from '../timing.ts';
import { CANVAS, COLOR, FPS, alpha } from '../theme.ts';
import { CodePanel } from '../ui/CodePanel.tsx';
import type { SceneDefinition } from './types.ts';

const { width: W, height: H } = CANVAS;

/** 「ずんだ餅を描いてほしいのだ」の少しあとから描き始める */
const DRAW_AT = line('p1').at + 26;

/** 1 本の線。`at` と `duration` は DRAW_AT からのフレーム。`fill` があれば、引き終えたあと塗る。 */
type Stroke = { points: Point[]; at: number; duration: number; color: string; width: number; fill?: string };

const MOCHI = [
  { x: 960, y: 500, seed: 1 },
  { x: 818, y: 556, seed: 2 },
  { x: 1102, y: 556, seed: 3 },
];
const INK = '#6B5B4B';
const LEAF_INK = '#3E6B1A';

const DRAWING: Stroke[] = [
  // お皿
  { points: ellipse(960, 612, 390, 104), at: 0, duration: 22, color: INK, width: 6, fill: '#FFFFFF' },
  { points: ellipse(960, 600, 300, 70), at: 14, duration: 16, color: '#D8CBBB', width: 4 },
  // 奥から順に、お餅とずんだあん
  ...MOCHI.flatMap((m, i): Stroke[] => [
    { points: ellipse(m.x, m.y, 118, 74), at: 32 + i * 30, duration: 14, color: INK, width: 5, fill: '#FFFDF6' },
    { points: blob(m.x, m.y - 28, 112, 44, m.seed), at: 44 + i * 30, duration: 16, color: LEAF_INK, width: 5, fill: '#8CC63F' },
  ]),
  // 両脇のえだまめ
  { points: pod(560, 668, 250, -16), at: 128, duration: 20, color: LEAF_INK, width: 5, fill: '#7CB342' },
  { points: pod(1366, 660, 230, 196), at: 142, duration: 20, color: LEAF_INK, width: 5, fill: '#7CB342' },
];

/** あんの上の豆粒。あんを塗り終えたころに、ぽんぽんと乗る。 */
const BEANS = MOCHI.flatMap((m, i) => [-56, -18, 22, 60].map((dx, j) => ({
  x: m.x + dx,
  y: m.y - 30 + (j % 2 ? -12 : 8),
  radius: 11 + ((i + j) % 2) * 2,
  at: 60 + i * 30 + j * 3,
})));

const strokeProgress = (t: number, stroke: Stroke) =>
  progress(t, stroke.at, stroke.duration, Easings.easeInOutSine);

function Stage() {
  const frame = useFilmFrame('path');
  const t = frame - DRAW_AT;
  const drawing = DRAWING.find((s) => t >= s.at && t < s.at + s.duration);
  const yum = progress(frame, line('p3').at, 16, Easings.easeOutCubic);
  return (
    <>
      <Rect width={W} height={H} fill={COLOR.paper} />
      {Array.from({ length: 24 }, (_, i) => <Rect key={`v${i}`} x={i * 84} width={2} height={H} fill="#E9DEC6" />)}
      {Array.from({ length: 14 }, (_, i) => <Rect key={`h${i}`} y={i * 84} width={W} height={2} fill="#E9DEC6" />)}
      <Group y={-10}>
        {DRAWING.map((stroke, i) => {
          const drawn = strokeProgress(t, stroke);
          if (drawn <= 0) return null;
          const filled = stroke.fill ? progress(t, stroke.at + stroke.duration - 4, 10) : 0;
          return (
            <Group key={i}>
              {stroke.fill && filled > 0 && <Path points={stroke.points} closed fill={alpha(stroke.fill, filled)} />}
              <Polyline points={stroke.points} progress={drawn} stroke={stroke.color} strokeWidth={stroke.width} />
            </Group>
          );
        })}
        {BEANS.map((bean, i) => t >= bean.at && (
          <Rect key={i} x={bean.x} y={bean.y} width={bean.radius * 2} height={bean.radius * 2} cornerRadius={bean.radius}
            anchorX={0.5} anchorY={0.5} scale={spring({ frame: t - bean.at, fps: FPS, config: { damping: 9 } })}
            fill="#C5E384" stroke="#5C8A2B" strokeWidth={3} />
        ))}
        {drawing && <Pen at={pointOnPolyline(drawing.points, strokeProgress(t, drawing))} />}
        {/* 「おいしそうなのだ……」で、キラキラの線が広がる */}
        {yum > 0 && Array.from({ length: 10 }, (_, i) => {
          const angle = (-150 + i * 13) * (Math.PI / 180);
          const inner = 300;
          const outer = 300 + 70 * yum;
          return (
            <Line key={i} x1={960 + inner * Math.cos(angle)} y1={520 + inner * Math.sin(angle) * 0.8}
              x2={960 + outer * Math.cos(angle)} y2={520 + outer * Math.sin(angle) * 0.8}
              stroke="#F2A93B" strokeWidth={6} opacity={yum} />
          );
        })}
      </Group>
    </>
  );
}

/** 線の先端のペン先。 */
function Pen({ at: [x, y] }: { at: [number, number] }) {
  return <Rect x={x} y={y} width={22} height={22} cornerRadius={11} anchorX={0.5} anchorY={0.5}
    fill={COLOR.zunda} glow={{ color: '#B8FF5AAA', blur: 14 }} />;
}

function Overlay() {
  const frame = useFilmFrame('path');
  return (
    <CodePanel frame={frame} steps={[{ at: line('p2').at - 4, lines: [
      '<Path commands={[',
      "  { type: 'moveTo', x: 570, y: 612 },",
      "  { type: 'cubicTo', x1: 570, y1: 540,",
      '    x2: 760, y2: 508, x: 960, y: 508 },',
      ']} stroke="#6B5B4B" strokeWidth={6} />',
      '<Polyline points={mochi} progress={t} />',
    ] }]} />
  );
}

export const pathScene: SceneDefinition = { id: 'path', title: 'Path で描く', wipeIn: true, Stage, Overlay };
