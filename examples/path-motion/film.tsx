import * as React from 'react';
import { Assets, Composition, Easings, Font, Rect, Text, progress, useCurrentFrame } from '@celesta.studio/react';
import {
  Line,
  Path,
  getLength,
  getPointAtLength,
  getTangentAtLength,
  interpolatePath,
  parseSvgPath,
  pointOnPath,
  tangentOnPath,
} from '@celesta.studio/shapes';
import type { PathCommand, TextStyle } from '@celesta.studio/react';

// Motion along a path, and morphing between paths. Left: a route written as
// SVG path data (cubic, smooth cubic, arc, and quadratic segments) draws
// itself on with `progress`, while an arrowhead rides its tip, turned to the
// tangent; small arrowheads mark the tangent every 80 px. Right: a circle, a star, and a
// heart, each with different commands, morph into one another. Six seconds
// at 30 fps, 1920 × 1080.

const W = 1920, H = 1080;
const INK = '#F2F0EA', MUTED = '#7D8297', ACCENT = '#FFC94D', ROUTE_COLOR = '#4DD6FF', BG = '#12141B';
const SANS = 'Inter', MONO = 'JetBrains Mono';
const text = (fontFamily: string, fontSize: number, color: string, extra: TextStyle = {}): TextStyle => ({
  fontFamily, fontSize, fill: { type: 'solid', color }, ...extra,
});

// Each segment leaves in the direction the last one arrived, so the route has no corners.
const ROUTE = parseSvgPath(
  'M140 820 C 240 560 420 1000 520 700 S 560 420 560 330 A 110 110 0 0 1 780 330 Q 780 640 880 800',
);
const ROUTE_LENGTH = getLength(ROUTE);
const TICK_SPACING = 80;

/** An arrowhead pointing along +x, about the origin. */
const arrowhead = (size: number): [number, number][] =>
  [[size, 0], [-0.7 * size, -0.65 * size], [-0.3 * size, 0], [-0.7 * size, 0.65 * size]];

function Route() {
  const frame = useCurrentFrame();
  const t = progress(frame, 15, 150, Easings.easeInOutSine);
  const tip = pointOnPath(ROUTE, t);
  const heading = tangentOnPath(ROUTE, t);
  const drawn = t * ROUTE_LENGTH;
  const ticks = Array.from({ length: Math.floor(ROUTE_LENGTH / TICK_SPACING) + 1 }, (_, i) => i * TICK_SPACING)
    .filter((s) => s <= drawn);
  return <>
    <Path commands={ROUTE} stroke="#2A3042" strokeWidth={6} cap="round" />
    <Path commands={ROUTE} stroke={ROUTE_COLOR} strokeWidth={6} cap="round" progress={t} />
    {ticks.map((s) => {
      // Beside the route, on its left, pointing the way it goes.
      const at = getPointAtLength(ROUTE, s);
      const along = getTangentAtLength(ROUTE, s);
      return <Path key={s} x={at.x + along.y * 26} y={at.y - along.x * 26} rotation={along.angle}
        points={arrowhead(9)} closed fill={ACCENT} />;
    })}
    <Path x={tip.x} y={tip.y} rotation={heading.angle} points={arrowhead(22)} closed fill={INK} />
    <Text x={140} y={150} anchorY="baseline" style={text(SANS, 44, INK, { fontWeight: 700 })}>
      Along a path
    </Text>
    <Text x={140} y={200} anchorY="baseline" style={text(MONO, 24, MUTED)}>
      {`${drawn.toFixed(0).padStart(4)} / ${ROUTE_LENGTH.toFixed(0)} px   ${heading.angle.toFixed(0).padStart(4)}°`}
    </Text>
  </>;
}

const R = 250;

const CIRCLE = parseSvgPath(`M${R} 0A${R} ${R} 0 1 1 ${-R} 0A${R} ${R} 0 1 1 ${R} 0Z`);

const STAR: PathCommand[] = [
  ...Array.from({ length: 10 }, (_, i): PathCommand => {
    const radius = i % 2 === 0 ? R : R * 0.45;
    const angle = (i / 10) * Math.PI * 2 - Math.PI / 2;
    const x = Math.cos(angle) * radius;
    const y = Math.sin(angle) * radius;
    return i === 0 ? { type: 'moveTo', x, y } : { type: 'lineTo', x, y };
  }),
  { type: 'close' },
];

const HEART = parseSvgPath(
  'M0 230 C-40 190 -250 80 -250 -60 C-250 -170 -160 -240 -85 -240 C-40 -240 -10 -210 0 -170 ' +
    'C10 -210 40 -240 85 -240 C160 -240 250 -170 250 -60 C250 80 40 190 0 230 Z',
);

const SHAPES = [
  { name: 'circle', commands: CIRCLE },
  { name: 'star', commands: STAR },
  { name: 'heart', commands: HEART },
];

const LETTER: Record<PathCommand['type'], string> = { moveTo: 'M', lineTo: 'L', quadTo: 'Q', cubicTo: 'C', close: 'Z' };

/** Each shape's commands as SVG letters with counts, like `M L×9 Z`, for the caption. */
const describe = (commands: readonly PathCommand[]) => {
  const runs: [string, number][] = [];
  for (const { type } of commands) {
    const last = runs[runs.length - 1];
    if (last && last[0] === LETTER[type]) {
      last[1] += 1;
    } else {
      runs.push([LETTER[type], 1]);
    }
  }
  return runs.map(([letter, n]) => (n > 1 ? `${letter}×${n}` : letter)).join(' ');
};

function Morph() {
  const frame = useCurrentFrame();
  // Hold each shape for 30 frames, then morph to the next for 30.
  const step = Math.min(SHAPES.length - 1, Math.floor(frame / 60));
  const from = SHAPES[step];
  const to = SHAPES[(step + 1) % SHAPES.length];
  const t = progress(frame, step * 60 + 30, 30, Easings.easeInOutCubic);
  const commands = interpolatePath(from.commands, to.commands, t);
  return <>
    <Path x={1440} y={560} commands={commands}
      fill={{
        type: 'linear', start: { x: -R, y: -R }, end: { x: R, y: R },
        stops: [{ offset: 0, color: '#FF5C8A' }, { offset: 1, color: '#7B5CFF' }],
      }}
      stroke={INK} strokeWidth={4} join="round" />
    <Text x={1060} y={150} anchorY="baseline" style={text(SANS, 44, INK, { fontWeight: 700 })}>
      Between paths
    </Text>
    <Text x={1060} y={200} anchorY="baseline" style={text(MONO, 24, MUTED)}>
      {`${from.name} -> ${to.name}   t = ${t.toFixed(2)}`}
    </Text>
    <Text x={1060} y={920} anchorY="baseline" style={text(MONO, 22, MUTED)}>
      {`${from.name}: ${describe(from.commands)}`}
    </Text>
    <Text x={1060} y={956} anchorY="baseline" style={text(MONO, 22, MUTED)}>
      {`${to.name}: ${describe(to.commands)}`}
    </Text>
  </>;
}

export default function Film() {
  return (
    <Composition width={W} height={H} fps={30} durationInFrames={180}>
      <Assets>
        <Font google="Inter" weights={[500, 700]} />
        <Font google="JetBrains Mono" weights={[400]} />
      </Assets>
      <Rect width={W} height={H} fill={BG} />
      <Line x1={960} y1={120} x2={960} y2={960} stroke="#232837" strokeWidth={2} />
      <Route />
      <Morph />
    </Composition>
  );
}
